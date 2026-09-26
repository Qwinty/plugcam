//! scrcpy 4.1 wire protocol. It is internal to scrcpy and changes between versions, so it is
//! pinned together with the server file (see `server::SERVER_VERSION`, docs/SPEC.md 2.2).

use std::io::{self, Read};

/// `"h264"` as a big-endian u32.
pub const CODEC_H264: u32 = 0x6832_3634;

const DEVICE_NAME_LEN: usize = 64;
const FLAG_SESSION: u64 = 1 << 63;
const FLAG_CONFIG: u64 = 1 << 62;
const FLAG_KEY_FRAME: u64 = 1 << 61;
const PTS_MASK: u64 = FLAG_KEY_FRAME - 1;
/// A 4K key frame at 40 Mbit/s is a few MB; anything above this is a corrupt stream.
const MAX_PACKET_SIZE: u32 = 32 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// A new capture session started; the video size may have changed.
    Session { width: u32, height: u32 },
    /// Codec configuration (H.264 SPS/PPS, Annex B); the decoder needs it before the next frame.
    Config(Vec<u8>),
    /// One encoded frame (Annex B). PTS is in microseconds.
    Frame { pts_us: u64, key_frame: bool, data: Vec<u8> },
}

/// Forward tunnels only: the device writes one zero byte on the first socket so the client
/// can tell a real server from adb accepting the connection with nothing behind it.
pub fn read_dummy_byte(r: &mut impl Read) -> io::Result<()> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)
}

/// Device name, sent once on the first socket: 64 bytes of UTF-8 padded with zeros.
pub fn read_device_name(r: &mut impl Read) -> io::Result<String> {
    let mut buf = [0u8; DEVICE_NAME_LEN];
    r.read_exact(&mut buf)?;
    let end = buf.iter().position(|&b| b == 0).unwrap_or(DEVICE_NAME_LEN);
    Ok(String::from_utf8_lossy(&buf[..end]).into_owned())
}

/// Codec id, sent once at the start of the video socket.
pub fn read_codec_id(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_be_bytes(b))
}

/// Reads the next session or media packet from the video socket.
/// A stream cut in the middle of a packet yields `UnexpectedEof`.
pub fn read_packet(r: &mut impl Read) -> io::Result<Packet> {
    let mut header = [0u8; 12];
    r.read_exact(&mut header)?;
    let head = u64::from_be_bytes(header[..8].try_into().unwrap());
    let tail = u32::from_be_bytes(header[8..].try_into().unwrap());

    if head & FLAG_SESSION != 0 {
        // Bytes 0-3 are flags, 4-7 the width, 8-11 the height.
        return Ok(Packet::Session { width: head as u32, height: tail });
    }

    if tail > MAX_PACKET_SIZE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("packet of {tail} bytes")));
    }
    let mut data = vec![0u8; tail as usize];
    r.read_exact(&mut data)?;

    if head & FLAG_CONFIG != 0 {
        Ok(Packet::Config(data))
    } else {
        Ok(Packet::Frame { pts_us: head & PTS_MASK, key_frame: head & FLAG_KEY_FRAME != 0, data })
    }
}

/// Client-to-device messages on the control socket. Only the camera ones are needed here;
/// type numbers are from `ControlMessage.java` in scrcpy 4.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlMessage {
    SetTorch(bool),
    ZoomIn,
    ZoomOut,
}

impl ControlMessage {
    pub fn serialize(&self) -> Vec<u8> {
        match *self {
            ControlMessage::SetTorch(on) => vec![18, on as u8],
            ControlMessage::ZoomIn => vec![19],
            ControlMessage::ZoomOut => vec![20],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(w: u32, h: u32) -> Vec<u8> {
        let mut v = vec![0x80, 0, 0, 0];
        v.extend(w.to_be_bytes());
        v.extend(h.to_be_bytes());
        v
    }

    fn media(flags_and_pts: u64, data: &[u8]) -> Vec<u8> {
        let mut v = flags_and_pts.to_be_bytes().to_vec();
        v.extend((data.len() as u32).to_be_bytes());
        v.extend(data);
        v
    }

    #[test]
    fn parses_session_config_and_frames() {
        let mut s = session(1920, 1080);
        s.extend(media(FLAG_CONFIG, &[0, 0, 0, 1, 0x67]));
        s.extend(media(FLAG_KEY_FRAME | 33_333, &[0, 0, 0, 1, 0x65, 1, 2]));
        s.extend(media(66_666, &[0, 0, 0, 1, 0x41]));
        let mut r = s.as_slice();

        assert_eq!(read_packet(&mut r).unwrap(), Packet::Session { width: 1920, height: 1080 });
        assert_eq!(read_packet(&mut r).unwrap(), Packet::Config(vec![0, 0, 0, 1, 0x67]));
        assert_eq!(
            read_packet(&mut r).unwrap(),
            Packet::Frame { pts_us: 33_333, key_frame: true, data: vec![0, 0, 0, 1, 0x65, 1, 2] }
        );
        assert_eq!(
            read_packet(&mut r).unwrap(),
            Packet::Frame { pts_us: 66_666, key_frame: false, data: vec![0, 0, 0, 1, 0x41] }
        );
        assert_eq!(read_packet(&mut r).unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn session_packet_with_client_resized_flag() {
        let mut s = session(1080, 1920);
        s[3] = 1;
        assert_eq!(read_packet(&mut s.as_slice()).unwrap(), Packet::Session { width: 1080, height: 1920 });
    }

    #[test]
    fn truncated_stream_is_an_error_not_a_panic() {
        let full = media(FLAG_KEY_FRAME, &[1; 100]);
        for cut in [0, 5, 11, 12, 50, 111] {
            let err = read_packet(&mut &full[..cut]).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof, "cut at {cut}");
        }
    }

    #[test]
    fn absurd_packet_size_is_rejected() {
        let mut s = 0u64.to_be_bytes().to_vec();
        s.extend(u32::MAX.to_be_bytes());
        assert_eq!(read_packet(&mut s.as_slice()).unwrap_err().kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn device_meta() {
        let mut name = b"OnePlus PHK110".to_vec();
        name.resize(64, 0);
        name.extend(CODEC_H264.to_be_bytes());
        let mut r = name.as_slice();
        assert_eq!(read_device_name(&mut r).unwrap(), "OnePlus PHK110");
        assert_eq!(read_codec_id(&mut r).unwrap(), CODEC_H264);
    }

    // Same bytes as ControlMessageReaderTest.java (scrcpy 4.1).
    #[test]
    fn control_messages_match_scrcpy() {
        assert_eq!(ControlMessage::SetTorch(true).serialize(), [18, 1]);
        assert_eq!(ControlMessage::SetTorch(false).serialize(), [18, 0]);
        assert_eq!(ControlMessage::ZoomIn.serialize(), [19]);
        assert_eq!(ControlMessage::ZoomOut.serialize(), [20]);
    }
}
