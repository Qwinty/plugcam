//! Tests on streams recorded from a real phone (tests/fixtures at the repository root).

use std::io;
use std::path::PathBuf;

use plugcam::decode::H264Decoder;
use plugcam::frame::FrameConverter;
use plugcam::scrcpy::protocol::{self, Packet};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn packets(data: &[u8]) -> (Vec<Packet>, io::Error) {
    let mut r = data;
    let mut out = Vec::new();
    loop {
        match protocol::read_packet(&mut r) {
            Ok(p) => out.push(p),
            Err(e) => return (out, e),
        }
    }
}

#[test]
fn real_stream_structure() {
    let data = fixture("oneplus11r-front-640x480.scrcpy");
    let (packets, end) = packets(&data);
    // The recording stops at an arbitrary byte, so it ends mid-packet or cleanly.
    assert_eq!(end.kind(), io::ErrorKind::UnexpectedEof);

    assert_eq!(packets[0], Packet::Session { width: 640, height: 480 });
    let Packet::Config(config) = &packets[1] else { panic!("second packet should be config: {:?}", packets[1]) };
    // SPS (NAL type 7) first, Annex B start code.
    assert!(config.starts_with(&[0, 0, 0, 1]) && config[4] & 0x1f == 7, "{config:02x?}");

    let frames: Vec<_> = packets[2..]
        .iter()
        .map(|p| match p {
            Packet::Frame { pts_us, key_frame, data } => (*pts_us, *key_frame, data.len()),
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert!(frames.len() > 60, "{} frames", frames.len());
    assert!(frames[0].1, "first frame must be a key frame");
    assert!(frames.windows(2).all(|w| w[1].0 > w[0].0), "PTS must increase");
    // ~30 fps: 33 ms apart on average.
    let avg_us = (frames.last().unwrap().0 - frames[0].0) / (frames.len() as u64 - 1);
    assert!((25_000..45_000).contains(&avg_us), "average frame interval {avg_us} µs");
}

#[test]
fn every_prefix_of_a_real_stream_parses_or_ends_cleanly() {
    let data = fixture("oneplus11r-front-640x480.scrcpy");
    for cut in (0..data.len()).step_by(997) {
        let (_, end) = packets(&data[..cut]);
        assert_eq!(end.kind(), io::ErrorKind::UnexpectedEof, "cut {cut}: {end}");
    }
}

#[test]
fn real_stream_decodes_to_frames() {
    let data = fixture("oneplus11r-front-640x480.scrcpy");
    let (packets, _) = packets(&data);
    let mut decoder = match H264Decoder::new(640, 480) {
        Ok(d) => d,
        // Windows Server images may lack the Media Foundation H.264 decoder.
        Err(e) => return eprintln!("skipped: no H.264 decoder here ({e})"),
    };
    let mut converter = FrameConverter::new(1920, 1080);
    let mut decoded = 0;
    let mut middle_pixel = [0u8; 3];
    for p in &packets[1..] {
        let (data, pts) = match p {
            Packet::Config(c) => (c, 0),
            Packet::Frame { data, pts_us, .. } => (data, *pts_us),
            Packet::Session { .. } => unreachable!(),
        };
        decoder
            .decode(data, pts, &mut |f| {
                assert_eq!((f.width, f.height), (640, 480));
                let bgr = converter.convert(f).unwrap();
                let i = (540 * 1920 + 960) * 3;
                middle_pixel.copy_from_slice(&bgr[i..i + 3]);
                decoded += 1;
            })
            .unwrap();
    }
    let frames = packets.iter().filter(|p| matches!(p, Packet::Frame { .. })).count();
    // Low-latency mode: every frame fed comes out (the last one may still be in flight).
    assert!(decoded + 1 >= frames, "{decoded} of {frames} frames decoded");
    // 4:3 in 16:9 → black bars left and right, picture in the middle.
    assert_eq!(&converter.output()[(540 * 1920 + 10) * 3..][..3], [0, 0, 0]);
    assert_ne!(middle_pixel, [0, 0, 0], "the picture should not be black");
}
