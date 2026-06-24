//! A real-time [`AudioPlayback`](rameau_playback::AudioPlayback) backend built
//! on [`kira`].
//!
//! [`Kira`] owns a `kira::AudioManager` and plays each voice as a
//! `StaticSoundData`: kira does the resampling, pitch-shifting, mixing and
//! device output. Starting a voice returns a `StaticSoundHandle` that
//! `update` and `stop` drive with tweens.
//!
//! # Capability mapping
//!
//! - `pitch` → playback rate via semitones.
//! - `volume` (linear) → decibels.
//! - `position` → stereo panning (`position.x`); full 3D spatialization and
//!   `velocity`/Doppler are **degraded** away, not errored.
//! - offline render is real-time only → `PlaybackError::Unsupported`.

mod backend;

pub use backend::Kira;
