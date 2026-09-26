//! Small JPEG preview of the camera picture for the app window, at most 40 fps (a 30 fps
//! stream shows every frame) and only while the window is visible. The pipeline only copies
//! a frame into the slot; scaling and encoding happen on the preview thread so they never
//! delay the virtual camera.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};

const INTERVAL: Duration = Duration::from_millis(25);

#[derive(Default)]
pub struct PreviewSlot {
    enabled: AtomicBool,
    inner: Mutex<Slot>,
    ready: Condvar,
}

#[derive(Default)]
struct Slot {
    frame: Vec<u8>,
    width: u32,
    height: u32,
    fresh: bool,
    last: Option<Instant>,
}

impl PreviewSlot {
    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Called by the pipeline for every frame; copies at most 40 of them per second.
    pub fn offer(&self, bgr: &[u8], width: u32, height: u32) {
        if !self.is_enabled() {
            return;
        }
        let mut s = self.inner.lock().unwrap();
        if s.last.is_some_and(|t| t.elapsed() < INTERVAL) {
            return;
        }
        s.frame.clear();
        s.frame.extend_from_slice(bgr);
        (s.width, s.height, s.fresh, s.last) = (width, height, true, Some(Instant::now()));
        self.ready.notify_one();
    }

    /// Waits up to `timeout` for a new frame and hands it over, swapping buffers with `buf`.
    pub fn take(&self, buf: &mut Vec<u8>, timeout: Duration) -> Option<(u32, u32)> {
        let guard = self.inner.lock().unwrap();
        let (mut s, _) = self.ready.wait_timeout_while(guard, timeout, |s| !s.fresh).unwrap();
        if !s.fresh {
            return None;
        }
        s.fresh = false;
        std::mem::swap(buf, &mut s.frame);
        Some((s.width, s.height))
    }
}

pub struct PreviewEncoder {
    max_width: u32,
    resizer: Resizer,
    scaled: Vec<u8>,
}

impl PreviewEncoder {
    pub fn new(max_width: u32) -> Self {
        Self { max_width, resizer: Resizer::new(), scaled: Vec::new() }
    }

    /// Scales a BGR frame down to `max_width` (keeping the aspect ratio) and encodes it as JPEG.
    pub fn encode(&mut self, bgr: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
        let (w, h) = if width > self.max_width {
            (self.max_width, (height as u64 * self.max_width as u64 / width as u64).max(1) as u32)
        } else {
            (width, height)
        };
        let pixels: &[u8] = if (w, h) == (width, height) {
            bgr
        } else {
            self.scaled.resize(w as usize * h as usize * 3, 0);
            let src = ImageRef::new(width, height, bgr, PixelType::U8x3).map_err(|e| e.to_string())?;
            let mut dst = Image::from_slice_u8(w, h, &mut self.scaled, PixelType::U8x3).map_err(|e| e.to_string())?;
            let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear));
            self.resizer.resize(&src, &mut dst, &options).map_err(|e| e.to_string())?;
            &self.scaled
        };
        let mut jpeg = Vec::with_capacity(64 * 1024);
        jpeg_encoder::Encoder::new(&mut jpeg, 75)
            .encode(pixels, w as u16, h as u16, jpeg_encoder::ColorType::Bgr)
            .map_err(|e| e.to_string())?;
        Ok(jpeg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_passes_frames_only_when_enabled_and_throttles() {
        let slot = PreviewSlot::default();
        let mut buf = Vec::new();
        slot.offer(&[1; 12], 2, 2);
        assert_eq!(slot.take(&mut buf, Duration::ZERO), None);

        slot.set_enabled(true);
        slot.offer(&[1; 12], 2, 2);
        slot.offer(&[2; 12], 2, 2); // too soon, dropped
        assert_eq!(slot.take(&mut buf, Duration::ZERO), Some((2, 2)));
        assert_eq!(buf, [1; 12]);
        assert_eq!(slot.take(&mut buf, Duration::ZERO), None);
    }

    #[test]
    fn encodes_a_scaled_jpeg() {
        let bgr = vec![128u8; 1920 * 1080 * 3];
        let jpeg = PreviewEncoder::new(640).encode(&bgr, 1920, 1080).unwrap();
        assert_eq!(&jpeg[..2], [0xFF, 0xD8]);
        // SOF0 carries height then width.
        let sof = jpeg.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
        let h = u16::from_be_bytes([jpeg[sof + 5], jpeg[sof + 6]]);
        let w = u16::from_be_bytes([jpeg[sof + 7], jpeg[sof + 8]]);
        assert_eq!((w, h), (640, 360));
    }
}
