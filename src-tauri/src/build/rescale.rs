//! Re-rendering somebody else's cursor file at the size the user asked for.
//!
//! ## The bug this exists for
//!
//! A generated pack is drawn from SVG at whatever size is wanted, so the size
//! control has always worked for those. An imported pack is not drawn — it is a
//! finished `.cur` or `.ani` made by somebody else — and `build_imported` used
//! to put those files into the scheme **exactly as they arrived**. Since the
//! whole shipped catalog is imported packs, that is the size control for a
//! hundred and thirty-three of the cursors in the app.
//!
//! For a static `.cur` it half-worked by accident: the file carries a directory
//! of resolutions, and `engine::set_role` asks `LoadImageW` for a rung, so the
//! shell scaled it. Badly — bilinear, on straight alpha, no gamma — but it did
//! change size.
//!
//! **For an `.ani` it did not work at all.** An animated cursor holds exactly
//! one resolution and cannot be rescaled by the loader; `set_animated_role`
//! documents why it must call `LoadCursorFromFileW` with no size at all. So an
//! imported animation was pinned to whatever size its author drew it at —
//! usually 32 px — and the slider moved, the number changed, `CursorBaseSize`
//! was written, and the pointer on screen stayed exactly the same. That is
//! "increasing the size works for every cursor except the animated ones".
//!
//! So the artwork is re-rendered here instead, through the same Lanczos3-in-
//! linear-light path everything else uses, and cached per size.
//!
//! ## What is deliberately not preserved
//!
//! A re-rendered `.ani` does not carry the source file's `LIST INFO` chunk, so
//! the artist's name inside the *installed* copy is lost. The shipped archive
//! still has it, and the two sites this catalog draws on are credited on the
//! website — this is a derived render in a cache directory, the same as every
//! other file under `cache/`, and reproducing a metadata chunk through a decode
//! and re-encode would be inventing provenance rather than carrying it.

use crate::build::{ani_writer, cur_writer, icon_reader, pipeline};
use crate::cursor::roles::Role;
use crate::error::{AppError, AppResult};
use crate::paths;
use std::path::{Path, PathBuf};

/// Bumped when a change here would produce different pixels for the same input.
///
/// The cache is keyed on it, so an old render is stepped over rather than
/// served — the same contract `catalog::RENDER_VERSION` has.
const RESCALE_VERSION: u32 = 1;

/// The size beyond which a source is not enlarged any further.
///
/// `pipeline::sizes_for_source` already refuses to build rungs past 4×, and the
/// requested size is added to that ladder rather than replacing the rule: a
/// 32 px arrow asked for at 128 px is a 4× enlargement, which is the documented
/// limit and still visibly better than letting the shell do it.
fn ladder_for(width: u32, height: u32, wanted: u32) -> Vec<u32> {
    let mut sizes = pipeline::sizes_for_source(width, height);
    if !sizes.contains(&wanted) {
        sizes.push(wanted);
        sizes.sort_unstable();
    }
    sizes
}

/// The hotspot as a fraction of the image, which is the only form that survives
/// a resize.
///
/// A `.cur` stores it in pixels, so a 3,3 hotspot on a 32 px arrow has to become
/// 12,12 at 128 px or the click point drifts to the middle of the artwork. That
/// drift is exactly what makes a resized cursor feel like it points somewhere
/// other than where it is.
fn hotspot_fraction(bytes: &[u8], image: &icon_reader::IconImage) -> (f32, f32) {
    if let Some((hx, hy)) = image.hotspot {
        let w = image.bitmap.width.max(1) as f32;
        let h = image.bitmap.height.max(1) as f32;
        return ((hx as f32 / w).clamp(0.0, 1.0), (hy as f32 / h).clamp(0.0, 1.0));
    }
    // Falls back to reading the file directly, then to the top-left corner —
    // which is where a pointer's click lands and the least wrong guess there is.
    icon_reader::hotspot_fraction(bytes).unwrap_or((0.0, 0.0))
}

/// Where the re-rendered copy of one role lives.
fn cache_path(slug: &str, role: Role, size: u32, animated: bool) -> AppResult<PathBuf> {
    let extension = if animated { "ani" } else { "cur" };
    Ok(paths::cache_dir()?
        .join("imported")
        .join(paths::validate_relative(slug)?)
        .join(format!("v{RESCALE_VERSION}-{size}"))
        .join(format!("{}.{extension}", role.file_stem())))
}

/// One imported role, re-rendered at `size`.
///
/// Returns the cached path, building it if it is not there yet. The source file
/// is never modified — a user's imported pack is their data.
pub fn imported_role(slug: &str, role: Role, source: &Path, size: u32) -> AppResult<PathBuf> {
    let bytes = std::fs::read(source)?;
    let animated = icon_reader::looks_like_an_ani(&bytes);

    let destination = cache_path(slug, role, size, animated)?;
    if destination.is_file() {
        return Ok(destination);
    }

    let rendered = if animated {
        render_ani(&bytes, size)?
    } else if icon_reader::looks_like_an_icon(&bytes) {
        render_cur(&bytes, size)?
    } else {
        return Err(AppError::invalid("that is not a cursor file"));
    };

    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&destination, rendered)?;
    Ok(destination)
}

/// Every frame resized, with the delays and the playback order left alone.
fn render_ani(bytes: &[u8], size: u32) -> AppResult<Vec<u8>> {
    let frames = icon_reader::decode_ani(bytes)?;
    if frames.is_empty() {
        return Err(AppError::invalid("that animation has no frames"));
    }
    // `decode_ani` returns frames in playback order, having already expanded any
    // `seq` chunk — so writing them back in this order preserves a bounce
    // without needing to write a `seq` of our own.
    let hotspot = icon_reader::hotspot_fraction(bytes).unwrap_or((0.0, 0.0));

    let mut out = Vec::with_capacity(frames.len());
    for (bitmap, delay_ms) in frames {
        // One resolution per frame, which is what an `.ani` frame is. Going
        // through `build_multi_resolution` rather than `resized` directly is
        // what applies the sharpening pass and scales the hotspot with the
        // picture — the two things a hand-rolled resize here would forget.
        let images = cur_writer::build_multi_resolution(&bitmap, hotspot, &[size], false)?;
        out.push(ani_writer::AniFrame { images, delay_ms });
    }

    // Speed 1.0: the artist's timing, unchanged.
    //
    // `settings.animation_speed` is deliberately not applied. Imported
    // animations have always played at their own rate — they were installed as
    // the original file — and folding the speed setting in here would silently
    // re-time every animated cursor in the catalog the moment this shipped.
    ani_writer::write_ani(&out, 1.0, &ani_writer::AniMetadata::default())
}

/// A static cursor rebuilt as a full resolution ladder around the size wanted.
fn render_cur(bytes: &[u8], size: u32) -> AppResult<Vec<u8>> {
    let image = icon_reader::decode_icon(bytes)?;
    let hotspot = hotspot_fraction(bytes, &image);
    let sizes = ladder_for(image.bitmap.width, image.bitmap.height, size);
    // A ladder rather than the one size asked for. `engine::set_role` requests a
    // specific rung, but the shell also reloads this file straight from the
    // registry — at sign-in, after a theme change — and then it is
    // `CursorBaseSize` that decides, which our slider does not fully control.
    let images = cur_writer::build_multi_resolution(&image.bitmap, hotspot, &sizes, false)?;
    cur_writer::write_cur(&images)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::bitmap::Bitmap;
    use crate::build::cur_writer::{write_cur, CursorImage};

    fn swatch(size: u32) -> Bitmap {
        let mut art = Bitmap::new(size, size);
        for y in 0..size {
            for x in 0..size {
                art.set_pixel(x, y, [255, 128, 0, 255]);
            }
        }
        art
    }

    #[test]
    fn the_requested_size_is_always_on_the_ladder() {
        // The point of adding it: a 32 px source asked for at 128 must carry a
        // 128 px entry, or the shell scales the nearest one and the whole
        // re-render was pointless.
        assert!(ladder_for(32, 32, 128).contains(&128));
        assert!(ladder_for(16, 16, 10).contains(&10));
        // And a size already on it is not duplicated.
        let ladder = ladder_for(64, 64, 32);
        assert_eq!(ladder.iter().filter(|&&s| s == 32).count(), 1);
        assert!(ladder.windows(2).all(|pair| pair[0] < pair[1]), "sorted and unique");
    }

    #[test]
    fn a_static_cursor_is_rebuilt_at_the_size_asked_for() {
        let source = write_cur(&[CursorImage::new(swatch(32), (3, 3))]).expect("a cursor");
        let rebuilt = render_cur(&source, 128).expect("rebuilt");

        let entries = icon_reader::decode_icon(&rebuilt).expect("readable");
        // `decode_icon` hands back the largest entry, so a file built to include
        // 128 px must come back at least that big.
        assert!(
            entries.bitmap.width >= 128,
            "largest entry is {} px, so 128 was never written",
            entries.bitmap.width
        );
    }

    /// The hotspot has to move with the picture.
    ///
    /// A 3,3 hotspot on a 32 px cursor is a tenth of the way in. Resized to
    /// 128 px it must still be a tenth of the way in — if it stayed at 3,3 the
    /// click point would sit almost in the corner, and if it were left at the
    /// centre the pointer would click well below where it points.
    #[test]
    fn the_hotspot_is_scaled_rather_than_carried_over_in_pixels() {
        let source = write_cur(&[CursorImage::new(swatch(32), (3, 3))]).expect("a cursor");
        let image = icon_reader::decode_icon(&source).expect("readable");
        let (fx, fy) = hotspot_fraction(&source, &image);
        assert!((fx - 3.0 / 32.0).abs() < 0.02, "x fraction was {fx}");
        assert!((fy - 3.0 / 32.0).abs() < 0.02, "y fraction was {fy}");

        let images = cur_writer::build_multi_resolution(&swatch(32), (fx, fy), &[128], false)
            .expect("built");
        let (hx, _) = images[0].hotspot;
        assert!((11..=14).contains(&hx), "hotspot landed at {hx}, not near 12");
    }

    #[test]
    fn something_that_is_not_a_cursor_is_refused_rather_than_written() {
        assert!(render_cur(b"not a cursor at all", 32).is_err());
        assert!(render_ani(b"RIFFxxxxACON", 32).is_err());
    }
}
