//! Small Windows facts the app needs: is the camera DLL registered, which Windows build, and
//! what a bug report says about the PC.

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{HKEY, HKEY_CLASSES_ROOT, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::core::{HSTRING, PCWSTR};

/// Must match `CLSID_DShowSoftcam` in vcam/src/softcam/softcam.cpp.
pub const VCAM_CLSID: &str = "{518D4949-2206-4A92-B74A-6CF1C7348605}";

fn reg_string(root: HKEY, key: &str, value: Option<&str>) -> Option<String> {
    let key = HSTRING::from(key);
    let value = value.map(HSTRING::from);
    let value_ptr = value.as_ref().map_or(PCWSTR::null(), |v| PCWSTR(v.as_ptr()));
    let mut buf = [0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    let rc = unsafe { RegGetValueW(root, &key, value_ptr, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut len)) };
    if rc != ERROR_SUCCESS {
        return None;
    }
    let chars = (len as usize / 2).saturating_sub(1); // without the trailing NUL
    Some(String::from_utf16_lossy(&buf[..chars]))
}

/// Path of the registered 64-bit "Plugcam Camera" DLL, if it is registered and present.
pub fn vcam_registered_path() -> Option<String> {
    let path = reg_string(HKEY_CLASSES_ROOT, &format!(r"CLSID\{VCAM_CLSID}\InprocServer32"), None)?;
    std::path::Path::new(&path).is_file().then_some(path)
}

/// Windows build number (22000+ is Windows 11), 0 if unknown.
pub fn windows_build() -> u32 {
    reg_string(HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", Some("CurrentBuildNumber"))
        .and_then(|b| b.parse().ok())
        .unwrap_or(0)
}

/// e.g. `Windows 11 24H2 (build 26100)`.
pub fn windows_name() -> String {
    let build = windows_build();
    let release = reg_string(HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", Some("DisplayVersion"));
    let major = if build >= 22000 { 11 } else { 10 };
    match release {
        Some(r) => format!("Windows {major} {r} (build {build})"),
        None => format!("Windows {major} (build {build})"),
    }
}

/// Names of the graphics adapters, e.g. `NVIDIA GeForce RTX 3060`; software ones left out.
pub fn gpu_names() -> Vec<String> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1};
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else { return Vec::new() };
    let mut names = Vec::new();
    for i in 0.. {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(i) }) else { break };
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else { continue };
        if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
            continue;
        }
        let len = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
        let name = String::from_utf16_lossy(&desc.Description[..len]);
        // DXGI can list one GPU more than once, e.g. a laptop's with the display on another.
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// The user's Downloads folder.
pub fn downloads_dir() -> Option<std::path::PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Downloads, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None).ok()?;
        let result = path.to_string().ok().map(std::path::PathBuf::from);
        CoTaskMemFree(Some(path.0 as *const _));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_windows_build() {
        assert!(windows_build() >= 10240, "Windows 10 or later");
        assert!(windows_name().starts_with("Windows 1"));
    }

    #[test]
    fn finds_downloads() {
        assert!(downloads_dir().is_some_and(|d| d.is_dir()));
    }
}
