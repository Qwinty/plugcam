//! H.264 → NV12 with the decoder built into Windows (Media Foundation `CMSH264DecoderMFT`),
//! in low-latency mode so every access unit comes out as soon as it goes in.
//!
//! COM objects here are bound to the thread that created the decoder.

use std::mem::ManuallyDrop;

use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};

use crate::frame::{ColorInfo, Nv12Frame};

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
}

pub struct H264Decoder {
    // Field order matters: COM objects must be released before `_runtime` shuts MF down.
    mft: IMFTransform,
    output: Option<OutputFormat>,
    out_sample: Option<IMFSample>,
    _runtime: MfRuntime,
}

impl H264Decoder {
    /// `width`/`height` come from the scrcpy session packet; the decoder confirms or corrects them.
    pub fn new(width: u32, height: u32) -> Result<Self, DecodeError> {
        let runtime = MfRuntime::start()?;
        unsafe {
            let mft: IMFTransform = CoCreateInstance(&CMSH264DecoderMFT, None, CLSCTX_INPROC_SERVER)?;
            if let Ok(attrs) = mft.GetAttributes() {
                // Without this the decoder holds frames back for B-frame reordering.
                attrs.SetUINT32(&CODECAPI_AVLowLatencyMode, 1)?;
            }

            let input = MFCreateMediaType()?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            input.SetUINT64(&MF_MT_FRAME_SIZE, (width as u64) << 32 | height as u64)?;
            input.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            mft.SetInputType(0, &input, 0)?;

            let mut decoder = Self { mft, output: None, out_sample: None, _runtime: runtime };
            decoder.negotiate_output()?;
            decoder.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            decoder.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
            Ok(decoder)
        }
    }

    /// Feeds one Annex B access unit (a frame, or SPS/PPS) and calls `on_frame` for every
    /// picture that comes out. With low-latency mode that is normally the frame just fed.
    pub fn decode(&mut self, data: &[u8], pts_us: u64, on_frame: &mut dyn FnMut(&Nv12Frame)) -> Result<(), DecodeError> {
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
            let out_sample = match &self.out_sample {
                Some(s) => {
                    // A reused buffer must look empty, or the decoder fails with
                    // "CopyDecodedFrame failed" on the second picture.
                    unsafe { s.GetBufferByIndex(0)?.SetCurrentLength(0)? };
                    s.clone()
                }
                None => {
                    let fmt = self.output.ok_or(DecodeError::NoNv12)?;
                    let s = unsafe { MFCreateSample()? };
                    unsafe { s.AddBuffer(&MFCreateMemoryBuffer(fmt.buffer_size)?)? };
                    self.out_sample = Some(s.clone());
                    s
                }
            };
            let mut buffers = [MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: 0,
                pSample: ManuallyDrop::new(Some(out_sample)),
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

    fn emit(&self, sample: &IMFSample, on_frame: &mut dyn FnMut(&Nv12Frame)) -> Result<(), DecodeError> {
        let fmt = self.output.ok_or(DecodeError::NoNv12)?;
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
            let buffer_size = info.cbSize;

            let fmt = OutputFormat {
                coded_height,
                stride,
                width,
                height,
                color: ColorInfo { full_range, bt709 },
                buffer_size,
            };
            log::info!(
                "decoder output: {width}x{height} (coded {coded_width}x{coded_height}, stride {stride}, buffer {buffer_size}), {:?}",
                fmt.color
            );
            self.output = Some(fmt);
            return Ok(());
        }
        Err(DecodeError::NoNv12)
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
