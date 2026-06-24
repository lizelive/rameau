//! A simple attack/release volume envelope.
//!
//! The high-level [`AudioPlayback`](rameau_playback::AudioPlayback) interface
//! only distinguishes "voice started" from "voice stopped", so this software
//! backend articulates each voice with a two-segment contour: a linear **A**ttack
//! ramp up to full level when the voice starts, a flat sustain while it is held,
//! and an exponential **R**elease down to silence once it is stopped.

/// Where in the attack/release contour a voice currently is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Attack,
    Sustain,
    Release,
    Done,
}

/// A running attack/release volume envelope.
#[derive(Debug, Clone)]
pub struct Envelope {
    stage: Stage,
    /// Current linear gain in `0.0..=1.0`.
    value: f32,
    /// Per-sample increment during attack.
    attack_step: f32,
    /// Per-sample multiplier applied during release.
    release_rate: f32,
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
    /// Builds an envelope whose attack and release last `attack`/`release`
    /// seconds at `sample_rate` Hz.
    pub fn new(attack: f32, release: f32, sample_rate: f32) -> Self {
        let attack_samples = (attack.max(0.0) * sample_rate).round() as u32;
        let release_samples = (release.max(0.0) * sample_rate).round() as u32;
        Self {
            stage: Stage::Attack,
            value: 0.0,
            attack_step: if attack_samples == 0 {
                1.0
            } else {
                1.0 / attack_samples as f32
            },
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
            Stage::Attack => {
                self.value += self.attack_step;
                if self.value >= 1.0 {
                    self.value = 1.0;
                    self.stage = Stage::Sustain;
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
