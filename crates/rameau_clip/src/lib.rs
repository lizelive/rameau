//! A minimal, format-independent representation of a block of audio samples.
//!
//! The [`AudioClip`] trait abstracts over "a contiguous run of PCM samples at a
//! known sample rate". [`Clip`] is the simple owned container that implements
//! it, but other crates can implement [`AudioClip`] for their own types so that
//! tools such as a WAV writer can consume them without copying.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A contiguous run of audio samples with an associated sample rate.
///
/// The samples are interpreted as a single channel of PCM; the element type is
/// left to the implementor (commonly `i16` or `f32`).
pub trait AudioClip {
    /// The per-sample value type (e.g. `i16` for 16-bit PCM).
    type Value;

    /// The audio samples, in playback order.
    fn data(&self) -> &[Self::Value];

    /// The audio samples, mutably, in playback order.
    ///
    /// This is the hot path for a renderer that fills a fixed-size block: take
    /// the slice once and write every sample without bounds-checking churn.
    fn data_mut(&mut self) -> &mut [Self::Value];

    /// Playback sample rate in Hz.
    fn sample_rate(&self) -> u32;
}

/// A simple owned [`AudioClip`]: a `Vec` of samples plus a sample rate.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Clip<T> {
    /// The audio samples, in playback order.
    pub data: Vec<T>,
    /// Playback sample rate in Hz.
    pub sample_rate: u32,
}

impl<T> Clip<T> {
    /// Creates a clip from owned samples and a sample rate.
    pub fn new(data: Vec<T>, sample_rate: u32) -> Self {
        Self { data, sample_rate }
    }
}

impl<T> AudioClip for Clip<T> {
    type Value = T;

    fn data(&self) -> &[T] {
        &self.data
    }

    fn data_mut(&mut self) -> &mut [T] {
        &mut self.data
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_exposes_data_and_rate() {
        let clip = Clip::new(vec![0i16, 1, -1, 32767], 44_100);
        assert_eq!(clip.data(), &[0, 1, -1, 32767]);
        assert_eq!(clip.sample_rate(), 44_100);
    }

    #[test]
    fn data_mut_writes_through() {
        let mut clip = Clip::new(vec![1i16, 2, 3], 8_000);
        clip.data_mut().fill(0);
        assert_eq!(clip.data(), &[0, 0, 0]);
    }
}
