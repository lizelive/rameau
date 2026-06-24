//! A single sounding voice in the software mixer.

use std::sync::Arc;

use rameau_clip::Clip;

use crate::envelope::Envelope;

/// Equal-power pan gains for `pan` in `-1.0..=1.0` (left..right).
fn pan_gains(pan: f32) -> (f32, f32) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * 0.5 * std::f32::consts::FRAC_PI_2;
    (angle.cos(), angle.sin())
}

/// One playing sample: a fractional read position, a pitch increment, optional
/// loop points, an attack/release envelope and pan gains.
pub struct Voice {
    /// The handle id this voice was started under.
    pub id: u64,
    clip: Arc<Clip<i16>>,

    /// Absolute frame (on the backend clock) at which the voice becomes audible.
    start_frame: u64,
    /// Absolute frame at which to begin the release, if scheduled.
    release_frame: Option<u64>,

    /// Fractional read position in sample frames.
    pos: f64,
    loop_start: f64,
    loop_end: f64,
    looping: bool,

    /// Output-rate ratio (`clip_rate / out_rate`) before pitch is applied.
    rate_ratio: f64,
    /// Effective per-sample increment, including pitch.
    increment: f64,

    /// Linear gain before panning.
    amp: f32,
    gain_left: f32,
    gain_right: f32,

    env: Envelope,
    finished: bool,
}

impl Voice {
    /// Starts a voice reading `clip`, becoming audible at `start_frame`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: u64,
        clip: Arc<Clip<i16>>,
        out_sample_rate: u32,
        start_frame: u64,
        pitch: f32,
        volume: f32,
        pan: f32,
        loop_region: Option<core::ops::Range<u32>>,
        attack: f32,
        release: f32,
    ) -> Self {
        let rate_ratio = clip.sample_rate as f64 / out_sample_rate as f64;
        let increment = rate_ratio * 2f64.powf(pitch as f64 / 12.0);
        let (loop_start, loop_end, looping) = match loop_region {
            Some(r) if r.end > r.start => (r.start as f64, r.end as f64, true),
            _ => (0.0, 0.0, false),
        };
        let (pl, pr) = pan_gains(pan);
        Self {
            id,
            clip,
            start_frame,
            release_frame: None,
            pos: 0.0,
            loop_start,
            loop_end,
            looping,
            rate_ratio,
            increment,
            amp: volume,
            gain_left: volume * pl,
            gain_right: volume * pr,
            env: Envelope::new(attack, release, out_sample_rate as f32),
            finished: false,
        }
    }

    /// Whether the voice has finished and can be reclaimed.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Updates the live pitch/volume/pan of the voice.
    pub fn update(&mut self, pitch: f32, volume: f32, pan: f32) {
        self.increment = self.rate_ratio * 2f64.powf(pitch as f64 / 12.0);
        self.amp = volume;
        let (pl, pr) = pan_gains(pan);
        self.gain_left = volume * pl;
        self.gain_right = volume * pr;
    }

    /// Schedules the release for absolute frame `frame` (immediate releases pass
    /// the current clock).
    pub fn schedule_release(&mut self, frame: u64) {
        // If the voice is already audible and the release is due, the render
        // loop will pick it up; storing the frame keeps offline scheduling exact.
        self.release_frame = Some(frame);
    }

    /// Mixes this voice into the interleaved-stereo block `out`, whose first
    /// frame corresponds to absolute frame `block_start`. `chan_l`/`chan_r` are
    /// extra per-channel gains (e.g. MIDI channel volume).
    pub fn render(&mut self, out: &mut [f32], block_start: u64, chan_l: f32, chan_r: f32) {
        if self.finished {
            return;
        }
        let frames = out.len() / 2;
        let block_end = block_start + frames as u64;
        if self.start_frame >= block_end {
            return; // not audible yet this block
        }
        let data = &self.clip.data;
        if data.is_empty() {
            self.finished = true;
            return;
        }

        let gl = self.gain_left * chan_l;
        let gr = self.gain_right * chan_r;
        let last = (data.len() - 1) as f64;
        let end = data.len() as f64;
        let begin = self.start_frame.saturating_sub(block_start) as usize;

        for fi in begin..frames {
            let abs = block_start + fi as u64;
            if let Some(rf) = self.release_frame
                && abs >= rf
            {
                self.env.release();
                self.release_frame = None;
            }

            // Linear interpolation between the two straddling samples.
            let i = self.pos.floor();
            let frac = (self.pos - i) as f32;
            let i0 = i as usize;
            let i1 = if (i + 1.0) <= last { i0 + 1 } else { i0 };
            let s0 = data[i0] as f32;
            let s1 = data[i1] as f32;
            let sample_value = (s0 + (s1 - s0) * frac) * (1.0 / 32768.0);

            let g = self.env.next_gain();
            out[fi * 2] += sample_value * g * gl;
            out[fi * 2 + 1] += sample_value * g * gr;

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
