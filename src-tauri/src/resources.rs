//! Finds files shipped with the app (adb, scrcpy-server, camera DLL).

use std::path::PathBuf;

/// Looks for `name` in this order: the path in environment variable `env_override`, next to
/// the executable, in `resources/` next to it (Tauri bundle layout), then in the source tree
/// (`src-tauri/resources`, for `cargo run`).
pub fn find(name: &str, env_override: &str) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(env_override) {
        return Some(PathBuf::from(p));
    }
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from)) {
        candidates.push(dir.join(name));
        candidates.push(dir.join("resources").join(name));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join(name));
    candidates.into_iter().find(|p| p.is_file())
}

/// The camera DLL: bundled or `PLUGCAM_VCAM_DLL`, else the development build in `vcam/dist`.
pub fn vcam_dll() -> Option<PathBuf> {
    find("plugcam_cam.dll", "PLUGCAM_VCAM_DLL").or_else(|| {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(r"..\vcam\dist\bin\x64\plugcam_cam.dll");
        dev.is_file().then_some(dev)
    })
}
