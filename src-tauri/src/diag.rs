//! Diagnostics: where log lines go, and the logs a bug report attaches.
//!
//! Lines at `info` and up always land in a ring buffer in memory: nothing touches the disk, and
//! a report still has the recent history to show. The "Detailed log" setting adds Plugcam's own
//! `debug` lines and writes everything to `plugcam.log` (rotated at 5 MB, one old file kept).
//! A panic saves the buffer to `last-crash.log`, so a crash leaves a trace with the file off too.
//! Phone serials and the user's profile folder are hidden before a line is stored anywhere.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError, TryLockError};

use log::{Level, LevelFilter, Log, Metadata, Record};

pub const LOG_FILE: &str = "plugcam.log";
pub const OLD_LOG_FILE: &str = "plugcam.1.log";
pub const CRASH_FILE: &str = "last-crash.log";
const BUFFER_LINES: usize = 2000;
const FILE_LIMIT: u64 = 5 * 1024 * 1024;
/// How much of the log files a report takes, from the end.
const REPORT_LOG_LIMIT: u64 = 4 * 1024 * 1024;

struct Logger {
    detailed: AtomicBool,
    state: Mutex<State>,
    /// Development builds also print to the console, filtered by `RUST_LOG` as before.
    #[cfg(debug_assertions)]
    stderr: env_logger::Logger,
}

#[derive(Default)]
struct State {
    lines: VecDeque<String>,
    /// Lines ever stored, and how many of them the file already has.
    stored: u64,
    in_file: u64,
    dir: Option<PathBuf>,
    file: Option<File>,
    file_size: u64,
    /// `(text, replacement)` pairs applied to every line.
    hidden: Vec<(String, String)>,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

/// Installs the logger and the panic hook. Lines are kept in memory until `configure`.
pub fn init() {
    let logger = LOGGER.get_or_init(|| Logger {
        detailed: AtomicBool::new(false),
        state: Mutex::new(State::default()),
        #[cfg(debug_assertions)]
        stderr: env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .format_timestamp_millis()
            .build(),
    });
    if log::set_logger(logger).is_err() {
        return;
    }
    logger.update_max_level();
    if let Some(home) = std::env::var_os("USERPROFILE") {
        logger.lock().hide(&home.to_string_lossy(), "%USERPROFILE%");
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        on_panic(&format!("panic in thread '{thread}': {info}"));
        previous(info);
    }));
}

/// Sets the folder for the log files and turns the file on or off.
pub fn configure(dir: &Path, detailed: bool) {
    if let Some(logger) = LOGGER.get() {
        logger.lock().dir = Some(dir.to_path_buf());
        set_detailed(detailed);
    }
}

/// The "Detailed log" setting: Plugcam's `debug` lines, and everything written to the file.
pub fn set_detailed(on: bool) {
    let Some(logger) = LOGGER.get() else { return };
    logger.detailed.store(on, Ordering::Relaxed);
    logger.update_max_level();
    let opened = {
        let mut st = logger.lock();
        if st.file.is_some() == on {
            return;
        }
        if on {
            st.open_file()
        } else {
            st.file = None;
            Ok(())
        }
    };
    match opened {
        Ok(()) => log::info!("detailed log {}", if on { "on" } else { "off" }),
        Err(e) => log::warn!("opening the log file: {e}"),
    }
}

pub fn is_detailed() -> bool {
    LOGGER.get().is_some_and(|l| l.detailed.load(Ordering::Relaxed))
}

/// Where the log files go, once `configure` ran.
pub fn log_dir() -> Option<PathBuf> {
    LOGGER.get().and_then(|l| l.lock().dir.clone())
}

/// Replaces a phone's serial in every later line, e.g. `R5CT1234ABCD` → `***ABCD`.
pub fn hide_serial(serial: &str) {
    if let Some(logger) = LOGGER.get() {
        logger.lock().hide(serial, &mask_serial(serial));
    }
}

pub fn mask_serial(serial: &str) -> String {
    let chars: Vec<char> = serial.chars().collect();
    let shown = if chars.len() > 6 { &chars[chars.len() - 4..] } else { &[][..] };
    format!("***{}", shown.iter().collect::<String>())
}

/// Hides serials and the profile folder in text that did not go through the logger.
pub fn redact(text: &str) -> String {
    match LOGGER.get() {
        Some(logger) => logger.lock().redact(text),
        None => text.to_string(),
    }
}

/// The log for a report: the end of the files with the detailed log on, else the lines in
/// memory. The flag says which.
pub fn report_log() -> (String, bool) {
    let Some(logger) = LOGGER.get() else { return (String::new(), false) };
    let st = logger.lock();
    match (&st.dir, st.file.is_some()) {
        (Some(dir), true) => {
            let dir = dir.clone();
            drop(st);
            (read_tail(&[dir.join(OLD_LOG_FILE), dir.join(LOG_FILE)], REPORT_LOG_LIMIT), true)
        }
        _ => (st.lines.iter().map(|l| format!("{l}\n")).collect(), false),
    }
}

/// `last-crash.log` if there is one.
pub fn crash_log() -> Option<String> {
    std::fs::read_to_string(log_dir()?.join(CRASH_FILE)).ok()
}

fn on_panic(text: &str) {
    let Some(logger) = LOGGER.get() else { return };
    // The panic may come from inside the logger, with the lock held by this very thread.
    let mut st = match logger.state.try_lock() {
        Ok(st) => st,
        Err(TryLockError::Poisoned(p)) => p.into_inner(),
        Err(TryLockError::WouldBlock) => return,
    };
    let now = jiff::Timestamp::now();
    st.store(format!("{now:.3} ERROR {text}"));
    if let Some(dir) = st.dir.clone() {
        let mut out = format!("Plugcam {} crashed at {now:.0}\n\n", env!("CARGO_PKG_VERSION"));
        for line in &st.lines {
            out.push_str(line);
            out.push('\n');
        }
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(CRASH_FILE), out);
    }
}

/// Whether a line goes to the buffer and the file.
fn keeps(metadata: &Metadata, detailed: bool) -> bool {
    metadata.level() <= Level::Info
        || (detailed && metadata.level() <= Level::Debug && metadata.target().starts_with("plugcam"))
}

impl Logger {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn update_max_level(&self) {
        let ours = if self.detailed.load(Ordering::Relaxed) { LevelFilter::Debug } else { LevelFilter::Info };
        #[cfg(debug_assertions)]
        let ours = ours.max(self.stderr.filter());
        log::set_max_level(ours);
    }
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        #[cfg(debug_assertions)]
        if self.stderr.enabled(metadata) {
            return true;
        }
        keeps(metadata, self.detailed.load(Ordering::Relaxed))
    }

    fn log(&self, record: &Record) {
        #[cfg(debug_assertions)]
        if self.stderr.matches(record) {
            self.stderr.log(record);
        }
        if !keeps(record.metadata(), self.detailed.load(Ordering::Relaxed)) {
            return;
        }
        let target = record.target().strip_prefix("plugcam::").unwrap_or(record.target());
        let line = format!("{:.3} {:<5} {target}: {}", jiff::Timestamp::now(), record.level(), record.args());
        self.lock().store(line);
    }

    fn flush(&self) {}
}

impl State {
    fn hide(&mut self, text: &str, replacement: &str) {
        if !text.is_empty() && !self.hidden.iter().any(|(t, _)| t == text) {
            self.hidden.push((text.to_string(), replacement.to_string()));
        }
    }

    fn redact(&self, text: &str) -> String {
        let mut text = text.to_string();
        for (from, to) in &self.hidden {
            if text.contains(from.as_str()) {
                text = text.replace(from.as_str(), to);
            }
        }
        text
    }

    fn store(&mut self, line: String) {
        let line = self.redact(&line);
        if self.lines.len() == BUFFER_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line.clone());
        self.stored += 1;
        if self.file.is_some() {
            self.in_file = self.stored;
            self.write(&line);
        }
    }

    /// Opens `plugcam.log` for appending and adds the buffered lines it does not have yet,
    /// e.g. the start of this session.
    fn open_file(&mut self) -> std::io::Result<()> {
        let dir = self.dir.clone().ok_or_else(|| std::io::Error::other("no log folder"))?;
        std::fs::create_dir_all(&dir)?;
        let file = OpenOptions::new().create(true).append(true).open(dir.join(LOG_FILE))?;
        self.file_size = file.metadata().map(|m| m.len()).unwrap_or(0);
        self.file = Some(file);
        let first = self.stored - self.lines.len() as u64;
        let skip = self.in_file.saturating_sub(first) as usize;
        let backlog: Vec<String> = self.lines.iter().skip(skip).cloned().collect();
        self.in_file = self.stored;
        for line in backlog {
            self.write(&line);
        }
        Ok(())
    }

    fn write(&mut self, line: &str) {
        let Some(file) = self.file.as_mut() else { return };
        if file.write_all(line.as_bytes()).and_then(|()| file.write_all(b"\n")).is_err() {
            self.file = None;
            return;
        }
        self.file_size += line.len() as u64 + 1;
        if self.file_size >= FILE_LIMIT {
            self.file = None;
            if let Some(dir) = &self.dir {
                let _ = std::fs::rename(dir.join(LOG_FILE), dir.join(OLD_LOG_FILE));
            }
            let _ = self.open_file();
        }
    }
}

/// The last `limit` bytes of the files taken as one text, starting at a whole line.
fn read_tail(paths: &[PathBuf], limit: u64) -> String {
    let sizes: Vec<u64> = paths.iter().map(|p| p.metadata().map(|m| m.len()).unwrap_or(0)).collect();
    let mut skip = sizes.iter().sum::<u64>().saturating_sub(limit);
    let trimmed = skip > 0;
    let mut bytes = Vec::new();
    for (path, size) in paths.iter().zip(sizes) {
        if skip >= size {
            skip -= size;
            continue;
        }
        if let Ok(mut file) = File::open(path)
            && file.seek(SeekFrom::Start(skip)).is_ok()
        {
            let _ = file.read_to_end(&mut bytes);
        }
        skip = 0;
    }
    let text = String::from_utf8_lossy(&bytes);
    let start = if trimmed { text.find('\n').map_or(0, |i| i + 1) } else { 0 };
    text[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("plugcam-diag-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn masks_serials() {
        assert_eq!(mask_serial("R5CT1234ABCD"), "***ABCD");
        assert_eq!(mask_serial("192.168.1.5:5555"), "***5555");
        assert_eq!(mask_serial("abc"), "***");
    }

    #[test]
    fn hides_text_in_lines() {
        let mut st = State::default();
        st.hide("R5CT1234ABCD", "***ABCD");
        st.hide(r"C:\Users\max", "%USERPROFILE%");
        st.store(r"R5CT1234ABCD: settings at C:\Users\max\AppData".into());
        assert_eq!(st.lines[0], r"***ABCD: settings at %USERPROFILE%\AppData");
    }

    #[test]
    fn buffer_keeps_the_last_lines() {
        let mut st = State::default();
        for i in 0..BUFFER_LINES + 5 {
            st.store(i.to_string());
        }
        assert_eq!(st.lines.len(), BUFFER_LINES);
        assert_eq!(st.lines[0], "5");
    }

    #[test]
    fn file_gets_the_backlog_once() {
        let dir = temp_dir("backlog");
        let mut st = State { dir: Some(dir.clone()), ..State::default() };
        st.store("before".into());
        st.open_file().unwrap();
        st.store("while on".into());
        st.file = None;
        st.store("while off".into());
        st.open_file().unwrap();
        st.file = None;
        let text = std::fs::read_to_string(dir.join(LOG_FILE)).unwrap();
        assert_eq!(text, "before\nwhile on\nwhile off\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn file_rotates() {
        let dir = temp_dir("rotate");
        let mut st = State { dir: Some(dir.clone()), ..State::default() };
        st.open_file().unwrap();
        let line = "x".repeat(1023);
        for _ in 0..(FILE_LIMIT / 1024 + 10) {
            st.store(line.clone());
        }
        st.file = None;
        let old = std::fs::metadata(dir.join(OLD_LOG_FILE)).unwrap().len();
        let new = std::fs::metadata(dir.join(LOG_FILE)).unwrap().len();
        assert_eq!(old, FILE_LIMIT);
        assert_eq!(new, 10 * 1024);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn tail_starts_at_a_whole_line() {
        let dir = temp_dir("tail");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a"), "one\ntwo\n").unwrap();
        std::fs::write(dir.join("b"), "three\n").unwrap();
        let paths = [dir.join("a"), dir.join("b"), dir.join("missing")];
        assert_eq!(read_tail(&paths, 100), "one\ntwo\nthree\n");
        assert_eq!(read_tail(&paths, 8), "three\n");
        let _ = std::fs::remove_dir_all(dir);
    }
}
