//! A pure-software [`AudioPlayback`](rameau_playback::AudioPlayback) backend.
//!
//! [`Software`] is a small polyphonic sample mixer: it keeps a pool of voices,
//! each playing one clip with linear-interpolated resampling, looping, panning
//! and an attack/release envelope, and mixes them into an interleaved-stereo
//! `f32` buffer in `render`.
//!
//! Unlike a real-time engine it does not own an output device — it just fills a
//! buffer on demand, which makes it the natural backend for **offline
//! rendering**: start every note at its scheduled `Timestamp::AtSeconds`, then
//! call `render` once over the whole piece. It can equally drive a live device
//! by being fed small blocks from an audio callback (e.g. `rameau_tinyaudio`).
//!
//! Per-voice spatial parameters degrade gracefully: a `Vec3` position collapses
//! to a stereo pan (its `x`), and velocity (Doppler) is ignored.

mod backend;
mod envelope;
mod voice;

pub use backend::{Handle, Software};
