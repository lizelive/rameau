//! A [`rameau_playback`] backend built on [`tinyaudio`].
//!
//! [`TinyAudio`] implements [`Playback`](rameau_playback::Playback), opening a
//! `tinyaudio` output device that drives the supplied callback on its own audio
//! thread.
//!
//! Buffers below [`min_frames_per_buffer`] are rejected. A device asked for a
//! shorter callback period than the platform can schedule does not fail — it
//! starves, producing a fraction of real-time audio — so the check happens here
//! instead.

mod backend;

pub use backend::{Stream, TinyAudio, min_frames_per_buffer};
