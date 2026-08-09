//! Decode -> trim -> resample -> tint -> outline -> write.
//!
//! Everything a user drops on the app passes through here, so this is where the
//! untrusted-input rules from PRD §13.5 live: size caps before decode, a pixel
//! budget that a decompression bomb cannot talk its way past, a hard timeout,
//! and format sniffing from magic bytes — never from the file extension.

use crate::build::ani_writer::{self, AniFrame, AniMetadata};
use crate::build::bitmap::Bitmap;
use crate::build::cur_writer::{self, TARGET_SIZES};
use crate::error::{AppError, AppResult};
use image::{AnimationDecoder, ImageDecoder};
use std::io::Cursor;
use std::time::Duration;

/// 20 MB in, per PRD §6.1. Comfortably more than any real cursor source.
pub const MAX_INPUT_BYTES: usize = 20 * 1024 * 1024;
pub const MAX_DIMENSION: u32 = 4_096;
/// The real defence: a 4096x4096 RGBA image is 64 MB decoded, and that is the
/// ceiling regardless of how small the compressed file claimed to be.
pub const MAX_PIXELS: u64 = (MAX_DIMENSION as u64) * (MAX_DIMENSION as u64);
pub const DECODE_TIMEOUT: Duration = Duration::from_secs(30);
/// Frames beyond this are dropped before decode, so a 10,000-frame GIF cannot
/// turn into 10,000 resample jobs.
pub const MAX_SOURCE_FRAMES: usize = 240;

#[derive(Debug, Clone)]
pub enum Source {
    Static(Bitmap),
    Animated(Vec<(Bitmap, u32)>),
}

impl Source {
    pub fn first(&self) -> AppResult<&Bitmap> {
        match self {
            Source::Static(bitmap) => Ok(bitmap),
            Source::Animated(frames) => frames
                .first()
                .map(|(bitmap, _)| bitmap)
                .ok_or_else(|| AppError::invalid("the animation has no frames")),
        }
    }

    pub fn frame_count(&self) -> usize {
        match self {
            Source::Static(_) => 1,
            Source::Animated(frames) => frames.len(),
        }
    }

    pub fn is_animated(&self) -> bool {
        matches!(self, Source::Animated(_))
    }
}

/// Identifies a format from its leading bytes.
///
/// Extensions are a claim made by whoever named the file; magic bytes are a
/// property of the file itself. Only the latter decides what decoder runs.
pub fn sniff(bytes: &[u8]) -> AppResult<image::ImageFormat> {
    if bytes.len() < 12 {
        return Err(AppError::invalid("the file is too short to be an image"));
    }
    let format = match bytes {
        [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, ..] => image::ImageFormat::Png,
        [0xff, 0xd8, 0xff, ..] => image::ImageFormat::Jpeg,
        [b'G', b'I', b'F', b'8', ..] => image::ImageFormat::Gif,
        [b'B', b'M', ..] => image::ImageFormat::Bmp,
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => {
            image::ImageFormat::WebP
        }
        _ => {
            return Err(AppError::invalid(
                "only PNG, JPEG, GIF, WebP and BMP images can be imported",
            ))
        }
    };
    Ok(format)
}

fn guard_dimensions(width: u32, height: u32) -> AppResult<()> {
    if width == 0 || height == 0 {
        return Err(AppError::invalid("the image has no pixels"));
    }
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(AppError::ImageTooLarge(format!(
            "{width}x{height}, limit is {MAX_DIMENSION}x{MAX_DIMENSION}"
        )));
    }
    if (width as u64) * (height as u64) > MAX_PIXELS {
        return Err(AppError::ImageTooLarge("too many pixels".into()));
    }
    Ok(())
}

fn to_bitmap(image: image::RgbaImage) -> AppResult<Bitmap> {
    Bitmap::from_rgba(image.width(), image.height(), image.into_raw())
}

/// Decodes on a worker thread under a wall-clock timeout.
///
/// A pathological file can keep a decoder busy for a very long time without
/// ever exceeding a memory limit. The timeout is the answer to that, and it is
/// why decoding does not happen inline on the command thread.
pub fn decode(bytes: Vec<u8>) -> AppResult<Source> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(AppError::ImageTooLarge(format!(
            "{} MB, limit is {} MB",
            bytes.len() / 1_048_576,
            MAX_INPUT_BYTES / 1_048_576
        )));
    }
    let format = sniff(&bytes)?;

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("cursorforge-decode".into())
        .spawn(move || {
            let _ = tx.send(decode_inner(&bytes, format));
        })
        .map_err(|e| AppError::msg(format!("could not start the decoder: {e}")))?;

    match rx.recv_timeout(DECODE_TIMEOUT) {
        Ok(result) => result,
        Err(_) => Err(AppError::invalid(
            "the image took too long to decode and was abandoned",
        )),
    }
}

fn decode_inner(bytes: &[u8], format: image::ImageFormat) -> AppResult<Source> {
    match format {
        image::ImageFormat::Gif => decode_gif(bytes),
        image::ImageFormat::Png => decode_png(bytes),
        _ => {
            let reader = image::ImageReader::with_format(Cursor::new(bytes), format);
            let (width, height) = reader
                .into_dimensions()
                .map_err(|e| AppError::invalid(format!("unreadable image: {e}")))?;
            guard_dimensions(width, height)?;

            let decoded = image::load_from_memory_with_format(bytes, format)
                .map_err(|e| AppError::invalid(format!("unreadable image: {e}")))?;
            Ok(Source::Static(to_bitmap(decoded.to_rgba8())?))
        }
    }
}

/// PNG doubles as APNG. A still PNG takes the static path; an animated one is
/// treated exactly like a GIF, so both arrive at the `.ani` writer identically.
fn decode_png(bytes: &[u8]) -> AppResult<Source> {
    let decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes))
        .map_err(|e| AppError::invalid(format!("unreadable PNG: {e}")))?;
    let (width, height) = decoder.dimensions();
    guard_dimensions(width, height)?;

    if decoder.is_apng().unwrap_or(false) {
        let frames = decoder
            .apng()
            .map_err(|e| AppError::invalid(format!("unreadable APNG: {e}")))?
            .into_frames()
            .take(MAX_SOURCE_FRAMES)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::invalid(format!("unreadable APNG frames: {e}")))?;
        return collect_frames(frames);
    }

    let decoded = image::DynamicImage::from_decoder(decoder)
        .map_err(|e| AppError::invalid(format!("unreadable PNG: {e}")))?;
    Ok(Source::Static(to_bitmap(decoded.to_rgba8())?))
}

fn decode_gif(bytes: &[u8]) -> AppResult<Source> {
    let decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
        .map_err(|e| AppError::invalid(format!("unreadable GIF: {e}")))?;
    let (width, height) = decoder.dimensions();
    guard_dimensions(width, height)?;

    let frames = decoder
        .into_frames()
        .take(MAX_SOURCE_FRAMES)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::invalid(format!("unreadable GIF frames: {e}")))?;
    collect_frames(frames)
}

fn collect_frames(frames: Vec<image::Frame>) -> AppResult<Source> {
    if frames.is_empty() {
        return Err(AppError::invalid("the animation has no frames"));
    }
    if frames.len() == 1 {
        let frame = frames.into_iter().next().unwrap_or_else(|| unreachable!());
        return Ok(Source::Static(to_bitmap(frame.into_buffer())?));
    }

    let mut out = Vec::with_capacity(frames.len());
    for frame in frames {
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        // A zero delay means "as fast as possible", which every browser and
        // shell interprets as ~100 ms. Honour that rather than emitting a
        // 1-jiffy frame nobody can see.
        let delay_ms = numerator
            .checked_div(denominator)
            .map_or(100, |ms| ms.clamp(10, 2_000));
        out.push((to_bitmap(frame.into_buffer())?, delay_ms));
    }
    Ok(Source::Animated(out))
}

/// Cut out the background, then trim, square, and pad by one pixel so a contrast
/// outline has room to exist without being clipped at the canvas edge.
pub fn prepare_master(bitmap: &Bitmap) -> AppResult<Bitmap> {
    // The cut-out has to come first, because everything after it depends on
    // alpha. A JPEG off a search page, or a screenshot, is fully opaque — so
    // `trimmed()` finds nothing to trim and the whole rectangle, card and
    // corners included, becomes the cursor. That is the difference between
    // turning anything into a cursor and dragging a white box around the screen.
    //
    // An image that already carries transparency is left exactly as it was; see
    // `matte` for that rule and for what this deliberately does not attempt.
    let mut source = bitmap.clone();
    let report = crate::build::matte::remove_background(&mut source);
    if report.removed > 0.0 {
        log::debug!(
            "cut {:.0}% of the image away as background",
            report.removed * 100.0
        );
    }

    let trimmed = source.trimmed();
    if trimmed.is_empty() {
        return Err(AppError::invalid(
            "the image is completely transparent, so there is nothing to make a cursor from",
        ));
    }
    Ok(trimmed.squared().padded(1))
}

/// Geometric and tonal edits applied to a user's own artwork before it becomes
/// a cursor.
///
/// Order matters and is fixed here rather than left to the caller: crop, then
/// rotate, then flip, then invert. Cropping first means the rectangle the user
/// drew on the preview is the rectangle taken, regardless of what is done to it
/// afterwards; inverting last means it applies to whatever survived.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Transform {
    /// Quarter turns clockwise, 0–3. Right angles only — an arbitrary angle
    /// needs resampling, and at cursor sizes that softens every edge.
    pub quarter_turns: u32,
    pub flip_h: bool,
    pub flip_v: bool,
    pub invert: bool,
    /// Crop rectangle in fractions of the image, or `None` for the whole thing.
    pub crop: Option<[f32; 4]>,
}

impl Transform {
    pub fn is_identity(&self) -> bool {
        self.quarter_turns % 4 == 0
            && !self.flip_h
            && !self.flip_v
            && !self.invert
            && self.crop.is_none()
    }

    pub fn apply(&self, bitmap: &Bitmap) -> Bitmap {
        let mut out = match self.crop {
            Some([x0, y0, x1, y1]) => bitmap.cropped(x0, y0, x1, y1),
            None => bitmap.clone(),
        };
        if self.quarter_turns % 4 != 0 {
            out = out.rotated(self.quarter_turns);
        }
        if self.flip_h {
            out = out.flipped_h();
        }
        if self.flip_v {
            out = out.flipped_v();
        }
        if self.invert {
            out = out.inverted();
        }
        out
    }
}


/// Prepares every frame of an animation as one unit.
///
/// An animation cannot use `prepare_master` per frame. That trims each frame to
/// its own content, so a frame where the subject is smaller comes back a
/// different size and shifted — the cursor jitters, and the hotspot means
/// something different on every frame.
///
/// So the background is removed from each frame independently, then the
/// *union* of what survived decides one rectangle, and every frame is cropped to
/// that same rectangle. Frames stay registered with each other and the hotspot
/// keeps its meaning.
pub fn prepare_animation(frames: &[(Bitmap, u32)]) -> AppResult<Vec<(Bitmap, u32)>> {
    if frames.is_empty() {
        return Err(AppError::invalid("the animation has no frames"));
    }

    let cut: Vec<(Bitmap, u32)> = frames
        .iter()
        .map(|(bitmap, delay)| {
            let mut copy = bitmap.clone();
            crate::build::matte::remove_background(&mut copy);
            (copy, *delay)
        })
        .collect();

    // One rectangle for the whole animation: the union of every frame's content.
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for (bitmap, _) in &cut {
        let Some((x0, y0, x1, y1)) = bitmap.opaque_bounds() else {
            continue;
        };
        bounds = Some(match bounds {
            None => (x0, y0, x1, y1),
            Some((ax0, ay0, ax1, ay1)) => {
                (ax0.min(x0), ay0.min(y0), ax1.max(x1), ay1.max(y1))
            }
        });
    }

    let Some((x0, y0, x1, y1)) = bounds else {
        return Err(AppError::invalid(
            "every frame is completely transparent, so there is nothing to make a cursor from",
        ));
    };

    let (w, h) = (cut[0].0.width.max(1), cut[0].0.height.max(1));
    let (fx0, fy0) = (x0 as f32 / w as f32, y0 as f32 / h as f32);
    let (fx1, fy1) = ((x1 + 1) as f32 / w as f32, (y1 + 1) as f32 / h as f32);

    Ok(cut
        .into_iter()
        .map(|(bitmap, delay)| (bitmap.cropped(fx0, fy0, fx1, fy1).squared().padded(1), delay))
        .collect())
}

/// How a master is coloured before it becomes a cursor.
#[derive(Debug, Clone)]
pub struct Finish {
    /// `None` leaves the artwork's own colours alone — right for user images.
    /// `Some(rgb)` recolours a greyscale master — right for catalog packs.
    pub tint: Option<[u8; 3]>,
    pub opacity: f32,
    pub outline: bool,
}

impl Default for Finish {
    fn default() -> Self {
        Self {
            tint: None,
            opacity: 1.0,
            outline: true,
        }
    }
}

fn finish(master: &Bitmap, finish: &Finish) -> Bitmap {
    let tinted = match finish.tint {
        Some(rgb) => master.tinted(rgb),
        None => master.clone(),
    };
    if (finish.opacity - 1.0).abs() > f32::EPSILON {
        tinted.with_opacity(finish.opacity)
    } else {
        tinted
    }
}

/// Builds a multi-resolution `.cur` from one master image.
pub fn build_cur(
    master: &Bitmap,
    hotspot: (f32, f32),
    options: &Finish,
    sizes: &[u32],
) -> AppResult<Vec<u8>> {
    let coloured = finish(master, options);
    let images = cur_writer::build_multi_resolution(&coloured, hotspot, sizes, options.outline)?;
    cur_writer::write_cur(&images)
}

/// Builds an `.ani` at a single pixel size.
///
/// `.ani` files cannot hold multiple resolutions — the format has no directory
/// for them — so PRD §5.4 calls for one file per size, chosen at apply time.
/// This function builds one of those files.
pub fn build_ani(
    frames: &[(Bitmap, u32)],
    hotspot: (f32, f32),
    options: &Finish,
    size: u32,
    speed: f32,
    metadata: &AniMetadata,
) -> AppResult<Vec<u8>> {
    let mut built = Vec::with_capacity(frames.len());
    for (bitmap, delay_ms) in frames {
        let coloured = finish(bitmap, options);
        let images =
            cur_writer::build_multi_resolution(&coloured, hotspot, &[size], options.outline)?;
        built.push(AniFrame {
            images,
            delay_ms: *delay_ms,
        });
    }
    ani_writer::write_ani(&built, speed, metadata)
}

/// Slices a sprite sheet into frames, left to right, top to bottom.
pub fn slice_sprite_sheet(sheet: &Bitmap, columns: u32, rows: u32) -> AppResult<Vec<Bitmap>> {
    if columns == 0 || rows == 0 {
        return Err(AppError::invalid("a sprite sheet needs at least one row and column"));
    }
    if columns > 64 || rows > 64 {
        return Err(AppError::invalid("that is too many sprite-sheet cells"));
    }
    if sheet.width % columns != 0 || sheet.height % rows != 0 {
        return Err(AppError::invalid(format!(
            "{}x{} does not divide evenly into {columns}x{rows} cells",
            sheet.width, sheet.height
        )));
    }

    let cell_width = sheet.width / columns;
    let cell_height = sheet.height / rows;
    let mut out = Vec::with_capacity((columns * rows) as usize);
    for row in 0..rows {
        for column in 0..columns {
            let mut cell = Bitmap::new(cell_width, cell_height);
            for y in 0..cell_height {
                for x in 0..cell_width {
                    cell.set_pixel(x, y, sheet.pixel(column * cell_width + x, row * cell_height + y));
                }
            }
            out.push(cell);
        }
    }
    Ok(out)
}

/// The eight sizes every static catalog cursor ships.
pub fn target_sizes() -> &'static [u32] {
    &TARGET_SIZES
}

/// The sizes worth generating for a source image of a given resolution.
///
/// Catalog artwork is vector, so every size is drawn at full detail. An imported
/// bitmap is not: upscaling a 128 px PNG to 256 px adds no detail, just a blurry
/// image and a much larger file. A `.cur` stores every size as uncompressed
/// BGRA, so the top three sizes alone are about 80% of the bytes — for a 128 px
/// source that is half a megabyte per cursor buying nothing.
///
/// One size above the source is kept so the cursor still has something to offer
/// at high DPI rather than being upscaled by the OS from its largest entry.
pub fn sizes_for_source(width: u32, height: u32) -> Vec<u32> {
    let longest = width.max(height).max(32);
    let mut out: Vec<u32> = TARGET_SIZES.into_iter().filter(|&s| s <= longest).collect();
    if let Some(&next) = TARGET_SIZES.iter().find(|&&s| s > longest) {
        out.push(next);
    }
    if out.is_empty() {
        out.push(32);
    }
    out
}

/// Picks the `.ani` size closest to what Windows is currently drawing.
pub fn nearest_size(requested: u32) -> u32 {
    TARGET_SIZES
        .into_iter()
        .min_by_key(|size| size.abs_diff(requested))
        .unwrap_or(32)
}

/// Renders the 1:1 previews the import screen shows before anything is written.
pub fn preview_ladder(master: &Bitmap, options: &Finish) -> AppResult<Vec<(u32, String)>> {
    let coloured = finish(master, options);
    let mut out = Vec::with_capacity(TARGET_SIZES.len());
    for size in TARGET_SIZES {
        let scaled = coloured.resized(size, size)?;
        let finished = if options.outline {
            scaled.with_contrast_outline()
        } else {
            scaled
        };
        out.push((size, finished.to_png_data_uri()?));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let mut buffer = image::RgbaImage::new(width, height);
        for pixel in buffer.pixels_mut() {
            *pixel = image::Rgba([255, 255, 255, 255]);
        }
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(buffer)
            .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn formats_are_identified_from_magic_bytes_only() {
        assert_eq!(sniff(&png_bytes(4, 4)).unwrap(), image::ImageFormat::Png);
        assert_eq!(
            sniff(b"GIF89a\0\0\0\0\0\0").unwrap(),
            image::ImageFormat::Gif
        );
        // A PE header renamed to .png is still a PE header.
        assert!(sniff(b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00").is_err());
        assert!(sniff(b"short").is_err());
    }

    #[test]
    fn oversized_input_is_rejected_before_any_decoding_happens() {
        let huge = vec![0u8; MAX_INPUT_BYTES + 1];
        let error = decode(huge).unwrap_err();
        assert!(matches!(error, AppError::ImageTooLarge(_)));
    }

    #[test]
    fn dimension_guard_rejects_a_decompression_bomb() {
        assert!(guard_dimensions(MAX_DIMENSION + 1, 1).is_err());
        assert!(guard_dimensions(0, 10).is_err());
        assert!(guard_dimensions(MAX_DIMENSION, MAX_DIMENSION).is_ok());
    }

    #[test]
    fn a_real_png_decodes_to_a_static_source() {
        let source = decode(png_bytes(16, 24)).unwrap();
        assert!(!source.is_animated());
        let bitmap = source.first().unwrap();
        assert_eq!((bitmap.width, bitmap.height), (16, 24));
    }

    #[test]
    fn preparing_a_master_squares_and_pads_it() {
        let mut wide = Bitmap::new(20, 10);
        for x in 0..20 {
            wide.set_pixel(x, 5, [255, 255, 255, 255]);
        }
        let master = prepare_master(&wide).unwrap();
        assert_eq!(master.width, master.height, "square");
        assert_eq!(master.width, 22, "20px of artwork plus 1px of padding each side");
    }

    #[test]
    fn a_fully_transparent_image_is_refused_with_a_clear_reason() {
        let error = prepare_master(&Bitmap::new(16, 16)).unwrap_err();
        assert!(error.to_string().contains("transparent"));
    }

    #[test]
    fn sprite_sheets_slice_in_reading_order() {
        let mut sheet = Bitmap::new(4, 2);
        sheet.set_pixel(0, 0, [1, 0, 0, 255]);
        sheet.set_pixel(2, 0, [2, 0, 0, 255]);
        sheet.set_pixel(0, 1, [3, 0, 0, 255]);
        sheet.set_pixel(2, 1, [4, 0, 0, 255]);

        let cells = slice_sprite_sheet(&sheet, 2, 2).unwrap();
        assert_eq!(cells.len(), 4);
        assert_eq!(cells[0].pixel(0, 0)[0], 1);
        assert_eq!(cells[1].pixel(0, 0)[0], 2);
        assert_eq!(cells[2].pixel(0, 0)[0], 3);
        assert_eq!(cells[3].pixel(0, 0)[0], 4);
    }

    #[test]
    fn sprite_sheets_that_do_not_divide_evenly_are_refused() {
        assert!(slice_sprite_sheet(&Bitmap::new(5, 4), 2, 2).is_err());
        assert!(slice_sprite_sheet(&Bitmap::new(4, 4), 0, 2).is_err());
    }

    #[test]
    fn size_ladders_do_not_upscale_a_small_source_into_a_huge_file() {
        // A 64 px source gets the sizes up to 64 plus one step beyond;
        // anything past that is upscaled blur costing real bytes.
        let sizes = sizes_for_source(64, 64);
        assert!(sizes.contains(&64));
        assert!(!sizes.contains(&128), "128 from a 64px source is just blur");

        // A large source still gets the full ladder.
        assert_eq!(sizes_for_source(512, 512), TARGET_SIZES.to_vec());

        // A tiny source still produces something usable, starting at the
        // smallest rung rather than jumping straight to 32.
        let tiny = sizes_for_source(12, 12);
        assert!(!tiny.is_empty());
        assert_eq!(tiny[0], TARGET_SIZES[0]);
    }


    /// The failure this guards: trimming each frame to its own content makes a
    /// cursor that jitters, because a frame where the subject is smaller comes
    /// back a different size and shifted.
    #[test]
    fn animated_frames_stay_registered_after_the_background_goes() {
        let mut frames = Vec::new();
        for step in 0..4u32 {
            let mut frame = Bitmap::new(40, 40);
            for y in 0..40 {
                for x in 0..40 {
                    frame.set_pixel(x, y, [255, 255, 255, 255]);
                }
            }
            // A square that changes size frame to frame.
            let half = 6 + step * 2;
            for y in (20 - half)..(20 + half) {
                for x in (20 - half)..(20 + half) {
                    frame.set_pixel(x, y, [10, 20, 200, 255]);
                }
            }
            frames.push((frame, 60));
        }

        let prepared = prepare_animation(&frames).expect("prepares");
        assert_eq!(prepared.len(), 4);

        // Every frame comes back the same size, or the animation jumps.
        let first = (prepared[0].0.width, prepared[0].0.height);
        for (frame, _) in &prepared {
            assert_eq!((frame.width, frame.height), first, "frames must stay registered");
        }

        // And the white card is gone from all of them.
        for (frame, _) in &prepared {
            assert_eq!(frame.alpha(0, 0), 0, "the corner should be transparent");
        }
    }

    #[test]
    fn an_animation_with_no_content_is_refused_clearly() {
        let frames = vec![(Bitmap::new(8, 8), 60u32), (Bitmap::new(8, 8), 60)];
        assert!(prepare_animation(&frames).is_err());
        assert!(prepare_animation(&[]).is_err());
    }

    #[test]
    fn nearest_size_snaps_to_a_shipped_resolution() {
        assert_eq!(nearest_size(30), 32);
        assert_eq!(nearest_size(50), 48);
        // The ladder reaches 10 px now, because the size control does.
        assert_eq!(nearest_size(1), 10);
        assert_eq!(nearest_size(11), 10);
        assert_eq!(nearest_size(14), 16);
        // And stops at 128, which is the largest Windows will draw for a
        // pointer in practice. Asking for more produces a file nothing reads.
        assert_eq!(nearest_size(1_000), 128);
    }

    /// Every size the settings slider can produce must land on a real rung,
    /// otherwise a cursor is built at one resolution and drawn at another.
    #[test]
    fn every_size_the_ui_offers_snaps_onto_the_ladder() {
        use crate::state::settings::{MAX_CURSOR_PX, MIN_CURSOR_PX};
        for requested in MIN_CURSOR_PX..=MAX_CURSOR_PX {
            let snapped = nearest_size(requested);
            assert!(
                TARGET_SIZES.contains(&snapped),
                "{requested} snapped to {snapped}, which is not a shipped size"
            );
        }
    }

    #[test]
    fn a_built_cur_contains_every_requested_size() {
        let mut master = Bitmap::new(64, 64);
        for y in 8..56 {
            for x in 8..56 {
                master.set_pixel(x, y, [255, 255, 255, 255]);
            }
        }
        let file = build_cur(&master, (0.0, 0.0), &Finish::default(), &TARGET_SIZES).unwrap();
        assert_eq!(u16::from_le_bytes([file[2], file[3]]), 2, "is a cursor");
        assert_eq!(u16::from_le_bytes([file[4], file[5]]), 8, "eight resolutions");
    }
}
