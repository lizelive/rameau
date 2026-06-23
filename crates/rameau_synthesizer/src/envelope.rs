//! A DAHDSR volume envelope, the amplitude contour SoundFont voices follow.
//!
//! SoundFont articulation specifies the volume envelope as six segments:
//! **D**elay, **A**ttack, **H**old, **D**ecay, **S**ustain and **R**elease.
//! Times are given in timecents and the sustain level as an attenuation in
//! centibels; this module turns those into a per-sample gain.
//!
//! The attack is linear in amplitude (as the specification requires) while the
//! decay and release are exponential, which is both cheaper and closer to how
//! real instruments and the reference implementations behave.

/// Where in the DAHDSR contour a voice currently is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Delay,
    Attack,
    Hold,
    Decay,
    Sustain,
    Release,
    Done,
}

/// A running DAHDSR volume envelope.
#[derive(Debug, Clone)]
pub struct Envelope {
    stage: Stage,
    /// Current linear gain in `0.0..=1.0`.
    value: f32,
    /// Samples remaining in the current timed stage.
    countdown: u32,

    // Pre-computed, in samples / per-sample increments at the render rate.
    attack_samples: u32,
    hold_samples: u32,
    decay_samples: u32,
    /// Linear sustain gain in `0.0..=1.0`.
    sustain: f32,
    /// Per-sample multiplier applied during decay/release.
    decay_rate: f32,
    release_rate: f32,
}

/// Timecents that mean "effectively zero time" in the SoundFont spec.
const SILENT_TIMECENTS: f32 = -32_768.0;

/// Converts a duration in timecents to a sample count at `sample_rate`.
fn timecents_to_samples(timecents: f32, sample_rate: f32) -> u32 {
    if timecents <= SILENT_TIMECENTS {
        return 0;
    }
    let seconds = 2.0f32.powf(timecents / 1200.0);
    (seconds * sample_rate).round() as u32
}

/// The per-sample multiplier that decays from `1.0` to ~`floor` over `samples`.
fn exp_rate(samples: u32, floor: f32) -> f32 {
    if samples == 0 {
        0.0
    } else {
        floor.powf(1.0 / samples as f32)
    }
}

impl Envelope {
    /// Builds an envelope from SoundFont generator values.
    ///
    /// `delay`, `attack`, `hold`, `decay` and `release` are in timecents;
    /// `sustain_cb` is the sustain attenuation in centibels (0 = full level).
    pub fn new(
        delay: f32,
        attack: f32,
        hold: f32,
        decay: f32,
        sustain_cb: f32,
        release: f32,
        sample_rate: f32,
    ) -> Self {
        let delay_samples = timecents_to_samples(delay, sample_rate);
        let attack_samples = timecents_to_samples(attack, sample_rate);
        let hold_samples = timecents_to_samples(hold, sample_rate);
        let decay_samples = timecents_to_samples(decay, sample_rate);
        let release_samples = timecents_to_samples(release, sample_rate);

        // centibels of attenuation -> linear gain.
        let sustain = 10.0f32.powf(-sustain_cb.max(0.0) / 200.0).clamp(0.0, 1.0);

        Self {
            stage: Stage::Delay,
            value: 0.0,
            countdown: delay_samples,
            attack_samples,
            hold_samples,
            decay_samples,
            sustain,
            // Decay falls toward the sustain level; release toward silence.
            decay_rate: exp_rate(decay_samples, sustain.max(1e-3)),
            release_rate: exp_rate(release_samples, 1e-4),
        }
    }

    /// Whether the envelope has fully released and the voice can be freed.
    pub fn is_finished(&self) -> bool {
        self.stage == Stage::Done
    }

    /// Advances to the release stage from the current level.
    pub fn release(&mut self) {
        if self.stage != Stage::Done {
            self.stage = Stage::Release;
        }
    }

    /// Returns the gain for the current sample and advances one sample.
    pub fn next_gain(&mut self) -> f32 {
        match self.stage {
            Stage::Delay => {
                if self.countdown == 0 {
                    self.stage = Stage::Attack;
                    self.countdown = self.attack_samples;
                    return self.next_gain();
                }
                self.countdown -= 1;
                0.0
            }
            Stage::Attack => {
                if self.attack_samples == 0 {
                    self.value = 1.0;
                    self.stage = Stage::Hold;
                    self.countdown = self.hold_samples;
                    return self.value;
                }
                self.value += 1.0 / self.attack_samples as f32;
                if self.countdown == 0 || self.value >= 1.0 {
                    self.value = 1.0;
                    self.stage = Stage::Hold;
                    self.countdown = self.hold_samples;
                } else {
                    self.countdown -= 1;
                }
                self.value
            }
            Stage::Hold => {
                if self.countdown == 0 {
                    self.stage = Stage::Decay;
                    self.countdown = self.decay_samples;
                    return self.next_gain();
                }
                self.countdown -= 1;
                self.value
            }
            Stage::Decay => {
                self.value *= self.decay_rate;
                if self.value <= self.sustain || self.countdown == 0 {
                    self.value = self.sustain;
                    self.stage = Stage::Sustain;
                } else {
                    self.countdown -= 1;
                }
                self.value
            }
            Stage::Sustain => self.value,
            Stage::Release => {
                self.value *= self.release_rate;
                if self.value <= 1e-4 {
                    self.value = 0.0;
                    self.stage = Stage::Done;
                }
                self.value
            }
            Stage::Done => 0.0,
        }
    }
}
