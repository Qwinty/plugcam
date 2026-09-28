//! H.264 → NV12 with the decoder built into Windows (Media Foundation `CMSH264DecoderMFT`),
//! in low-latency mode so every access unit comes out as soon as it goes in.
//!
//! The decoder runs on the GPU (DXVA through Direct3D 11) when it can: decoding 1080p on the
//! CPU costs about 25 ms of processor time per frame, most of what streaming costs at all.
//! Decoded pictures are copied back into system memory for the frame converter. Without a
//! usable GPU it decodes on the CPU, and for a while after the GPU decoder failed.
//!
//! COM objects here are bound to the thread that created the decoder.

use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows::core::Interface;

use crate::frame::{ColorInfo, Nv12Frame};

/// When a GPU decoder last failed. Decoders opened within `GPU_RETRY` of that stay on the CPU,
/// so a broken driver costs one reconnect, and a passing one (a GPU reset) is not for good.
static GPU_FAILED_AT: Mutex<Option<Instant>> = Mutex::new(None);
const GPU_RETRY: Duration = Duration::from_secs(10 * 60);

fn gpu_failed_recently() -> bool {
    GPU_FAILED_AT.lock().unwrap().is_some_and(|t| t.elapsed() < GPU_RETRY)
}

fn gpu_failed() {
    *GPU_FAILED_AT.lock().unwrap() = Some(Instant::now());
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("Media Foundation: {0}")]
    Windows(#[from] windows::core::Error),
    #[error("decoder produced no NV12 output type")]
    NoNv12,
    #[error("decoded buffer is {got} bytes, expected at least {want}")]
    ShortBuffer { got: u32, want: usize },
}

/// Output geometry negotiated with the decoder.
#[derive(Debug, Clone, Copy)]
struct OutputFormat {
    /// Coded size (height may be padded to a multiple of 16, e.g. 1088).
    coded_height: u32,
    stride: u32,
    /// Visible picture (display aperture).
    width: u32,
    height: u32,
    color: ColorInfo,
    buffer_size: u32,
    /// The decoder hands out its own samples (always so on the GPU).
    provides_samples: bool,
}

pub struct H264Decoder {
    // Field order matters: COM objects must be released before `_runtime` shuts MF down.
    mft: IMFTransform,
    gpu: Option<Gpu>,
    output: Option<OutputFormat>,
    out_sample: Option<IMFSample>,
    _runtime: MfRuntime,
}

impl H264Decoder {
    /// `width`/`height` come from the scrcpy session packet; the decoder confirms or corrects them.
    /// Uses the GPU unless it failed recently or `PLUGCAM_SOFTWARE_DECODE` is set.
    pub fn new(width: u32, height: u32) -> Result<Self, DecodeError> {
        let software = gpu_failed_recently() || std::env::var_os("PLUGCAM_SOFTWARE_DECODE").is_some();
        if !software {
            match Self::open(width, height, true) {
                // Without a usable device `open` already fell back to the CPU.
                Ok(d) => return Ok(d),
                Err(e) => {
                    log::warn!("GPU decoder: {e}; decoding on the CPU");
                    gpu_failed();
                }
            }
        }
        Self::open(width, height, false)
    }

    /// True when pictures are decoded on the GPU.
    pub fn is_hardware(&self) -> bool {
        self.gpu.is_some()
    }

    fn open(width: u32, height: u32, use_gpu: bool) -> Result<Self, DecodeError> {
        let runtime = MfRuntime::start()?;
        unsafe {
            let mft: IMFTransform = CoCreateInstance(&CMSH264DecoderMFT, None, CLSCTX_INPROC_SERVER)?;
            if let Ok(attrs) = mft.GetAttributes() {
                // Without this the decoder holds frames back for B-frame reordering.
                attrs.SetUINT32(&CODECAPI_AVLowLatencyMode, 1)?;
            }
            // The device manager has to be set before the media types.
            let gpu = if use_gpu {
                match Gpu::attach(&mft) {
                    Ok(gpu) => Some(gpu),
                    Err(e) => {
                        log::info!("no GPU decoding: {e}");
                        None
                    }
                }
            } else {
                None
            };

            let input = MFCreateMediaType()?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            input.SetUINT64(&MF_MT_FRAME_SIZE, (width as u64) << 32 | height as u64)?;
            input.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            mft.SetInputType(0, &input, 0)?;

            let mut decoder = Self { mft, gpu, output: None, out_sample: None, _runtime: runtime };
            decoder.negotiate_output()?;
            decoder.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            decoder.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
            log::info!("decoding on the {}", if decoder.gpu.is_some() { "GPU" } else { "CPU" });
            Ok(decoder)
        }
    }

    /// Feeds one Annex B access unit (a frame, or SPS/PPS) and calls `on_frame` for every
    /// picture that comes out. With low-latency mode that is normally the frame just fed.
    pub fn decode(&mut self, data: &[u8], pts_us: u64, on_frame: &mut dyn FnMut(&Nv12Frame)) -> Result<(), DecodeError> {
        let result = self.feed(data, pts_us, on_frame);
        if result.is_err() && self.gpu.is_some() {
            log::warn!("GPU decoder failed; the next sessions decode on the CPU for a while");
            gpu_failed();
        }
        result
    }

    fn feed(&mut self, data: &[u8], pts_us: u64, on_frame: &mut dyn FnMut(&Nv12Frame)) -> Result<(), DecodeError> {
        let sample = input_sample(data, pts_us)?;
        loop {
            match unsafe { self.mft.ProcessInput(0, &sample, 0) } {
                Ok(()) => break,
                // Its queue is full: take pictures out, then retry.
                Err(e) if e.code() == MF_E_NOTACCEPTING => self.drain(on_frame)?,
                Err(e) => return Err(e.into()),
            }
        }
        self.drain(on_frame)
    }

    fn drain(&mut self, on_frame: &mut dyn FnMut(&Nv12Frame)) -> Result<(), DecodeError> {
        loop {
            let fmt = self.output.ok_or(DecodeError::NoNv12)?;
            let out_sample = if fmt.provides_samples {
                None
            } else {
                Some(match &self.out_sample {
                    Some(s) => {
                        // A reused buffer must look empty, or the decoder fails with
                        // "CopyDecodedFrame failed" on the second picture.
                        unsafe { s.GetBufferByIndex(0)?.SetCurrentLength(0)? };
                        s.clone()
                    }
                    None => {
                        let s = unsafe { MFCreateSample()? };
                        unsafe { s.AddBuffer(&MFCreateMemoryBuffer(fmt.buffer_size)?)? };
                        self.out_sample = Some(s.clone());
                        s
                    }
                })
            };
            let mut buffers = [MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: 0,
                pSample: ManuallyDrop::new(out_sample),
                dwStatus: 0,
                pEvents: ManuallyDrop::new(None),
            }];
            let mut status = 0u32;
            let result = unsafe { self.mft.ProcessOutput(0, &mut buffers, &mut status) };
            let sample = unsafe { ManuallyDrop::take(&mut buffers[0].pSample) };
            drop(unsafe { ManuallyDrop::take(&mut buffers[0].pEvents) });

            log::trace!("ProcessOutput: {:?} status {status} sample {}", result.as_ref().map_err(|e| e.code()), sample.is_some());
            match result {
                Ok(()) => {
                    if let Some(sample) = sample {
                        self.emit(&sample, on_frame)?;
                    }
                }
                Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(()),
                Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                    log::debug!("decoder stream change");
                    self.negotiate_output()?;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn emit(&mut self, sample: &IMFSample, on_frame: &mut dyn FnMut(&Nv12Frame)) -> Result<(), DecodeError> {
        let fmt = self.output.ok_or(DecodeError::NoNv12)?;
        let buffer = unsafe { sample.GetBufferByIndex(0)? };
        if let (Some(gpu), Ok(surface)) = (self.gpu.as_mut(), buffer.cast::<IMFDXGIBuffer>()) {
            return gpu.read_back(&surface, fmt, on_frame);
        }

        // The UV plane starts after the coded (padded) height, but converters want planes
        // that are exactly as tall as the visible picture.
        let stride = fmt.stride as usize;
        let uv_start = stride * fmt.coded_height as usize;
        let y_len = stride * fmt.height as usize;
        let uv_len = stride * (fmt.height as usize).div_ceil(2);
        let want = uv_start + uv_len;
        unsafe {
            let buffer = sample.ConvertToContiguousBuffer()?;
            let mut ptr = std::ptr::null_mut();
            let mut len = 0u32;
            buffer.Lock(&mut ptr, None, Some(&mut len))?;
            if (len as usize) < want {
                buffer.Unlock()?;
                return Err(DecodeError::ShortBuffer { got: len, want });
            }
            let data = std::slice::from_raw_parts(ptr, len as usize);
            on_frame(&Nv12Frame {
                width: fmt.width,
                height: fmt.height,
                y: &data[..y_len],
                y_stride: fmt.stride,
                uv: &data[uv_start..uv_start + uv_len],
                uv_stride: fmt.stride,
                color: fmt.color,
            });
            buffer.Unlock()?;
        }
        Ok(())
    }

    /// Picks NV12 among the decoder's output types and reads its geometry and colorimetry.
    fn negotiate_output(&mut self) -> Result<(), DecodeError> {
        self.out_sample = None;
        for i in 0.. {
            let Ok(t) = (unsafe { self.mft.GetOutputAvailableType(0, i) }) else { break };
            if unsafe { t.GetGUID(&MF_MT_SUBTYPE)? } != MFVideoFormat_NV12 {
                continue;
            }
            unsafe { self.mft.SetOutputType(0, &t, 0)? };
            let size = unsafe { t.GetUINT64(&MF_MT_FRAME_SIZE)? };
            let (coded_width, coded_height) = ((size >> 32) as u32, size as u32);
            let stride = unsafe { t.GetUINT32(&MF_MT_DEFAULT_STRIDE) }
                .map(|s| (s as i32).unsigned_abs())
                .unwrap_or(coded_width);

            let mut area = MFVideoArea::default();
            let aperture = unsafe {
                let bytes = std::slice::from_raw_parts_mut(&mut area as *mut _ as *mut u8, size_of::<MFVideoArea>());
                t.GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, bytes, None)
            };
            let (width, height) = match aperture {
                Ok(()) if area.Area.cx > 0 && area.Area.cy > 0 => (area.Area.cx as u32, area.Area.cy as u32),
                _ => (coded_width, coded_height),
            };

            let full_range = unsafe { t.GetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE) }
                .map_or(true, |r| r != MFNominalRange_16_235.0 as u32);
            let bt709 = unsafe { t.GetUINT32(&MF_MT_YUV_MATRIX) }
                .is_ok_and(|m| m == MFVideoTransferMatrix_BT709.0 as u32);
            let info = unsafe { self.mft.GetOutputStreamInfo(0)? };
            log::debug!("output stream info: flags {:#x}, size {}, alignment {}", info.dwFlags, info.cbSize, info.cbAlignment);
            let provides_samples = info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0;

            let fmt = OutputFormat {
                coded_height,
                stride,
                width,
                height,
                color: ColorInfo { full_range, bt709 },
                buffer_size: info.cbSize,
                provides_samples,
            };
            log::info!(
                "decoder output: {width}x{height} (coded {coded_width}x{coded_height}, stride {stride}, buffer {}), {:?}",
                fmt.buffer_size,
                fmt.color
            );
            self.output = Some(fmt);
            return Ok(());
        }
        Err(DecodeError::NoNv12)
    }
}

/// The Direct3D 11 device the decoder works on, and a texture to read pictures back through.
struct Gpu {
    context: ID3D11DeviceContext,
    staging: Option<(ID3D11Texture2D, D3D11_TEXTURE2D_DESC)>,
    device: ID3D11Device,
    _manager: IMFDXGIDeviceManager,
}

impl Gpu {
    fn attach(mft: &IMFTransform) -> windows::core::Result<Self> {
        unsafe {
            let aware = mft.GetAttributes()?.GetUINT32(&MF_SA_D3D11_AWARE).unwrap_or(0);
            if aware == 0 {
                return Err(windows::core::Error::new(windows::Win32::Foundation::E_NOTIMPL, "decoder is not D3D11-aware"));
            }
            let (mut device, mut context) = (None, None);
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
            let (device, context) = (device.unwrap(), context.unwrap());
            // The decoder uses the device from its own threads too.
            let _ = device.cast::<ID3D11Multithread>()?.SetMultithreadProtected(true);

            let (mut token, mut manager) = (0u32, None);
            MFCreateDXGIDeviceManager(&mut token, &mut manager)?;
            let manager = manager.unwrap();
            manager.ResetDevice(&device, token)?;
            mft.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, manager.as_raw() as usize)?;
            Ok(Self { context, staging: None, device, _manager: manager })
        }
    }

    /// Copies a decoded picture into system memory and hands it to `on_frame`.
    fn read_back(&mut self, surface: &IMFDXGIBuffer, fmt: OutputFormat, on_frame: &mut dyn FnMut(&Nv12Frame)) -> Result<(), DecodeError> {
        unsafe {
            let mut texture: Option<ID3D11Texture2D> = None;
            surface.GetResource(&ID3D11Texture2D::IID, &mut texture as *mut _ as *mut *mut c_void)?;
            let texture = texture.ok_or(DecodeError::NoNv12)?;
            let index = surface.GetSubresourceIndex()?;
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut desc);

            let reuse = self.staging.as_ref().is_some_and(|(_, d)| (d.Width, d.Height, d.Format) == (desc.Width, desc.Height, desc.Format));
            if !reuse {
                let staging_desc = D3D11_TEXTURE2D_DESC {
                    MipLevels: 1,
                    ArraySize: 1,
                    Usage: D3D11_USAGE_STAGING,
                    BindFlags: 0,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    MiscFlags: 0,
                    ..desc
                };
                let mut staging = None;
                self.device.CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
                log::debug!("GPU picture: {}x{} {:?}", desc.Width, desc.Height, desc.Format);
                self.staging = Some((staging.unwrap(), staging_desc));
            }
            let (staging, staging_desc) = self.staging.as_ref().unwrap();

            self.context.CopySubresourceRegion(staging, 0, 0, 0, 0, &texture, index, None);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            // The picture must fit the texture, or the slices below would reach past the mapping.
            let fits = fmt.width <= staging_desc.Width && fmt.height <= staging_desc.Height && mapped.RowPitch >= fmt.width;
            if !fits || mapped.pData.is_null() {
                self.context.Unmap(staging, 0);
                let texture_bytes = mapped.RowPitch * staging_desc.Height * 3 / 2;
                return Err(DecodeError::ShortBuffer { got: texture_bytes, want: fmt.width as usize * fmt.height as usize * 3 / 2 });
            }
            // NV12 in one mapping: the Y rows of the whole texture, then the UV rows.
            let pitch = mapped.RowPitch as usize;
            let uv_start = pitch * staging_desc.Height as usize;
            let y_len = pitch * fmt.height as usize;
            let uv_len = pitch * (fmt.height as usize).div_ceil(2);
            let data = std::slice::from_raw_parts(mapped.pData as *const u8, uv_start + uv_len);
            on_frame(&Nv12Frame {
                width: fmt.width,
                height: fmt.height,
                y: &data[..y_len],
                y_stride: pitch as u32,
                uv: &data[uv_start..],
                uv_stride: pitch as u32,
                color: fmt.color,
            });
            self.context.Unmap(staging, 0);
        }
        Ok(())
    }
}

fn input_sample(data: &[u8], pts_us: u64) -> windows::core::Result<IMFSample> {
    unsafe {
        let buffer = MFCreateMemoryBuffer(data.len() as u32)?;
        let mut ptr = std::ptr::null_mut();
        buffer.Lock(&mut ptr, None, None)?;
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
        buffer.Unlock()?;
        buffer.SetCurrentLength(data.len() as u32)?;
        let sample = MFCreateSample()?;
        sample.AddBuffer(&buffer)?;
        sample.SetSampleTime(pts_us as i64 * 10)?; // 100 ns units
        Ok(sample)
    }
}

/// COM + Media Foundation for the lifetime of a decoder.
struct MfRuntime {
    com: bool,
}

impl MfRuntime {
    fn start() -> windows::core::Result<Self> {
        // S_FALSE (already initialized) still needs a matching CoUninitialize;
        // RPC_E_CHANGED_MODE (thread is STA) does not, and MF works there too.
        let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        if let Err(e) = unsafe { MFStartup(MF_VERSION, MFSTARTUP_LITE) } {
            if com {
                unsafe { CoUninitialize() };
            }
            return Err(e);
        }
        Ok(Self { com })
    }
}

impl Drop for MfRuntime {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            if self.com {
                CoUninitialize();
            }
        }
    }
}
