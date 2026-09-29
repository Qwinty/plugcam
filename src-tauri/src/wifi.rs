//! Phones over Wi-Fi: the pairing QR code, finding phones through mDNS, reconnecting a phone
//! that dropped off the network and moving a cable-connected phone to Wi-Fi.
//!
//! All of it is adb's own wireless debugging; nothing is installed on the phone.

use std::time::{Duration, Instant};

use crate::adb::{Adb, MdnsKind};

/// Port for `adb tcpip`, the one Android Studio and scrcpy use.
pub const TCPIP_PORT: u16 = 5555;
/// mDNS serials end with this: `adb-<serial>-<id>._adb-tls-connect._tcp`.
const MDNS_CONNECT_SUFFIX: &str = "._adb-tls-connect._tcp";
/// adb reconnects a paired phone by itself once it sees it on mDNS again; this is how long
/// to give it before connecting by address.
const AUTO_RECONNECT_WAIT: Duration = Duration::from_secs(3);
/// `adb tcpip` restarts adbd on the phone; it listens again after about a second.
const TCPIP_RESTART_WAIT: Duration = Duration::from_secs(1);
const TCPIP_CONNECT_TRIES: u32 = 6;
/// A phone on the same network answers `echo` in well under a second.
const ALIVE_TIMEOUT: Duration = Duration::from_secs(4);
/// For `adb connect`, `disconnect` and `mdns services` while reconnecting, so stopping the
/// camera is not held up by a phone that is gone.
pub const QUICK_TIMEOUT: Duration = Duration::from_secs(5);
/// Characters that are safe in the `WIFI:` QR payload without escaping.
const QR_ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";

/// The phone's mDNS name inside an adb serial, if the serial is an mDNS one. adb appends
/// " (2)", " (3)"… when a stale transport still holds the name; that is dropped, so the name
/// stays the same phone's and matches `adb mdns services`.
pub fn mdns_name(serial: &str) -> Option<&str> {
    let name = serial.strip_suffix(MDNS_CONNECT_SUFFIX)?;
    let base = name
        .strip_suffix(')')
        .and_then(|s| s.rsplit_once(" ("))
        .filter(|(_, n)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        .map(|(base, _)| base);
    Some(base.unwrap_or(name))
}

/// The adb serial of a phone adb connected by its mDNS name.
pub fn mdns_serial(name: &str) -> String {
    format!("{name}{MDNS_CONNECT_SUFFIX}")
}

/// What the QR code on the PC tells the phone: which mDNS name to announce while pairing and
/// the password to pair with. Same format as Android Studio's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrPairing {
    pub name: String,
    pub password: String,
}

impl QrPairing {
    pub fn new() -> Self {
        Self { name: format!("plugcam-{}", random_text(8)), password: random_text(10) }
    }

    pub fn payload(&self) -> String {
        format!("WIFI:T:ADB;S:{};P:{};;", self.name, self.password)
    }

    /// The QR code as an SVG image drawn with `currentColor` on a transparent background.
    pub fn svg(&self) -> String {
        let code = qrcode::QrCode::new(self.payload().as_bytes()).expect("short payload always fits");
        let width = code.width();
        let quiet = 2;
        let size = width + 2 * quiet;
        let mut path = String::new();
        for (i, dark) in code.to_colors().iter().enumerate() {
            if *dark == qrcode::Color::Dark {
                let (x, y) = (i % width + quiet, i / width + quiet);
                path.push_str(&format!("M{x} {y}h1v1h-1z"));
            }
        }
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {size} {size}" shape-rendering="crispEdges"><path fill="currentColor" d="{path}"/></svg>"#
        )
    }
}

impl Default for QrPairing {
    fn default() -> Self {
        Self::new()
    }
}

/// From the OS random generator. The alphabet has 32 characters, so `byte % 32` is unbiased.
fn random_text(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    getrandom::fill(&mut bytes).expect("the OS random generator works");
    bytes.iter().map(|b| QR_ALPHABET[*b as usize % QR_ALPHABET.len()] as char).collect()
}

/// Where to `adb connect` a phone we knew under `serial`: `ip:port` serials are the address;
/// mDNS ones are looked up again, since the phone picks a new port whenever wireless debugging
/// restarts.
pub fn current_address(adb: &Adb, serial: &str) -> Option<String> {
    match mdns_name(serial) {
        None => serial.contains(':').then(|| serial.to_string()),
        Some(name) => adb
            .mdns_services_within(QUICK_TIMEOUT)
            .ok()?
            .into_iter()
            .find(|s| s.kind == MdnsKind::Connect && s.name == name)
            .map(|s| s.address),
    }
}

/// Drops a Wi-Fi phone's adb connection and connects again, so a link that went stale on a
/// network hiccup is replaced by a fresh one. Returns the phone's serial once it is back, which
/// may differ from the old one: an mDNS phone connected by address shows up as `ip:port`.
/// Gives up between steps once `go_on` says so (the camera was stopped, the phone forgotten).
pub fn reconnect(adb: &Adb, serial: &str, go_on: &impl Fn() -> bool) -> Option<String> {
    let _ = adb.disconnect_within(serial, QUICK_TIMEOUT);
    if mdns_name(serial).is_some() {
        let until = Instant::now() + AUTO_RECONNECT_WAIT;
        while Instant::now() < until {
            if !go_on() {
                return None;
            }
            if is_ready(adb, serial) {
                return Some(serial.to_string());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }
    let address = current_address(adb, serial)?;
    if !go_on() {
        return None;
    }
    match adb.connect_within(&address, QUICK_TIMEOUT) {
        Ok(()) => Some(address),
        Err(e) => {
            log::info!("reconnecting {serial}: {e}");
            None
        }
    }
}

/// Whether adb still reaches the phone. A Wi-Fi link that died quietly stays listed as
/// `device` for a while, but commands on it hang.
pub fn is_alive(adb: &Adb, serial: &str) -> bool {
    adb.output(Some(serial), &["shell", "echo", "ok"], ALIVE_TIMEOUT).is_ok_and(|o| o.success && o.stdout.trim() == "ok")
}

fn is_ready(adb: &Adb, serial: &str) -> bool {
    adb.devices().is_ok_and(|list| list.iter().any(|d| d.serial == serial && d.is_ready()))
}

#[derive(Debug, thiserror::Error)]
pub enum SwitchError {
    /// The phone has no Wi-Fi address: Wi-Fi is off or it is on mobile data only.
    #[error("the phone is not on Wi-Fi")]
    NoWifi,
    #[error(transparent)]
    Adb(#[from] crate::adb::AdbError),
}

/// "From cable to Wi-Fi": restarts adb on a cable-connected phone in TCP mode and connects to
/// it over the network. Returns the new serial (`ip:5555`). The link is not encrypted and
/// lasts until the phone restarts.
pub fn switch_from_cable(adb: &Adb, serial: &str) -> Result<String, SwitchError> {
    let ip = adb.wifi_ip(serial).ok_or(SwitchError::NoWifi)?;
    adb.tcpip(serial, TCPIP_PORT)?;
    std::thread::sleep(TCPIP_RESTART_WAIT);
    let address = format!("{ip}:{TCPIP_PORT}");
    let mut last = None;
    for _ in 0..TCPIP_CONNECT_TRIES {
        match adb.connect(&address) {
            Ok(()) => return Ok(address),
            Err(e) => last = Some(e),
        }
        std::thread::sleep(Duration::from_millis(700));
    }
    Err(last.expect("at least one try").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mdns_serials() {
        assert_eq!(mdns_name("adb-834ee2d7-Xy1AbC._adb-tls-connect._tcp"), Some("adb-834ee2d7-Xy1AbC"));
        assert_eq!(mdns_name("adb-834ee2d7-Xy1AbC (2)._adb-tls-connect._tcp"), Some("adb-834ee2d7-Xy1AbC"));
        assert_eq!(mdns_name("adb-834ee2d7-Xy1AbC (12)._adb-tls-connect._tcp"), Some("adb-834ee2d7-Xy1AbC"));
        assert_eq!(mdns_name("adb-834ee2d7-Xy1AbC (x)._adb-tls-connect._tcp"), Some("adb-834ee2d7-Xy1AbC (x)"));
        assert_eq!(mdns_name("192.168.1.20:5555"), None);
        assert_eq!(mdns_name("834ee2d7"), None);
    }

    #[test]
    fn qr_payload_is_android_studio_format() {
        let q = QrPairing { name: "plugcam-abcd2345".into(), password: "pass23word".into() };
        assert_eq!(q.payload(), "WIFI:T:ADB;S:plugcam-abcd2345;P:pass23word;;");
        let svg = q.svg();
        assert!(svg.starts_with("<svg") && svg.contains("currentColor") && svg.ends_with("</svg>"));
    }

    #[test]
    fn random_names_differ_and_are_safe() {
        let (a, b) = (QrPairing::new(), QrPairing::new());
        assert_ne!(a, b);
        assert!(a.name.starts_with("plugcam-") && a.name.len() == 16);
        assert!(a.password.bytes().all(|c| QR_ALPHABET.contains(&c)));
        assert_eq!(QR_ALPHABET.len(), 32);
    }

    #[test]
    fn mdns_serial_round_trip() {
        assert_eq!(mdns_name(&mdns_serial("adb-834ee2d7-Xy1AbC")), Some("adb-834ee2d7-Xy1AbC"));
    }
}
