//! The portable build: a folder with `Plugcam.exe`, `resources\` and `portable.txt`, run from
//! anywhere without installing. It keeps its data next to the exe, registers the camera itself
//! (one administrator prompt) and updates by swapping its own files.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};
use windows::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
use windows::core::{HSTRING, PCWSTR, w};

/// Its presence next to the exe turns the portable mode on.
pub const MARKER: &str = "portable.txt";
/// `Plugcam.exe --camera register|unregister`: what the elevated copy of the app does.
pub const CAMERA_ARG: &str = "--camera";
/// `--wait-pid <pid>`: the updated app waits for the old one to exit before starting.
pub const WAIT_PID_ARG: &str = "--wait-pid";
const OLD_SUFFIX: &str = ".old";

/// The exe's path as it was at start: the running file gets renamed during an update.
fn exe_path() -> &'static Path {
    static EXE: OnceLock<PathBuf> = OnceLock::new();
    EXE.get_or_init(|| std::env::current_exe().unwrap_or_else(|_| PathBuf::from("Plugcam.exe")))
}

fn exe_dir() -> &'static Path {
    exe_path().parent().unwrap_or(Path::new("."))
}

pub fn is_portable() -> bool {
    static PORTABLE: OnceLock<bool> = OnceLock::new();
    *PORTABLE.get_or_init(|| exe_dir().join(MARKER).is_file())
}

/// `data\` next to the exe: settings and the WebView's cache.
pub fn data_dir() -> PathBuf {
    exe_dir().join("data")
}

/// Runs before the app starts. Returns an exit code when this process only had a helper job.
pub fn handle_args() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1));
    if let Some(action) = value(CAMERA_ARG) {
        return Some(match run_regsvr32(action == "unregister") {
            Ok(()) => 0,
            Err(e) => {
                log::error!("camera {action}: {e}");
                1
            }
        });
    }
    if let Some(pid) = value(WAIT_PID_ARG).and_then(|p| p.parse::<u32>().ok()) {
        wait_for_exit(pid, Duration::from_secs(15));
    }
    if is_portable() {
        let webview = data_dir().join("webview");
        // Read by WebView2 when the window is created; nothing else runs yet.
        unsafe { std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", webview) };
        remove_old_files(exe_dir());
    }
    None
}

fn wait_for_exit(pid: u32, timeout: Duration) {
    unsafe {
        if let Ok(process) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            WaitForSingleObject(process, timeout.as_millis() as u32);
            let _ = CloseHandle(process);
        }
    }
}

/// The two camera DLLs: x64 for 64-bit apps, x86 for 32-bit ones.
fn camera_dlls() -> Result<(PathBuf, Option<PathBuf>), String> {
    let x64 = crate::resources::find("plugcam_cam.dll", "PLUGCAM_VCAM_DLL").ok_or("plugcam_cam.dll not found")?;
    let x86 = x64.parent().map(|d| d.join("x86").join("plugcam_cam.dll")).filter(|p| p.is_file());
    Ok((x64, x86))
}

/// Registers or removes "Plugcam Camera"; needs administrator rights.
fn run_regsvr32(unregister: bool) -> Result<(), String> {
    let (x64, x86) = camera_dlls()?;
    let windir = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| r"C:\Windows".into());
    let mut jobs = vec![(windir.join(r"System32\regsvr32.exe"), x64)];
    if let Some(x86) = x86 {
        jobs.push((windir.join(r"SysWOW64\regsvr32.exe"), x86));
    }
    for (regsvr32, dll) in jobs {
        let mut cmd = std::process::Command::new(&regsvr32);
        cmd.arg("/s");
        if unregister {
            cmd.arg("/u");
        }
        let status = cmd.arg(&dll).status().map_err(|e| format!("{}: {e}", regsvr32.display()))?;
        if !status.success() {
            return Err(format!("regsvr32 {} failed ({status})", dll.display()));
        }
    }
    Ok(())
}

/// Starts this exe as administrator to register (or remove) the camera and waits for it.
/// Windows shows its consent prompt; saying no is an error too.
pub fn set_camera_registered(on: bool) -> Result<(), String> {
    let file = HSTRING::from(exe_path().as_os_str());
    let params = HSTRING::from(format!("{CAMERA_ARG} {}", if on { "register" } else { "unregister" }));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    unsafe {
        ShellExecuteExW(&mut info).map_err(|e| e.message())?;
        let process = info.hProcess;
        let waited = WaitForSingleObject(process, INFINITE);
        let mut code = 1u32;
        let _ = GetExitCodeProcess(process, &mut code);
        let _ = CloseHandle(process);
        if waited != WAIT_OBJECT_0 || code != 0 {
            return Err(format!("camera registration failed (exit code {code})"));
        }
    }
    Ok(())
}

/// Replaces the app's files with the ones in `zip` (the portable release) and starts the new
/// version, which waits for this process to exit. Files in use (the exe, a camera DLL loaded
/// by a video app) can be renamed but not overwritten, so each old file is moved to `*.old`
/// first; the next start deletes those. On any error the old files are put back.
pub fn install_update(zip: &[u8]) -> Result<(), String> {
    let dir = exe_dir();
    let staging = dir.join(".update");
    let _ = std::fs::remove_dir_all(&staging);
    let result = (|| {
        zip::ZipArchive::new(Cursor::new(zip)).and_then(|mut a| a.extract(&staging)).map_err(|e| e.to_string())?;
        let root = release_root(&staging).ok_or("Plugcam.exe not found in the update")?;
        swap_in(&root, dir)
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result?;

    std::process::Command::new(exe_path())
        .arg(WAIT_PID_ARG)
        .arg(std::process::id().to_string())
        .spawn()
        .map_err(|e| format!("cannot start the new version: {e}"))?;
    Ok(())
}

/// The folder holding `Plugcam.exe`: the archive root or its only subfolder.
fn release_root(staging: &Path) -> Option<PathBuf> {
    if staging.join("Plugcam.exe").is_file() {
        return Some(staging.to_path_buf());
    }
    let mut dirs = std::fs::read_dir(staging).ok()?.flatten().filter(|e| e.path().is_dir());
    let only = dirs.next()?.path();
    (dirs.next().is_none() && only.join("Plugcam.exe").is_file()).then_some(only)
}

fn swap_in(from: &Path, to: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    collect_files(from, &mut files).map_err(|e| e.to_string())?;
    let mut done: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    let result = files.iter().try_for_each(|src| {
        let rel = src.strip_prefix(from).unwrap();
        // The exe keeps whatever name the user gave it.
        let dest = if rel == Path::new("Plugcam.exe") { exe_path().to_path_buf() } else { to.join(rel) };
        let backup = if dest.exists() {
            let backup = free_old_name(&dest);
            std::fs::rename(&dest, &backup).map_err(|e| format!("{}: {e}", dest.display()))?;
            Some(backup)
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            None
        };
        done.push((dest.clone(), backup));
        std::fs::rename(src, &dest).map_err(|e| format!("{}: {e}", dest.display()))
    });
    if result.is_err() {
        for (dest, backup) in done.into_iter().rev() {
            let _ = std::fs::remove_file(&dest);
            if let Some(backup) = backup {
                let _ = std::fs::rename(backup, &dest);
            }
        }
    }
    result
}

/// `name.old`, or `name.1.old`… when an older one is still locked.
fn free_old_name(path: &Path) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    (0..100)
        .map(|i| path.with_file_name(if i == 0 { format!("{name}{OLD_SUFFIX}") } else { format!("{name}.{i}{OLD_SUFFIX}") }))
        .find(|p| !p.exists() || std::fs::remove_file(p).is_ok())
        .unwrap_or_else(|| path.with_file_name(format!("{name}.{}{OLD_SUFFIX}", std::process::id())))
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// Deletes the `*.old` files an update left behind; ones still in use stay for next time.
fn remove_old_files(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != "data") {
                remove_old_files(&path);
            }
        } else if path.to_string_lossy().ends_with(OLD_SUFFIX) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("plugcam-portable-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn swaps_files_and_keeps_backups() {
        let dir = temp_dir("swap");
        let (new, app) = (dir.join("new"), dir.join("app"));
        std::fs::create_dir_all(new.join("resources")).unwrap();
        std::fs::create_dir_all(app.join("resources")).unwrap();
        std::fs::write(new.join("resources/adb.exe"), "new adb").unwrap();
        std::fs::write(new.join("resources/added.txt"), "added").unwrap();
        std::fs::write(app.join("resources/adb.exe"), "old adb").unwrap();
        std::fs::write(app.join("resources/adb.exe.old"), "older").unwrap();

        swap_in(&new, &app).unwrap();
        assert_eq!(std::fs::read_to_string(app.join("resources/adb.exe")).unwrap(), "new adb");
        assert_eq!(std::fs::read_to_string(app.join("resources/adb.exe.old")).unwrap(), "old adb");
        assert!(app.join("resources/added.txt").is_file());

        remove_old_files(&app);
        assert!(!app.join("resources/adb.exe.old").exists());
        assert!(app.join("resources/adb.exe").is_file());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn finds_the_release_in_a_subfolder() {
        let dir = temp_dir("root");
        assert_eq!(release_root(&dir), None);
        std::fs::create_dir_all(dir.join("Plugcam")).unwrap();
        std::fs::write(dir.join("Plugcam/Plugcam.exe"), "").unwrap();
        assert_eq!(release_root(&dir), Some(dir.join("Plugcam")));
        std::fs::write(dir.join("Plugcam.exe"), "").unwrap();
        assert_eq!(release_root(&dir), Some(dir.clone()));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
