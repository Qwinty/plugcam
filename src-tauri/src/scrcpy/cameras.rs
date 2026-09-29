//! The phone's cameras, parsed from scrcpy-server's `list_cameras=true list_camera_sizes=true`
//! report, and the choice of capture mode for a resolution and frame rate.

use serde::Serialize;

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

/// Regular sessions give 60 fps only up to this height: the fps list is the same for every
/// size, and a phone that lists 60 may not reach it at 1440p or 4K.
const REGULAR_60_MAX_HEIGHT: u32 = 1080;
/// High-speed sessions start at 120 fps; the OnePlus 11R delivers ~60 of them (docs/stage0.md).
const HIGH_SPEED_FPS: u32 = 120;

/// Capture settings for one camera, resolution and frame rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureMode {
    pub size: (u32, u32),
    pub fps: u32,
    pub high_speed: bool,
}

/// One entry of the frame rate picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FpsOption {
    /// The setting: 30 or 60.
    pub value: u32,
    /// What the phone is asked for (e.g. 24 on a lens without 30, 120 in high-speed).
    pub fps: u32,
    pub high_speed: bool,
}

/// Picks the capture mode closest to `size` at `fps` (30 or 60), or `None` if the camera
/// lists no sizes.
///
/// 60 fps comes from a regular session when the camera offers it. Otherwise, and only with
/// `allow_high_speed`, it comes from a constrained high-speed session: there Android forces
/// auto AE/AWB/AF and FAST post-processing, and exposure is at most 1/120 s, so colors differ,
/// the picture is noisier and lamps on 50 Hz mains flicker. When neither works at this size,
/// the camera stays at 30.
pub fn capture_mode(cam: &CameraInfo, size: (u32, u32), fps: u32, allow_high_speed: bool) -> Option<CaptureMode> {
    let size = best_size(cam.sizes.iter().copied(), size.0, size.1)?;
    if fps >= 60 {
        if cam.fps.contains(&60) && size.1 <= REGULAR_60_MAX_HEIGHT {
            return Some(CaptureMode { size, fps: 60, high_speed: false });
        }
        if allow_high_speed && cam.high_speed.iter().any(|m| m.size == size && m.fps.contains(&HIGH_SPEED_FPS)) {
            return Some(CaptureMode { size, fps: HIGH_SPEED_FPS, high_speed: true });
        }
    }
    let fps = if cam.fps.is_empty() || cam.fps.contains(&30) {
        30
    } else {
        cam.fps.iter().copied().filter(|&f| f < 30).max().or(cam.fps.iter().copied().min()).unwrap()
    };
    Some(CaptureMode { size, fps, high_speed: false })
}

/// The `candidates` this camera captures as they are, or the smallest one if it has none of
/// them (the picture is then scaled up).
pub fn resolutions(cam: &CameraInfo, candidates: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let native: Vec<_> = candidates.iter().copied().filter(|s| cam.sizes.contains(s)).collect();
    if native.is_empty() {
        candidates.iter().copied().min_by_key(|s| s.0 * s.1).into_iter().collect()
    } else {
        native
    }
}

/// Frame rates the picker offers at `size`: 30, and 60 when the camera really gets there.
pub fn fps_options(cam: &CameraInfo, size: (u32, u32), allow_high_speed: bool) -> Vec<FpsOption> {
    [30, 60]
        .into_iter()
        .filter_map(|value| {
            let m = capture_mode(cam, size, value, allow_high_speed)?;
            (value == 30 || m.fps >= 60).then_some(FpsOption { value, fps: m.fps, high_speed: m.high_speed })
        })
        .collect()
}

/// At `size`, 60 fps is possible on this camera only through a high-speed session.
pub fn sixty_needs_high_speed(cam: &CameraInfo, size: (u32, u32)) -> bool {
    let sixty = |allow| capture_mode(cam, size, 60, allow).is_some_and(|m| m.fps >= 60);
    !sixty(false) && sixty(true)
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

    const HD: (u32, u32) = (1280, 720);
    const FULL_HD: (u32, u32) = (1920, 1080);
    const UHD: (u32, u32) = (3840, 2160);

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
        let mode = |i: usize, size, fps, hs| capture_mode(&cams[i], size, fps, hs);
        let m = |size, fps, high_speed| Some(CaptureMode { size, fps, high_speed });
        assert_eq!(mode(0, FULL_HD, 30, false), m(FULL_HD, 30, false));
        assert_eq!(mode(0, HD, 30, false), m(HD, 30, false));
        assert_eq!(mode(0, UHD, 30, false), m(UHD, 30, false));
        // No regular 60 on this phone: high-speed only when allowed, else it stays at 30.
        assert_eq!(mode(0, FULL_HD, 60, false), m(FULL_HD, 30, false));
        assert_eq!(mode(0, FULL_HD, 60, true), m(FULL_HD, 120, true));
        assert_eq!(mode(0, HD, 60, true), m(HD, 120, true));
        assert_eq!(mode(0, UHD, 60, true), m(UHD, 30, false));
        assert_eq!(mode(1, FULL_HD, 60, true), m(FULL_HD, 30, false));
        // Macro camera tops out at 1440x1080: the largest 16:9 size under 1080 lines is 1280x720.
        assert_eq!(mode(3, FULL_HD, 30, false), m(HD, 30, false));
        assert!(sixty_needs_high_speed(&cams[0], FULL_HD));
        assert!(!sixty_needs_high_speed(&cams[0], UHD));
        assert!(!sixty_needs_high_speed(&cams[1], FULL_HD));
    }

    #[test]
    fn regular_60_is_preferred() {
        let mut cam = oneplus().remove(0);
        cam.fps.push(60);
        let m = |size, fps, high_speed| Some(CaptureMode { size, fps, high_speed });
        assert_eq!(capture_mode(&cam, FULL_HD, 60, true), m(FULL_HD, 60, false));
        assert_eq!(capture_mode(&cam, FULL_HD, 60, false), m(FULL_HD, 60, false));
        assert_eq!(capture_mode(&cam, UHD, 60, true), m(UHD, 30, false));
        assert!(!sixty_needs_high_speed(&cam, FULL_HD));
        let opts = fps_options(&cam, FULL_HD, false);
        assert_eq!(opts[1], FpsOption { value: 60, fps: 60, high_speed: false });
    }

    #[test]
    fn picker_options_on_oneplus_11r() {
        let cams = oneplus();
        let all = [HD, FULL_HD, (2560, 1440), UHD];
        assert_eq!(resolutions(&cams[0], &all), all);
        assert_eq!(resolutions(&cams[3], &all), [HD]);
        assert_eq!(resolutions(&cams[3], &[FULL_HD, UHD]), [FULL_HD]);

        let values = |size, hs| fps_options(&cams[0], size, hs).iter().map(|o| o.value).collect::<Vec<_>>();
        assert_eq!(values(FULL_HD, false), [30]);
        assert_eq!(values(FULL_HD, true), [30, 60]);
        assert_eq!(values(UHD, true), [30]);
        assert_eq!(fps_options(&cams[0], FULL_HD, true)[1], FpsOption { value: 60, fps: 120, high_speed: true });
        // Only 24 and 30 on the macro lens; a lens with no 30 offers the best below it.
        assert_eq!(fps_options(&cams[3], HD, true), [FpsOption { value: 30, fps: 30, high_speed: false }]);
        let mut cam = cams[3].clone();
        cam.fps = vec![15, 24];
        assert_eq!(fps_options(&cam, HD, true)[0].fps, 24);
    }

    #[test]
    fn no_cameras_and_missing_fields() {
        assert!(parse_camera_list("[server] INFO: List of cameras:\n    (none)\n").is_empty());
        let cams = parse_camera_list("    --camera-id=7    (external, 640x480)\n        - 640x480\n");
        assert_eq!(cams[0].facing, "external");
        assert!(cams[0].fps.is_empty() && cams[0].zoom.is_none());
        assert_eq!(capture_mode(&cams[0], FULL_HD, 30, false).unwrap().size, (640, 480));
        assert_eq!(resolutions(&cams[0], &[FULL_HD, HD]), [HD]);
    }
}
