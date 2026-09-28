//! Decoded NV12 picture → BGR frame of the virtual camera: color conversion, scaling to fit
//! with black bars, optional picture adjustments, rotation and mirroring.

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

/// Simple picture adjustments, each from -100 to 100. All zero leaves the picture untouched
/// and costs nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ColorAdjust {
    /// Lifts or deepens the mid tones; black stays black and white stays white.
    #[serde(deserialize_with = "lenient_percent")]
    pub brightness: i8,
    #[serde(deserialize_with = "lenient_percent")]
    pub contrast: i8,
    /// -100 is black and white.
    #[serde(deserialize_with = "lenient_percent")]
    pub saturation: i8,
    /// Positive is warmer (towards orange), negative cooler (towards blue).
    #[serde(deserialize_with = "lenient_percent")]
    pub warmth: i8,
}

/// Any number (or null) reads as a value from -100 to 100, so one odd value in settings.json
/// does not throw all the other settings away.
fn lenient_percent<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i8, D::Error> {
    let v: Option<f64> = serde::Deserialize::deserialize(d)?;
    Ok(v.filter(|v| v.is_finite()).map_or(0, |v| v.round().clamp(-100.0, 100.0) as i8))
}

impl ColorAdjust {
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    pub fn clamped(self) -> Self {
        let c = |v: i8| v.clamp(-100, 100);
        Self { brightness: c(self.brightness), contrast: c(self.contrast), saturation: c(self.saturation), warmth: c(self.warmth) }
    }

    /// Four bytes in one word, for handing to the pipeline thread without a lock.
    pub fn to_bits(self) -> u32 {
        u32::from_le_bytes([self.brightness as u8, self.contrast as u8, self.saturation as u8, self.warmth as u8])
    }

    pub fn from_bits(bits: u32) -> Self {
        let [b, c, s, w] = bits.to_le_bytes().map(|v| v as i8);
        Self { brightness: b, contrast: c, saturation: s, warmth: w }
    }
}

/// Lookup tables for `ColorAdjust`: one for Y (unless it stays as it is), one each for U and V.
struct ColorLuts {
    key: (ColorAdjust, bool),
    y: Option<[u8; 256]>,
    u: [u8; 256],
    v: [u8; 256],
    /// U and V stay as they are, so the UV plane can be used unchanged.
    chroma_neutral: bool,
}

impl ColorLuts {
    fn new(adjust: ColorAdjust, full_range: bool) -> Self {
        // Nominal black and white of Y, and the reach of U/V around 128.
        let (black, white, chroma) = if full_range { (0.0, 255.0, 127.5) } else { (16.0, 235.0, 112.0) };
        let gamma = 2f32.powf(-adjust.brightness as f32 / 100.0);
        let contrast = 2f32.powf(adjust.contrast as f32 / 100.0);
        let y = (adjust.brightness != 0 || adjust.contrast != 0).then(|| {
            std::array::from_fn(|i| {
                let t = ((i as f32 - black) / (white - black)).clamp(0.0, 1.0);
                let t = (t.powf(gamma) - 0.5) * contrast + 0.5;
                (black + t * (white - black)).round().clamp(0.0, 255.0) as u8
            })
        });
        let saturation = 1.0 + adjust.saturation as f32 / 100.0;
        // At 100 a fifth of the full chroma reach: clearly warm, not orange.
        let shift = adjust.warmth as f32 / 100.0 * 0.2 * chroma;
        let chroma_lut = |shift: f32| -> [u8; 256] {
            std::array::from_fn(|i| (128.0 + (i as f32 - 128.0) * saturation + shift).round().clamp(0.0, 255.0) as u8)
        };
        Self {
            key: (adjust, full_range),
            y,
            u: chroma_lut(-shift),
            v: chroma_lut(shift),
            chroma_neutral: adjust.saturation == 0 && adjust.warmth == 0,
        }
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
    /// Clockwise degrees: 0, 90, 180 or 270.
    rotation: u16,
    placement: Option<Placement>,
    color: ColorAdjust,
    luts: Option<ColorLuts>,
    adjusted_y: Vec<u8>,
    adjusted_uv: Vec<u8>,
    src_bgr: Vec<u8>,
    scaled_y: Vec<u8>,
    scaled_uv: Vec<u8>,
    out: Vec<u8>,
    /// Tightly packed copies of padded planes (see `convert`).
    packed_y: Vec<u8>,
    packed_uv: Vec<u8>,
    rotated: Vec<[u8; 3]>,
    resizer: Resizer,
}

impl FrameConverter {
    pub fn new(out_width: u32, out_height: u32) -> Self {
        Self {
            out_width,
            out_height,
            mirror: false,
            rotation: 0,
            placement: None,
            color: ColorAdjust::default(),
            luts: None,
            adjusted_y: Vec::new(),
            adjusted_uv: Vec::new(),
            src_bgr: Vec::new(),
            scaled_y: Vec::new(),
            scaled_uv: Vec::new(),
            out: vec![0; out_width as usize * out_height as usize * 3],
            packed_y: Vec::new(),
            packed_uv: Vec::new(),
            rotated: Vec::new(),
            resizer: Resizer::new(),
        }
    }

    pub fn set_mirror(&mut self, mirror: bool) {
        self.mirror = mirror;
    }

    /// Clockwise degrees; anything but 90, 180 or 270 means no rotation.
    pub fn set_rotation(&mut self, degrees: u16) {
        self.rotation = if matches!(degrees, 90 | 180 | 270) { degrees } else { 0 };
    }

    pub fn set_color(&mut self, adjust: ColorAdjust) {
        self.color = adjust.clamped();
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
        let (y_plane, uv_plane) = if frame.y_stride == w && frame.uv_stride == w {
            (frame.y, frame.uv)
        } else {
            // yuvutils-rs 0.8 rejects an interleaved UV plane whose stride is wider than the
            // picture, which is what the decoder gives for 1080-wide video: stride 1088.
            pack(&mut self.packed_y, frame.y, frame.y_stride, w, h);
            pack(&mut self.packed_uv, frame.uv, frame.uv_stride, w.div_ceil(2) * 2, h.div_ceil(2));
            (&self.packed_y[..], &self.packed_uv[..])
        };

        let turned = matches!(self.rotation, 90 | 270);
        let place = if turned { fit(h, w, self.out_width, self.out_height) } else { fit(w, h, self.out_width, self.out_height) };
        if self.placement != Some(place) {
            // Bars are only painted when the geometry changes; frames overwrite the picture area.
            self.out.fill(0);
            self.placement = Some(place);
        }

        // Work at the final size as early as possible: scale the NV12 planes (half the bytes
        // of BGR), convert the colors, and only then rotate the now small picture.
        let (sw, sh) = if turned { (place.height, place.width) } else { (place.width, place.height) };
        let (y_plane, uv_plane) = if (sw, sh) == (w, h) {
            (y_plane, uv_plane)
        } else {
            let r = &mut self.resizer;
            resize_plane(r, y_plane, (w, h), &mut self.scaled_y, (sw, sh), PixelType::U8)?;
            let (cw, ch, scw, sch) = (w.div_ceil(2), h.div_ceil(2), sw.div_ceil(2), sh.div_ceil(2));
            resize_plane(r, uv_plane, (cw, ch), &mut self.scaled_uv, (scw, sch), PixelType::U8x2)?;
            (&self.scaled_y[..], &self.scaled_uv[..])
        };

        // Adjust the colors on the scaled NV12 planes: fewer pixels than the source, and half
        // the bytes of BGR.
        let (y_plane, uv_plane) = if self.color.is_neutral() {
            (y_plane, uv_plane)
        } else {
            let key = (self.color, frame.color.full_range);
            if self.luts.as_ref().is_none_or(|l| l.key != key) {
                self.luts = Some(ColorLuts::new(key.0, key.1));
            }
            let luts = self.luts.as_ref().unwrap();
            let y_plane = match &luts.y {
                Some(lut) => {
                    self.adjusted_y.resize(y_plane.len(), 0);
                    for (dst, &src) in self.adjusted_y.iter_mut().zip(y_plane) {
                        *dst = lut[src as usize];
                    }
                    &self.adjusted_y[..]
                }
                None => y_plane,
            };
            let uv_plane = if luts.chroma_neutral {
                uv_plane
            } else {
                self.adjusted_uv.resize(uv_plane.len(), 0);
                for (dst, src) in self.adjusted_uv.chunks_exact_mut(2).zip(uv_plane.chunks_exact(2)) {
                    dst[0] = luts.u[src[0] as usize];
                    dst[1] = luts.v[src[1] as usize];
                }
                &self.adjusted_uv[..]
            };
            (y_plane, uv_plane)
        };

        self.src_bgr.resize(sw as usize * sh as usize * 3, 0);
        let yuv = YuvBiPlanarImage {
            y_plane,
            y_stride: sw,
            uv_plane,
            uv_stride: sw.div_ceil(2) * 2,
            width: sw,
            height: sh,
        };
        let range = if frame.color.full_range { YuvRange::Full } else { YuvRange::Limited };
        let matrix = if frame.color.bt709 { YuvStandardMatrix::Bt709 } else { YuvStandardMatrix::Bt601 };
        yuvutils_rs::yuv_nv12_to_bgr(&yuv, &mut self.src_bgr, sw * 3, range, matrix, YuvConversionMode::Balanced)?;

        let picture: &[u8] = if self.rotation == 0 {
            &self.src_bgr
        } else {
            let (pixels, _) = self.src_bgr.as_chunks::<3>();
            rotate(pixels, sw as usize, sw as usize, sh as usize, self.rotation, &mut self.rotated);
            self.rotated.as_flattened()
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

/// Scales a tight plane of `pixel`s from `src_size` into `dst` at `dst_size`.
fn resize_plane(
    resizer: &mut Resizer,
    src: &[u8],
    src_size: (u32, u32),
    dst: &mut Vec<u8>,
    dst_size: (u32, u32),
    pixel: PixelType,
) -> Result<(), FrameError> {
    let err = |e: &dyn std::fmt::Display| FrameError::Resize(e.to_string());
    dst.resize(dst_size.0 as usize * dst_size.1 as usize * pixel.size(), 0);
    let src = ImageRef::new(src_size.0, src_size.1, src, pixel).map_err(|e| err(&e))?;
    let mut dst = Image::from_slice_u8(dst_size.0, dst_size.1, dst, pixel).map_err(|e| err(&e))?;
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear));
    resizer.resize(&src, &mut dst, &options).map_err(|e| err(&e))
}

/// Rotates a `w`x`h` plane (rows `stride` elements apart) clockwise into a tight buffer.
/// Works in tiles so both the reads and the scattered writes stay in cache.
fn rotate<T: Copy + Default>(src: &[T], stride: usize, w: usize, h: usize, degrees: u16, dst: &mut Vec<T>) {
    const TILE: usize = 32;
    dst.clear();
    if degrees == 180 {
        // Upside down is the same pixels in reverse order: one linear pass.
        for row in src.chunks(stride).take(h).rev() {
            dst.extend(row[..w].iter().rev());
        }
        return;
    }
    dst.resize(w * h, T::default());
    for tile_y in (0..h).step_by(TILE) {
        for tile_x in (0..w).step_by(TILE) {
            for y in tile_y..(tile_y + TILE).min(h) {
                let row = &src[y * stride..y * stride + w];
                for x in tile_x..(tile_x + TILE).min(w) {
                    let i = if degrees == 90 { x * h + (h - 1 - y) } else { (w - 1 - x) * h + y };
                    dst[i] = row[x];
                }
            }
        }
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
    fn rotate_plane() {
        // 3x2 with stride 4:  1 2 3 .
        //                     4 5 6 .
        let src = [1u8, 2, 3, 0, 4, 5, 6, 0];
        let mut dst = Vec::new();
        rotate(&src, 4, 3, 2, 90, &mut dst);
        assert_eq!(dst, [4, 1, 5, 2, 6, 3]);
        rotate(&src, 4, 3, 2, 180, &mut dst);
        assert_eq!(dst, [6, 5, 4, 3, 2, 1]);
        rotate(&src, 4, 3, 2, 270, &mut dst);
        assert_eq!(dst, [3, 6, 2, 5, 1, 4]);
    }

    #[test]
    fn rotation_turns_the_picture() {
        // 8x4, left half white: turned 90 degrees clockwise into a 4x8 frame, white is on top.
        let (w, h) = (8u32, 4u32);
        let y: Vec<u8> = (0..w * h).map(|i| if i % w < w / 2 { 255 } else { 0 }).collect();
        let uv = vec![128u8; (w * h / 2) as usize];
        let mut conv = FrameConverter::new(h, w);
        conv.set_rotation(90);
        let out = conv.convert(&frame(w, h, &y, &uv)).unwrap().to_vec();
        assert!(close(pixel(&out, h, 1, 1), [255, 255, 255]));
        assert!(close(pixel(&out, h, 1, 6), [0, 0, 0]));
        conv.set_rotation(270);
        let out = conv.convert(&frame(w, h, &y, &uv)).unwrap().to_vec();
        assert!(close(pixel(&out, h, 1, 1), [0, 0, 0]));
        assert!(close(pixel(&out, h, 1, 6), [255, 255, 255]));

        // 180 keeps the size and moves the white half to the right.
        let mut conv = FrameConverter::new(w, h);
        conv.set_rotation(180);
        let out = conv.convert(&frame(w, h, &y, &uv)).unwrap();
        assert!(close(pixel(out, w, 0, 1), [0, 0, 0]));
        assert!(close(pixel(out, w, 7, 1), [255, 255, 255]));
    }

    #[test]
    fn neutral_adjustments_change_nothing() {
        let (y, uv) = solid(8, 4, [76, 85, 255]);
        let mut conv = FrameConverter::new(8, 4);
        let plain = conv.convert(&frame(8, 4, &y, &uv)).unwrap().to_vec();
        conv.set_color(ColorAdjust { saturation: 30, ..Default::default() });
        assert_ne!(conv.convert(&frame(8, 4, &y, &uv)).unwrap(), plain);
        conv.set_color(ColorAdjust::default());
        assert_eq!(conv.convert(&frame(8, 4, &y, &uv)).unwrap(), plain);
    }

    #[test]
    fn brightness_lifts_mid_tones_but_keeps_black_and_white() {
        let y = ColorLuts::new(ColorAdjust { brightness: 50, ..Default::default() }, true).y.unwrap();
        assert_eq!((y[0], y[255]), (0, 255));
        assert!(y[64] > 90 && y[128] > 150, "{} {}", y[64], y[128]);
        let limited = ColorLuts::new(ColorAdjust { brightness: 50, ..Default::default() }, false).y.unwrap();
        assert_eq!((limited[16], limited[235]), (16, 235));
    }

    #[test]
    fn saturation_and_warmth_move_the_chroma() {
        let gray = ColorLuts::new(ColorAdjust { saturation: -100, ..Default::default() }, true);
        assert!(gray.y.is_none());
        assert_eq!((gray.u[20], gray.v[240]), (128, 128));

        let warm = ColorLuts::new(ColorAdjust { warmth: 100, ..Default::default() }, true);
        assert!(warm.u[128] < 110 && warm.v[128] > 146, "{} {}", warm.u[128], warm.v[128]);
        // A light gray picture made warmer turns orange: more red than blue.
        let (y, uv) = solid(8, 4, [200, 128, 128]);
        let mut conv = FrameConverter::new(8, 4);
        conv.set_color(ColorAdjust { warmth: 100, ..Default::default() });
        let [b, _, r] = pixel(conv.convert(&frame(8, 4, &y, &uv)).unwrap(), 8, 1, 1);
        assert!(r > b + 40, "r {r} b {b}");
    }

    #[test]
    fn odd_color_values_are_clamped_not_rejected() {
        let c: ColorAdjust = serde_json::from_str(r#"{"brightness": 500, "contrast": -1000, "saturation": null, "warmth": 12.6}"#).unwrap();
        assert_eq!(c, ColorAdjust { brightness: 100, contrast: -100, saturation: 0, warmth: 13 });
    }

    #[test]
    fn color_adjust_round_trips_through_bits() {
        let c = ColorAdjust { brightness: -100, contrast: 7, saturation: 100, warmth: -1 };
        assert_eq!(ColorAdjust::from_bits(c.to_bits()), c);
    }

    /// `cargo test --release convert_speed -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn convert_speed() {
        let (w, h) = (1920u32, 1080u32);
        let (y, uv) = solid(w, h, [120, 100, 150]);
        let mut conv = FrameConverter::new(w, h);
        for deg in [0u16, 90, 180] {
            conv.set_rotation(deg);
            let t = std::time::Instant::now();
            for _ in 0..20 {
                conv.convert(&frame(w, h, &y, &uv)).unwrap();
            }
            println!("1080p at {deg}: {:.2} ms per frame", t.elapsed().as_secs_f64() * 1000.0 / 20.0);
        }
        conv.set_rotation(0);
        conv.set_color(ColorAdjust { brightness: 20, contrast: 10, saturation: 10, warmth: 10 });
        let t = std::time::Instant::now();
        for _ in 0..20 {
            conv.convert(&frame(w, h, &y, &uv)).unwrap();
        }
        println!("1080p with color adjustments: {:.2} ms per frame", t.elapsed().as_secs_f64() * 1000.0 / 20.0);
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
