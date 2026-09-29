//! Phone → "Plugcam Camera": waits for a phone, starts the server, decodes, converts and pushes
//! frames, and starts over whenever something breaks, until stopped.
//!
//! Over Wi-Fi the link is adb over TCP: a hiccup does not lose frames, it queues them, and the
//! picture falls further and further behind. So a Wi-Fi phone gets frequent key frames and a
//! lower bit rate, frames that arrive late are skipped up to the next key frame, and a phone
//! that dropped off the network is connected again while the camera keeps the last picture.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::adb::{self, Adb, Device};
use crate::decode::H264Decoder;
use crate::frame::{ColorAdjust, FrameConverter};
use crate::preview::PreviewSlot;
use crate::scrcpy::protocol::{self, ControlMessage, Packet};
use crate::scrcpy::server::{self, CameraParams, Session};
use crate::vcam::VirtualCamera;
use crate::wifi;

/// Some cameras are listed but never deliver a frame (camera 4 on the OnePlus 11R).
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(6);
/// Over Wi-Fi the first frame can also be late because the network is slow.
const FIRST_FRAME_TIMEOUT_WIFI: Duration = Duration::from_secs(10);
/// A camera streams continuously; this much silence means the link is gone.
const STALL_TIMEOUT: Duration = Duration::from_secs(3);
/// Wi-Fi goes quiet for a second or two now and then without being gone.
const STALL_TIMEOUT_WIFI: Duration = Duration::from_secs(5);
const RETRY_DELAY: Duration = Duration::from_secs(2);
const NO_PICTURE_RETRY_DELAY: Duration = Duration::from_secs(15);
const STATS_PERIOD: Duration = Duration::from_secs(5);

/// Seconds between key frames over Wi-Fi, so skipping ahead after a hiccup takes under a second.
const WIFI_KEY_FRAME_INTERVAL: u32 = 1;
/// Starting bit rate over Wi-Fi (the "Economy" one), unless the user's is lower.
const WIFI_START_BITRATE: u32 = 6_000_000;
/// Each time the stream stalls or keeps falling behind, the next session gets this share of
/// the bit rate, down to the floor.
const WIFI_SLOWDOWN: f64 = 0.7;
const WIFI_MIN_BITRATE: u32 = 2_000_000;
/// After this much streaming without trouble, the next session gets one step of the bit rate
/// back, up to where it started.
const WIFI_SPEEDUP_AFTER: Duration = Duration::from_secs(300);
/// A Wi-Fi session shorter than this counts as failed; failures in a row wait longer and longer
/// before the next try, up to `NO_PICTURE_RETRY_DELAY`.
const SHORT_SESSION: Duration = Duration::from_secs(10);
/// Behind real time by more than this, frames are skipped up to the next key frame.
const MAX_LAG: Duration = Duration::from_millis(500);
/// Skip at most this long, even if key frames are late too (a PC too slow to decode in real
/// time); after that the delay is accepted as the new normal.
const MAX_SKIP: Duration = Duration::from_secs(3);
/// This many catch-ups within the window mean the network cannot carry the bit rate.
const CONGESTION_SKIPS: usize = 3;
const CONGESTION_WINDOW: Duration = Duration::from_secs(60);
/// Tries at bringing back a Wi-Fi phone that dropped off the network.
pub const RECONNECT_TRIES: u32 = 5;

pub struct PipelineConfig {
    pub adb: Adb,
    pub server_file: PathBuf,
    /// Prefer this phone; otherwise (or when it is not ready) the first ready one, USB first.
    pub serial: Option<String>,
    pub camera: CameraParams,
    pub mirror: bool,
    /// Clockwise degrees, turned on the PC so it can change without restarting the stream.
    pub rotation: u16,
    /// Brightness, contrast, saturation and warmth; can change while streaming.
    pub color: ColorAdjust,
    /// Record the raw video stream (packets after the codec id) for test fixtures.
    pub dump: Option<PathBuf>,
    /// Where to offer frames for the app window's preview.
    pub preview: Option<Arc<PreviewSlot>>,
    /// Told the camera zoom whenever the phone sets it.
    pub on_zoom: Option<Arc<dyn Fn(f32) + Send + Sync>>,
    /// Told `(old, new)` when a Wi-Fi phone came back under another serial (an mDNS phone
    /// connected again by address).
    pub on_serial: Option<OnSerial>,
    /// Whether a phone may still be used; `false` for one the user forgot, so it is neither
    /// picked nor connected again.
    pub wanted: Option<Wanted>,
}

pub type OnSerial = Arc<dyn Fn(&str, &str) + Send + Sync>;
pub type Wanted = Arc<dyn Fn(&str) -> bool + Send + Sync>;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Status {
    WaitingForDevice,
    Connecting { serial: String },
    Streaming { serial: String, device: String, width: u32, height: u32 },
    /// A Wi-Fi phone stopped answering; the camera keeps the last picture while it is
    /// connected again.
    Reconnecting { serial: String, attempt: u32, tries: u32 },
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
    /// Bit rate asked of the phone, in bits per second.
    pub bitrate: u32,
}

struct Shared {
    stop: AtomicBool,
    mirror: AtomicBool,
    rotation: AtomicU16,
    /// `ColorAdjust::to_bits`.
    color: AtomicU32,
    /// The phone to use when it is ready; can change without restarting the stream.
    preferred: Mutex<Option<String>>,
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
            color: AtomicU32::new(config.color.to_bits()),
            preferred: Mutex::new(config.serial.clone()),
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

    pub fn set_color(&self, adjust: ColorAdjust) {
        self.shared.color.store(adjust.to_bits(), Ordering::Relaxed);
    }

    /// The phone to go back to after the stream breaks; the running stream is left alone.
    pub fn set_preferred(&self, serial: Option<String>) {
        *self.shared.preferred.lock().unwrap() = serial;
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
        // Unblocks the read the pipeline thread may be sitting in. While the server is still
        // starting there is no socket yet; the thread sees the flag at its first packet instead.
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

    let wanted = |serial: &str| config.wanted.as_ref().is_none_or(|w| w(serial));
    let mut wifi_rate = WifiRate::new(config.camera.bit_rate);
    // Wi-Fi sessions in a row that ended before `SHORT_SESSION`.
    let mut wifi_failures = 0u32;
    // The phone the two above are about; another phone starts both over.
    let mut last_serial: Option<String> = None;

    while !stopped() {
        let preferred = shared.preferred.lock().unwrap().clone();
        let device = match pick_device(&config.adb, preferred.as_deref(), &wanted) {
            Ok(Some(device)) => device,
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
        let serial = device.serial.clone();
        let wifi = device.is_wifi();
        if last_serial.as_deref() != Some(serial.as_str()) {
            wifi_rate = WifiRate::new(config.camera.bit_rate);
            wifi_failures = 0;
            last_serial = Some(serial.clone());
        }
        let mut camera = config.camera.clone();
        if wifi {
            camera.key_frame_interval = Some(WIFI_KEY_FRAME_INTERVAL);
            camera.bit_rate = Some(wifi_rate.current);
        }

        status(Status::Connecting { serial: serial.clone() });
        let started = Instant::now();
        let mut frames = 0;
        let result = match server::start(&config.adb, &serial, &config.server_file, &camera) {
            Ok(session) => {
                *shared.video.lock().unwrap() = session.video.try_clone().ok();
                *shared.control.lock().unwrap() = session.control.try_clone().ok();
                if let Some(on_zoom) = &config.on_zoom {
                    let on_zoom = on_zoom.clone();
                    session.log.on_zoom(move |z| on_zoom(z));
                }
                shared.stats.lock().unwrap().bitrate = camera.bit_rate.unwrap_or_default();

                let link = Link { wifi, can_slow_down: wifi && wifi_rate.can_slow_down() };
                let result = stream(&session, &serial, link, &config, &vcam, &mut converter, shared, &mut status, &mut frames);

                *shared.video.lock().unwrap() = None;
                *shared.control.lock().unwrap() = None;
                let problems = session.log.problems();
                session.close();
                *shared.stats.lock().unwrap() = Stats::default();
                result.map_err(|e| match e {
                    StreamEnd::Broken(e) => StreamEnd::Broken(format!("{e}{problems}")),
                    e => e,
                })
            }
            Err(e) => Err(StreamEnd::Broken(e.to_string())),
        };

        if stopped() {
            break;
        }
        if !wifi {
            vcam.send_frame(converter.dim()); // make the frozen picture look disconnected
            match result {
                Ok(()) | Err(StreamEnd::Congested) => pause(RETRY_DELAY),
                Err(StreamEnd::NoPicture) => {
                    status(Status::NoPicture { serial });
                    pause(NO_PICTURE_RETRY_DELAY);
                }
                Err(StreamEnd::Broken(e)) => {
                    status(Status::Error { message: e });
                    pause(RETRY_DELAY);
                }
            }
            continue;
        }

        // Wi-Fi: keep the last picture while trying to get the stream back.
        let streamed = if frames > 0 { started.elapsed() } else { Duration::ZERO };
        wifi_failures = if streamed >= SHORT_SESSION { 0 } else { wifi_failures + 1 };
        // A phone that left the network says nothing about what the network can carry. A phone
        // that is no longer wanted is not asked.
        let alive = !matches!(result, Err(StreamEnd::Broken(_))) || (wanted(&serial) && wifi::is_alive(&config.adb, &serial));
        let congested = match &result {
            Err(StreamEnd::Congested) => true,
            Err(StreamEnd::Broken(_)) => frames > 0 && alive, // it stalled after streaming
            _ => false,
        };
        wifi_rate.session_ended(streamed, congested);
        match result {
            Ok(()) | Err(StreamEnd::Congested) => {}
            Err(StreamEnd::NoPicture) => {
                vcam.send_frame(converter.dim());
                status(Status::NoPicture { serial });
                pause(NO_PICTURE_RETRY_DELAY);
            }
            Err(StreamEnd::Broken(_)) if !alive => {
                let still_wanted = || !stopped() && wanted(&serial);
                match bring_back(&config.adb, &serial, &mut status, &pause, &still_wanted) {
                    Some(back) => {
                        follow_serial(&mut shared.preferred.lock().unwrap(), &serial, &back);
                        last_serial = Some(back.clone()); // the same phone: keep its bit rate
                        if back != serial
                            && let Some(on_serial) = &config.on_serial
                        {
                            on_serial(&serial, &back);
                        }
                        if wifi_failures >= 2 {
                            pause(retry_delay(wifi_failures));
                        }
                    }
                    None => {
                        // Wait for it like for any phone; the app keeps looking for it.
                        vcam.send_frame(converter.dim());
                        status(Status::WaitingForDevice);
                    }
                }
            }
            Err(StreamEnd::Broken(e)) => {
                if wifi_failures >= 2 {
                    vcam.send_frame(converter.dim());
                    status(Status::Error { message: e });
                } else {
                    log::info!("Wi-Fi stream ended, starting again: {e}");
                }
                pause(retry_delay(wifi_failures));
            }
        }
    }
    status(Status::Stopped);
    vcam
}

/// Connects a Wi-Fi phone that stopped answering again, a few times, as long as it is still
/// wanted. Returns its serial once it is back.
fn bring_back(
    adb: &Adb,
    serial: &str,
    status: &mut impl FnMut(Status),
    pause: &impl Fn(Duration),
    still_wanted: &impl Fn() -> bool,
) -> Option<String> {
    for attempt in 1..=RECONNECT_TRIES {
        if !still_wanted() {
            return None;
        }
        status(Status::Reconnecting { serial: serial.to_string(), attempt, tries: RECONNECT_TRIES });
        if let Some(back) = wifi::reconnect(adb, serial, still_wanted) {
            if !still_wanted() {
                // Forgotten while it was being connected.
                let _ = adb.disconnect_within(&back, wifi::QUICK_TIMEOUT);
                return None;
            }
            log::info!("{serial} is back as {back}");
            return Some(back);
        }
        pause(RETRY_DELAY);
    }
    None
}

/// A phone came back as `new`: if it was the preferred one, it still is. Any other preferred
/// phone (or none: automatic) stays as it was.
fn follow_serial(preferred: &mut Option<String>, old: &str, new: &str) {
    if preferred.as_deref() == Some(old) {
        *preferred = Some(new.to_string());
    }
}

/// The bit rate Wi-Fi sessions ask for: a step down whenever the network cannot carry it, a step
/// back up after `WIFI_SPEEDUP_AFTER` of streaming without trouble.
struct WifiRate {
    current: u32,
    /// The user's bit rate or `WIFI_START_BITRATE`, whichever is lower.
    top: u32,
    /// Streaming time since the last change.
    clean: Duration,
}

impl WifiRate {
    fn new(user: Option<u32>) -> Self {
        let top = user.unwrap_or(WIFI_START_BITRATE).min(WIFI_START_BITRATE);
        Self { current: top, top, clean: Duration::ZERO }
    }

    fn can_slow_down(&self) -> bool {
        self.current > WIFI_MIN_BITRATE
    }

    /// After a session that gave a picture for `streamed`; `congested` when it ended because
    /// the network could not keep up.
    fn session_ended(&mut self, streamed: Duration, congested: bool) {
        let old = self.current;
        if congested {
            self.clean = Duration::ZERO;
            if self.can_slow_down() {
                self.current = ((self.current as f64 * WIFI_SLOWDOWN).round() as u32).max(WIFI_MIN_BITRATE);
            }
        } else {
            self.clean += streamed;
            if self.clean >= WIFI_SPEEDUP_AFTER && self.current < self.top {
                self.clean = Duration::ZERO;
                self.current = ((self.current as f64 / WIFI_SLOWDOWN).round() as u32).min(self.top);
            }
        }
        if self.current != old {
            log::info!("Wi-Fi bit rate {:.1} → {:.1} Mbit/s", old as f32 / 1e6, self.current as f32 / 1e6);
        }
    }
}

/// How long to wait before the next Wi-Fi session after `failures` short ones in a row: at
/// once for a single hiccup, then longer and longer.
fn retry_delay(failures: u32) -> Duration {
    match failures {
        0 | 1 => Duration::from_millis(500),
        n => RETRY_DELAY.saturating_mul(1 << (n - 2).min(8)).min(NO_PICTURE_RETRY_DELAY),
    }
}

enum StreamEnd {
    NoPicture,
    /// Frames kept arriving late: the network cannot carry this bit rate.
    Congested,
    Broken(String),
}

impl From<String> for StreamEnd {
    fn from(e: String) -> Self {
        StreamEnd::Broken(e)
    }
}

fn pick_device(adb: &Adb, preferred: Option<&str>, wanted: &impl Fn(&str) -> bool) -> Result<Option<Device>, String> {
    let mut devices = adb.devices().map_err(|e| e.to_string())?;
    devices.retain(|d| wanted(&d.serial));
    Ok(adb::choose(&devices, preferred).cloned())
}

#[derive(Clone, Copy)]
struct Link {
    wifi: bool,
    /// The bit rate can still go down, so falling behind again and again ends the session.
    can_slow_down: bool,
}

/// Reads the whole stream of one session. Returns `Ok` when stopped on request.
#[allow(clippy::too_many_arguments)]
fn stream(
    session: &Session,
    serial: &str,
    link: Link,
    config: &PipelineConfig,
    vcam: &VirtualCamera,
    converter: &mut FrameConverter,
    shared: &Shared,
    status: &mut impl FnMut(Status),
    frames: &mut u64,
) -> Result<(), StreamEnd> {
    let (first_timeout, stall_timeout) =
        if link.wifi { (FIRST_FRAME_TIMEOUT_WIFI, STALL_TIMEOUT_WIFI) } else { (FIRST_FRAME_TIMEOUT, STALL_TIMEOUT) };
    let socket = session.video.try_clone().map_err(|e| e.to_string())?;
    socket.set_read_timeout(Some(first_timeout)).map_err(|e| e.to_string())?;
    let dump = match &config.dump {
        Some(path) => Some(File::create(path).map_err(|e| format!("{}: {e}", path.display()))?),
        None => None,
    };
    let mut reader = BufReader::with_capacity(1 << 20, Tee { inner: &socket, copy: dump });

    let mut decoder: Option<H264Decoder> = None;
    let mut period_start = Instant::now();
    let mut period_frames = 0u32;
    let mut period_busy = Duration::ZERO;
    let mut frame_error = None;
    let mut stall_timeout_set = false;
    let mut catch_up = link.wifi.then(CatchUp::default);

    loop {
        // A stop that came while the server was starting had no socket to shut down, and a
        // healthy stream never fails a read, so the flag is checked on every packet too.
        if shared.stop.load(Ordering::SeqCst) {
            return Ok(());
        }
        let packet = match protocol::read_packet(&mut reader) {
            Ok(p) => p,
            Err(_) if shared.stop.load(Ordering::SeqCst) => return Ok(()),
            // Over USB, no first frame means the lens gives no picture (some listed lenses
            // never do). Over Wi-Fi it may just be the network, unless the server says so.
            Err(_) if *frames == 0 && (!link.wifi || session.log.camera_failed()) => return Err(StreamEnd::NoPicture),
            Err(e) if *frames == 0 => return Err(format!("no video from the phone: {e}").into()),
            Err(e) => return Err(format!("video stream ended: {e}").into()),
        };

        let started = Instant::now();
        match packet {
            Packet::Session { width, height } => {
                log::info!("session {width}x{height}");
                decoder = Some(H264Decoder::new(width, height).map_err(|e| e.to_string())?);
                if let Some(c) = catch_up.as_mut() {
                    c.lag.reset();
                }
                status(Status::Streaming { serial: serial.to_string(), device: session.device_name.clone(), width, height });
            }
            Packet::Config(data) => {
                if let Some(d) = decoder.as_mut() {
                    d.decode(&data, 0, &mut |_| {}).map_err(|e| e.to_string())?;
                }
            }
            Packet::Frame { pts_us, key_frame, data } => {
                let Some(d) = decoder.as_mut() else { continue };
                if let Some(c) = catch_up.as_mut() {
                    match c.check(pts_us, key_frame, started) {
                        Verdict::Show => {}
                        Verdict::Skip => continue,
                        Verdict::Congested if link.can_slow_down => return Err(StreamEnd::Congested),
                        Verdict::Congested => continue,
                    }
                }
                converter.set_mirror(shared.mirror.load(Ordering::Relaxed));
                converter.set_rotation(shared.rotation.load(Ordering::Relaxed));
                converter.set_color(ColorAdjust::from_bits(shared.color.load(Ordering::Relaxed)));
                d.decode(&data, pts_us, &mut |picture| match converter.convert(picture) {
                    Ok(bgr) => {
                        vcam.send_frame(bgr);
                        if let Some(p) = &config.preview {
                            p.offer(bgr, vcam.width(), vcam.height());
                        }
                        *frames += 1;
                        period_frames += 1;
                    }
                    Err(e) => frame_error = Some(e.to_string()),
                })
                .map_err(|e| e.to_string())?;
                if let Some(e) = frame_error.take() {
                    return Err(e.into());
                }
                if *frames > 0 && !stall_timeout_set {
                    socket.set_read_timeout(Some(stall_timeout)).map_err(|e| e.to_string())?;
                    stall_timeout_set = true;
                }
            }
        }
        period_busy += started.elapsed();

        let elapsed = period_start.elapsed();
        if elapsed >= STATS_PERIOD {
            let mut stats = shared.stats.lock().unwrap();
            stats.fps = period_frames as f32 / elapsed.as_secs_f32();
            stats.process_ms = period_busy.as_secs_f32() * 1000.0 / period_frames.max(1) as f32;
            log::info!("{:.1} fps, {:.1} ms per frame", stats.fps, stats.process_ms);
            (period_start, period_frames, period_busy) = (Instant::now(), 0, Duration::ZERO);
        }
    }
}

/// How far behind the phone the picture is: time passed on the PC since the reference frame
/// minus time passed on the phone's clock. Frames that sat in a queue make the difference grow.
#[derive(Default)]
struct LagMeter {
    /// PC time and phone time of the reference frame.
    base: Option<(Instant, u64)>,
    /// The smallest difference seen, in microseconds: the frame that came through fastest.
    min_offset: i64,
    last: Option<Instant>,
}

impl LagMeter {
    fn lag(&mut self, pts_us: u64, now: Instant) -> Duration {
        let (t0, p0) = *self.base.get_or_insert((now, pts_us));
        let offset = now.duration_since(t0).as_micros() as i64 - (pts_us as i64 - p0 as i64);
        // Let the minimum rise by 1 ms per second, so a slow drift between the two clocks
        // (a fraction of a second per hour) is never taken for lag.
        let leak = self.last.map_or(0, |l| now.duration_since(l).as_micros() as i64 / 1000);
        self.min_offset = (self.min_offset + leak).min(offset);
        self.last = Some(now);
        Duration::from_micros((offset - self.min_offset).max(0) as u64)
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

enum Verdict {
    Show,
    Skip,
    /// Skipping ahead keeps being needed.
    Congested,
}

/// Skips frames up to the next key frame once the picture is `MAX_LAG` behind.
#[derive(Default)]
struct CatchUp {
    lag: LagMeter,
    skipping_since: Option<Instant>,
    skipped: u32,
    /// When each recent catch-up started.
    recent: VecDeque<Instant>,
}

impl CatchUp {
    fn check(&mut self, pts_us: u64, key_frame: bool, now: Instant) -> Verdict {
        let behind = self.lag.lag(pts_us, now);
        let Some(since) = self.skipping_since else {
            if behind <= MAX_LAG {
                return Verdict::Show;
            }
            log::info!("{} ms behind, skipping to the next key frame", behind.as_millis());
            self.skipping_since = Some(now);
            self.skipped = 1;
            self.recent.retain(|t| now.duration_since(*t) < CONGESTION_WINDOW);
            self.recent.push_back(now);
            if self.recent.len() >= CONGESTION_SKIPS {
                self.recent.clear();
                return Verdict::Congested;
            }
            return Verdict::Skip;
        };
        let caught_up = behind <= MAX_LAG / 2;
        if key_frame && (caught_up || now.duration_since(since) > MAX_SKIP) {
            log::info!("skipped {} frames, {} ms behind now", self.skipped, behind.as_millis());
            if !caught_up {
                // The PC cannot keep up; measure from here instead of skipping forever.
                self.lag.reset();
                self.lag.lag(pts_us, now);
            }
            self.skipping_since = None;
            return Verdict::Show;
        }
        self.skipped += 1;
        Verdict::Skip
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

#[cfg(test)]
mod tests {
    use super::*;

    const MS: u64 = 1000;

    fn clock() -> impl Fn(u64) -> Instant {
        let t0 = Instant::now();
        move |ms| t0 + Duration::from_millis(ms)
    }

    #[test]
    fn lag_is_how_much_later_than_the_fastest_frame() {
        let at = clock();
        let mut m = LagMeter::default();
        assert_eq!(m.lag(1_000 * MS, at(0)), Duration::ZERO);
        assert!((6..=7).contains(&m.lag(1_033 * MS, at(40)).as_millis()));
        // A frame that came through faster becomes the reference.
        assert_eq!(m.lag(1_066 * MS, at(60)), Duration::ZERO);
        // Frames stuck for a second.
        assert!(m.lag(1_100 * MS, at(1_100)).as_millis() >= 900);
    }

    #[test]
    fn slow_clock_drift_is_not_lag() {
        let at = clock();
        let mut m = LagMeter::default();
        // The phone's clock runs 100 ppm slow for an hour: 0.36 s apart by the end.
        let mut worst = Duration::ZERO;
        for s in 0..3600u64 {
            let pts = s * 1_000_000 - s * 100;
            worst = worst.max(m.lag(pts, at(s * 1000)));
        }
        assert!(worst < Duration::from_millis(5), "{worst:?}");
    }

    #[test]
    fn catches_up_at_the_next_key_frame() {
        let at = clock();
        let mut c = CatchUp::default();
        assert!(matches!(c.check(0, true, at(0)), Verdict::Show));
        assert!(matches!(c.check(33 * MS, false, at(33)), Verdict::Show));
        // Two seconds of frames arrive at once after a hiccup.
        assert!(matches!(c.check(66 * MS, false, at(2_000)), Verdict::Skip));
        assert!(matches!(c.check(1_000 * MS, true, at(2_001)), Verdict::Skip)); // key frame, still late
        assert!(matches!(c.check(1_500 * MS, false, at(2_002)), Verdict::Skip));
        assert!(matches!(c.check(2_000 * MS, true, at(2_010)), Verdict::Show)); // caught up
        assert!(matches!(c.check(2_033 * MS, false, at(2_043)), Verdict::Show));
    }

    #[test]
    fn gives_up_skipping_when_the_pc_is_too_slow() {
        let at = clock();
        let mut c = CatchUp::default();
        c.check(0, true, at(0));
        assert!(matches!(c.check(33 * MS, false, at(1_000)), Verdict::Skip));
        assert!(matches!(c.check(1_000 * MS, true, at(3_000)), Verdict::Skip));
        assert!(matches!(c.check(2_000 * MS, true, at(4_100)), Verdict::Show)); // 3 s passed
        assert!(matches!(c.check(2_033 * MS, false, at(4_133)), Verdict::Show));
    }

    #[test]
    fn repeated_catch_ups_mean_congestion() {
        let at = clock();
        let mut c = CatchUp::default();
        let mut verdicts = Vec::new();
        let mut now = 0;
        for _ in 0..3 {
            c.check(now * MS, true, at(now));
            // One second stuck, then a key frame that is on time.
            verdicts.push(c.check((now + 33) * MS, false, at(now + 1_000)));
            now += 1_000;
            assert!(matches!(c.check(now * MS, true, at(now)), Verdict::Show));
            now += 5_000;
        }
        assert!(matches!(verdicts[0], Verdict::Skip));
        assert!(matches!(verdicts[1], Verdict::Skip));
        assert!(matches!(verdicts[2], Verdict::Congested));
    }

    #[test]
    fn wifi_bit_rate_steps_down_and_back_up() {
        let min = Duration::from_secs(60);
        let mut r = WifiRate::new(Some(12_000_000));
        assert_eq!(r.current, WIFI_START_BITRATE);
        r.session_ended(min, true);
        assert_eq!(r.current, 4_200_000);
        r.session_ended(min, true);
        r.session_ended(min, true);
        assert_eq!(r.current, 2_058_000);
        r.session_ended(min, true);
        assert_eq!(r.current, WIFI_MIN_BITRATE);
        assert!(!r.can_slow_down());
        r.session_ended(min, true);
        assert_eq!(r.current, WIFI_MIN_BITRATE);
        // Clean streaming adds up across sessions (e.g. the phone left the network in between).
        r.session_ended(4 * min, false);
        assert_eq!(r.current, WIFI_MIN_BITRATE);
        r.session_ended(min, false);
        assert_eq!(r.current, 2_857_143);
        for _ in 0..3 {
            r.session_ended(5 * min, false);
        }
        assert_eq!(r.current, WIFI_START_BITRATE);
        r.session_ended(60 * min, false);
        assert_eq!(r.current, WIFI_START_BITRATE);
        // Trouble starts the clean time over.
        r.session_ended(4 * min, false);
        r.session_ended(Duration::ZERO, true);
        r.session_ended(4 * min, false);
        assert_eq!(r.current, 4_200_000);
    }

    #[test]
    fn wifi_bit_rate_never_above_the_users() {
        let mut r = WifiRate::new(Some(4_000_000));
        assert_eq!(r.current, 4_000_000);
        r.session_ended(Duration::ZERO, true);
        r.session_ended(Duration::from_secs(600), false);
        assert_eq!(r.current, 4_000_000);
    }

    #[test]
    fn only_the_preferred_phone_is_followed_to_a_new_serial() {
        let mut p = Some("adb-1._adb-tls-connect._tcp".to_string());
        follow_serial(&mut p, "adb-1._adb-tls-connect._tcp", "10.0.0.2:4000");
        assert_eq!(p.as_deref(), Some("10.0.0.2:4000"));
        let mut p = Some("usb1".to_string());
        follow_serial(&mut p, "10.0.0.2:4000", "10.0.0.2:4001");
        assert_eq!(p.as_deref(), Some("usb1"));
        let mut p = None;
        follow_serial(&mut p, "10.0.0.2:4000", "10.0.0.2:4001");
        assert_eq!(p, None);
    }

    #[test]
    fn failed_sessions_back_off() {
        let delays: Vec<u64> = (0..9).map(|n| retry_delay(n).as_millis() as u64).collect();
        assert_eq!(delays, [500, 500, 2_000, 4_000, 8_000, 15_000, 15_000, 15_000, 15_000]);
        assert_eq!(retry_delay(u32::MAX), NO_PICTURE_RETRY_DELAY);
    }
}
