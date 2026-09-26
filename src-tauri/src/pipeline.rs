//! Phone → "Plugcam Camera": waits for a phone, starts the server, decodes, converts and pushes
//! frames, and starts over whenever something breaks, until stopped.

use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::adb::Adb;
use crate::decode::H264Decoder;
use crate::frame::FrameConverter;
use crate::preview::PreviewSlot;
use crate::scrcpy::protocol::{self, ControlMessage, Packet};
use crate::scrcpy::server::{self, CameraParams, Session};
use crate::vcam::VirtualCamera;

/// Some cameras are listed but never deliver a frame (camera 4 on the OnePlus 11R).
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(6);
/// A camera streams continuously; this much silence means the link is gone.
const STALL_TIMEOUT: Duration = Duration::from_secs(3);
const RETRY_DELAY: Duration = Duration::from_secs(2);
const NO_PICTURE_RETRY_DELAY: Duration = Duration::from_secs(15);
const STATS_PERIOD: Duration = Duration::from_secs(5);

pub struct PipelineConfig {
    pub adb: Adb,
    pub server_file: PathBuf,
    /// Pick this phone; otherwise the first ready one, USB preferred.
    pub serial: Option<String>,
    pub camera: CameraParams,
    pub mirror: bool,
    /// Clockwise degrees, turned on the PC so it can change without restarting the stream.
    pub rotation: u16,
    /// Record the raw video stream (packets after the codec id) for test fixtures.
    pub dump: Option<PathBuf>,
    /// Where to offer frames for the app window's preview.
    pub preview: Option<Arc<PreviewSlot>>,
    /// Told the camera zoom whenever the phone sets it.
    pub on_zoom: Option<Arc<dyn Fn(f32) + Send + Sync>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Status {
    WaitingForDevice,
    Connecting { serial: String },
    Streaming { serial: String, device: String, width: u32, height: u32 },
    /// The selected camera streams nothing; retrying will not help (and may upset the
    /// phone's camera service for a while, docs/stage2.md).
    NoPicture { serial: String },
    Error { message: String },
    Stopped,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Stats {
    pub fps: f32,
    /// Decode + convert + send, averaged over the last period.
    pub process_ms: f32,
}

struct Shared {
    stop: AtomicBool,
    mirror: AtomicBool,
    rotation: AtomicU16,
    video: Mutex<Option<TcpStream>>,
    control: Mutex<Option<TcpStream>>,
    stats: Mutex<Stats>,
}

pub struct Pipeline {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<VirtualCamera>>,
}

impl Pipeline {
    pub fn start(config: PipelineConfig, vcam: VirtualCamera, on_status: impl Fn(&Status) + Send + 'static) -> Self {
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            mirror: AtomicBool::new(config.mirror),
            rotation: AtomicU16::new(config.rotation),
            video: Mutex::new(None),
            control: Mutex::new(None),
            stats: Mutex::new(Stats::default()),
        });
        let thread_shared = shared.clone();
        let thread = std::thread::Builder::new()
            .name("pipeline".into())
            .spawn(move || run(config, vcam, &thread_shared, on_status))
            .expect("spawn pipeline thread");
        Self { shared, thread: Some(thread) }
    }

    /// Torch and zoom; fails when no phone is streaming.
    pub fn send_control(&self, msg: ControlMessage) -> io::Result<()> {
        match self.shared.control.lock().unwrap().as_mut() {
            Some(s) => s.write_all(&msg.serialize()),
            None => Err(io::Error::new(io::ErrorKind::NotConnected, "no phone is streaming")),
        }
    }

    pub fn set_mirror(&self, on: bool) {
        self.shared.mirror.store(on, Ordering::Relaxed);
    }

    pub fn set_rotation(&self, degrees: u16) {
        self.shared.rotation.store(degrees, Ordering::Relaxed);
    }

    pub fn stats(&self) -> Stats {
        *self.shared.stats.lock().unwrap()
    }

    /// Stops streaming and gives the virtual camera back, so it can be reused for the next
    /// pipeline without disappearing from the apps that have it open.
    pub fn stop(mut self) -> Option<VirtualCamera> {
        self.shutdown()
    }

    fn shutdown(&mut self) -> Option<VirtualCamera> {
        self.shared.stop.store(true, Ordering::SeqCst);
        // Unblocks the read the pipeline thread may be sitting in.
        if let Some(s) = self.shared.video.lock().unwrap().as_ref() {
            let _ = s.shutdown(Shutdown::Both);
        }
        self.thread.take().and_then(|t| t.join().ok())
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(config: PipelineConfig, vcam: VirtualCamera, shared: &Shared, on_status: impl Fn(&Status)) -> VirtualCamera {
    let mut last_status = None;
    let mut status = |s: Status| {
        if last_status.as_ref() != Some(&s) {
            log::info!("status: {s:?}");
            on_status(&s);
            last_status = Some(s);
        }
    };
    let stopped = || shared.stop.load(Ordering::SeqCst);
    let pause = |d: Duration| {
        let until = Instant::now() + d;
        while !stopped() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(50));
        }
    };

    let mut converter = FrameConverter::new(vcam.width(), vcam.height());
    vcam.send_frame(converter.output()); // black until the phone is there

    while !stopped() {
        let serial = match pick_device(&config) {
            Ok(Some(serial)) => serial,
            Ok(None) => {
                status(Status::WaitingForDevice);
                pause(Duration::from_secs(1));
                continue;
            }
            Err(message) => {
                status(Status::Error { message });
                pause(RETRY_DELAY);
                continue;
            }
        };

        status(Status::Connecting { serial: serial.clone() });
        let session = match server::start(&config.adb, &serial, &config.server_file, &config.camera) {
            Ok(s) => s,
            Err(e) => {
                status(Status::Error { message: e.to_string() });
                pause(RETRY_DELAY);
                continue;
            }
        };
        *shared.video.lock().unwrap() = session.video.try_clone().ok();
        *shared.control.lock().unwrap() = session.control.try_clone().ok();
        if let Some(on_zoom) = &config.on_zoom {
            let on_zoom = on_zoom.clone();
            session.log.on_zoom(move |z| on_zoom(z));
        }

        let result = stream(&session, &serial, &config, &vcam, &mut converter, shared, &mut status);

        *shared.video.lock().unwrap() = None;
        *shared.control.lock().unwrap() = None;
        let problems = session.log.problems();
        session.close();
        *shared.stats.lock().unwrap() = Stats::default();

        if stopped() {
            break;
        }
        vcam.send_frame(converter.dim()); // make the frozen picture look disconnected
        match result {
            Ok(()) => pause(RETRY_DELAY),
            Err(StreamEnd::NoPicture) => {
                status(Status::NoPicture { serial });
                pause(NO_PICTURE_RETRY_DELAY);
            }
            Err(StreamEnd::Broken(e)) => {
                status(Status::Error { message: format!("{e}{problems}") });
                pause(RETRY_DELAY);
            }
        }
    }
    status(Status::Stopped);
    vcam
}

enum StreamEnd {
    NoPicture,
    Broken(String),
}

impl From<String> for StreamEnd {
    fn from(e: String) -> Self {
        StreamEnd::Broken(e)
    }
}

fn pick_device(config: &PipelineConfig) -> Result<Option<String>, String> {
    let devices = config.adb.devices().map_err(|e| e.to_string())?;
    let ready = devices.iter().filter(|d| d.is_ready());
    Ok(match &config.serial {
        Some(serial) => ready.into_iter().find(|d| &d.serial == serial).map(|d| d.serial.clone()),
        None => {
            let ready: Vec<_> = ready.collect();
            ready.iter().find(|d| !d.is_wifi()).or(ready.first()).map(|d| d.serial.clone())
        }
    })
}

/// Reads the whole stream of one session. Returns `Ok` when stopped on request.
fn stream(
    session: &Session,
    serial: &str,
    config: &PipelineConfig,
    vcam: &VirtualCamera,
    converter: &mut FrameConverter,
    shared: &Shared,
    status: &mut impl FnMut(Status),
) -> Result<(), StreamEnd> {
    let socket = session.video.try_clone().map_err(|e| e.to_string())?;
    socket.set_read_timeout(Some(FIRST_FRAME_TIMEOUT)).map_err(|e| e.to_string())?;
    let dump = match &config.dump {
        Some(path) => Some(File::create(path).map_err(|e| format!("{}: {e}", path.display()))?),
        None => None,
    };
    let mut reader = BufReader::with_capacity(1 << 20, Tee { inner: &socket, copy: dump });

    let mut decoder: Option<H264Decoder> = None;
    let mut frames = 0u64;
    let mut period_start = Instant::now();
    let mut period_frames = 0u32;
    let mut period_busy = Duration::ZERO;
    let mut frame_error = None;
    let mut stall_timeout_set = false;

    loop {
        let packet = match protocol::read_packet(&mut reader) {
            Ok(p) => p,
            Err(_) if shared.stop.load(Ordering::SeqCst) => return Ok(()),
            // Timed out, or the server gave up (some listed lenses never produce a frame).
            Err(_) if frames == 0 => return Err(StreamEnd::NoPicture),
            Err(e) => return Err(format!("video stream ended: {e}").into()),
        };

        let started = Instant::now();
        match packet {
            Packet::Session { width, height } => {
                log::info!("session {width}x{height}");
                decoder = Some(H264Decoder::new(width, height).map_err(|e| e.to_string())?);
                status(Status::Streaming { serial: serial.to_string(), device: session.device_name.clone(), width, height });
            }
            Packet::Config(data) => {
                if let Some(d) = decoder.as_mut() {
                    d.decode(&data, 0, &mut |_| {}).map_err(|e| e.to_string())?;
                }
            }
            Packet::Frame { pts_us, data, .. } => {
                let Some(d) = decoder.as_mut() else { continue };
                converter.set_mirror(shared.mirror.load(Ordering::Relaxed));
                converter.set_rotation(shared.rotation.load(Ordering::Relaxed));
                d.decode(&data, pts_us, &mut |picture| match converter.convert(picture) {
                    Ok(bgr) => {
                        vcam.send_frame(bgr);
                        if let Some(p) = &config.preview {
                            p.offer(bgr, vcam.width(), vcam.height());
                        }
                        frames += 1;
                        period_frames += 1;
                    }
                    Err(e) => frame_error = Some(e.to_string()),
                })
                .map_err(|e| e.to_string())?;
                if let Some(e) = frame_error.take() {
                    return Err(e.into());
                }
                if frames > 0 && !stall_timeout_set {
                    socket.set_read_timeout(Some(STALL_TIMEOUT)).map_err(|e| e.to_string())?;
                    stall_timeout_set = true;
                }
            }
        }
        period_busy += started.elapsed();

        let elapsed = period_start.elapsed();
        if elapsed >= STATS_PERIOD {
            let stats = Stats {
                fps: period_frames as f32 / elapsed.as_secs_f32(),
                process_ms: period_busy.as_secs_f32() * 1000.0 / period_frames.max(1) as f32,
            };
            log::info!("{:.1} fps, {:.1} ms per frame", stats.fps, stats.process_ms);
            *shared.stats.lock().unwrap() = stats;
            (period_start, period_frames, period_busy) = (Instant::now(), 0, Duration::ZERO);
        }
    }
}

/// Copies everything read into an optional file.
struct Tee<'a> {
    inner: &'a TcpStream,
    copy: Option<File>,
}

impl Read for Tee<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        if let Some(f) = self.copy.as_mut() {
            f.write_all(&buf[..n])?;
        }
        Ok(n)
    }
}
