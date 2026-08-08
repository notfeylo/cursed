//! Choosing where the click actually lands.
//!
//! Hotspots are computed and stored **normalised** (0.0-1.0). An absolute pixel
//! hotspot is correct at exactly one size and wrong at the other seven, which is
//! why third-party packs so often click a few pixels off at high DPI (PRD §5.3).

use crate::build::bitmap::Bitmap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HotspotMode {
    Center,
    TopLeft,
    /// Centre of mass of the alpha channel — right for symmetric marks such as
    /// crosshairs, dots and rings, and the safest default for artwork we have
    /// not seen.
    #[default]
    AlphaCentroid,
    /// The topmost, then leftmost, opaque pixel — right for arrow-shaped art,
    /// where the point of the arrow is the point of the cursor.
    TipDetect,
    Manual,
}

/// Normalised hotspot for a mode. Returns the centre for an empty bitmap rather
/// than failing: a blank image is the caller's problem to report, not a reason
/// for the hotspot maths to produce a NaN.
pub fn compute(bitmap: &Bitmap, mode: HotspotMode) -> (f32, f32) {
    match mode {
        HotspotMode::TopLeft => (0.0, 0.0),
        HotspotMode::Center | HotspotMode::Manual => (0.5, 0.5),
        HotspotMode::AlphaCentroid => alpha_centroid(bitmap),
        HotspotMode::TipDetect => tip(bitmap),
    }
}

fn denominator(bitmap: &Bitmap) -> (f32, f32) {
    (
        (bitmap.width.saturating_sub(1)).max(1) as f32,
        (bitmap.height.saturating_sub(1)).max(1) as f32,
    )
}

fn alpha_centroid(bitmap: &Bitmap) -> (f32, f32) {
    let mut total = 0f64;
    let mut sum_x = 0f64;
    let mut sum_y = 0f64;
    for y in 0..bitmap.height {
        for x in 0..bitmap.width {
            let weight = bitmap.alpha(x, y) as f64;
            if weight == 0.0 {
                continue;
            }
            total += weight;
            sum_x += weight * x as f64;
            sum_y += weight * y as f64;
        }
    }
    if total == 0.0 {
        return (0.5, 0.5);
    }
    let (dx, dy) = denominator(bitmap);
    (
        ((sum_x / total) as f32 / dx).clamp(0.0, 1.0),
        ((sum_y / total) as f32 / dy).clamp(0.0, 1.0),
    )
}

/// Scans top-down for the first row with coverage, then takes the leftmost
/// covered pixel in that row. The alpha threshold ignores the soft fringe an
/// anti-aliased tip leaves behind, which would otherwise pull the hotspot a
/// pixel or two off the visible point.
fn tip(bitmap: &Bitmap) -> (f32, f32) {
    const THRESHOLD: u8 = 96;
    for y in 0..bitmap.height {
        for x in 0..bitmap.width {
            if bitmap.alpha(x, y) >= THRESHOLD {
                let (dx, dy) = denominator(bitmap);
                return ((x as f32 / dx).clamp(0.0, 1.0), (y as f32 / dy).clamp(0.0, 1.0));
            }
        }
    }
    alpha_centroid(bitmap)
}

/// Picks the mode that suits the artwork.
///
/// The question is not "where is the mass" but "does this shape have a point".
/// An arrow's first opaque pixel sits in the very corner and its mass trails
/// away from it; a ring or a crosshair meets its bounding box at the *middle*
/// of the top edge. So the tip's own position decides, not the centroid — which
/// for an arrow sits well down the blade and would vote the wrong way.
///
/// Guessing this well is what makes "drop a PNG and it just works" true.
pub fn suggest(bitmap: &Bitmap) -> HotspotMode {
    let (tip_x, tip_y) = tip(bitmap);
    let (centroid_x, centroid_y) = alpha_centroid(bitmap);
    let points_from_the_corner = tip_x < 0.25 && tip_y < 0.25;
    let mass_trails_away = centroid_x > tip_x && centroid_y > tip_y;

    if points_from_the_corner && mass_trails_away {
        HotspotMode::TipDetect
    } else {
        HotspotMode::AlphaCentroid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arrow() -> Bitmap {
        // A crude triangle with its point at the top-left corner.
        let mut bitmap = Bitmap::new(16, 16);
        for y in 0..16u32 {
            for x in 0..=y.min(8) {
                bitmap.set_pixel(x, y, [255, 255, 255, 255]);
            }
        }
        bitmap
    }

    fn centred_ring() -> Bitmap {
        let mut bitmap = Bitmap::new(17, 17);
        for y in 0..17u32 {
            for x in 0..17u32 {
                let dx = x as f32 - 8.0;
                let dy = y as f32 - 8.0;
                let r = (dx * dx + dy * dy).sqrt();
                if (5.0..7.0).contains(&r) {
                    bitmap.set_pixel(x, y, [255, 255, 255, 255]);
                }
            }
        }
        bitmap
    }

    #[test]
    fn tip_detect_finds_the_point_of_an_arrow() {
        assert_eq!(compute(&arrow(), HotspotMode::TipDetect), (0.0, 0.0));
    }

    #[test]
    fn centroid_of_a_symmetric_ring_is_its_centre() {
        let (x, y) = compute(&centred_ring(), HotspotMode::AlphaCentroid);
        assert!((x - 0.5).abs() < 0.01, "x was {x}");
        assert!((y - 0.5).abs() < 0.01, "y was {y}");
    }

    #[test]
    fn suggestion_matches_the_shape() {
        assert_eq!(suggest(&arrow()), HotspotMode::TipDetect);
        assert_eq!(suggest(&centred_ring()), HotspotMode::AlphaCentroid);
    }

    #[test]
    fn an_empty_bitmap_never_produces_nan() {
        let empty = Bitmap::new(8, 8);
        for mode in [
            HotspotMode::Center,
            HotspotMode::TopLeft,
            HotspotMode::AlphaCentroid,
            HotspotMode::TipDetect,
            HotspotMode::Manual,
        ] {
            let (x, y) = compute(&empty, mode);
            assert!(x.is_finite() && y.is_finite());
            assert!((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y));
        }
    }

    #[test]
    fn a_single_pixel_bitmap_does_not_divide_by_zero() {
        let mut one = Bitmap::new(1, 1);
        one.set_pixel(0, 0, [255, 255, 255, 255]);
        assert_eq!(compute(&one, HotspotMode::AlphaCentroid), (0.0, 0.0));
    }
}
