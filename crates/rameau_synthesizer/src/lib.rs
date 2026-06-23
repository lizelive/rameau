//! A low-latency, sample-based SoundFont synthesizer.
//!
//! [`Synthesizer`] turns a stream of timestamped [`MidiEvent`]s into audio using
//! a [`SoundFont`] supplied at construction. It performs the SoundFont
//! generator resolution itself (preset zone -> instrument -> sample), articulates
//! each note with a DAHDSR volume envelope, and mixes all active voices into an
//! interleaved stereo `f32` buffer.
//!
//! # Rendering model
//!
//! The single entry point is [`Synthesizer::render`]. It fills an
//! [`AudioClip`] whose length determines the block size: a clip holding
//! `2 * frames` samples is rendered as `frames` stereo frames. Event timestamps
//! are **sample-frame offsets within that block**, so the same call serves both
//! offline rendering (one large block) and real-time playback (one small block
//! per audio callback), with sample-accurate event timing in both cases.
//!
//! ```no_run
//! use rameau_clip::Clip;
//! use rameau_midi::event::MidiEvent;
//! use rameau_soundfont::SoundFont;
//! use rameau_synthesizer::Synthesizer;
//!
//! let sf = SoundFont::load_file("bank.sf2").unwrap();
//! let mut synth = Synthesizer::new(sf, 44_100);
//!
//! // One 512-frame stereo block.
//! let mut block = Clip::new(vec![0.0f32; 512 * 2], 44_100);
//! let events = [(0u64, MidiEvent::NoteOn { channel: 0, key: 60, vel: 100 })];
//! synth.render(events, &mut block);
//! ```

mod envelope;
mod gens;
mod voice;

use std::collections::HashMap;

use rameau_midi::event::MidiEvent;
use rameau_soundfont::{GeneratorType as G, SoundFont, Zone};

use gens::Gens;
use voice::{Voice, VoiceParams};

pub use rameau_clip::{AudioClip, Clip};
pub use rameau_midi::event::MidiEvent as Event;

/// The MIDI channel reserved for percussion (channel 10, zero-based 9).
const DRUM_CHANNEL: u8 = 9;
/// The bank percussion presets live in for General MIDI banks.
const DRUM_BANK: u16 = 128;
/// Maximum simultaneously sounding voices before voice stealing kicks in.
const MAX_POLYPHONY: usize = 64;

/// Per-MIDI-channel controller and program state.
#[derive(Debug, Clone, Copy)]
struct ChannelState {
    bank: u16,
    program: u8,
    /// CC7, channel volume (0..=127).
    volume: u8,
    /// CC11, expression (0..=127).
    expression: u8,
    /// CC10, pan (0..=127, 64 = centre).
    pan: u8,
    /// 14-bit pitch bend (0..=16383, 8192 = centre).
    bend: u16,
    /// CC64, sustain pedal held.
    sustain: bool,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            bank: 0,
            program: 0,
            volume: 100,
            expression: 127,
            pan: 64,
            bend: 8192,
            sustain: false,
        }
    }
}

impl ChannelState {
    /// Combined volume x expression as a linear scalar in `0.0..=1.0`.
    fn gain(&self) -> f32 {
        (self.volume as f32 / 127.0) * (self.expression as f32 / 127.0)
    }

    /// Pitch bend in cents, assuming the default +/-2 semitone range.
    fn bend_cents(&self) -> f32 {
        (self.bend as f32 - 8192.0) / 8192.0 * 200.0
    }

    /// Pan as a signed position in `-1.0..=1.0` (left..right).
    fn pan_unit(&self) -> f32 {
        (self.pan as f32 - 64.0) / 63.0
    }
}

/// A polyphonic SoundFont synthesizer.
pub struct Synthesizer {
    soundfont: SoundFont,
    sample_rate: u32,
    channels: [ChannelState; 16],
    voices: Vec<Voice>,
    /// `(bank, program)` -> index into `soundfont.presets`.
    preset_index: HashMap<(u16, u16), usize>,
}

impl Synthesizer {
    /// Creates a synthesizer that renders `soundfont` at `sample_rate` Hz.
    pub fn new(soundfont: SoundFont, sample_rate: u32) -> Self {
        let mut preset_index = HashMap::new();
        for (i, preset) in soundfont.presets.iter().enumerate() {
            // First definition wins for any duplicate (bank, program).
            preset_index
                .entry((preset.bank, preset.program))
                .or_insert(i);
        }
        Self {
            soundfont,
            sample_rate,
            channels: [ChannelState::default(); 16],
            voices: Vec::with_capacity(MAX_POLYPHONY),
            preset_index,
        }
    }

    /// The render sample rate in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Number of voices currently sounding.
    pub fn active_voices(&self) -> usize {
        self.voices.len()
    }

    /// The SoundFont this synthesizer plays.
    pub fn soundfont(&self) -> &SoundFont {
        &self.soundfont
    }

    /// Silences every voice immediately and resets controllers.
    pub fn reset(&mut self) {
        self.voices.clear();
        self.channels = [ChannelState::default(); 16];
    }

    /// Renders `events` into `output`, an interleaved stereo `f32` clip.
    ///
    /// The number of frames rendered is `output.data().len() / 2`. Each event's
    /// timestamp is the sample-frame offset, within this block, at which it
    /// takes effect; events must be supplied in non-decreasing timestamp order.
    /// The output buffer is overwritten (not mixed into).
    pub fn render<I, C>(&mut self, events: I, output: &mut C)
    where
        I: IntoIterator<Item = (u64, MidiEvent)>,
        C: AudioClip<Value = f32>,
    {
        let buf = output.data_mut();
        buf.fill(0.0);
        let total_frames = buf.len() / 2;
        if total_frames == 0 {
            return;
        }

        let mut events = events.into_iter().peekable();
        let mut frame = 0usize;
        while frame < total_frames {
            // Apply every event due at or before the current frame.
            while let Some(&(ts, ev)) = events.peek() {
                if ts as usize <= frame {
                    self.handle_event(ev);
                    events.next();
                } else {
                    break;
                }
            }

            // Render up to the next event boundary (or the end of the block).
            let next = events
                .peek()
                .map(|&(ts, _)| (ts as usize).clamp(frame + 1, total_frames))
                .unwrap_or(total_frames);

            self.render_segment(&mut buf[frame * 2..next * 2]);
            frame = next;
        }

        self.voices.retain(|v| !v.is_finished());
    }

    /// Mixes all active voices into one contiguous interleaved-stereo segment.
    fn render_segment(&mut self, segment: &mut [f32]) {
        // Per-channel output gains are constant across the segment.
        let mut gains = [0.0f32; 16];
        for (g, c) in gains.iter_mut().zip(self.channels.iter()) {
            *g = c.gain();
        }

        let samples = &self.soundfont.samples;
        for voice in &mut self.voices {
            let g = gains[voice.channel as usize];
            let sample = &samples[voice.sample_index()];
            voice.render_additive(sample, segment, g, g);
        }
    }

    /// Dispatches a single MIDI event to the appropriate handler.
    fn handle_event(&mut self, event: MidiEvent) {
        match event {
            MidiEvent::NoteOn { channel, key, vel } if vel > 0 => self.note_on(channel, key, vel),
            // A note-on with zero velocity is a note-off.
            MidiEvent::NoteOn { channel, key, .. } | MidiEvent::NoteOff { channel, key } => {
                self.note_off(channel, key)
            }
            MidiEvent::ControlChange {
                channel,
                ctrl,
                value,
            } => self.control_change(channel, ctrl, value),
            MidiEvent::PitchBend { channel, value } => {
                if let Some(c) = self.channels.get_mut(channel as usize) {
                    c.bend = value;
                }
                let cents = self.channels[channel as usize & 15].bend_cents();
                for v in &mut self.voices {
                    if v.channel == channel {
                        v.set_bend_cents(cents);
                    }
                }
            }
            MidiEvent::ProgramChange { channel, program } => {
                if let Some(c) = self.channels.get_mut(channel as usize) {
                    c.program = program.index();
                }
            }
            MidiEvent::ChannelPressure { .. } | MidiEvent::PolyphonicKeyPressure { .. } => {}
            MidiEvent::AllNotesOff { channel } => {
                for v in &mut self.voices {
                    if v.channel == channel {
                        v.release();
                    }
                }
            }
            MidiEvent::AllSoundOff { channel } => {
                for v in &mut self.voices {
                    if v.channel == channel {
                        v.kill();
                    }
                }
            }
            MidiEvent::SystemReset => self.reset(),
        }
    }

    fn control_change(&mut self, channel: u8, ctrl: u8, value: u8) {
        let Some(c) = self.channels.get_mut(channel as usize) else {
            return;
        };
        match ctrl {
            0 => c.bank = value as u16, // Bank select (MSB).
            7 => c.volume = value,
            10 => c.pan = value,
            11 => c.expression = value,
            64 => {
                let down = value >= 64;
                c.sustain = down;
                if !down {
                    // Pedal up: release everything it was holding.
                    for v in &mut self.voices {
                        if v.channel == channel && v.held_by_pedal {
                            v.held_by_pedal = false;
                            v.release();
                        }
                    }
                }
            }
            120 => {
                for v in &mut self.voices {
                    if v.channel == channel {
                        v.kill();
                    }
                }
            }
            121 => {
                *c = ChannelState {
                    bank: c.bank,
                    program: c.program,
                    ..ChannelState::default()
                }
            }
            123 => {
                for v in &mut self.voices {
                    if v.channel == channel {
                        v.release();
                    }
                }
            }
            _ => {}
        }
    }

    fn note_off(&mut self, channel: u8, key: u8) {
        let sustained = self
            .channels
            .get(channel as usize)
            .map(|c| c.sustain)
            .unwrap_or(false);
        for v in &mut self.voices {
            if v.channel == channel && v.key == key && !v.held_by_pedal {
                if sustained {
                    v.held_by_pedal = true;
                } else {
                    v.release();
                }
            }
        }
    }

    fn note_on(&mut self, channel: u8, key: u8, vel: u8) {
        let params = self.resolve_voices(channel, key, vel);
        if params.is_empty() {
            return;
        }
        let bend_cents = self.channels[channel as usize & 15].bend_cents();
        for p in params {
            // Exclusive class: a new voice silences others of the same class on
            // the same channel (e.g. an open hi-hat cut by a closed one).
            if p.exclusive_class != 0 {
                for v in &mut self.voices {
                    if v.channel == channel && v.exclusive_class == p.exclusive_class {
                        v.kill();
                    }
                }
            }
            self.make_room();
            let sample_index = p.sample_index;
            let mut voice = Voice::new(p, sample_index);
            voice.set_bend_cents(bend_cents);
            self.voices.push(voice);
        }
    }

    /// Evicts a voice if we are at the polyphony limit, preferring already
    /// finished voices and otherwise the quietest one.
    fn make_room(&mut self) {
        if self.voices.len() < MAX_POLYPHONY {
            return;
        }
        if let Some(i) = self.voices.iter().position(|v| v.is_finished()) {
            self.voices.swap_remove(i);
            return;
        }
        if let Some((i, _)) = self
            .voices
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.loudness().total_cmp(&b.loudness()))
        {
            self.voices.swap_remove(i);
        }
    }

    /// Resolves a note-on into one [`VoiceParams`] per matching sample zone.
    fn resolve_voices(&self, channel: u8, key: u8, vel: u8) -> Vec<VoiceParams> {
        let sf = &self.soundfont;
        let chan = &self.channels[channel as usize & 15];
        let bank = if channel == DRUM_CHANNEL {
            DRUM_BANK
        } else {
            chan.bank
        };

        let Some(&preset_idx) = self
            .preset_index
            .get(&(bank, chan.program as u16))
            .or_else(|| self.preset_index.get(&(0, chan.program as u16)))
            .or_else(|| self.preset_index.get(&(bank, 0)))
        else {
            return Vec::new();
        };

        let preset = &sf.presets[preset_idx];
        let (preset_global, preset_local) = split_global(&preset.zones, G::INSTRUMENT);

        let mut out = Vec::new();
        for pz in preset_local {
            if !gens::zone_matches(pz, key, vel) {
                continue;
            }
            let Some(inst_idx) = gens::index_of(pz, G::INSTRUMENT) else {
                continue;
            };
            let Some(inst) = sf.instruments.get(inst_idx as usize) else {
                continue;
            };
            let (inst_global, inst_local) = split_global(&inst.zones, G::SAMPLE_ID);

            for iz in inst_local {
                if !gens::zone_matches(iz, key, vel) {
                    continue;
                }
                let Some(sample_idx) = gens::index_of(iz, G::SAMPLE_ID) else {
                    continue;
                };
                let Some(sample) = sf.samples.get(sample_idx as usize) else {
                    continue;
                };
                if !voice::is_playable(sample) {
                    continue;
                }

                // Merge: instrument zones are absolute, preset zones additive.
                let mut g = Gens::defaults();
                if let Some(zone) = inst_global {
                    g.apply_set(zone);
                }
                g.apply_set(iz);
                if let Some(zone) = preset_global {
                    g.apply_add(zone);
                }
                g.apply_add(pz);

                out.push(build_params(
                    &g,
                    sample,
                    sample_idx as usize,
                    channel,
                    key,
                    vel,
                    chan,
                    self.sample_rate,
                ));
            }
        }
        out
    }
}

/// Splits a zone list into its optional leading global zone and the rest.
///
/// A zone is *global* when it lacks `terminal` (the `Instrument` generator for
/// preset zones, the `SampleID` generator for instrument zones); only the first
/// zone may be global.
fn split_global(zones: &[Zone], terminal: G) -> (Option<&Zone>, &[Zone]) {
    match zones.first() {
        Some(first) if gens::index_of(first, terminal).is_none() => (Some(first), &zones[1..]),
        _ => (None, zones),
    }
}

/// Builds [`VoiceParams`] from a fully resolved generator set and its sample.
#[allow(clippy::too_many_arguments)]
fn build_params(
    g: &Gens,
    sample: &rameau_soundfont::Sample,
    sample_index: usize,
    channel: u8,
    key: u8,
    vel: u8,
    chan: &ChannelState,
    out_sample_rate: u32,
) -> VoiceParams {
    let data_len = sample.clip.data.len() as i64;

    // Address offsets: fine plus coarse (x32768), clamped into the clip.
    let clamp = |x: i64| x.clamp(0, data_len) as u32;
    let start = clamp(
        g.get(G::START_ADDRESS_OFFSET) as i64
            + g.get(G::START_ADDRESS_COARSE_OFFSET) as i64 * 32_768,
    );
    let end = clamp(
        data_len
            + g.get(G::END_ADDRESS_OFFSET) as i64
            + g.get(G::END_ADDRESS_COARSE_OFFSET) as i64 * 32_768,
    );
    let loop_start = clamp(
        sample.loop_start as i64
            + g.get(G::START_LOOP_ADDRESS_OFFSET) as i64
            + g.get(G::START_LOOP_ADDRESS_COARSE_OFFSET) as i64 * 32_768,
    );
    let loop_end = clamp(
        sample.loop_end as i64
            + g.get(G::END_LOOP_ADDRESS_OFFSET) as i64
            + g.get(G::END_LOOP_ADDRESS_COARSE_OFFSET) as i64 * 32_768,
    );

    // SampleModes: 1 and 3 loop; 0 and 2 play through once.
    let mode = g.get(G::SAMPLE_MODES) & 0x3;
    let looping = mode == 1 || mode == 3;

    let root = g.get(G::OVERRIDING_ROOT_KEY);
    let root_key = if root >= 0 {
        root as u8
    } else {
        sample.original_key
    };

    let tune_cents =
        g.getf(G::COARSE_TUNE) * 100.0 + g.getf(G::FINE_TUNE) + sample.correction as f32;

    // Initial attenuation in centibels -> linear gain, scaled by velocity.
    let att_gain = 10.0f32.powf(-g.getf(G::INITIAL_ATTENUATION).max(0.0) / 200.0);
    let amp = att_gain * (vel as f32 / 127.0);

    // Zone pan (0.1% units) combined with channel pan, clamped to [-1, 1].
    let zone_pan = g.getf(G::PAN) / 500.0;
    let pan = (zone_pan + chan.pan_unit()).clamp(-1.0, 1.0);

    VoiceParams {
        channel,
        key,
        exclusive_class: g.get(G::EXCLUSIVE_CLASS),
        out_sample_rate: out_sample_rate as f32,
        sample_rate: sample.clip.sample_rate as f32,
        root_key,
        scale_tuning: g.get(G::SCALE_TUNING),
        tune_cents,
        sample_index,
        start,
        end,
        loop_start,
        loop_end,
        looping,
        amp,
        pan,
        delay: g.getf(G::DELAY_VOLUME_ENVELOPE),
        attack: g.getf(G::ATTACK_VOLUME_ENVELOPE),
        hold: g.getf(G::HOLD_VOLUME_ENVELOPE),
        decay: g.getf(G::DECAY_VOLUME_ENVELOPE),
        sustain_cb: g.getf(G::SUSTAIN_VOLUME_ENVELOPE),
        release: g.getf(G::RELEASE_VOLUME_ENVELOPE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rameau_soundfont::{Generator, GeneratorAmount, Instrument, Preset, Sample};

    /// Builds a tiny one-preset SoundFont: a single looped sample mapped across
    /// the whole keyboard, recorded at MIDI key 60.
    fn test_soundfont() -> SoundFont {
        let data: Vec<i16> = (0..100)
            .map(|i| ((i as f32 * 0.2).sin() * 10_000.0) as i16)
            .collect();
        let sample = Sample {
            name: "sine".into(),
            clip: Clip::new(data, 44_100),
            loop_start: 10,
            loop_end: 90,
            original_key: 60,
            correction: 0,
            link: 0,
            kind: rameau_soundfont::SampleType::Mono,
        };

        let inst = Instrument {
            name: "inst".into(),
            zones: vec![Zone {
                generators: vec![
                    Generator {
                        kind: G::SAMPLE_MODES,
                        amount: GeneratorAmount::Short(1),
                    },
                    Generator {
                        kind: G::SAMPLE_ID,
                        amount: GeneratorAmount::Word(0),
                    },
                ],
                modulators: vec![],
            }],
        };

        let preset = Preset {
            name: "preset".into(),
            program: 0,
            bank: 0,
            zones: vec![Zone {
                generators: vec![Generator {
                    kind: G::INSTRUMENT,
                    amount: GeneratorAmount::Word(0),
                }],
                modulators: vec![],
            }],
            ..Default::default()
        };

        SoundFont {
            presets: vec![preset],
            instruments: vec![inst],
            samples: vec![sample],
            ..Default::default()
        }
    }

    fn rms(block: &Clip<f32>) -> f32 {
        let sum: f32 = block.data.iter().map(|s| s * s).sum();
        (sum / block.data.len() as f32).sqrt()
    }

    #[test]
    fn silent_without_events() {
        let mut synth = Synthesizer::new(test_soundfont(), 44_100);
        let mut block = Clip::new(vec![0.0f32; 256 * 2], 44_100);
        synth.render(std::iter::empty(), &mut block);
        assert_eq!(rms(&block), 0.0);
        assert_eq!(synth.active_voices(), 0);
    }

    #[test]
    fn note_on_produces_sound() {
        let mut synth = Synthesizer::new(test_soundfont(), 44_100);
        let mut block = Clip::new(vec![0.0f32; 1024 * 2], 44_100);
        synth.render(
            [(
                0u64,
                MidiEvent::NoteOn {
                    channel: 0,
                    key: 69,
                    vel: 127,
                },
            )],
            &mut block,
        );
        assert!(rms(&block) > 0.0, "a held note should produce audio");
        assert_eq!(synth.active_voices(), 1);
    }

    #[test]
    fn note_off_then_release_frees_the_voice() {
        let mut synth = Synthesizer::new(test_soundfont(), 8_000);
        let mut block = Clip::new(vec![0.0f32; 64 * 2], 8_000);
        synth.render(
            [(
                0u64,
                MidiEvent::NoteOn {
                    channel: 0,
                    key: 60,
                    vel: 100,
                },
            )],
            &mut block,
        );
        assert_eq!(synth.active_voices(), 1);
        // Release and let the (very short default) envelope run out.
        synth.render(
            [(
                0u64,
                MidiEvent::NoteOff {
                    channel: 0,
                    key: 60,
                },
            )],
            &mut block,
        );
        for _ in 0..200 {
            synth.render(std::iter::empty(), &mut block);
        }
        assert_eq!(synth.active_voices(), 0, "voice should free after release");
    }

    #[test]
    fn timestamps_delay_onset_within_a_block() {
        let mut synth = Synthesizer::new(test_soundfont(), 44_100);
        let frames = 512;
        let mut block = Clip::new(vec![0.0f32; frames * 2], 44_100);
        // Start halfway through the block.
        synth.render(
            [(
                (frames as u64) / 2,
                MidiEvent::NoteOn {
                    channel: 0,
                    key: 60,
                    vel: 127,
                },
            )],
            &mut block,
        );
        let first_half: f32 = block.data[..frames].iter().map(|s| s.abs()).sum();
        let second_half: f32 = block.data[frames..].iter().map(|s| s.abs()).sum();
        assert_eq!(first_half, 0.0, "no audio before the timestamp");
        assert!(second_half > 0.0, "audio after the timestamp");
    }
}
