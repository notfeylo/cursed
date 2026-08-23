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
//!  4. Sweep up what is left: islands of background too small and too close to
//!     the background's colour to be anything else, and single faint pixels
//!     with nothing around them. This is the difference between a cut that is
//!     correct and one that looks clean — the eye finds one grey fleck on a
//!     transparent background immediately.
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
    /// Set when nothing was removed **on purpose**. The bitmap is untouched.
    pub refused: Option<Refusal>,
    /// What the image looked like before anything was attempted.
    pub keyability: Keyability,
}

impl MatteReport {
    /// Nothing was attempted because nothing was asked for — `Cut::Keep`.
    pub fn not_attempted() -> Self {
        Self::untouched(Keyability {
            confident: true,
            ..Keyability::default()
        })
    }

    fn untouched(keyability: Keyability) -> Self {
        Self {
            removed: 0.0,
            already_had_alpha: false,
            refused: None,
            keyability,
        }
    }

    /// What a **learned** matte did, reported in the same shape as a keyed one.
    ///
    /// Photo mode produces an alpha channel rather than a flood fill, and every
    /// consumer of this report — the banner, the toggle's hint, the preview —
    /// only ever asks how much came off and whether anything was refused. Those
    /// questions have the same answers either way, so they get the same struct
    /// rather than a parallel one.
    pub fn learned(removed: f32, keyability: Keyability) -> Self {
        Self {
            removed,
            already_had_alpha: false,
            refused: None,
            keyability,
        }
    }

    /// A refusal raised outside this module — photo mode's own sanity check on
    /// what the model gave back.
    pub fn refused(reason: Refusal, keyability: Keyability) -> Self {
        Self::refusing(reason, keyability)
    }

    fn refusing(reason: Refusal, keyability: Keyability) -> Self {
        Self {
            removed: 0.0,
            already_had_alpha: false,
            refused: Some(reason),
            keyability,
        }
    }
}

/// Why a removal did not happen.
///
/// **A refusal is a result, not a failure.** The alternative — attempting a key
/// that cannot work and returning whatever came out — is how a user gets an
/// unrecognisable dark blob back from a photograph of a football on grass, with
/// nothing anywhere saying why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Refusal {
    /// The background is not a background: a gradient, a vignette, a texture, or
    /// all three. No tolerance value produces a correct result on this image, so
    /// none is tried.
    LooksLikeAPhotograph,
    /// A key was attempted and what came back was not a subject. Reverted.
    WouldHaveEatenTheSubject,
    /// The key found almost nothing to remove, so the image is returned as it
    /// arrived rather than with a few hundred pixels nibbled off its corners.
    BarelyMoved,
}

impl Refusal {
    /// What to tell the user. Plain, specific, and never blaming them.
    pub fn message(self) -> &'static str {
        match self {
            Refusal::LooksLikeAPhotograph => {
                "This looks like a photo. Automatic background removal works on flat \
                 backgrounds — logos, icons, screenshots — and it will not do a good job \
                 here. Use the image as it is, or cut it out yourself in the editor."
            }
            Refusal::WouldHaveEatenTheSubject => {
                "Removing the background would have taken the subject with it, so the \
                 image was left as it is. Try the editor if you want to cut it by hand."
            }
            Refusal::BarelyMoved => {
                "There was no background to find, so the image was left as it is."
            }
        }
    }
}

/// How well an image will key, measured before anything is attempted.
///
/// Four independent signals, because each one alone has a hole. A flat card with
/// heavy grain passes the corner test and fails the variance one; a two-tone
/// gradient passes the variance test and fails the corner one.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Keyability {
    /// The largest colour distance between any two of the four corner patches.
    ///
    /// A vignette is exactly this: the corners are darker than the middle, and
    /// darker by different amounts. A flat card's corners agree to within noise.
    pub corner_disagreement: i32,
    /// How much the perimeter band varies from its own median.
    ///
    /// Grass, fabric, foliage and carpet all read here. Texture cannot be
    /// flood-filled at any tolerance: the values that make it up are further
    /// from each other than they are from the subject.
    pub border_variance: i32,
    /// Distinct colours per thousand border pixels, quantised to 32 levels per
    /// channel.
    ///
    /// A rate rather than a count. A count is a function of the image's size as
    /// much as its content — the same photograph measures three times as many
    /// colours at 4K as at 720p — so a threshold on one is a threshold that
    /// means something different for every import.
    pub border_colour_density: f32,
    /// Fraction of the border band sitting on a strong local edge, 0.0–1.0.
    pub border_edge_density: f32,
    /// True when all four signals say this is a flat background.
    pub confident: bool,
}

// The four thresholds.
//
// Chosen against measurements, not intuition — the first set was guessed and
// let the football photograph through on three signals out of four. The numbers
// each case actually produces are in `docs/verification/background-removal.md`;
// what matters here is that every threshold sits in a gap with room on both
// sides, and that no single one is load-bearing on its own.

/// Corner patches disagreeing by more than this is a vignette or a gradient.
///
/// Flat cards measure 0–1. The football photograph measures 42. A logo on a
/// deliberately graded card — a real and legitimate thing to import — lands in
/// the twenties, so the line goes above that and below the photograph.
const MAX_CORNER_DISAGREEMENT: i32 = 40;

/// Perimeter variation above this is texture rather than sensor noise.
///
/// The weakest of the four, and kept because it is the only one that fires on a
/// *bright* textured background. A vignette compresses the border's values
/// toward black, which drags this figure down precisely when the background is
/// least keyable — the football photograph measures only 14 here.
const MAX_BORDER_VARIANCE: i32 = 20;

/// Distinct colours per thousand border pixels.
///
/// Flat cards measure under 2 even with noise on them; the photograph measures
/// 14.
const MAX_BORDER_COLOUR_DENSITY: f32 = 8.0;

/// Fraction of the border band sitting on a strong local edge.
///
/// **The decisive one for texture.** Grass, fabric, foliage and carpet are
/// defined by pixel-to-pixel contrast, which is exactly what this counts: the
/// photograph measures 50% against 0% for every flat case. It is also the only
/// signal that cannot be fooled by a dark background, because it measures
/// differences rather than values.
const MAX_BORDER_EDGE_DENSITY: f32 = 0.22;

/// A key that claimed more than this, and left nothing coherent behind, ate the
/// subject.
const MAX_PLAUSIBLE_COVERAGE: f32 = 0.85;
/// Below this, the key achieved nothing worth keeping.
const MIN_USEFUL_COVERAGE: f32 = 0.05;

/// How wide a band around the perimeter counts as "the border".
const BORDER_BAND: u32 = 4;

/// Scores an image for whether its background can be keyed at all.
///
/// **This is the check that was missing.** The pipeline is a flood fill with a
/// tolerance, which is correct for a flat background and has no correct answer
/// on a photograph: too tight and it stops at the first blade of grass, too
/// loose and it walks through the subject. There is no value in between. So the
/// question to ask first is not "what tolerance" but "is this keyable at all".
pub fn assess(bitmap: &Bitmap) -> Keyability {
    // A transparency checkerboard is two colours by construction, so measuring
    // one unflattened reports a textured, disagreeing, multi-coloured border and
    // refuses the single case this file most recently learned to handle.
    //
    // Flattened here rather than only in `attempt`, so that `assess` describes
    // the image *as the pipeline will see it* wherever it is called from — the
    // contact sheet, the UI's preview, a diagnostic. An assessment that only
    // tells the truth on one call path is worse than none.
    if let Some(board) = detect_checkerboard(bitmap) {
        let mut flattened = bitmap.clone();
        flatten_checkerboard(&mut flattened, &board);
        return assess_flattened(&flattened);
    }
    assess_flattened(bitmap)
}

fn assess_flattened(bitmap: &Bitmap) -> Keyability {
    let (w, h) = (bitmap.width, bitmap.height);
    if w < 16 || h < 16 {
        // Too small to measure. Treated as keyable: this is icon-sized art, and
        // refusing to key a 16x16 icon because there is no room to sample a
        // border band would be the wrong failure.
        return Keyability {
            confident: true,
            ..Keyability::default()
        };
    }

    let corner_disagreement = corner_disagreement(bitmap);

    // The perimeter band, sampled once and reused by three of the four signals.
    let mut band: Vec<[u8; 4]> = Vec::new();
    let mut on_edge = 0usize;
    let mut seen: Vec<u32> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let edge = x < BORDER_BAND
                || y < BORDER_BAND
                || x >= w.saturating_sub(BORDER_BAND)
                || y >= h.saturating_sub(BORDER_BAND);
            if !edge {
                continue;
            }
            let pixel = bitmap.pixel(x, y);
            // Transparency is not a colour.
            //
            // A PNG with a transparent margin carries `[0, 0, 0, 0]` around its
            // edge, and measuring that as black makes a perfectly ordinary logo
            // on a white card look like a high-contrast, many-coloured,
            // disagreeing border — which is to say, like a photograph. It was
            // refused for having done the thing this app asks people to do.
            if pixel[3] < 16 {
                continue;
            }
            band.push(pixel);
            if local_contrast(bitmap, x, y) > SMOOTH_ENOUGH {
                on_edge += 1;
            }
            // 5 bits per channel: fine enough to tell two shades of grass apart,
            // coarse enough that JPEG noise on a flat card does not read as
            // three hundred colours.
            let key = ((pixel[0] as u32 >> 3) << 10)
                | ((pixel[1] as u32 >> 3) << 5)
                | (pixel[2] as u32 >> 3);
            if !seen.contains(&key) {
                seen.push(key);
            }
        }
    }

    if band.is_empty() {
        return Keyability {
            confident: true,
            ..Keyability::default()
        };
    }

    let median = median_colour(&band);
    let mut deviations: Vec<i32> = band.iter().map(|p| distance(*p, median)).collect();
    deviations.sort_unstable();
    let border_variance = deviations[deviations.len() / 2];

    let border_colour_density = seen.len() as f32 * 1000.0 / band.len() as f32;
    let border_edge_density = on_edge as f32 / band.len() as f32;

    // **Two signals, not one.**
    //
    // Any single one of these trips on ordinary imports. A JPEG-compressed card
    // carries dozens of distinct colours; a photographed logo lit from one side
    // disagrees corner to corner; a subtly dithered background reads as noisy.
    // Refusing on one was refusing real work — measured against actual files on
    // a real machine rather than against generated cases, which are far cleaner
    // than anything a person imports.
    //
    // A background that genuinely cannot be keyed fails several at once: the
    // football photograph trips three of the four, and every wallpaper measured
    // trips three. One is evidence; two is a pattern.
    let tripped = usize::from(corner_disagreement > MAX_CORNER_DISAGREEMENT)
        + usize::from(border_variance > MAX_BORDER_VARIANCE)
        + usize::from(border_colour_density > MAX_BORDER_COLOUR_DENSITY)
        + usize::from(border_edge_density > MAX_BORDER_EDGE_DENSITY);
    let confident = tripped < 2;

    Keyability {
        corner_disagreement,
        border_variance,
        border_colour_density,
        border_edge_density,
        confident,
    }
}

/// The largest colour distance between any two corner patches.
fn corner_disagreement(bitmap: &Bitmap) -> i32 {
    const PATCH: u32 = 8;
    let (w, h) = (bitmap.width, bitmap.height);
    let corner = |x0: u32, y0: u32| -> [u8; 4] {
        let mut samples = Vec::with_capacity((PATCH * PATCH) as usize);
        for y in y0..(y0 + PATCH).min(h) {
            for x in x0..(x0 + PATCH).min(w) {
                let pixel = bitmap.pixel(x, y);
                // Same rule as the band: transparency is absence, not black.
                if pixel[3] >= 16 {
                    samples.push(pixel);
                }
            }
        }
        median_colour(&samples)
    };

    let corners = [
        corner(0, 0),
        corner(w.saturating_sub(PATCH), 0),
        corner(0, h.saturating_sub(PATCH)),
        corner(w.saturating_sub(PATCH), h.saturating_sub(PATCH)),
    ];

    let mut worst = 0;
    for i in 0..corners.len() {
        for j in (i + 1)..corners.len() {
            worst = worst.max(distance(corners[i], corners[j]));
        }
    }
    worst
}

/// Per-channel median of a set of pixels.
fn median_colour(samples: &[[u8; 4]]) -> [u8; 4] {
    if samples.is_empty() {
        return [0, 0, 0, 255];
    }
    let channel = |index: usize| -> u8 {
        let mut values: Vec<u8> = samples.iter().map(|p| p[index]).collect();
        values.sort_unstable();
        values[values.len() / 2]
    };
    [channel(0), channel(1), channel(2), 255]
}

/// What a cut left behind, measured.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Survivors {
    /// Opaque pixels remaining.
    pub total: usize,
    /// Connected regions of any size.
    pub pieces: usize,
    /// Regions big enough to be a deliberate shape rather than a speck.
    pub substantial: usize,
    /// The biggest region, as a fraction of everything that survived.
    pub largest: f32,
    /// How much of the survivors' own bounding box they fill.
    ///
    /// The signal that separates a wordmark from a shredded face. Both leave
    /// many similar pieces and neither has a dominant one; the difference is
    /// that a wordmark's pieces *are* the artwork and fill their box, while a
    /// shredded subject is a scatter of islands around a hole where the subject
    /// used to be.
    pub density: f32,
}

/// A region smaller than this is a speck whatever it belongs to.
const SUBSTANTIAL_PIECE: usize = 16;

/// Measures what survived a cut.
pub fn survivors(bitmap: &Bitmap) -> Survivors {
    let (w, h) = (bitmap.width, bitmap.height);
    let opaque = |x: u32, y: u32| bitmap.alpha(x, y) > 128;

    let mut seen = vec![false; (w * h) as usize];
    let mut total = 0usize;
    let mut pieces = 0usize;
    let mut substantial = 0usize;
    let mut largest = 0usize;
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);

    for y in 0..h {
        for x in 0..w {
            if !opaque(x, y) {
                continue;
            }
            total += 1;
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);

            if seen[(y * w + x) as usize] {
                continue;
            }
            // Iterative, not recursive: a full-frame region on a 1024px image
            // is a million-deep recursion, and a stack overflow is not a
            // diagnostic.
            let mut size = 0usize;
            let mut stack = vec![(x, y)];
            seen[(y * w + x) as usize] = true;
            while let Some((cx, cy)) = stack.pop() {
                size += 1;
                for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (cx as i32 + dx, cy as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let (nx, ny) = (nx as u32, ny as u32);
                    let i = (ny * w + nx) as usize;
                    if seen[i] || !opaque(nx, ny) {
                        continue;
                    }
                    seen[i] = true;
                    stack.push((nx, ny));
                }
            }
            pieces += 1;
            largest = largest.max(size);
            if size >= SUBSTANTIAL_PIECE {
                substantial += 1;
            }
        }
    }

    if total == 0 {
        return Survivors::default();
    }
    let box_area = ((x1 - x0 + 1) as usize) * ((y1 - y0 + 1) as usize);
    Survivors {
        total,
        pieces,
        substantial,
        largest: largest as f32 / total as f32,
        density: total as f32 / box_area.max(1) as f32,
    }
}

/// A region holding this much of the survivors is the subject, whatever is left
/// around it.
const LARGEST_IS_A_SUBJECT: f32 = 0.5;
/// More separate pieces than this is not a drawing.
const MOST_PIECES: usize = 64;
/// Below this fill of their own bounding box, the survivors are a scatter
/// around a hole rather than a shape.
const LEAST_DENSITY: f32 = 0.20;

/// Whether what survived a cut is a subject rather than debris.
///
/// **Judged from the output, not predicted from the input.** Both failures this
/// has had — a football on grass, and a portrait whose face was flooded away —
/// came back as dozens of disconnected islands. That is visible in the result
/// whatever caused it, which makes it the one check that does not depend on
/// guessing right about the image beforehand.
///
/// Two ways to pass, because artwork legitimately takes two shapes:
///
///  1. **One dominant region** — a logo, a character, a photograph's subject.
///  2. **A few real pieces filling their own box** — a wordmark is one region
///     per letter and a sigil one per stroke; neither has a dominant component
///     and both are correct.
///
/// A shredded portrait matches neither: many pieces, none dominant, scattered
/// around a hole where the face was, so the box they occupy is mostly empty.
/// `density` is what separates that from a wordmark, and it is why this is not
/// simply a component count — the two have similar counts and completely
/// different fill.
pub fn survivor_is_coherent(bitmap: &Bitmap) -> bool {
    let s = survivors(bitmap);
    if s.total == 0 {
        return false;
    }
    if s.largest >= LARGEST_IS_A_SUBJECT {
        return true;
    }
    (1..=MOST_PIECES).contains(&s.substantial) && s.density >= LEAST_DENSITY
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
///
/// Refuses rather than guesses. See [`attempt`].
pub fn remove_background(bitmap: &mut Bitmap) -> MatteReport {
    attempt(bitmap, false, None)
}

/// Keys at a tolerance the user chose, rather than one derived from the border.
///
/// The editor's slider. A hand-set tolerance is an explicit instruction and
/// carries the same weight as `force`: the keyability refusal is skipped,
/// because the person moving the slider is looking at the result while they
/// move it. The sanity check on what comes back still applies.
pub fn remove_background_at(bitmap: &mut Bitmap, tolerance: i32) -> MatteReport {
    attempt(bitmap, false, Some(tolerance))
}

/// The range the slider spans, and what "automatic" would have picked.
pub fn tolerance_range() -> (i32, i32) {
    (MIN_TOLERANCE, SOFT_TOLERANCE)
}

/// What the automatic path would choose for this image, so the slider can start
/// where the app would have.
pub fn suggested_tolerance(bitmap: &Bitmap) -> i32 {
    match sample_border(bitmap) {
        Some(background) => tolerance_for(border_spread(bitmap, background)),
        None => TOLERANCE,
    }
}

/// The same, but ignoring the "this already has transparency" shortcut **and**
/// the this-looks-like-a-photograph refusal.
///
/// An image can carry an alpha channel and still have a background — a PNG
/// exported with a white card behind it, a GIF whose transparency only covers
/// the corners. The automatic path leaves those alone on purpose, because
/// re-cutting art that somebody already cut is how you lose a soft edge. When
/// the user asks for it explicitly, that caution is the wrong default.
///
/// What `force` does **not** override is the sanity check on the result. "Try
/// anyway" is a reasonable thing for a user to mean; "hand me back an
/// unrecognisable blob" is not, and no button in this app means that.
pub fn remove_background_forced(bitmap: &mut Bitmap) -> MatteReport {
    attempt(bitmap, true, None)
}

/// Decides whether to key, keys on a copy, checks the result, and only then
/// commits it.
///
/// The order is the fix. Every step of it exists because the previous shape of
/// this function did the opposite:
///
///  1. **Flatten a transparency checkerboard first**, so the measurements below
///     see one background rather than two.
///  2. **Assess before attempting.** A photograph has no correct tolerance, so
///     it gets none tried on it.
///  3. **Work on a copy.** The caller's bitmap is not touched until there is a
///     result worth having.
///  4. **Check what came back.** A key that claimed almost everything and left
///     scattered debris is a destroyed image, whatever its coverage figure says.
///  5. **Commit, or revert and say why.**
fn attempt(bitmap: &mut Bitmap, force: bool, tolerance: Option<i32>) -> MatteReport {
    let (w, h) = (bitmap.width, bitmap.height);
    if w < 3 || h < 3 {
        return MatteReport::untouched(Keyability {
            confident: true,
            ..Keyability::default()
        });
    }

    // 1. Before anything measures anything: if this is a screenshot of an
    // editor's transparency grid, make it one colour. Every signal below assumes
    // a single background and produces a confidently wrong answer on two.
    //
    // Done on the caller's bitmap because it is not destructive — it replaces
    // two greys that are both background with one of them — and because the
    // assessment has to run on the flattened version to be meaningful.
    if let Some(board) = detect_checkerboard(bitmap) {
        log::debug!(
            "matte: transparency checkerboard detected, {}px cells; flattening before the cut",
            board.cell
        );
        flatten_checkerboard(bitmap, &board);
    }

    let keyability = assess(bitmap);

    if !force && already_cut_out(bitmap) {
        return MatteReport {
            removed: 0.0,
            already_had_alpha: true,
            refused: None,
            keyability,
        };
    }

    // 2. The refusal that is the whole point of this pass.
    //
    // Not applied under `force`: a user who has looked at the preview and asked
    // for it anyway has overruled the guess, and they are allowed to. The
    // result is still checked at step 4.
    if !force && tolerance.is_none() && !keyability.confident {
        log::info!(
            "matte: refusing to key — corners disagree by {}, border varies by {}, \
             {:.1} border colours per 1000, {:.0}% of the border on an edge",
            keyability.corner_disagreement,
            keyability.border_variance,
            keyability.border_colour_density,
            keyability.border_edge_density * 100.0
        );
        return MatteReport::refusing(Refusal::LooksLikeAPhotograph, keyability);
    }

    // 3. On a copy. The original is what the user gets back if any of this goes
    // wrong, and it cannot be reconstructed from a bad matte.
    let mut candidate = bitmap.clone();
    let report = cut(&mut candidate, force, tolerance);

    // 4. What came back.
    if report.removed < MIN_USEFUL_COVERAGE {
        return MatteReport::refusing(Refusal::BarelyMoved, keyability);
    }
    if report.removed > MAX_PLAUSIBLE_COVERAGE && !survivor_is_coherent(&candidate) {
        log::warn!(
            "matte: a key claiming {:.0}% left no coherent subject; reverting",
            report.removed * 100.0
        );
        return MatteReport::refusing(Refusal::WouldHaveEatenTheSubject, keyability);
    }

    // 5. Only now.
    *bitmap = candidate;
    MatteReport {
        removed: report.removed,
        already_had_alpha: false,
        refused: None,
        keyability,
    }
}

/// The alternating grey grid an image editor draws behind transparency.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Checkerboard {
    pub light: [u8; 3],
    pub dark: [u8; 3],
    /// Side of one square, in pixels.
    pub cell: u32,
}

/// How close a pixel must be to one of the two greys to count as the board.
///
/// Tight. The board is drawn by software, not photographed, so its two colours
/// are exact everywhere except where the subject's antialiased edge sits on top
/// of them. Anything looser starts taking grey parts of the subject.
const BOARD_TOLERANCE: i32 = 10;

/// The largest fraction of the border that may be something other than the two
/// board colours before this stops being a checkerboard.
const BOARD_COVERAGE: f32 = 0.85;

/// Finds the transparency checkerboard, if the image is a screenshot of one.
///
/// **This is the case that defeats everything else in this file.** Somebody
/// opens a transparent PNG in an editor, screenshots it, and imports the
/// screenshot. What arrives is fully opaque and its background is not one colour
/// but two, alternating on a grid.
///
/// Every step downstream then does the wrong thing, and does it confidently:
/// `sample_border` takes the median of two greys and returns a colour that is in
/// neither square, `border_spread` measures the distance between the two squares
/// and calls the image a noisy photographic backdrop, and `tolerance_for` hands
/// out enough slack to swallow anything grey in the subject. The result is a cut
/// that removes half the artwork and leaves a chequered fringe.
///
/// Detected from the **border** for the same reason `already_cut_out` is: the
/// subject sits in the middle, and a board that has been drawn behind it is
/// unobstructed at the edges.
///
/// Three things have to hold, and each rules out a different false positive:
///
/// 1. **Two colours cover the border.** A photograph of anything does not.
/// 2. **Both are near-neutral, and one is lighter than the other.** Editors draw
///    the board in greys — white and light grey, or two mid greys. A red and
///    blue chequered *shirt* is not a transparency board.
/// 3. **They alternate on a regular grid.** This is what separates a board from
///    a two-tone logo, and it is why run lengths are measured rather than just
///    counted.
pub fn detect_checkerboard(bitmap: &Bitmap) -> Option<Checkerboard> {
    let (w, h) = (bitmap.width, bitmap.height);
    if w < 16 || h < 16 {
        return None;
    }

    // The two most common border colours.
    let mut counts: Vec<([u8; 3], usize)> = Vec::new();
    let mut total = 0usize;
    let tally = |pixel: [u8; 4], counts: &mut Vec<([u8; 3], usize)>, total: &mut usize| {
        if pixel[3] < 250 {
            // Already transparent: this is not a screenshot of a board, it is
            // the real thing.
            return;
        }
        *total += 1;
        let rgb = [pixel[0], pixel[1], pixel[2]];
        match counts.iter_mut().find(|(c, _)| grey_distance(*c, rgb) <= BOARD_TOLERANCE) {
            Some((_, n)) => *n += 1,
            None => counts.push((rgb, 1)),
        }
    };
    for x in 0..w {
        tally(bitmap.pixel(x, 0), &mut counts, &mut total);
        tally(bitmap.pixel(x, h - 1), &mut counts, &mut total);
    }
    for y in 0..h {
        tally(bitmap.pixel(0, y), &mut counts, &mut total);
        tally(bitmap.pixel(w - 1, y), &mut counts, &mut total);
    }
    if total == 0 {
        return None;
    }

    // Most common first.
    counts.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    if counts.len() < 2 {
        return None; // one colour: an ordinary flat background, handled elsewhere
    }
    let (first, first_n) = counts[0];
    let (second, second_n) = counts[1];

    if ((first_n + second_n) as f32 / total as f32) < BOARD_COVERAGE {
        return None;
    }
    // Both squares have to be visible. A board whose second colour appears four
    // times is not a board.
    if second_n * 6 < first_n {
        return None;
    }
    if !near_neutral(first) || !near_neutral(second) {
        return None;
    }
    if grey_distance(first, second) < 12 {
        return None; // two shades too close to be a board anyone drew on purpose
    }

    let (light, dark) = if luma(first) >= luma(second) {
        (first, second)
    } else {
        (second, first)
    };

    let cell = board_cell(bitmap, light, dark)?;
    Some(Checkerboard { light, dark, cell })
}

/// The square size, from run lengths along the top and left borders.
///
/// Measured rather than assumed. Editors use 8, 10, 16 and 32 depending on the
/// tool and the zoom the screenshot was taken at, and a screenshot scaled on the
/// way in can produce anything. What matters is that the runs are *consistent* —
/// that is the property a two-tone logo does not have.
fn board_cell(bitmap: &Bitmap, light: [u8; 3], dark: [u8; 3]) -> Option<u32> {
    let mut runs: Vec<u32> = Vec::new();

    let mut walk = |length: u32, at: &dyn Fn(u32) -> [u8; 4]| {
        let mut run = 0u32;
        let mut current: Option<bool> = None;
        for i in 0..length {
            let pixel = at(i);
            let rgb = [pixel[0], pixel[1], pixel[2]];
            let is_light = grey_distance(rgb, light) <= BOARD_TOLERANCE;
            let is_dark = grey_distance(rgb, dark) <= BOARD_TOLERANCE;
            if !is_light && !is_dark {
                current = None;
                run = 0;
                continue;
            }
            match current {
                Some(was) if was == is_light => run += 1,
                _ => {
                    // Interior runs only: the first and last are cut off by the
                    // edge of the image and would drag the answer down.
                    if run > 0 && i > run + 1 {
                        runs.push(run);
                    }
                    current = Some(is_light);
                    run = 1;
                }
            }
        }
    };

    walk(bitmap.width, &|x| bitmap.pixel(x, 0));
    walk(bitmap.height, &|y| bitmap.pixel(0, y));

    if runs.len() < 3 {
        return None;
    }
    runs.sort_unstable();
    let median = runs[runs.len() / 2];
    if !(2..=64).contains(&median) {
        return None;
    }
    // Consistency: most runs must be the median length. A logo made of two
    // colours produces runs of every length; a drawn grid produces one.
    let consistent = runs.iter().filter(|r| r.abs_diff(median) <= 1).count();
    if consistent * 4 < runs.len() * 3 {
        return None;
    }
    Some(median)
}

/// Replaces both board colours with one, so the rest of the cut sees a flat
/// background and behaves the way it already knows how to.
///
/// Deliberately not "clear both colours to transparent". That would key out
/// every grey pixel in the subject that happens to match a square, wherever it
/// is — the exact failure the flood fill exists to avoid. Flattening keeps the
/// connectivity rule: a grey patch in the middle of the subject survives,
/// because it is not joined to the edge.
fn flatten_checkerboard(bitmap: &mut Bitmap, board: &Checkerboard) {
    for y in 0..bitmap.height {
        for x in 0..bitmap.width {
            let pixel = bitmap.pixel(x, y);
            if pixel[3] < 250 {
                continue;
            }
            let rgb = [pixel[0], pixel[1], pixel[2]];
            if grey_distance(rgb, board.light) <= BOARD_TOLERANCE
                || grey_distance(rgb, board.dark) <= BOARD_TOLERANCE
            {
                bitmap.set_pixel(x, y, [board.light[0], board.light[1], board.light[2], 255]);
            }
        }
    }
}

/// Largest per-channel difference between two RGB triples.
fn grey_distance(a: [u8; 3], b: [u8; 3]) -> i32 {
    (0..3)
        .map(|i| (a[i] as i32 - b[i] as i32).abs())
        .max()
        .unwrap_or(0)
}

/// Whether a colour is close enough to grey to be part of a drawn board.
fn near_neutral(rgb: [u8; 3]) -> bool {
    let max = rgb.iter().copied().max().unwrap_or(0) as i32;
    let min = rgb.iter().copied().min().unwrap_or(0) as i32;
    max - min <= 12
}

fn luma(rgb: [u8; 3]) -> i32 {
    (rgb[0] as i32 * 30 + rgb[1] as i32 * 59 + rgb[2] as i32 * 11) / 100
}

/// The key itself.
///
/// Assumes its caller has already decided this image is worth keying and is
/// working on a copy — see [`attempt`], which is the only caller.
fn cut(bitmap: &mut Bitmap, _force: bool, override_tolerance: Option<i32>) -> MatteReport {
    let (w, h) = (bitmap.width, bitmap.height);
    let empty = MatteReport::untouched(Keyability::default());
    if w < 3 || h < 3 {
        return empty;
    }

    let Some(background) = sample_border(bitmap) else {
        return empty;
    };

    // How much slack this particular image gets, from how uniform its border is.
    // A flat card is keyed almost exactly; a noisy photographic backdrop is given
    // room. See `border_spread`.
    // The user's slider wins when there is one. The derived value is a good
    // default and is never better than a person looking at the result: the
    // whole reason the editor exists is that some images need a number nothing
    // can infer.
    let tolerance = override_tolerance
        .map(|t| t.clamp(MIN_TOLERANCE, SOFT_TOLERANCE))
        .unwrap_or_else(|| tolerance_for(border_spread(bitmap, background)));
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

    // Everything bigger than one pixel that is still background.
    //
    // `despeckle` only ever clears a pixel whose four neighbours are all
    // background, which means it removes specks of exactly one pixel and
    // nothing else. Two adjacent survivors protect each other and stay
    // forever — and compression debris does not arrive one pixel at a time. It
    // arrives as flecks along a hard edge, as the last few pixels of a shadow,
    // as a fringe of a colour that fell just outside the tolerance. Those are
    // the crumbs and spots left on an otherwise clean cut.
    remove_crumbs(bitmap, &mut cleared, background, soft);

    let removed = cleared.iter().filter(|c| **c).count();

    // Refusing to gut the image is part of the job. If almost everything
    // matched the border, the "subject" was the background and clearing it
    // would leave nothing.
    // No coverage guard here any more, and its removal is a fix rather than a
    // relaxation.
    //
    // This used to bail out at 97%, on the reasoning that clearing almost
    // everything means the "subject" was the background. The reasoning is right
    // and the measure is wrong: **a small logo on a large canvas legitimately
    // keys away 99% of the image**, and every one of those was silently
    // returned with no background removed at all — the most common shape of
    // cursor source art there is.
    //
    // Coverage cannot tell a sparse logo from a destroyed photograph. What can
    // is whether anything coherent survived, and that is checked by `attempt`,
    // which owns the decision to keep or revert this result.
    let fraction = removed as f32 / (w * h) as f32;

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

    // The background's colour taken back out of the fringe it contaminated.
    despill(bitmap, &cleared, background);

    sweep_dust(bitmap);

    MatteReport {
        removed: fraction,
        already_had_alpha: false,
        refused: None,
        keyability: Keyability::default(),
    }
}

/// Removes background colour left in the pixels along the subject's edge.
///
/// **This is what the halo on a JPEG was.** Compression puts a bright overshoot
/// just outside a hard edge — ringing — and those pixels sit too far from the
/// background to be keyed and too close to the edge to be subject. Everything
/// upstream is deliberately edge-*preserving*, so they survive, and what the
/// user sees is a pale outline tracing their artwork.
///
/// The correction is local and conservative. For each kept pixel that touches
/// the flood, look at the kept pixels a little further in — ones that do *not*
/// touch it, and are therefore uncontaminated — and take their median as what
/// this pixel should look like. If the pixel leans toward the background
/// relative to that median, replace its colour with the median and keep its
/// alpha.
///
/// Colour only, never alpha. The alpha is the shape, and the shape was decided
/// by the flood; a despill that moved it would be re-keying under another name.
///
/// Conservative in the one direction that matters: a pixel with too few clean
/// neighbours to judge is left exactly as it is. On a one-pixel-wide feature —
/// a whisker, the point of an arrow — there is no interior to sample, and
/// guessing there would eat the feature.
fn despill(bitmap: &mut Bitmap, cleared: &[bool], background: [u8; 4]) {
    let (w, h) = (bitmap.width, bitmap.height);
    let index = |x: u32, y: u32| (y * w + x) as usize;

    /// How far from the flood a pixel can be and still be contaminated.
    ///
    /// Two, not one. A compression fringe is not one pixel wide — ringing puts
    /// an overshoot two or three pixels deep around a hard edge — and a
    /// despill that only reached pixels touching the flood left the inner half
    /// of the ring behind, which on a square subject is a visible bright line
    /// one pixel in from the edge.
    const FRINGE: i32 = 2;
    /// How far in to look for something clean to compare against. Must be
    /// beyond the fringe, or the samples are contaminated too.
    const CLEAN: i32 = 4;

    let within = |x: u32, y: u32, radius: i32, of: &dyn Fn(usize) -> bool| {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                if of(index(nx as u32, ny as u32)) {
                    return true;
                }
            }
        }
        false
    };

    let touches_flood = |x: u32, y: u32| within(x, y, FRINGE, &|i| cleared[i]);

    // Collected before anything is written, so a corrected pixel is never used
    // as a clean sample for the next one along.
    let mut corrections: Vec<(u32, u32, [u8; 4])> = Vec::new();

    for y in 0..h {
        for x in 0..w {
            if cleared[index(x, y)] || !touches_flood(x, y) {
                continue;
            }
            let pixel = bitmap.pixel(x, y);
            if pixel[3] == 0 {
                continue;
            }

            // Clean neighbours: kept, opaque, and far enough in that they are
            // not part of the fringe themselves.
            let mut clean: Vec<[u8; 4]> = Vec::new();
            for dy in -CLEAN..=CLEAN {
                for dx in -CLEAN..=CLEAN {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let (nx, ny) = (nx as u32, ny as u32);
                    if cleared[index(nx, ny)] || touches_flood(nx, ny) {
                        continue;
                    }
                    let sample = bitmap.pixel(nx, ny);
                    if sample[3] > 200 {
                        clean.push(sample);
                    }
                }
            }
            if clean.len() < 4 {
                continue; // nothing trustworthy to compare against
            }

            let interior = median_colour(&clean);
            let pixel_to_background = distance(pixel, background);
            let interior_to_background = distance(interior, background);

            // Contaminated means "closer to the background than the artwork
            // behind it is, by enough to see". The margin keeps ordinary
            // antialiasing — which is *supposed* to sit between the two — from
            // being flattened into the subject's colour.
            if interior_to_background - pixel_to_background > SPILL_MARGIN {
                corrections.push((x, y, [interior[0], interior[1], interior[2], pixel[3]]));
            }
        }
    }

    let count = corrections.len();
    for (x, y, colour) in corrections {
        bitmap.set_pixel(x, y, colour);
    }
    if count > 0 {
        log::debug!("{count} fringe pixels despilled");
    }
}

/// How much closer to the background a fringe pixel must be than the artwork
/// behind it before it counts as contaminated.
///
/// Generous. A tight margin flattens every genuinely soft edge into a hard one,
/// which is a worse artefact than the halo it was fixing and shows up on every
/// image rather than on compressed ones.
const SPILL_MARGIN: i32 = 40;

/// The alpha below which a pixel standing entirely on its own is dust.
///
/// A tenth of full opacity. Anything fainter than this contributes almost
/// nothing where it belongs, and where it does not belong it is a grey fleck on
/// a checkerboard.
const DUST_ALPHA: u8 = 26;

/// Clears faint pixels that ended up with no opaque neighbour at all.
///
/// The grading pass keeps boundary pixels and admits they are partial, which is
/// right where the boundary is a real edge — a hair, an antialiased outline, the
/// soft side of a shadow. Where the "boundary" was one stray pixel of
/// compression noise, it produces a pixel at eight percent alpha with nothing
/// around it: invisible against a white page, obvious against a dark one, and
/// exactly the speckling people describe as spots.
///
/// The test is deliberately absolute. Not "faint and small" or "faint and near
/// the background", but faint **and completely alone** — all eight neighbours
/// fully transparent. A pixel with any opaque neighbour is part of something and
/// is left alone, whatever its alpha.
fn sweep_dust(bitmap: &mut Bitmap) {
    let (w, h) = (bitmap.width, bitmap.height);
    if w < 3 || h < 3 {
        return;
    }

    let mut dust = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let alpha = bitmap.alpha(x, y);
            if alpha == 0 || alpha > DUST_ALPHA {
                continue;
            }
            let alone = (-1i32..=1).all(|dy| {
                (-1i32..=1).all(|dx| {
                    if dx == 0 && dy == 0 {
                        return true;
                    }
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    nx < 0
                        || ny < 0
                        || nx >= w as i32
                        || ny >= h as i32
                        || bitmap.alpha(nx as u32, ny as u32) == 0
                })
            });
            if alone {
                dust.push((x, y));
            }
        }
    }

    for (x, y) in dust {
        bitmap.set_pixel(x, y, [0, 0, 0, 0]);
    }
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

/// The largest island of leftover background that still counts as a crumb,
/// as a fraction of the image: one part in two thousand.
///
/// On a 1024×1024 working image that is about five hundred pixels — a blob of
/// roughly 22×22. That sounds generous until you consider what has to be true
/// for a region to reach this code: it survived the flood, it survived the
/// enclosed-background pass, it is not connected to anything else that
/// survived, **and** its colour is still within the soft band of the sampled
/// background. A real detail that happens to be detached — the dot of an i, a
/// spark, a highlight — fails the colour test long before the size one.
const CRUMB_FRACTION: usize = 2_000;

/// The smallest crumb limit, for images too small for the fraction to mean
/// anything. Sixteen pixels is a 4×4 speck.
const MIN_CRUMB: usize = 16;

/// Clears small isolated islands that are still the background colour.
///
/// Runs on connected components rather than on neighbourhoods, because that is
/// the shape of the problem: a crumb is *disconnected* background, and its size
/// is a property of the whole island, not of any pixel in it. Eight-connected,
/// so a diagonal chain of debris counts as one island rather than as a handful
/// of survivors propping each other up.
///
/// Two conditions, and both are needed:
///
/// - **Small**, relative to the image. Anything large is part of the picture.
/// - **Still the background colour**, on average across the island. This is what
///   keeps the pass from eating detached artwork. A crumb is background that got
///   away; something that is not the background's colour is not a crumb, however
///   small and however isolated it is.
///
/// The largest island is never touched whatever it measures, because on a small
/// image with a small subject it can satisfy both conditions — and clearing the
/// subject is a worse failure than leaving a speck.
fn remove_crumbs(bitmap: &Bitmap, cleared: &mut [bool], background: [u8; 4], soft: i32) {
    let (w, h) = (bitmap.width, bitmap.height);
    let total = (w as usize) * (h as usize);
    let limit = (total / CRUMB_FRACTION).max(MIN_CRUMB);

    let mut seen = vec![false; total];
    let mut islands: Vec<(Vec<usize>, i64)> = Vec::new();

    for sy in 0..h {
        for sx in 0..w {
            let start = (sy * w + sx) as usize;
            if cleared[start] || seen[start] {
                continue;
            }

            let mut island = Vec::new();
            let mut drift = 0i64;
            let mut stack = vec![(sx, sy)];
            seen[start] = true;
            while let Some((x, y)) = stack.pop() {
                island.push((y * w + x) as usize);
                drift += distance(bitmap.pixel(x, y), background) as i64;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                            continue;
                        }
                        let i = (ny as u32 * w + nx as u32) as usize;
                        if seen[i] || cleared[i] {
                            continue;
                        }
                        seen[i] = true;
                        stack.push((nx as u32, ny as u32));
                    }
                }
            }

            islands.push((island, drift));
        }
    }

    let largest = islands.iter().map(|(pixels, _)| pixels.len()).max().unwrap_or(0);
    let mut swept = 0usize;
    for (pixels, drift) in islands {
        if pixels.len() >= largest || pixels.len() > limit {
            continue;
        }
        let mean = drift / pixels.len() as i64;
        if mean > soft as i64 {
            continue;
        }
        swept += pixels.len();
        for i in pixels {
            cleared[i] = true;
        }
    }
    if swept > 0 {
        log::debug!("{swept} pixels of leftover background swept up as crumbs");
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

    // Transparent pixels are skipped, and this is not a refinement.
    //
    // A transparent pixel's colour channels hold whatever the encoder left
    // there, which for almost every PNG is `[0, 0, 0]`. Including them took the
    // median of a transparent margin and returned **black** — so the background
    // to key against became black, and every dark pixel of the subject
    // connected to the edge was removed as background. On dark artwork that is
    // most of it.
    //
    // Reached whenever a PNG that already carries transparency is keyed anyway,
    // which is exactly what the "Remove the background" toggle is for.
    let take = |pixel: [u8; 4], samples: &mut Vec<[u8; 4]>| {
        if pixel[3] >= 16 {
            samples.push(pixel);
        }
    };
    for x in 0..w {
        take(bitmap.pixel(x, 0), &mut samples);
        take(bitmap.pixel(x, h - 1), &mut samples);
    }
    for y in 0..h {
        take(bitmap.pixel(0, y), &mut samples);
        take(bitmap.pixel(w - 1, y), &mut samples);
    }

    // A border with nothing opaque in it has no background colour, because it
    // *is* background already. `None` stops the key rather than inventing one.
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
    // Same rule as `sample_border`: a transparent pixel has no colour to
    // deviate. Counting them makes a clean transparent margin look like an
    // extremely noisy background, which buys the key a much looser tolerance
    // than the image deserves — and a loose tolerance on light artwork eats it.
    let take = |pixel: [u8; 4], deviations: &mut Vec<i32>| {
        if pixel[3] >= 16 {
            deviations.push(distance(pixel, background));
        }
    };
    for x in 0..w {
        take(bitmap.pixel(x, 0), &mut deviations);
        take(bitmap.pixel(x, h - 1), &mut deviations);
    }
    for y in 0..h {
        take(bitmap.pixel(0, y), &mut deviations);
        take(bitmap.pixel(w - 1, y), &mut deviations);
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
pub(crate) fn unblend(pixel: [u8; 4], background: [u8; 4], alpha: u8) -> [u8; 4] {
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

    /// A photograph: textured background, vignette, and a subject in the middle.
    ///
    /// Every property that makes the real thing unkeyable, in miniature —
    /// per-pixel texture so no two neighbours match, and a radial falloff so the
    /// corners do not agree with each other.
    fn photograph(size: u32) -> Bitmap {
        let mut b = Bitmap::new(size, size);
        let (cx, cy) = (size as f32 / 2.0, size as f32 / 2.0);
        let hash = |a: u32, c: u32| {
            let mut n = a.wrapping_mul(374_761_393).wrapping_add(c.wrapping_mul(668_265_263));
            n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
            (n ^ (n >> 16)) % 256
        };
        for y in 0..size {
            for x in 0..size {
                let grain = hash(x, y) as i32;
                let tuft = hash(x / 7, y / 5) as i32;
                let dx = (x as f32 - cx) / cx;
                let dy = (y as f32 - cy) / cy;
                let light = (1.0 - 0.42 * (dx * dx + dy * dy).sqrt().min(1.4)).max(0.45);
                let base = [
                    30 + grain / 6 + tuft / 8,
                    70 + grain / 3 + tuft / 4,
                    24 + grain / 8 + tuft / 10,
                ];
                let p = base.map(|c| ((c as f32) * light).clamp(0.0, 255.0) as u8);
                b.set_pixel(x, y, [p[0], p[1], p[2], 255]);
            }
        }
        // The subject.
        for y in (size * 2 / 5)..(size * 3 / 5) {
            for x in (size * 3 / 10)..(size * 7 / 10) {
                b.set_pixel(x, y, [140, 70, 40, 255]);
            }
        }
        b
    }

    /// **A PNG's transparent margin is not black.**
    ///
    /// `sample_border` took the median of the border pixels including fully
    /// transparent ones. A transparent pixel's colour channels are whatever the
    /// encoder happened to leave there, which for almost every PNG is
    /// `[0, 0, 0]` — so the background was sampled as **black**, and every dark
    /// pixel of the subject connected to the edge was keyed away with it.
    ///
    /// Reached whenever a PNG that already carries transparency is keyed
    /// anyway: the "Remove the background" toggle, which exists precisely for
    /// art that has an alpha channel and a card behind it.
    #[test]
    fn a_transparent_margin_is_not_sampled_as_black() {
        let mut b = Bitmap::new(64, 64);
        // A dark subject filling most of the frame, on transparency.
        for y in 8..56 {
            for x in 8..56 {
                b.set_pixel(x, y, [18, 18, 22, 255]);
            }
        }

        let background = sample_border(&b);
        assert!(
            background.is_none()
                || distance(background.unwrap(), [0, 0, 0, 255]) > 40,
            "a transparent margin was sampled as {background:?}, which is black — every \
             dark pixel of the subject would be keyed away as background"
        );

        // And the whole path: forcing a key must not eat the subject.
        let report = remove_background_forced(&mut b);
        assert!(
            b.alpha(32, 32) > 200,
            "the middle of the subject was removed (removed {:.0}%, refused {:?})",
            report.removed * 100.0,
            report.refused
        );
    }

    /// **The regression for the reported bug.**
    ///
    /// A real user imported a photograph of a football on grass, in a clean VM,
    /// and got an unrecognisable dark blob back. The flood fill did not
    /// malfunction — it was handed an input no tolerance can key, and returned
    /// whatever came out.
    ///
    /// The fix is that nothing is attempted. Two assertions, and the second is
    /// the one that matters: refusing while still having modified the image
    /// would be the same bug with a message attached.
    #[test]
    fn a_photograph_is_refused_rather_than_destroyed() {
        let original = photograph(96);
        let mut subject = original.clone();

        let report = remove_background(&mut subject);

        assert_eq!(report.refused, Some(Refusal::LooksLikeAPhotograph));
        assert_eq!(report.removed, 0.0);
        assert!(!report.keyability.confident);
        assert_eq!(
            subject.pixels, original.pixels,
            "a refusal must leave the image byte-identical, not nearly so"
        );
    }

    /// The signals have to separate the two kinds of image with room to spare.
    /// A threshold that only just catches a photograph is a threshold that will
    /// let the next one through.
    #[test]
    fn a_flat_card_and_a_photograph_are_not_close() {
        let card = assess(&solid(96, 96, [250, 249, 247, 255]));
        let photo = assess(&photograph(96));

        assert!(card.confident, "a flat card must key: {card:?}");
        assert!(!photo.confident, "a photograph must not: {photo:?}");

        // Texture is the decisive signal and it is not marginal.
        assert!(
            photo.border_edge_density > card.border_edge_density + 0.2,
            "edge density should separate these clearly: {} vs {}",
            photo.border_edge_density,
            card.border_edge_density
        );
    }

    /// A user who looks at the refusal and asks for it anyway is allowed to.
    /// "Try it and let me look" is a reasonable thing to mean.
    #[test]
    fn force_overrules_the_refusal() {
        let mut subject = photograph(96);
        let report = remove_background_forced(&mut subject);
        assert_ne!(
            report.refused,
            Some(Refusal::LooksLikeAPhotograph),
            "force means the guess has been overruled"
        );
    }

    /// What `force` must **not** overrule.
    ///
    /// No button in this app means "hand me back something unrecognisable". An
    /// image with nothing but background in it keys to nothing, and nothing is
    /// not a result.
    #[test]
    fn nothing_overrules_handing_back_a_destroyed_image() {
        let original = solid(64, 64, [200, 200, 200, 255]);
        let mut subject = original.clone();

        let report = remove_background_forced(&mut subject);

        assert!(report.refused.is_some(), "an image of nothing has nothing to key");
        assert_eq!(report.removed, 0.0);
        assert_eq!(subject.pixels, original.pixels);
    }

    /// A small logo on a big canvas legitimately keys away almost everything.
    /// The coverage figure alone cannot tell that from a destroyed photograph,
    /// which is why coherence is measured rather than coverage alone.
    #[test]
    fn a_sparse_logo_is_not_mistaken_for_a_destroyed_image() {
        let mut b = solid(128, 128, [255, 255, 255, 255]);
        for y in 60..70 {
            for x in 60..70 {
                b.set_pixel(x, y, [10, 10, 200, 255]);
            }
        }
        let report = remove_background(&mut b);

        assert_eq!(report.refused, None, "a small logo is a normal import");
        assert!(report.removed > 0.9, "almost all of it is card: {}", report.removed);
        assert_eq!(b.alpha(64, 64), 255, "the logo survives");
    }

    /// **The portrait failure, as the shape it came back as.**
    ///
    /// A head-and-shoulders photo on flat white was keyed and the whole face
    /// went with the background: lit skin runs a few levels off white, so the
    /// flood crossed the boundary and, once inside, took everything until it
    /// hit genuinely dark pixels. What survived was hair, brows, eyes, nostrils
    /// and a lip outline — dozens of disconnected islands scattered around a
    /// hole where the face had been.
    ///
    /// Constructed here as that output rather than by trying to make the matte
    /// produce it, because the safety net's job is to judge a result whatever
    /// produced it. Every background-side signal passed on that image: the
    /// background genuinely was ideal, and the failure was in the relationship
    /// between subject and background, which is only visible afterwards.
    #[test]
    fn a_shredded_portrait_is_not_a_subject() {
        let mut b = Bitmap::new(200, 240);
        let ink = [30u8, 26, 28, 255];
        let mut blob = |cx: i32, cy: i32, rx: i32, ry: i32| {
            for y in (cy - ry).max(0)..(cy + ry).min(240) {
                for x in (cx - rx).max(0)..(cx + rx).min(200) {
                    let dx = (x - cx) as f32 / rx as f32;
                    let dy = (y - cy) as f32 / ry as f32;
                    if dx * dx + dy * dy <= 1.0 {
                        b.set_pixel(x as u32, y as u32, ink);
                    }
                }
            }
        };
        // Hair, as the strands a flood leaves rather than one mass.
        for i in 0..26 {
            blob(40 + (i % 13) * 10, 30 + (i / 13) * 14, 4, 7);
        }
        // Brows, eyes, nostrils, lips.
        blob(75, 105, 14, 3);
        blob(125, 105, 14, 3);
        blob(75, 120, 9, 5);
        blob(125, 120, 9, 5);
        blob(94, 150, 4, 3);
        blob(106, 150, 4, 3);
        for i in 0..7 {
            blob(78 + i * 8, 178, 5, 3);
        }

        let s = survivors(&b);
        // The report described about fifty islands; adjacent strands merge
        // here into twenty. The count is incidental — what condemns it is that
        // nothing dominates and the pieces occupy a box that is mostly empty.
        assert!(s.pieces >= 15, "this should be many islands: {s:?}");
        assert!(s.density < LEAST_DENSITY, "a scatter around a hole: {s:?}");
        assert!(s.largest < 0.5, "no island dominates: {s:?}");
        assert!(
            !survivor_is_coherent(&b),
            "a shredded face must never be returned as a cutout: {s:?}"
        );
    }

    /// And the shape that must still pass: a wordmark, which also has many
    /// pieces and no dominant one, and is correct.
    #[test]
    fn a_wordmark_survives_the_same_check() {
        let mut b = Bitmap::new(200, 60);
        // Six letter-sized blocks filling their line.
        for letter in 0..6 {
            let x0 = 10 + letter * 31;
            for y in 12..48 {
                for x in x0..x0 + 22 {
                    b.set_pixel(x, y, [20, 20, 24, 255]);
                }
            }
        }
        let s = survivors(&b);
        assert_eq!(s.substantial, 6, "{s:?}");
        assert!(s.largest < 0.5, "no letter dominates: {s:?}");
        assert!(
            survivor_is_coherent(&b),
            "a wordmark is artwork, not debris: {s:?}"
        );
    }

    /// One blob is a subject. A thousand specks is debris.
    #[test]
    fn coherence_tells_a_subject_from_confetti() {
        let mut blob = Bitmap::new(64, 64);
        for y in 20..44 {
            for x in 20..44 {
                blob.set_pixel(x, y, [200, 100, 50, 255]);
            }
        }
        assert!(survivor_is_coherent(&blob));

        let mut confetti = Bitmap::new(64, 64);
        for y in (0..64).step_by(2) {
            for x in (0..64).step_by(2) {
                confetti.set_pixel(x, y, [200, 100, 50, 255]);
            }
        }
        assert!(!survivor_is_coherent(&confetti), "scattered pixels are not a subject");

        assert!(!survivor_is_coherent(&Bitmap::new(32, 32)), "nothing survived");
    }

    /// The halo. Compression ringing puts a bright overshoot just outside a
    /// hard edge; it survives the key because it is neither background nor
    /// subject, and it reads as a pale outline traced around the artwork.
    #[test]
    fn a_bright_fringe_left_by_compression_is_despilled() {
        let mut b = solid(48, 48, [250, 250, 250, 255]);
        // A dark square with a ring of near-white overshoot around it.
        for y in 14..34 {
            for x in 14..34 {
                let edge = !(16..32).contains(&x) || !(16..32).contains(&y);
                b.set_pixel(x, y, if edge { [235, 235, 235, 255] } else { [20, 20, 20, 255] });
            }
        }

        remove_background(&mut b);

        // Anything still opaque must belong to the subject rather than to the
        // card it was photographed on.
        for y in 0..48 {
            for x in 0..48 {
                let p = b.pixel(x, y);
                if p[3] < 200 {
                    continue;
                }
                assert!(
                    p[0] < 200,
                    "a near-white pixel survived at ({x},{y}): {p:?} — that is the halo"
                );
            }
        }
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

    /// Compression debris: a few pixels of not-quite-background left in the
    /// clear area, in clumps of more than one so `despeckle` cannot touch them.
    #[test]
    fn clumps_of_leftover_background_are_swept_up() {
        let mut b = solid(64, 64, [255, 255, 255, 255]);
        for y in 20..44 {
            for x in 20..44 {
                b.set_pixel(x, y, [20, 30, 200, 255]);
            }
        }
        // Three crumbs, each too big for a single-pixel despeckle and each just
        // off the background colour, the way JPEG ringing leaves them.
        for (cx, cy) in [(6u32, 6u32), (54, 8), (10, 52)] {
            for dy in 0..2 {
                for dx in 0..3 {
                    b.set_pixel(cx + dx, cy + dy, [238, 240, 243, 255]);
                }
            }
        }

        let report = remove_background(&mut b);
        assert!(report.removed > 0.5);

        for (cx, cy) in [(6u32, 6u32), (54, 8), (10, 52)] {
            for dy in 0..2 {
                for dx in 0..3 {
                    assert_eq!(
                        b.alpha(cx + dx, cy + dy),
                        0,
                        "a crumb survived at {},{}",
                        cx + dx,
                        cy + dy
                    );
                }
            }
        }
        assert_eq!(b.alpha(32, 32), 255, "the subject is untouched");
    }

    /// The other half of the rule: something small and detached that is *not*
    /// the background's colour is artwork, and artwork stays.
    #[test]
    fn a_detached_detail_that_is_not_the_background_colour_survives() {
        let mut b = solid(64, 64, [255, 255, 255, 255]);
        for y in 24..40 {
            for x in 24..40 {
                b.set_pixel(x, y, [10, 10, 10, 255]);
            }
        }
        // The dot of an i: four pixels, nowhere near the subject, nowhere near
        // white either.
        for dy in 0..2 {
            for dx in 0..2 {
                b.set_pixel(8 + dx, 8 + dy, [220, 30, 30, 255]);
            }
        }

        remove_background(&mut b);

        for dy in 0..2 {
            for dx in 0..2 {
                assert_eq!(
                    b.alpha(8 + dx, 8 + dy),
                    255,
                    "a detached red detail must not be swept up as a crumb"
                );
            }
        }
    }

    #[test]
    fn a_single_faint_pixel_with_nothing_around_it_is_dust() {
        let mut b = Bitmap::new(16, 16);
        b.set_pixel(4, 4, [180, 180, 180, 12]);
        b.set_pixel(9, 9, [180, 180, 180, 200]);
        b.set_pixel(9, 10, [180, 180, 180, 200]);

        sweep_dust(&mut b);

        assert_eq!(b.alpha(4, 4), 0, "a lone 5% pixel is dust");
        assert_eq!(b.alpha(9, 9), 200, "a pixel with a neighbour is not dust");
    }
}
