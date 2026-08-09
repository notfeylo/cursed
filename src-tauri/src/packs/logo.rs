//! Whatever the mark currently is, in the shape the review tooling wants.
//!
//! This module used to hold three candidate directions drawn from a brief. They
//! served their purpose — the contact sheets killed two of them on the 16 px
//! test — and the mark that shipped came from supplied artwork instead, traced
//! by `genpacks --trace`. Keeping the losing candidates around would leave three
//! marks in the tree and no way to tell from a filename which one is real.
//!
//! So the candidate art is gone and this is a shim. The tooling it feeds —
//! `--logo-sheet` and `--logo-zoom` — is not throwaway: any future change to the
//! mark should be judged the same way, by rasterising each size and reading the
//! pixels rather than by looking at a scaled render.

use crate::packs::brand;

/// One entry now. The sheet renderer iterates this, so a future candidate can
/// be added here without touching the tooling.
pub const DIRECTIONS: [&str; 1] = ["cursed"];

pub fn svg(_direction: &str, colour: &str) -> String {
    brand::small_mark_svg(colour)
}

/// The mark has one form at every size — it is a solid silhouette with no
/// stroke, gap or counter, so there is nothing that needs redrawing small.
pub fn svg_for_size(_direction: &str, colour: &str, _size: u32) -> String {
    brand::small_mark_svg(colour)
}

pub fn describe(_direction: &str) -> &'static str {
    "A pointer seen almost edge-on: a broad wedge with a flat base and a curled \
     tip. One solid silhouette, no interior detail, which is why it needs no \
     separate small-size drawing."
}
