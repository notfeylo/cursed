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

    let removed = cleared.iter().filter(|c| **c).count();

    // Refusing to gut the image is part of the job. If almost everything
    // matched the border, the "subject" was the background and clearing it
    // would leave nothing.
    let fraction = removed as f32 / (w * h) as f32;
    if fraction > 0.97 {
        return MatteReport { removed: 0.0, already_had_alpha: false };
    }

    for y in 0..h {
        for x in 0..w {
            if cleared[(y * w + x) as usize] {
                bitmap.set_pixel(x, y, [0, 0, 0, 0]);
            }
        }
    }

    feather(bitmap, &cleared);

    MatteReport { removed: fraction, already_had_alpha: false }
}

/// Softens the one-pixel boundary between kept and cleared.
///
/// A hard flood leaves a staircase, and a cursor is looked at closely at small
/// sizes where that is the most obvious thing about it.
fn feather(bitmap: &mut Bitmap, cleared: &[bool]) {
    let (w, h) = (bitmap.width, bitmap.height);
    let mut edges: Vec<(u32, u32, u8)> = Vec::new();

    for y in 0..h {
        for x in 0..w {
            if cleared[(y * w + x) as usize] {
                continue;
            }
            let mut neighbours = 0u32;
            let mut clear = 0u32;
            for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                neighbours += 1;
                if cleared[(ny as u32 * w + nx as u32) as usize] {
                    clear += 1;
                }
            }
            if clear > 0 && neighbours > 0 {
                let keep = 1.0 - (clear as f32 / neighbours as f32) * 0.55;
                let alpha = bitmap.alpha(x, y) as f32 * keep;
                edges.push((x, y, alpha as u8));
            }
        }
    }

    for (x, y, alpha) in edges {
        let [r, g, b, _] = bitmap.pixel(x, y);
        bitmap.set_pixel(x, y, [r, g, b, alpha]);
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

/// Chebyshev distance in RGB, which is stricter than Euclidean on a single
/// channel drifting — the way a coloured background usually differs.
fn near(pixel: [u8; 4], background: [u8; 4]) -> bool {
    if pixel[3] < 16 {
        return true;
    }
    let d = |a: u8, b: u8| (a as i32 - b as i32).abs();
    d(pixel[0], background[0]) <= TOLERANCE
        && d(pixel[1], background[1]) <= TOLERANCE
        && d(pixel[2], background[2]) <= TOLERANCE
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

    /// The subject's own colour appearing in the middle must survive, even when
    /// it is exactly the background colour. Only pixels connected to the edge
    /// are background.
    #[test]
    fn a_matching_colour_enclosed_by_the_subject_survives() {
        let mut b = solid(32, 32, [255, 255, 255, 255]);
        for y in 8..24 {
            for x in 8..24 {
                b.set_pixel(x, y, [10, 10, 10, 255]);
            }
        }
        // A white window in the middle of the dark square.
        for y in 14..18 {
            for x in 14..18 {
                b.set_pixel(x, y, [255, 255, 255, 255]);
            }
        }
        remove_background(&mut b);

        assert_eq!(b.alpha(0, 0), 0, "the outside went");
        assert_eq!(b.alpha(15, 15), 255, "the enclosed white did not");
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
