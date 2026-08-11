//! An RGBA bitmap and the handful of operations the cursor pipeline needs.
//!
//! Straight (non-premultiplied) alpha throughout, because that is what both the
//! `image` crate and the `.cur` DIB format use. The one place premultiplication
//! matters — downscaling — is handled inside [`Bitmap::resized`], since resizing
//! straight alpha is what produces the dark halos that make neon cursors look
//! dirty (PRD §5.6).

use crate::error::{AppError, AppResult};
use fast_image_resize::images::Image as FirImage;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};

/// sRGB → linear light, for all 256 encoded levels.
///
/// A table rather than a `powf` per channel: the input is a `u8`, so there are
/// only 256 possible answers, and the source of a resize can be sixteen
/// megapixels. The reverse direction is computed rather than tabulated because
/// it only ever runs over the *destination*, which is a cursor.
fn srgb_to_linear() -> &'static [f32; 256] {
    static TABLE: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0.0f32; 256];
        for (level, value) in table.iter_mut().enumerate() {
            let c = level as f32 / 255.0;
            *value = if c <= 0.040_448_237 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            };
        }
        table
    })
}

/// Linear light → an sRGB byte.
fn linear_to_srgb(value: f32) -> u8 {
    let v = value.clamp(0.0, 1.0);
    let encoded = if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    /// RGBA8, row-major, top-down.
    pub pixels: Vec<u8>,
}

impl Bitmap {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width as usize) * (height as usize) * 4],
        }
    }

    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> AppResult<Self> {
        let expected = (width as usize) * (height as usize) * 4;
        if pixels.len() != expected {
            return Err(AppError::invalid(format!(
                "pixel buffer is {} bytes, expected {expected}",
                pixels.len()
            )));
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    #[inline]
    fn index(&self, x: u32, y: u32) -> usize {
        ((y as usize) * (self.width as usize) + (x as usize)) * 4
    }

    #[inline]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = self.index(x, y);
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }

    #[inline]
    pub fn set_pixel(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        let i = self.index(x, y);
        self.pixels[i..i + 4].copy_from_slice(&rgba);
    }

    #[inline]
    pub fn alpha(&self, x: u32, y: u32) -> u8 {
        self.pixels[self.index(x, y) + 3]
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0 || self.pixels.iter().skip(3).step_by(4).all(|&a| a == 0)
    }

    /// Lanczos3 resample, in **linear light**, with premultiplied alpha.
    ///
    /// Resampling means averaging pixels, and an sRGB value is not a quantity of
    /// light — it is a quantity of light raised to roughly 1/2.2, which is how
    /// eight bits are made to cover a range the eye can use. Averaging those
    /// encoded numbers averages the wrong thing. Mid-grey in sRGB is 128, but
    /// half the light of white is 188: shrink a black-and-white checker in
    /// encoded space and it comes out at 128 — a full 60 levels too dark.
    ///
    /// Every edge in the image is that checker in miniature. Done in encoded
    /// space, a downscale darkens every boundary between light and dark, which
    /// is exactly the muddy, dirty look a photograph gets on the way to 32 px:
    /// highlights lose their brightness, thin bright details disappear into
    /// their surroundings, and the result reads as low quality without any one
    /// pixel being obviously wrong.
    ///
    /// So: decode to linear, resample there, encode back. The intermediate is
    /// 16-bit because 8-bit linear has visible steps in the darks — the whole
    /// reason sRGB is curved in the first place.
    ///
    /// Alpha is premultiplied for the resample and undone after. Without that,
    /// an edge pixel is averaged against transparent black and glowing artwork
    /// gains a dark halo (PRD §5.6). Alpha itself is *not* gamma-encoded and is
    /// scaled linearly.
    pub fn resized(&self, width: u32, height: u32) -> AppResult<Bitmap> {
        if width == 0 || height == 0 {
            return Err(AppError::invalid("cannot resize to a zero dimension"));
        }
        if width == self.width && height == self.height {
            return Ok(self.clone());
        }

        let to_linear = srgb_to_linear();
        let mut src = FirImage::new(self.width, self.height, PixelType::U16x4);
        for (pixel, out) in self
            .pixels
            .chunks_exact(4)
            .zip(src.buffer_mut().chunks_exact_mut(8))
        {
            let linear = [
                (to_linear[pixel[0] as usize] * 65535.0).round() as u16,
                (to_linear[pixel[1] as usize] * 65535.0).round() as u16,
                (to_linear[pixel[2] as usize] * 65535.0).round() as u16,
                // 257 == 65535 / 255 exactly, so 0 and 255 map to 0 and 65535.
                pixel[3] as u16 * 257,
            ];
            for (channel, bytes) in linear.iter().zip(out.chunks_exact_mut(2)) {
                bytes.copy_from_slice(&channel.to_ne_bytes());
            }
        }

        let mut dst = FirImage::new(width, height, PixelType::U16x4);
        let options = ResizeOptions::new()
            .resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3))
            .use_alpha(true);

        Resizer::new()
            .resize(&src, &mut dst, &options)
            .map_err(|e| AppError::msg(format!("resample failed: {e}")))?;

        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        for (out, chunk) in pixels
            .chunks_exact_mut(4)
            .zip(dst.buffer().chunks_exact(8))
        {
            let channel = |i: usize| -> u16 {
                u16::from_ne_bytes([chunk[i * 2], chunk[i * 2 + 1]])
            };
            out[0] = linear_to_srgb(channel(0) as f32 / 65535.0);
            out[1] = linear_to_srgb(channel(1) as f32 / 65535.0);
            out[2] = linear_to_srgb(channel(2) as f32 / 65535.0);
            out[3] = ((channel(3) as u32 + 128) / 257) as u8;
        }

        Bitmap::from_rgba(width, height, pixels)
    }

    /// Places this bitmap, unscaled, in the middle of a larger square canvas.
    ///
    /// This is how a role keeps its own size while Windows scales everything.
    /// `CursorBaseSize` is one global number — there is no per-role size — so
    /// the only way to draw a 32 px hand while the pointer is 128 px is to hand
    /// Windows a 128 px image with a 32 px hand in the middle of it. Windows
    /// scales the canvas; the glyph inside arrives at the size it was drawn.
    ///
    /// Returns self unchanged if the canvas is not larger, so this is safe to
    /// call unconditionally.
    pub fn centred_in(&self, canvas: u32) -> Bitmap {
        if canvas <= self.width || canvas <= self.height {
            return self.clone();
        }
        let mut out = Bitmap::new(canvas, canvas);
        let ox = (canvas - self.width) / 2;
        let oy = (canvas - self.height) / 2;
        for y in 0..self.height {
            for x in 0..self.width {
                out.set_pixel(ox + x, oy + y, self.pixel(x, y));
            }
        }
        out
    }

    /// Unsharp mask, for artwork that has just been shrunk a long way.
    ///
    /// A good downscale filter is a low-pass filter — that is what stops it
    /// aliasing — so shrinking a detailed photograph to 32 px necessarily throws
    /// away the high frequencies that read as *detail*. Lanczos3 already
    /// reconstructs some of that, but past roughly 4× the result still lands
    /// soft, and soft at cursor sizes reads as "blurry, low quality" because
    /// there are no longer enough pixels for the eye to infer an edge.
    ///
    /// The fix is the one every icon pipeline uses: subtract a blurred copy to
    /// put the local contrast back. It does not invent detail, it restores the
    /// contrast the low-pass removed.
    ///
    /// Done in **premultiplied** space. Sharpening straight RGB samples the
    /// colour of fully transparent pixels — which is arbitrary, and usually
    /// black — so every edge against transparency would gain a dark rim: exactly
    /// the halo the resize above is careful to avoid. Alpha is sharpened with it
    /// so the silhouette stays crisp rather than the artwork gaining a hard edge
    /// inside a soft outline.
    pub fn sharpened(&self, amount: f32) -> Bitmap {
        if amount <= 0.0 || self.width < 3 || self.height < 3 {
            return self.clone();
        }
        let (w, h) = (self.width, self.height);

        // Premultiply once, so both the blur and the recombination below are
        // working on values where a transparent pixel genuinely contributes
        // nothing rather than contributing black.
        let premultiplied: Vec<f32> = self
            .pixels
            .chunks_exact(4)
            .flat_map(|p| {
                let a = p[3] as f32 / 255.0;
                [p[0] as f32 * a, p[1] as f32 * a, p[2] as f32 * a, p[3] as f32]
            })
            .collect();

        // A 3×3 tent blur is the right radius here: at these sizes anything
        // wider stops being "local contrast" and starts ringing.
        const KERNEL: [f32; 9] = [1.0, 2.0, 1.0, 2.0, 4.0, 2.0, 1.0, 2.0, 1.0];
        const WEIGHT: f32 = 16.0;

        let mut out = vec![0u8; self.pixels.len()];
        for y in 0..h {
            for x in 0..w {
                let mut blurred = [0.0f32; 4];
                let mut k = 0;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let sx = (x as i32 + dx).clamp(0, w as i32 - 1) as u32;
                        let sy = (y as i32 + dy).clamp(0, h as i32 - 1) as u32;
                        let base = ((sy * w + sx) * 4) as usize;
                        for c in 0..4 {
                            blurred[c] += premultiplied[base + c] * KERNEL[k];
                        }
                        k += 1;
                    }
                }

                let base = ((y * w + x) * 4) as usize;
                let mut sharp = [0.0f32; 4];
                for c in 0..4 {
                    let original = premultiplied[base + c];
                    sharp[c] = original + amount * (original - blurred[c] / WEIGHT);
                }

                let alpha = sharp[3].clamp(0.0, 255.0);
                out[base + 3] = alpha.round() as u8;

                // Back to straight alpha.
                //
                // Each premultiplied channel is clamped to the alpha first, and
                // that clamp is doing real work rather than being defensive.
                // Colour and alpha are sharpened independently, so at an edge
                // where alpha was pulled *down* the colour can end up larger
                // than its own coverage — a state no premultiplied pixel can
                // legally be in. Dividing that by the smaller alpha sends it
                // straight to white, which is precisely what turned black
                // carbon fibre into a silver rim.
                let a = alpha / 255.0;
                for c in 0..3 {
                    out[base + c] = if a > 0.004 {
                        (sharp[c].clamp(0.0, alpha) / a).clamp(0.0, 255.0).round() as u8
                    } else {
                        // Under a pixel of coverage the division amplifies noise
                        // into confetti, so these keep their colour and let alpha
                        // carry the edge.
                        self.pixels[base + c]
                    };
                }
            }
        }

        Bitmap { width: w, height: h, pixels: out }
    }

    /// Bounding box of everything not fully transparent.
    pub fn opaque_bounds(&self) -> Option<(u32, u32, u32, u32)> {
        let (mut min_x, mut min_y) = (u32::MAX, u32::MAX);
        let (mut max_x, mut max_y) = (0u32, 0u32);
        let mut found = false;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.alpha(x, y) > 0 {
                    found = true;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
        found.then_some((min_x, min_y, max_x, max_y))
    }

    /// Drops fully transparent border rows and columns (PRD §6.1 step 2).
    pub fn trimmed(&self) -> Bitmap {
        let Some((min_x, min_y, max_x, max_y)) = self.opaque_bounds() else {
            return self.clone(); // entirely transparent: nothing to trim towards
        };
        if (min_x, min_y, max_x, max_y) == (0, 0, self.width - 1, self.height - 1) {
            return self.clone();
        }

        let width = max_x - min_x + 1;
        let height = max_y - min_y + 1;
        let mut out = Bitmap::new(width, height);
        for y in 0..height {
            for x in 0..width {
                out.set_pixel(x, y, self.pixel(min_x + x, min_y + y));
            }
        }
        out
    }

    /// Centres the artwork on a square canvas, preserving aspect ratio.
    /// Cursors are square by convention; letterboxing here keeps the hotspot
    /// maths in one coordinate system for the whole pipeline.
    pub fn squared(&self) -> Bitmap {
        if self.width == self.height {
            return self.clone();
        }
        let side = self.width.max(self.height);
        let mut out = Bitmap::new(side, side);
        let dx = (side - self.width) / 2;
        let dy = (side - self.height) / 2;
        for y in 0..self.height {
            for x in 0..self.width {
                out.set_pixel(x + dx, y + dy, self.pixel(x, y));
            }
        }
        out
    }

    /// Recolours a white/greyscale master to `tint`, keeping the master's own
    /// shading as a luminance multiplier. This is the catalog multiplier from
    /// PRD §7.1: 64 greyscale packs times any colour.
    pub fn tinted(&self, tint: [u8; 3]) -> Bitmap {
        let mut out = self.clone();
        for chunk in out.pixels.chunks_exact_mut(4) {
            if chunk[3] == 0 {
                continue;
            }
            // Rec. 709 luma of the master pixel drives how much tint survives.
            let luma = (0.2126 * chunk[0] as f32
                + 0.7152 * chunk[1] as f32
                + 0.0722 * chunk[2] as f32)
                / 255.0;
            chunk[0] = (tint[0] as f32 * luma).round().clamp(0.0, 255.0) as u8;
            chunk[1] = (tint[1] as f32 * luma).round().clamp(0.0, 255.0) as u8;
            chunk[2] = (tint[2] as f32 * luma).round().clamp(0.0, 255.0) as u8;
        }
        out
    }

    pub fn with_opacity(&self, opacity: f32) -> Bitmap {
        let factor = opacity.clamp(0.0, 1.0);
        let mut out = self.clone();
        for chunk in out.pixels.chunks_exact_mut(4) {
            chunk[3] = (chunk[3] as f32 * factor).round() as u8;
        }
        out
    }

    /// Adds a 1 px dark rim just outside the artwork.
    ///
    /// Applied *after* resampling so the outline is one device pixel at every
    /// size — an outline drawn before downscaling would vanish at 32 px and go
    /// chunky at 256 px. Without it, a dark cursor disappears on a white page.
    pub fn with_contrast_outline(&self) -> Bitmap {
        const THRESHOLD: u8 = 24;
        const OUTLINE: [u8; 3] = [4, 5, 8];

        let mut out = self.clone();
        for y in 0..self.height {
            for x in 0..self.width {
                if self.alpha(x, y) >= THRESHOLD {
                    continue; // already artwork
                }
                // Strongest neighbouring coverage decides the rim's opacity, so
                // the outline fades out exactly where the artwork does.
                let mut strongest = 0u8;
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        let nx = x as i64 + dx;
                        let ny = y as i64 + dy;
                        if nx < 0 || ny < 0 || nx >= self.width as i64 || ny >= self.height as i64 {
                            continue;
                        }
                        strongest = strongest.max(self.alpha(nx as u32, ny as u32));
                    }
                }
                if strongest >= THRESHOLD {
                    out.set_pixel(x, y, [OUTLINE[0], OUTLINE[1], OUTLINE[2], strongest]);
                }
            }
        }
        out
    }

    /// Grows the canvas by `pad` on every side so an outline drawn at the very
    /// edge of the artwork is not clipped by the canvas boundary.
    pub fn padded(&self, pad: u32) -> Bitmap {
        if pad == 0 {
            return self.clone();
        }
        let mut out = Bitmap::new(self.width + pad * 2, self.height + pad * 2);
        for y in 0..self.height {
            for x in 0..self.width {
                out.set_pixel(x + pad, y + pad, self.pixel(x, y));
            }
        }
        out
    }

    /// Composites `over` on top of `self` (both straight alpha, same size).
    pub fn composite(&self, over: &Bitmap) -> AppResult<Bitmap> {
        if self.width != over.width || self.height != over.height {
            return Err(AppError::invalid("composite requires matching dimensions"));
        }
        let mut out = self.clone();
        for i in (0..out.pixels.len()).step_by(4) {
            let sa = over.pixels[i + 3] as f32 / 255.0;
            if sa <= 0.0 {
                continue;
            }
            let da = out.pixels[i + 3] as f32 / 255.0;
            let oa = sa + da * (1.0 - sa);
            for c in 0..3 {
                let sc = over.pixels[i + c] as f32 / 255.0;
                let dc = out.pixels[i + c] as f32 / 255.0;
                let value = if oa > 0.0 {
                    (sc * sa + dc * da * (1.0 - sa)) / oa
                } else {
                    0.0
                };
                out.pixels[i + c] = (value * 255.0).round().clamp(0.0, 255.0) as u8;
            }
            out.pixels[i + 3] = (oa * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        Ok(out)
    }

    /// Encodes as a real PNG.
    ///
    /// Every imported image ends up here regardless of what it arrived as, so a
    /// JPEG, WebP, BMP or GIF frame becomes a genuine PNG with a true alpha
    /// channel rather than whatever lossy, palette-limited thing it started as.
    pub fn to_png(&self, compression: image::codecs::png::CompressionType) -> AppResult<Vec<u8>> {
        let mut png = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new_with_quality(
            &mut png,
            compression,
            image::codecs::png::FilterType::Adaptive,
        );
        image::ImageEncoder::write_image(
            encoder,
            &self.pixels,
            self.width,
            self.height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| AppError::msg(format!("PNG encoding failed: {e}")))?;
        Ok(png)
    }

    /// A `data:` URI for the hotspot picker and catalog tiles. Encoding a real
    /// PNG keeps the frontend free of any filesystem or asset-protocol access.
    pub fn to_png_data_uri(&self) -> AppResult<String> {
        let png = self.to_png(image::codecs::png::CompressionType::Fast)?;
        Ok(format!("data:image/png;base64,{}", base64(&png)))
    }
}

/// Base64 without a dependency — one table, one loop.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(triple >> 18) as usize & 63] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(triple >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[triple as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dot(width: u32, height: u32, x: u32, y: u32) -> Bitmap {
        let mut bitmap = Bitmap::new(width, height);
        bitmap.set_pixel(x, y, [255, 255, 255, 255]);
        bitmap
    }

    #[test]
    fn trim_reduces_to_the_opaque_box() {
        let trimmed = dot(16, 16, 5, 7).trimmed();
        assert_eq!((trimmed.width, trimmed.height), (1, 1));
        assert_eq!(trimmed.pixel(0, 0), [255, 255, 255, 255]);
    }

    #[test]
    fn a_fully_transparent_bitmap_survives_trimming_unchanged() {
        let empty = Bitmap::new(8, 8);
        let trimmed = empty.trimmed();
        assert_eq!((trimmed.width, trimmed.height), (8, 8));
    }

    #[test]
    fn squaring_centres_without_distorting() {
        let mut wide = Bitmap::new(8, 4);
        wide.set_pixel(0, 0, [1, 2, 3, 255]);
        let squared = wide.squared();
        assert_eq!((squared.width, squared.height), (8, 8));
        assert_eq!(squared.pixel(0, 2), [1, 2, 3, 255]);
    }

    #[test]
    fn tint_maps_white_to_the_colour_and_leaves_transparency_alone() {
        let mut bitmap = Bitmap::new(2, 1);
        bitmap.set_pixel(0, 0, [255, 255, 255, 255]);
        bitmap.set_pixel(1, 0, [0, 0, 0, 0]);
        let tinted = bitmap.tinted([0x2e, 0x8b, 0xff]);
        assert_eq!(tinted.pixel(0, 0), [0x2e, 0x8b, 0xff, 255]);
        assert_eq!(tinted.pixel(1, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn outline_wraps_the_artwork_and_never_overwrites_it() {
        let outlined = dot(3, 3, 1, 1).with_contrast_outline();
        assert_eq!(outlined.pixel(1, 1), [255, 255, 255, 255], "artwork intact");
        assert_eq!(outlined.alpha(0, 0), 255, "diagonal neighbour is rimmed");
        assert_eq!(outlined.alpha(0, 1), 255, "orthogonal neighbour is rimmed");
        assert!(outlined.pixel(0, 0)[0] < 32, "the rim is dark");
    }

    #[test]
    fn resize_preserves_a_solid_block_without_halos() {
        let mut block = Bitmap::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                block.set_pixel(x, y, [255, 255, 255, 255]);
            }
        }
        let small = block.resized(32, 32).unwrap();
        assert_eq!((small.width, small.height), (32, 32));
        assert_eq!(small.pixel(16, 16), [255, 255, 255, 255]);
    }

    /// The one measurement that says a resample is done in light rather than in
    /// encoded values, and the reason a photograph stops looking muddy at 32 px.
    ///
    /// A black-and-white checker is, physically, half the light of white. Half
    /// the light is **188** in sRGB, not 128 — the curve is not linear, so the
    /// midpoint of the encoding is nowhere near the midpoint of the light.
    /// Averaging the stored bytes gives 128 and darkens every edge in the image.
    #[test]
    fn downscaling_averages_light_and_not_encoded_values() {
        let mut checker = Bitmap::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let white = (x + y) % 2 == 0;
                let value = if white { 255 } else { 0 };
                checker.set_pixel(x, y, [value, value, value, 255]);
            }
        }

        let grey = checker.resized(1, 1).unwrap().pixel(0, 0)[0];
        assert!(
            (180..=196).contains(&grey),
            "a 50% checker resolved to {grey}; light-space averaging gives ~188, \
             encoded-space averaging gives ~128"
        );
    }

    #[test]
    fn resizing_keeps_the_ends_of_the_alpha_range_exact() {
        let mut b = Bitmap::new(16, 16);
        for y in 0..16 {
            for x in 0..16 {
                // Opaque red on the left half, fully transparent on the right.
                let opaque = x < 8;
                b.set_pixel(x, y, if opaque { [255, 0, 0, 255] } else { [0, 0, 0, 0] });
            }
        }
        let small = b.resized(8, 8).unwrap();
        assert_eq!(small.alpha(0, 4), 255, "opaque must stay opaque");
        assert_eq!(small.alpha(7, 4), 0, "empty must stay empty");
        assert_eq!(
            small.pixel(0, 4)[0],
            255,
            "the colour under full alpha must survive the round trip"
        );
    }

    #[test]
    fn base64_matches_the_reference_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}

/// Geometric and tonal edits a user can apply to their own artwork.
///
/// These live on `Bitmap` rather than in the pipeline because they are pure
/// pixel operations with no knowledge of cursors, and because every one of them
/// has to be exactly reversible — a user who flips twice must get the original
/// back, not a resampled approximation of it. Nothing here interpolates.
impl Bitmap {
    /// Mirrors left to right.
    pub fn flipped_h(&self) -> Bitmap {
        let mut out = Bitmap::new(self.width, self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                out.set_pixel(self.width - 1 - x, y, self.pixel(x, y));
            }
        }
        out
    }

    /// Mirrors top to bottom.
    pub fn flipped_v(&self) -> Bitmap {
        let mut out = Bitmap::new(self.width, self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                out.set_pixel(x, self.height - 1 - y, self.pixel(x, y));
            }
        }
        out
    }

    /// Rotates clockwise by a quarter turn at a time.
    ///
    /// Only right angles. An arbitrary angle needs resampling, and a cursor is
    /// small enough that resampling visibly softens every edge — which is the
    /// one thing a pointer cannot afford.
    pub fn rotated(&self, quarter_turns: u32) -> Bitmap {
        match quarter_turns % 4 {
            0 => self.clone(),
            1 => {
                let mut out = Bitmap::new(self.height, self.width);
                for y in 0..self.height {
                    for x in 0..self.width {
                        out.set_pixel(self.height - 1 - y, x, self.pixel(x, y));
                    }
                }
                out
            }
            2 => {
                let mut out = Bitmap::new(self.width, self.height);
                for y in 0..self.height {
                    for x in 0..self.width {
                        out.set_pixel(self.width - 1 - x, self.height - 1 - y, self.pixel(x, y));
                    }
                }
                out
            }
            _ => {
                let mut out = Bitmap::new(self.height, self.width);
                for y in 0..self.height {
                    for x in 0..self.width {
                        out.set_pixel(y, self.width - 1 - x, self.pixel(x, y));
                    }
                }
                out
            }
        }
    }

    /// Inverts colour, leaving alpha alone.
    ///
    /// Alpha is deliberately untouched: inverting it would turn the cursor's
    /// shape inside out and fill the transparent surround with solid colour.
    pub fn inverted(&self) -> Bitmap {
        let mut out = self.clone();
        for y in 0..self.height {
            for x in 0..self.width {
                let [r, g, b, a] = self.pixel(x, y);
                out.set_pixel(x, y, [255 - r, 255 - g, 255 - b, a]);
            }
        }
        out
    }

    /// Takes a rectangle, given in fractions of the image, clamped to its bounds.
    ///
    /// Fractions rather than pixels so a crop chosen on a preview means the same
    /// thing on the full-resolution master.
    pub fn cropped(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Bitmap {
        let to_x = |v: f32| ((v.clamp(0.0, 1.0) * self.width as f32) as u32).min(self.width);
        let to_y = |v: f32| ((v.clamp(0.0, 1.0) * self.height as f32) as u32).min(self.height);
        let (left, right) = (to_x(x0.min(x1)), to_x(x0.max(x1)));
        let (top, bottom) = (to_y(y0.min(y1)), to_y(y0.max(y1)));

        // A degenerate rectangle would produce a zero-sized bitmap, which every
        // downstream step would then have to guard against. Refuse instead.
        if right.saturating_sub(left) < 2 || bottom.saturating_sub(top) < 2 {
            return self.clone();
        }

        let (w, h) = (right - left, bottom - top);
        let mut out = Bitmap::new(w, h);
        for y in 0..h {
            for x in 0..w {
                out.set_pixel(x, y, self.pixel(left + x, top + y));
            }
        }
        out
    }
}

#[cfg(test)]
mod transform_tests {
    use super::*;

    fn marked() -> Bitmap {
        // One opaque pixel in the top-left corner, so every transform has an
        // unambiguous expected destination.
        let mut b = Bitmap::new(4, 6);
        b.set_pixel(0, 0, [255, 0, 0, 255]);
        b
    }

    #[test]
    fn flipping_twice_returns_the_original() {
        let b = marked();
        assert_eq!(b.flipped_h().flipped_h().pixels, b.pixels);
        assert_eq!(b.flipped_v().flipped_v().pixels, b.pixels);
    }

    #[test]
    fn a_flip_moves_the_mark_to_the_opposite_corner() {
        let b = marked();
        assert_eq!(b.flipped_h().pixel(3, 0), [255, 0, 0, 255]);
        assert_eq!(b.flipped_v().pixel(0, 5), [255, 0, 0, 255]);
    }

    #[test]
    fn four_quarter_turns_return_the_original() {
        let b = marked();
        let round = b.rotated(1).rotated(1).rotated(1).rotated(1);
        assert_eq!(round.width, b.width);
        assert_eq!(round.height, b.height);
        assert_eq!(round.pixels, b.pixels);
    }

    #[test]
    fn a_quarter_turn_swaps_the_axes() {
        let b = marked();
        let turned = b.rotated(1);
        assert_eq!((turned.width, turned.height), (6, 4));
        // Top-left goes to top-right on a clockwise turn.
        assert_eq!(turned.pixel(5, 0), [255, 0, 0, 255]);
    }

    #[test]
    fn inverting_leaves_alpha_alone() {
        let mut b = Bitmap::new(2, 2);
        b.set_pixel(0, 0, [10, 20, 30, 128]);
        let inverted = b.inverted();
        assert_eq!(inverted.pixel(0, 0), [245, 235, 225, 128]);
        // Transparent stays transparent — inverting alpha would fill the
        // surround with solid colour and destroy the silhouette.
        assert_eq!(inverted.pixel(1, 1)[3], 0);
    }

    #[test]
    fn a_crop_takes_the_rectangle_asked_for() {
        let mut b = Bitmap::new(10, 10);
        b.set_pixel(5, 5, [1, 2, 3, 255]);
        let c = b.cropped(0.4, 0.4, 0.8, 0.8);
        assert_eq!((c.width, c.height), (4, 4));
        assert_eq!(c.pixel(1, 1), [1, 2, 3, 255]);
    }

    #[test]
    fn a_degenerate_crop_is_refused_rather_than_returning_nothing() {
        let b = Bitmap::new(10, 10);
        let c = b.cropped(0.5, 0.5, 0.5, 0.5);
        assert_eq!((c.width, c.height), (10, 10), "unchanged rather than empty");
    }

    #[test]
    fn crop_coordinates_may_arrive_in_any_order_or_out_of_range() {
        let b = Bitmap::new(10, 10);
        let a = b.cropped(0.8, 0.8, 0.2, 0.2);
        let c = b.cropped(0.2, 0.2, 0.8, 0.8);
        assert_eq!((a.width, a.height), (c.width, c.height));
        // Out of range is clamped, not an error.
        assert_eq!(b.cropped(-1.0, -1.0, 2.0, 2.0).width, 10);
    }
}
