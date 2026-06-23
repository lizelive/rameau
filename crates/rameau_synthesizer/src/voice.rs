//! A single sounding voice: one sample played back with pitch, panning and a
//! volume envelope.
//!
//! A voice owns everything needed to turn a [`Sample`]'s PCM into audio for one
//! note: a fractional read position, a per-sample pitch increment, loop points,
//! a [`Envelope`] and the static gains derived from the note's generators. The
//! synthesizer allocates one voice per matching instrument zone of a note-on.

use rameau_soundfont::{Sample, SampleType};

use crate::envelope::Envelope;

/// Resolved parameters needed to start a voice, produced by the synthesizer
/// from the merged generator set.
pub struct VoiceParams {
    pub channel: u8,
    pub key: u8,
    pub exclusive_class: i32,
    /// Index into `SoundFont::samples` this voice reads from.
    pub sample_index: usize,

    pub out_sample_rate: f32,
    pub sample_rate: f32,

    /// MIDI key the sample was recorded at (after `OverridingRootKey`).
    pub root_key: u8,
    /// Cents per key (default 100).
    pub scale_tuning: i32,
    /// Fixed tuning offset in cents (coarse, fine and sample correction).
    pub tune_cents: f32,

    /// Playback window and loop points, in sample frames.
    pub start: u32,
    pub end: u32,
    pub loop_start: u32,
    pub loop_end: u32,
    pub looping: bool,

    /// Static note gain (attenuation x velocity), pre-pan.
    pub amp: f32,
    /// Zone + channel pan in `-1.0..=1.0` (left..right).
    pub pan: f32,

    pub delay: f32,
    pub attack: f32,
    pub hold: f32,
    pub decay: f32,
    pub sustain_cb: f32,
    pub release: f32,
}

/// A sounding voice.
pub struct Voice {
    pub channel: u8,
    pub key: u8,
    pub exclusive_class: i32,
    /// Held down by the sustain pedal: a note-off is deferred until release.
    pub held_by_pedal: bool,

    sample_index: usize,

    /// Fractional read position in sample frames.
    pos: f64,
    end: f64,
    loop_start: f64,
    loop_end: f64,
    looping: bool,

    /// Base increment from sample-rate ratio and fixed tuning (no pitch bend).
    base_increment: f64,
    /// Effective increment including the current pitch bend.
    increment: f64,

    amp: f32,
    /// Per-channel-output gains (equal-power pan applied to `amp`).
    gain_left: f32,
    gain_right: f32,

    env: Envelope,
    finished: bool,
}

/// Equal-power pan gains for `pan` in `-1.0..=1.0`.
fn pan_gains(pan: f32) -> (f32, f32) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * 0.5 * std::f32::consts::FRAC_PI_2;
    (angle.cos(), angle.sin())
}

impl Voice {
    /// Starts a voice from resolved parameters.
    pub fn new(p: VoiceParams, sample_index: usize) -> Self {
        let ratio = (p.sample_rate / p.out_sample_rate) as f64;
        // Fixed pitch in cents: keyboard tracking plus fixed tuning.
        let key_cents = (p.key as f32 - p.root_key as f32) * p.scale_tuning as f32;
        let cents = key_cents + p.tune_cents;
        let base_increment = ratio * 2f64.powf(cents as f64 / 1200.0);

        let (pl, pr) = pan_gains(p.pan);

        let env = Envelope::new(
            p.delay,
            p.attack,
            p.hold,
            p.decay,
            p.sustain_cb,
            p.release,
            p.out_sample_rate,
        );

        Self {
            channel: p.channel,
            key: p.key,
            exclusive_class: p.exclusive_class,
            held_by_pedal: false,
            sample_index,
            pos: p.start as f64,
            end: p.end as f64,
            loop_start: p.loop_start as f64,
            loop_end: p.loop_end as f64,
            looping: p.looping && p.loop_end > p.loop_start,
            base_increment,
            increment: base_increment,
            amp: p.amp,
            gain_left: p.amp * pl,
            gain_right: p.amp * pr,
            env,
            finished: false,
        }
    }

    /// The sample this voice reads from (index into `SoundFont::samples`).
    pub fn sample_index(&self) -> usize {
        self.sample_index
    }

    /// Whether the voice has finished and can be reused.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// A rough loudness estimate, used to pick the quietest voice to steal.
    pub fn loudness(&self) -> f32 {
        self.amp
    }

    /// Sets the pitch bend for this voice, in cents.
    pub fn set_bend_cents(&mut self, cents: f32) {
        self.increment = self.base_increment * 2f64.powf(cents as f64 / 1200.0);
    }

    /// Begins the release stage of the envelope (key up).
    pub fn release(&mut self) {
        self.env.release();
    }

    /// Stops the voice immediately (e.g. exclusive-class cut-off or steal).
    pub fn kill(&mut self) {
        self.finished = true;
    }

    /// Mixes this voice into `out` (interleaved stereo, `2 * frames` long),
    /// scaled by the channel gains, advancing the read position and envelope.
    pub fn render_additive(&mut self, sample: &Sample, out: &mut [f32], chan_l: f32, chan_r: f32) {
        if self.finished {
            return;
        }
        let data = &sample.clip.data;
        if data.is_empty() {
            self.finished = true;
            return;
        }
        let gl = self.gain_left * chan_l;
        let gr = self.gain_right * chan_r;
        let last = (data.len() - 1) as f64;
        let end = self.end.min(data.len() as f64);

        for frame in out.chunks_exact_mut(2) {
            // Linear interpolation between the two straddling samples.
            let i = self.pos.floor();
            let frac = (self.pos - i) as f32;
            let i0 = i as usize;
            let i1 = if (i + 1.0) <= last { i0 + 1 } else { i0 };
            let s0 = data[i0] as f32;
            let s1 = data[i1] as f32;
            // i16 PCM -> -1.0..=1.0.
            let sample_value = (s0 + (s1 - s0) * frac) * (1.0 / 32768.0);

            let g = self.env.next_gain();
            frame[0] += sample_value * g * gl;
            frame[1] += sample_value * g * gr;

            self.pos += self.increment;

            if self.looping {
                if self.pos >= self.loop_end {
                    self.pos -= self.loop_end - self.loop_start;
                }
            } else if self.pos >= end {
                self.finished = true;
                break;
            }

            if self.env.is_finished() {
                self.finished = true;
                break;
            }
        }
    }
}

/// Whether a sample's type carries usable mono/stereo PCM (not a ROM sample).
pub fn is_playable(sample: &Sample) -> bool {
    !matches!(
        sample.kind,
        SampleType::RomMono | SampleType::RomRight | SampleType::RomLeft | SampleType::RomLinked
    ) && !sample.clip.data.is_empty()
}
