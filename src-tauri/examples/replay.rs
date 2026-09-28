//! Replays a raw stream recorded with `plugcam-cli --dump FILE` through the decoder and the
//! frame converter, without a phone, and times both. Optionally saves the last converted
//! frame as a BMP (the first ones are often still dark while the camera starts).
//!
//! cargo run --release --example replay -- FILE [last-frame.bmp]
//!
//! `PLUGCAM_SOFTWARE_DECODE=1` compares with decoding on the CPU.

use std::io::Write;
use std::time::{Duration, Instant};

use plugcam::decode::H264Decoder;
use plugcam::frame::FrameConverter;
use plugcam::scrcpy::protocol::{self, Packet};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: replay FILE [last-frame.bmp]");
    let bmp = args.next();
    env_logger::Builder::new().filter_level(log::LevelFilter::Info).init();

    let data = std::fs::read(&path).expect("read dump");
    let mut r = data.as_slice();
    let mut decoder = None;
    let mut converter = FrameConverter::new(1920, 1080);
    let (mut packets, mut frames) = (0, 0);
    let (mut decode_time, mut convert_time) = (Duration::ZERO, Duration::ZERO);
    let cpu_start = process_cpu();
    let wall_start = Instant::now();

    while !r.is_empty() {
        let packet = match protocol::read_packet(&mut r) {
            Ok(p) => p,
            Err(e) => {
                println!("stream ends: {e}");
                break;
            }
        };
        packets += 1;
        match packet {
            Packet::Session { width, height } => {
                println!("session {width}x{height}");
                decoder = Some(H264Decoder::new(width, height).expect("decoder"));
            }
            Packet::Config(c) => decoder.as_mut().unwrap().decode(&c, 0, &mut |_| {}).expect("config"),
            Packet::Frame { pts_us, data, .. } => {
                let d = decoder.as_mut().unwrap();
                let started = Instant::now();
                let mut converting = Duration::ZERO;
                d.decode(&data, pts_us, &mut |f| {
                    frames += 1;
                    let t = Instant::now();
                    converter.convert(f).expect("convert");
                    converting += t.elapsed();
                })
                .unwrap_or_else(|e| panic!("packet {packets}: {e}"));
                decode_time += started.elapsed() - converting;
                convert_time += converting;
            }
        }
    }
    println!("{packets} packets, {frames} frames decoded");
    if let Some(path) = bmp {
        write_bmp(&path, 1920, 1080, converter.output());
    }
    let per_frame = |d: Duration| d.as_secs_f64() * 1000.0 / frames.max(1) as f64;
    println!("decode {:.2} ms, convert {:.2} ms per frame", per_frame(decode_time), per_frame(convert_time));
    let (wall, cpu) = (wall_start.elapsed(), process_cpu() - cpu_start);
    println!("CPU time {:.2} ms per frame ({:.1} cores while replaying)", per_frame(cpu), cpu.as_secs_f64() / wall.as_secs_f64());
    println!("private memory: {} MB now, {} MB at most", private_mb().0, private_mb().1);
}

/// Private bytes of this process in MB: current and peak.
fn private_mb() -> (usize, usize) {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
    use windows::Win32::System::Threading::GetCurrentProcess;
    let mut c = PROCESS_MEMORY_COUNTERS_EX::default();
    let size = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, size).unwrap() };
    (c.PrivateUsage >> 20, c.PeakPagefileUsage >> 20)
}

/// User + kernel time of this process so far.
fn process_cpu() -> Duration {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let [mut a, mut b, mut kernel, mut user] = [FILETIME::default(); 4];
    unsafe { GetProcessTimes(GetCurrentProcess(), &mut a, &mut b, &mut kernel, &mut user).unwrap() };
    let ticks = |t: FILETIME| (t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64;
    Duration::from_nanos((ticks(kernel) + ticks(user)) * 100)
}

fn write_bmp(path: &str, w: u32, h: u32, bgr: &[u8]) {
    let row = (w * 3).div_ceil(4) * 4;
    let size = 54 + row * h;
    let mut f = std::fs::File::create(path).unwrap();
    let mut header = Vec::with_capacity(54);
    header.extend(b"BM");
    header.extend(size.to_le_bytes());
    header.extend([0u8; 4]);
    header.extend(54u32.to_le_bytes());
    header.extend(40u32.to_le_bytes());
    header.extend((w as i32).to_le_bytes());
    header.extend((-(h as i32)).to_le_bytes()); // top-down
    header.extend(1u16.to_le_bytes());
    header.extend(24u16.to_le_bytes());
    header.extend([0u8; 24]);
    f.write_all(&header).unwrap();
    let pad = vec![0u8; (row - w * 3) as usize];
    for line in bgr.chunks_exact(w as usize * 3) {
        f.write_all(line).unwrap();
        f.write_all(&pad).unwrap();
    }
}
