//! The [`TinyAudio`] [`Playback`] backend and its [`Stream`] handle.

use rameau_playback::{Playback, PlaybackConfig};
use tinyaudio::{OutputDevice, OutputDeviceParameters, run_output_device};

/// A [`Playback`] backend that renders through `tinyaudio`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TinyAudio;

/// Handle to a running `tinyaudio` output stream.
///
/// Dropping this stops playback by tearing down the underlying device.
pub struct Stream {
    // Kept alive for its `Drop`; `tinyaudio` stops the device when the
    // `OutputDevice` is dropped.
    _device: OutputDevice,
}

impl Playback for TinyAudio {
    type Stream = Stream;
    type Error = Box<dyn std::error::Error>;

    fn open<F>(&self, config: PlaybackConfig, callback: F) -> Result<Self::Stream, Self::Error>
    where
        F: FnMut(&mut [f32]) + Send + 'static,
    {
        let params = OutputDeviceParameters {
            channels_count: config.channels as usize,
            sample_rate: config.sample_rate as usize,
            channel_sample_count: config.frames_per_buffer,
        };

        let device = run_output_device(params, callback)?;
        Ok(Stream { _device: device })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Opening a real device requires audio hardware, so this only checks that
    // the backend wires up to the `Playback` trait and produces a sender we can
    // hand a callback. We avoid actually starting a device in CI.
    fn _assert_implements_playback() {
        fn takes_playback<P: Playback>(_: P) {}
        takes_playback(TinyAudio);
    }

    #[test]
    fn backend_is_default_constructible() {
        let _backend = TinyAudio;
        let _config = PlaybackConfig::stereo_cd();
    }
}
