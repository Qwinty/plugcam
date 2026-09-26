//! Decoded NV12 picture → BGR frame of the virtual camera: color conversion, scaling to fit
//! with black bars, and optional mirroring.

use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use yuvutils_rs::{YuvBiPlanarImage, YuvConversionMode, YuvRange, YuvStandardMatrix};

/// How the YUV values are to be read. Phones (OnePlus 11R included) send full-range BT.601,
/// which is also the fallback when the decoder does not report it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorInfo {
    pub full_range: bool,
    pub bt709: bool,
}

impl Default for ColorInfo {
    fn default() -> Self {
        Self { full_range: true, bt709: false }
    }
}

/// A borrowed NV12 picture: Y plane, then interleaved UV at half resolution.
pub struct Nv12Frame<'a> {
    pub width: u32,
    pub height: u32,
    pub y: &'a [u8],
    pub y_stride: u32,
    pub uv: &'a [u8],
    pub uv_stride: u32,
    pub color: ColorInfo,
}

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("color conversion: {0}")]
    Yuv(#[from] yuvutils_rs::YuvError),
    #[error("scaling: {0}")]
    Resize(String),
}

/// Where the picture lands inside the output frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub width: u32,
    pub height: u32,
    pub x: u32,
    pub y: u32,
}

/// Largest rectangle with the source aspect ratio that fits the output, centered.
pub fn fit(src_w: u32, src_h: u32, out_w: u32, out_h: u32) -> Placement {
    let (w, h) = if src_w as u64 * out_h as u64 > src_h as u64 * out_w as u64 {
        (out_w, ((src_h as u64 * out_w as u64 + src_w as u64 / 2) / src_w as u64) as u32)
    } else {
        (((src_w as u64 * out_h as u64 + src_h as u64 / 2) / src_h as u64) as u32, out_h)
    };
    let (w, h) = (w.clamp(1, out_w), h.clamp(1, out_h));
    Placement { width: w, height: h, x: (out_w - w) / 2, y: (out_h - h) / 2 }
}

pub struct FrameConverter {
    out_width: u32,
    out_height: u32,
    mirror: bool,
    placement: Option<Placement>,
    src_bgr: Vec<u8>,
    scaled: Vec<u8>,
    out: Vec<u8>,
    /// Tightly packed copies of padded planes (see `convert`).
    packed_y: Vec<u8>,
    packed_uv: Vec<u8>,
    resizer: Resizer,
}

impl FrameConverter {
    pub fn new(out_width: u32, out_height: u32) -> Self {
        Self {
            out_width,
            out_height,
            mirror: false,
            placement: None,
            src_bgr: Vec::new(),
            scaled: Vec::new(),
            out: vec![0; out_width as usize * out_height as usize * 3],
            packed_y: Vec::new(),
            packed_uv: Vec::new(),
            resizer: Resizer::new(),
        }
    }

    pub fn set_mirror(&mut self, mirror: bool) {
        self.mirror = mirror;
    }

    /// The last output frame (black until the first `convert`).
    pub fn output(&self) -> &[u8] {
        &self.out
    }

    /// Darkens the last output frame in place; shown while the phone is disconnected.
    pub fn dim(&mut self) -> &[u8] {
        self.out.iter_mut().for_each(|p| *p /= 3);
        &self.out
    }

    pub fn convert(&mut self, frame: &Nv12Frame) -> Result<&[u8], FrameError> {
        let (w, h) = (frame.width, frame.height);
        let place = fit(w, h, self.out_width, self.out_height);
        if self.placement != Some(place) {
            // Bars are only painted when the geometry changes; frames overwrite the picture area.
            self.out.fill(0);
            self.placement = Some(place);
        }

        self.src_bgr.resize(w as usize * h as usize * 3, 0);
        // yuvutils-rs 0.8 rejects an interleaved UV plane whose stride is wider than the picture,
        // which is what the decoder gives for 1080-wide (rotated) video: stride 1088.
        let (y_plane, uv_plane) = if frame.y_stride == w && frame.uv_stride == w {
            (frame.y, frame.uv)
        } else {
            pack(&mut self.packed_y, frame.y, frame.y_stride, w, h);
            pack(&mut self.packed_uv, frame.uv, frame.uv_stride, w.div_ceil(2) * 2, h.div_ceil(2));
            (&self.packed_y[..], &self.packed_uv[..])
        };
        let yuv = YuvBiPlanarImage {
            y_plane,
            y_stride: w,
            uv_plane,
            uv_stride: w.div_ceil(2) * 2,
            width: w,
            height: h,
        };
        let range = if frame.color.full_range { YuvRange::Full } else { YuvRange::Limited };
        let matrix = if frame.color.bt709 { YuvStandardMatrix::Bt709 } else { YuvStandardMatrix::Bt601 };
        yuvutils_rs::yuv_nv12_to_bgr(&yuv, &mut self.src_bgr, w * 3, range, matrix, YuvConversionMode::Balanced)?;

        let picture: &[u8] = if (place.width, place.height) == (w, h) {
            &self.src_bgr
        } else {
            self.scaled.resize(place.width as usize * place.height as usize * 3, 0);
            let src = ImageRef::new(w, h, &self.src_bgr, PixelType::U8x3).map_err(|e| FrameError::Resize(e.to_string()))?;
            let mut dst = Image::from_slice_u8(place.width, place.height, &mut self.scaled, PixelType::U8x3)
                .map_err(|e| FrameError::Resize(e.to_string()))?;
            let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear));
            self.resizer.resize(&src, &mut dst, &options).map_err(|e| FrameError::Resize(e.to_string()))?;
            &self.scaled
        };

        let row_len = place.width as usize * 3;
        let out_row_len = self.out_width as usize * 3;
        for (row_idx, row) in picture.chunks_exact(row_len).enumerate() {
            let start = (place.y as usize + row_idx) * out_row_len + place.x as usize * 3;
            let dst = &mut self.out[start..start + row_len];
            if self.mirror {
                for (d, s) in dst.chunks_exact_mut(3).zip(row.chunks_exact(3).rev()) {
                    d.copy_from_slice(s);
                }
            } else {
                dst.copy_from_slice(row);
            }
        }
        Ok(&self.out)
    }
}

/// Copies `rows` rows of `row_len` bytes from a plane with `stride` into a tight buffer.
fn pack(dst: &mut Vec<u8>, src: &[u8], stride: u32, row_len: u32, rows: u32) {
    dst.clear();
    for row in src.chunks(stride as usize).take(rows as usize) {
        dst.extend_from_slice(&row[..row_len as usize]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Solid NV12 picture of one YUV color.
    fn solid(w: u32, h: u32, yuv: [u8; 3]) -> (Vec<u8>, Vec<u8>) {
        let y = vec![yuv[0]; (w * h) as usize];
        let uv = [yuv[1], yuv[2]].repeat((w * h / 4) as usize);
        (y, uv)
    }

    fn frame<'a>(w: u32, h: u32, y: &'a [u8], uv: &'a [u8]) -> Nv12Frame<'a> {
        Nv12Frame { width: w, height: h, y, y_stride: w, uv, uv_stride: w, color: ColorInfo::default() }
    }

    fn pixel(buf: &[u8], out_w: u32, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * out_w + x) * 3) as usize;
        [buf[i], buf[i + 1], buf[i + 2]]
    }

    fn close(a: [u8; 3], b: [u8; 3]) -> bool {
        a.iter().zip(b).all(|(&x, y)| x.abs_diff(y) <= 3)
    }

    #[test]
    fn full_range_bt601_colors() {
        // (Y, U, V) → expected BGR, full-range BT.601 (JPEG) values.
        let cases = [
            ([128, 128, 128], [128, 128, 128]), // gray
            ([255, 128, 128], [255, 255, 255]), // white
            ([0, 128, 128], [0, 0, 0]),         // black
            ([76, 85, 255], [0, 0, 254]),       // red
            ([150, 44, 21], [1, 255, 0]),       // green
            ([29, 255, 107], [255, 0, 0]),      // blue
        ];
        for (yuv, bgr) in cases {
            let (y, uv) = solid(8, 4, yuv);
            let mut conv = FrameConverter::new(8, 4);
            let out = conv.convert(&frame(8, 4, &y, &uv)).unwrap();
            assert!(close(pixel(out, 8, 3, 2), bgr), "{yuv:?} → {:?}, want {bgr:?}", pixel(out, 8, 3, 2));
        }
    }

    #[test]
    fn limited_range_is_expanded() {
        let (y, uv) = solid(8, 4, [235, 128, 128]);
        let mut f = frame(8, 4, &y, &uv);
        f.color.full_range = false;
        let mut conv = FrameConverter::new(8, 4);
        assert!(close(pixel(conv.convert(&f).unwrap(), 8, 0, 0), [255, 255, 255]));
    }

    #[test]
    fn fit_letterbox_and_pillarbox() {
        assert_eq!(fit(1920, 1080, 1920, 1080), Placement { width: 1920, height: 1080, x: 0, y: 0 });
        assert_eq!(fit(1440, 1080, 1920, 1080), Placement { width: 1440, height: 1080, x: 240, y: 0 });
        assert_eq!(fit(1080, 1920, 1920, 1080), Placement { width: 608, height: 1080, x: 656, y: 0 });
        assert_eq!(fit(2560, 1080, 1920, 1080), Placement { width: 1920, height: 810, x: 0, y: 135 });
        assert_eq!(fit(1280, 720, 1920, 1080), Placement { width: 1920, height: 1080, x: 0, y: 0 });
    }

    #[test]
    fn pillarbox_bars_are_black() {
        // 4:3 white picture into a 16:9 frame: 2 px bars left and right of a 12 px picture.
        let (y, uv) = solid(12, 12, [255, 128, 128]);
        let mut conv = FrameConverter::new(16, 12);
        let out = conv.convert(&frame(12, 12, &y, &uv)).unwrap();
        assert_eq!(pixel(out, 16, 0, 6), [0, 0, 0]);
        assert_eq!(pixel(out, 16, 1, 6), [0, 0, 0]);
        assert!(close(pixel(out, 16, 2, 6), [255, 255, 255]));
        assert!(close(pixel(out, 16, 13, 6), [255, 255, 255]));
        assert_eq!(pixel(out, 16, 14, 6), [0, 0, 0]);
    }

    #[test]
    fn scaling_keeps_a_solid_color() {
        let (y, uv) = solid(64, 36, [76, 85, 255]);
        let mut conv = FrameConverter::new(32, 18);
        let out = conv.convert(&frame(64, 36, &y, &uv)).unwrap();
        assert!(close(pixel(out, 32, 16, 9), [0, 0, 254]));
    }

    #[test]
    fn padded_stride_is_handled() {
        // 12x4 picture in planes with stride 16, padding filled with garbage.
        let (w, h, stride) = (12u32, 4u32, 16u32);
        let y: Vec<u8> = (0..stride * h).map(|i| if i % stride < w { 255 } else { 7 }).collect();
        let uv: Vec<u8> = (0..stride * h / 2).map(|i| if i % stride < w { 128 } else { 7 }).collect();
        let f = Nv12Frame { width: w, height: h, y: &y, y_stride: stride, uv: &uv, uv_stride: stride, color: ColorInfo::default() };
        let mut conv = FrameConverter::new(w, h);
        let out = conv.convert(&f).unwrap();
        for x in 0..w {
            assert!(close(pixel(out, w, x, 3), [255, 255, 255]), "x={x}");
        }
    }

    #[test]
    fn mirror_swaps_left_and_right() {
        // Left half white, right half black.
        let (w, h) = (8u32, 4u32);
        let y: Vec<u8> = (0..w * h).map(|i| if i % w < w / 2 { 255 } else { 0 }).collect();
        let uv = vec![128u8; (w * h / 2) as usize];
        let mut conv = FrameConverter::new(w, h);
        let plain = conv.convert(&frame(w, h, &y, &uv)).unwrap().to_vec();
        conv.set_mirror(true);
        let mirrored = conv.convert(&frame(w, h, &y, &uv)).unwrap();
        assert!(close(pixel(&plain, w, 0, 1), [255, 255, 255]));
        assert!(close(pixel(&plain, w, 7, 1), [0, 0, 0]));
        assert!(close(pixel(mirrored, w, 0, 1), [0, 0, 0]));
        assert!(close(pixel(mirrored, w, 7, 1), [255, 255, 255]));
    }
}
