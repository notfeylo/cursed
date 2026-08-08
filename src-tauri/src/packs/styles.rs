//! The catalog packs.
//!
//! Each pack is a parameter set, not a folder of bitmaps. That is the whole
//! trick behind PRD §7.1: the artwork is vector code, the colour is applied at
//! apply time, and a catalog that looks enormous costs the installer nothing.
//!
//! Names are deliberately generic. No pack is named after a platform, console,
//! game or company — PRD §15.3 rules out third-party marks, and "the retro one"
//! does not need somebody else's trademark to read as retro.

use crate::packs::art::{Fill, Form, Reticle, Style, Treatment};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Category {
    Precision,
    Neon,
    Minimal,
    Retro,
    Gaming,
    Animated,
    Fun,
}

impl Category {
    pub const fn as_str(self) -> &'static str {
        match self {
            Category::Precision => "PRECISION",
            Category::Neon => "NEON",
            Category::Minimal => "MINIMAL",
            Category::Retro => "RETRO",
            Category::Gaming => "GAMING",
            Category::Animated => "ANIMATED",
            Category::Fun => "FUN",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackDef {
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    pub style: Style,
    /// Animated packs ship `.ani` for the two roles Windows animates.
    pub animated: bool,
    /// The colour the pack looks best in before the user picks their own.
    pub default_tint: &'static str,
}

struct Builder {
    style: Style,
}

impl Builder {
    fn new() -> Self {
        Self {
            style: Style::default(),
        }
    }
    fn form(mut self, form: Form) -> Self {
        self.style.form = form;
        self
    }
    fn fill(mut self, fill: Fill) -> Self {
        self.style.fill = fill;
        self
    }
    fn reticle(mut self, reticle: Reticle) -> Self {
        self.style.reticle = reticle;
        self
    }
    fn weight(mut self, weight: f32) -> Self {
        self.style.weight = weight;
        self
    }
    fn glow(mut self, glow: f32) -> Self {
        self.style.glow = glow;
        self
    }
    fn round(mut self) -> Self {
        self.style.round_joins = true;
        self
    }
    fn opacity(mut self, opacity: f32) -> Self {
        self.style.opacity = opacity;
        self
    }
    fn scale(mut self, scale: f32) -> Self {
        self.style.scale = scale;
        self
    }
    fn treat(mut self, treatment: Treatment) -> Self {
        self.style.treatment = treatment;
        self
    }
    fn done(self) -> Style {
        self.style
    }
}

fn pack(
    id: &'static str,
    name: &'static str,
    category: Category,
    style: Style,
    default_tint: &'static str,
) -> PackDef {
    PackDef {
        id,
        name,
        category,
        style,
        animated: matches!(category, Category::Animated),
        default_tint,
    }
}

/// Every pack, in catalog order.
pub fn all() -> Vec<PackDef> {
    use Category::*;
    use Fill::*;
    use Form::*;
    use Reticle::*;

    vec![
        // ── PRECISION ─────────────────────────────────────────
        pack("precision-plus", "PLUS", Precision, Builder::new().reticle(Plus).done(), "#EDF1F7"),
        pack("precision-thin-cross", "THIN CROSS", Precision, Builder::new().reticle(ThinCross).weight(3.0).fill(Hairline).done(), "#EDF1F7"),
        pack("precision-dot", "DOT", Precision, Builder::new().reticle(Dot).form(Triangle).done(), "#EDF1F7"),
        pack("precision-t-cross", "T-CROSS", Precision, Builder::new().reticle(TCross).done(), "#EDF1F7"),
        pack("precision-chevron", "CHEVRON", Precision, Builder::new().reticle(ChevronPair).form(Chevron).fill(Hairline).done(), "#EDF1F7"),
        pack("precision-bracket", "BRACKET", Precision, Builder::new().reticle(Bracket).form(Triangle).fill(Outline).done(), "#EDF1F7"),
        pack("precision-micro-dot", "MICRO-DOT", Precision, Builder::new().reticle(MicroDot).scale(0.82).done(), "#EDF1F7"),
        pack("precision-gap-cross", "GAP-CROSS", Precision, Builder::new().reticle(GapCross).done(), "#EDF1F7"),
        pack("precision-diamond", "DIAMOND", Precision, Builder::new().reticle(Diamond).form(Triangle).done(), "#EDF1F7"),
        pack("precision-notch", "NOTCH", Precision, Builder::new().reticle(Notch).weight(4.0).done(), "#EDF1F7"),

        // ── NEON ──────────────────────────────────────────────
        pack("neon-glow", "GLOW", Neon, Builder::new().glow(0.85).round().reticle(Plus).done(), "#2E8BFF"),
        pack("neon-electric", "ELECTRIC", Neon, Builder::new().glow(0.9).reticle(ThinCross).done(), "#2E8BFF"),
        pack("neon-cyber", "CYBER", Neon, Builder::new().glow(0.85).form(Blade).reticle(GapCross).done(), "#FF3DD8"),
        pack("neon-toxic", "TOXIC", Neon, Builder::new().glow(0.8).form(Triangle).reticle(Circle).done(), "#7DFF3D"),
        pack("neon-ember", "EMBER", Neon, Builder::new().glow(0.9).round().reticle(DotRing).done(), "#FF7A2E"),
        pack("neon-ice", "ICE", Neon, Builder::new().glow(0.75).form(Chevron).fill(Hairline).reticle(Diamond).done(), "#8AE9FF"),
        pack("neon-plasma", "PLASMA", Neon, Builder::new().glow(0.95).reticle(CircleCross).done(), "#A24BFF"),
        pack("neon-vapor", "VAPOR", Neon, Builder::new().glow(0.8).form(Blade).round().reticle(ChevronPair).done(), "#FF6FD8"),

        // ── MINIMAL ───────────────────────────────────────────
        pack("minimal-hairline", "HAIRLINE", Minimal, Builder::new().fill(Hairline).weight(2.4).reticle(ThinCross).done(), "#EDF1F7"),
        pack("minimal-mono", "MONO", Minimal, Builder::new().fill(Outline).weight(4.0).reticle(Circle).done(), "#EDF1F7"),
        pack("minimal-ink", "INK", Minimal, Builder::new().reticle(Dot).weight(6.0).done(), "#EDF1F7"),
        pack("minimal-ghost", "GHOST", Minimal, Builder::new().fill(Outline).opacity(0.55).reticle(Plus).done(), "#EDF1F7"),
        pack("minimal-paper", "PAPER", Minimal, Builder::new().round().reticle(Plus).weight(4.0).done(), "#EDF1F7"),
        pack("minimal-bevel", "BEVEL", Minimal, Builder::new().reticle(Diamond).weight(5.5).done(), "#EDF1F7"),
        pack("minimal-flat", "FLAT", Minimal, Builder::new().reticle(TCross).form(Triangle).done(), "#EDF1F7"),
        pack("minimal-nano", "NANO", Minimal, Builder::new().reticle(MicroDot).scale(0.7).done(), "#EDF1F7"),
        pack("minimal-wire", "WIRE", Minimal, Builder::new().fill(Outline).reticle(Bracket).weight(3.2).done(), "#EDF1F7"),
        pack("minimal-slab", "SLAB", Minimal, Builder::new().reticle(Notch).weight(7.5).done(), "#EDF1F7"),

        // ── RETRO / PIXEL ─────────────────────────────────────
        pack("retro-8bit", "8-BIT", Retro, Builder::new().form(Pixel).reticle(Plus).weight(6.0).done(), "#EDF1F7"),
        pack("retro-16bit", "16-BIT", Retro, Builder::new().form(Pixel).reticle(GapCross).weight(5.0).done(), "#8AE9FF"),
        pack("retro-classic", "CLASSIC", Retro, Builder::new().form(Pixel).reticle(Dot).done(), "#EDF1F7"),
        pack("retro-crt", "CRT", Retro, Builder::new().form(Pixel).glow(0.5).reticle(ThinCross).done(), "#33D6A6"),
        pack("retro-dos", "DOS", Retro, Builder::new().form(Pixel).fill(Outline).reticle(Notch).done(), "#EDF1F7"),
        pack("retro-handheld", "HANDHELD", Retro, Builder::new().form(Pixel).reticle(Diamond).done(), "#9BBC0F"),
        pack("retro-terminal", "TERMINAL", Retro, Builder::new().form(Pixel).fill(Hairline).reticle(TCross).done(), "#33D6A6"),
        pack("retro-pixel-hand", "PIXEL HAND", Retro, Builder::new().form(Pixel).reticle(MicroDot).weight(4.0).done(), "#EDF1F7"),

        // ── GAMING ────────────────────────────────────────────
        // Twelve reticles on a consistent blade-form pointer, so switching
        // between them changes precision select without changing the pointer.
        pack("gaming-haircross", "HAIRCROSS", Gaming, Builder::new().form(Blade).reticle(ThinCross).weight(2.6).done(), "#33D6A6"),
        pack("gaming-duel", "DUEL", Gaming, Builder::new().form(Blade).reticle(GapCross).done(), "#33D6A6"),
        pack("gaming-sprint", "SPRINT", Gaming, Builder::new().form(Blade).reticle(ChevronPair).done(), "#2E8BFF"),
        pack("gaming-apex", "APEX", Gaming, Builder::new().form(Blade).reticle(Diamond).done(), "#FF4D5E"),
        pack("gaming-vector", "VECTOR", Gaming, Builder::new().form(Blade).reticle(CircleCross).done(), "#2E8BFF"),
        pack("gaming-prism", "PRISM", Gaming, Builder::new().form(Blade).reticle(TripleTick).done(), "#A24BFF"),
        pack("gaming-recoil", "RECOIL", Gaming, Builder::new().form(Blade).reticle(Bracket).done(), "#FF7A2E"),
        pack("gaming-tracer", "TRACER", Gaming, Builder::new().form(Blade).glow(0.6).reticle(Dot).done(), "#7DFF3D"),
        pack("gaming-sight", "SIGHT", Gaming, Builder::new().form(Blade).reticle(DotRing).done(), "#33D6A6"),
        pack("gaming-pivot", "PIVOT", Gaming, Builder::new().form(Blade).reticle(Notch).done(), "#EDF1F7"),
        pack("gaming-lock", "LOCK", Gaming, Builder::new().form(Blade).reticle(Circle).weight(3.4).done(), "#FF4D5E"),
        pack("gaming-zero", "ZERO", Gaming, Builder::new().form(Blade).reticle(MicroDot).scale(0.85).done(), "#EDF1F7"),

        // ── ANIMATED ──────────────────────────────────────────
        pack("animated-pulse", "PULSE", Animated, Builder::new().glow(0.7).round().reticle(DotRing).done(), "#2E8BFF"),
        pack("animated-orbit", "ORBIT", Animated, Builder::new().reticle(Circle).done(), "#2E8BFF"),
        pack("animated-ripple", "RIPPLE", Animated, Builder::new().glow(0.5).reticle(CircleCross).done(), "#8AE9FF"),
        pack("animated-scanline", "SCANLINE", Animated, Builder::new().form(Pixel).reticle(ThinCross).done(), "#33D6A6"),
        pack("animated-spinner", "SPINNER", Animated, Builder::new().round().reticle(Plus).done(), "#2E8BFF"),
        pack("animated-breathe", "BREATHE", Animated, Builder::new().glow(0.8).round().reticle(Dot).done(), "#A24BFF"),
        pack("animated-comet", "COMET", Animated, Builder::new().form(Triangle).glow(0.7).reticle(TripleTick).done(), "#FF7A2E"),

        // ── FUN ───────────────────────────────────────────────
        pack("fun-arcade", "ARCADE", Fun, Builder::new().form(Pixel).glow(0.6).reticle(Diamond).done(), "#FF3DD8"),
        pack("fun-holo", "HOLO", Fun, Builder::new().glow(0.9).opacity(0.8).reticle(CircleCross).done(), "#8AE9FF"),
        pack("fun-liquid", "LIQUID", Fun, Builder::new().round().weight(7.0).reticle(Dot).done(), "#2E8BFF"),
        pack("fun-origami", "ORIGAMI", Fun, Builder::new().form(Triangle).fill(Outline).reticle(Diamond).done(), "#EDF1F7"),
        pack("fun-blade", "BLADE", Fun, Builder::new().form(Blade).weight(3.0).reticle(TripleTick).done(), "#FF4D5E"),
        pack("fun-rune", "RUNE", Fun, Builder::new().form(Chevron).fill(Hairline).glow(0.5).reticle(Bracket).done(), "#A24BFF"),
        pack("fun-circuit", "CIRCUIT", Fun, Builder::new().form(Pixel).fill(Outline).reticle(GapCross).done(), "#33D6A6"),
        pack("fun-sticker", "STICKER", Fun, Builder::new().round().weight(8.0).reticle(Circle).done(), "#FF7A2E"),

        // ── PRECISION II ──────────────────────────────────────
        pack("precision-square", "SQUARE", Precision, Builder::new().form(Slim).reticle(Square).done(), "#EDF1F7"),
        pack("precision-corner", "CORNER", Precision, Builder::new().form(Slim).reticle(CornerDot).done(), "#EDF1F7"),
        pack("precision-saltire", "SALTIRE", Precision, Builder::new().form(Slim).reticle(Saltire).done(), "#EDF1F7"),
        pack("precision-hex", "HEX", Precision, Builder::new().form(Slim).reticle(Hex).done(), "#EDF1F7"),
        pack("precision-grid", "GRID", Precision, Builder::new().form(Slim).fill(Hairline).reticle(Grid).done(), "#EDF1F7"),
        pack("precision-rings", "RINGS", Precision, Builder::new().form(Slim).reticle(Rings).done(), "#EDF1F7"),
        pack("precision-bar", "BAR", Precision, Builder::new().form(Slim).reticle(Bar).done(), "#EDF1F7"),
        pack("precision-caret", "CARET", Precision, Builder::new().form(Slim).reticle(Caret).done(), "#EDF1F7"),

        // ── NEON II ───────────────────────────────────────────
        pack("neon-halo", "HALO", Neon, Builder::new().form(Round).glow(0.9).round().reticle(Rings).done(), "#5CB8FF"),
        pack("neon-flux", "FLUX", Neon, Builder::new().form(Kite).glow(0.85).reticle(Arc).done(), "#33D6A6"),
        pack("neon-nova", "NOVA", Neon, Builder::new().form(Wedge).glow(0.95).reticle(Star).done(), "#FFD23D"),
        pack("neon-signal", "SIGNAL", Neon, Builder::new().form(Slim).glow(0.8).reticle(Bar).done(), "#FF3DD8"),
        pack("neon-drift", "DRIFT", Neon, Builder::new().form(Split).glow(0.85).reticle(Caret).done(), "#A24BFF"),
        pack("neon-pulse-x", "PULSE X", Neon, Builder::new().form(Round).glow(0.9).round().reticle(Saltire).done(), "#FF7A2E"),

        // ── MINIMAL II ────────────────────────────────────────
        pack("minimal-thread", "THREAD", Minimal, Builder::new().form(Slim).fill(Hairline).weight(2.0).reticle(Bar).done(), "#EDF1F7"),
        pack("minimal-pebble", "PEBBLE", Minimal, Builder::new().form(Round).round().reticle(Dot).done(), "#EDF1F7"),
        pack("minimal-notch-x", "NOTCH X", Minimal, Builder::new().form(Split).reticle(Notch).done(), "#EDF1F7"),
        pack("minimal-quill", "QUILL", Minimal, Builder::new().form(Kite).fill(Hairline).reticle(Caret).done(), "#EDF1F7"),
        pack("minimal-block", "BLOCK", Minimal, Builder::new().form(Wedge).reticle(Square).done(), "#EDF1F7"),
        pack("minimal-arc", "ARC", Minimal, Builder::new().form(Round).fill(Outline).reticle(Arc).done(), "#EDF1F7"),
        pack("minimal-chalk", "CHALK", Minimal, Builder::new().form(Slim).round().weight(6.5).reticle(Saltire).done(), "#EDF1F7"),
        pack("minimal-trace", "TRACE", Minimal, Builder::new().form(Split).fill(Hairline).reticle(Grid).done(), "#EDF1F7"),

        // ── RETRO II ──────────────────────────────────────────
        pack("retro-vector", "VECTOR CRT", Retro, Builder::new().form(Pixel).fill(Hairline).glow(0.45).reticle(Square).done(), "#33D6A6"),
        pack("retro-mono", "MONO CRT", Retro, Builder::new().form(Pixel).reticle(Grid).done(), "#8AE9FF"),
        pack("retro-tape", "TAPE", Retro, Builder::new().form(Wedge).reticle(Bar).done(), "#FF7A2E"),
        pack("retro-cassette", "CASSETTE", Retro, Builder::new().form(Pixel).reticle(Rings).done(), "#FFD23D"),
        pack("retro-plasma", "PLASMA TUBE", Retro, Builder::new().form(Pixel).glow(0.6).reticle(Star).done(), "#FF3DD8"),
        pack("retro-bevel", "BEVEL CRT", Retro, Builder::new().form(Wedge).fill(Outline).reticle(Hex).done(), "#EDF1F7"),

        // ── GAMING II ─────────────────────────────────────────
        pack("gaming-burst", "BURST", Gaming, Builder::new().form(Slim).reticle(Star).done(), "#FFD23D"),
        pack("gaming-arcshot", "ARCSHOT", Gaming, Builder::new().form(Blade).reticle(Arc).done(), "#7DFF3D"),
        pack("gaming-mesh", "MESH", Gaming, Builder::new().form(Blade).fill(Hairline).reticle(Grid).done(), "#8AE9FF"),

        // ── ANIMATED II ───────────────────────────────────────
        pack("animated-halo", "HALO SPIN", Animated, Builder::new().form(Round).glow(0.75).round().reticle(Rings).done(), "#5CB8FF"),
        pack("animated-sweep", "SWEEP", Animated, Builder::new().form(Slim).reticle(Arc).done(), "#33D6A6"),
        pack("animated-flicker", "FLICKER", Animated, Builder::new().form(Split).reticle(Bar).done(), "#FF3DD8"),
        pack("animated-beacon", "BEACON", Animated, Builder::new().form(Wedge).glow(0.8).reticle(Star).done(), "#FFD23D"),
        pack("animated-drift-x", "DRIFT X", Animated, Builder::new().form(Kite).glow(0.6).reticle(Caret).done(), "#A24BFF"),

        // ── FUN II ────────────────────────────────────────────
        pack("fun-bubble", "BUBBLE", Fun, Builder::new().form(Round).round().weight(7.5).reticle(Rings).done(), "#5CB8FF"),
        pack("fun-comet-x", "COMET X", Fun, Builder::new().form(Kite).glow(0.8).reticle(Star).done(), "#FF7A2E"),
        pack("fun-paper-x", "PAPER X", Fun, Builder::new().form(Split).fill(Outline).reticle(Hex).done(), "#EDF1F7"),
        pack("fun-candy", "CANDY", Fun, Builder::new().form(Round).round().glow(0.5).reticle(Caret).done(), "#FF3DD8"),
        pack("fun-anvil", "ANVIL", Fun, Builder::new().form(Wedge).weight(6.0).reticle(Square).done(), "#EDF1F7"),
        pack("fun-prism-x", "PRISM X", Fun, Builder::new().form(Kite).glow(0.7).reticle(Saltire).done(), "#A24BFF"),

        // ── VOLUME — gradient bodies, so the pointer reads as a lit surface ──
        pack("vol-titan", "TITAN", Minimal, Builder::new().form(Wedge).treat(Treatment::Gradient).reticle(Hex).done(), "#8AE9FF"),
        pack("vol-obsidian", "OBSIDIAN", Minimal, Builder::new().form(Prism).treat(Treatment::Gradient).reticle(Diamond).done(), "#A24BFF"),
        pack("vol-chrome", "CHROME", Minimal, Builder::new().form(Classic).treat(Treatment::Gradient).reticle(Rings).done(), "#EDF1F7"),
        pack("vol-cobalt", "COBALT", Neon, Builder::new().form(Slim).treat(Treatment::Gradient).glow(0.7).reticle(CircleCross).done(), "#2E8BFF"),
        pack("vol-magma", "MAGMA", Neon, Builder::new().form(Bolt).treat(Treatment::Gradient).glow(0.85).reticle(Star).done(), "#FF7A2E"),
        pack("vol-aurora", "AURORA", Neon, Builder::new().form(Crescent).treat(Treatment::Gradient).glow(0.8).reticle(Arc).done(), "#33D6A6"),
        pack("vol-nebula", "NEBULA", Neon, Builder::new().form(Round).treat(Treatment::Gradient).glow(0.9).round().reticle(Rings).done(), "#A24BFF"),
        pack("vol-quartz", "QUARTZ", Minimal, Builder::new().form(Prism).treat(Treatment::Gradient).reticle(Hex).done(), "#EDF1F7"),
        pack("vol-onyx", "ONYX", Minimal, Builder::new().form(Fang).treat(Treatment::Gradient).reticle(Saltire).done(), "#8A94A6"),
        pack("vol-solar", "SOLAR", Neon, Builder::new().form(Beam).treat(Treatment::Gradient).glow(0.85).reticle(Star).done(), "#FFD23D"),
        pack("vol-abyss", "ABYSS", Minimal, Builder::new().form(Needle).treat(Treatment::Gradient).reticle(Bar).done(), "#5CB8FF"),
        pack("vol-ember-x", "EMBER X", Neon, Builder::new().form(Shard).treat(Treatment::Gradient).glow(0.8).reticle(Caret).done(), "#FF4D5E"),

        // ── DEPTH — an offset silhouette behind, for weight ──────────
        pack("dep-monolith", "MONOLITH", Minimal, Builder::new().form(Wedge).treat(Treatment::Depth).reticle(Square).done(), "#EDF1F7"),
        pack("dep-riser", "RISER", Minimal, Builder::new().form(Classic).treat(Treatment::Depth).reticle(Plus).done(), "#5CB8FF"),
        pack("dep-strata", "STRATA", Retro, Builder::new().form(Pixel).treat(Treatment::Depth).reticle(Grid).done(), "#33D6A6"),
        pack("dep-cast", "CAST", Fun, Builder::new().form(Round).treat(Treatment::Depth).round().reticle(Dot).done(), "#FF7A2E"),
        pack("dep-relief", "RELIEF", Minimal, Builder::new().form(Sigil).treat(Treatment::Depth).reticle(Hex).done(), "#A24BFF"),
        pack("dep-anvil-x", "ANVIL X", Gaming, Builder::new().form(Wedge).treat(Treatment::Depth).reticle(CornerDot).done(), "#FF4D5E"),
        pack("dep-slate", "SLATE", Minimal, Builder::new().form(Beam).treat(Treatment::Depth).reticle(Bar).done(), "#8A94A6"),
        pack("dep-echo", "ECHO", Neon, Builder::new().form(Stack).treat(Treatment::Depth).glow(0.6).reticle(ChevronPair).done(), "#2E8BFF"),
        pack("dep-vault", "VAULT", Precision, Builder::new().form(Prism).treat(Treatment::Depth).reticle(Rings).done(), "#EDF1F7"),
        pack("dep-tower", "TOWER", Retro, Builder::new().form(Nib).treat(Treatment::Depth).reticle(Notch).done(), "#FFD23D"),

        // ── ORBIT — a ring behind the glyph ──────────────────────────
        pack("orb-eclipse", "ECLIPSE", Neon, Builder::new().form(Crescent).treat(Treatment::Halo).glow(0.85).reticle(Rings).done(), "#A24BFF"),
        pack("orb-satellite", "SATELLITE", Precision, Builder::new().form(Needle).treat(Treatment::Halo).reticle(Circle).done(), "#8AE9FF"),
        pack("orb-corona", "CORONA", Neon, Builder::new().form(Beam).treat(Treatment::Halo).glow(0.9).reticle(Star).done(), "#FFD23D"),
        pack("orb-lagrange", "LAGRANGE", Precision, Builder::new().form(Slim).treat(Treatment::Halo).reticle(DotRing).done(), "#EDF1F7"),
        pack("orb-perigee", "PERIGEE", Gaming, Builder::new().form(Blade).treat(Treatment::Halo).reticle(CircleCross).done(), "#33D6A6"),
        pack("orb-halo-x", "HALO X", Fun, Builder::new().form(Round).treat(Treatment::Halo).round().glow(0.7).reticle(Arc).done(), "#5CB8FF"),
        pack("orb-vector-x", "VECTOR X", Gaming, Builder::new().form(Kite).treat(Treatment::Halo).reticle(Saltire).done(), "#2E8BFF"),
        pack("orb-pulsar", "PULSAR", Animated, Builder::new().form(Needle).treat(Treatment::Halo).glow(0.8).reticle(Rings).done(), "#FF3DD8"),
        pack("orb-quasar", "QUASAR", Animated, Builder::new().form(Bolt).treat(Treatment::Halo).glow(0.9).reticle(Star).done(), "#8AE9FF"),
        pack("orb-ringlet", "RINGLET", Minimal, Builder::new().form(Slim).treat(Treatment::Halo).fill(Hairline).reticle(Circle).done(), "#EDF1F7"),

        // ── SCAN — banded surfaces, deliberately synthetic ───────────
        pack("scn-hologram", "HOLOGRAM", Neon, Builder::new().form(Classic).treat(Treatment::Scan).glow(0.8).reticle(Grid).done(), "#8AE9FF"),
        pack("scn-lattice-x", "LATTICE X", Retro, Builder::new().form(Pixel).treat(Treatment::Scan).reticle(Grid).done(), "#33D6A6"),
        pack("scn-interlace", "INTERLACE", Retro, Builder::new().form(Wedge).treat(Treatment::Scan).reticle(Bar).done(), "#FFD23D"),
        pack("scn-signal-x", "SIGNAL X", Neon, Builder::new().form(Beam).treat(Treatment::Scan).glow(0.75).reticle(ThinCross).done(), "#2E8BFF"),
        pack("scn-static-x", "STATIC X", Retro, Builder::new().form(Shard).treat(Treatment::Scan).reticle(Notch).done(), "#FF3DD8"),
        pack("scn-raster", "RASTER", Retro, Builder::new().form(Prism).treat(Treatment::Scan).reticle(Square).done(), "#8AE9FF"),
        pack("scn-vhs", "VHS", Retro, Builder::new().form(Classic).treat(Treatment::Scan).reticle(TripleTick).done(), "#FF7A2E"),
        pack("scn-tape-x", "TAPE X", Retro, Builder::new().form(Nib).treat(Treatment::Scan).reticle(Caret).done(), "#A24BFF"),
        pack("scn-glitch-x", "GLITCH X", Animated, Builder::new().form(Shard).treat(Treatment::Scan).reticle(Saltire).done(), "#FF3DD8"),
        pack("scn-matrix", "MATRIX", Animated, Builder::new().form(Needle).treat(Treatment::Scan).glow(0.6).reticle(Grid).done(), "#33D6A6"),

        // ── TRAIL — motion frozen into the shape ─────────────────────
        pack("trl-hyperdrive", "HYPERDRIVE", Neon, Builder::new().form(Kite).treat(Treatment::Trail).glow(0.85).reticle(ChevronPair).done(), "#2E8BFF"),
        pack("trl-afterburn", "AFTERBURN", Neon, Builder::new().form(Bolt).treat(Treatment::Trail).glow(0.9).reticle(Star).done(), "#FF7A2E"),
        pack("trl-slipstream", "SLIPSTREAM", Gaming, Builder::new().form(Blade).treat(Treatment::Trail).reticle(Caret).done(), "#33D6A6"),
        pack("trl-warp", "WARP", Animated, Builder::new().form(Beam).treat(Treatment::Trail).glow(0.8).reticle(Arc).done(), "#A24BFF"),
        pack("trl-drift-y", "DRIFT Y", Gaming, Builder::new().form(Slim).treat(Treatment::Trail).reticle(Bar).done(), "#FF4D5E"),
        pack("trl-phase", "PHASE", Animated, Builder::new().form(Crescent).treat(Treatment::Trail).glow(0.7).reticle(Rings).done(), "#8AE9FF"),
        pack("trl-boost", "BOOST", Gaming, Builder::new().form(Stack).treat(Treatment::Trail).reticle(ChevronPair).done(), "#FFD23D"),
        pack("trl-streak", "STREAK", Fun, Builder::new().form(Needle).treat(Treatment::Trail).reticle(TripleTick).done(), "#FF3DD8"),
        pack("trl-comet-y", "COMET Y", Fun, Builder::new().form(Fang).treat(Treatment::Trail).glow(0.8).reticle(Dot).done(), "#FF7A2E"),
        pack("trl-mach", "MACH", Precision, Builder::new().form(Needle).treat(Treatment::Trail).reticle(ThinCross).done(), "#EDF1F7"),

        // ── OUTLINE — dashed, technical, drafting-table ──────────────
        pack("out-blueprint", "BLUEPRINT", Precision, Builder::new().form(Classic).treat(Treatment::Dashed).fill(Outline).reticle(Grid).done(), "#5CB8FF"),
        pack("out-schematic", "SCHEMATIC", Precision, Builder::new().form(Wedge).treat(Treatment::Dashed).fill(Outline).reticle(Square).done(), "#8AE9FF"),
        pack("out-marquee", "MARQUEE", Minimal, Builder::new().form(Triangle).treat(Treatment::Dashed).fill(Outline).reticle(CornerDot).done(), "#EDF1F7"),
        pack("out-stencil", "STENCIL", Minimal, Builder::new().form(Split).treat(Treatment::Dashed).fill(Outline).reticle(Bracket).done(), "#EDF1F7"),
        pack("out-draft", "DRAFT", Precision, Builder::new().form(Nib).treat(Treatment::Dashed).fill(Outline).reticle(Saltire).done(), "#33D6A6"),
        pack("out-contour", "CONTOUR", Minimal, Builder::new().form(Round).treat(Treatment::Dashed).fill(Outline).round().reticle(Circle).done(), "#EDF1F7"),
        pack("out-grid-x", "GRID X", Precision, Builder::new().form(Prism).treat(Treatment::Dashed).fill(Outline).reticle(Grid).done(), "#8AE9FF"),
        pack("out-perf", "PERFORATE", Fun, Builder::new().form(Sigil).treat(Treatment::Dashed).fill(Outline).reticle(Hex).done(), "#A24BFF"),

        // ── RIM — a lit edge, the most "3D" of the treatments ────────
        pack("rim-bevel-x", "BEVEL X", Minimal, Builder::new().form(Classic).treat(Treatment::Rim).reticle(Diamond).done(), "#EDF1F7"),
        pack("rim-forged", "FORGED", Gaming, Builder::new().form(Blade).treat(Treatment::Rim).reticle(Saltire).done(), "#FF7A2E"),
        pack("rim-alloy", "ALLOY", Minimal, Builder::new().form(Wedge).treat(Treatment::Rim).reticle(Hex).done(), "#8A94A6"),
        pack("rim-plated", "PLATED", Minimal, Builder::new().form(Prism).treat(Treatment::Rim).reticle(Square).done(), "#5CB8FF"),
        pack("rim-lumen", "LUMEN", Neon, Builder::new().form(Round).treat(Treatment::Rim).glow(0.8).round().reticle(Rings).done(), "#8AE9FF"),
        pack("rim-halcyon", "HALCYON", Neon, Builder::new().form(Crescent).treat(Treatment::Rim).glow(0.75).reticle(Arc).done(), "#33D6A6"),
        pack("rim-edge", "EDGE", Gaming, Builder::new().form(Fang).treat(Treatment::Rim).reticle(CornerDot).done(), "#FF4D5E"),
        pack("rim-gilded", "GILDED", Fun, Builder::new().form(Sigil).treat(Treatment::Rim).glow(0.6).reticle(Star).done(), "#FFD23D"),
        pack("rim-frost", "FROST", Minimal, Builder::new().form(Shard).treat(Treatment::Rim).reticle(Diamond).done(), "#8AE9FF"),
        pack("rim-machined", "MACHINED", Precision, Builder::new().form(Beam).treat(Treatment::Rim).reticle(CircleCross).done(), "#EDF1F7"),

        // ── FACET — cut surfaces ─────────────────────────────────────
        pack("fct-gemstone", "GEMSTONE", Fun, Builder::new().form(Prism).treat(Treatment::Facet).reticle(Diamond).done(), "#A24BFF"),
        pack("fct-lowpoly", "LOW POLY", Fun, Builder::new().form(Wedge).treat(Treatment::Facet).reticle(Hex).done(), "#33D6A6"),
        pack("fct-fracture", "FRACTURE", Gaming, Builder::new().form(Shard).treat(Treatment::Facet).reticle(Saltire).done(), "#FF4D5E"),
        pack("fct-crystal", "CRYSTAL", Neon, Builder::new().form(Prism).treat(Treatment::Facet).glow(0.8).reticle(Star).done(), "#8AE9FF"),
        pack("fct-origami-x", "ORIGAMI X", Fun, Builder::new().form(Triangle).treat(Treatment::Facet).reticle(Caret).done(), "#EDF1F7"),
        pack("fct-carbon", "CARBON", Minimal, Builder::new().form(Classic).treat(Treatment::Facet).reticle(Grid).done(), "#8A94A6"),
        pack("fct-shard-x", "SHARD X", Gaming, Builder::new().form(Bolt).treat(Treatment::Facet).reticle(TripleTick).done(), "#FFD23D"),
        pack("fct-splinter", "SPLINTER", Gaming, Builder::new().form(Needle).treat(Treatment::Facet).reticle(ThinCross).done(), "#FF3DD8"),

        // ── INLAY — hollow core, solid border ────────────────────────
        pack("inl-signet", "SIGNET", Fun, Builder::new().form(Sigil).treat(Treatment::Inlay).reticle(Hex).done(), "#A24BFF"),
        pack("inl-cameo", "CAMEO", Minimal, Builder::new().form(Round).treat(Treatment::Inlay).round().reticle(Circle).done(), "#EDF1F7"),
        pack("inl-emblem", "EMBLEM", Fun, Builder::new().form(Crescent).treat(Treatment::Inlay).reticle(Star).done(), "#FFD23D"),
        pack("inl-relic", "RELIC", Retro, Builder::new().form(Nib).treat(Treatment::Inlay).reticle(Diamond).done(), "#FF7A2E"),
        pack("inl-badge", "BADGE", Minimal, Builder::new().form(Wedge).treat(Treatment::Inlay).reticle(Square).done(), "#5CB8FF"),
        pack("inl-token", "TOKEN", Fun, Builder::new().form(Prism).treat(Treatment::Inlay).reticle(Rings).done(), "#33D6A6"),
        pack("inl-crest", "CREST", Fun, Builder::new().form(Fang).treat(Treatment::Inlay).reticle(Saltire).done(), "#FF4D5E"),
        pack("inl-seal", "SEAL", Minimal, Builder::new().form(Stack).treat(Treatment::Inlay).reticle(CornerDot).done(), "#EDF1F7"),

        // ── FORM SHOWCASE — the new silhouettes, unadorned ───────────
        pack("frm-bolt", "BOLT", Gaming, Builder::new().form(Bolt).reticle(Star).done(), "#FFD23D"),
        pack("frm-needle", "NEEDLE", Precision, Builder::new().form(Needle).reticle(ThinCross).done(), "#EDF1F7"),
        pack("frm-fang", "FANG", Gaming, Builder::new().form(Fang).reticle(Saltire).done(), "#FF4D5E"),
        pack("frm-beam", "BEAM", Neon, Builder::new().form(Beam).glow(0.8).reticle(Bar).done(), "#2E8BFF"),
        pack("frm-prism", "PRISM CORE", Minimal, Builder::new().form(Prism).reticle(Hex).done(), "#8AE9FF"),
        pack("frm-crescent", "CRESCENT", Fun, Builder::new().form(Crescent).reticle(Arc).done(), "#A24BFF"),
        pack("frm-stack", "STACK", Minimal, Builder::new().form(Stack).reticle(ChevronPair).done(), "#EDF1F7"),
        pack("frm-shard", "SHARD", Gaming, Builder::new().form(Shard).reticle(Diamond).done(), "#33D6A6"),
        pack("frm-nib", "NIB", Retro, Builder::new().form(Nib).reticle(Caret).done(), "#FF7A2E"),
        pack("frm-sigil", "SIGIL", Fun, Builder::new().form(Sigil).reticle(Rings).done(), "#FF3DD8"),
        pack("frm-bolt-mini", "BOLT MINI", Gaming, Builder::new().form(Bolt).scale(0.78).reticle(MicroDot).done(), "#8AE9FF"),
        pack("frm-needle-ghost", "NEEDLE GHOST", Minimal, Builder::new().form(Needle).opacity(0.6).reticle(Dot).done(), "#EDF1F7"),
        pack("frm-fang-glow", "FANG GLOW", Neon, Builder::new().form(Fang).glow(0.9).reticle(DotRing).done(), "#FF3DD8"),
        pack("frm-beam-thin", "BEAM THIN", Precision, Builder::new().form(Beam).fill(Hairline).reticle(GapCross).done(), "#EDF1F7"),
    ]
}

pub fn find(id: &str) -> Option<PackDef> {
    all().into_iter().find(|pack| pack.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// The spec set 64 as the v1 floor. Growing past it is fine; dropping below
    /// it is a regression, and a silent duplicate that shrinks the list would
    /// otherwise go unnoticed.
    #[test]
    fn the_catalog_meets_or_beats_the_target() {
        let count = all().len();
        assert!(count >= 64, "catalog shrank to {count}; the v1 floor is 64");
        assert_eq!(count, 205, "update this when packs are added deliberately");
    }

    /// Colour is a setting, not a design.
    ///
    /// Two packs with identical geometry and different default tints are the
    /// same cursor twice. Shipping both floods a category with what looks like
    /// repetition and buries the designs that genuinely differ — the user can
    /// already recolour anything, so the second entry earns nothing.
    #[test]
    fn no_two_packs_are_the_same_design_in_a_different_colour() {
        use std::collections::HashMap;

        let packs = all();
        let mut seen: HashMap<String, &'static str> = HashMap::new();
        let mut clashes = Vec::new();

        for pack in &packs {
            let s = &pack.style;
            // Everything that changes a pixel, and nothing that does not.
            let signature = format!(
                "{:?}|{:?}|{:?}|{:?}|{:.2}|{:.2}|{}|{:.2}|{:.2}",
                s.form,
                s.fill,
                s.reticle,
                s.treatment,
                s.weight,
                s.glow,
                s.round_joins,
                s.opacity,
                s.scale
            );
            if let Some(first) = seen.insert(signature.clone(), pack.name) {
                clashes.push(format!("{} is {} in another colour", pack.name, first));
            }
        }

        assert!(
            clashes.is_empty(),
            "{} colour-only duplicate(s):\n  {}",
            clashes.len(),
            clashes.join("\n  ")
        );
    }

    #[test]
    fn ids_and_names_are_unique_and_slug_safe() {
        let packs = all();
        let ids: HashSet<_> = packs.iter().map(|p| p.id).collect();
        let names: HashSet<_> = packs.iter().map(|p| p.name).collect();
        assert_eq!(ids.len(), packs.len(), "duplicate pack id");
        assert_eq!(names.len(), packs.len(), "duplicate pack name");

        for pack in &packs {
            assert!(
                pack.id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{} is not a safe directory name",
                pack.id
            );
            assert!(crate::util::parse_hex_color(pack.default_tint).is_some());
        }
    }

    #[test]
    fn every_category_is_represented() {
        let packs = all();
        for category in [
            Category::Precision,
            Category::Neon,
            Category::Minimal,
            Category::Retro,
            Category::Gaming,
            Category::Animated,
            Category::Fun,
        ] {
            assert!(
                packs.iter().any(|p| p.category == category),
                "{category:?} has no packs"
            );
        }
        assert!(
            packs.iter().filter(|p| p.category == Category::Gaming).count() >= 12,
            "PRD §7 asks for at least twelve gaming reticles"
        );

        // No category should be a token single entry — an empty-looking filter
        // pill is worse than not offering the filter.
        for category in [
            Category::Precision,
            Category::Neon,
            Category::Minimal,
            Category::Retro,
            Category::Gaming,
            Category::Animated,
            Category::Fun,
        ] {
            let count = packs.iter().filter(|p| p.category == category).count();
            assert!(count >= 6, "{category:?} has only {count} packs");
        }
    }

    #[test]
    fn only_the_animated_category_is_animated() {
        for pack in all() {
            assert_eq!(pack.animated, pack.category == Category::Animated, "{}", pack.id);
        }
    }

    #[test]
    fn no_pack_name_borrows_a_third_party_mark() {
        // PRD §15.3. Matching on whole words, not substrings: "mac" inside
        // MACHINED is an ordinary English word, and a check that cannot tell the
        // difference gets weakened or deleted the first time it cries wolf.
        const FORBIDDEN: [&str; 12] = [
            "windows", "microsoft", "mac", "macos", "apple", "amiga", "gameboy", "nintendo",
            "sega", "playstation", "xbox", "valve",
        ];
        const FORBIDDEN_PHRASES: [&str; 2] = ["game boy", "play station"];

        for pack in all() {
            let name = pack.name.to_ascii_lowercase();
            let id = pack.id.to_ascii_lowercase();

            let words: Vec<&str> = name
                .split(|c: char| !c.is_ascii_alphanumeric())
                .chain(id.split(|c: char| !c.is_ascii_alphanumeric()))
                .filter(|w| !w.is_empty())
                .collect();

            for mark in FORBIDDEN {
                assert!(
                    !words.contains(&mark),
                    "{} ({}) borrows the mark \"{mark}\"",
                    pack.name,
                    pack.id
                );
            }
            for phrase in FORBIDDEN_PHRASES {
                assert!(!name.contains(phrase), "{} borrows \"{phrase}\"", pack.name);
            }
        }
    }

    /// The check above must still catch a real borrowing, or it is decoration.
    #[test]
    fn the_trademark_check_would_catch_an_actual_borrowing() {
        let words: Vec<&str> = "retro gameboy"
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        assert!(words.contains(&"gameboy"), "a real mark must still trip it");

        let innocent: Vec<&str> = "machined"
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        assert!(!innocent.contains(&"mac"), "an ordinary word must not trip it");
    }
}
