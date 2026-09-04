//! Shared core types for the rameau workspace.
//!
//! Today that is the deterministic random source the composing crates share:
//! a [`SplitMix64`] generator behind the [`Rng`] trait, plus weighted
//! selection. Keeping it here (rather than pulling in a randomness crate)
//! keeps every composed bar reproducible from a seed across platforms.

#![forbid(unsafe_code)]

pub mod rng;

pub use rng::{Rng, SplitMix64};
