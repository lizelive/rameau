//! A backend-independent *sample-playback engine*.
//!
//! Where [`Playback`](crate::Playback) is the low-level "hand a device a fill
//! callback" layer, [`AudioPlayback`] is the high-level voice engine the rest of
//! the workspace drives. A backend owns clips (decoded audio) and *playbacks*
//! (sounding voices); callers start a clip with pitch/volume/spatial parameters,
//! update those parameters over time, and stop the voice — without caring whether
//! the mixing happens in a real-time engine (e.g. `rameau_kira`) or in software.
//!
//! # Graceful degradation
//!
//! Not every backend supports every capability. Methods return
//! [`PlaybackError::Unsupported`] when a whole feature is missing (e.g. offline
//! [`render`](AudioPlayback::render) on a real-time backend, or
//! [`clip_from_vorbis`](AudioPlayback::clip_from_vorbis) when no Vorbis decoder is
//! present) so callers can fall back. Per-voice *parameters* degrade silently
//! instead of erroring: a backend without spatialization collapses
//! [`VoiceParams::position`] to a stereo pan and ignores
//! [`VoiceParams::velocity`].

use rameau_clip::AudioClip;

/// A 3D position or velocity, listener-relative.
///
/// `x` is the left/right axis (negative = left), `y` up/down, `z` front/back.
/// Backends without spatial audio use only `x`, as a stereo pan in `-1.0..=1.0`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    /// A position/velocity on the left/right axis only (a stereo pan).
    pub const fn pan(x: f32) -> Self {
        Self { x, y: 0.0, z: 0.0 }
    }
}

/// The time-varying controls of a single voice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceParams {
    /// Pitch offset from the clip's natural pitch, in semitones.
    pub pitch: f32,
    /// Linear output gain, nominally `0.0..=1.0`.
    pub volume: f32,
    /// Spatial position (listener-relative). `x` doubles as a stereo pan.
    pub position: Vec3,
    /// Spatial velocity, used for Doppler on backends that support it.
    pub velocity: Vec3,
}

impl Default for VoiceParams {
    fn default() -> Self {
        Self {
            pitch: 0.0,
            volume: 1.0,
            position: Vec3::default(),
            velocity: Vec3::default(),
        }
    }
}

/// When a command takes effect on the playback timeline.
///
/// Backends apply this best-effort: those without sample-accurate scheduling
/// treat every timestamp as [`Now`](Timestamp::Now).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Timestamp {
    /// As soon as possible.
    Now,
    /// At an absolute time, in seconds since the engine's timeline began.
    AtSeconds(f64),
}

/// An inclusive-start, exclusive-end loop region, in sample frames.
pub type LoopRegion = core::ops::Range<u32>;

/// Why a playback operation could not be carried out.
#[derive(Debug)]
pub enum PlaybackError {
    /// The backend does not implement this capability at all.
    Unsupported(&'static str),
    /// Audio data could not be decoded.
    Decode(String),
    /// The underlying audio engine reported an error.
    Backend(String),
}

impl core::fmt::Display for PlaybackError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PlaybackError::Unsupported(what) => write!(f, "unsupported by backend: {what}"),
            PlaybackError::Decode(m) => write!(f, "decode error: {m}"),
            PlaybackError::Backend(m) => write!(f, "audio backend error: {m}"),
        }
    }
}

impl std::error::Error for PlaybackError {}

/// A sample-playback engine: a source of clips and sounding voices.
///
/// See the [module docs](self) for the capability/degradation model.
pub trait AudioPlayback {
    /// A piece of decoded audio ready to be played (backend-native).
    type Clip;
    /// A handle to one sounding voice, returned by [`start`](Self::start).
    type Playback;

    /// Builds a clip from mono 16-bit PCM at `sample_rate` Hz.
    fn clip_from_pcm(
        &mut self,
        samples: &[i16],
        sample_rate: u32,
    ) -> Result<Self::Clip, PlaybackError>;

    /// Builds a clip from the bytes of a single Ogg/Vorbis stream.
    ///
    /// Backends without a Vorbis decoder return
    /// [`PlaybackError::Unsupported`]; callers should fall back to decoding to
    /// PCM and using [`clip_from_pcm`](Self::clip_from_pcm).
    fn clip_from_vorbis(&mut self, ogg: &[u8]) -> Result<Self::Clip, PlaybackError>;

    /// Starts `clip` as a new voice and returns a handle to it.
    ///
    /// `loop_region`, when set, makes playback loop over that frame range.
    fn start(
        &mut self,
        when: Timestamp,
        clip: &Self::Clip,
        params: VoiceParams,
        loop_region: Option<LoopRegion>,
    ) -> Result<Self::Playback, PlaybackError>;

    /// Updates the live parameters of an existing voice.
    fn update(
        &mut self,
        when: Timestamp,
        playback: &mut Self::Playback,
        params: VoiceParams,
    ) -> Result<(), PlaybackError>;

    /// Stops a voice (releasing it, with any backend fade-out).
    fn stop(
        &mut self,
        when: Timestamp,
        playback: &mut Self::Playback,
    ) -> Result<(), PlaybackError>;

    /// Renders the engine's current output offline into `clip`.
    ///
    /// `clip` is interpreted as interleaved stereo `f32` (its length is
    /// `2 * frames`). Real-time-only backends return
    /// [`PlaybackError::Unsupported`].
    fn render(&mut self, clip: &mut dyn AudioClip<Value = f32>) -> Result<(), PlaybackError>;
}
