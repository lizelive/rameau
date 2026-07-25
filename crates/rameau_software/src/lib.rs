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
//!
//! The mixer's output stage gives the summed voices headroom and runs them
//! through a brickwall [`Limiter`], and a voice cap steals the oldest sounding
//! voice rather than letting held notes accumulate without limit. Both are on by
//! default; `Software::with_limiter(None)` restores a raw linear sum.

#![allow(
    clippy::std_instead_of_alloc,
    reason = "this backend targets std. Naming allocation types through \
              `alloc` would buy no portability and would cost an \
              `extern crate alloc` declaration to do it"
)]

mod backend;
mod envelope;
mod limiter;
mod voice;

pub use backend::{Handle, Software};
pub use limiter::Limiter;
