//! Replays a raw stream recorded with `plugcam-cli --dump FILE` through the decoder and the
//! frame converter, without a phone. Optionally saves the first converted frame as a BMP.
//!
//! cargo run --release --example replay -- FILE [first-frame.bmp]

use std::io::Write;

use plugcam::decode::H264Decoder;
use plugcam::frame::FrameConverter;
use plugcam::scrcpy::protocol::{self, Packet};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: replay FILE [first-frame.bmp]");
    let bmp = args.next();
    env_logger::Builder::new().filter_level(log::LevelFilter::Info).init();

    let data = std::fs::read(&path).expect("read dump");
    let mut r = data.as_slice();
    let mut decoder = None;
    let mut converter = FrameConverter::new(1920, 1080);
    let (mut packets, mut frames) = (0, 0);
    let mut saved = bmp.is_none();

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
                d.decode(&data, pts_us, &mut |f| {
                    frames += 1;
                    let bgr = converter.convert(f).expect("convert");
                    if !saved {
                        write_bmp(bmp.as_deref().unwrap(), 1920, 1080, bgr);
                        saved = true;
                    }
                })
                .unwrap_or_else(|e| panic!("packet {packets}: {e}"));
            }
        }
    }
    println!("{packets} packets, {frames} frames decoded");
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
