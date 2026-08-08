//! Catalog artwork, as parametric vectors.
//!
//! Every glyph is drawn in a 100x100 box, in white and grey only. Colour is
//! applied later by the tint pass, which is what turns 64 packs into an
//! effectively unbounded catalog from a payload of roughly nothing (PRD §7.1).
//!
//! Greys are not decoration: the tint multiplies the master's luminance, so a
//! mid-grey interior becomes a darker shade of the user's colour and the
//! artwork keeps its form instead of flattening into a silhouette.

use crate::cursor::roles::Role;

/// The white edge and the grey body every glyph is built from.
const EDGE: &str = "#ffffff";
const BODY: &str = "#c8c8c8";
const DEEP: &str = "#8a8a8a";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// The familiar Windows pointer silhouette.
    Classic,
    /// No tail — a clean triangle.
    Triangle,
    /// Outline only, hairline weight.
    Chevron,
    /// Narrow and raked, for the gaming and blade packs.
    Blade,
    /// Stair-stepped, quantised to an 8x8 grid.
    Pixel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// Filled body with a bright rim.
    Solid,
    /// Stroked outline over a hollow centre.
    Outline,
    /// A single thin stroke.
    Hairline,
}

/// How the precision-select role is drawn. This is the role the PRECISION and
/// GAMING packs exist to differentiate, so it carries the most variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reticle {
    Plus,
    ThinCross,
    Dot,
    TCross,
    GapCross,
    MicroDot,
    Circle,
    Diamond,
    Bracket,
    ChevronPair,
    Notch,
    DotRing,
    CircleCross,
    TripleTick,
}

#[derive(Debug, Clone)]
pub struct Style {
    pub form: Form,
    pub fill: Fill,
    pub reticle: Reticle,
    /// Stroke weight in viewBox units.
    pub weight: f32,
    /// Outer glow opacity; 0 disables the glow pass entirely.
    pub glow: f32,
    /// Corner treatment: `round` for soft packs, `miter` for technical ones.
    pub round_joins: bool,
    /// Overall opacity, for the ghost-style packs.
    pub opacity: f32,
    /// Scale applied to the whole glyph, for nano / micro packs.
    pub scale: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            form: Form::Classic,
            fill: Fill::Solid,
            reticle: Reticle::Plus,
            weight: 5.0,
            glow: 0.0,
            round_joins: false,
            opacity: 1.0,
            scale: 1.0,
        }
    }
}

/// Normalised hotspot per role. Stored 0.0-1.0 so it survives every target size
/// (PRD §5.3). The five arrow-plus-badge roles share the arrow's tip, exactly as
/// Windows' own scheme does.
pub const fn hotspot(role: Role) -> (f32, f32) {
    match role {
        Role::Arrow
        | Role::Help
        | Role::AppStarting
        | Role::Pin
        | Role::Person
        | Role::NWPen => (0.06, 0.04),
        Role::Hand => (0.34, 0.06),
        Role::UpArrow => (0.5, 0.06),
        _ => (0.5, 0.5),
    }
}

fn join(style: &Style) -> &'static str {
    if style.round_joins {
        "round"
    } else {
        "miter"
    }
}

/// Wraps glyph markup in a document, applying the glow, opacity and scale
/// passes that are common to every role.
///
/// The scale is applied **about the role's own hotspot**, not about the corner
/// or the centre. Scale about anything else and a shrunk glyph drifts away from
/// its declared click point — a nano-sized crosshair would end up clicking a few
/// pixels off its own centre, which is the exact defect normalised hotspots
/// exist to rule out.
fn document(style: &Style, role: Role, body: String) -> String {
    let scale = style.scale.clamp(0.4, 1.2);
    let (hx, hy) = hotspot(role);
    let transform = if (scale - 1.0).abs() < f32::EPSILON {
        String::new()
    } else {
        format!(
            r#" transform="translate({:.2} {:.2}) scale({scale})""#,
            hx * 100.0 * (1.0 - scale),
            hy * 100.0 * (1.0 - scale)
        )
    };

    let glow = if style.glow > 0.0 {
        format!(
            r#"<g opacity="{:.2}" filter="url(#g)">{body}</g>"#,
            style.glow.clamp(0.0, 1.0)
        )
    } else {
        String::new()
    };

    let defs = if style.glow > 0.0 {
        r#"<defs><filter id="g" x="-40%" y="-40%" width="180%" height="180%"><feGaussianBlur stdDeviation="4"/></filter></defs>"#
    } else {
        ""
    };

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">{defs}<g opacity="{:.2}"{transform}>{glow}{body}</g></svg>"#,
        style.opacity.clamp(0.05, 1.0)
    )
}

/// The arrow silhouette for a form, as a path `d` attribute.
fn arrow_d(form: Form) -> &'static str {
    match form {
        Form::Classic => "M6 4 L60 56 L37 56 L52 88 L40 94 L25 62 L6 78 Z",
        Form::Triangle => "M6 4 L58 50 L6 68 Z",
        Form::Chevron => "M7 5 L55 51 L34 51 L46 82",
        Form::Blade => "M6 4 L46 72 L30 64 L24 90 L14 88 L18 58 Z",
        // Quantised to an 8-unit grid: every vertex lands on a pixel boundary,
        // which is what keeps a retro pack looking drawn rather than resampled.
        Form::Pixel => {
            "M8 8 L8 72 L24 56 L24 72 L32 88 L48 88 L40 64 L56 64 L56 56 L40 40 L40 24 L24 24 L24 8 Z"
        }
    }
}

/// Arrow body plus rim, honouring the fill mode.
fn arrow(style: &Style) -> String {
    let d = arrow_d(style.form);
    let linejoin = join(style);
    match style.fill {
        Fill::Solid => format!(
            r#"<path d="{d}" fill="{BODY}" stroke="{EDGE}" stroke-width="{:.1}" stroke-linejoin="{linejoin}"/>"#,
            style.weight * 0.55
        ),
        Fill::Outline => format!(
            r#"<path d="{d}" fill="none" stroke="{EDGE}" stroke-width="{:.1}" stroke-linejoin="{linejoin}" stroke-linecap="round"/>"#,
            style.weight
        ),
        Fill::Hairline => format!(
            r#"<path d="{d}" fill="none" stroke="{EDGE}" stroke-width="{:.1}" stroke-linejoin="{linejoin}" stroke-linecap="round"/>"#,
            (style.weight * 0.45).max(1.6)
        ),
    }
}

/// A small mark riding on the arrow's shoulder, used by Help, AppStarting, Pin
/// and Person — the four roles Windows itself draws as arrow-plus-badge.
fn badge(style: &Style, inner: &str) -> String {
    format!(
        r#"{}<g transform="translate(52 6)">{inner}</g>"#,
        arrow(style)
    )
}

fn question_mark() -> String {
    // Drawn as geometry, not text: the renderer ships without font support on
    // purpose, so nothing in the catalog can depend on a system typeface.
    format!(
        r#"<path d="M6 12 A10 10 0 1 1 20 22 L20 28" fill="none" stroke="{EDGE}" stroke-width="6" stroke-linecap="round"/>
<circle cx="20" cy="39" r="4" fill="{EDGE}"/>"#
    )
}

fn busy_ring(rotation: f32) -> String {
    format!(
        r#"<g transform="rotate({rotation:.1} 21 21)">
<circle cx="21" cy="21" r="15" fill="none" stroke="{DEEP}" stroke-width="6"/>
<path d="M21 6 A15 15 0 0 1 36 21" fill="none" stroke="{EDGE}" stroke-width="6" stroke-linecap="round"/></g>"#
    )
}

fn pin_badge() -> String {
    format!(
        r#"<path d="M21 4 A13 13 0 0 1 34 17 C34 27 21 42 21 42 C21 42 8 27 8 17 A13 13 0 0 1 21 4 Z" fill="{BODY}" stroke="{EDGE}" stroke-width="4" stroke-linejoin="round"/>
<circle cx="21" cy="17" r="5" fill="{DEEP}"/>"#
    )
}

fn person_badge() -> String {
    format!(
        r#"<circle cx="21" cy="13" r="9" fill="{BODY}" stroke="{EDGE}" stroke-width="4"/>
<path d="M5 40 A16 14 0 0 1 37 40 Z" fill="{BODY}" stroke="{EDGE}" stroke-width="4" stroke-linejoin="round"/>"#
    )
}

fn reticle(style: &Style) -> String {
    let w = style.weight.max(2.0);
    let thin = (w * 0.55).max(1.4);
    match style.reticle {
        Reticle::Plus => format!(
            r#"<path d="M50 14 L50 86 M14 50 L86 50" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linecap="round"/>"#
        ),
        Reticle::ThinCross => format!(
            r#"<path d="M50 10 L50 90 M10 50 L90 50" stroke="{EDGE}" stroke-width="{thin:.1}" stroke-linecap="butt"/>"#
        ),
        Reticle::Dot => format!(
            r#"<circle cx="50" cy="50" r="{:.1}" fill="{EDGE}"/>"#,
            w * 1.6
        ),
        Reticle::TCross => format!(
            r#"<path d="M14 50 L86 50 M50 50 L50 88" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linecap="round"/>"#
        ),
        Reticle::GapCross => format!(
            r#"<path d="M50 12 L50 38 M50 62 L50 88 M12 50 L38 50 M62 50 L88 50" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linecap="round"/>"#
        ),
        Reticle::MicroDot => format!(
            r#"<circle cx="50" cy="50" r="{:.1}" fill="{EDGE}"/>
<circle cx="50" cy="50" r="{:.1}" fill="none" stroke="{DEEP}" stroke-width="{thin:.1}"/>"#,
            w * 0.8,
            w * 3.2
        ),
        Reticle::Circle => format!(
            r#"<circle cx="50" cy="50" r="26" fill="none" stroke="{EDGE}" stroke-width="{w:.1}"/>"#
        ),
        Reticle::Diamond => format!(
            r#"<path d="M50 18 L82 50 L50 82 L18 50 Z" fill="none" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linejoin="miter"/>"#
        ),
        Reticle::Bracket => format!(
            r#"<path d="M22 30 L22 18 L34 18 M66 18 L78 18 L78 30 M78 70 L78 82 L66 82 M34 82 L22 82 L22 70" fill="none" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linecap="round"/>"#
        ),
        Reticle::ChevronPair => format!(
            r#"<path d="M30 34 L50 50 L30 66 M70 34 L50 50 L70 66" fill="none" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linecap="round" stroke-linejoin="round"/>"#
        ),
        Reticle::Notch => format!(
            r#"<path d="M50 12 L50 34 M50 66 L50 88" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linecap="round"/>
<circle cx="50" cy="50" r="{:.1}" fill="{EDGE}"/>"#,
            w * 0.9
        ),
        Reticle::DotRing => format!(
            r#"<circle cx="50" cy="50" r="22" fill="none" stroke="{DEEP}" stroke-width="{thin:.1}"/>
<circle cx="50" cy="50" r="{:.1}" fill="{EDGE}"/>"#,
            w * 1.3
        ),
        Reticle::CircleCross => format!(
            r#"<circle cx="50" cy="50" r="24" fill="none" stroke="{EDGE}" stroke-width="{thin:.1}"/>
<path d="M50 16 L50 34 M50 66 L50 84 M16 50 L34 50 M66 50 L84 50" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linecap="round"/>"#
        ),
        Reticle::TripleTick => format!(
            r#"<path d="M50 14 L50 30 M20 50 L36 50 M80 50 L64 50 M50 86 L50 70" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linecap="round"/>
<circle cx="50" cy="50" r="{thin:.1}" fill="{EDGE}"/>"#
        ),
    }
}

fn ibeam(style: &Style) -> String {
    let w = (style.weight * 0.9).max(3.0);
    format!(
        r#"<path d="M38 14 L62 14 M50 14 L50 86 M38 86 L62 86" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linecap="round"/>"#
    )
}

fn pen(style: &Style) -> String {
    let w = (style.weight * 0.6).max(2.0);
    format!(
        r#"<path d="M8 6 L34 20 L82 78 L72 88 L18 44 Z" fill="{BODY}" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linejoin="round"/>
<path d="M8 6 L20 30 L30 18 Z" fill="{EDGE}"/>"#
    )
}

fn unavailable(style: &Style) -> String {
    let w = (style.weight * 1.6).max(7.0);
    format!(
        r#"<circle cx="50" cy="50" r="34" fill="none" stroke="{EDGE}" stroke-width="{w:.1}"/>
<path d="M26 26 L74 74" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linecap="round"/>"#
    )
}

fn hourglass(style: &Style) -> String {
    let w = (style.weight * 0.6).max(2.0);
    format!(
        r#"<path d="M30 12 L70 12 L70 30 L54 50 L70 70 L70 88 L30 88 L30 70 L46 50 L30 30 Z" fill="{BODY}" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linejoin="round"/>
<path d="M36 20 L64 20 L50 44 Z" fill="{DEEP}"/>"#
    )
}

/// A double-headed resize arrow along an arbitrary angle.
fn resize(style: &Style, degrees: f32) -> String {
    let w = (style.weight * 1.1).max(4.0);
    format!(
        r#"<g transform="rotate({degrees:.1} 50 50)">
<path d="M50 12 L64 30 L56 30 L56 70 L64 70 L50 88 L36 70 L44 70 L44 30 L36 30 Z" fill="{BODY}" stroke="{EDGE}" stroke-width="{:.1}" stroke-linejoin="round"/></g>"#,
        w * 0.45
    )
}

fn size_all(style: &Style) -> String {
    let w = (style.weight * 0.45).max(2.0);
    format!(
        r#"<path d="M50 8 L62 24 L54 24 L54 46 L76 46 L76 38 L92 50 L76 62 L76 54 L54 54 L54 76 L62 76 L50 92 L38 76 L46 76 L46 54 L24 54 L24 62 L8 50 L24 38 L24 46 L46 46 L46 24 L38 24 Z" fill="{BODY}" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linejoin="round"/>"#
    )
}

fn up_arrow(style: &Style) -> String {
    let w = (style.weight * 0.5).max(2.0);
    format!(
        r#"<path d="M50 6 L78 44 L60 44 L60 92 L40 92 L40 44 L22 44 Z" fill="{BODY}" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linejoin="round"/>"#
    )
}

fn hand(style: &Style) -> String {
    let w = (style.weight * 0.5).max(2.0);
    format!(
        r#"<path d="M28 6 C34 6 38 10 38 16 L38 48 L44 48 L44 30 C44 25 48 22 52 22 C56 22 60 25 60 30 L60 50 L66 50 L66 36 C66 31 70 28 74 28 C78 28 82 31 82 36 L82 68 C82 82 72 94 56 94 L46 94 C34 94 26 86 22 74 L14 52 C12 46 15 41 20 39 C24 38 28 40 30 45 L34 54 L34 16 C34 10 22 6 28 6 Z" fill="{BODY}" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linejoin="round"/>"#
    )
}

/// Renders one role, at animation phase `t` (0.0-1.0; ignored by still roles).
pub fn render_role(style: &Style, role: Role, t: f32) -> String {
    let body = match role {
        Role::Arrow => arrow(style),
        Role::Help => badge(style, &question_mark()),
        Role::AppStarting => badge(style, &busy_ring(t * 360.0)),
        Role::Wait => {
            // The busy cursor is the one role users stare at, so it gets the
            // rotation when the pack is animated and an hourglass when it is not.
            if t > 0.0 {
                format!(
                    r#"<g transform="translate(29 29) scale(1.9)">{}</g>"#,
                    busy_ring(t * 360.0)
                )
            } else {
                hourglass(style)
            }
        }
        Role::Crosshair => reticle(style),
        Role::IBeam => ibeam(style),
        Role::NWPen => pen(style),
        Role::No => unavailable(style),
        Role::SizeNS => resize(style, 0.0),
        Role::SizeWE => resize(style, 90.0),
        Role::SizeNWSE => resize(style, 45.0),
        Role::SizeNESW => resize(style, -45.0),
        Role::SizeAll => size_all(style),
        Role::UpArrow => up_arrow(style),
        Role::Hand => hand(style),
        Role::Pin => badge(style, &pin_badge()),
        Role::Person => badge(style, &person_badge()),
    };
    document(style, role, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cursor::roles::ALL_ROLES;

    #[test]
    fn every_role_renders_to_parseable_svg_for_every_form() {
        for form in [
            Form::Classic,
            Form::Triangle,
            Form::Chevron,
            Form::Blade,
            Form::Pixel,
        ] {
            let style = Style {
                form,
                ..Style::default()
            };
            for role in ALL_ROLES {
                let svg = render_role(&style, role, 0.0);
                assert!(svg.starts_with("<svg"), "{role} produced no document");
                assert!(svg.ends_with("</svg>"));
                let bitmap = crate::build::svg::render(&svg, 64)
                    .unwrap_or_else(|e| panic!("{form:?}/{role} failed to render: {e}"));
                assert!(!bitmap.is_empty(), "{form:?}/{role} rendered blank");
            }
        }
    }

    #[test]
    fn every_reticle_variant_draws_something() {
        for reticle_variant in [
            Reticle::Plus,
            Reticle::ThinCross,
            Reticle::Dot,
            Reticle::TCross,
            Reticle::GapCross,
            Reticle::MicroDot,
            Reticle::Circle,
            Reticle::Diamond,
            Reticle::Bracket,
            Reticle::ChevronPair,
            Reticle::Notch,
            Reticle::DotRing,
            Reticle::CircleCross,
            Reticle::TripleTick,
        ] {
            let style = Style {
                reticle: reticle_variant,
                ..Style::default()
            };
            let svg = render_role(&style, Role::Crosshair, 0.0);
            let bitmap = crate::build::svg::render(&svg, 64).unwrap();
            assert!(!bitmap.is_empty(), "{reticle_variant:?} rendered blank");
        }
    }

    #[test]
    fn arrow_hotspot_sits_on_opaque_pixels() {
        let style = Style::default();
        let bitmap = crate::build::svg::render(&render_role(&style, Role::Arrow, 0.0), 64).unwrap();
        let (hx, hy) = hotspot(Role::Arrow);
        let x = (hx * 63.0).round() as u32;
        let y = (hy * 63.0).round() as u32;
        // The declared tip must land on artwork, or the click point floats in
        // empty space — the exact defect this normalisation exists to prevent.
        let nearby = (0..=3).any(|d| {
            bitmap.alpha(x.saturating_add(d).min(63), y.saturating_add(d).min(63)) > 0
        });
        assert!(nearby, "arrow hotspot is not on the artwork");
    }

    #[test]
    fn animation_phase_changes_the_busy_glyph() {
        let style = Style::default();
        let first = render_role(&style, Role::Wait, 0.1);
        let second = render_role(&style, Role::Wait, 0.6);
        assert_ne!(first, second, "phase must affect the rendered frame");
    }
}
