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
/// spinner, with the pointer set inside it.
///
/// This is the full-size form and it is used at **32 px and above**. Below that
/// the ring, the gaps and the space between ring and pointer are all competing
/// for the same two or three pixels and the whole thing silts up into a blob —
/// see [`sigil_small`], which is what actually renders in a tray icon.
fn sigil(colour: &str) -> String {
    wrap(&format!(
        r##"<g fill="none" stroke="{colour}" stroke-width="7" stroke-linecap="butt">
  <path d="M32 4.5 A27.5 27.5 0 0 1 59.5 32 A27.5 27.5 0 0 1 32 59.5"/>
  <path d="M21.4 6.6 A27.5 27.5 0 0 0 6.6 21.4"/>
  <path d="M4.5 32 A27.5 27.5 0 0 0 19.3 56.9"/>
</g>
<path d="M23 17 L43.5 36.5 L33.5 37.5 L38.5 49 L33.5 51 L28.5 39.5 L23 44.5 Z" fill="{colour}"/>"##
    ))
}

/// **Sigil at small sizes** — the same idea, drawn so it survives.
///
/// A solid disc with the pointer knocked out of it. This is the standard answer
/// to an icon that dies when it shrinks, and it is why macOS, Windows and
/// Firefox all ship size-specific glyphs rather than one scaled artwork: below
/// about 32 px there are not enough pixels for a stroke, a gap and a counter, so
/// the only reliable currency left is solid mass against a hole.
///
/// Same reading as the full mark — a pointer, contained — with the figure and
/// ground swapped. Every feature here is at least three units wide, which is one
/// whole pixel at 16 px.
pub fn sigil_small(colour: &str) -> String {
    // Delegates so the candidate sheet and the shipped icon cannot drift into
    // two slightly different small marks.
    crate::packs::brand::small_mark_svg(colour)
}

/// The form to draw at a given pixel size.
///
/// The switch is at 32 px: the tray, the taskbar and a favicon all ask for 16
/// and 24, and every one of them would otherwise get the version that does not
/// survive.
pub fn svg_for_size(direction: &str, colour: &str, size: u32) -> String {
    if direction == "sigil" && size < 32 {
        sigil_small(colour)
    } else {
        svg(direction, colour)
    }
}
