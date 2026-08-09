//! Candidate marks for the Cursed identity.
//!
//! Three directions, each a **single flat colour**. That is deliberate: the
//! brief requires the mark to read as a silhouette, so if a direction only works
//! once it has a gradient, it is not a logo yet. Rendering them in one colour
//! makes that pass/fail rather than a matter of opinion.
//!
//! The constraint that actually decides this is **16 px**. In the tray and the
//! favicon, anything thinner than about one pixel disappears and anything with
//! interior detail turns to mud. At a 64-unit viewBox, one tray pixel is four
//! units — so no stroke here is narrower than four, and no interior gap is
//! tighter than three.
//!
//! Nothing in the app uses these yet. They exist to be rendered into contact
//! sheets and looked at.

/// The three candidates, by id.
pub const DIRECTIONS: [&str; 3] = ["horns", "fracture", "sigil"];

pub fn svg(direction: &str, colour: &str) -> String {
    match direction {
        "horns" => horns(colour),
        "fracture" => fracture(colour),
        "sigil" => sigil(colour),
        _ => horns(colour),
    }
}

pub fn describe(direction: &str) -> &'static str {
    match direction {
        "horns" => "One solid silhouette. The pointer's tail splits into two back-swept horns \
                    formed by a V cut into its base — the 'possessed' reading comes from the \
                    shape itself, not from anything added on top. No interior detail at all, \
                    which is the safest possible behaviour at 16 px.",
        "fracture" => "The pointer split by a hard diagonal and offset, as though it glitched \
                       mid-frame. Two solid pieces with a four-unit gap — exactly one pixel at \
                       16 px, which is the riskiest part of this direction and the thing to \
                       judge on the contact sheet.",
        "sigil" => "The pointer enclosed in a broken ring: a containment mark rather than an \
                    occult one. Strongest identity of the three and the least like a cursor, \
                    but the ring is the thinnest element in any direction here, so 16 px is \
                    where it lives or dies.",
        _ => "",
    }
}

/// A wrapper with no background, so the sheet decides what it sits on.
fn wrap(body: &str) -> String {
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">{body}</svg>"##)
}

/// **Horns** — the notch becomes the identity.
///
/// A single closed path: tip, right edge, the tail spike, then back up into a
/// deep V that leaves two prongs on the left. Because it is one filled shape
/// with no holes, it degrades to a recognisable angular mass rather than to
/// noise.
fn horns(colour: &str) -> String {
    wrap(&format!(
        r##"<path d="M13 4 L47 33 L34 34.5 L41 51 L34 54 L27.5 38 L21.5 47 L22.5 31.5 L13 39 Z" fill="{colour}"/>"##
    ))
}

/// **Fracture** — one pointer, torn and displaced.
///
/// Two solid pieces rather than a stroked crack: a stroke thin enough to read as
/// a crack at 256 px would vanish entirely at 16 px, whereas an offset survives
/// because it changes the silhouette itself.
fn fracture(colour: &str) -> String {
    wrap(&format!(
        r##"<g fill="{colour}">
  <path d="M11 4 L40.5 29 L28 30 L31.5 36 L9.5 36 L11 4 Z"/>
  <path d="M14.5 40 L36 40 L44 55.5 L37 58.5 L31 45 L21 53 L14.5 40 Z"/>
</g>"##
    ))
}

/// **Sigil** — the pointer, contained.
///
/// A ring broken at two points so it reads as a mark rather than a loading
/// spinner, with the pointer set inside it at reduced scale. Six units of stroke
/// is 1.5 px at 16 px, which is the thinnest thing in any of these three.
fn sigil(colour: &str) -> String {
    wrap(&format!(
        r##"<g fill="none" stroke="{colour}" stroke-width="6" stroke-linecap="butt">
  <path d="M32 5 A27 27 0 0 1 59 32 A27 27 0 0 1 32 59"/>
  <path d="M22.5 7.7 A27 27 0 0 0 7.7 22.5"/>
  <path d="M5 32 A27 27 0 0 0 19.8 56.1"/>
</g>
<path d="M24 19 L44 36 L35 37 L39.5 47.5 L34.5 49.5 L30 39 L24 44 Z" fill="{colour}"/>"##
    ))
}
