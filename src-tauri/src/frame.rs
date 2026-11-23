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

/// How much of the picture to cut off on each side, as it is shown (after rotation and
/// mirroring), like OBS's crop filter. What is left is scaled to fill the frame with its own
/// aspect ratio. Kept in hundredths of a percent, so a rectangle drawn on the preview can match
/// the frame's aspect ratio closely; settings.json has plain percents (`12.5`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Crop {
    #[serde(serialize_with = "percent_out", deserialize_with = "lenient_crop")]
    pub left: u16,
    #[serde(serialize_with = "percent_out", deserialize_with = "lenient_crop")]
    pub right: u16,
    #[serde(serialize_with = "percent_out", deserialize_with = "lenient_crop")]
    pub top: u16,
    #[serde(serialize_with = "percent_out", deserialize_with = "lenient_crop")]
    pub bottom: u16,
}

/// Opposite sides together cut off at most this much (90%): a tenth of the picture stays.
pub const MAX_CROP: u16 = 9000;

fn lenient_crop<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    let v: Option<f64> = serde::Deserialize::deserialize(d)?;
    Ok(v.filter(|v| v.is_finite()).map_or(0, |v| (v * 100.0).round().clamp(0.0, MAX_CROP as f64) as u16))
}

fn percent_out<S: serde::Serializer>(v: &u16, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_f64(*v as f64 / 100.0)
}

/// Whether a `w`x`h` rectangle is within 1% of the `out_w`x`out_h` aspect ratio: then it is
/// stretched to fill the frame rather than leaving bars a few pixels wide.
fn fills(w: u32, h: u32, out_w: u32, out_h: u32) -> bool {
    let (a, b) = (w as u64 * out_h as u64, h as u64 * out_w as u64);
    a.abs_diff(b) * 100 <= a.max(b)
}

impl Crop {
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }

    /// From percents, e.g. `Crop::percent(12.5, 12.5, 0.0, 10.0)`.
    pub fn percent(left: f64, right: f64, top: f64, bottom: f64) -> Self {
        let c = |v: f64| (v * 100.0).round().clamp(0.0, MAX_CROP as f64) as u16;
        Self { left: c(left), right: c(right), top: c(top), bottom: c(bottom) }
    }

    /// Each side within 0..=90% and opposite sides together at most 90% (the second one gives).
    pub fn clamped(self) -> Self {
        let pair = |a: u16, b: u16| {
            let a = a.min(MAX_CROP);
            (a, b.min(MAX_CROP - a))
        };
        let (left, right) = pair(self.left, self.right);
        let (top, bottom) = pair(self.top, self.bottom);
        Self { left, right, top, bottom }
    }

    /// One word, for handing to the pipeline thread without a lock.
    pub fn to_bits(self) -> u64 {
        [self.left, self.right, self.top, self.bottom].iter().rev().fold(0, |bits, &v| bits << 16 | v as u64)
    }

    pub fn from_bits(bits: u64) -> Self {
        let side = |i: u32| (bits >> (16 * i)) as u16;
        Self { left: side(0), right: side(1), top: side(2), bottom: side(3) }
    }

    /// The same cut in the coordinates of the decoded picture, before it is turned clockwise
    /// by `rotation` and then mirrored.
    fn in_source(self, rotation: u16, mirror: bool) -> Self {
        let Self { left, right, top, bottom } = if mirror { Self { left: self.right, right: self.left, ..self } } else { self };
        match rotation {
            // Turned clockwise, the source's left edge ends up on top, its top on the right...
            90 => Self { left: top, top: right, right: bottom, bottom: left },
            180 => Self { left: right, right: left, top: bottom, bottom: top },
            270 => Self { left: bottom, top: left, right: top, bottom: right },
            _ => Self { left, right, top, bottom },
        }
    }

    /// The kept rectangle of a `w`x`h` NV12 picture: `(x, y, width, height)`, on even pixels
    /// so the half-resolution UV plane is cut at the same place.
    fn rect(self, w: u32, h: u32) -> (u32, u32, u32, u32) {
        if self.is_none() {
            return (0, 0, w, h);
        }
        let span = |size: u32, start: u16, end: u16| {
            if start == 0 && end == 0 {
                return (0, size);
            }
            let from = (size as u64 * start as u64 / 10_000) as u32 & !1;
            let to = size - (size as u64 * end as u64 / 10_000) as u32;
            let len = (to.saturating_sub(from) & !1).max(2).min(size - from);
            (from, len)
        };
        let (x, cw) = span(w, self.left, self.right);
        let (y, ch) = span(h, self.top, self.bottom);
        (x, y, cw, ch)
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
    /// Already cropped and scaled as `FrameConverter::prescale` asked (done on the GPU).
    pub prescaled: bool,
}

/// Crop and scale a decoder can do before handing the picture over (the GPU decoder does it
/// with the video processor, so the CPU gets a picture it does not have to scale): the
/// rectangle `x, y, width, height` of the decoded picture, stretched to `out_width` x
/// `out_height`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prescale {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub out_width: u32,
    pub out_height: u32,
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
    crop: Crop,
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
            crop: Crop::default(),
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

    /// Sides to cut off, in percent of the picture as shown.
    pub fn set_crop(&mut self, crop: Crop) {
        self.crop = crop.clamped();
    }

    /// Where a `w`x`h` picture (before turning) lands in the frame. A cropped one that is
    /// within 1% of the frame's aspect ratio fills it.
    fn place(&self, w: u32, h: u32, cropped: bool) -> Placement {
        let (w, h) = if matches!(self.rotation, 90 | 270) { (h, w) } else { (w, h) };
        if cropped && fills(w, h, self.out_width, self.out_height) {
            return Placement { width: self.out_width, height: self.out_height, x: 0, y: 0 };
        }
        fit(w, h, self.out_width, self.out_height)
    }

    /// What a decoder should do to a `width`x`height` picture before `convert`: crop it and
    /// scale it to the size it will have in the frame. `None` when nothing is cropped (the
    /// picture is then scaled on the CPU as before, if at all).
    pub fn prescale(&self, width: u32, height: u32) -> Option<Prescale> {
        let (x, y, w, h) = self.crop.in_source(self.rotation, self.mirror).rect(width, height);
        if (x, y, w, h) == (0, 0, width, height) {
            return None;
        }
        let place = self.place(w, h, true);
        let turned = matches!(self.rotation, 90 | 270);
        let (out_width, out_height) = if turned { (place.height, place.width) } else { (place.width, place.height) };
        // NV12 comes in 2x2 blocks.
        if out_width % 2 != 0 || out_height % 2 != 0 {
            return None;
        }
        Some(Prescale { x, y, width: w, height: h, out_width, out_height })
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
        // The part of the picture that is kept; all of it unless cropped (or it already was).
        let (cx, cy, w, h) = if frame.prescaled {
            (0, 0, frame.width, frame.height)
        } else {
            self.crop.in_source(self.rotation, self.mirror).rect(frame.width, frame.height)
        };
        let cropped = (cx, cy, w, h) != (0, 0, frame.width, frame.height);

        let turned = matches!(self.rotation, 90 | 270);
        let place = self.place(w, h, cropped);
        if self.placement != Some(place) {
            // Bars are only painted when the geometry changes; frames overwrite the picture area.
            self.out.fill(0);
            self.placement = Some(place);
        }

        // Work at the final size as early as possible: scale the NV12 planes (half the bytes
        // of BGR), convert the colors, and only then rotate the now small picture.
        let (sw, sh) = if turned { (place.height, place.width) } else { (place.width, place.height) };
        let scale = (sw, sh) != (w, h);
        // The scaler reads the kept rectangle straight from the planes, so cropping costs no
        // copy. Its filter also reads a few pixels past the rectangle, so it must stay clear
        // of the padding to the right of a padded picture (rows are the stride wide).
        let padded = frame.y_stride != frame.width || frame.uv_stride != frame.width.div_ceil(2) * 2;
        let reach = (w / sw.max(1)).max(h / sh.max(1)) + 2;
        let in_place = scale
            && (!padded || cx + w + 2 * reach <= frame.width)
            && frame.uv_stride % 2 == 0
            && frame.y.len() >= frame.y_stride as usize * frame.height as usize
            && frame.uv.len() >= frame.uv_stride as usize * frame.height.div_ceil(2) as usize;
        let (y_plane, uv_plane) = if in_place {
            let r = &mut self.resizer;
            let y_src = (frame.y_stride, frame.height);
            resize_plane(r, frame.y, y_src, (cx, cy, w, h), &mut self.scaled_y, (sw, sh), PixelType::U8)?;
            let uv_src = (frame.uv_stride / 2, frame.height.div_ceil(2));
            let uv_rect = (cx / 2, cy / 2, w.div_ceil(2), h.div_ceil(2));
            let uv_size = (sw.div_ceil(2), sh.div_ceil(2));
            resize_plane(r, frame.uv, uv_src, uv_rect, &mut self.scaled_uv, uv_size, PixelType::U8x2)?;
            (&self.scaled_y[..], &self.scaled_uv[..])
        } else {
            let (y_plane, uv_plane) = if frame.y_stride == w && frame.uv_stride == w && !cropped {
                (frame.y, frame.uv)
            } else {
                // yuvutils-rs 0.8 rejects an interleaved UV plane whose stride is wider than the
                // picture, which is what the decoder gives for 1080-wide video: stride 1088.
                // Only reached when the picture is not scaled (or its planes are odd).
                pack(&mut self.packed_y, frame.y, frame.y_stride, cx, cy, w, h);
                pack(&mut self.packed_uv, frame.uv, frame.uv_stride, cx, cy / 2, w.div_ceil(2) * 2, h.div_ceil(2));
                (&self.packed_y[..], &self.packed_uv[..])
            };
            if scale {
                let r = &mut self.resizer;
                resize_plane(r, y_plane, (w, h), (0, 0, w, h), &mut self.scaled_y, (sw, sh), PixelType::U8)?;
                let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
                let uv_size = (sw.div_ceil(2), sh.div_ceil(2));
                resize_plane(r, uv_plane, (cw, ch), (0, 0, cw, ch), &mut self.scaled_uv, uv_size, PixelType::U8x2)?;
                (&self.scaled_y[..], &self.scaled_uv[..])
            } else {
                (y_plane, uv_plane)
            }
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

/// Scales the `rect` (x, y, width, height) of a plane of `pixel`s, `src_size` pixels wide
/// (the stride) and high, into `dst` at `dst_size`.
fn resize_plane(
    resizer: &mut Resizer,
    src: &[u8],
    src_size: (u32, u32),
    rect: (u32, u32, u32, u32),
    dst: &mut Vec<u8>,
    dst_size: (u32, u32),
    pixel: PixelType,
) -> Result<(), FrameError> {
    let err = |e: &dyn std::fmt::Display| FrameError::Resize(e.to_string());
    dst.resize(dst_size.0 as usize * dst_size.1 as usize * pixel.size(), 0);
    let src = ImageRef::new(src_size.0, src_size.1, src, pixel).map_err(|e| err(&e))?;
    let mut dst = Image::from_slice_u8(dst_size.0, dst_size.1, dst, pixel).map_err(|e| err(&e))?;
    let mut options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear));
    if rect != (0, 0, src_size.0, src_size.1) {
        options = options.crop(rect.0 as f64, rect.1 as f64, rect.2 as f64, rect.3 as f64);
    }
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

/// Copies `rows` rows of `row_len` bytes, starting at byte `x` of row `y`, from a plane with
/// `stride` into a tight buffer.
fn pack(dst: &mut Vec<u8>, src: &[u8], stride: u32, x: u32, y: u32, row_len: u32, rows: u32) {
    dst.clear();
    for row in src.chunks(stride as usize).skip(y as usize).take(rows as usize) {
        dst.extend_from_slice(&row[x as usize..(x + row_len) as usize]);
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
        Nv12Frame { width: w, height: h, y, y_stride: w, uv, uv_stride: w, color: ColorInfo::default(), prescaled: false }
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
        let f = Nv12Frame { width: w, height: h, y: &y, y_stride: stride, uv: &uv, uv_stride: stride, color: ColorInfo::default(), prescaled: false };
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

    /// 16x8 picture of four bands of Y: 0..4 = 40, 4..8 = 80, 8..12 = 160, 12..16 = 240.
    fn bands() -> (Vec<u8>, Vec<u8>) {
        let (w, h) = (16u32, 8u32);
        let y = (0..w * h).map(|i| [40u8, 80, 160, 240][(i % w / 4) as usize]).collect();
        (y, vec![128u8; (w * h / 2) as usize])
    }

    fn gray(out: &[u8], out_w: u32, x: u32, y: u32) -> u8 {
        pixel(out, out_w, x, y)[1]
    }

    #[test]
    fn crop_keeps_only_the_rest_and_fills_the_frame() {
        let (y, uv) = bands();
        // Cut the outer bands off: 8x8 of the two middle ones, scaled to fill an 8x8 frame.
        let mut conv = FrameConverter::new(8, 8);
        conv.set_crop(Crop::percent(25.0, 25.0, 0.0, 0.0));
        let out = conv.convert(&frame(16, 8, &y, &uv)).unwrap().to_vec();
        assert_eq!(conv.placement, Some(Placement { width: 8, height: 8, x: 0, y: 0 }));
        assert!(gray(&out, 8, 0, 4).abs_diff(80) <= 3, "{}", gray(&out, 8, 0, 4));
        assert!(gray(&out, 8, 7, 4).abs_diff(160) <= 3, "{}", gray(&out, 8, 7, 4));

        // The kept part keeps its shape: 8x8 into a 16x8 frame is pillarboxed.
        let mut conv = FrameConverter::new(16, 8);
        conv.set_crop(Crop::percent(25.0, 25.0, 0.0, 0.0));
        conv.convert(&frame(16, 8, &y, &uv)).unwrap();
        assert_eq!(conv.placement, Some(Placement { width: 8, height: 8, x: 4, y: 0 }));
    }

    #[test]
    fn crop_follows_the_picture_as_shown() {
        let (y, uv) = bands();
        // Mirrored, the brightest band is on the left; cropping "left" removes it.
        let mut conv = FrameConverter::new(12, 8);
        conv.set_mirror(true);
        conv.set_crop(Crop::percent(25.0, 0.0, 0.0, 0.0));
        let out = conv.convert(&frame(16, 8, &y, &uv)).unwrap();
        assert!(gray(out, 12, 0, 4).abs_diff(160) <= 3, "{}", gray(out, 12, 0, 4));
        assert!(gray(out, 12, 11, 4).abs_diff(40) <= 3, "{}", gray(out, 12, 11, 4));

        // Turned 90 degrees clockwise, the source's left (darkest) band is on top; cropping "top"
        // removes it, so the top row of the 8x12 frame is the 80 band.
        let mut conv = FrameConverter::new(8, 12);
        conv.set_rotation(90);
        conv.set_crop(Crop::percent(0.0, 0.0, 25.0, 0.0));
        let out = conv.convert(&frame(16, 8, &y, &uv)).unwrap();
        assert!(gray(out, 8, 4, 0).abs_diff(80) <= 3, "{}", gray(out, 8, 4, 0));
        assert!(gray(out, 8, 4, 11).abs_diff(240) <= 3, "{}", gray(out, 8, 4, 11));

        // Turned 270, the source's left band is at the bottom.
        let mut conv = FrameConverter::new(8, 12);
        conv.set_rotation(270);
        conv.set_crop(Crop::percent(0.0, 0.0, 0.0, 25.0));
        let out = conv.convert(&frame(16, 8, &y, &uv)).unwrap();
        assert!(gray(out, 8, 4, 11).abs_diff(80) <= 3, "{}", gray(out, 8, 4, 11));
        assert!(gray(out, 8, 4, 0).abs_diff(240) <= 3, "{}", gray(out, 8, 4, 0));
    }

    #[test]
    fn scaling_a_padded_picture_ignores_the_padding() {
        // 40x4 white in stride-48 planes with dark garbage in the padding, scaled down: whole
        // (copied first) and cropped (straight from the planes, or copied near the padding).
        let (w, h, stride) = (40u32, 4u32, 48u32);
        let y: Vec<u8> = (0..stride * h).map(|i| if i % stride < w { 255 } else { 0 }).collect();
        let uv: Vec<u8> = (0..stride * h / 2).map(|i| if i % stride < w { 128 } else { 0 }).collect();
        let f = Nv12Frame { width: w, height: h, y: &y, y_stride: stride, uv: &uv, uv_stride: stride, color: ColorInfo::default(), prescaled: false };
        let crops = [Crop::default(), Crop::percent(20.0, 0.0, 10.0, 0.0), Crop::percent(0.0, 50.0, 0.0, 0.0)];
        for crop in crops {
            let mut conv = FrameConverter::new(10, 2);
            conv.set_crop(crop);
            let out = conv.convert(&f).unwrap().to_vec();
            let p = conv.placement.unwrap();
            for x in p.x..p.x + p.width {
                assert!(close(pixel(&out, 10, x, p.y), [255, 255, 255]), "{crop:?} x={x}: {:?}", pixel(&out, 10, x, p.y));
            }
        }
    }

    #[test]
    fn prescaled_pictures_are_not_cropped_again() {
        let mut conv = FrameConverter::new(16, 8);
        assert_eq!(conv.prescale(32, 16), None);
        conv.set_crop(Crop::percent(25.0, 25.0, 0.0, 0.0));
        let p = conv.prescale(32, 16).unwrap();
        assert_eq!(p, Prescale { x: 8, y: 0, width: 16, height: 16, out_width: 8, out_height: 8 });

        // The decoder hands over the middle bands already cropped and scaled: 8x8 of 80 | 160.
        let y: Vec<u8> = (0..64).map(|i| if i % 8 < 4 { 80 } else { 160 }).collect();
        let uv = vec![128u8; 32];
        let f = Nv12Frame { prescaled: true, ..frame(8, 8, &y, &uv) };
        let out = conv.convert(&f).unwrap().to_vec();
        assert_eq!(conv.placement, Some(Placement { width: 8, height: 8, x: 4, y: 0 }));
        assert!(gray(&out, 16, 4, 4).abs_diff(80) <= 3 && gray(&out, 16, 11, 4).abs_diff(160) <= 3);

        // Turned, the prescaled size is before turning.
        conv.set_rotation(90);
        conv.set_crop(Crop::percent(0.0, 0.0, 50.0, 0.0));
        assert_eq!(conv.prescale(32, 16), Some(Prescale { x: 16, y: 0, width: 16, height: 16, out_width: 8, out_height: 8 }));
    }

    #[test]
    fn a_crop_close_to_the_frame_shape_fills_it() {
        let (y, uv) = solid(1920, 1080, [128, 128, 128]);
        let mut conv = FrameConverter::new(1920, 1080);
        // 12.5% off each side is exactly 16:9; 12.4/12.6 is a few pixels off, still filled.
        for crop in [Crop::percent(12.5, 12.5, 12.5, 12.5), Crop::percent(12.4, 12.6, 12.5, 12.5)] {
            conv.set_crop(crop);
            conv.convert(&frame(1920, 1080, &y, &uv)).unwrap();
            assert_eq!(conv.placement, Some(Placement { width: 1920, height: 1080, x: 0, y: 0 }), "{crop:?}");
            assert_eq!(conv.prescale(1920, 1080).map(|p| (p.out_width, p.out_height)), Some((1920, 1080)));
        }
        // A clearly different shape keeps its aspect ratio.
        conv.set_crop(Crop::percent(25.0, 25.0, 0.0, 0.0));
        conv.convert(&frame(1920, 1080, &y, &uv)).unwrap();
        assert_eq!(conv.placement, Some(Placement { width: 960, height: 1080, x: 480, y: 0 }));
    }

    #[test]
    fn crop_rect_is_even_and_inside() {
        let c = Crop::percent(10.0, 33.0, 1.0, 0.0);
        assert_eq!(c.rect(1920, 1080), (192, 10, 1094, 1070));
        assert_eq!(Crop::percent(90.0, 0.0, 0.0, 0.0).rect(7, 5), (6, 0, 1, 5));
        assert_eq!(Crop::default().rect(1921, 1081), (0, 0, 1921, 1081));
        let wide = Crop::percent(50.0, 50.0, 80.0, 80.0).clamped();
        assert_eq!(wide, Crop::percent(50.0, 40.0, 80.0, 10.0));
        let odd: Crop = serde_json::from_str(r#"{"left": 120, "right": -3, "top": 12.625, "bottom": null}"#).unwrap();
        assert_eq!(odd, Crop { left: 9000, right: 0, top: 1263, bottom: 0 });
        assert_eq!(serde_json::to_string(&odd).unwrap(), r#"{"left":90.0,"right":0.0,"top":12.63,"bottom":0.0}"#);
        assert_eq!(Crop::from_bits(wide.to_bits()), wide);
        assert_eq!(Crop::from_bits(odd.to_bits()), odd);
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
