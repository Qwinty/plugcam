//! Small Windows facts the app needs: is the camera DLL registered, which Windows build.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_windows_build() {
        assert!(windows_build() >= 10240, "Windows 10 or later");
    }
}
