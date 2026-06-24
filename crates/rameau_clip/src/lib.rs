//! A minimal, format-independent representation of a block of audio samples.
//!
//! The [`AudioClip`] trait abstracts over "a contiguous run of PCM samples at a
//! known sample rate". [`Clip`] is the simple owned container that implements
//! it, but other crates can implement [`AudioClip`] for their own types so that
//! tools such as a WAV writer can consume them without copying.

mod clip;

pub use clip::{AudioClip, Clip};
