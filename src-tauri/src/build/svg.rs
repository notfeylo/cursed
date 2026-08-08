//! SVG rasterisation for the catalog.
//!
//! Catalog artwork ships as vectors and is rendered at each target size, so a
//! 256 px cursor is genuinely drawn at 256 px rather than upscaled from a 32 px
//! bitmap. This is the entire reason catalog cursors stay sharp at 200% DPI
//! while third-party packs turn to mush (PRD §5).

use crate::build::bitmap::Bitmap;
use crate::error::{AppError, AppResult};
use resvg::tiny_skia;
use resvg::usvg;

/// Renders an SVG document into a square RGBA bitmap of `size` pixels.
pub fn render(svg: &str, size: u32) -> AppResult<Bitmap> {
    if size == 0 || size > 1024 {
        return Err(AppError::invalid(format!("{size} is not a sane render size")));
    }

    let options = usvg::Options::default();
    let tree = usvg::Tree::from_str(svg, &options)
        .map_err(|e| AppError::invalid(format!("the SVG could not be parsed: {e}")))?;

    let mut pixmap = tiny_skia::Pixmap::new(size, size)
        .ok_or_else(|| AppError::msg("could not allocate a render target"))?;

    let source = tree.size();
    let scale = size as f32 / source.width().max(source.height()).max(1.0);
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // tiny-skia hands back premultiplied alpha; the rest of the pipeline and the
    // .cur DIB format both want straight alpha, so undo it here rather than
    // letting premultiplied pixels leak downstream and darken every edge.
    let mut pixels = Vec::with_capacity((size as usize) * (size as usize) * 4);
    for pixel in pixmap.pixels() {
        let demultiplied = pixel.demultiply();
        pixels.extend_from_slice(&[
            demultiplied.red(),
            demultiplied.green(),
            demultiplied.blue(),
            demultiplied.alpha(),
        ]);
    }

    Bitmap::from_rgba(size, size, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUARE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
        <rect x="0" y="0" width="10" height="10" fill="#ffffff"/></svg>"##;

    #[test]
    fn renders_a_filled_square_at_the_requested_size() {
        let bitmap = render(SQUARE, 64).unwrap();
        assert_eq!((bitmap.width, bitmap.height), (64, 64));
        assert_eq!(bitmap.pixel(32, 32), [255, 255, 255, 255]);
    }

    #[test]
    fn transparent_areas_stay_transparent_after_demultiplying() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
            <circle cx="5" cy="5" r="2" fill="#ffffff"/></svg>"##;
        let bitmap = render(svg, 32).unwrap();
        assert_eq!(bitmap.alpha(0, 0), 0, "corner is outside the circle");
        assert_eq!(bitmap.alpha(16, 16), 255, "centre is inside it");
    }

    #[test]
    fn malformed_svg_is_an_error_not_a_panic() {
        assert!(render("<svg", 32).is_err());
        assert!(render("", 32).is_err());
        assert!(render(SQUARE, 0).is_err());
        assert!(render(SQUARE, 4096).is_err());
    }
}
