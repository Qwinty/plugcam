//! User settings, stored as JSON in the app's config folder. Missing or unknown fields fall
//! back to defaults, so older and newer files both load.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::scrcpy::cameras::Quality;
use crate::scrcpy::server::{CameraParams, Facing};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub onboarding_done: bool,
    /// `back` or `front`.
    pub facing: String,
    /// A specific lens; `None` = the phone's default camera for `facing`.
    pub camera_id: Option<String>,
    pub quality: Quality,
    pub mirror: bool,
    /// Clockwise degrees of the picture: 0, 90, 180 or 270.
    pub rotation: u16,
    /// `None` = automatic, by quality.
    pub bitrate_mbps: Option<u32>,
    pub vcam_width: u32,
    pub vcam_height: u32,
    pub launch_at_login: bool,
    pub close_to_tray: bool,
    pub auto_start_camera: bool,
    /// Lenses that sent no picture, by phone model; hidden from the lens picker.
    pub broken_cameras: BTreeMap<String, Vec<String>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            onboarding_done: false,
            facing: "back".into(),
            camera_id: None,
            quality: Quality::Standard,
            mirror: false,
            rotation: 0,
            bitrate_mbps: None,
            vcam_width: 1920,
            vcam_height: 1080,
            launch_at_login: false,
            close_to_tray: true,
            auto_start_camera: false,
            broken_cameras: BTreeMap::new(),
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
        if !matches!(self.rotation, 0 | 90 | 180 | 270) {
            self.rotation = 0;
        }
        if !VCAM_SIZES.contains(&(self.vcam_width, self.vcam_height)) {
            (self.vcam_width, self.vcam_height) = (d.vcam_width, d.vcam_height);
        }
        self.bitrate_mbps = self.bitrate_mbps.map(|b| b.clamp(4, 40));
        self
    }

    /// Bit rate for the phone's encoder: the user's choice, or one that suits the quality.
    pub fn bitrate(&self) -> u32 {
        let mbps = self.bitrate_mbps.unwrap_or(match self.quality {
            Quality::Economy => 6,
            Quality::Standard => 12,
            Quality::Smooth => 16,
        });
        mbps * 1_000_000
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
        let mut s = Settings { mirror: true, quality: Quality::Smooth, ..Settings::default() };
        s.mark_broken("PHK110", "4");
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        assert!(Settings::load(&path).is_broken("PHK110", "4"));

        std::fs::write(&path, r#"{"mirror": true, "rotation": 45, "vcamWidth": 1000, "futureField": 1}"#).unwrap();
        let s = Settings::load(&path);
        assert!(s.mirror);
        assert_eq!(s.rotation, 0);
        assert_eq!((s.vcam_width, s.vcam_height), (1920, 1080));

        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bitrate_follows_quality_unless_set() {
        let mut s = Settings::default();
        assert_eq!(s.bitrate(), 12_000_000);
        s.quality = Quality::Economy;
        assert_eq!(s.bitrate(), 6_000_000);
        s.bitrate_mbps = Some(25);
        assert_eq!(s.bitrate(), 25_000_000);
    }
}
