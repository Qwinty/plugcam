//! Starts scrcpy-server on the phone in camera mode and opens the video and control sockets.

use std::collections::VecDeque;
use std::hash::{BuildHasher, Hasher};
use std::io::{self, BufRead, BufReader, Read};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::{protocol, zoom};
use crate::adb::{Adb, AdbError};
use crate::resources;

/// The server refuses to run for any other client version. Bump only together with a
/// protocol review (docs/SPEC.md 1.4).
pub const SERVER_VERSION: &str = "4.1";
pub const SERVER_SHA256: &str = "deacb991ed2509715160ffdc7907e47b4160eb30d1566217e9047fd5b8850cae";

const DEVICE_SERVER_PATH: &str = "/data/local/tmp/plugcam-server.jar";
const DEVICE_LIST_PATH: &str = "/data/local/tmp/plugcam-list.jar";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(30);
/// A server left over from a crashed session keeps the camera busy for a moment after it is
/// killed ("Too many other clients connecting"), see docs/stage0.md.
const STALE_SERVER_GRACE: Duration = Duration::from_secs(3);
const LOG_LINES_KEPT: usize = 20;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error(transparent)]
    Adb(#[from] AdbError),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("scrcpy-server file not found (bundled or PLUGCAM_SERVER)")]
    NotFound,
    #[error("scrcpy-server checksum mismatch: expected {expected}, got {actual}")]
    Checksum { expected: &'static str, actual: String },
    #[error("unexpected video codec {0:#010x}, expected h264")]
    Codec(u32),
    #[error("the phone did not connect within {0:?}{1}")]
    Timeout(Duration, String),
    #[error("scrcpy-server exited{0}")]
    Exited(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facing {
    Back,
    Front,
    External,
}

impl Facing {
    fn as_str(self) -> &'static str {
        match self {
            Facing::Back => "back",
            Facing::Front => "front",
            Facing::External => "external",
        }
    }
}

/// What to capture. `camera_id` wins over `facing` when both are set.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraParams {
    pub camera_id: Option<String>,
    pub facing: Option<Facing>,
    pub size: Option<(u32, u32)>,
    pub fps: u32,
    /// Camera2 constrained high-speed session: fps must be one of the high-speed rates
    /// (120 on the OnePlus 11R, which reaches the PC as ~60 fps).
    pub high_speed: bool,
    /// Bits per second; `None` keeps scrcpy's default (8 Mbit/s).
    pub bit_rate: Option<u32>,
    /// 0, 90, 180 or 270, applied on the phone before encoding.
    pub orientation: u16,
    pub torch: bool,
    pub zoom: Option<f32>,
    /// Seconds between key frames; `None` keeps scrcpy's 10. Over Wi-Fi a short interval lets
    /// the PC skip ahead to real time quickly after a hiccup.
    pub key_frame_interval: Option<u32>,
}

impl Default for CameraParams {
    fn default() -> Self {
        Self {
            camera_id: None,
            facing: Some(Facing::Back),
            size: Some((1920, 1080)),
            fps: 30,
            high_speed: false,
            bit_rate: None,
            orientation: 0,
            torch: false,
            zoom: None,
            key_frame_interval: None,
        }
    }
}

impl CameraParams {
    fn server_args(&self) -> Vec<String> {
        let mut args = vec!["video_source=camera".to_string(), "video_codec=h264".to_string()];
        match (&self.camera_id, self.facing) {
            (Some(id), _) => args.push(format!("camera_id={id}")),
            (None, Some(facing)) => args.push(format!("camera_facing={}", facing.as_str())),
            (None, None) => {}
        }
        if let Some((w, h)) = self.size {
            args.push(format!("camera_size={w}x{h}"));
        }
        args.push(format!("camera_fps={}", self.fps));
        if self.high_speed {
            args.push("camera_high_speed=true".into());
        }
        if let Some(rate) = self.bit_rate {
            args.push(format!("video_bit_rate={rate}"));
        }
        if self.orientation != 0 {
            args.push(format!("capture_orientation={}", self.orientation));
        }
        if self.torch {
            args.push("camera_torch=true".into());
        }
        if let Some(zoom) = self.zoom {
            args.push(format!("camera_zoom={zoom}"));
        }
        if let Some(secs) = self.key_frame_interval {
            args.push(format!("video_codec_options=i-frame-interval={secs}"));
        }
        args
    }
}

type ZoomHook = Box<dyn Fn(f32) + Send>;

/// Last lines printed by the server, used to explain failures, and the camera zoom it reports.
#[derive(Clone, Default)]
pub struct ServerLog {
    lines: Arc<Mutex<VecDeque<String>>>,
    zoom: Arc<Mutex<(Option<f32>, Option<ZoomHook>)>>,
}

impl ServerLog {
    fn push(&self, line: String) {
        if let Some(z) = zoom::parse_log_line(&line) {
            let mut zoom = self.zoom.lock().unwrap();
            zoom.0 = Some(z);
            if let Some(hook) = &zoom.1 {
                hook(z);
            }
        }
        let mut lines = self.lines.lock().unwrap();
        if lines.len() == LOG_LINES_KEPT {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    /// Calls `hook` with every zoom the server sets from now on, and with the last one it
    /// already set, if any.
    pub fn on_zoom(&self, hook: impl Fn(f32) + Send + 'static) {
        let mut zoom = self.zoom.lock().unwrap();
        if let Some(z) = zoom.0 {
            hook(z);
        }
        zoom.1 = Some(Box::new(hook));
    }

    /// The camera itself failed (e.g. `Camera capture failed: frame 0` from a lens that is listed
    /// but gives no picture), as opposed to the connection being slow.
    pub fn camera_failed(&self) -> bool {
        let lines = self.lines.lock().unwrap();
        lines.iter().any(|l| l.contains("Camera capture failed") || (l.contains("ERROR") && l.contains("amera")))
    }

    /// Distinct error and warning lines (at most 3), formatted to be appended to a message.
    pub fn problems(&self) -> String {
        let lines = self.lines.lock().unwrap();
        let mut bad: Vec<&str> = Vec::new();
        for line in lines.iter().map(|l| l.trim()) {
            let is_problem = line.contains("ERROR") || line.contains("Exception") || line.contains("WARN");
            // "Camera capture failed: frame 0", "…frame 1"… count as one problem.
            let key = |l: &str| l.trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
            if is_problem && bad.len() < 3 && !bad.iter().any(|b| key(b) == key(line)) {
                bad.push(line);
            }
        }
        if bad.is_empty() { String::new() } else { format!(": {}", bad.join(" | ")) }
    }
}

/// A running server with its sockets. Dropping it closes the sockets and stops the server.
pub struct Session {
    pub device_name: String,
    pub video: TcpStream,
    pub control: TcpStream,
    pub log: ServerLog,
    child: Child,
}

impl Session {
    /// Closing the sockets makes the server exit on its own; kill the adb process if it does not.
    pub fn close(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        let _ = self.video.shutdown(Shutdown::Both);
        let _ = self.control.shutdown(Shutdown::Both);
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Bundled `scrcpy-server` (or `PLUGCAM_SERVER`), checked against the pinned SHA-256.
pub fn server_file() -> Result<PathBuf, ServerError> {
    let path = resources::find("scrcpy-server", "PLUGCAM_SERVER").ok_or(ServerError::NotFound)?;
    let actual = hex(&Sha256::digest(std::fs::read(&path)?));
    if actual != SERVER_SHA256 {
        return Err(ServerError::Checksum { expected: SERVER_SHA256, actual });
    }
    Ok(path)
}

/// Pushes and starts the server, then connects the video and control sockets.
pub fn start(adb: &Adb, serial: &str, server: &Path, params: &CameraParams) -> Result<Session, ServerError> {
    kill_stale_servers(adb, serial);
    adb.push(serial, server, DEVICE_SERVER_PATH)?;

    let scid = random_scid();
    let socket = format!("localabstract:scrcpy_{scid:08x}");

    // Reverse tunnel first (the phone connects to us); forward as a fallback, as scrcpy does.
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    let reverse = match adb.reverse(serial, &socket, &format!("tcp:{port}")) {
        Ok(()) => true,
        Err(e) => {
            log::warn!("adb reverse failed, trying forward: {e}");
            adb.forward(serial, &format!("tcp:{port}"), &socket)?;
            false
        }
    };

    let mut extra = params.server_args();
    if !reverse {
        extra.push("tunnel_forward=true".into());
    }
    let mut child = spawn_server(adb, serial, scid, &extra)?;
    let log = capture_output(&mut child);

    let sockets = if reverse {
        accept_reverse(&listener, &mut child, &log)
    } else {
        drop(listener);
        connect_forward(port, &mut child, &log)
    };
    // Established connections survive; the tunnel itself is no longer needed.
    let _ = if reverse { adb.reverse_remove(serial, &socket) } else { adb.forward_remove(serial, &format!("tcp:{port}")) };

    let (video, control) = match sockets {
        Ok(s) => s,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    };
    let mut session = Session { device_name: String::new(), video, control, log, child };

    let mut video = &session.video;
    let meta = protocol::read_device_name(&mut video).and_then(|name| Ok((name, protocol::read_codec_id(&mut video)?)));
    let (name, codec) = meta.map_err(|_| {
        // The server closed the socket right away, e.g. "Camera with id 1 not found".
        std::thread::sleep(Duration::from_millis(200));
        ServerError::Exited(session.log.problems())
    })?;
    session.device_name = name;
    if codec != protocol::CODEC_H264 {
        return Err(ServerError::Codec(codec));
    }
    Ok(session)
}

/// Runs the server in list mode and returns its report of cameras and their sizes.
pub fn list_cameras(adb: &Adb, serial: &str, server: &Path) -> Result<String, ServerError> {
    // Own copy: the server deletes its jar when it exits, which must not hit a streaming session
    // that is starting at the same time.
    adb.push(serial, server, DEVICE_LIST_PATH)?;
    let classpath = format!("CLASSPATH={DEVICE_LIST_PATH}");
    let args = ["shell", &classpath, "app_process", "/", "com.genymobile.scrcpy.Server", SERVER_VERSION];
    let args = [&args[..], &["log_level=info", "list_cameras=true", "list_camera_sizes=true"]].concat();
    let out = adb.output(Some(serial), &args, LIST_TIMEOUT)?;
    Ok(out.stdout + &out.stderr)
}

fn kill_stale_servers(adb: &Adb, serial: &str) {
    // The bracket keeps pkill from matching the shell that runs this very command.
    if adb.shell(serial, "pkill -f 'com[.]genymobile[.]scrcpy[.]Server'").is_ok() {
        log::info!("killed a leftover scrcpy-server, waiting for the camera to be released");
        std::thread::sleep(STALE_SERVER_GRACE);
    }
}

fn spawn_server(adb: &Adb, serial: &str, scid: u32, extra: &[String]) -> io::Result<Child> {
    let mut cmd = adb.command(Some(serial));
    cmd.args(["shell", &format!("CLASSPATH={DEVICE_SERVER_PATH}")])
        .args(["app_process", "/", "com.genymobile.scrcpy.Server", SERVER_VERSION])
        .arg(format!("scid={scid:08x}"))
        .args(["log_level=info", "audio=false", "control=true", "cleanup=true"])
        .args(extra)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    log::debug!("starting server: {cmd:?}");
    cmd.spawn()
}

fn capture_output(child: &mut Child) -> ServerLog {
    let log = ServerLog::default();
    let pipes: [Option<Box<dyn Read + Send>>; 2] = [
        child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>),
        child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>),
    ];
    for pipe in pipes.into_iter().flatten() {
        let log = log.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                log::info!("{line}");
                log.push(line);
            }
        });
    }
    log
}

fn exited(child: &mut Child, log: &ServerLog) -> Option<ServerError> {
    match child.try_wait() {
        Ok(Some(_)) => {
            // Give the output threads a moment to collect the last lines.
            std::thread::sleep(Duration::from_millis(200));
            Some(ServerError::Exited(log.problems()))
        }
        _ => None,
    }
}

fn accept_reverse(listener: &TcpListener, child: &mut Child, log: &ServerLog) -> Result<(TcpStream, TcpStream), ServerError> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    let mut sockets = Vec::with_capacity(2);
    while sockets.len() < 2 {
        match listener.accept() {
            Ok((s, _)) => {
                s.set_nonblocking(false)?;
                s.set_nodelay(true)?;
                sockets.push(s);
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                if let Some(e) = exited(child, log) {
                    return Err(e);
                }
                if Instant::now() > deadline {
                    return Err(ServerError::Timeout(CONNECT_TIMEOUT, log.problems()));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(e.into()),
        }
    }
    let control = sockets.pop().unwrap();
    Ok((sockets.pop().unwrap(), control))
}

fn connect_forward(port: u16, child: &mut Child, log: &ServerLog) -> Result<(TcpStream, TcpStream), ServerError> {
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    let video = loop {
        // adb accepts the connection even before the server listens; the dummy byte proves it's there.
        if let Ok(mut s) = TcpStream::connect((Ipv4Addr::LOCALHOST, port)) {
            s.set_read_timeout(Some(Duration::from_secs(2)))?;
            if protocol::read_dummy_byte(&mut s).is_ok() {
                s.set_read_timeout(None)?;
                break s;
            }
        }
        if let Some(e) = exited(child, log) {
            return Err(e);
        }
        if Instant::now() > deadline {
            return Err(ServerError::Timeout(CONNECT_TIMEOUT, log.problems()));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let control = TcpStream::connect((Ipv4Addr::LOCALHOST, port))?;
    video.set_nodelay(true)?;
    control.set_nodelay(true)?;
    Ok((video, control))
}

/// 31-bit random session id, so parallel clients on one phone get different socket names.
fn random_scid() -> u32 {
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(Instant::now().elapsed().as_nanos() ^ std::process::id() as u128);
    (h.finish() & 0x7fff_ffff) as u32
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_args() {
        assert_eq!(
            CameraParams::default().server_args(),
            ["video_source=camera", "video_codec=h264", "camera_facing=back", "camera_size=1920x1080", "camera_fps=30"]
        );
    }

    #[test]
    fn camera_id_wins_and_extras_are_passed() {
        let p = CameraParams {
            camera_id: Some("2".into()),
            facing: Some(Facing::Front),
            size: None,
            fps: 120,
            high_speed: true,
            bit_rate: Some(16_000_000),
            orientation: 90,
            torch: true,
            zoom: Some(2.0),
            key_frame_interval: Some(1),
        };
        assert_eq!(
            p.server_args(),
            [
                "video_source=camera",
                "video_codec=h264",
                "camera_id=2",
                "camera_fps=120",
                "camera_high_speed=true",
                "video_bit_rate=16000000",
                "capture_orientation=90",
                "camera_torch=true",
                "camera_zoom=2",
                "video_codec_options=i-frame-interval=1",
            ]
        );
    }

    #[test]
    fn scid_is_31_bit() {
        for _ in 0..100 {
            assert!(random_scid() <= 0x7fff_ffff);
        }
    }

    #[test]
    fn bundled_server_matches_pinned_checksum() {
        server_file().unwrap();
    }
}
