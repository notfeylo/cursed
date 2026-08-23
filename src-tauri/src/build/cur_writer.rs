//! A `.cur` writer, written against the byte layout rather than against a crate.
//!
//! The two things hand-rolled converters get wrong, both handled here:
//!
//!  1. **`idType` is 2, not 1.** A `.cur` and a `.ico` are the same container;
//!     the type word is the only thing that distinguishes them.
//!  2. **A cursor has no planes or bit count in its directory entry.** The
//!     `wPlanes` and `wBitCount` fields are *reused* to carry the hotspot X and
//!     Y. Writing `1` and `32` there — as an icon writer would — produces a file
//!     Windows loads with its hotspot at (1, 32), which feels broken in a way
//!     users describe as "the click doesn't land where I point".
//!
//! Layout, in order:
//!
//! ```text
//! ICONDIR        6 bytes    reserved=0, type=2, count=N
//! ICONDIRENTRY   16 bytes   x N
//! image data     variable   x N  (BITMAPINFOHEADER + BGRA XOR + 1bpp AND)
//! ```

use crate::build::bitmap::Bitmap;
use crate::error::{AppError, AppResult};

/// One resolution inside a multi-resolution cursor.
#[derive(Debug, Clone)]
pub struct CursorImage {
    pub bitmap: Bitmap,
    /// Hotspot in pixels, in this image's own coordinate space.
    pub hotspot: (u16, u16),
}

impl CursorImage {
    pub fn new(bitmap: Bitmap, hotspot: (u16, u16)) -> Self {
        Self { bitmap, hotspot }
    }
}

const ICONDIR_SIZE: usize = 6;
const ICONDIRENTRY_SIZE: usize = 16;
const BITMAPINFOHEADER_SIZE: u32 = 40;

/// Stride of a 1-bpp AND mask row, padded to a 4-byte boundary as the DIB
/// format requires. Forgetting this padding is why some converters produce
/// cursors that render with a diagonal tear.
fn and_mask_stride(width: u32) -> usize {
    (width.div_ceil(32) * 4) as usize
}

/// Encodes one image as `BITMAPINFOHEADER` + XOR (colour) + AND (transparency).
fn encode_dib(image: &CursorImage) -> AppResult<Vec<u8>> {
    let bitmap = &image.bitmap;
    let (width, height) = (bitmap.width, bitmap.height);
    if width == 0 || height == 0 || width > 256 || height > 256 {
        return Err(AppError::invalid(format!(
            "a cursor image must be 1-256 px on each side, got {width}x{height}"
        )));
    }

    let stride = and_mask_stride(width);
    let xor_len = (width as usize) * (height as usize) * 4;
    let and_len = stride * (height as usize);
    let mut out = Vec::with_capacity(BITMAPINFOHEADER_SIZE as usize + xor_len + and_len);

    // ── BITMAPINFOHEADER ───────────────────────────────────────
    out.extend_from_slice(&BITMAPINFOHEADER_SIZE.to_le_bytes()); // biSize
    out.extend_from_slice(&(width as i32).to_le_bytes()); // biWidth
    // biHeight is DOUBLE the real height: the XOR and AND masks are stacked in
    // one image, and the header describes the pair, not the picture.
    out.extend_from_slice(&((height as i32) * 2).to_le_bytes()); // biHeight
    out.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    out.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    out.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    out.extend_from_slice(&((xor_len + and_len) as u32).to_le_bytes()); // biSizeImage
    out.extend_from_slice(&0i32.to_le_bytes()); // biXPelsPerMeter
    out.extend_from_slice(&0i32.to_le_bytes()); // biYPelsPerMeter
    out.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
    out.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant

    // ── XOR mask: BGRA, bottom-up ──────────────────────────────
    for y in (0..height).rev() {
        for x in 0..width {
            let [r, g, b, a] = bitmap.pixel(x, y);
            out.extend_from_slice(&[b, g, r, a]);
        }
    }

    // ── AND mask: 1 bpp, bottom-up, MSB is the leftmost pixel ──
    // A set bit means "leave the screen alone" — i.e. transparent. 32-bit
    // cursors are drawn from the alpha channel, but the AND mask still has to
    // agree, or the pointer picks up a black box in the handful of code paths
    // that fall back to it (some remote sessions, some legacy shells).
    for y in (0..height).rev() {
        let mut row = vec![0u8; stride];
        for x in 0..width {
            if bitmap.alpha(x, y) == 0 {
                row[(x / 8) as usize] |= 0x80 >> (x % 8);
            }
        }
        out.extend_from_slice(&row);
    }

    Ok(out)
}

/// Writes a complete multi-resolution `.cur` file.
///
/// Images may be supplied in any order; they are written largest-last so the
/// smallest — the one Windows reaches for most often — sits nearest the start
/// of the file.
pub fn write_cur(images: &[CursorImage]) -> AppResult<Vec<u8>> {
    if images.is_empty() {
        return Err(AppError::invalid("a cursor needs at least one image"));
    }
    if images.len() > u16::MAX as usize {
        return Err(AppError::invalid("too many images for one cursor file"));
    }

    let mut sorted: Vec<&CursorImage> = images.iter().collect();
    sorted.sort_by_key(|image| image.bitmap.width);

    let encoded: Vec<Vec<u8>> = sorted
        .iter()
        .map(|image| encode_dib(image))
        .collect::<AppResult<_>>()?;

    let mut out = Vec::new();

    // ── ICONDIR ────────────────────────────────────────────────
    out.extend_from_slice(&0u16.to_le_bytes()); // idReserved
    out.extend_from_slice(&2u16.to_le_bytes()); // idType: 2 = CURSOR
    out.extend_from_slice(&(sorted.len() as u16).to_le_bytes()); // idCount

    // ── ICONDIRENTRY x N ───────────────────────────────────────
    let mut offset = (ICONDIR_SIZE + ICONDIRENTRY_SIZE * sorted.len()) as u32;
    for (image, data) in sorted.iter().zip(&encoded) {
        let bitmap = &image.bitmap;
        // 256 is stored as 0: the field is one byte and 256 does not fit.
        let dim = |value: u32| -> u8 {
            if value >= 256 {
                0
            } else {
                value as u8
            }
        };

        out.push(dim(bitmap.width)); // bWidth
        out.push(dim(bitmap.height)); // bHeight
        out.push(0); // bColorCount (0 for >8bpp)
        out.push(0); // bReserved

        // The hotspot lives here. Not planes. Not bit count.
        out.extend_from_slice(&image.hotspot.0.to_le_bytes()); // wPlanes  <- hotspot X
        out.extend_from_slice(&image.hotspot.1.to_le_bytes()); // wBitCount <- hotspot Y

        out.extend_from_slice(&(data.len() as u32).to_le_bytes()); // dwBytesInRes
        out.extend_from_slice(&offset.to_le_bytes()); // dwImageOffset
        offset += data.len() as u32;
    }

    for data in encoded {
        out.extend_from_slice(&data);
    }
    Ok(out)
}

/// Builds the whole ladder of sizes from one master, scaling the hotspot with
/// it. The hotspot is carried as a normalised 0.0-1.0 pair precisely so this
/// works: an absolute pixel hotspot is correct at exactly one size (PRD §5.3).
/// How hard to sharpen a rendition, from how far it was shrunk to get there.
///
/// Scaled by the reduction rather than fixed, because the softness being
/// corrected is caused by the reduction. A master that is already near the
/// target size loses almost nothing and needs almost nothing put back;
/// a 500 px photograph at 24 px has been through a 20× low-pass and needs a
/// firm hand.
///
/// Capped well below 1.0. Past that an unsharp mask stops restoring contrast and
/// starts drawing bright rims along every edge, which at 16 px is the entire
/// image — trading blurry for crunchy is not a fix.
fn sharpen_for(master_px: u32, target_px: u32) -> f32 {
    if target_px == 0 || master_px <= target_px {
        return 0.0;
    }
    let reduction = master_px as f32 / target_px as f32;
    // Nothing below 2x: a gentle downscale is already sharp, and sharpening it
    // only adds noise.
    if reduction < 2.0 {
        return 0.0;
    }
    ((reduction - 2.0) / 10.0).clamp(0.0, 0.55)
}

pub fn build_multi_resolution(
    master: &Bitmap,
    hotspot_normalised: (f32, f32),
    sizes: &[u32],
    outline: bool,
) -> AppResult<Vec<CursorImage>> {
    let (hx, hy) = (
        hotspot_normalised.0.clamp(0.0, 1.0),
        hotspot_normalised.1.clamp(0.0, 1.0),
    );

    let mut images = Vec::with_capacity(sizes.len());
    for &size in sizes {
        if size == 0 || size > 256 {
            return Err(AppError::invalid(format!("{size} is not a valid cursor size")));
        }
        let scaled = master.resized(size, size)?.sharpened(sharpen_for(master.width, size));
        // The outline is drawn per size so it is always exactly one pixel wide.
        let finished = if outline {
            scaled.with_contrast_outline()
        } else {
            scaled
        };

        let max = size.saturating_sub(1) as f32;
        images.push(CursorImage::new(
            finished,
            (
                (hx * max).round().clamp(0.0, max) as u16,
                (hy * max).round().clamp(0.0, max) as u16,
            ),
        ));
    }
    Ok(images)
}

/// Every size a catalog cursor ships (PRD §5.1).
/// The resolutions baked into every static cursor.
///
/// Reaches down to 10 px because the size control does: a cursor asked for at
/// 10 px and only offered a 32 px image is downscaled by Windows, and a
/// downscale of an already-small glyph is mud.
///
/// **The top is 256 px because that is where `CursorBaseSize` stops, not where
/// our own slider does.** Ours offers 128; Windows' accessibility pointer-size
/// setting writes the same registry value and goes to 256, and
/// `scheme::write_base_size` has always clamped to 256 rather than 128. A
/// machine sitting at 200 px was therefore handed a 128 px bitmap and Windows
/// stretched it — bilinear, no premultiplication, no gamma — which is precisely
/// the "zoomed in and pixelated" pointer people report and could never be
/// reproduced by anyone whose pointer was the default size.
///
/// Raster imports do not automatically get the top rungs: `sizes_for_source`
/// still refuses to enlarge a source past `MAX_UPSCALE`. Vector packs get all
/// ten, because a rung rendered from an SVG costs nothing but the render.
///
/// The cost is cache, not install: packs ship as SVG and are rasterised into
/// `%APPDATA%\Cursed\cache`, where the two new rungs roughly quadruple a
/// cursor's bytes. Nothing in the installer changes.
pub const TARGET_SIZES: [u32; 10] = [10, 16, 24, 32, 48, 64, 96, 128, 192, 256];

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(size: u32) -> Bitmap {
        let mut bitmap = Bitmap::new(size, size);
        for y in 0..size {
            for x in 0..size {
                bitmap.set_pixel(x, y, [255, 255, 255, 255]);
            }
        }
        bitmap
    }

    fn read_u16(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([bytes[at], bytes[at + 1]])
    }

    fn read_u32(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }

    #[test]
    fn header_declares_a_cursor_not_an_icon() {
        let file = write_cur(&[CursorImage::new(solid(32), (0, 0))]).unwrap();
        assert_eq!(read_u16(&file, 0), 0, "idReserved");
        assert_eq!(read_u16(&file, 2), 2, "idType must be 2 for a cursor");
        assert_eq!(read_u16(&file, 4), 1, "idCount");
    }

    #[test]
    fn the_hotspot_lives_in_the_planes_and_bitcount_fields() {
        let file = write_cur(&[CursorImage::new(solid(32), (7, 11))]).unwrap();
        assert_eq!(read_u16(&file, ICONDIR_SIZE + 4), 7, "wPlanes carries X");
        assert_eq!(read_u16(&file, ICONDIR_SIZE + 6), 11, "wBitCount carries Y");
    }

    #[test]
    fn dib_height_is_doubled_for_the_stacked_masks() {
        let file = write_cur(&[CursorImage::new(solid(32), (0, 0))]).unwrap();
        let offset = read_u32(&file, ICONDIR_SIZE + 12) as usize;
        assert_eq!(read_u32(&file, offset), 40, "biSize");
        assert_eq!(read_u32(&file, offset + 4), 32, "biWidth");
        assert_eq!(read_u32(&file, offset + 8), 64, "biHeight = 2 x 32");
        assert_eq!(read_u16(&file, offset + 14), 32, "biBitCount");
        assert_eq!(read_u32(&file, offset + 16), 0, "BI_RGB");
    }

    #[test]
    fn a_256px_image_stores_its_dimensions_as_zero() {
        let file = write_cur(&[CursorImage::new(solid(256), (1, 1))]).unwrap();
        assert_eq!(file[ICONDIR_SIZE], 0, "bWidth for 256 is encoded as 0");
        assert_eq!(file[ICONDIR_SIZE + 1], 0, "bHeight for 256 is encoded as 0");
    }

    #[test]
    fn offsets_and_lengths_describe_the_actual_payloads() {
        let images: Vec<_> = [32u32, 48, 64]
            .iter()
            .map(|&s| CursorImage::new(solid(s), (0, 0)))
            .collect();
        let file = write_cur(&images).unwrap();

        let count = read_u16(&file, 4) as usize;
        assert_eq!(count, 3);
        let mut expected = (ICONDIR_SIZE + ICONDIRENTRY_SIZE * count) as u32;
        for i in 0..count {
            let entry = ICONDIR_SIZE + ICONDIRENTRY_SIZE * i;
            let len = read_u32(&file, entry + 8);
            let offset = read_u32(&file, entry + 12);
            assert_eq!(offset, expected, "entry {i} offset");
            assert!(offset as usize + len as usize <= file.len(), "entry {i} fits");
            expected += len;
        }
        assert_eq!(expected as usize, file.len(), "no trailing slack");
    }

    #[test]
    fn images_are_ordered_smallest_first_whatever_order_they_arrive_in() {
        let images = vec![
            CursorImage::new(solid(64), (0, 0)),
            CursorImage::new(solid(32), (0, 0)),
            CursorImage::new(solid(48), (0, 0)),
        ];
        let file = write_cur(&images).unwrap();
        assert_eq!(file[ICONDIR_SIZE], 32);
        assert_eq!(file[ICONDIR_SIZE + ICONDIRENTRY_SIZE], 48);
        assert_eq!(file[ICONDIR_SIZE + ICONDIRENTRY_SIZE * 2], 64);
    }

    #[test]
    fn and_mask_rows_are_padded_to_four_bytes() {
        assert_eq!(and_mask_stride(1), 4);
        assert_eq!(and_mask_stride(32), 4);
        assert_eq!(and_mask_stride(33), 8);
        assert_eq!(and_mask_stride(48), 8);
        assert_eq!(and_mask_stride(256), 32);
    }

    #[test]
    fn transparent_pixels_set_their_and_mask_bit() {
        let mut bitmap = Bitmap::new(8, 1);
        bitmap.set_pixel(0, 0, [255, 255, 255, 255]); // opaque
        let file = write_cur(&[CursorImage::new(bitmap, (0, 0))]).unwrap();
        let offset = read_u32(&file, ICONDIR_SIZE + 12) as usize;
        let and_offset = offset + 40 + 8 * 4;
        // Bit 7 (leftmost pixel) clear = opaque; the other seven set = transparent.
        assert_eq!(file[and_offset], 0b0111_1111);
    }

    #[test]
    fn hotspots_scale_with_every_generated_size() {
        let images = build_multi_resolution(&solid(256), (0.5, 0.25), &TARGET_SIZES, false).unwrap();
        assert_eq!(images.len(), TARGET_SIZES.len());
        for image in &images {
            let max = (image.bitmap.width - 1) as f32;
            assert_eq!(image.hotspot.0, (0.5 * max).round() as u16);
            assert_eq!(image.hotspot.1, (0.25 * max).round() as u16);
        }
    }

    #[test]
    fn empty_and_oversized_inputs_are_refused() {
        assert!(write_cur(&[]).is_err());
        let too_big = Bitmap::new(300, 300);
        assert!(write_cur(&[CursorImage::new(too_big, (0, 0))]).is_err());
    }

    /// The gate that matters: Windows' own loader is the only authority on
    /// whether these bytes are a cursor. Every structural assertion above is a
    /// guess until `LoadImageW` accepts the file (PRD §19 rule 3).
    #[test]
    fn a_generated_multi_resolution_cursor_loads_in_windows() {
        let images =
            build_multi_resolution(&solid(256), (0.06, 0.04), &TARGET_SIZES, true).unwrap();
        let bytes = write_cur(&images).unwrap();

        let dir = std::env::temp_dir().join("cursorforge-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roundtrip.cur");
        std::fs::write(&path, &bytes).unwrap();

        let loaded = crate::cursor::engine::verify_loadable(&path);
        let _ = std::fs::remove_file(&path);
        assert!(loaded.is_ok(), "Windows refused the file: {loaded:?}");
    }

    /// A cursor with real transparency exercises the AND mask, which is the part
    /// a solid block would never touch.
    #[test]
    fn a_partially_transparent_cursor_loads_in_windows() {
        let mut art = Bitmap::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                if x + y > 30 && x + y < 90 {
                    art.set_pixel(x, y, [255, 255, 255, 255]);
                }
            }
        }
        let images = build_multi_resolution(&art, (0.5, 0.5), &[32, 48, 64], false).unwrap();
        let bytes = write_cur(&images).unwrap();

        let dir = std::env::temp_dir().join("cursorforge-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roundtrip-alpha.cur");
        std::fs::write(&path, &bytes).unwrap();

        let loaded = crate::cursor::engine::verify_loadable(&path);
        let _ = std::fs::remove_file(&path);
        assert!(loaded.is_ok(), "Windows refused the file: {loaded:?}");
    }
}
