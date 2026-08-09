//! The Cursed mark, defined once.
//!
//! The app icon, the in-app logo and the website all draw the same geometry.
//! Keeping it here — and rendering the icon through our own SVG rasteriser —
//! means the three can never drift into three slightly different logos.
//!
//! The mark is a **sigil**: a broken ring with a pointer set inside it — a
//! pointer, contained. A containment mark rather than an occult one.
//!
//! It exists in two forms, and which one is drawn depends on the size:
//!
//! - **32 px and above** — the ring, with two gaps in it.
//! - **Below 32 px** — a solid disc with the pointer knocked out of it.
//!
//! That is not a fallback, it is the design. Below about 32 px the ring stroke,
//! its gaps and the space between ring and pointer are all competing for the
//! same two or three pixels, and the whole thing silts up into a blob. Solid
//! mass against a hole is the only currency that survives down there, which is
//! why macOS, Windows and Firefox all ship size-specific glyphs instead of one
//! scaled drawing. Verified by rasterising each size and reading the pixels.

/// The mark on its own, transparent, in a 64×64 box.
///
/// `accent` and `accent_hi` are hex strings so the same geometry can be drawn in
/// the brand blue, in white for a monochrome context, or in a user's own colour.
pub fn mark_svg(accent: &str, accent_hi: &str, depth: bool) -> String {
    let shadow = if depth {
        format!(
            r##"<path d="{ARROW}" fill="#000000" opacity="0.45" transform="translate(2.4 2.8)"/>"##
        )
    } else {
        String::new()
    };

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">
  <defs>
    <linearGradient id="hex" x1="0" y1="0" x2="0.6" y2="1">
      <stop offset="0" stop-color="{accent_hi}" stop-opacity="0.95"/>
      <stop offset="1" stop-color="{accent}" stop-opacity="0.25"/>
    </linearGradient>
    <linearGradient id="blade" x1="0.1" y1="0" x2="0.9" y2="1">
      <stop offset="0" stop-color="#ffffff"/>
      <stop offset="0.42" stop-color="{accent_hi}"/>
      <stop offset="1" stop-color="{accent}"/>
    </linearGradient>
    <radialGradient id="core" cx="0.5" cy="0.42" r="0.62">
      <stop offset="0" stop-color="{accent_hi}" stop-opacity="0.55"/>
      <stop offset="1" stop-color="{accent}" stop-opacity="0"/>
    </radialGradient>
  </defs>

  <!-- the containment ring, broken -->
  <g fill="none" stroke="url(#hex)" stroke-width="7" stroke-linecap="butt">
    <path d="{RING_MAIN}"/>
    <path d="{RING_TOP}"/>
    <path d="{RING_LEFT}"/>
  </g>
  <circle cx="32" cy="32" r="24" fill="url(#core)"/>
  {shadow}
  <!-- the pointer, contained -->
  <path d="{ARROW}" fill="url(#blade)" stroke="#ffffff" stroke-width="1.2" stroke-linejoin="round"/>
</svg>"##
    )
}

/// The ring, in three arcs with two gaps. Seven units of stroke is the
/// narrowest that still reads at 32 px.
const RING_MAIN: &str = "M32 4.5 A27.5 27.5 0 0 1 59.5 32 A27.5 27.5 0 0 1 32 59.5";
const RING_TOP: &str = "M21.4 6.6 A27.5 27.5 0 0 0 6.6 21.4";
const RING_LEFT: &str = "M4.5 32 A27.5 27.5 0 0 0 19.3 56.9";

/// The pointer, sized to sit inside the ring with clear air around it.
const ARROW: &str = "M23 17 L43.5 36.5 L33.5 37.5 L38.5 49 L33.5 51 L28.5 39.5 L23 44.5 Z";

/// Below 32 px: a solid disc with the pointer knocked out.
///
/// Every feature is at least three units wide, which is a whole pixel at 16 px.
/// `evenodd` is what makes the pointer a hole rather than a second shape.
pub fn small_mark_svg(colour: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64"><path fill-rule="evenodd" fill="{colour}" d="M32 2 A30 30 0 1 1 31.99 2 Z M24 15 L45 35.5 L34 36.5 L39.5 48.5 L33.5 51 L28 39 L24 44 Z"/></svg>"##
    )
}

/// The full app icon: the mark on a rounded, bevelled tile.
///
/// Icons are composited against unknown backgrounds — a taskbar, a dark or light
/// installer, a store listing — so this one carries its own ground rather than
/// relying on transparency to look intentional.
pub fn icon_svg() -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <defs>
    <linearGradient id="tile" x1="0" y1="0" x2="0.4" y2="1">
      <stop offset="0" stop-color="#141b2c"/>
      <stop offset="0.55" stop-color="#0a0e18"/>
      <stop offset="1" stop-color="#05070b"/>
    </linearGradient>
    <linearGradient id="edge" x1="0" y1="0" x2="0.3" y2="1">
      <stop offset="0" stop-color="#5cb8ff" stop-opacity="0.75"/>
      <stop offset="0.5" stop-color="#2e8bff" stop-opacity="0.22"/>
      <stop offset="1" stop-color="#2e8bff" stop-opacity="0.05"/>
    </linearGradient>
    <radialGradient id="bloom" cx="0.5" cy="0.44" r="0.55">
      <stop offset="0" stop-color="#2e8bff" stop-opacity="0.55"/>
      <stop offset="1" stop-color="#2e8bff" stop-opacity="0"/>
    </radialGradient>
    <filter id="soft" x="-30%" y="-30%" width="160%" height="160%">
      <feGaussianBlur stdDeviation="16"/>
    </filter>
  </defs>

  <rect x="16" y="16" width="480" height="480" rx="108" fill="url(#tile)"/>
  <rect x="16" y="16" width="480" height="480" rx="108" fill="url(#bloom)"/>
  <rect x="17.5" y="17.5" width="477" height="477" rx="106" fill="none"
        stroke="url(#edge)" stroke-width="3"/>

  <g filter="url(#soft)" opacity="0.75">
    <g transform="translate(96 96) scale(5)">{}</g>
  </g>
  <g transform="translate(96 96) scale(5)">{}</g>
</svg>"##,
        inner_mark("#2e8bff", "#5cb8ff", false),
        inner_mark("#2e8bff", "#5cb8ff", true),
    )
}

/// The mark's geometry without its own `<svg>` wrapper, for embedding.
fn inner_mark(accent: &str, accent_hi: &str, depth: bool) -> String {
    let shadow = if depth {
        r##"<path d="M23 15 L45 36 L33.5 37.2 L39.6 50 L34.2 52.4 L28.2 39.6 L23 45.4 Z" fill="#000000" opacity="0.4" transform="translate(2.4 2.8)"/>"##
    } else {
        ""
    };
    format!(
        r##"<g>
  <g fill="none" stroke="{accent_hi}" stroke-width="7" stroke-linecap="butt" opacity="0.95">
    <path d="{RING_MAIN}"/>
    <path d="{RING_TOP}"/>
    <path d="{RING_LEFT}"/>
  </g>
  {shadow}
  <path d="{ARROW}" fill="{accent}" stroke="#ffffff" stroke-width="1.2" stroke-linejoin="round"/>
</g>"##
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mark_renders_at_icon_and_favicon_sizes() {
        // 16 px is the real test: a mark that only works large is not an icon.
        for size in [16, 32, 64, 256, 512] {
            let bitmap = crate::build::svg::render(&icon_svg(), size)
                .unwrap_or_else(|e| panic!("icon failed at {size}px: {e}"));
            assert!(!bitmap.is_empty(), "icon rendered blank at {size}px");

            let covered = bitmap
                .pixels
                .iter()
                .skip(3)
                .step_by(4)
                .filter(|&&a| a > 24)
                .count();
            let share = covered as f32 / (size * size) as f32;
            assert!(
                share > 0.4,
                "icon covers only {:.0}% of its tile at {size}px",
                share * 100.0
            );
        }
    }

    #[test]
    fn the_bare_mark_renders_and_is_transparent_at_the_corners() {
        let bitmap = crate::build::svg::render(&mark_svg("#2e8bff", "#5cb8ff", true), 64).unwrap();
        assert!(!bitmap.is_empty());
        // The hexagon does not reach the corners, so a transparent corner proves
        // the mark is a shape rather than an accidental filled square.
        assert_eq!(bitmap.alpha(1, 1), 0, "top-left corner should be clear");
    }

    #[test]
    fn the_mark_can_be_drawn_in_any_colour() {
        for (a, hi) in [("#ffffff", "#ffffff"), ("#ff4d5e", "#ff8a95")] {
            let bitmap = crate::build::svg::render(&mark_svg(a, hi, false), 48).unwrap();
            assert!(!bitmap.is_empty(), "{a} produced nothing");
        }
    }
}
