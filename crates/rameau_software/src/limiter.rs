//! A brickwall peak limiter for the mixer's output stage.
//!
//! Summing voices is unbounded: a dense chord across presets whose zones each
//! contribute several voices easily exceeds `±1.0`, and the device hard-clips
//! what it is handed. This limiter bounds the block instead, trading a little
//! transient distortion for the harsh clipping that would otherwise occur.
//!
//! It is feed-forward with instant attack, an exponential release and no
//! lookahead. The gain computed from a frame's peak is applied to *that same*
//! frame, which is what makes it a true brickwall without a delay buffer.

/// Default ceiling the output is held below, in linear amplitude.
pub const DEFAULT_THRESHOLD: f32 = 0.95;
/// Default time for the gain to recover most of the way back to unity, seconds.
pub const DEFAULT_RELEASE: f32 = 0.1;

/// A running brickwall peak limiter.
#[derive(Debug, Clone)]
pub struct Limiter {
    /// Ceiling in linear amplitude; output magnitude stays at or below this.
    threshold: f32,
    /// Per-sample coefficient pulling the gain back towards unity.
    release_coef: f32,
    /// Current gain reduction in `0.0..=1.0` (1.0 = no reduction).
    gain: f32,
}

impl Limiter {
    /// Builds a limiter holding output below `threshold`, recovering over
    /// `release` seconds at `sample_rate` Hz.
    pub fn new(threshold: f32, release: f32, sample_rate: u32) -> Self {
        let samples = (release.max(0.0) * sample_rate as f32).max(1.0);
        Self {
            threshold: threshold.max(f32::MIN_POSITIVE),
            // Reaches ~63% of the way back to unity over `release` seconds.
            release_coef: 1.0 - (-1.0f32 / samples).exp(),
            gain: 1.0,
        }
    }

    /// A limiter with the default ceiling and release at `sample_rate` Hz.
    pub fn with_defaults(sample_rate: u32) -> Self {
        Self::new(DEFAULT_THRESHOLD, DEFAULT_RELEASE, sample_rate)
    }

    /// The ceiling this limiter holds output below.
    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    /// Limits an interleaved-stereo block in place.
    pub fn process(&mut self, buf: &mut [f32]) {
        for frame in buf.chunks_mut(2) {
            let peak = frame.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            let target = if peak > self.threshold {
                self.threshold / peak
            } else {
                1.0
            };

            if target < self.gain {
                // Instant attack: no overshoot escapes, so no lookahead needed.
                self.gain = target;
            } else {
                self.gain += (target - self.gain) * self.release_coef;
            }

            for s in frame {
                *s *= self.gain;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(buf: &[f32]) -> f32 {
        buf.iter().fold(0.0f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn holds_output_below_the_threshold() {
        let mut lim = Limiter::with_defaults(48_000);
        // A block far above full scale, as a stack of voices would produce.
        let mut buf = vec![4.0f32; 512 * 2];
        lim.process(&mut buf);
        assert!(
            peak(&buf) <= DEFAULT_THRESHOLD + 1e-6,
            "peak {} exceeded the ceiling",
            peak(&buf)
        );
    }

    #[test]
    fn first_sample_is_already_limited() {
        // Instant attack is the whole reason no lookahead is needed: the very
        // first frame of a loud block must already be bounded.
        let mut lim = Limiter::with_defaults(48_000);
        let mut buf = vec![10.0f32, 10.0];
        lim.process(&mut buf);
        assert!(buf[0] <= DEFAULT_THRESHOLD + 1e-6);
        assert!(buf[1] <= DEFAULT_THRESHOLD + 1e-6);
    }

    #[test]
    fn leaves_quiet_signal_untouched() {
        let mut lim = Limiter::with_defaults(48_000);
        let mut buf = vec![0.25f32; 128 * 2];
        lim.process(&mut buf);
        assert!(
            buf.iter().all(|&s| (s - 0.25).abs() < 1e-6),
            "a signal under the ceiling must pass through unchanged"
        );
    }

    #[test]
    fn recovers_towards_unity_after_a_transient() {
        let mut lim = Limiter::with_defaults(48_000);
        lim.process(&mut [8.0f32; 2]); // slam the gain down
        let reduced = lim.gain;
        assert!(reduced < 0.5, "a loud transient should pull the gain down");

        // A second of quiet signal is ample time to recover.
        let mut quiet = vec![0.1f32; 48_000 * 2];
        lim.process(&mut quiet);
        assert!(
            lim.gain > 0.99,
            "gain should return towards unity, got {}",
            lim.gain
        );
    }

    #[test]
    fn stereo_frames_share_one_gain() {
        // Gain is computed per frame from the louder channel, so limiting must
        // not shift the stereo image.
        let mut lim = Limiter::with_defaults(48_000);
        let mut buf = vec![4.0f32, 2.0];
        lim.process(&mut buf);
        assert!(
            (buf[0] / buf[1] - 2.0).abs() < 1e-5,
            "left/right ratio should survive limiting"
        );
    }
}
