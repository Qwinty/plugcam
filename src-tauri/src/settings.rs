//! User settings, stored as JSON in the app's config folder. Missing or unknown fields fall
//! back to defaults, so older and newer files both load.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::frame::ColorAdjust;
use crate::scrcpy::server::{CameraParams, Facing};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub onboarding_done: bool,
    /// `back` or `front`.
    pub facing: String,
    /// A specific lens; `None` = the phone's default camera for `facing`.
    pub camera_id: Option<String>,
    /// 30 or 60; a lens that cannot do 60 runs at 30 (see `cameras::capture_mode`).
    pub fps: u32,
    /// Let 60 fps come from a high-speed session on phones without a regular 60: smoother
    /// but with worse colors and noise, so off unless the user turns it on.
    pub allow_high_speed: bool,
    /// Presets of 0.1.x (`economy`, `standard`, `smooth`), read once and turned into the
    /// fields above by `sanitized`.
    #[serde(skip_serializing)]
    pub quality: Option<String>,
    pub mirror: bool,
    /// Clockwise degrees of the picture: 0, 90, 180 or 270.
    pub rotation: u16,
    /// Picture adjustments made on the PC.
    pub color: ColorAdjust,
    /// `None` = automatic, by resolution and fps.
    pub bitrate_mbps: Option<u32>,
    /// The resolution picked in the window: the virtual camera's size, and the size the phone
    /// captures in when its lens offers it.
    pub vcam_width: u32,
    pub vcam_height: u32,
    pub launch_at_login: bool,
    pub close_to_tray: bool,
    pub auto_start_camera: bool,
    /// UI language code (see `app::i18n`); `None` = same as Windows.
    pub language: Option<String>,
    /// Lenses that sent no picture, by phone model; hidden from the lens picker.
    pub broken_cameras: BTreeMap<String, Vec<String>>,
    /// Look for a new version at start and once a day.
    pub check_updates: bool,
    /// The version whose "What's new" was last shown; `None` on a fresh install.
    pub last_seen_version: Option<String>,
    /// The phone picked in the app: a USB phone's serial, or a Wi-Fi phone's id (see
    /// `WifiPhone::id`), which stays the same when its address changes. `None` = the first
    /// ready one, USB first.
    pub phone: Option<String>,
    /// Phones used over Wi-Fi, connected again whenever they are on the network.
    #[serde(deserialize_with = "skip_bad_entries")]
    pub wifi_phones: Vec<WifiPhone>,
    /// Ids of Wi-Fi phones the user forgot. They stay paired, so adb may connect them by
    /// itself; the app then leaves them alone until they are picked or added again.
    #[serde(deserialize_with = "skip_bad_entries")]
    pub forgotten_wifi: Vec<String>,
    /// Write a detailed log to a file for bug reports (see `crate::diag`).
    pub detailed_log: bool,
}

/// A list where entries that do not parse are dropped, rather than the whole settings file.
fn skip_bad_entries<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let items = match value {
        serde_json::Value::Array(items) => items,
        _ => Vec::new(),
    };
    Ok(items.into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect())
}

/// A phone remembered for Wi-Fi.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiPhone {
    /// Its mDNS name (`adb-<serial>-<id>`) when paired, which survives new addresses; `ip:port`
    /// when it was switched over from the cable.
    pub id: String,
    /// Human name, e.g. `OnePlus PHK110`, once known.
    pub name: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            onboarding_done: false,
            facing: "back".into(),
            camera_id: None,
            fps: 30,
            allow_high_speed: false,
            quality: None,
            mirror: false,
            rotation: 0,
            color: ColorAdjust::default(),
            bitrate_mbps: None,
            vcam_width: 1920,
            vcam_height: 1080,
            launch_at_login: false,
            close_to_tray: true,
            auto_start_camera: false,
            language: None,
            broken_cameras: BTreeMap::new(),
            check_updates: true,
            last_seen_version: None,
            phone: None,
            wifi_phones: Vec::new(),
            forgotten_wifi: Vec::new(),
            detailed_log: false,
        }
    }
}

pub const VCAM_SIZES: [(u32, u32); 4] = [(1280, 720), (1920, 1080), (2560, 1440), (3840, 2160)];

impl Settings {
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str::<Settings>(&text)
                .map(Settings::sanitized)
                .unwrap_or_else(|e| {
                    log::warn!("{}: {e}; using defaults", path.display());
                    Settings::default()
                }),
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Write then rename, so a crash never leaves half a file.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).expect("settings serialize"))?;
        std::fs::rename(tmp, path)
    }

    /// Replaces out-of-range values with defaults.
    pub fn sanitized(mut self) -> Self {
        let d = Settings::default();
        if !matches!(self.facing.as_str(), "back" | "front") {
            self.facing = d.facing;
        }
        match self.quality.take().as_deref() {
            Some("smooth") => self.fps = 60,
            // Saver captured 720p; the camera size stays if it was changed from the default.
            Some("economy") if (self.vcam_width, self.vcam_height) == (d.vcam_width, d.vcam_height) => {
                (self.vcam_width, self.vcam_height) = (1280, 720)
            }
            _ => {}
        }
        if !matches!(self.fps, 30 | 60) {
            self.fps = d.fps;
        }
        if !matches!(self.rotation, 0 | 90 | 180 | 270) {
            self.rotation = 0;
        }
        if !VCAM_SIZES.contains(&(self.vcam_width, self.vcam_height)) {
            (self.vcam_width, self.vcam_height) = (d.vcam_width, d.vcam_height);
        }
        self.bitrate_mbps = self.bitrate_mbps.map(|b| b.clamp(4, 40));
        self.color = self.color.clamped();
        if self.language.as_deref().is_some_and(|l| !crate::app::i18n::SUPPORTED_LANGUAGES.contains(&l)) {
            self.language = None;
        }
        let mut seen = std::collections::HashSet::new();
        self.wifi_phones.retain(|p| !p.id.is_empty() && seen.insert(p.id.clone()));
        let mut seen = std::collections::HashSet::new();
        self.forgotten_wifi.retain(|id| !id.is_empty() && seen.insert(id.clone()));
        self
    }

    /// Drops a Wi-Fi phone and remembers not to use it.
    pub fn forget_wifi_phone(&mut self, id: &str) {
        self.wifi_phones.retain(|p| p.id != id);
        if !self.forgotten_wifi.iter().any(|i| i == id) {
            self.forgotten_wifi.push(id.to_string());
        }
    }

    /// Adds a Wi-Fi phone, or refreshes its name if it is known.
    pub fn remember_wifi_phone(&mut self, id: &str, name: Option<String>) {
        self.forgotten_wifi.retain(|i| i != id);
        match self.wifi_phones.iter_mut().find(|p| p.id == id) {
            Some(p) => p.name = name.or(p.name.take()),
            None => self.wifi_phones.push(WifiPhone { id: id.to_string(), name }),
        }
    }

    /// Bit rate for the phone's encoder: the user's choice, or one that suits the resolution
    /// and frame rate (a third more for 60 fps).
    pub fn bitrate(&self) -> u32 {
        let auto = || {
            let mbps = match self.vcam_height {
                ..=720 => 6,
                ..=1080 => 12,
                ..=1440 => 20,
                _ => 32,
            };
            if self.fps >= 60 { mbps * 4 / 3 } else { mbps }
        };
        self.bitrate_mbps.unwrap_or_else(auto) * 1_000_000
    }

    /// Server parameters without the capture mode (size, fps), which depends on the camera.
    pub fn camera_params(&self) -> CameraParams {
        CameraParams {
            camera_id: self.camera_id.clone(),
            facing: Some(if self.facing == "front" { Facing::Front } else { Facing::Back }),
            size: None,
            fps: 30,
            high_speed: false,
            bit_rate: Some(self.bitrate()),
            orientation: 0, // rotation is done on the PC, see `PipelineConfig::rotation`
            torch: false,
            zoom: None,
            key_frame_interval: None,
        }
    }

    pub fn is_broken(&self, model: &str, camera_id: &str) -> bool {
        self.broken_cameras.get(model).is_some_and(|ids| ids.iter().any(|i| i == camera_id))
    }

    pub fn mark_broken(&mut self, model: &str, camera_id: &str) {
        let ids = self.broken_cameras.entry(model.to_string()).or_default();
        if !ids.iter().any(|i| i == camera_id) {
            ids.push(camera_id.to_string());
        }
    }
}

/// `%APPDATA%\Plugcam\settings.json` unless the app passes its own folder.
pub fn default_path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("Plugcam").join("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_partial_files() {
        let dir = std::env::temp_dir().join(format!("plugcam-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        let mut s = Settings { mirror: true, fps: 60, allow_high_speed: true, ..Settings::default() };
        s.mark_broken("PHK110", "4");
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        assert!(Settings::load(&path).is_broken("PHK110", "4"));

        std::fs::write(&path, r#"{"mirror": true, "rotation": 45, "vcamWidth": 1000, "futureField": 1}"#).unwrap();
        let s = Settings::load(&path);
        assert!(s.mirror);
        assert_eq!(s.rotation, 0);
        assert_eq!((s.vcam_width, s.vcam_height), (1920, 1080));

        std::fs::write(&path, r#"{"wifiPhones": [{"id": "a"}, {"id": "a", "name": "x"}, {"id": ""}]}"#).unwrap();
        assert_eq!(Settings::load(&path).wifi_phones, [WifiPhone { id: "a".into(), name: None }]);

        // A bad entry costs only itself.
        std::fs::write(
            &path,
            r#"{"mirror": true, "wifiPhones": [{"name": "no id"}, "text", {"id": 5}, {"id": "b", "name": 7}, {"id": "c"}]}"#,
        )
        .unwrap();
        let s = Settings::load(&path);
        assert!(s.mirror);
        assert_eq!(s.wifi_phones, [WifiPhone { id: "c".into(), name: None }]);
        std::fs::write(&path, r#"{"mirror": true, "wifiPhones": {"id": "c"}}"#).unwrap();
        let s = Settings::load(&path);
        assert!(s.mirror && s.wifi_phones.is_empty());

        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn remembers_wifi_phones_once() {
        let mut s = Settings::default();
        s.remember_wifi_phone("adb-1-x", None);
        s.remember_wifi_phone("adb-1-x", Some("Pixel 8".into()));
        s.remember_wifi_phone("adb-1-x", None);
        s.remember_wifi_phone("10.0.0.2:5555", None);
        assert_eq!(s.wifi_phones.len(), 2);
        assert_eq!(s.wifi_phones[0].name.as_deref(), Some("Pixel 8"));
    }

    #[test]
    fn forgotten_wifi_phones() {
        let mut s = Settings::default();
        s.remember_wifi_phone("adb-1-x", None);
        s.forget_wifi_phone("adb-1-x");
        s.forget_wifi_phone("adb-1-x");
        assert!(s.wifi_phones.is_empty());
        assert_eq!(s.forgotten_wifi, ["adb-1-x"]);
        s.remember_wifi_phone("adb-1-x", None);
        assert!(s.forgotten_wifi.is_empty());

        let s: Settings =
            serde_json::from_str(r#"{"forgottenWifi": ["adb-1-x", 5, "", "adb-1-x", "10.0.0.2:5555"]}"#).unwrap();
        assert_eq!(s.sanitized().forgotten_wifi, ["adb-1-x", "10.0.0.2:5555"]);
        let s: Settings = serde_json::from_str(r#"{"mirror": true, "forgottenWifi": "adb-1-x"}"#).unwrap();
        assert!(s.mirror && s.forgotten_wifi.is_empty());
    }

    #[test]
    fn bitrate_follows_resolution_and_fps_unless_set() {
        let mut s = Settings::default();
        assert_eq!(s.bitrate(), 12_000_000);
        s.fps = 60;
        assert_eq!(s.bitrate(), 16_000_000);
        (s.vcam_width, s.vcam_height, s.fps) = (1280, 720, 30);
        assert_eq!(s.bitrate(), 6_000_000);
        (s.vcam_width, s.vcam_height) = (3840, 2160);
        assert_eq!(s.bitrate(), 32_000_000);
        s.bitrate_mbps = Some(25);
        assert_eq!(s.bitrate(), 25_000_000);
    }

    #[test]
    fn presets_of_0_1_migrate() {
        let load = |json: &str| Settings::sanitized(serde_json::from_str(json).unwrap());
        let s = load(r#"{"quality": "smooth"}"#);
        assert_eq!((s.fps, s.allow_high_speed, s.vcam_height, s.quality), (60, false, 1080, None));
        let s = load(r#"{"quality": "economy"}"#);
        assert_eq!((s.fps, s.vcam_width, s.vcam_height), (30, 1280, 720));
        let s = load(r#"{"quality": "economy", "vcamWidth": 3840, "vcamHeight": 2160}"#);
        assert_eq!(s.vcam_height, 2160);
        let s = load(r#"{"quality": "standard", "fps": 45}"#);
        assert_eq!((s.fps, s.vcam_height), (30, 1080));
        assert!(!serde_json::to_string(&s).unwrap().contains("quality"));
    }
}
