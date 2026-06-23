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

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

/// Blanket forwarding so `&C` is an [`AudioClip`] wherever `C` is.
impl<C: AudioClip + ?Sized> AudioClip for &C {
    type Value = C::Value;

    fn data(&self) -> &[Self::Value] {
        (**self).data()
    }

    fn sample_rate(&self) -> u32 {
        (**self).sample_rate()
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
    fn reference_forwards() {
        let clip = Clip::new(vec![1i16, 2, 3], 8_000);
        fn rate(c: impl AudioClip) -> u32 {
            c.sample_rate()
        }
        assert_eq!(rate(&clip), 8_000);
    }
}
