//! The Cursed mark, defined once.
//!
//! The app icon, the in-app logo, the tray icon and the website all draw the
//! same geometry. Keeping it here — and rendering the icon through our own SVG
//! rasteriser — means the four can never drift into four slightly different
//! logos.
//!
//! The mark is a pointer seen almost edge-on: a broad wedge with a flat base and
//! a curled tip, as though the cursor has been tipped forward out of the screen.
//!
//! The path is **traced from the supplied artwork**, not transcribed by eye.
//! `genpacks --trace <png>` masks the image, walks the shape's boundary and
//! simplifies it, so the angles are the artwork's own. A logo redrawn by hand
//! from a picture is always nearly right, which is the worst way for a logo to
//! be wrong: every angle slightly off, and nothing to check it against.
//!
//! One solid silhouette, no interior detail and no counters, which is why —
//! unlike the ring it replaced — it needs no separate small-size drawing. It
//! degrades to a recognisable wedge rather than to mud.

/// The mark on its own, transparent, in a 64×64 box.
///
/// `accent` and `accent_hi` are hex strings so the same geometry can be drawn in
/// the brand blue, in white for a monochrome context, or in a user's own colour.
pub fn mark_svg(accent: &str, accent_hi: &str, depth: bool) -> String {
    let shadow = if depth {
        format!(
            r##"<path d="{MARK}" fill="#000000" opacity="0.40" transform="translate(2.2 2.6)"/>"##
        )
    } else {
        String::new()
    };

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">
  <defs>
    <linearGradient id="blade" x1="0.15" y1="0" x2="0.85" y2="1">
      <stop offset="0" stop-color="#ffffff"/>
      <stop offset="0.45" stop-color="{accent_hi}"/>
      <stop offset="1" stop-color="{accent}"/>
    </linearGradient>
    <radialGradient id="core" cx="0.5" cy="0.6" r="0.6">
      <stop offset="0" stop-color="{accent_hi}" stop-opacity="0.45"/>
      <stop offset="1" stop-color="{accent}" stop-opacity="0"/>
    </radialGradient>
  </defs>

  <ellipse cx="32" cy="40" rx="30" ry="22" fill="url(#core)"/>
  {shadow}
  <path d="{MARK}" fill="url(#blade)"/>
</svg>"##
    )
}

/// The pointer wedge, traced from the supplied artwork and centred in a 64-unit
/// box. Seven points: the curled tip, the shoulder, the base, and the leading
/// edge back up to the tip.
pub const MARK: &str =
    "M45.15 10.76 L46.49 13.46 L44.47 34.36 L61.33 51.89 L2.00 51.89 L4.02 48.52 L44.47 11.44 Z";

/// Flat single-colour form, for a tray icon, a stencil or a silhouette test.
///
/// It is the same path. The mark carries no stroke, no gap and no counter, so
/// there is nothing that needs redrawing to survive 16 px — which is exactly
/// what the ring it replaced could not manage.
pub fn small_mark_svg(colour: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64"><path d="{MARK}" fill="{colour}"/></svg>"##
    )
}

/// The full lockup: the mark above the wordmark.
///
/// The letterforms are traced from the same artwork, counters and all, so the
/// wordmark is the supplied one rather than an approximation set in whatever
/// face happened to be to hand.
pub fn lockup_svg(colour: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 61" width="100" height="61" fill="{colour}">{LOCKUP}</svg>"##
    )
}

/// Mark and wordmark in one coordinate space, exactly as they sit in the
/// supplied file. `evenodd` is what keeps the counters in R and D open.
pub const LOCKUP: &str = concat!(
    r#"<path fill-rule="evenodd" d="M61.46 0.00 L62.50 0.52 L62.50 2.08 L60.94 18.23 L73.96 31.77 L28.13 31.77 L29.69 29.17 L60.94 0.52 Z"/>"#,
    r#"<path fill-rule="evenodd" d="M4.17 33.33 L9.90 33.33 L13.02 35.42 L13.54 42.19 L9.90 42.19 L9.38 38.54 L8.33 37.50 L5.73 37.50 L4.69 38.54 L4.69 55.73 L8.85 56.25 L9.38 52.08 L13.02 51.56 L13.02 58.85 L10.42 60.42 L3.12 60.42 L0.52 57.81 L0.00 38.02 L0.52 35.94 L3.65 33.85 Z"/>"#,
    r#"<path fill-rule="evenodd" d="M17.71 36.98 L21.88 37.50 L22.40 56.25 L26.56 55.73 L26.56 37.50 L30.73 37.50 L30.73 57.29 L29.69 59.38 L27.60 60.42 L20.31 60.42 L17.71 58.33 L17.71 37.50 Z"/>"#,
    r#"<path fill-rule="evenodd" d="M34.90 36.98 L44.79 36.98 L47.92 40.10 L47.92 50.00 L45.83 52.60 L47.92 60.42 L43.75 60.42 L41.67 53.13 L38.54 53.65 L38.54 60.42 L34.90 60.42 L34.90 37.50 Z M39.06 41.15 L43.23 41.67 L42.71 48.96 L39.06 48.96 L39.06 41.67 Z"/>"#,
    r#"<path fill-rule="evenodd" d="M54.69 36.98 L61.46 36.98 L64.58 39.06 L64.58 44.27 L60.94 44.27 L60.94 42.19 L59.38 40.62 L55.73 41.67 L56.25 45.83 L64.58 48.44 L64.58 58.33 L63.54 59.90 L54.69 60.42 L51.56 57.29 L52.08 53.13 L55.21 53.13 L56.77 56.77 L59.90 56.77 L60.94 55.73 L60.42 51.04 L52.60 48.96 L51.56 47.92 L51.56 40.10 L54.17 37.50 Z"/>"#,
    r#"<path fill-rule="evenodd" d="M68.75 36.98 L81.77 36.98 L81.77 41.15 L72.92 41.67 L73.44 46.88 L80.21 47.40 L79.69 51.04 L72.92 51.56 L72.92 55.73 L81.25 56.25 L81.77 60.42 L68.75 60.42 L68.75 37.50 Z"/>"#,
    r#"<path fill-rule="evenodd" d="M85.94 36.98 L95.83 36.98 L98.44 38.54 L99.48 42.19 L99.48 55.73 L98.44 58.85 L96.35 60.42 L85.94 60.42 L85.94 37.50 Z M90.62 41.67 L94.79 42.19 L94.27 55.73 L90.62 55.73 L90.62 42.19 Z"/>"#,
);

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
    <linearGradient id="blade" x1="0.15" y1="0" x2="0.85" y2="1">
      <stop offset="0" stop-color="#ffffff"/>
      <stop offset="0.45" stop-color="#5cb8ff"/>
      <stop offset="1" stop-color="#2e8bff"/>
    </linearGradient>
    <radialGradient id="bloom" cx="0.5" cy="0.52" r="0.55">
      <stop offset="0" stop-color="#2e8bff" stop-opacity="0.55"/>
      <stop offset="1" stop-color="#2e8bff" stop-opacity="0"/>
    </radialGradient>
    <filter id="soft" x="-30%" y="-30%" width="160%" height="160%">
      <feGaussianBlur stdDeviation="14"/>
    </filter>
  </defs>

  <rect x="16" y="16" width="480" height="480" rx="108" fill="url(#tile)"/>
  <rect x="16" y="16" width="480" height="480" rx="108" fill="url(#bloom)"/>
  <rect x="17.5" y="17.5" width="477" height="477" rx="106" fill="none"
        stroke="url(#edge)" stroke-width="3"/>

  <g filter="url(#soft)" opacity="0.7">
    <g transform="translate(104 104) scale(4.75)"><path d="{MARK}" fill="#2e8bff"/></g>
  </g>
  <g transform="translate(104 104) scale(4.75)"><path d="{MARK}" fill="url(#blade)"/></g>
</svg>"##
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
        }
    }

    #[test]
    fn the_flat_form_is_the_same_path_as_the_full_one() {
        // One silhouette, no size-specific redraw. If these ever diverge the
        // tray icon stops being the same logo as the app icon.
        assert!(small_mark_svg("#ffffff").contains(MARK));
        assert!(mark_svg("#2e8bff", "#5cb8ff", false).contains(MARK));
    }

    #[test]
    fn the_mark_survives_being_drawn_small_and_flat() {
        for size in [16, 24, 32] {
            let bitmap = crate::build::svg::render(&small_mark_svg("#ffffff"), size)
                .unwrap_or_else(|e| panic!("flat mark failed at {size}px: {e}"));
            // A shape that rasterises to almost nothing at 16 px is not a tray
            // icon, whatever it looks like at 256.
            let inked = (0..bitmap.height)
                .flat_map(|y| (0..bitmap.width).map(move |x| (x, y)))
                .filter(|&(x, y)| bitmap.alpha(x, y) > 32)
                .count();
            let total = (bitmap.width * bitmap.height) as usize;
            assert!(
                inked * 100 / total >= 15,
                "only {inked}/{total} pixels inked at {size}px"
            );
        }
    }

    #[test]
    fn the_lockup_keeps_its_counters_open() {
        // R and D enclose a hole. Without evenodd they fill in solid and the
        // wordmark turns into blocks.
        assert_eq!(LOCKUP.matches("fill-rule=\"evenodd\"").count(), 7);
        let bitmap = crate::build::svg::render(&lockup_svg("#ffffff"), 256)
            .expect("lockup renders");
        assert!(!bitmap.is_empty());
    }
}
