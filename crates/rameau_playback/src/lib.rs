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

/// How an output stream should be configured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaybackConfig {
    /// Number of interleaved channels (e.g. `1` for mono, `2` for stereo).
    pub channels: u16,
    /// Playback sample rate in Hz.
    pub sample_rate: u32,
    /// Number of frames the backend requests per callback invocation.
    ///
    /// A *frame* is one sample per channel, so each callback is handed a
    /// buffer of `channels * frames_per_buffer` `f32` values. Smaller values
    /// reduce latency at the cost of more frequent callbacks.
    pub frames_per_buffer: usize,
}

impl PlaybackConfig {
    /// CD-quality stereo at 44.1 kHz with a modest buffer.
    pub const fn stereo_cd() -> Self {
        Self {
            channels: 2,
            sample_rate: 44_100,
            frames_per_buffer: 512,
        }
    }

    /// Total number of `f32` samples in one callback buffer
    /// (`channels * frames_per_buffer`).
    pub const fn buffer_len(&self) -> usize {
        self.channels as usize * self.frames_per_buffer
    }
}

/// An audio backend capable of opening a real-time output stream.
pub trait Playback {
    /// A handle that keeps the stream alive; dropping it stops playback.
    type Stream;

    /// The error type returned when a stream cannot be opened.
    type Error;

    /// Opens an output stream that repeatedly calls `callback` to fill
    /// interleaved buffers of `f32` samples.
    ///
    /// The callback receives a slice whose length is
    /// [`PlaybackConfig::buffer_len`]. Samples are interleaved by channel
    /// (`L, R, L, R, …` for stereo) and expected to lie in `[-1.0, 1.0]`.
    ///
    /// Playback continues until the returned [`Stream`](Self::Stream) is
    /// dropped.
    fn open<F>(&self, config: PlaybackConfig, callback: F) -> Result<Self::Stream, Self::Error>
    where
        F: FnMut(&mut [f32]) + Send + 'static;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_len_accounts_for_channels() {
        let config = PlaybackConfig {
            channels: 2,
            sample_rate: 48_000,
            frames_per_buffer: 256,
        };
        assert_eq!(config.buffer_len(), 512);
    }

    #[test]
    fn stereo_cd_defaults() {
        let config = PlaybackConfig::stereo_cd();
        assert_eq!(config.channels, 2);
        assert_eq!(config.sample_rate, 44_100);
        assert_eq!(config.buffer_len(), 1_024);
    }
}
