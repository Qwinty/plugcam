//! The server's camera zoom model (scrcpy 4.1 `CameraCapture.zoom`): each zoom-in/out message
//! moves one step on a log scale, `zoom = FACTOR^step`, starting from the current zoom rounded
//! to the nearest step, and the result is clamped to the camera's zoom range. The server does
//! not send the zoom back; it only logs `Set camera zoom: <value>`, which `parse_log_line` reads.

/// `ZOOM_FACTOR` in CameraCapture.java.
pub const FACTOR: f64 = 1.0 + 1.0 / 16.0;

/// The step the server would start from at this zoom.
pub fn step_of(zoom: f32) -> i32 {
    ((zoom.max(f32::MIN_POSITIVE) as f64).ln() / FACTOR.ln()).round() as i32
}

/// Steps between two zoom values, as the server counts them.
pub fn steps_between(from: f32, to: f32) -> i32 {
    step_of(to) - step_of(from)
}

/// The steps reachable within a zoom range: at the ends the server clamps, and the clamped
/// value rounds back to these steps, so going past them sends messages that change nothing.
pub fn step_range(range: (f32, f32)) -> (i32, i32) {
    (step_of(range.0), step_of(range.1))
}

/// `Set camera zoom: 1.1289062` → 1.1289062.
pub fn parse_log_line(line: &str) -> Option<f32> {
    let (_, value) = line.split_once("Set camera zoom: ")?;
    value.trim().parse().ok().filter(|z: &f32| z.is_finite() && *z > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One server step from `zoom`, including its clamping.
    fn server_step(zoom: f32, dir: i32, range: (f32, f32)) -> f32 {
        let z = FACTOR.powi(step_of(zoom) + dir) as f32;
        z.clamp(range.0, range.1)
    }

    #[test]
    fn steps() {
        assert_eq!(step_of(1.0), 0);
        assert_eq!(step_of(FACTOR as f32), 1);
        assert_eq!(steps_between(1.0, 2.0), 11); // 1.0625^11 = 1.95
        assert_eq!(steps_between(2.0, 1.0), -11);
        assert_eq!(step_range((1.0, 10.0)), (0, 38));
        assert_eq!(step_range((0.5, 10.0)), (-11, 38));
    }

    #[test]
    fn step_range_matches_the_server_at_the_ends() {
        let range = (1.0, 10.0);
        // Walk the server up past the top: it stays at the step step_range reports.
        let mut z = 1.0;
        for _ in 0..50 {
            z = server_step(z, 1, range);
        }
        assert_eq!(z, 10.0);
        assert_eq!(step_of(z), step_range(range).1);
        // One step down from the top lands where the model expects.
        assert_eq!(step_of(server_step(z, -1, range)), step_range(range).1 - 1);
    }

    #[test]
    fn log_line() {
        assert_eq!(parse_log_line("[server] INFO: Set camera zoom: 1.1289062"), Some(1.1289062));
        assert_eq!(parse_log_line("INFO: Set camera zoom: 10.0"), Some(10.0));
        assert_eq!(parse_log_line("INFO: Using camera '0'"), None);
        assert_eq!(parse_log_line("INFO: Set camera zoom: NaN"), None);
    }
}
