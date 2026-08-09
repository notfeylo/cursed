//! Cutting the background out of an image that never had one removed.
//!
//! "Turn anything into a cursor" means accepting a screenshot, a JPEG off a
//! search page, a sticker with a white card behind it — none of which carry
//! alpha. Pasted straight into a cursor those become a rectangle of background
//! dragged around the screen, which is worse than not importing at all.
//!
//! What this does is deliberately narrow, because the alternative is a matting
//! model and this is a 2 MB app that runs offline:
//!
//!  1. If the image already has real transparency, leave it completely alone.
//!     Somebody who cut their own image out has already answered the question.
//!  2. Otherwise take the background colour from the border pixels, and flood
//!     from every edge inward, clearing anything close enough to it.
//!  3. Feather the boundary by one pixel so the result has a soft edge rather
//!     than a staircase.
//!
//! It is honest about what it is: this removes *a* background — flat, gradient,
//! or near-flat — and it will not cut a subject out of a busy photograph. It
//! reports how much it removed so the caller can tell the difference between a
//! clean cut and a no-op.

use crate::build::bitmap::Bitmap;

/// How far a pixel may sit from the sampled background and still be background.
///
/// Generous enough for JPEG ringing and a soft gradient, tight enough that a
/// subject sharing a hue with its backdrop survives.
const TOLERANCE: i32 = 38;

/// Above this, the image is treated as already cut out.
const ALREADY_TRANSPARENT: f32 = 0.06;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatteReport {
    /// Fraction of the image cleared, 0.0–1.0.
    pub removed: f32,
    /// True when the image arrived with transparency and was left untouched.
    pub already_had_alpha: bool,
}

/// True when enough of the image is already transparent to call it cut out.
pub fn has_transparency(bitmap: &Bitmap) -> bool {
    let total = (bitmap.width * bitmap.height) as f32;
    if total == 0.0 {
        return false;
    }
    let clear = (0..bitmap.height)
        .flat_map(|y| (0..bitmap.width).map(move |x| (x, y)))
        .filter(|&(x, y)| bitmap.alpha(x, y) < 16)
        .count() as f32;
    clear / total >= ALREADY_TRANSPARENT
}

/// Removes a flat or near-flat background, in place, returning what it did.
pub fn remove_background(bitmap: &mut Bitmap) -> MatteReport {
    let (w, h) = (bitmap.width, bitmap.height);
    if w < 3 || h < 3 {
        return MatteReport { removed: 0.0, already_had_alpha: false };
    }
    if has_transparency(bitmap) {
        return MatteReport { removed: 0.0, already_had_alpha: true };
    }

    let Some(background) = sample_border(bitmap) else {
        return MatteReport { removed: 0.0, already_had_alpha: false };
    };

    // Flood from the edges rather than clearing every matching pixel anywhere.
    // A white shirt in the middle of the subject matches the white background
    // exactly; the difference is that it is not connected to the edge.
    let mut cleared = vec![false; (w * h) as usize];
    let mut stack: Vec<(u32, u32)> = Vec::new();
    let push = |x: u32, y: u32, stack: &mut Vec<(u32, u32)>, cleared: &mut Vec<bool>| {
        let i = (y * w + x) as usize;
        if cleared[i] {
            return;
        }
        if near(bitmap.pixel(x, y), background) {
            cleared[i] = true;
            stack.push((x, y));
        }
    };

    for x in 0..w {
        push(x, 0, &mut stack, &mut cleared);
        push(x, h - 1, &mut stack, &mut cleared);
    }
    for y in 0..h {
        push(0, y, &mut stack, &mut cleared);
        push(w - 1, y, &mut stack, &mut cleared);
    }

    while let Some((x, y)) = stack.pop() {
        for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                continue;
            }
            let (nx, ny) = (nx as u32, ny as u32);
            let i = (ny * w + nx) as usize;
            if cleared[i] {
                continue;
            }
            if near(bitmap.pixel(nx, ny), background) {
                cleared[i] = true;
                stack.push((nx, ny));
            }
        }
    }

    // Background the edge flood cannot reach.
    //
    // Flooding inward only finds background *connected to the border*. The hole
    // in an O, the gap between two fingers, the space inside a handle — all of
    // that is enclosed by the subject, so it survived and the cut-out came back
    // with patches of the old card still in it. That is the "background colour
    // is still there" complaint, and it is not a tolerance problem: those pixels
    // were never considered.
    //
    // They are cleared on the tight tolerance rather than the soft band, so a
    // region has to actually be the background colour, not merely resemble it.
    enclose_background(bitmap, background, &mut cleared);

    // Single pixels of noise inside an otherwise clean background.
    //
    // A JPEG leaves ringing around a hard edge, so the flood stops at scattered
    // pixels that are a shade off. Left alone they read as dirt in the
    // transparent area — the "small gaps" in reverse.
    despeckle(&mut cleared, w, h);

    let removed = cleared.iter().filter(|c| **c).count();

    // Refusing to gut the image is part of the job. If almost everything
    // matched the border, the "subject" was the background and clearing it
    // would leave nothing.
    let fraction = removed as f32 / (w * h) as f32;
    if fraction > 0.97 {
        return MatteReport { removed: 0.0, already_had_alpha: false };
    }

    // Second pass: grade the boundary instead of cutting it.
    //
    // The flood decided which pixels are *reachable* background. This decides
    // how much background each one is. A pixel deep inside the flood goes
    // completely; one sitting on the boundary — a hair, an antialiased edge, the
    // soft side of a shadow — keeps the partial alpha it always had, and has the
    // background's colour taken back out of it so it does not read as a pale
    // fringe.
    let mut softened = 0usize;
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let pixel = bitmap.pixel(x, y);

            if cleared[i] {
                // Reachable background. Fully clear unless it sits against a
                // kept pixel, where it is likely a blend of the two.
                let touching_subject = [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|(dx, dy)| {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        nx >= 0
                            && ny >= 0
                            && nx < w as i32
                            && ny < h as i32
                            && !cleared[(ny as u32 * w + nx as u32) as usize]
                    });
                if touching_subject {
                    let alpha = graded_alpha(distance(pixel, background));
                    if alpha > 0 {
                        bitmap.set_pixel(x, y, unblend(pixel, background, alpha));
                        softened += 1;
                        continue;
                    }
                }
                bitmap.set_pixel(x, y, [0, 0, 0, 0]);
            } else {
                // Kept. Grading only ever applies at the boundary — a pixel has
                // to actually touch the flood to be a blend of it.
                //
                // Grading every kept pixel that merely *resembles* the
                // background destroys enclosed regions: a white window inside a
                // dark subject is exactly the background colour and nowhere near
                // the background, and it must stay opaque. The flood already
                // encodes that distinction; adjacency is how to read it.
                let touching_background = [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|(dx, dy)| {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        nx >= 0
                            && ny >= 0
                            && nx < w as i32
                            && ny < h as i32
                            && cleared[(ny as u32 * w + nx as u32) as usize]
                    });
                if !touching_background {
                    continue;
                }
                let d = distance(pixel, background);
                if d < SOFT_TOLERANCE {
                    let alpha = graded_alpha(d).max(1);
                    let blended = unblend(pixel, background, alpha);
                    let combined =
                        [blended[0], blended[1], blended[2], alpha.min(pixel[3])];
                    bitmap.set_pixel(x, y, combined);
                    softened += 1;
                }
            }
        }
    }
    log::debug!("{softened} pixels graded rather than cut");

    MatteReport { removed: fraction, already_had_alpha: false }
}


/// Clears regions of background colour that the border flood never reached.
///
/// Deliberately uses the tight tolerance. The soft band exists for pixels that
/// are *part* background, which only makes sense against a real boundary; using
/// it here would eat any subject colour that happened to sit near the
/// background's.
fn enclose_background(bitmap: &Bitmap, background: [u8; 4], cleared: &mut [bool]) {
    let (w, h) = (bitmap.width, bitmap.height);
    let mut visited = vec![false; (w * h) as usize];

    for sy in 0..h {
        for sx in 0..w {
            let start = (sy * w + sx) as usize;
            if cleared[start] || visited[start] {
                continue;
            }
            if distance(bitmap.pixel(sx, sy), background) > TOLERANCE {
                continue;
            }

            // Gather the whole connected region before deciding, so the decision
            // is made once for the region rather than per pixel.
            let mut region = Vec::new();
            let mut stack = vec![(sx, sy)];
            visited[start] = true;
            while let Some((x, y)) = stack.pop() {
                region.push((x, y));
                for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let (nx, ny) = (nx as u32, ny as u32);
                    let i = (ny * w + nx) as usize;
                    if visited[i] || cleared[i] {
                        continue;
                    }
                    if distance(bitmap.pixel(nx, ny), background) <= TOLERANCE {
                        visited[i] = true;
                        stack.push((nx, ny));
                    }
                }
            }

            for (x, y) in region {
                cleared[(y * w + x) as usize] = true;
            }
        }
    }
}

/// Clears lone opaque pixels stranded inside the background.
///
/// Only a pixel whose four neighbours are all background. Anything larger is
/// part of something, and guessing about it is how a cut-out loses detail it
/// should have kept.
fn despeckle(cleared: &mut [bool], w: u32, h: u32) {
    let mut lonely = Vec::new();
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = (y * w + x) as usize;
            if cleared[i] {
                continue;
            }
            let neighbours = [
                ((y - 1) * w + x) as usize,
                ((y + 1) * w + x) as usize,
                (y * w + x - 1) as usize,
                (y * w + x + 1) as usize,
            ];
            if neighbours.iter().all(|&n| cleared[n]) {
                lonely.push(i);
            }
        }
    }
    for i in lonely {
        cleared[i] = true;
    }
}

/// The dominant colour around the border.
///
/// The median of the edge pixels rather than the mean: a mean is dragged by any
/// part of the subject that touches the edge, a median is not.
fn sample_border(bitmap: &Bitmap) -> Option<[u8; 4]> {
    let (w, h) = (bitmap.width, bitmap.height);
    let mut samples: Vec<[u8; 4]> = Vec::new();
    for x in 0..w {
        samples.push(bitmap.pixel(x, 0));
        samples.push(bitmap.pixel(x, h - 1));
    }
    for y in 0..h {
        samples.push(bitmap.pixel(0, y));
        samples.push(bitmap.pixel(w - 1, y));
    }
    if samples.is_empty() {
        return None;
    }
    let channel = |index: usize| -> u8 {
        let mut values: Vec<u8> = samples.iter().map(|p| p[index]).collect();
        values.sort_unstable();
        values[values.len() / 2]
    };
    Some([channel(0), channel(1), channel(2), 255])
}

/// Chebyshev distance from the background colour.
///
/// Chebyshev rather than Euclidean because a background usually differs from the
/// subject in one channel more than the others, and Euclidean dilutes that by
/// averaging it away.
fn distance(pixel: [u8; 4], background: [u8; 4]) -> i32 {
    let d = |a: u8, b: u8| (a as i32 - b as i32).abs();
    d(pixel[0], background[0])
        .max(d(pixel[1], background[1]))
        .max(d(pixel[2], background[2]))
}

fn near(pixel: [u8; 4], background: [u8; 4]) -> bool {
    pixel[3] < 16 || distance(pixel, background) <= TOLERANCE
}

/// Everything between `TOLERANCE` and `SOFT_TOLERANCE` is *partly* background.
///
/// This is what a hard threshold cannot do. A hair, an antialiased edge or a
/// drop shadow is a blend of subject and background in one pixel, and any single
/// cutoff either keeps it — leaving a pale fringe of the old background — or
/// discards it, chewing a notch out of the edge. Grading the alpha across the
/// band keeps the pixel and admits it is partial, which is what the original
/// image already encoded.
const SOFT_TOLERANCE: i32 = 96;

/// Alpha for a pixel at a given distance from the background, 0–255.
fn graded_alpha(distance: i32) -> u8 {
    if distance <= TOLERANCE {
        return 0;
    }
    if distance >= SOFT_TOLERANCE {
        return 255;
    }
    let t = (distance - TOLERANCE) as f32 / (SOFT_TOLERANCE - TOLERANCE) as f32;
    // Smoothstep rather than linear: a linear ramp leaves a visible band where
    // it meets full opacity.
    let eased = t * t * (3.0 - 2.0 * t);
    (eased * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Removes the background's colour from a partly-transparent pixel.
///
/// A pixel that is 40% subject over a white card is not 40% of the subject's
/// colour — it is the subject blended *with* white, so it reads pale. Undoing
/// that blend is what stops a cut-out having a bright halo everywhere its edge
/// used to touch the background. This is the same correction compositors call
/// despill, done against the sampled background rather than a fixed key colour.
fn unblend(pixel: [u8; 4], background: [u8; 4], alpha: u8) -> [u8; 4] {
    if alpha == 0 || alpha == 255 {
        return [pixel[0], pixel[1], pixel[2], alpha];
    }
    let a = alpha as f32 / 255.0;
    let recover = |c: u8, b: u8| -> u8 {
        let value = (c as f32 - b as f32 * (1.0 - a)) / a;
        value.clamp(0.0, 255.0) as u8
    };
    [
        recover(pixel[0], background[0]),
        recover(pixel[1], background[1]),
        recover(pixel[2], background[2]),
        alpha,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Bitmap {
        let mut b = Bitmap::new(w, h);
        for y in 0..h {
            for x in 0..w {
                b.set_pixel(x, y, rgba);
            }
        }
        b
    }

    #[test]
    fn a_shape_on_a_white_card_is_cut_out() {
        let mut b = solid(32, 32, [255, 255, 255, 255]);
        for y in 10..22 {
            for x in 10..22 {
                b.set_pixel(x, y, [20, 40, 200, 255]);
            }
        }
        let report = remove_background(&mut b);

        assert!(!report.already_had_alpha);
        assert!(report.removed > 0.7, "most of a white card should go");
        assert_eq!(b.alpha(1, 1), 0, "corner is background");
        assert_eq!(b.alpha(16, 16), 255, "the subject survives untouched");
    }

    #[test]
    fn an_image_that_already_has_alpha_is_left_alone() {
        let mut b = solid(32, 32, [255, 255, 255, 255]);
        for y in 0..12 {
            for x in 0..32 {
                b.set_pixel(x, y, [0, 0, 0, 0]);
            }
        }
        let before = b.pixel(16, 20);
        let report = remove_background(&mut b);

        assert!(report.already_had_alpha);
        assert_eq!(report.removed, 0.0);
        assert_eq!(b.pixel(16, 20), before, "nothing was touched");
    }

    /// Background enclosed by the subject is still background.
    ///
    /// This reverses an earlier decision here, deliberately. The first version
    /// protected any region the border flood could not reach, on the theory that
    /// a subject might legitimately contain the background's colour. In practice
    /// that is what left patches of the old card inside a cut-out — the hole in
    /// an O, the gap between two fingers — and "there is still white in it" is a
    /// bug report, not a design choice.
    ///
    /// The safeguard is the tolerance: an enclosed region has to actually be the
    /// background colour, not merely resemble it.
    #[test]
    fn background_enclosed_by_the_subject_is_removed_too() {
        let mut b = solid(32, 32, [255, 255, 255, 255]);
        for y in 8..24 {
            for x in 8..24 {
                b.set_pixel(x, y, [10, 10, 10, 255]);
            }
        }
        // A hole in the middle of the subject, the colour of the card behind it.
        for y in 14..18 {
            for x in 14..18 {
                b.set_pixel(x, y, [255, 255, 255, 255]);
            }
        }
        remove_background(&mut b);

        assert_eq!(b.alpha(0, 0), 0, "the outside went");
        assert_eq!(b.alpha(15, 15), 0, "and so did the hole");
        assert_eq!(b.alpha(10, 10), 255, "the subject itself is untouched");
    }

    /// A subject colour that is merely *near* the background must survive being
    /// enclosed, or a cut-out loses its own light areas.
    #[test]
    fn an_enclosed_colour_that_only_resembles_the_background_survives() {
        let mut b = solid(32, 32, [255, 255, 255, 255]);
        for y in 8..24 {
            for x in 8..24 {
                b.set_pixel(x, y, [10, 10, 10, 255]);
            }
        }
        // Well outside TOLERANCE of white, so it is the subject's own colour.
        for y in 14..18 {
            for x in 14..18 {
                b.set_pixel(x, y, [170, 170, 170, 255]);
            }
        }
        remove_background(&mut b);
        assert_eq!(b.alpha(15, 15), 255, "a near-miss is not the background");
    }

    /// An image that is entirely background should come back untouched rather
    /// than emptied — there would be no cursor left otherwise.
    #[test]
    fn an_image_with_no_subject_is_refused_rather_than_gutted() {
        let mut b = solid(24, 24, [200, 200, 200, 255]);
        let report = remove_background(&mut b);
        assert_eq!(report.removed, 0.0);
        assert_eq!(b.alpha(12, 12), 255, "still opaque");
    }

    /// The complaint this exists to answer: "it still leaves some white".
    ///
    /// A subject drawn with an antialiased edge blends into its background over
    /// a pixel or two. A hard threshold either keeps those — a pale rim of the
    /// old card, visible against any dark desktop — or discards them, chewing a
    /// notch out of the shape. Neither is acceptable on something magnified on
    /// screen all day.
    #[test]
    fn an_antialiased_edge_leaves_no_pale_fringe() {
        let mut b = solid(40, 40, [255, 255, 255, 255]);
        // A dark disc with a soft, blended edge, the way any real artwork is.
        let (cx, cy, r) = (20.0f32, 20.0f32, 12.0f32);
        for y in 0..40 {
            for x in 0..40 {
                let d = (((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt() - r) / 2.0;
                let coverage = (1.0 - d).clamp(0.0, 1.0);
                if coverage <= 0.0 {
                    continue;
                }
                // Subject over white, exactly as a renderer would composite it.
                let v = (255.0 * (1.0 - coverage) + 20.0 * coverage) as u8;
                b.set_pixel(x, y, [v, v, v, 255]);
            }
        }

        let report = remove_background(&mut b);
        assert!(report.removed > 0.4);

        // Nothing may survive that is both mostly opaque and nearly white.
        // That combination is the fringe, and it is what a hard cut leaves.
        let mut fringe = 0;
        for y in 0..40 {
            for x in 0..40 {
                let [r, g, bl, a] = b.pixel(x, y);
                if a > 160 && r > 225 && g > 225 && bl > 225 {
                    fringe += 1;
                }
            }
        }
        assert_eq!(fringe, 0, "{fringe} pixels of the old background survived");

        // And the subject itself is still solid, not eaten away.
        assert_eq!(b.alpha(20, 20), 255);
    }

    /// Partial pixels must come back as the subject's colour, not a pale
    /// version of it blended with whatever used to be behind it.
    #[test]
    fn a_partial_pixel_has_the_background_taken_back_out_of_it() {
        // Half-covered black over white reads as mid grey in the file.
        let blended = [128u8, 128, 128, 255];
        let recovered = unblend(blended, [255, 255, 255, 255], 128);
        assert!(
            recovered[0] < 30,
            "expected near-black after unblending, got {}",
            recovered[0]
        );
        assert_eq!(recovered[3], 128, "alpha is preserved");
    }

    #[test]
    fn a_gradient_background_still_goes() {
        let mut b = Bitmap::new(40, 40);
        for y in 0..40 {
            for x in 0..40 {
                let v = 210 + (y / 8) as u8; // a gentle vertical ramp
                b.set_pixel(x, y, [v, v, v, 255]);
            }
        }
        for y in 16..26 {
            for x in 16..26 {
                b.set_pixel(x, y, [255, 30, 30, 255]);
            }
        }
        let report = remove_background(&mut b);
        assert!(report.removed > 0.6, "a soft ramp is still one background");
        assert_eq!(b.alpha(20, 20), 255);
    }
}
