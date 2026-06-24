//! A backend-independent trait for real-time audio playback.
//!
//! [`Playback`] abstracts over "hand an output device a callback that fills
//! buffers of samples on demand". Backends such as `rameau_tinyaudio`
//! implement it so that the rest of the workspace can render audio without
//! depending on any particular audio API.
//!
//! The callback model is deliberately simple: the backend calls the supplied
//! closure on its own audio thread whenever it needs more samples, passing an
//! interleaved buffer of `f32` to fill. The closure must be `Send` and
//! `'static` because it typically runs on a thread owned by the backend.
//!
//! On top of this device layer, [`AudioPlayback`] is a higher-level *sample
//! engine* abstraction (start/update/stop/render of voices); see its module
//! documentation.

mod device;
mod engine;

pub use device::{Playback, PlaybackConfig};
pub use engine::{AudioPlayback, LoopRegion, PlaybackError, Timestamp, Vec3, VoiceParams};
