//! The phone's cameras, parsed from scrcpy-server's `list_cameras=true list_camera_sizes=true`
//! report, and the choice of capture mode for a quality preset.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CameraInfo {
    pub id: String,
    /// `back`, `front` or `external`.
    pub facing: String,
    /// Sensor active array, e.g. 4096x3072.
    pub sensor: (u32, u32),
    pub fps: Vec<u32>,
    pub zoom: Option<(f32, f32)>,
    /// Sizes the encoder can take at normal speed.
    pub sizes: Vec<(u32, u32)>,
    pub high_speed: Vec<HighSpeedMode>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HighSpeedMode {
    pub size: (u32, u32),
    pub fps: Vec<u32>,
}

impl CameraInfo {
    pub fn megapixels(&self) -> f32 {
        (self.sensor.0 as f32 * self.sensor.1 as f32 / 1e6 * 10.0).round() / 10.0
    }
}

/// Parses the server output; lines that are not part of the list are ignored.
pub fn parse_camera_list(output: &str) -> Vec<CameraInfo> {
    let mut cameras: Vec<CameraInfo> = Vec::new();
    let mut in_high_speed = false;
    for line in output.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("--camera-id=") {
            let (id, desc) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let desc = desc.trim().trim_start_matches('(').trim_end_matches(')');
            let facing = desc.split(',').next().unwrap_or("").trim().to_string();
            let sensor = desc.split(',').nth(1).and_then(|s| parse_size(s.trim())).unwrap_or((0, 0));
            let fps = between(desc, "fps={", "}").map(parse_numbers).unwrap_or_default();
            let zoom = between(desc, "zoom-range=[", "]").and_then(|z| {
                let (a, b) = z.split_once(',')?;
                Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
            });
            cameras.push(CameraInfo { id: id.to_string(), facing, sensor, fps, zoom, sizes: vec![], high_speed: vec![] });
            in_high_speed = false;
        } else if line.starts_with("High speed capture") {
            in_high_speed = true;
        } else if let (Some(size_line), Some(cam)) = (line.strip_prefix("- "), cameras.last_mut()) {
            let (size, extra) = size_line.split_once(' ').unwrap_or((size_line, ""));
            let Some(size) = parse_size(size) else { continue };
            if in_high_speed {
                let fps = between(extra, "fps={", "}").map(parse_numbers).unwrap_or_default();
                cam.high_speed.push(HighSpeedMode { size, fps });
            } else {
                cam.sizes.push(size);
            }
        }
    }
    cameras
}

fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let from = s.find(start)? + start.len();
    let len = s[from..].find(end)?;
    Some(&s[from..from + len])
}

fn parse_numbers(s: &str) -> Vec<u32> {
    s.split(',').filter_map(|n| n.trim().parse().ok()).collect()
}

fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    /// 720p30
    Economy,
    /// 1080p30
    #[default]
    Standard,
    /// ~1080p60 through a 120 fps high-speed session (see docs/stage0.md)
    Smooth,
}

/// Capture settings for one camera and quality preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureMode {
    pub size: (u32, u32),
    pub fps: u32,
    pub high_speed: bool,
}

impl Quality {
    fn target(self) -> (u32, u32) {
        match self {
            Quality::Economy => (1280, 720),
            Quality::Standard | Quality::Smooth => (1920, 1080),
        }
    }
}

/// Picks the capture mode for `quality`, or `None` if the camera cannot do it
/// (only `Smooth` can be unavailable: it needs a 120 fps high-speed mode).
pub fn capture_mode(cam: &CameraInfo, quality: Quality) -> Option<CaptureMode> {
    let (tw, th) = quality.target();
    if quality == Quality::Smooth {
        let modes = cam.high_speed.iter().filter(|m| m.fps.contains(&120));
        let size = best_size(modes.map(|m| m.size), tw, th)?;
        return Some(CaptureMode { size, fps: 120, high_speed: true });
    }
    let fps = if cam.fps.is_empty() || cam.fps.contains(&30) { 30 } else { *cam.fps.iter().max().unwrap() };
    let size = best_size(cam.sizes.iter().copied(), tw, th)?;
    Some(CaptureMode { size, fps, high_speed: false })
}

/// Exact target if offered; otherwise the largest 16:9 size not taller than the target;
/// otherwise the largest size not taller than the target; otherwise the smallest size.
fn best_size(sizes: impl Iterator<Item = (u32, u32)>, tw: u32, th: u32) -> Option<(u32, u32)> {
    let sizes: Vec<_> = sizes.collect();
    if sizes.contains(&(tw, th)) {
        return Some((tw, th));
    }
    let area = |s: &(u32, u32)| s.0 as u64 * s.1 as u64;
    let fits = |s: &&(u32, u32)| s.1 <= th && s.0 <= tw.max(th * 2);
    sizes
        .iter()
        .filter(fits)
        .filter(|s| s.0 * 9 == s.1 * 16)
        .max_by_key(|s| area(s))
        .or_else(|| sizes.iter().filter(fits).max_by_key(|s| area(s)))
        .or_else(|| sizes.iter().min_by_key(|s| area(s)))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oneplus() -> Vec<CameraInfo> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/oneplus11r-list-cameras.txt");
        parse_camera_list(&std::fs::read_to_string(path).unwrap())
    }

    #[test]
    fn parses_oneplus_11r() {
        let cams = oneplus();
        let ids: Vec<_> = cams.iter().map(|c| (c.id.as_str(), c.facing.as_str())).collect();
        assert_eq!(ids, [("0", "back"), ("1", "front"), ("2", "back"), ("3", "back"), ("4", "back")]);

        let main = &cams[0];
        assert_eq!(main.sensor, (4096, 3072));
        assert_eq!(main.megapixels(), 12.6);
        assert_eq!(main.fps, [10, 15, 20, 24, 30]);
        assert_eq!(main.zoom, Some((1.0, 10.0)));
        assert!(main.sizes.contains(&(1920, 1080)));
        assert_eq!(main.sizes.len(), 47);
        assert_eq!(main.high_speed.len(), 4);
        assert_eq!(main.high_speed[3], HighSpeedMode { size: (1920, 1080), fps: vec![120, 240, 480] });

        assert!(cams[1].high_speed.is_empty());
        assert_eq!(cams[3].fps, [24, 30]);
        assert_eq!(cams[4].zoom, Some((0.5, 10.0)));
    }

    #[test]
    fn capture_modes_on_oneplus_11r() {
        let cams = oneplus();
        let mode = |i: usize, q| capture_mode(&cams[i], q);
        assert_eq!(mode(0, Quality::Standard), Some(CaptureMode { size: (1920, 1080), fps: 30, high_speed: false }));
        assert_eq!(mode(0, Quality::Economy), Some(CaptureMode { size: (1280, 720), fps: 30, high_speed: false }));
        assert_eq!(mode(0, Quality::Smooth), Some(CaptureMode { size: (1920, 1080), fps: 120, high_speed: true }));
        assert_eq!(mode(1, Quality::Smooth), None);
        // Macro camera tops out at 1440x1080: the largest 16:9 size under 1080 lines is 1280x720.
        assert_eq!(mode(3, Quality::Standard), Some(CaptureMode { size: (1280, 720), fps: 30, high_speed: false }));
    }

    #[test]
    fn no_cameras_and_missing_fields() {
        assert!(parse_camera_list("[server] INFO: List of cameras:\n    (none)\n").is_empty());
        let cams = parse_camera_list("    --camera-id=7    (external, 640x480)\n        - 640x480\n");
        assert_eq!(cams[0].facing, "external");
        assert!(cams[0].fps.is_empty() && cams[0].zoom.is_none());
        assert_eq!(capture_mode(&cams[0], Quality::Standard).unwrap().size, (640, 480));
    }
}
