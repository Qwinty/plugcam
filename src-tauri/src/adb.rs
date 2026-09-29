//! Thin wrapper around adb.exe: device list, push, tunnels, shell commands and the Wi-Fi
//! side (pairing, connecting, mDNS discovery).

use std::io::{self, Read};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::resources;

/// Keeps adb from flashing a console window when called from the GUI app.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Longer than any normal command takes, even pushing the server over slow Wi-Fi.
const RUN_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum AdbError {
    #[error("adb.exe not found (bundled, PLUGCAM_ADB or PATH)")]
    NotFound,
    #[error("cannot run adb: {0}")]
    Io(#[from] io::Error),
    #[error("adb {args} failed: {output}")]
    Failed { args: String, output: String },
    #[error("adb {args} did not finish within {after:?}")]
    Timeout { args: String, after: Duration },
}

pub struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
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
        self.run_within(serial, args, RUN_TIMEOUT)
    }

    /// `run` with its own time limit.
    pub fn run_within(&self, serial: Option<&str>, args: &[&str], timeout: Duration) -> Result<String, AdbError> {
        let out = self.output(serial, args, timeout)?;
        if out.success {
            Ok(out.stdout)
        } else {
            Err(AdbError::Failed {
                args: args.join(" "),
                output: format!("{}{}", out.stderr.trim(), out.stdout.trim()),
            })
        }
    }

    /// Runs adb and collects its output, killing it after `timeout`: a command for a Wi-Fi
    /// phone that has just left the network can otherwise wait for minutes.
    pub fn output(&self, serial: Option<&str>, args: &[&str], timeout: Duration) -> Result<Output, AdbError> {
        let mut child = self.command(serial).args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        let read_all = |pipe: Option<Box<dyn Read + Send>>| {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                if let Some(mut p) = pipe {
                    let _ = p.read_to_end(&mut buf);
                }
                String::from_utf8_lossy(&buf).into_owned()
            })
        };
        let stdout = read_all(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
        let stderr = read_all(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AdbError::Timeout { args: args.join(" "), after: timeout });
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        Ok(Output {
            success: status.success(),
            stdout: stdout.join().unwrap_or_default(),
            stderr: stderr.join().unwrap_or_default(),
        })
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

    /// `adb connect host:port`. Older adb versions exit 0 even when it fails, so the output
    /// decides.
    pub fn connect(&self, address: &str) -> Result<(), AdbError> {
        self.connect_within(address, RUN_TIMEOUT)
    }

    /// `connect` that gives up after `timeout`; a phone that left the network can keep adb
    /// trying for a long time.
    pub fn connect_within(&self, address: &str, timeout: Duration) -> Result<(), AdbError> {
        let out = self.run_within(None, &["connect", address], timeout)?;
        if out.contains("connected to") && !out.contains("failed") && !out.contains("cannot") {
            Ok(())
        } else {
            Err(AdbError::Failed { args: format!("connect {address}"), output: out.trim().to_string() })
        }
    }

    pub fn disconnect(&self, serial: &str) -> Result<(), AdbError> {
        self.disconnect_within(serial, RUN_TIMEOUT)
    }

    pub fn disconnect_within(&self, serial: &str, timeout: Duration) -> Result<(), AdbError> {
        self.run_within(None, &["disconnect", serial], timeout).map(drop)
    }

    /// Pairs with a phone in "Pair device" mode (Android 11+). Returns the phone's mDNS name
    /// (`adb-<serial>-<id>`), which stays the same across networks and restarts.
    pub fn pair(&self, address: &str, code: &str) -> Result<Option<String>, AdbError> {
        self.pair_within(address, code, RUN_TIMEOUT)
    }

    pub fn pair_within(&self, address: &str, code: &str, timeout: Duration) -> Result<Option<String>, AdbError> {
        let fail = |output: String| AdbError::Failed { args: format!("pair {address}"), output };
        let out = self.run_within(None, &["pair", address, code], timeout).map_err(|e| match e {
            AdbError::Failed { output, .. } => fail(output),
            e => e,
        })?;
        if !out.contains("Successfully paired") {
            return Err(fail(out.trim().to_string()));
        }
        Ok(out.split_once("[guid=").and_then(|(_, g)| g.split(']').next()).map(str::to_string))
    }

    /// Phones announcing wireless debugging on the local network.
    pub fn mdns_services(&self) -> Result<Vec<MdnsService>, AdbError> {
        self.mdns_services_within(RUN_TIMEOUT)
    }

    pub fn mdns_services_within(&self, timeout: Duration) -> Result<Vec<MdnsService>, AdbError> {
        Ok(parse_mdns_services(&self.run_within(None, &["mdns", "services"], timeout)?))
    }

    /// Switches a phone connected by cable to plain TCP adb on `port` (lost on reboot).
    pub fn tcpip(&self, serial: &str, port: u16) -> Result<(), AdbError> {
        self.run(Some(serial), &["tcpip", &port.to_string()]).map(drop)
    }

    /// The phone's IPv4 address on Wi-Fi, if it is on Wi-Fi.
    pub fn wifi_ip(&self, serial: &str) -> Option<String> {
        let addr = self.shell(serial, "ip -f inet addr show wlan0").ok().and_then(|o| parse_inet(&o));
        addr.or_else(|| self.shell(serial, "ip route").ok().and_then(|o| parse_route_src(&o)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MdnsKind {
    /// `_adb-tls-pairing._tcp`: the phone shows a pairing code or scanned a QR code.
    Pairing,
    /// `_adb-tls-connect._tcp`: a phone with wireless debugging on, ready for `adb connect`.
    Connect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdnsService {
    /// `adb-<serial>-<id>` for phones, or the name in our QR code while pairing by QR.
    pub name: String,
    pub kind: MdnsKind,
    /// `ip:port`.
    pub address: String,
}

impl MdnsService {
    pub fn ip(&self) -> &str {
        self.address.rsplit_once(':').map_or(&self.address, |(ip, _)| ip)
    }
}

/// Parses `adb mdns services`: `name<TAB>type<TAB>ip:port` per line.
pub fn parse_mdns_services(output: &str) -> Vec<MdnsService> {
    output
        .lines()
        .filter_map(|line| {
            // Names may contain spaces; the type and the address never do.
            let (rest, address) = line.trim_end().rsplit_once(char::is_whitespace)?;
            let (name, kind) = rest.trim_end().rsplit_once(char::is_whitespace)?;
            let kind = match kind.trim_end_matches('.') {
                "_adb-tls-pairing._tcp" => MdnsKind::Pairing,
                "_adb-tls-connect._tcp" => MdnsKind::Connect,
                _ => return None,
            };
            let name = name.trim();
            (!name.is_empty() && address.contains(':')).then(|| MdnsService { name: name.into(), kind, address: address.into() })
        })
        .collect()
}

/// `inet 192.168.1.20/24 brd …` → `192.168.1.20`.
fn parse_inet(output: &str) -> Option<String> {
    output.split_whitespace().skip_while(|w| *w != "inet").nth(1).and_then(|a| a.split('/').next()).map(str::to_string)
}

/// `192.168.1.0/24 dev wlan0 proto kernel scope link src 192.168.1.20` → `192.168.1.20`.
fn parse_route_src(output: &str) -> Option<String> {
    let line = output.lines().find(|l| l.contains("wlan"))?;
    line.split_whitespace().skip_while(|w| *w != "src").nth(1).map(str::to_string)
}

/// The phone to use: the preferred one if it is ready, else the first ready one on USB, else
/// the first ready one on Wi-Fi.
pub fn choose<'a>(devices: impl IntoIterator<Item = &'a Device>, preferred: Option<&str>) -> Option<&'a Device> {
    let ready: Vec<&Device> = devices.into_iter().filter(|d| d.is_ready()).collect();
    preferred
        .and_then(|p| ready.iter().find(|d| d.serial == p))
        .or_else(|| ready.iter().find(|d| !d.is_wifi()))
        .or(ready.first())
        .copied()
}

/// States `adb devices` prints after the serial. mDNS serials can contain spaces
/// (`adb-R58N-AbCd (2)._adb-tls-connect._tcp`), so the state word is what ends the serial.
const STATES: [&str; 11] = [
    "device", "offline", "unauthorized", "authorizing", "connecting", "bootloader", "recovery",
    "sideload", "rescue", "host", "unknown",
];

/// Parses `adb devices -l`.
pub fn parse_devices(output: &str) -> Vec<Device> {
    output
        .lines()
        .skip_while(|l| !l.starts_with("List of devices attached"))
        .skip(1)
        .filter_map(parse_device_line)
        .collect()
}

fn parse_device_line(line: &str) -> Option<Device> {
    let words = words_with_offsets(line);
    // The first word is always part of the serial, even if it happens to be "device".
    let at = (1..words.len()).find(|&i| {
        let w = words[i].1;
        STATES.contains(&w) || (w == "no" && words.get(i + 1).is_some_and(|n| n.1 == "permissions"))
    })?;
    let serial = line[..words[at].0].trim().to_string();
    // "no permissions (...)" has spaces; the state is everything up to the first key:value.
    let mut state = Vec::new();
    let mut model = None;
    for &(_, w) in &words[at..] {
        if let Some(m) = w.strip_prefix("model:") {
            model = Some(m.replace('_', " "));
        } else if !is_property(w) && model.is_none() && state.len() < 8 {
            state.push(w);
        }
    }
    Some(Device { serial, state: state.join(" "), model })
}

/// `usb:1-1`, `product:x`, `transport_id:7`…
fn is_property(word: &str) -> bool {
    word.split_once(':').is_some_and(|(k, _)| !k.is_empty() && k.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
}

fn words_with_offsets(line: &str) -> Vec<(usize, &str)> {
    let mut words = Vec::new();
    let mut start = None;
    for (i, c) in line.char_indices() {
        match (c.is_whitespace(), start) {
            (true, Some(s)) => {
                words.push((s, &line[s..i]));
                start = None;
            }
            (false, None) => start = Some(i),
            _ => {}
        }
    }
    if let Some(s) = start {
        words.push((s, &line[s..]));
    }
    words
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
    fn mdns_serials_with_spaces() {
        let out = "List of devices attached
            adb-834ee2d7-Xy1AbC (2)._adb-tls-connect._tcp device product:PHK110 model:PHK110 device:OP5913L1 transport_id:3
            adb-834ee2d7-Xy1AbC._adb-tls-connect._tcp offline transport_id:4
            device                 device product:p model:Odd_Serial transport_id:5
            ABC123                 no permissions (missing udev rules? user is in the plugdev group); see [http://x] usb:1-2 transport_id:6
";
        let devices = parse_devices(out);
        assert_eq!(devices.len(), 4);
        assert_eq!(devices[0].serial, "adb-834ee2d7-Xy1AbC (2)._adb-tls-connect._tcp");
        assert!(devices[0].is_ready() && devices[0].is_wifi());
        assert_eq!(devices[0].model.as_deref(), Some("PHK110"));
        assert_eq!(devices[1].serial, "adb-834ee2d7-Xy1AbC._adb-tls-connect._tcp");
        assert_eq!(devices[1].state, "offline");
        assert_eq!(devices[2].serial, "device");
        assert!(devices[2].is_ready());
        assert_eq!(devices[3].serial, "ABC123");
        assert!(devices[3].state.starts_with("no permissions"));
        assert!(!devices[3].is_ready());
    }

    #[test]
    fn mdns_list() {
        let out = "List of discovered mdns services\n\
            adb-834ee2d7-Xy1AbC\t_adb-tls-connect._tcp\t192.168.1.20:37899\n\
            plugcam-k3j9\t_adb-tls-pairing._tcp.\t192.168.1.20:41234\n\
            adb-834ee2d7-Xy1AbC (2)\t_adb-tls-connect._tcp\t192.168.1.21:5555\n\
            something\t_other._tcp\t1.2.3.4:1\n";
        let s = parse_mdns_services(out);
        assert_eq!(s.len(), 3);
        assert_eq!(
            s[0],
            MdnsService { name: "adb-834ee2d7-Xy1AbC".into(), kind: MdnsKind::Connect, address: "192.168.1.20:37899".into() }
        );
        assert_eq!(s[1].kind, MdnsKind::Pairing);
        assert_eq!(s[1].name, "plugcam-k3j9");
        assert_eq!(s[1].ip(), "192.168.1.20");
        assert_eq!(s[2].name, "adb-834ee2d7-Xy1AbC (2)");
        assert!(parse_mdns_services("List of discovered mdns services\n").is_empty());
    }

    #[test]
    fn phone_ip() {
        let addr = "37: wlan0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500\n    inet 192.168.1.20/24 brd 192.168.1.255 scope global wlan0\n";
        assert_eq!(parse_inet(addr).as_deref(), Some("192.168.1.20"));
        assert_eq!(parse_inet(""), None);
        let route = "10.0.0.0/8 dev rmnet0 src 10.1.2.3\n192.168.1.0/24 dev wlan0 proto kernel scope link src 192.168.1.20\n";
        assert_eq!(parse_route_src(route).as_deref(), Some("192.168.1.20"));
    }

    #[test]
    fn choose_prefers_the_chosen_ready_phone() {
        let d = |serial: &str, state: &str| Device { serial: serial.into(), state: state.into(), model: None };
        let list = [d("1.2.3.4:5555", "device"), d("abc", "unauthorized"), d("usb1", "device")];
        assert_eq!(choose(&list, None).unwrap().serial, "usb1");
        assert_eq!(choose(&list, Some("1.2.3.4:5555")).unwrap().serial, "1.2.3.4:5555");
        assert_eq!(choose(&list, Some("abc")).unwrap().serial, "usb1");
        assert_eq!(choose(&list, Some("gone")).unwrap().serial, "usb1");
        assert!(choose(&list[1..2], None).is_none());
    }

    #[test]
    fn empty_list() {
        assert!(parse_devices("List of devices attached\n\n").is_empty());
    }
}
