//! Catalog artwork, as parametric vectors.
//!
//! Every glyph is drawn in a 100x100 box, in white and grey only. Colour is
//! applied later by the tint pass, which is what turns 216 packs into an
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
    /// Long and narrow, with a fine point — reads as precise.
    Slim,
    /// Soft, rounded silhouette with no sharp interior corners.
    Round,
    /// Classic outline broken by a gap along the spine.
    Split,
    /// Wide and stubby, heavier than Classic.
    Wedge,
    /// A drawn arrow: shaft plus a separate head.
    Kite,
    /// A lightning bolt — angular and asymmetric.
    Bolt,
    /// Very fine, almost a line, for surgical work.
    Needle,
    /// Curved inward like a claw.
    Fang,
    /// A narrow beam with a flared base.
    Beam,
    /// Faceted, like a cut gem.
    Prism,
    /// A crescent sweep.
    Crescent,
    /// Two stacked chevrons and no body.
    Stack,
    /// Broken into separate shards.
    Shard,
    /// A pen nib.
    Nib,
    /// A sigil with an enclosed counter.
    Sigil,
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
    /// Open square outline.
    Square,
    /// Four corner ticks with a centre dot.
    CornerDot,
    /// A pair of opposing arcs.
    Arc,
    /// Six-point star burst.
    Star,
    /// Fine grid of crossing lines.
    Grid,
    /// Concentric rings.
    Rings,
    /// A single horizontal rule.
    Bar,
    /// Chevron above a dot.
    Caret,
    /// Cross rotated 45 degrees.
    Saltire,
    /// Hexagon outline.
    Hex,
}

/// A surface treatment layered over the form.
///
/// This is the axis that makes a large catalog worth having. Form and reticle
/// change *what* the pointer is; a treatment changes what it is made of — and
/// because everything is drawn in greys and tinted at apply time, a grey
/// gradient becomes a gradient in the user's own colour rather than a flat fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Treatment {
    /// Flat body. The original look.
    Flat,
    /// Shaded body, so it reads as a surface catching light rather than a fill.
    Gradient,
    /// An offset silhouette behind the glyph — depth without a bitmap.
    Depth,
    /// A ring sitting behind the glyph.
    Halo,
    /// Horizontal bands across the body, clipped to its shape.
    Scan,
    /// A motion streak trailing away from the tip.
    Trail,
    /// Dashed outline.
    Dashed,
    /// A bright lit edge down one side, like a bevel.
    Rim,
    /// Body broken into facets by thin cuts.
    Facet,
    /// Hollow core with a solid border, like an inlay.
    Inlay,
}

#[derive(Debug, Clone)]
pub struct Style {
    pub form: Form,
    pub fill: Fill,
    pub reticle: Reticle,
    pub treatment: Treatment,
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
            treatment: Treatment::Flat,
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
        // The hover cursor is the brand wedge now, and its point is at the top
        // right rather than a fingertip at the top left. A hotspot left at the
        // old fingertip would put the click a third of the cursor away from the
        // place the artwork says it is — visible immediately on any small target.
        Role::Hand => (0.70, 0.19),
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

    let mut defs = String::from("<defs>");
    if style.glow > 0.0 {
        defs.push_str(
            r#"<filter id="g" x="-40%" y="-40%" width="180%" height="180%"><feGaussianBlur stdDeviation="4"/></filter>"#,
        );
    }
    defs.push_str(&treatment_defs(style));
    defs.push_str("</defs>");

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">{defs}<g opacity="{:.2}"{transform}>{glow}{body}</g></svg>"#,
        style.opacity.clamp(0.05, 1.0)
    )
}

/// Definitions a treatment needs. Gradients are written in greys so the tint
/// pass turns them into shades of the user's own colour.
fn treatment_defs(style: &Style) -> String {
    match style.treatment {
        Treatment::Gradient | Treatment::Rim | Treatment::Inlay => format!(
            r#"<linearGradient id="t" x1="0" y1="0" x2="0.85" y2="1">
<stop offset="0" stop-color="{EDGE}"/><stop offset="0.55" stop-color="{BODY}"/><stop offset="1" stop-color="{DEEP}"/>
</linearGradient>"#
        ),
        Treatment::Scan => format!(
            r#"<pattern id="t" width="6" height="6" patternUnits="userSpaceOnUse">
<rect width="6" height="3" fill="{EDGE}"/><rect y="3" width="6" height="3" fill="{DEEP}"/>
</pattern>"#
        ),
        _ => String::new(),
    }
}

/// The paint used for a glyph's body under the current treatment.
fn body_paint(style: &Style) -> &'static str {
    match style.treatment {
        Treatment::Gradient | Treatment::Rim | Treatment::Inlay => "url(#t)",
        Treatment::Scan => "url(#t)",
        _ => BODY,
    }
}

/// Extra geometry drawn *behind* the glyph.
fn treatment_backdrop(style: &Style, d: &str) -> String {
    match style.treatment {
        Treatment::Depth => format!(
            r#"<path d="{d}" fill="{DEEP}" opacity="0.55" transform="translate(5 5)"/>"#
        ),
        Treatment::Halo => format!(
            r#"<circle cx="42" cy="42" r="34" fill="none" stroke="{DEEP}" stroke-width="3" opacity="0.8"/>"#
        ),
        Treatment::Trail => format!(
            r#"<path d="{d}" fill="{DEEP}" opacity="0.42" transform="translate(11 11) scale(0.94)"/>
<path d="{d}" fill="{DEEP}" opacity="0.2" transform="translate(20 20) scale(0.88)"/>"#
        ),
        _ => String::new(),
    }
}

/// Extra geometry drawn *over* the glyph, clipped to its silhouette.
fn treatment_overlay(style: &Style, d: &str) -> String {
    match style.treatment {
        Treatment::Rim => format!(
            r#"<path d="{d}" fill="none" stroke="{EDGE}" stroke-width="2.4" stroke-linejoin="round" opacity="0.95"/>"#
        ),
        Treatment::Facet => format!(
            r#"<clipPath id="c"><path d="{d}"/></clipPath>
<g clip-path="url(#c)"><path d="M-10 40 L110 10 M-10 66 L110 36 M-10 92 L110 62" stroke="{DEEP}" stroke-width="2.4" fill="none" opacity="0.9"/></g>"#
        ),
        Treatment::Inlay => format!(
            r#"<path d="{d}" fill="none" stroke="{EDGE}" stroke-width="5" stroke-linejoin="round"/>"#
        ),
        _ => String::new(),
    }
}

/// Stroke dashing, when the treatment calls for it.
fn dash(style: &Style) -> &'static str {
    if style.treatment == Treatment::Dashed {
        r#" stroke-dasharray="7 5""#
    } else {
        ""
    }
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
        Form::Slim => "M6 4 L44 66 L28 62 L34 88 L26 90 L20 64 L6 72 Z",
        // Every corner is a curve, so it reads soft at any size.
        Form::Round => {
            "M8 6 Q6 3 10 5 L58 50 Q62 54 56 55 L38 57 L50 84 Q52 89 46 90 L40 92 Q35 93 33 88 L22 62 L10 74 Q6 78 6 72 Z"
        }
        // Classic silhouette with the spine opened up.
        Form::Split => "M6 4 L58 54 L36 54 L50 88 L40 92 L27 60 L6 76 Z M18 26 L18 56 L28 46 Z",
        Form::Wedge => "M6 4 L70 44 L40 50 L54 80 L40 88 L26 58 L6 70 Z",
        // A shaft with a discrete head, rather than one silhouette.
        Form::Kite => "M6 4 L34 32 L26 40 Z M30 36 L38 28 L86 76 L78 84 Z",
        Form::Bolt => "M6 4 L46 38 L28 42 L62 74 L52 90 L20 52 L34 48 L6 22 Z",
        Form::Needle => "M6 4 L38 74 L30 72 L32 92 L24 92 L22 70 L14 72 Z",
        Form::Fang => "M6 4 Q40 26 52 62 Q40 56 30 58 L38 88 L28 90 L20 60 Q12 44 6 34 Z",
        Form::Beam => "M6 4 L30 30 L24 36 L44 84 L26 90 L12 40 L4 34 Z M10 46 L20 44 L30 82 L22 86 Z",
        Form::Prism => "M6 4 L52 44 L34 48 L30 30 Z M30 30 L34 48 L48 84 L36 90 L20 52 Z M34 48 L52 44 L48 84 Z",
        Form::Crescent => "M6 4 Q54 30 60 78 Q44 52 22 40 Q34 62 30 88 L20 86 Q26 46 6 20 Z",
        Form::Stack => "M8 6 L40 34 L32 42 L8 20 Z M12 44 L44 72 L36 80 L12 58 Z",
        Form::Shard => "M6 4 L30 24 L22 32 Z M28 32 L44 46 L34 54 Z M40 56 L56 82 L44 88 L34 62 Z",
        Form::Nib => "M6 4 L36 30 L30 40 L44 88 L32 92 L20 44 L12 38 Z M18 20 L26 27 L22 33 Z",
        Form::Sigil => {
            "M6 4 L52 42 L32 46 L46 86 L34 90 L22 52 L6 66 Z M16 22 L16 46 L26 38 Z"
        }
    }
}

/// Arrow body plus rim, honouring the fill mode and surface treatment.
fn arrow(style: &Style) -> String {
    let d = arrow_d(style.form);
    let linejoin = join(style);
    let paint = body_paint(style);
    let dashes = dash(style);

    let core = match style.fill {
        Fill::Solid => format!(
            r#"<path d="{d}" fill="{paint}" stroke="{EDGE}" stroke-width="{:.1}" stroke-linejoin="{linejoin}"{dashes}/>"#,
            style.weight * 0.55
        ),
        Fill::Outline => format!(
            r#"<path d="{d}" fill="none" stroke="{EDGE}" stroke-width="{:.1}" stroke-linejoin="{linejoin}" stroke-linecap="round"{dashes}/>"#,
            style.weight
        ),
        Fill::Hairline => format!(
            r#"<path d="{d}" fill="none" stroke="{EDGE}" stroke-width="{:.1}" stroke-linejoin="{linejoin}" stroke-linecap="round"{dashes}/>"#,
            (style.weight * 0.45).max(1.6)
        ),
    };

    format!(
        "{}{core}{}",
        treatment_backdrop(style, d),
        treatment_overlay(style, d)
    )
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

/// The precision-select glyph, plus whatever the treatment adds around it.
fn reticle(style: &Style) -> String {
    let core = reticle_core(style);
    let backdrop = match style.treatment {
        Treatment::Halo => format!(
            r#"<circle cx="50" cy="50" r="38" fill="none" stroke="{DEEP}" stroke-width="2.6" opacity="0.75"/>"#
        ),
        Treatment::Depth => format!(
            r#"<g opacity="0.45" transform="translate(3 3)">{}</g>"#,
            reticle_core(style)
        ),
        Treatment::Trail => format!(
            r#"<g opacity="0.3" transform="translate(6 6)">{}</g>"#,
            reticle_core(style)
        ),
        _ => String::new(),
    };
    format!("{backdrop}{core}")
}

fn reticle_core(style: &Style) -> String {
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
        Reticle::Square => format!(
            r#"<rect x="24" y="24" width="52" height="52" fill="none" stroke="{EDGE}" stroke-width="{w:.1}"/>"#
        ),
        Reticle::CornerDot => format!(
            r#"<path d="M24 34 L24 24 L34 24 M66 24 L76 24 L76 34 M76 66 L76 76 L66 76 M34 76 L24 76 L24 66" fill="none" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linecap="round"/>
<circle cx="50" cy="50" r="{:.1}" fill="{EDGE}"/>"#,
            w * 1.1
        ),
        Reticle::Arc => format!(
            r#"<path d="M26 34 A30 30 0 0 1 74 34 M26 66 A30 30 0 0 0 74 66" fill="none" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linecap="round"/>
<circle cx="50" cy="50" r="{thin:.1}" fill="{EDGE}"/>"#
        ),
        Reticle::Star => format!(
            r#"<path d="M50 12 L50 88 M22 28 L78 72 M78 28 L22 72" stroke="{EDGE}" stroke-width="{thin:.1}" stroke-linecap="round"/>
<circle cx="50" cy="50" r="{:.1}" fill="{EDGE}"/>"#,
            w * 0.9
        ),
        Reticle::Grid => format!(
            r#"<path d="M34 16 L34 84 M66 16 L66 84 M16 34 L84 34 M16 66 L84 66" stroke="{EDGE}" stroke-width="{thin:.1}"/>"#
        ),
        Reticle::Rings => format!(
            r#"<circle cx="50" cy="50" r="30" fill="none" stroke="{DEEP}" stroke-width="{thin:.1}"/>
<circle cx="50" cy="50" r="18" fill="none" stroke="{EDGE}" stroke-width="{w:.1}"/>
<circle cx="50" cy="50" r="{thin:.1}" fill="{EDGE}"/>"#
        ),
        Reticle::Bar => format!(
            r#"<path d="M14 50 L86 50" stroke="{EDGE}" stroke-width="{:.1}" stroke-linecap="round"/>"#,
            w * 1.4
        ),
        Reticle::Caret => format!(
            r#"<path d="M32 44 L50 26 L68 44" fill="none" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linecap="round" stroke-linejoin="round"/>
<circle cx="50" cy="66" r="{:.1}" fill="{EDGE}"/>"#,
            w * 1.2
        ),
        Reticle::Saltire => format!(
            r#"<path d="M24 24 L76 76 M76 24 L24 76" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linecap="round"/>"#
        ),
        Reticle::Hex => format!(
            r#"<path d="M50 18 L78 34 L78 66 L50 82 L22 66 L22 34 Z" fill="none" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linejoin="round"/>"#
        ),
    }
}

/// The text cursor.
///
/// Three bare strokes before, which is the shape everyone draws and nobody
/// looks at twice. The job of an I-beam is to say exactly where a character
/// will land, so this one keeps a true vertical spine and gives the serifs a
/// slight flare — wider at the ends than the middle — which reads as deliberate
/// at 32 px and still resolves at 16.
///
/// The centre gap matters more than it looks: it lets you see the character
/// underneath the cursor, which is the one thing you are trying to aim at.
fn ibeam(style: &Style) -> String {
    let w = (style.weight * 0.85).max(2.6);
    let serif = (style.weight * 1.05).max(3.2);
    format!(
        r#"<g stroke-linecap="round">
  <path d="M36 12 L64 12" stroke="{EDGE}" stroke-width="{serif:.1}"/>
  <path d="M50 16 L50 44" stroke="{EDGE}" stroke-width="{w:.1}"/>
  <path d="M50 56 L50 84" stroke="{EDGE}" stroke-width="{w:.1}"/>
  <path d="M36 88 L64 88" stroke="{EDGE}" stroke-width="{serif:.1}"/>
</g>"#
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

/// The hover cursor: the brand wedge, marked as a link.
///
/// It was a generic pointing hand — the same one every cursor pack has drawn
/// since 1995, and the thing most obviously *not* ours in a pack that is
/// otherwise all ours.
///
/// The wedge alone would be wrong: hover has to be distinguishable from the
/// ordinary pointer at a glance, and two identical silhouettes are not. So it
/// carries a ring at the shoulder — the universal "this is a link" cue, read
/// without thinking — while the silhouette stays the mark.
fn hand(style: &Style) -> String {
    let w = (style.weight * 0.45).max(1.6);
    format!(
        r#"<g transform="translate(6 4) scale(1.42)">
  <path d="{MARK_PATH}" fill="{BODY}" stroke="{EDGE}" stroke-width="{w:.1}" stroke-linejoin="round"/>
  <circle cx="52" cy="15" r="7.5" fill="none" stroke="{EDGE}" stroke-width="{ring:.1}"/>
  <circle cx="52" cy="15" r="2.6" fill="{EDGE}"/>
</g>"#,
        ring = (style.weight * 0.5).max(2.2)
    )
}

/// The wedge from the brand mark, in this module's 100-unit box.
///
/// Kept in step with `brand::MARK` by hand rather than shared, because the two
/// live in different coordinate systems and a shared constant would need a
/// transform at every use anyway.
const MARK_PATH: &str =
    "M45.15 10.76 L46.49 13.46 L44.47 34.36 L61.33 51.89 L2.00 51.89 L4.02 48.52 L44.47 11.44 Z";

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

    /// Every form the catalog can reference. Kept here so adding a variant
    /// without exercising it fails the build rather than shipping a blank
    /// cursor.
    const ALL_FORMS: [Form; 20] = [
        Form::Classic,
        Form::Triangle,
        Form::Chevron,
        Form::Blade,
        Form::Pixel,
        Form::Slim,
        Form::Round,
        Form::Split,
        Form::Wedge,
        Form::Kite,
        Form::Bolt,
        Form::Needle,
        Form::Fang,
        Form::Beam,
        Form::Prism,
        Form::Crescent,
        Form::Stack,
        Form::Shard,
        Form::Nib,
        Form::Sigil,
    ];

    const ALL_TREATMENTS: [Treatment; 10] = [
        Treatment::Flat,
        Treatment::Gradient,
        Treatment::Depth,
        Treatment::Halo,
        Treatment::Scan,
        Treatment::Trail,
        Treatment::Dashed,
        Treatment::Rim,
        Treatment::Facet,
        Treatment::Inlay,
    ];

    const ALL_RETICLES: [Reticle; 24] = [
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
        Reticle::Square,
        Reticle::CornerDot,
        Reticle::Arc,
        Reticle::Star,
        Reticle::Grid,
        Reticle::Rings,
        Reticle::Bar,
        Reticle::Caret,
        Reticle::Saltire,
        Reticle::Hex,
    ];

    #[test]
    fn every_role_renders_to_parseable_svg_for_every_form() {
        for form in ALL_FORMS {
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
        for reticle_variant in ALL_RETICLES {
            let style = Style {
                reticle: reticle_variant,
                ..Style::default()
            };
            let svg = render_role(&style, Role::Crosshair, 0.0);
            let bitmap = crate::build::svg::render(&svg, 64).unwrap();
            assert!(!bitmap.is_empty(), "{reticle_variant:?} rendered blank");
        }
    }

    /// Treatments add geometry and paints; a broken one would silently render a
    /// blank or a black box, which is exactly the sort of thing that ships.
    #[test]
    fn every_treatment_renders_on_every_form() {
        for treatment in ALL_TREATMENTS {
            for form in ALL_FORMS {
                let style = Style {
                    form,
                    treatment,
                    ..Style::default()
                };
                for role in [Role::Arrow, Role::Crosshair] {
                    let svg = render_role(&style, role, 0.0);
                    let bitmap = crate::build::svg::render(&svg, 64).unwrap_or_else(|e| {
                        panic!("{treatment:?}/{form:?}/{role} failed to render: {e}")
                    });
                    assert!(!bitmap.is_empty(), "{treatment:?}/{form:?}/{role} was blank");
                }
            }
        }
    }

    /// A glyph that renders but sits mostly outside the box is worse than one
    /// that fails outright, because it only shows up once it is on screen.
    #[test]
    fn every_form_fills_a_reasonable_share_of_the_canvas() {
        for form in ALL_FORMS {
            let style = Style {
                form,
                ..Style::default()
            };
            let bitmap =
                crate::build::svg::render(&render_role(&style, Role::Arrow, 0.0), 64).unwrap();
            let covered = bitmap
                .pixels
                .iter()
                .skip(3)
                .step_by(4)
                .filter(|&&a| a > 24)
                .count();
            let share = covered as f32 / (64.0 * 64.0);
            assert!(
                (0.02..0.75).contains(&share),
                "{form:?} covers {:.1}% of the canvas",
                share * 100.0
            );
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
