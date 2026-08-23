//! Reading `.cur`, `.ico` and `.ani` files back into pixels, from bytes alone.
//!
//! [`cur_reader`](crate::build::cur_reader) already reads a cursor by asking
//! Windows to draw it, which is the right answer when there is a *path* and one
//! frame is enough. Neither is true on the import path: the decoder is handed
//! bytes, and an animated cursor dropped on the window has to arrive as every
//! frame with its own delay, or a `.ani` becomes a still picture of its first
//! frame.
//!
//! It also cannot go through Windows for a second reason. `LoadImageW` with an
//! explicit size returns one frame of a `.ani` and `CopyIcon` returns a still —
//! the trap documented in `docs/CURSOR_FORMAT.md` — so the only way to get the
//! whole animation is to parse the container.
//!
//! ```text
//! ICO / CUR                          ANI
//! ICONDIR       6 bytes              RIFF <size> ACON
//! ICONDIRENTRY  16 bytes x N           anih <36>   frame count, steps, rate
//! image data    PNG, or a DIB          rate <4*N>  per-step delay   (optional)
//!               (BITMAPINFOHEADER      seq  <4*N>  playback order   (optional)
//!                + XOR + 1bpp AND)     LIST <size> fram
//!                                        icon <size> <a whole .cur file>  x N
//! ```
//!
//! **Every read here is bounds-checked against the buffer rather than against
//! what the file claims.** These are the untrusted-input rules from PRD §13.5:
//! the header of a cursor file is a set of offsets and lengths written by
//! somebody else, and `fuzz::decoding_an_image_never_panics` hammers this path
//! with mutated bytes. A slice index that trusts a declared length is a panic in
//! a release build, from a file a user dragged onto the window.

use crate::build::bitmap::Bitmap;
use crate::error::{AppError, AppResult};

/// The `.ani` format counts in sixtieths of a second.
const JIFFIES_PER_SECOND: u32 = 60;
/// What Windows uses when an `.ani` declares no rate at all: 6 jiffies, 100 ms.
const DEFAULT_JIFFIES: u32 = 6;

/// A frame's delay is clamped to the same range the GIF path uses, so an
/// animation converted from either format behaves identically downstream.
const MIN_DELAY_MS: u32 = 10;
const MAX_DELAY_MS: u32 = 2_000;

/// Icons and cursors are at most 256 px square by the format's own rules. The
/// ceiling is generous rather than exact: a few tools write larger PNG frames,
/// and there is no reason to refuse art we can use.
const MAX_ICON_DIMENSION: u32 = 1_024;

/// How many frames of an `.ani` are decoded before the rest are dropped.
///
/// Matches `pipeline::MAX_SOURCE_FRAMES`, and is checked here as well because
/// this module decodes each frame — a 10,000-step `seq` must not become 10,000
/// decodes on the way to a cap applied afterwards.
const MAX_FRAMES: usize = 240;

/// One image out of a `.cur` or `.ico`.
#[derive(Debug, Clone)]
pub struct IconImage {
    pub bitmap: Bitmap,
    /// Present only for a `.cur`: an icon has no hotspot, and the fields that
    /// would carry one hold the plane count and bit depth instead.
    pub hotspot: Option<(u16, u16)>,
}

/// `00 00 01 00` is an icon, `00 00 02 00` a cursor. Both are the same
/// container; the type word is the only thing that separates them.
pub fn looks_like_an_icon(bytes: &[u8]) -> bool {
    matches!(bytes, [0x00, 0x00, 0x01 | 0x02, 0x00, ..])
}

/// `RIFF....ACON`. The four bytes in between are the file size and are not
/// checked here — a truncated animation is caught where it is parsed.
pub fn looks_like_an_ani(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"ACON"
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn i32_at(bytes: &[u8], offset: usize) -> Option<i32> {
    u32_at(bytes, offset).map(|value| value as i32)
}

/// A directory entry, as read rather than as declared.
#[derive(Debug, Clone, Copy)]
struct Entry {
    /// Declared width and height. Zero means 256 in this format, and both are
    /// only a hint: the image's own header is what the pixels are read against.
    width: u32,
    height: u32,
    bit_count: u16,
    hotspot: Option<(u16, u16)>,
    offset: usize,
    length: usize,
}

/// Reads the directory of a `.cur` or `.ico`.
fn directory(bytes: &[u8]) -> AppResult<Vec<Entry>> {
    if !looks_like_an_icon(bytes) {
        return Err(AppError::invalid("that is not a cursor or icon file"));
    }
    let is_cursor = bytes.get(2) == Some(&0x02);
    let count = u16_at(bytes, 4).unwrap_or(0) as usize;
    if count == 0 {
        return Err(AppError::invalid("this cursor file contains no images"));
    }

    let mut entries = Vec::with_capacity(count.min(64));
    for index in 0..count {
        let base = 6 + index * 16;
        // A count is a claim. A file that says it holds forty images and stops
        // after two is not an error worth refusing over — what was actually
        // written is decoded and the rest ignored.
        let Some(raw) = bytes.get(base..base + 16) else {
            break;
        };
        let width = if raw[0] == 0 { 256 } else { raw[0] as u32 };
        let height = if raw[1] == 0 { 256 } else { raw[1] as u32 };
        let planes_or_x = u16_at(raw, 4).unwrap_or(0);
        let bits_or_y = u16_at(raw, 6).unwrap_or(0);
        let length = u32_at(raw, 8).unwrap_or(0) as usize;
        let offset = u32_at(raw, 12).unwrap_or(0) as usize;

        // Offsets and lengths point into this file and nowhere else.
        if offset < 6 || length == 0 || offset.saturating_add(length) > bytes.len() {
            continue;
        }
        entries.push(Entry {
            width,
            height,
            // For a cursor these two fields carry the hotspot, so there is no
            // bit count to read: it comes from the image's own header instead.
            bit_count: if is_cursor { 0 } else { bits_or_y },
            hotspot: is_cursor.then_some((planes_or_x, bits_or_y)),
            offset,
            length,
        });
    }

    if entries.is_empty() {
        return Err(AppError::invalid(
            "this cursor file's directory does not point at any image inside it",
        ));
    }
    Ok(entries)
}

/// Decodes the best image in a `.cur` or `.ico`.
///
/// **Largest wins, and that is deliberate.** Everything downstream resamples to
/// the eight cursor sizes, so the entry to start from is the one carrying the
/// most detail — importing a 48 px cursor and rebuilding its ladder from the
/// 16 px entry would be a visible, permanent quality loss for no reason.
pub fn decode_icon(bytes: &[u8]) -> AppResult<IconImage> {
    let mut ordered = directory(bytes)?;
    ordered.sort_by_key(|entry| (entry.width * entry.height, entry.bit_count));

    let mut last: Option<AppError> = None;
    for entry in ordered.iter().rev() {
        let Some(data) = bytes.get(entry.offset..entry.offset + entry.length) else {
            continue;
        };
        match decode_entry(data, *entry) {
            Ok(bitmap) => {
                return Ok(IconImage {
                    bitmap,
                    hotspot: entry.hotspot,
                })
            }
            // A file can carry one entry a decoder chokes on and three it does
            // not — a 256 px PNG frame beside four ordinary DIBs is the common
            // shape. Falling through to the next-largest is the difference
            // between importing such a file and refusing it.
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| AppError::invalid("no image inside this cursor file could be read")))
}

fn decode_entry(data: &[u8], entry: Entry) -> AppResult<Bitmap> {
    // Vista onwards, a directory entry may hold a whole PNG rather than a DIB.
    if data.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        let decoded = image::load_from_memory_with_format(data, image::ImageFormat::Png)
            .map_err(|e| AppError::invalid(format!("unreadable image inside the cursor: {e}")))?;
        let rgba = decoded.to_rgba8();
        guard(rgba.width(), rgba.height())?;
        return Bitmap::from_rgba(rgba.width(), rgba.height(), rgba.into_raw());
    }
    decode_dib(data, entry)
}

fn guard(width: u32, height: u32) -> AppResult<()> {
    if width == 0 || height == 0 {
        return Err(AppError::invalid(
            "an image inside this cursor file has no pixels",
        ));
    }
    if width > MAX_ICON_DIMENSION || height > MAX_ICON_DIMENSION {
        return Err(AppError::invalid(format!(
            "an image inside this cursor file is {width}x{height}, which is larger than a cursor \
             can be"
        )));
    }
    Ok(())
}

/// Decodes the DIB half of an icon entry: a header, a colour bitmap, and a
/// 1-bpp transparency mask stacked underneath it.
///
/// The two things that make this different from reading a `.bmp`: there is no
/// `BITMAPFILEHEADER`, and **`biHeight` is doubled** because it describes the
/// XOR and AND masks together rather than the picture.
fn decode_dib(data: &[u8], entry: Entry) -> AppResult<Bitmap> {
    let header_size = u32_at(data, 0).unwrap_or(0) as usize;
    if header_size < 40 || header_size > data.len() {
        // 12 is a BITMAPCOREHEADER, which no icon writer has produced this
        // century. Anything smaller is not a header at all.
        return Err(AppError::invalid(
            "an image inside this cursor file has a header this app does not recognise",
        ));
    }

    let width = i32_at(data, 4).unwrap_or(0);
    let declared_height = i32_at(data, 8).unwrap_or(0);
    let bit_count = u16_at(data, 14).unwrap_or(0);
    let compression = u32_at(data, 16).unwrap_or(0);
    let colours_used = u32_at(data, 32).unwrap_or(0) as usize;

    if width <= 0 || declared_height == 0 {
        return Err(AppError::invalid(
            "an image inside this cursor file has no pixels",
        ));
    }
    // BI_RGB, or BI_BITFIELDS on a 32-bit image where the masks are the usual
    // ones. Compressed icons (BI_RLE8, JPEG-in-DIB) are not a thing Windows
    // writes and are refused rather than guessed at.
    if compression != 0 && compression != 3 {
        return Err(AppError::invalid(
            "an image inside this cursor file is compressed in a way this app cannot read",
        ));
    }

    // A negative height means the rows run top-down; the format's own doubling
    // of a positive height is what says the AND mask is present.
    let top_down = declared_height < 0;
    let stated = declared_height.unsigned_abs();
    let (height, mut has_mask) = if !top_down && stated % 2 == 0 && stated / 2 >= entry.height.min(stated) {
        (stated / 2, true)
    } else {
        (stated, false)
    };
    let width = width as u32;
    guard(width, height)?;

    let mut palette_offset = header_size;
    if compression == 3 {
        // Three colour masks sit between the header and the pixels.
        palette_offset = palette_offset.saturating_add(12);
    }

    let palette: Vec<[u8; 4]> = if bit_count <= 8 && bit_count > 0 {
        let entries = if colours_used == 0 {
            1usize << bit_count
        } else {
            colours_used.min(256)
        };
        let mut table = Vec::with_capacity(entries);
        for index in 0..entries {
            let at = palette_offset + index * 4;
            match data.get(at..at + 4) {
                // Stored BGRA, and the fourth byte is reserved rather than
                // alpha — transparency in a palette icon comes from the mask.
                Some(rgb) => table.push([rgb[2], rgb[1], rgb[0], 255]),
                None => table.push([0, 0, 0, 255]),
            }
        }
        table
    } else {
        Vec::new()
    };

    let pixels_offset = palette_offset + palette.len() * 4;
    let xor_stride = stride(width, bit_count as u32);
    let and_stride = stride(width, 1);
    let xor_len = xor_stride.saturating_mul(height as usize);
    let and_len = and_stride.saturating_mul(height as usize);
    if xor_stride == 0 {
        return Err(AppError::invalid(
            "an image inside this cursor file declares no colour depth",
        ));
    }

    let available = data.len().saturating_sub(pixels_offset);
    if available < xor_len {
        return Err(AppError::invalid(
            "an image inside this cursor file stops before its pixels do",
        ));
    }
    // A file that declared a mask but did not write one is common enough to be
    // worth surviving: the image comes back opaque rather than refused.
    if has_mask && available < xor_len + and_len {
        has_mask = false;
    }

    let mut bitmap = Bitmap::new(width, height);
    for y in 0..height {
        // DIB rows run bottom-up unless the height was negative.
        let row = if top_down { y } else { height - 1 - y };
        let row_at = pixels_offset + (row as usize) * xor_stride;
        let Some(row_bytes) = data.get(row_at..row_at + xor_stride) else {
            continue;
        };
        for x in 0..width {
            bitmap.set_pixel(x, y, sample(row_bytes, x, bit_count, &palette));
        }
    }

    let mask_at = pixels_offset + xor_len;
    if has_mask {
        if bit_count == 1 {
            // **A monochrome cursor is not a picture with a mask.** It is a
            // pair of masks with four states, and one of them is "invert the
            // screen": that is how a crosshair stays visible on any background,
            // and it is what `cross_l.cur`, `lcross.cur` and `libeam.cur` — all
            // shipped by Windows — are made of almost entirely.
            //
            // Read with the ordinary rule, every one of those pixels is
            // transparent, so the whole cursor is, and the import fails with
            // "the image is completely transparent". Four of Windows' own
            // cursors could not be imported for exactly that reason.
            apply_monochrome_masks(
                &mut bitmap,
                data,
                (pixels_offset, xor_stride),
                (mask_at, and_stride),
                top_down,
            );
        } else {
            apply_mask(&mut bitmap, data, mask_at, and_stride, top_down, false);
        }
    }

    // A 32-bit icon whose alpha channel is entirely zero is not an invisible
    // icon — it is one written before alpha was used, whose transparency lives
    // in the mask alone. Read literally it imports as nothing at all.
    if bit_count == 32 && bitmap.pixels.iter().skip(3).step_by(4).all(|&a| a == 0) {
        if has_mask {
            apply_mask(&mut bitmap, data, mask_at, and_stride, top_down, true);
        } else {
            for index in (3..bitmap.pixels.len()).step_by(4) {
                bitmap.pixels[index] = 255;
            }
        }
    }

    Ok(bitmap)
}

/// Applies the 1-bpp AND mask.
///
/// `as_alpha` is the difference between clearing what the mask calls
/// transparent — the ordinary case — and rebuilding the alpha channel from the
/// mask entirely, which is what a 32-bit icon with no alpha of its own needs.
fn apply_mask(
    bitmap: &mut Bitmap,
    data: &[u8],
    mask_at: usize,
    and_stride: usize,
    top_down: bool,
    as_alpha: bool,
) {
    for y in 0..bitmap.height {
        let row = if top_down { y } else { bitmap.height - 1 - y };
        let row_at = mask_at + (row as usize) * and_stride;
        let Some(row_bytes) = data.get(row_at..row_at + and_stride) else {
            continue;
        };
        for x in 0..bitmap.width {
            let byte = row_bytes.get((x / 8) as usize).copied().unwrap_or(0);
            // A set bit means "leave the screen alone": transparent.
            let transparent = byte & (0x80 >> (x % 8)) != 0;
            if !transparent && !as_alpha {
                continue;
            }
            let [r, g, b, a] = bitmap.pixel(x, y);
            let alpha = if transparent {
                0
            } else if as_alpha {
                255
            } else {
                a
            };
            bitmap.set_pixel(x, y, [r, g, b, alpha]);
        }
    }
}

/// Resolves the four states a 1-bpp cursor's two masks describe.
///
/// | AND | XOR | Windows draws | What is stored here |
/// |---|---|---|---|
/// | 0 | 0 | black | black, opaque |
/// | 0 | 1 | white | white, opaque |
/// | 1 | 0 | the screen | transparent |
/// | 1 | 1 | **the screen inverted** | black, opaque |
///
/// The last row is the only judgement call in this file. A cursor cannot invert
/// anything once it is a bitmap — inversion is a drawing operation against
/// whatever is underneath — so it has to become a colour, and black is what an
/// inverted pointer looks like over the light backgrounds these were designed
/// against. It is also what every cursor editor shows for these files.
fn apply_monochrome_masks(
    bitmap: &mut Bitmap,
    data: &[u8],
    colour: (usize, usize),
    mask: (usize, usize),
    top_down: bool,
) {
    let (xor_at, xor_stride) = colour;
    let (mask_at, and_stride) = mask;
    let bit = |row: &[u8], x: u32| -> bool {
        row.get((x / 8) as usize)
            .is_some_and(|byte| byte & (0x80 >> (x % 8)) != 0)
    };

    for y in 0..bitmap.height {
        let row = if top_down { y } else { bitmap.height - 1 - y };
        let and_row = data.get(mask_at + (row as usize) * and_stride..)
            .and_then(|rest| rest.get(..and_stride));
        let xor_row = data.get(xor_at + (row as usize) * xor_stride..)
            .and_then(|rest| rest.get(..xor_stride));
        let (Some(and_row), Some(xor_row)) = (and_row, xor_row) else {
            continue;
        };

        for x in 0..bitmap.width {
            if !bit(and_row, x) {
                continue; // opaque, and already the right colour
            }
            let [r, g, b, _] = bitmap.pixel(x, y);
            if bit(xor_row, x) {
                // The invert case. Black rather than whatever the palette put
                // there, which for a monochrome image is white — and a white
                // crosshair on a white page is an invisible cursor.
                let _ = (r, g, b);
                bitmap.set_pixel(x, y, [0, 0, 0, 255]);
            } else {
                bitmap.set_pixel(x, y, [r, g, b, 0]);
            }
        }
    }
}

/// Bytes per row, padded to the 4-byte boundary every DIB row sits on.
fn stride(width: u32, bits: u32) -> usize {
    ((width as u64 * bits as u64).div_ceil(32) * 4) as usize
}

/// One pixel out of a DIB row, at whatever depth the header declared.
fn sample(row: &[u8], x: u32, bit_count: u16, palette: &[[u8; 4]]) -> [u8; 4] {
    let indexed = |index: usize| -> [u8; 4] { palette.get(index).copied().unwrap_or([0, 0, 0, 255]) };
    match bit_count {
        1 => {
            let byte = row.get((x / 8) as usize).copied().unwrap_or(0);
            indexed(((byte >> (7 - (x % 8))) & 1) as usize)
        }
        4 => {
            let byte = row.get((x / 2) as usize).copied().unwrap_or(0);
            let nibble = if x % 2 == 0 { byte >> 4 } else { byte & 0x0f };
            indexed(nibble as usize)
        }
        8 => indexed(row.get(x as usize).copied().unwrap_or(0) as usize),
        16 => {
            // X1R5G5B5, which is what this format means by 16-bit.
            let at = (x as usize) * 2;
            let value = u16::from_le_bytes([
                row.get(at).copied().unwrap_or(0),
                row.get(at + 1).copied().unwrap_or(0),
            ]);
            let expand = |five: u16| ((five << 3) | (five >> 2)) as u8;
            [
                expand((value >> 10) & 0x1f),
                expand((value >> 5) & 0x1f),
                expand(value & 0x1f),
                255,
            ]
        }
        24 => {
            let at = (x as usize) * 3;
            [
                row.get(at + 2).copied().unwrap_or(0),
                row.get(at + 1).copied().unwrap_or(0),
                row.get(at).copied().unwrap_or(0),
                255,
            ]
        }
        32 => {
            let at = (x as usize) * 4;
            [
                row.get(at + 2).copied().unwrap_or(0),
                row.get(at + 1).copied().unwrap_or(0),
                row.get(at).copied().unwrap_or(0),
                row.get(at + 3).copied().unwrap_or(255),
            ]
        }
        _ => [0, 0, 0, 0],
    }
}

/// Decodes every frame of an animated cursor, with the delay each one is shown
/// for.
///
/// Returns frames in **playback order**. An `.ani` may carry a `seq` chunk that
/// plays its frames in an order other than the one they are stored in — a
/// bounce written as four frames played 0,1,2,3,2,1 — and a reader that ignores
/// it turns a smooth loop into a jump.
pub fn decode_ani(bytes: &[u8]) -> AppResult<Vec<(Bitmap, u32)>> {
    if !looks_like_an_ani(bytes) {
        return Err(AppError::invalid("that is not an animated cursor"));
    }
    // The declared size excludes the 8-byte `RIFF` header. Trusting it over the
    // buffer would be the same mistake as trusting a directory entry.
    let declared = u32_at(bytes, 4).unwrap_or(0) as usize;
    let end = declared.saturating_add(8).min(bytes.len());

    let mut icons: Vec<&[u8]> = Vec::new();
    let mut rates: Vec<u32> = Vec::new();
    let mut sequence: Vec<u32> = Vec::new();
    let mut default_jiffies = DEFAULT_JIFFIES;
    let mut declared_frames = 0usize;

    // 12 skips `RIFF <size> ACON`.
    let mut at = 12usize;
    while at + 8 <= end {
        let Some(id) = bytes.get(at..at + 4) else {
            break;
        };
        let id = [id[0], id[1], id[2], id[3]];
        let size = u32_at(bytes, at + 4).unwrap_or(0) as usize;
        let body_at = at + 8;
        let Some(body) = bytes.get(body_at..body_at.saturating_add(size).min(end)) else {
            break;
        };

        match &id {
            b"anih" => {
                // cbSize, cFrames, cSteps, cx, cy, cBitCount, cPlanes, JifRate,
                // flags — in that order, and it is cFrames that comes second.
                declared_frames = u32_at(body, 4).unwrap_or(0) as usize;
                let rate = u32_at(body, 28).unwrap_or(0);
                if rate > 0 {
                    default_jiffies = rate;
                }
            }
            b"rate" => {
                rates = body
                    .chunks_exact(4)
                    .take(MAX_FRAMES)
                    .filter_map(|chunk| u32_at(chunk, 0))
                    .collect();
            }
            b"seq " => {
                sequence = body
                    .chunks_exact(4)
                    .take(MAX_FRAMES)
                    .filter_map(|chunk| u32_at(chunk, 0))
                    .collect();
            }
            // Only the `fram` list holds the frames; a `LIST` of anything else
            // (`INFO`, most often) is metadata and is skipped by the arm below.
            b"LIST" if body.starts_with(b"fram") => {
                collect_icons(body, 4, &mut icons);
            }
            _ => {}
        }

        if size == 0 {
            break; // a zero-length chunk would otherwise never advance
        }
        // Chunks are word-aligned and the pad byte is not counted in the size.
        at = body_at.saturating_add(size + (size & 1));
    }

    if icons.is_empty() {
        return Err(AppError::invalid(
            "this animated cursor contains no frames this app can read",
        ));
    }
    if declared_frames != 0 && declared_frames != icons.len() {
        log::debug!(
            "ani: the header declares {declared_frames} frames and {} were found",
            icons.len()
        );
    }

    // Decoded once each, then referenced by the sequence — a bounce that plays
    // four frames six times is four decodes, not six.
    let decoded: Vec<Option<Bitmap>> = icons
        .iter()
        .map(|icon| decode_icon(icon).ok().map(|image| image.bitmap))
        .collect();

    let order: Vec<usize> = if sequence.is_empty() {
        (0..decoded.len()).collect()
    } else {
        sequence.iter().map(|&index| index as usize).collect()
    };

    let mut frames: Vec<(Bitmap, u32)> = Vec::with_capacity(order.len().min(MAX_FRAMES));
    for (step, &index) in order.iter().enumerate().take(MAX_FRAMES) {
        // An out-of-range index is a broken file, not a reason to refuse the
        // rest of the animation.
        let Some(Some(bitmap)) = decoded.get(index) else {
            continue;
        };
        let jiffies = rates.get(step).copied().unwrap_or(default_jiffies).max(1);
        let delay = (jiffies as u64 * 1_000 / JIFFIES_PER_SECOND as u64) as u32;
        frames.push((bitmap.clone(), delay.clamp(MIN_DELAY_MS, MAX_DELAY_MS)));
    }

    if frames.is_empty() {
        return Err(AppError::invalid(
            "none of this animated cursor's frames could be read",
        ));
    }

    // Frames of different sizes cannot stay registered with each other, and
    // everything downstream crops one rectangle out of all of them. Rare, and
    // cheap to correct here rather than to special-case there.
    let width = frames.iter().map(|(bitmap, _)| bitmap.width).max().unwrap_or(1);
    let height = frames.iter().map(|(bitmap, _)| bitmap.height).max().unwrap_or(1);
    for (bitmap, _) in frames.iter_mut() {
        if bitmap.width != width || bitmap.height != height {
            if let Ok(resized) = bitmap.resized(width, height) {
                *bitmap = resized;
            }
        }
    }

    Ok(frames)
}

/// Walks a `LIST fram` collecting its `icon` chunks.
fn collect_icons<'a>(list: &'a [u8], mut at: usize, out: &mut Vec<&'a [u8]>) {
    while at + 8 <= list.len() && out.len() < MAX_FRAMES {
        let Some(id) = list.get(at..at + 4) else {
            return;
        };
        let id = [id[0], id[1], id[2], id[3]];
        let size = u32_at(list, at + 4).unwrap_or(0) as usize;
        let body_at = at + 8;
        let Some(body) = list.get(body_at..body_at.saturating_add(size).min(list.len())) else {
            return;
        };
        if &id == b"icon" && !body.is_empty() {
            out.push(body);
        }
        if size == 0 {
            return;
        }
        at = body_at.saturating_add(size + (size & 1));
    }
}

/// The hotspot a `.cur` or `.ani` carries, as a fraction of its own size.
///
/// **This is the one thing an imported cursor knows that a picture does not.**
/// Everything else about a `.cur` can be recovered from its pixels; where the
/// click lands cannot, and guessing it is what makes a converted cursor feel
/// like it points slightly to the left of where you are pointing.
///
/// `None` for an icon, which has no hotspot, and for a cursor whose hotspot
/// sits outside its own image — a file saying something impossible rather than
/// something unusual.
pub fn hotspot_fraction(bytes: &[u8]) -> Option<(f32, f32)> {
    // An animation's hotspot lives in its frames, which are whole `.cur` files.
    // The first frame decides: every frame of a sane animation agrees, and a
    // pointer whose click point moved mid-animation would be unusable anyway.
    if looks_like_an_ani(bytes) {
        let mut frames: Vec<&[u8]> = Vec::new();
        let declared = u32_at(bytes, 4).unwrap_or(0) as usize;
        let end = declared.saturating_add(8).min(bytes.len());
        let mut at = 12usize;
        while at + 8 <= end && frames.is_empty() {
            let size = u32_at(bytes, at + 4).unwrap_or(0) as usize;
            let body_at = at + 8;
            let body = bytes.get(body_at..body_at.saturating_add(size).min(end))?;
            if bytes.get(at..at + 4) == Some(&b"LIST"[..]) && body.starts_with(b"fram") {
                collect_icons(body, 4, &mut frames);
            }
            if size == 0 {
                break;
            }
            at = body_at.saturating_add(size + (size & 1));
        }
        return hotspot_fraction(frames.first()?);
    }

    let image = decode_icon(bytes).ok()?;
    let (x, y) = image.hotspot?;
    let (width, height) = (image.bitmap.width, image.bitmap.height);
    if width == 0 || height == 0 || x as u32 >= width || y as u32 >= height {
        return None;
    }
    Some((x as f32 / width as f32, y as f32 / height as f32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::ani_writer::{self, AniFrame, AniMetadata};
    use crate::build::cur_writer::{self, CursorImage};

    fn square(size: u32, rgba: [u8; 4]) -> Bitmap {
        let mut bitmap = Bitmap::new(size, size);
        for y in 0..size {
            for x in 0..size {
                bitmap.set_pixel(x, y, rgba);
            }
        }
        bitmap
    }

    /// The reader and the writer are two halves of one format, so the test that
    /// matters most is that a file this app writes reads back as what went in.
    #[test]
    fn a_cursor_this_app_wrote_reads_back_unchanged() {
        let bitmap = square(32, [200, 30, 60, 255]);
        let bytes = cur_writer::write_cur(&[CursorImage::new(bitmap, (4, 7))]).expect("write");

        let read = decode_icon(&bytes).expect("read");
        assert_eq!((read.bitmap.width, read.bitmap.height), (32, 32));
        assert_eq!(read.hotspot, Some((4, 7)));
        assert_eq!(read.bitmap.pixel(10, 10), [200, 30, 60, 255]);
    }

    /// Transparency has to survive the round trip, or every imported cursor
    /// arrives as a rectangle.
    #[test]
    fn transparency_survives_the_round_trip() {
        let mut bitmap = square(16, [255, 255, 255, 255]);
        bitmap.set_pixel(0, 0, [0, 0, 0, 0]);
        let bytes = cur_writer::write_cur(&[CursorImage::new(bitmap, (0, 0))]).expect("write");

        let read = decode_icon(&bytes).expect("read");
        assert_eq!(read.bitmap.alpha(0, 0), 0);
        assert_eq!(read.bitmap.alpha(8, 8), 255);
    }

    /// The largest entry is the one worth rebuilding a ladder from.
    #[test]
    fn the_largest_image_in_the_file_is_the_one_read() {
        let images = vec![
            CursorImage::new(square(16, [10, 10, 10, 255]), (0, 0)),
            CursorImage::new(square(48, [20, 20, 20, 255]), (0, 0)),
            CursorImage::new(square(32, [30, 30, 30, 255]), (0, 0)),
        ];
        let bytes = cur_writer::write_cur(&images).expect("write");
        assert_eq!(decode_icon(&bytes).expect("read").bitmap.width, 48);
    }

    #[test]
    fn an_animation_reads_back_with_its_frames_and_delays() {
        let frames: Vec<AniFrame> = (0..3)
            .map(|index| AniFrame {
                images: vec![CursorImage::new(
                    square(32, [10 * index as u8 + 10, 0, 0, 255]),
                    (1, 1),
                )],
                delay_ms: 100,
            })
            .collect();
        let bytes = ani_writer::write_ani(&frames, 1.0, &AniMetadata::default()).expect("write");

        let read = decode_ani(&bytes).expect("read");
        assert_eq!(read.len(), 3);
        for (index, (bitmap, delay)) in read.iter().enumerate() {
            assert_eq!(bitmap.width, 32);
            assert_eq!(bitmap.pixel(1, 1)[0], 10 * index as u8 + 10);
            // 100 ms is 6 jiffies exactly, and comes back as 100.
            assert_eq!(*delay, 100);
        }
    }

    /// The point of parsing the container rather than asking Windows: a `.ani`
    /// must arrive as an animation, not as a picture of its first frame.
    #[test]
    fn an_animation_is_not_read_as_a_still() {
        let frames: Vec<AniFrame> = (0..4)
            .map(|_| AniFrame {
                images: vec![CursorImage::new(square(32, [1, 2, 3, 255]), (0, 0))],
                delay_ms: 60,
            })
            .collect();
        let bytes = ani_writer::write_ani(&frames, 1.0, &AniMetadata::default()).expect("write");
        assert_eq!(decode_ani(&bytes).expect("read").len(), 4);
    }

    /// Every one of these is a file somebody could drop on the window. None of
    /// them may panic, and all of them must produce an error rather than a
    /// bitmap.
    #[test]
    fn malformed_files_are_refused_rather_than_trusted() {
        let cases: Vec<Vec<u8>> = vec![
            vec![],
            vec![0, 0, 2, 0],
            // Claims one image, gives no directory entry.
            vec![0, 0, 2, 0, 1, 0],
            // Claims 65,535 images.
            vec![0, 0, 2, 0, 0xff, 0xff],
            // A directory entry pointing past the end of the file.
            {
                let mut bytes = vec![0, 0, 2, 0, 1, 0];
                bytes.extend_from_slice(&[32, 32, 0, 0, 0, 0, 0, 0]);
                bytes.extend_from_slice(&0xffff_ffffu32.to_le_bytes());
                bytes.extend_from_slice(&0xffff_ffffu32.to_le_bytes());
                bytes
            },
            b"RIFF\x04\x00\x00\x00ACON".to_vec(),
            // A RIFF whose chunk claims four gigabytes.
            {
                let mut bytes = b"RIFF\xff\xff\xff\xffACON".to_vec();
                bytes.extend_from_slice(b"anih");
                bytes.extend_from_slice(&0xffff_ffffu32.to_le_bytes());
                bytes
            },
        ];
        for bytes in cases {
            assert!(decode_icon(&bytes).is_err() || decode_ani(&bytes).is_err());
        }
    }

    /// A truncated file is the ordinary shape of a bad download, and it must
    /// not take the process with it.
    #[test]
    fn truncation_at_every_length_is_survivable() {
        let bytes = cur_writer::write_cur(&[CursorImage::new(square(32, [9, 9, 9, 255]), (1, 1))])
            .expect("write");
        for cut in 0..bytes.len() {
            let _ = decode_icon(&bytes[..cut]);
        }
        let frames = vec![AniFrame {
            images: vec![CursorImage::new(square(16, [9, 9, 9, 255]), (0, 0))],
            delay_ms: 100,
        }];
        let ani = ani_writer::write_ani(&frames, 1.0, &AniMetadata::default()).expect("write");
        for cut in 0..ani.len() {
            let _ = decode_ani(&ani[..cut]);
        }
    }

    /// The acceptance test this module exists for: **every cursor Windows
    /// itself ships must import.**
    ///
    /// Round-tripping our own writer proves the two halves agree with each
    /// other and nothing about whether they agree with the format. `C:\Windows\
    /// Cursors` is two hundred files written by Microsoft over twenty years —
    /// palette DIBs, 32-bit alpha, multi-resolution directories and animations
    /// with `rate` and `seq` chunks — and it is the closest thing to a
    /// conformance suite that exists for this format.
    ///
    /// Skipped rather than failed where the directory is absent, so a build on
    /// a machine without it is not a red test.
    #[test]
    fn every_cursor_windows_ships_can_be_read() {
        let dir = std::path::Path::new(r"C:\Windows\Cursors");
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };

        let (mut read, mut animated, mut failures) = (0usize, 0usize, Vec::new());
        for entry in entries.flatten() {
            let path = entry.path();
            let extension = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

            match extension.as_str() {
                "cur" => match decode_icon(&bytes) {
                    Ok(image) => {
                        read += 1;
                        assert!(image.bitmap.width > 0, "{name} decoded to nothing");
                        assert!(image.hotspot.is_some(), "{name} is a cursor with no hotspot");
                        // **Not merely readable — visible.** The monochrome
                        // cursors Windows ships (`cross_l`, `lcross`, `libeam`)
                        // are drawn almost entirely with the invert state, and
                        // a reader that treats that as transparency returns a
                        // perfectly valid, perfectly empty bitmap. The import
                        // then fails one step later with "the image is
                        // completely transparent", which points at the wrong
                        // module entirely.
                        assert!(
                            image.bitmap.opaque_bounds().is_some(),
                            "{name} decoded to a completely transparent image"
                        );
                    }
                    Err(e) => failures.push(format!("{name}: {e}")),
                },
                "ani" => match decode_ani(&bytes) {
                    Ok(frames) => {
                        read += 1;
                        animated += 1;
                        // A one-frame animation would mean the frame list was
                        // parsed but the container was not.
                        assert!(frames.len() > 1, "{name} came back as {} frame", frames.len());
                        assert!(
                            frames.iter().all(|(bitmap, _)| bitmap.opaque_bounds().is_some()),
                            "{name} has a frame that decoded to nothing visible"
                        );
                    }
                    Err(e) => failures.push(format!("{name}: {e}")),
                },
                _ => {}
            }
        }

        assert!(failures.is_empty(), "{} of Windows' own cursors would not import: {failures:#?}", failures.len());
        println!("read {read} of Windows' own cursor files, {animated} of them animated");
    }

    /// **The whole journey, on a real file: `.ani` in, working `.ani` out.**
    ///
    /// Reading the frames is only half of what a user is asking for when they
    /// drop an animated cursor on the window. The other half is that what comes
    /// back out is a cursor Windows will actually load and animate — which is
    /// the one thing no amount of parsing proves.
    #[test]
    fn an_imported_animation_comes_back_out_as_one_windows_accepts() {
        let path = std::path::Path::new(r"C:\Windows\Cursors").join("aero_busy.ani");
        let Ok(bytes) = std::fs::read(&path) else {
            return; // not this machine's problem
        };

        let frames = decode_ani(&bytes).expect("Windows' own busy cursor decodes");
        assert!(frames.len() > 1, "it is an animation");

        let prepared = crate::build::pipeline::prepare_animation(&frames).expect("prepared");
        let built = crate::build::pipeline::build_ani(
            &prepared,
            (0.5, 0.5),
            &crate::build::pipeline::Finish {
                tint: None,
                opacity: 1.0,
                outline: false,
            },
            32,
            1.0,
            &ani_writer::AniMetadata::default(),
        )
        .expect("rebuilt");

        let dir = std::env::temp_dir().join("cursorforge-tests");
        std::fs::create_dir_all(&dir).ok();
        let out = dir.join("imported-roundtrip.ani");
        std::fs::write(&out, &built).expect("written");
        let loaded = crate::cursor::engine::verify_loadable(&out);
        let _ = std::fs::remove_file(&out);
        assert!(loaded.is_ok(), "Windows refused the rebuilt animation: {loaded:?}");
    }

    #[test]
    fn a_cursors_hotspot_comes_back_as_a_fraction() {
        let bytes = cur_writer::write_cur(&[CursorImage::new(square(32, [1, 1, 1, 255]), (8, 16))])
            .expect("write");
        let (x, y) = hotspot_fraction(&bytes).expect("a cursor has a hotspot");
        assert!((x - 0.25).abs() < 0.001, "x was {x}");
        assert!((y - 0.5).abs() < 0.001, "y was {y}");
    }
}
