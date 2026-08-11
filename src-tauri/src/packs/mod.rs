//! Parametric artwork, and the renderer that turns it into real cursor files.
//!
//! The catalog proper is [`crate::bundled`] — 36 hand-made packs. What is
//! defined here in code is the single blend base ([`styles`]) that fills the
//! roles an imported pack leaves unmapped, the role artwork it is drawn from
//! ([`art`]), and the mark ([`brand`], [`logo`]).

pub mod art;
pub mod brand;
pub mod catalog;
pub mod cfpack;
pub mod logo;
pub mod styles;
