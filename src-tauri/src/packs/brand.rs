//! The Cursed mark, defined once.
//!
//! The app icon, the in-app logo and the website all draw the same geometry.
//! Keeping it here — and rendering the icon through our own SVG rasteriser —
//! means the three can never drift into three slightly different logos.
//!
//! The mark is a **forge core**: a hexagonal chamber, a molten seam across it,
//! and a pointer struck through the middle. It reads as a pointer at 16 px and
//! as a piece of machinery at 512 px, which is the whole trick with an icon that
//! has to live in a taskbar and on a store page.

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

  <!-- forge chamber -->
  <path d="{HEX}" fill="url(#core)" stroke="url(#hex)" stroke-width="2.4" stroke-linejoin="round"/>
  <!-- molten seam -->
  <path d="M14 40 L50 26" stroke="{accent_hi}" stroke-width="1.6" stroke-linecap="round" opacity="0.5"/>
  {shadow}
  <!-- the pointer, struck through the core -->
  <path d="{ARROW}" fill="url(#blade)" stroke="#ffffff" stroke-width="1.5" stroke-linejoin="round"/>
  <!-- spark -->
  <circle cx="47" cy="19" r="2.6" fill="#ffffff" opacity="0.92"/>
</svg>"##
    )
}

/// Hexagonal chamber, flat-topped, centred in the box.
const HEX: &str = "M32 5 L54 17.5 L54 46.5 L32 59 L10 46.5 L10 17.5 Z";

/// The pointer. Sized so its tip sits inside the hexagon at every scale.
const ARROW: &str = "M23 15 L45 36 L33.5 37.2 L39.6 50 L34.2 52.4 L28.2 39.6 L23 45.4 Z";

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
  <path d="{HEX}" fill="none" stroke="{accent_hi}" stroke-width="2.4" stroke-linejoin="round" opacity="0.9"/>
  <path d="M14 40 L50 26" stroke="{accent_hi}" stroke-width="1.6" stroke-linecap="round" opacity="0.5"/>
  {shadow}
  <path d="{ARROW}" fill="{accent}" stroke="#ffffff" stroke-width="1.5" stroke-linejoin="round"/>
  <circle cx="47" cy="19" r="2.6" fill="#ffffff" opacity="0.92"/>
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
