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

/// Shortest callback period a backend can be relied on to service, in seconds.
///
/// On Windows `tinyaudio` uses DirectSound, which runs a double buffer and
/// signals the feed thread once per half. That thread therefore has to wake
/// every `frames_per_buffer / sample_rate` seconds, and Windows' scheduling
/// granularity is around 10-16 ms. Ask for less and the thread simply cannot be
/// woken often enough: it misses notifications, the buffer wraps over stale
/// audio, and output arrives at a fraction of real time rather than failing
/// outright.
///
/// Measured on a 48 kHz stereo device: 256 frames (5.3 ms) produced 0.09 s of
/// audio in 1.5 s of wall time, 384 frames (8 ms) produced 0.51 s in 2.5 s, and
/// 512 frames (10.7 ms) ran at exactly real time.
#[cfg(windows)]
const MIN_CALLBACK_PERIOD: f64 = 0.010;
/// ALSA and PulseAudio service far shorter periods, so only guard the absurd.
#[cfg(not(windows))]
const MIN_CALLBACK_PERIOD: f64 = 0.001;

/// The smallest `frames_per_buffer` this platform can service at `sample_rate`.
pub fn min_frames_per_buffer(sample_rate: u32) -> usize {
    (MIN_CALLBACK_PERIOD * f64::from(sample_rate)).ceil() as usize
}

impl Playback for TinyAudio {
    type Stream = Stream;
    type Error = Box<dyn core::error::Error>;

    /// # Errors
    ///
    /// Returns an error if `frames_per_buffer` is below
    /// [`min_frames_per_buffer`] for the configured sample rate. Such a stream
    /// opens successfully but is starved of callbacks, so it is rejected here
    /// rather than left to play a fraction of the audio it was given.
    fn open<F>(&self, config: PlaybackConfig, callback: F) -> Result<Self::Stream, Self::Error>
    where
        F: FnMut(&mut [f32]) + Send + 'static,
    {
        let min = min_frames_per_buffer(config.sample_rate);
        if config.frames_per_buffer < min {
            return Err(format!(
                "frames_per_buffer {} is too small at {} Hz: this platform cannot service \
                 callbacks faster than every {:.0} ms, so the stream would be starved. \
                 Use at least {min} frames.",
                config.frames_per_buffer,
                config.sample_rate,
                MIN_CALLBACK_PERIOD * 1000.0,
            )
            .into());
        }

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

    #[test]
    fn minimum_buffer_scales_with_sample_rate() {
        // The constraint is a callback *period*, so the frame count it implies
        // has to grow with the sample rate.
        let at_48k = min_frames_per_buffer(48_000);
        let at_96k = min_frames_per_buffer(96_000);
        assert_eq!(at_96k, at_48k * 2);
    }

    #[cfg(windows)]
    #[test]
    fn minimum_buffer_matches_measured_directsound_threshold() {
        // 384 frames at 48 kHz was measured starving; 512 ran at real time.
        let min = min_frames_per_buffer(48_000);
        assert!(min > 384, "384 frames was measured as starved, got min {min}");
        assert!(min <= 512, "512 frames was measured as working, got min {min}");
    }

    #[test]
    fn default_config_is_serviceable() {
        // The library's own suggested default must not be rejected.
        let config = PlaybackConfig::stereo_cd();
        assert!(config.frames_per_buffer >= min_frames_per_buffer(config.sample_rate));
    }

    #[test]
    fn a_starved_buffer_is_rejected_rather_than_played() {
        // No device is opened: the size is rejected before touching hardware.
        let config = PlaybackConfig {
            channels: 2,
            sample_rate: 48_000,
            frames_per_buffer: 1,
        };
        let Err(err) = TinyAudio.open(config, |_: &mut [f32]| {}) else {
            panic!("a 1-frame buffer cannot be serviced and must be rejected");
        };
        assert!(
            err.to_string().contains("too small"),
            "error should explain the cause, got: {err}"
        );
    }
}
