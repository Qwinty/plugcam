//! Thin wrapper around adb.exe: device list, push, tunnels and shell commands.

use std::io;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::resources;

/// Keeps adb from flashing a console window when called from the GUI app.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, thiserror::Error)]
pub enum AdbError {
    #[error("adb.exe not found (bundled, PLUGCAM_ADB or PATH)")]
    NotFound,
    #[error("cannot run adb: {0}")]
    Io(#[from] io::Error),
    #[error("adb {args} failed: {output}")]
    Failed { args: String, output: String },
}

#[derive(Debug, Clone)]
pub struct Adb {
    exe: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub serial: String,
    /// `device` when ready; also `unauthorized`, `offline`, `no permissions`…
    pub state: String,
    pub model: Option<String>,
}

impl Device {
    pub fn is_ready(&self) -> bool {
        self.state == "device"
    }

    /// Wi-Fi devices are `ip:port` or mDNS `adb-…._adb-tls-connect._tcp`.
    pub fn is_wifi(&self) -> bool {
        self.serial.contains(':') || self.serial.contains("._adb-tls-connect.")
    }
}

impl Adb {
    /// Bundled adb next to the app first, then `PLUGCAM_ADB`, then whatever is on PATH.
    pub fn locate() -> Result<Self, AdbError> {
        if let Some(exe) = resources::find("adb.exe", "PLUGCAM_ADB") {
            return Ok(Self { exe });
        }
        let on_path = Command::new("adb")
            .arg("version")
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .output()
            .is_ok_and(|o| o.status.success());
        if on_path { Ok(Self { exe: "adb".into() }) } else { Err(AdbError::NotFound) }
    }

    pub fn exe(&self) -> &Path {
        &self.exe
    }

    /// `adb [-s serial]` with no console window and stdin closed.
    pub fn command(&self, serial: Option<&str>) -> Command {
        let mut cmd = Command::new(&self.exe);
        cmd.creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null());
        if let Some(serial) = serial {
            cmd.args(["-s", serial]);
        }
        cmd
    }

    /// Runs adb to completion; a non-zero exit becomes `AdbError::Failed` with its output.
    pub fn run(&self, serial: Option<&str>, args: &[&str]) -> Result<String, AdbError> {
        let out = self.command(serial).args(args).output()?;
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        if out.status.success() {
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(AdbError::Failed {
                args: args.join(" "),
                output: format!("{}{}", stderr.trim(), stdout.trim()),
            })
        }
    }

    pub fn devices(&self) -> Result<Vec<Device>, AdbError> {
        Ok(parse_devices(&self.run(None, &["devices", "-l"])?))
    }

    pub fn push(&self, serial: &str, local: &Path, remote: &str) -> Result<(), AdbError> {
        self.run(Some(serial), &["push", &local.to_string_lossy(), remote]).map(drop)
    }

    pub fn reverse(&self, serial: &str, remote: &str, local: &str) -> Result<(), AdbError> {
        self.run(Some(serial), &["reverse", remote, local]).map(drop)
    }

    pub fn reverse_remove(&self, serial: &str, remote: &str) -> Result<(), AdbError> {
        self.run(Some(serial), &["reverse", "--remove", remote]).map(drop)
    }

    pub fn forward(&self, serial: &str, local: &str, remote: &str) -> Result<(), AdbError> {
        self.run(Some(serial), &["forward", local, remote]).map(drop)
    }

    pub fn forward_remove(&self, serial: &str, local: &str) -> Result<(), AdbError> {
        self.run(Some(serial), &["forward", "--remove", local]).map(drop)
    }

    /// Runs a device shell command; fails if it exits non-zero.
    pub fn shell(&self, serial: &str, command: &str) -> Result<String, AdbError> {
        self.run(Some(serial), &["shell", command])
    }
}

/// Parses `adb devices -l`.
pub fn parse_devices(output: &str) -> Vec<Device> {
    output
        .lines()
        .skip_while(|l| !l.starts_with("List of devices attached"))
        .skip(1)
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let serial = words.next()?.to_string();
            // "no permissions (...)" has spaces; the state is everything up to the first key:value.
            let mut state = Vec::new();
            let mut model = None;
            for w in words {
                if let Some(m) = w.strip_prefix("model:") {
                    model = Some(m.replace('_', " "));
                } else if !w.contains(':') && model.is_none() && state.len() < 8 {
                    state.push(w);
                }
            }
            Some(Device { serial, state: state.join(" "), model })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_device_list() {
        let out = "* daemon started successfully\n\
            List of devices attached\n\
            834ee2d7               device product:PHK110 model:PHK110 device:OP5913L1 transport_id:4\n\
            192.168.1.20:5555      device product:x model:Pixel_8 device:shiba transport_id:7\n\
            R58N1234               unauthorized usb:1-1 transport_id:9\n\
            \n";
        let devices = parse_devices(out);
        assert_eq!(devices.len(), 3);
        assert_eq!(devices[0].serial, "834ee2d7");
        assert!(devices[0].is_ready() && !devices[0].is_wifi());
        assert_eq!(devices[0].model.as_deref(), Some("PHK110"));
        assert_eq!(devices[1].model.as_deref(), Some("Pixel 8"));
        assert!(devices[1].is_wifi());
        assert_eq!(devices[2].state, "unauthorized");
        assert!(!devices[2].is_ready());
    }

    #[test]
    fn empty_list() {
        assert!(parse_devices("List of devices attached\n\n").is_empty());
    }
}
