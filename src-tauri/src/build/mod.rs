//! Everything that turns pixels or vectors into a real Windows cursor file.
//!
//! The writers here are hand-rolled against the published byte layouts rather
//! than delegated to a crate, because the available ones do not carry a hotspot
//! per resolution — which is the one thing a multi-resolution cursor needs most.

pub mod ani_writer;
pub mod bitmap;
pub mod cur_reader;
pub mod cur_writer;
pub mod hotspot;
pub mod icon_reader;
pub mod matte;
pub mod pipeline;
pub mod svg;
