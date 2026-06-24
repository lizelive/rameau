//! A [`rameau_playback`] backend built on [`tinyaudio`].
//!
//! [`TinyAudio`] implements [`Playback`](rameau_playback::Playback), opening a
//! `tinyaudio` output device that drives the supplied callback on its own audio
//! thread.

mod backend;

pub use backend::{Stream, TinyAudio};
