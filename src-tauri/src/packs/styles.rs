//! The 64 catalog packs.
//!
//! Each pack is a parameter set, not a folder of bitmaps. That is the whole
//! trick behind PRD §7.1: the artwork is vector code, the colour is applied at
//! apply time, and a catalog that looks enormous costs the installer nothing.
//!
//! Names are deliberately generic. No pack is named after a platform, console,
//! game or company — PRD §15.3 rules out third-party marks, and "the retro one"
//! does not need somebody else's trademark to read as retro.

use crate::packs::art::{Fill, Form, Reticle, Style};
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
        pack("animated-glitch", "GLITCH", Animated, Builder::new().form(Blade).reticle(Notch).done(), "#FF3DD8"),
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
        pack("gaming-scope", "SCOPE", Gaming, Builder::new().form(Slim).reticle(Rings).done(), "#33D6A6"),
        pack("gaming-mark", "MARK", Gaming, Builder::new().form(Slim).reticle(Square).done(), "#2E8BFF"),
        pack("gaming-flick", "FLICK", Gaming, Builder::new().form(Slim).reticle(Saltire).done(), "#FF4D5E"),
        pack("gaming-burst", "BURST", Gaming, Builder::new().form(Slim).reticle(Star).done(), "#FFD23D"),
        pack("gaming-strafe", "STRAFE", Gaming, Builder::new().form(Slim).reticle(Caret).done(), "#A24BFF"),
        pack("gaming-hold", "HOLD", Gaming, Builder::new().form(Slim).reticle(CornerDot).done(), "#33D6A6"),
        pack("gaming-lane", "LANE", Gaming, Builder::new().form(Slim).reticle(Bar).done(), "#2E8BFF"),
        pack("gaming-hexlock", "HEXLOCK", Gaming, Builder::new().form(Slim).reticle(Hex).done(), "#FF7A2E"),
        pack("gaming-arcshot", "ARCSHOT", Gaming, Builder::new().form(Blade).reticle(Arc).done(), "#7DFF3D"),
        pack("gaming-mesh", "MESH", Gaming, Builder::new().form(Blade).fill(Hairline).reticle(Grid).done(), "#8AE9FF"),

        // ── ANIMATED II ───────────────────────────────────────
        pack("animated-halo", "HALO SPIN", Animated, Builder::new().form(Round).glow(0.75).round().reticle(Rings).done(), "#5CB8FF"),
        pack("animated-sweep", "SWEEP", Animated, Builder::new().form(Slim).reticle(Arc).done(), "#33D6A6"),
        pack("animated-flicker", "FLICKER", Animated, Builder::new().form(Split).reticle(Bar).done(), "#FF3DD8"),
        pack("animated-beacon", "BEACON", Animated, Builder::new().form(Wedge).glow(0.8).reticle(Star).done(), "#FFD23D"),
        pack("animated-drift-x", "DRIFT X", Animated, Builder::new().form(Kite).glow(0.6).reticle(Caret).done(), "#A24BFF"),
        pack("animated-lattice", "LATTICE", Animated, Builder::new().form(Pixel).reticle(Grid).done(), "#8AE9FF"),

        // ── FUN II ────────────────────────────────────────────
        pack("fun-bubble", "BUBBLE", Fun, Builder::new().form(Round).round().weight(7.5).reticle(Rings).done(), "#5CB8FF"),
        pack("fun-comet-x", "COMET X", Fun, Builder::new().form(Kite).glow(0.8).reticle(Star).done(), "#FF7A2E"),
        pack("fun-paper-x", "PAPER X", Fun, Builder::new().form(Split).fill(Outline).reticle(Hex).done(), "#EDF1F7"),
        pack("fun-candy", "CANDY", Fun, Builder::new().form(Round).round().glow(0.5).reticle(Caret).done(), "#FF3DD8"),
        pack("fun-anvil", "ANVIL", Fun, Builder::new().form(Wedge).weight(6.0).reticle(Square).done(), "#EDF1F7"),
        pack("fun-prism-x", "PRISM X", Fun, Builder::new().form(Kite).glow(0.7).reticle(Saltire).done(), "#A24BFF"),
        pack("fun-loop", "LOOP", Fun, Builder::new().form(Round).fill(Outline).reticle(Arc).done(), "#33D6A6"),
        pack("fun-static", "STATIC", Fun, Builder::new().form(Pixel).reticle(Grid).done(), "#8AE9FF"),
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
        assert_eq!(count, 116, "update this when packs are added deliberately");
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
        // PRD §15.3. Cheap to check, and exactly the kind of thing that slips in
        // during a rename.
        const FORBIDDEN: [&str; 10] = [
            "windows", "microsoft", "mac", "apple", "amiga", "gameboy", "game boy", "nintendo",
            "sega", "playstation",
        ];
        for pack in all() {
            let name = pack.name.to_ascii_lowercase();
            let id = pack.id.to_ascii_lowercase();
            for mark in FORBIDDEN {
                assert!(!name.contains(mark), "{} borrows {mark}", pack.name);
                assert!(!id.contains(mark), "{} borrows {mark}", pack.id);
            }
        }
    }
}
