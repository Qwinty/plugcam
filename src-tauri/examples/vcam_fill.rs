//! Stage 1 check: feeds "Plugcam Camera" with a color fill that changes every second
//! plus a moving white bar, so both color and motion are visible in Zoom/OBS.
//!
//! cargo run --release --example vcam_fill -- [seconds] [path\to\plugcam_cam.dll]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use plugcam::vcam::VirtualCamera;

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const FPS: u32 = 30;
// BGR
const COLORS: [[u8; 3]; 6] = [
    [0, 0, 220],   // red
    [0, 200, 0],   // green
    [220, 60, 0],  // blue
    [0, 200, 220], // yellow
    [200, 0, 200], // magenta
    [200, 200, 0], // cyan
];

fn main() {
    let mut args = std::env::args().skip(1);
    let seconds: u64 = args.next().map(|s| s.parse().expect("seconds")).unwrap_or(60);
    let dll = args.next().map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(r"..\vcam\dist\bin\x64\plugcam_cam.dll")
    });

    let cam = VirtualCamera::create(&dll, WIDTH, HEIGHT).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    });
    println!("Plugcam Camera is up at {WIDTH}x{HEIGHT} for {seconds}s (dll: {})", dll.display());

    let mut frame = vec![0u8; cam.frame_len()];
    let row_len = WIDTH as usize * 3;
    let bar_h = HEIGHT as usize / 10;
    let frame_time = Duration::from_secs(1) / FPS;
    let start = Instant::now();
    let mut was_connected = false;

    for n in 0u64.. {
        let elapsed = start.elapsed();
        if elapsed >= Duration::from_secs(seconds) {
            break;
        }
        let color = COLORS[(elapsed.as_secs() as usize) % COLORS.len()];
        let bar_top = (n as usize * 8) % (HEIGHT as usize - bar_h);
        for (y, row) in frame.chunks_exact_mut(row_len).enumerate() {
            let px = if (bar_top..bar_top + bar_h).contains(&y) { [255, 255, 255] } else { color };
            for p in row.chunks_exact_mut(3) {
                p.copy_from_slice(&px);
            }
        }
        cam.send_frame(&frame);

        let connected = cam.is_connected();
        if connected != was_connected {
            println!("{}", if connected { "an app opened the camera" } else { "the app closed the camera" });
            was_connected = connected;
        }
        if let Some(wait) = (frame_time * (n as u32 + 1)).checked_sub(start.elapsed()) {
            std::thread::sleep(wait);
        }
    }
}
