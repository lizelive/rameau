//! The formant synthesizer: a glottal pulse through a cascade of resonators.
//!
//! The model is a much-simplified Klatt synthesizer. A Rosenberg glottal
//! pulse at the fundamental (with vibrato, jitter and a breath of noise) is
//! differentiated for the lip-radiation tilt, then pushed through four
//! two-pole resonators in cascade, one per formant, plus a fixed "singer's
//! formant" near 3 kHz that gives a trained voice its ring. Nasal vowels get
//! an extra low resonance and a wider first formant.
//!
//! Every render is made to *loop*: the vibrato rate and the fundamental are
//! chosen so an integer number of cycles of each fits the loop, and the
//! aspiration noise is cross-faded across the seam.

use rameau_types::{Rng, SplitMix64};

use crate::vowel::{Formant, Vowel};

/// A register-specific description of one singer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Singer {
    /// Nominal fundamental in Hz.
    pub f0: f32,
    /// Factor applied to every formant frequency (1.0 = adult male).
    pub formant_scale: f32,
    /// Vibrato depth as a fraction of `f0` (0.017 ≈ 30 cents).
    pub vibrato_depth: f32,
    /// Breathiness: level of aspiration noise relative to the pulse.
    pub breath: f32,
}

/// Rendering parameters for one sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderSpec {
    /// Output sample rate in Hz.
    pub sample_rate: u32,
    /// Seconds before the loop region begins (attack plus vibrato onset).
    pub lead_in: f32,
    /// Approximate loop length in seconds; the exact length fits whole
    /// vibrato and pitch cycles.
    pub loop_seconds: f32,
    /// Number of singers layered (1 = solo).
    pub voices: usize,
    /// Seed for jitter, detune and breath noise.
    pub seed: u64,
}

impl Default for RenderSpec {
    fn default() -> Self {
        Self {
            sample_rate: 32_000,
            lead_in: 0.55,
            loop_seconds: 1.45,
            voices: 1,
            seed: 1789,
        }
    }
}

/// A rendered, loopable vowel.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    /// 16-bit PCM, mono.
    pub pcm: Vec<i16>,
    /// Loop start in frames.
    pub loop_start: u32,
    /// Loop end in frames (exclusive).
    pub loop_end: u32,
    /// The sample rate.
    pub sample_rate: u32,
}

/// A two-pole resonator (Klatt's `RESON`).
#[derive(Debug, Clone, Copy)]
struct Resonator {
    a: f32,
    b: f32,
    c: f32,
    y1: f32,
    y2: f32,
}

impl Resonator {
    fn new(hz: f32, bw: f32, sample_rate: f32) -> Self {
        let r = (-core::f32::consts::PI * bw / sample_rate).exp();
        let theta = core::f32::consts::TAU * hz / sample_rate;
        let c = -r * r;
        let b = 2.0 * r * theta.cos();
        let a = 1.0 - b - c;
        Self { a, b, c, y1: 0.0, y2: 0.0 }
    }

    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.a * x + self.b * self.y1 + self.c * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Rosenberg glottal pulse for phase `t` in `0..1`.
fn rosenberg(t: f32) -> f32 {
    const OPEN: f32 = 0.45;
    const CLOSE: f32 = 0.18;
    if t < OPEN {
        0.5 * (1.0 - (core::f32::consts::PI * t / OPEN).cos())
    } else if t < OPEN + CLOSE {
        (core::f32::consts::PI * (t - OPEN) / (2.0 * CLOSE)).cos()
    } else {
        0.0
    }
}

/// Renders `vowel` sung by `singer`.
///
/// `spec.voices > 1` layers that many detuned singers, each with its own
/// vibrato rate and jitter, which is what turns a soloist into a crowd.
pub fn render(vowel: Vowel, singer: Singer, spec: &RenderSpec) -> Rendered {
    let sr = spec.sample_rate.max(8_000) as f32;
    let mut rng = SplitMix64::new(spec.seed ^ (vowel.program() as u64) << 8);

    // Fit the loop: a whole number of vibrato cycles at about 5.5 Hz.
    let vib_cycles = (spec.loop_seconds * 5.5).round().max(1.0);
    let loop_len = vib_cycles / 5.5;
    let loop_frames = (loop_len * sr).round() as usize;
    let loop_len = loop_frames as f32 / sr;
    let lead_frames = (spec.lead_in * sr).round() as usize;
    let tail_frames = (0.05 * sr) as usize;
    let total = lead_frames + loop_frames + tail_frames;

    let mut mix = vec![0.0f32; total];
    let n_voices = spec.voices.max(1);
    for v in 0..n_voices {
        // Detune so the loop still holds a whole number of pitch cycles.
        let base_cycles = (singer.f0 * loop_len).round();
        let detune_cycles = if n_voices == 1 {
            0.0
        } else {
            rng.range_i32(-3, 3) as f32
        };
        let f0 = (base_cycles + detune_cycles) / loop_len;
        // Each singer's vibrato is its own whole number of cycles per loop.
        let vib_hz = if n_voices == 1 {
            vib_cycles / loop_len
        } else {
            (vib_cycles + rng.range_i32(-2, 2) as f32).max(2.0) / loop_len
        };
        let vib_phase = if v == 0 { 0.0 } else { rng.next_f64() as f32 };
        let jitter_amount = if n_voices == 1 { 0.003 } else { 0.006 };
        let breath = singer.breath * (1.0 + 0.3 * rng.next_f64() as f32);

        let mut formants: Vec<Resonator> = vowel
            .formants()
            .iter()
            .map(|Formant { hz, bw }| {
                let widen = if vowel.is_nasal() { 1.6 } else { 1.0 };
                Resonator::new(hz * singer.formant_scale, bw * widen, sr)
            })
            .collect();
        // Singer's formant, and a nasal murmur for nasal vowels.
        formants.push(Resonator::new(2_900.0 * singer.formant_scale.sqrt(), 220.0, sr));
        let mut nasal = vowel
            .is_nasal()
            .then(|| Resonator::new(280.0 * singer.formant_scale, 90.0, sr));
        // Glottal spectral tilt.
        let mut tilt = Resonator::new(0.0, 120.0, sr);

        // Pitch flutter: a few slow wobbles with a whole number of cycles per
        // loop, so — unlike a random walk — the phase returns exactly to
        // where it started at the seam.
        let flutter: Vec<(f32, f32, f32)> = (0..3)
            .map(|_| {
                let k = rng.range_i32(2, 7) as f32;
                (k / loop_len, jitter_amount * rng.next_f64() as f32, rng.next_f64() as f32)
            })
            .collect();
        let lead_in = spec.lead_in.max(0.05);
        let mut prev_pulse = 0.0f32;
        let mut buf = vec![0.0f32; total];
        for (i, out) in buf.iter_mut().enumerate() {
            let t = i as f32 / sr;
            // Vibrato depth fades in over the lead-in, then holds.
            let depth = singer.vibrato_depth * (t / lead_in).min(1.0);
            // Phase is integrated analytically: f0·t plus the integral of
            // each sinusoidal modulation, which is periodic in the loop.
            let mut cycles = f0 * t;
            cycles -= f0 * depth
                * (core::f32::consts::TAU * (vib_hz * t + vib_phase)).cos()
                / (core::f32::consts::TAU * vib_hz);
            for &(hz, amp, ph) in &flutter {
                cycles -= f0 * amp * (core::f32::consts::TAU * (hz * t + ph)).cos()
                    / (core::f32::consts::TAU * hz);
            }
            let phase = cycles.rem_euclid(1.0);
            let pulse = rosenberg(phase);
            // Radiation: first difference.
            let src = pulse - prev_pulse;
            prev_pulse = pulse;
            let noise = (rng.next_f64() as f32 * 2.0 - 1.0) * breath * (0.4 + 0.6 * pulse);
            let mut x = tilt.tick(src * 4.0) + noise;
            for r in &mut formants {
                x = r.tick(x);
            }
            if let Some(n) = &mut nasal {
                x += 0.6 * n.tick(src * 2.0);
            }
            *out = x;
        }
        for (m, s) in mix.iter_mut().zip(&buf) {
            *m += s / (n_voices as f32).sqrt();
        }
    }

    // Attack envelope over the first 25 ms, and cross-fade the loop seam so
    // the noise and any residual drift do not click.
    let attack = (0.025 * sr) as usize;
    for (i, s) in mix.iter_mut().enumerate().take(attack) {
        *s *= i as f32 / attack as f32;
    }
    let fade = (0.02 * sr) as usize;
    let loop_end = lead_frames + loop_frames;
    for k in 0..fade.min(loop_frames / 4) {
        let w = k as f32 / fade as f32;
        let seam = loop_end - fade + k;
        let head = lead_frames + k;
        let (Some(&a), Some(&b)) = (mix.get(seam), mix.get(head)) else { break };
        // Ease the tail into what the head sounds like.
        let blended = a * (1.0 - w) + b * w;
        if let Some(slot) = mix.get_mut(seam) {
            *slot = blended;
        }
    }

    let peak = mix.iter().fold(1e-6f32, |m, s| m.max(s.abs()));
    let gain = 0.85 / peak;
    let pcm: Vec<i16> = mix
        .iter()
        .map(|s| (s * gain * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16)
        .collect();
    Rendered {
        pcm,
        loop_start: lead_frames as u32,
        loop_end: loop_end as u32,
        sample_rate: spec.sample_rate.max(8_000),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_loopable_vowel() {
        let spec = RenderSpec {
            sample_rate: 16_000,
            ..RenderSpec::default()
        };
        let singer = Singer {
            f0: 220.0,
            formant_scale: 1.0,
            vibrato_depth: 0.017,
            breath: 0.02,
        };
        let r = render(Vowel::A, singer, &spec);
        assert!(r.loop_end > r.loop_start);
        assert!(r.pcm.len() as u32 >= r.loop_end);
        let peak = r.pcm.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!(peak > 20_000, "should be normalised near full scale, got {peak}");
        // The loop seam should not jump: compare the last loop sample to the first.
        let a = r.pcm[(r.loop_end - 1) as usize] as f32;
        let b = r.pcm[r.loop_start as usize] as f32;
        assert!((a - b).abs() < 4_000.0, "seam jump {a} -> {b}");
    }

    #[test]
    fn choir_is_different_from_solo() {
        let spec = RenderSpec {
            sample_rate: 16_000,
            voices: 4,
            ..RenderSpec::default()
        };
        let singer = Singer {
            f0: 220.0,
            formant_scale: 1.0,
            vibrato_depth: 0.017,
            breath: 0.02,
        };
        let choir = render(Vowel::O, singer, &spec);
        let solo = render(Vowel::O, singer, &RenderSpec { voices: 1, ..spec });
        assert_ne!(choir.pcm, solo.pcm);
    }
}
