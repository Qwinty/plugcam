//! Diagnostics: where log lines go, and the logs a bug report attaches.
//!
//! Lines at `info` and up always land in a ring buffer in memory: nothing touches the disk, and
//! a report still has the recent history to show. The "Detailed log" setting adds Plugcam's own
//! `debug` lines and writes everything to `plugcam.log` (rotated at 5 MB, one old file kept).
//! A panic or a native crash (an access violation in Media Foundation, say) saves the buffer to
//! `last-crash.log`, so a crash leaves a trace with the file off too.
//! Phone serials and the user's profile folder are hidden before a line is stored anywhere.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::FromRawHandle;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError, TryLockError};
use std::time::Duration;

use log::{Level, LevelFilter, Log, Metadata, Record};
use windows::Win32::Foundation::{GENERIC_WRITE, HMODULE};
use windows::Win32::Storage::FileSystem::{CREATE_ALWAYS, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ};
use windows::Win32::System::Diagnostics::Debug::{
    EXCEPTION_POINTERS, LPTOP_LEVEL_EXCEPTION_FILTER, SetUnhandledExceptionFilter,
};
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleFileNameW,
    GetModuleHandleExW,
};
use windows::core::PCWSTR;

pub const LOG_FILE: &str = "plugcam.log";
pub const OLD_LOG_FILE: &str = "plugcam.1.log";
pub const CRASH_FILE: &str = "last-crash.log";
/// The bundle identifier in `tauri.conf.json`: Tauri keeps the installed app's logs in
/// `%LOCALAPPDATA%\<identifier>\logs`.
const IDENTIFIER: &str = "io.github.plugcam";
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
    /// `last-crash.log` as null-terminated UTF-16, ready for a crash that must not allocate.
    crash_wide: Vec<u16>,
    file: Option<File>,
    file_size: u64,
    /// `(text, replacement)` pairs applied to every line.
    hidden: Vec<(String, String)>,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();
static PREVIOUS_FILTER: OnceLock<LPTOP_LEVEL_EXCEPTION_FILTER> = OnceLock::new();

/// Installs the logger and the crash handlers. The log folder is known from the start, so a
/// crash before `configure` is kept too.
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
    {
        let mut st = logger.lock();
        if let Some(home) = std::env::var_os("USERPROFILE") {
            st.hide(&home.to_string_lossy(), "%USERPROFILE%");
        }
        if let Some(dir) = default_dir() {
            st.set_dir(dir);
        }
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        on_panic(&format!("panic in thread '{thread}': {info}"));
        previous(info);
    }));
    // SAFETY: `on_exception` has the signature Windows expects and lives for the whole process.
    let previous = unsafe { SetUnhandledExceptionFilter(Some(on_exception)) };
    let _ = PREVIOUS_FILTER.set(previous);
}

/// Where `app::run` will put the logs: `logs\` next to portable data, or Tauri's log folder.
fn default_dir() -> Option<PathBuf> {
    if crate::portable::is_portable() {
        return Some(crate::portable::data_dir().join("logs"));
    }
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join(IDENTIFIER).join("logs"))
}

/// Sets the folder for the log files and turns the file on or off.
pub fn configure(dir: &Path, detailed: bool) {
    if let Some(logger) = LOGGER.get() {
        logger.lock().set_dir(dir.to_path_buf());
        set_detailed(detailed);
    }
}

/// The "Detailed log" setting: Plugcam's `debug` lines, and everything written to the file.
pub fn set_detailed(on: bool) {
    let Some(logger) = LOGGER.get() else { return };
    let changed = logger.lock().file.is_some() != on;
    if changed && !on {
        // Before the file closes, so the file shows where it ends.
        log::info!("detailed log off");
    }
    logger.detailed.store(on, Ordering::Relaxed);
    logger.update_max_level();
    if !changed {
        return;
    }
    if on {
        let opened = logger.lock().open_file();
        match opened {
            Ok(()) => log::info!("detailed log on"),
            Err(e) => log::warn!("opening the log file: {e}"),
        }
    } else {
        logger.lock().file = None;
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

/// The logs for a report, each with a title. The log files go in whenever they exist, so a
/// problem caught with the detailed log on is still there after it is turned off; this session's
/// lines from memory go in too unless the file is on and already has them.
pub fn report_logs() -> Vec<(String, String)> {
    let Some(logger) = LOGGER.get() else { return Vec::new() };
    let st = logger.lock();
    let file_on = st.file.is_some();
    let memory: Option<String> = (!file_on).then(|| st.lines.iter().map(|l| format!("{l}\n")).collect());
    let dir = st.dir.clone();
    drop(st);

    let mut logs = Vec::new();
    if let Some(dir) = dir {
        let text = read_tail(&[dir.join(OLD_LOG_FILE), dir.join(LOG_FILE)], REPORT_LOG_LIMIT);
        if !text.is_empty() {
            let title = if file_on {
                LOG_FILE.to_string()
            } else {
                let written = std::fs::metadata(dir.join(LOG_FILE))
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| jiff::Timestamp::try_from(t).ok())
                    .map_or_else(|| "?".to_string(), |t| format!("{t:.0}"));
                format!("{LOG_FILE}, detailed log now off, last written {written}")
            };
            logs.push((title, text));
        }
    }
    if let Some(memory) = memory {
        logs.push(("this session, from memory".to_string(), memory));
    }
    logs
}

/// `last-crash.log` if there is one.
pub fn crash_log() -> Option<String> {
    std::fs::read_to_string(log_dir()?.join(CRASH_FILE)).ok()
}

fn on_panic(text: &str) {
    let Some(logger) = LOGGER.get() else { return };
    let Some(mut st) = logger.lock_for_crash() else { return };
    let now = jiff::Timestamp::now();
    st.store(format!("{now:.3} ERROR {text}"));
    let header = format!("Plugcam {} crashed at {now:.0}\n\n", env!("CARGO_PKG_VERSION"));
    st.write_crash(&header);
}

/// The last word on an exception nothing else handled: an access violation in a system DLL, for
/// one. The heap may be broken here, so this writes `last-crash.log` without allocating, then
/// lets Windows go on as it would have (the previous filter, then Windows Error Reporting).
unsafe extern "system" fn on_exception(info: *const EXCEPTION_POINTERS) -> i32 {
    // SAFETY: Windows passes valid pointers or null.
    let record = unsafe { info.as_ref().and_then(|i| i.ExceptionRecord.as_ref()) };
    if let Some(mut st) = LOGGER.get().and_then(Logger::lock_for_crash) {
        let mut header = StackText::<512>::new();
        let _ = writeln!(header, "Plugcam {} crashed at {:.0}", env!("CARGO_PKG_VERSION"), jiff::Timestamp::now());
        match record {
            Some(r) => {
                let _ = write!(header, "native exception {:#010X} at {:p}", r.ExceptionCode.0 as u32, r.ExceptionAddress);
                let mut module = StackText::<260>::new();
                if module_name(r.ExceptionAddress, &mut module) {
                    let _ = write!(header, " in {}", module.as_str());
                }
            }
            None => {
                let _ = write!(header, "native exception");
            }
        }
        let _ = header.write_str("\n\n");
        st.write_crash(header.as_str());
        if let Some(file) = st.file.as_mut() {
            let _ = file.write_all(header.as_str().as_bytes());
        }
    }
    match PREVIOUS_FILTER.get().copied().flatten() {
        // SAFETY: the filter that was installed before ours, called as Windows would have.
        Some(previous) => unsafe { previous(info) },
        None => 0, // EXCEPTION_CONTINUE_SEARCH
    }
}

/// The file name of the module that holds `address`, e.g. `mfplat.dll`.
fn module_name(address: *const std::ffi::c_void, out: &mut StackText<260>) -> bool {
    let mut module = HMODULE::default();
    let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    // SAFETY: with FROM_ADDRESS the name argument is an address, which is only looked up.
    if unsafe { GetModuleHandleExW(flags, PCWSTR(address.cast()), &mut module) }.is_err() {
        return false;
    }
    let mut path = [0u16; 260];
    // SAFETY: the buffer is valid for its length.
    let len = unsafe { GetModuleFileNameW(Some(module), &mut path) } as usize;
    if len == 0 {
        return false;
    }
    let path = &path[..len.min(path.len())];
    let name = path.iter().rposition(|&c| c == u16::from(b'\\')).map_or(path, |i| &path[i + 1..]);
    for c in char::decode_utf16(name.iter().copied()) {
        let _ = out.write_char(c.unwrap_or(char::REPLACEMENT_CHARACTER));
    }
    true
}

/// Text formatted into a fixed buffer, for code that must not allocate. Too long is cut short.
struct StackText<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> StackText<N> {
    fn new() -> Self {
        Self { buf: [0; N], len: 0 }
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl<const N: usize> std::fmt::Write for StackText<N> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let mut take = s.len().min(N - self.len);
        while !s.is_char_boundary(take) {
            take -= 1;
        }
        self.buf[self.len..self.len + take].copy_from_slice(&s.as_bytes()[..take]);
        self.len += take;
        Ok(())
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

    /// The lock for a crash handler: another thread may be in the middle of a line, so wait a
    /// little, but give up rather than hang when the crash came from inside the logger itself.
    fn lock_for_crash(&self) -> Option<MutexGuard<'_, State>> {
        for _ in 0..100 {
            match self.state.try_lock() {
                Ok(st) => return Some(st),
                Err(TryLockError::Poisoned(p)) => return Some(p.into_inner()),
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(1)),
            }
        }
        None
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
    /// Sets the log folder and makes it now: a crash handler can only open a file in it.
    fn set_dir(&mut self, dir: PathBuf) {
        let _ = std::fs::create_dir_all(&dir);
        self.crash_wide = dir.join(CRASH_FILE).as_os_str().encode_wide().chain([0]).collect();
        self.dir = Some(dir);
    }

    /// Writes `last-crash.log`: `header`, then the lines in memory. Allocates nothing, so it also
    /// works from the native exception filter.
    fn write_crash(&self, header: &str) {
        if self.crash_wide.is_empty() {
            return;
        }
        // SAFETY: the path is null-terminated; the handle is owned by `file` and closed by it.
        let mut file = unsafe {
            let Ok(handle) = CreateFileW(
                PCWSTR(self.crash_wide.as_ptr()),
                GENERIC_WRITE.0,
                FILE_SHARE_READ,
                None,
                CREATE_ALWAYS,
                FILE_ATTRIBUTE_NORMAL,
                None,
            ) else {
                return;
            };
            File::from_raw_handle(handle.0)
        };
        let _ = file.write_all(header.as_bytes());
        for line in &self.lines {
            let _ = file.write_all(line.as_bytes()).and_then(|()| file.write_all(b"\n"));
        }
    }

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
    fn identifier_matches_tauri_config() {
        let config = include_str!("../tauri.conf.json");
        assert!(config.contains(&format!("\"identifier\": \"{IDENTIFIER}\"")));
    }

    #[test]
    fn stack_text_cuts_at_a_char() {
        let mut t = StackText::<5>::new();
        let _ = t.write_str("abééé");
        assert_eq!(t.as_str(), "abé");
    }

    #[test]
    fn crash_file_has_header_and_buffer() {
        let dir = temp_dir("crash");
        let mut st = State::default();
        st.set_dir(dir.clone());
        st.store("one".into());
        st.store("two".into());
        st.write_crash("crashed\n\n");
        let text = std::fs::read_to_string(dir.join(CRASH_FILE)).unwrap();
        assert_eq!(text, "crashed\n\none\ntwo\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn names_the_module_of_an_address() {
        let mut name = StackText::<260>::new();
        assert!(module_name(names_the_module_of_an_address as *const std::ffi::c_void, &mut name));
        assert!(name.as_str().ends_with(".exe"), "{}", name.as_str());
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
