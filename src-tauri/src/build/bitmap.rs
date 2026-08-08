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

    /// Lanczos3 resample with premultiplied alpha handled by the resizer.
    pub fn resized(&self, width: u32, height: u32) -> AppResult<Bitmap> {
        if width == 0 || height == 0 {
            return Err(AppError::invalid("cannot resize to a zero dimension"));
        }
        if width == self.width && height == self.height {
            return Ok(self.clone());
        }

        let src = FirImage::from_vec_u8(self.width, self.height, self.pixels.clone(), PixelType::U8x4)
            .map_err(|e| AppError::invalid(format!("source image rejected: {e}")))?;
        let mut dst = FirImage::new(width, height, PixelType::U8x4);

        let options = ResizeOptions::new()
            .resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3))
            // Premultiply -> resample -> unpremultiply. Without this, edge pixels
            // blend against transparent black and glowing artwork gains a halo.
            .use_alpha(true);

        Resizer::new()
            .resize(&src, &mut dst, &options)
            .map_err(|e| AppError::msg(format!("resample failed: {e}")))?;

        Bitmap::from_rgba(width, height, dst.into_vec())
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

    /// A `data:` URI for the hotspot picker and catalog tiles. Encoding a real
    /// PNG keeps the frontend free of any filesystem or asset-protocol access.
    pub fn to_png_data_uri(&self) -> AppResult<String> {
        let mut png = Vec::new();
        {
            let encoder = image::codecs::png::PngEncoder::new_with_quality(
                &mut png,
                image::codecs::png::CompressionType::Fast,
                image::codecs::png::FilterType::Adaptive,
            );
            image::ImageEncoder::write_image(
                encoder,
                &self.pixels,
                self.width,
                self.height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| AppError::msg(format!("preview encoding failed: {e}")))?;
        }
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

    #[test]
    fn base64_matches_the_reference_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
