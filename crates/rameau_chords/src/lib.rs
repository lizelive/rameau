//! Chords, roman numerals and the harmonic grammar of the long eighteenth
//! century.
//!
//! A [`RomanNumeral`] is a chord relative to a key — degree, quality,
//! inversion — and only becomes pitch classes when realised against a
//! [`Scale`](rameau_theory::Scale). [`Grammar`] knows which chord tends to
//! follow which in the style of Lully, Rameau and Grétry, knows the stock
//! ground-bass progressions (folia, romanesca, passamezzo, the lament
//! tetrachord) and can write a phrase that ends in the [`Cadence`] you ask for.

#![forbid(unsafe_code)]

pub mod chord;
pub mod grammar;

pub use chord::{Cadence, Quality, RomanNumeral};
pub use grammar::{Grammar, Progression, StockProgression};
