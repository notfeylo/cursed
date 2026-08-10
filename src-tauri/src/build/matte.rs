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

/// The tightest tolerance used, for a perfectly uniform background.
///
/// Not zero: even an exactly flat card has antialiasing along the subject's
/// edge, and a keyed edge with no slack at all leaves a hard, jagged outline —
/// the "pixels tearing up" look. This is enough to absorb that fringe and
/// nothing more.
const MIN_TOLERANCE: i32 = 10;

/// How much a neighbouring pixel may differ and still count as the same
/// surface. Small: this is the step across one pixel of a smooth gradient, not
/// across an edge.
const LOCAL_STEP: i32 = 14;

/// How far the flood may drift from the sampled background in total.
///
/// The leash on local growing. However many small steps it takes, a pixel can
/// only wander this far from where it started — otherwise a smooth ramp from
/// white to black lets the flood walk straight through the subject.
///
/// This was 110, which is most of the way from mid-grey to white, and it ate
/// carbon fibre alive: the weave's bright specks are each within one small step
/// of their neighbours, so the flood walked the highlights into the middle of
/// the subject and shredded it. Paired with the texture gate below, a much
/// shorter leash still follows any real gradient.
const MAX_DRIFT: i32 = 96;

/// The most a pixel's neighbourhood may vary for local growing to continue
/// through it.
///
/// This is the difference between "the background carries on here" and "this is
/// the subject, which happens to contain a pixel the colour of the backdrop".
///
/// A studio sweep, a vignette or a blurred photographic backdrop is *smooth* —
/// neighbouring pixels differ by very little, which is exactly why local growing
/// was needed to follow them. Carbon weave, knurling, mesh, glitter, printed
/// text and stitching are the opposite: high-frequency detail whose bright
/// specks sit within a step of a light background and whose dark ones sit within
/// a step of a dark background. Colour alone cannot tell those apart. Local
/// contrast can.
///
/// Generous enough to pass JPEG noise and film grain in a flat backdrop, tight
/// enough to stop at a textured surface.
const SMOOTH_ENOUGH: i32 = 34;

/// How much of the border must be transparent to call an image cut out.
///
/// Half. A subject that has been cut out leaves an edge that is mostly empty;
/// an image with a background has an edge that is mostly not. The old value was
/// 0.06 measured over the whole image, which rounded corners alone satisfied.
const ALREADY_TRANSPARENT: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatteReport {
    /// Fraction of the image cleared, 0.0–1.0.
    pub removed: f32,
    /// True when the image arrived with transparency and was left untouched.
    pub already_had_alpha: bool,
}

/// True when the image already has its background removed.
///
/// Decided from the **border**, not from the image as a whole, and that
/// distinction is the difference between working and not.
///
/// The old rule was "6% of all pixels are transparent". Rounded corners clear
/// that. A soft drop shadow clears it. A sticker saved with a transparent margin
/// clears it. All of those still have a solid background in the middle, and all
/// of them were skipped entirely — the single most common reason a background
/// "could not be removed" was that this returned true and nothing was even
/// attempted.
///
/// A genuinely cut-out subject has a transparent *edge*: that is what cutting it
/// out did. An image with a background has an opaque edge, whatever is going on
/// inside it.
pub fn already_cut_out(bitmap: &Bitmap) -> bool {
    let (w, h) = (bitmap.width, bitmap.height);
    if w < 3 || h < 3 {
        return false;
    }
    let mut clear = 0usize;
    let mut total = 0usize;
    let count = |x: u32, y: u32, clear: &mut usize, total: &mut usize| {
        *total += 1;
        if bitmap.alpha(x, y) < 16 {
            *clear += 1;
        }
    };
    for x in 0..w {
        count(x, 0, &mut clear, &mut total);
        count(x, h - 1, &mut clear, &mut total);
    }
    for y in 0..h {
        count(0, y, &mut clear, &mut total);
        count(w - 1, y, &mut clear, &mut total);
    }
    total > 0 && (clear as f32 / total as f32) >= ALREADY_TRANSPARENT
}

/// Removes a flat or near-flat background, in place, returning what it did.
pub fn remove_background(bitmap: &mut Bitmap) -> MatteReport {
    cut(bitmap, false)
}

/// The same, but ignoring the "this already has transparency" shortcut.
///
/// An image can carry an alpha channel and still have a background — a PNG
/// exported with a white card behind it, a GIF whose transparency only covers
/// the corners. The automatic path leaves those alone on purpose, because
/// re-cutting art that somebody already cut is how you lose a soft edge. When
/// the user asks for it explicitly, that caution is the wrong default.
pub fn remove_background_forced(bitmap: &mut Bitmap) -> MatteReport {
    cut(bitmap, true)
}

fn cut(bitmap: &mut Bitmap, force: bool) -> MatteReport {
    let (w, h) = (bitmap.width, bitmap.height);
    if w < 3 || h < 3 {
        return MatteReport { removed: 0.0, already_had_alpha: false };
    }
    if !force && already_cut_out(bitmap) {
        return MatteReport { removed: 0.0, already_had_alpha: true };
    }

    let Some(background) = sample_border(bitmap) else {
        return MatteReport { removed: 0.0, already_had_alpha: false };
    };

    // How much slack this particular image gets, from how uniform its border is.
    // A flat card is keyed almost exactly; a noisy photographic backdrop is given
    // room. See `border_spread`.
    let tolerance = tolerance_for(border_spread(bitmap, background));
    let soft = soft_for(tolerance);

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
        if near(bitmap.pixel(x, y), background, tolerance) {
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
        let here = bitmap.pixel(x, y);
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
            let pixel = bitmap.pixel(nx, ny);

            // Two ways in, and the second is what makes a varied background
            // come off whole.
            //
            // Matching the sampled background handles a flat card. But a
            // gradient, a vignette, or a photo's blurred backdrop drifts well
            // past any single tolerance, so a flood that only asks "is this the
            // background colour" stops a third of the way in and leaves the rest
            // behind. Asking instead whether this pixel continues smoothly from
            // the one it was reached from follows that drift, while still
            // stopping dead at a subject edge — which is precisely where the
            // step between neighbouring pixels becomes large.
            //
            // The leash is `MAX_DRIFT`: however many small steps it takes, a
            // pixel can only wander so far from the colour it started at. Without
            // that, a smooth ramp from white to black would let the flood walk
            // all the way across the subject.
            // The texture gate is the third condition, and it is what stops a
            // detailed subject being walked into. Local growing is only allowed
            // to continue through a pixel that sits in a *smooth* neighbourhood;
            // an exact match on the sampled background still gets through
            // anywhere, because that is not a guess.
            let joins = near(pixel, background, tolerance)
                || (distance(pixel, here) <= LOCAL_STEP
                    && distance(pixel, background) <= MAX_DRIFT
                    && local_contrast(bitmap, nx, ny) <= SMOOTH_ENOUGH);

            if joins {
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
    enclose_background(bitmap, background, &mut cleared, tolerance);

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
                    let alpha = graded_alpha(distance(pixel, background), tolerance, soft);
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
                if d < soft {
                    let alpha = graded_alpha(d, tolerance, soft).max(1);
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
/// A region has to *start* on an exact match — the seed is the claim that this
/// is background at all, and it is not a guess worth making loosely. From there
/// it grows by the same rule as the border flood: exact match, or a smooth step
/// through untextured pixels within the drift leash.
///
/// It used to grow on the exact match alone, and that left a visible ring of old
/// background inside anything with a hole in it. The two openings of a steering
/// wheel are lit from one side, so the card behind them carries a soft shadow;
/// the lit half matched and cleared, the shaded half did not, and what survived
/// was a bright crescent stuck to the inside of the rim. The gradient there is
/// the same kind of gradient the border flood already follows, and there was no
/// reason for these two to disagree about it.
///
/// The texture gate is what makes that safe. Following a shadow across a smooth
/// card is not the same permission as walking into a subject, because a subject
/// with any detail in it fails the smoothness test at its own edge.
fn enclose_background(
    bitmap: &Bitmap,
    background: [u8; 4],
    cleared: &mut [bool],
    tolerance: i32,
) {
    let (w, h) = (bitmap.width, bitmap.height);
    let mut visited = vec![false; (w * h) as usize];

    for sy in 0..h {
        for sx in 0..w {
            let start = (sy * w + sx) as usize;
            if cleared[start] || visited[start] {
                continue;
            }
            if distance(bitmap.pixel(sx, sy), background) > tolerance {
                continue;
            }

            // Gather the whole connected region before deciding, so the decision
            // is made once for the region rather than per pixel.
            let mut region = Vec::new();
            let mut stack = vec![(sx, sy)];
            visited[start] = true;
            while let Some((x, y)) = stack.pop() {
                region.push((x, y));
                let here = bitmap.pixel(x, y);
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
                    let pixel = bitmap.pixel(nx, ny);
                    let joins = near(pixel, background, tolerance)
                        || (distance(pixel, here) <= LOCAL_STEP
                            && distance(pixel, background) <= MAX_DRIFT
                            && local_contrast(bitmap, nx, ny) <= SMOOTH_ENOUGH);
                    if joins {
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

/// How much the 3×3 neighbourhood around a pixel varies, as the largest
/// per-channel spread across it.
///
/// Cheap on purpose — this runs for every candidate the flood considers, and a
/// proper variance would cost a multiply per channel per neighbour to tell us
/// the same thing. Spread answers the only question being asked: is this a flat
/// region, or is there detail here?
///
/// Edge pixels clamp to the image rather than wrapping, so the border — where
/// the flood starts, and where a backdrop is flattest — is never mistaken for
/// texture.
fn local_contrast(bitmap: &Bitmap, x: u32, y: u32) -> i32 {
    let (w, h) = (bitmap.width, bitmap.height);
    let (mut lo, mut hi) = ([255i32; 3], [0i32; 3]);

    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            let sx = (x as i32 + dx).clamp(0, w as i32 - 1) as u32;
            let sy = (y as i32 + dy).clamp(0, h as i32 - 1) as u32;
            let pixel = bitmap.pixel(sx, sy);
            for channel in 0..3 {
                let value = pixel[channel] as i32;
                lo[channel] = lo[channel].min(value);
                hi[channel] = hi[channel].max(value);
            }
        }
    }

    (0..3).map(|c| hi[c] - lo[c]).max().unwrap_or(0)
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

fn near(pixel: [u8; 4], background: [u8; 4], tolerance: i32) -> bool {
    pixel[3] < 16 || distance(pixel, background) <= tolerance
}

/// How uniform the border is, as the median absolute deviation from the sampled
/// background colour.
///
/// This is what decides how much slack the rest of the cut is allowed. The two
/// kinds of background need opposite treatment and cannot share a constant:
///
/// - A **flat** backdrop — an exported card, a studio sweep, the grey overlay a
///   background remover puts behind its preview — is uniform to within a couple
///   of levels. It can be keyed exactly, and *must* be, because a subject that
///   happens to be grey is only a few levels away and a loose tolerance eats it.
/// - A **photographic** backdrop carries noise, grain and JPEG ringing. Keying
///   it tightly leaves a confetti of speckles behind.
///
/// A single tolerance of 38 was the compromise, and it failed the first case
/// badly: on a flat grey card it took the grey parts of the subject with it.
fn border_spread(bitmap: &Bitmap, background: [u8; 4]) -> i32 {
    let (w, h) = (bitmap.width, bitmap.height);
    let mut deviations: Vec<i32> = Vec::new();
    for x in 0..w {
        deviations.push(distance(bitmap.pixel(x, 0), background));
        deviations.push(distance(bitmap.pixel(x, h - 1), background));
    }
    for y in 0..h {
        deviations.push(distance(bitmap.pixel(0, y), background));
        deviations.push(distance(bitmap.pixel(w - 1, y), background));
    }
    if deviations.is_empty() {
        return 0;
    }
    deviations.sort_unstable();
    // Median, not mean: a subject touching the edge shows up as a handful of
    // enormous deviations, and a mean would let those declare a flat card noisy.
    deviations[deviations.len() / 2]
}

/// The tolerance to use for an image, from how uniform its border is.
///
/// Floored well above zero so a perfectly flat card still absorbs its own
/// antialiasing, and capped at the old fixed value so this can only ever be
/// tighter than the behaviour it replaces, never looser.
fn tolerance_for(spread: i32) -> i32 {
    (MIN_TOLERANCE + spread * 3).clamp(MIN_TOLERANCE, TOLERANCE)
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

/// The soft band for a given tolerance, keeping the original 38:96 proportion.
///
/// It scales with the tolerance rather than staying fixed, because the band's
/// job is to cover the blend between subject and background — and on a flat card
/// that blend is only a few levels wide. Holding it at 96 while the tolerance
/// dropped to 10 would grade most of a grey subject to partial alpha, which
/// looks exactly like the washed-out, see-through edge it exists to prevent.
fn soft_for(tolerance: i32) -> i32 {
    (tolerance * SOFT_TOLERANCE / TOLERANCE).max(tolerance + 1)
}

/// Alpha for a pixel at a given distance from the background, 0–255.
fn graded_alpha(distance: i32, tolerance: i32, soft: i32) -> u8 {
    if distance <= tolerance {
        return 0;
    }
    if distance >= soft {
        return 255;
    }
    let t = (distance - tolerance) as f32 / (soft - tolerance) as f32;
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

    /// A subject that has genuinely been cut out is left alone.
    ///
    /// The test for that is a transparent *border*, not a transparent count.
    /// This image has a clear edge all the way round, which is what cutting a
    /// subject out leaves behind.
    #[test]
    fn an_image_that_is_already_cut_out_is_left_alone() {
        let mut b = solid(32, 32, [0, 0, 0, 0]);
        for y in 10..22 {
            for x in 10..22 {
                b.set_pixel(x, y, [20, 40, 200, 255]);
            }
        }
        let before = b.pixel(16, 16);
        let report = remove_background(&mut b);

        assert!(report.already_had_alpha);
        assert_eq!(report.removed, 0.0);
        assert_eq!(b.pixel(16, 16), before, "nothing was touched");
    }

    /// The bug this replaces: an image with a solid background and a little
    /// transparency somewhere was skipped entirely.
    ///
    /// Rounded corners, a soft shadow, a transparent margin — any of those used
    /// to satisfy "6% of pixels are transparent" and the background was never
    /// even looked at. It is the border that decides, and this one is opaque.
    #[test]
    fn transparency_somewhere_does_not_excuse_a_background() {
        let mut b = solid(32, 32, [255, 255, 255, 255]);
        // A transparent notch, well away from the edges.
        for y in 2..8 {
            for x in 2..8 {
                b.set_pixel(x, y, [0, 0, 0, 0]);
            }
        }
        for y in 12..24 {
            for x in 12..24 {
                b.set_pixel(x, y, [10, 30, 190, 255]);
            }
        }
        let report = remove_background(&mut b);

        assert!(!report.already_had_alpha, "an opaque border means a background");
        assert!(report.removed > 0.5, "only {:.2} removed", report.removed);
        assert_eq!(b.alpha(16, 16), 255, "the subject survives");
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

    /// A textured subject on a background of a similar tone must survive whole.
    ///
    /// This is the carbon-fibre case, and it is the one local growing got badly
    /// wrong. The subject here is a weave: every other pixel is close to the
    /// background's own grey, so there is a continuous path of
    /// nearly-background-coloured pixels leading from the edge into the middle
    /// of it. Following that path is exactly what shredded a steering wheel into
    /// lace — each individual step looked like more background.
    ///
    /// What separates them is not colour, it is local contrast: the backdrop is
    /// flat and the weave is not.
    #[test]
    fn a_textured_subject_is_not_walked_into_by_the_flood() {
        let mut b = Bitmap::new(40, 40);
        for y in 0..40 {
            for x in 0..40 {
                b.set_pixel(x, y, [128, 128, 128, 255]);
            }
        }
        // A woven block: alternating light and dark, where the light threads sit
        // within a step or two of the background.
        for y in 10..30 {
            for x in 10..30 {
                let light = (x + y) % 2 == 0;
                let v = if light { 140 } else { 30 };
                b.set_pixel(x, y, [v, v, v, 255]);
            }
        }

        let report = remove_background(&mut b);

        assert!(report.removed > 0.4, "the flat surround should still go");
        // Every thread of the weave, light ones included, has to survive.
        let mut lost = 0;
        for y in 12..28 {
            for x in 12..28 {
                if b.alpha(x, y) < 200 {
                    lost += 1;
                }
            }
        }
        assert_eq!(lost, 0, "{lost} pixels of a textured subject were eaten");
    }

    /// Sharing a colour with the background is not the same as being it.
    ///
    /// A flat card keyed loosely takes the subject's mid-tones with it. The
    /// tolerance is chosen from how uniform the border is, so an exactly flat
    /// background is keyed tightly and a grey subject on a grey card survives.
    #[test]
    fn a_flat_background_is_keyed_tightly_enough_to_spare_a_similar_subject() {
        assert_eq!(tolerance_for(0), MIN_TOLERANCE, "a flat card gets no slack");
        assert!(
            tolerance_for(0) < TOLERANCE,
            "the old fixed tolerance is the ceiling, never the floor"
        );
        // A noisy photographic border earns its slack back, up to the old value.
        assert_eq!(tolerance_for(50), TOLERANCE);
        assert!(tolerance_for(4) > tolerance_for(0));
    }
}
