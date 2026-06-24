//! The polyphonic [`Synthesizer`] that drives an [`AudioPlayback`] backend.

use std::collections::HashMap;

use rameau_clip::AudioClip;
use rameau_midi::event::MidiEvent;
use rameau_playback::{AudioPlayback, LoopRegion, PlaybackError, Timestamp, Vec3, VoiceParams};
use rameau_soundfont::{GeneratorType as G, Sample, SoundFont, Zone};

use crate::gens::{self, Gens};

/// The MIDI channel reserved for percussion (channel 10, zero-based 9).
const DRUM_CHANNEL: u8 = 9;
/// The bank percussion presets live in for General MIDI banks.
const DRUM_BANK: u16 = 128;
/// Maximum simultaneously gated voices before the oldest is stolen.
const MAX_POLYPHONY: usize = 128;

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
    /// Combined volume × expression as a linear scalar in `0.0..=1.0`.
    fn gain(&self) -> f32 {
        (self.volume as f32 / 127.0) * (self.expression as f32 / 127.0)
    }

    /// Pitch bend in semitones, assuming the default ±2 semitone range.
    fn bend_semitones(&self) -> f32 {
        (self.bend as f32 - 8192.0) / 8192.0 * 2.0
    }

    /// Pan as a signed position in `-1.0..=1.0` (left..right).
    fn pan_unit(&self) -> f32 {
        (self.pan as f32 - 64.0) / 63.0
    }
}

/// One gated voice the synthesizer is currently driving on the backend.
///
/// It caches the note-static parts of the voice's parameters so that controller
/// changes (volume, pan, pitch bend) can be folded with the live channel state
/// and pushed to the backend via [`AudioPlayback::update`].
struct Live<P: AudioPlayback> {
    channel: u8,
    key: u8,
    exclusive_class: i32,
    /// Held down by the sustain pedal: a note-off is deferred until release.
    held_by_pedal: bool,
    handle: P::Playback,

    /// Pitch in semitones, excluding pitch bend (keyboard tracking + tuning).
    base_pitch: f32,
    /// Velocity 0..=127.
    vel: u8,
    /// Static attenuation gain (pre velocity and channel volume).
    att_gain: f32,
    /// Zone pan in `-1.0..=1.0`, before the channel pan.
    zone_pan: f32,
}

impl<P: AudioPlayback> Live<P> {
    /// Computes the voice's current [`VoiceParams`] from the channel state.
    fn params(&self, chan: &ChannelState) -> VoiceParams {
        params_of(self.base_pitch, self.vel, self.att_gain, self.zone_pan, chan)
    }
}

/// Folds a voice's static parameters with live channel state.
fn params_of(
    base_pitch: f32,
    vel: u8,
    att_gain: f32,
    zone_pan: f32,
    chan: &ChannelState,
) -> VoiceParams {
    VoiceParams {
        pitch: base_pitch + chan.bend_semitones(),
        volume: att_gain * (vel as f32 / 127.0) * chan.gain(),
        position: Vec3::pan((zone_pan + chan.pan_unit()).clamp(-1.0, 1.0)),
        velocity: Vec3::default(),
    }
}

/// A polyphonic SoundFont synthesizer driving an [`AudioPlayback`] backend.
pub struct Synthesizer<P: AudioPlayback> {
    soundfont: SoundFont<P::Clip>,
    backend: P,
    sample_rate: u32,
    channels: [ChannelState; 16],
    voices: Vec<Live<P>>,
    /// `(bank, program)` -> index into `soundfont.presets`.
    preset_index: HashMap<(u16, u16), usize>,
}

impl<P: AudioPlayback> Synthesizer<P> {
    /// Creates a synthesizer that plays `soundfont` through `backend`.
    ///
    /// `sample_rate` is the rate the backend renders at.
    pub fn new(soundfont: SoundFont<P::Clip>, backend: P, sample_rate: u32) -> Self {
        let mut preset_index = HashMap::new();
        for (i, preset) in soundfont.presets.iter().enumerate() {
            preset_index
                .entry((preset.bank, preset.program))
                .or_insert(i);
        }
        Self {
            soundfont,
            backend,
            sample_rate,
            channels: [ChannelState::default(); 16],
            voices: Vec::new(),
            preset_index,
        }
    }

    /// The render sample rate in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Number of voices currently gated.
    pub fn active_voices(&self) -> usize {
        self.voices.len()
    }

    /// The SoundFont this synthesizer plays.
    pub fn soundfont(&self) -> &SoundFont<P::Clip> {
        &self.soundfont
    }

    /// Mutable access to the playback backend.
    pub fn backend(&mut self) -> &mut P {
        &mut self.backend
    }

    /// Consumes the synthesizer and returns its backend.
    pub fn into_backend(self) -> P {
        self.backend
    }

    /// Renders the backend's output offline into `clip` (interleaved stereo).
    ///
    /// Forwards to [`AudioPlayback::render`]; real-time-only backends return
    /// [`PlaybackError::Unsupported`].
    pub fn render(&mut self, clip: &mut impl AudioClip<Value = f32>) -> Result<(), PlaybackError> {
        self.backend.render(clip)
    }

    /// Applies a sequence of timestamped events in order.
    pub fn play<I>(&mut self, events: I) -> Result<(), PlaybackError>
    where
        I: IntoIterator<Item = (Timestamp, MidiEvent)>,
    {
        for (when, event) in events {
            self.handle(when, event)?;
        }
        Ok(())
    }

    /// Stops every voice immediately and resets controllers.
    pub fn reset(&mut self, when: Timestamp) -> Result<(), PlaybackError> {
        for mut v in std::mem::take(&mut self.voices) {
            self.backend.stop(when, &mut v.handle)?;
        }
        self.channels = [ChannelState::default(); 16];
        Ok(())
    }

    /// Applies a single MIDI event at time `when`.
    pub fn handle(&mut self, when: Timestamp, event: MidiEvent) -> Result<(), PlaybackError> {
        match event {
            MidiEvent::NoteOn { channel, key, vel } if vel > 0 => {
                self.note_on(when, channel, key, vel)
            }
            MidiEvent::NoteOn { channel, key, .. } | MidiEvent::NoteOff { channel, key, .. } => {
                self.note_off(when, channel, key)
            }
            MidiEvent::ControlChange {
                channel,
                ctrl,
                value,
            } => self.control_change(when, channel, ctrl, value),
            MidiEvent::PitchBend { channel, value } => {
                if let Some(c) = self.channels.get_mut(channel as usize) {
                    c.bend = value;
                }
                self.update_channel(when, channel)
            }
            MidiEvent::ProgramChange { channel, program } => {
                if let Some(c) = self.channels.get_mut(channel as usize) {
                    c.program = program.index();
                }
                Ok(())
            }
            MidiEvent::ChannelPressure { .. } | MidiEvent::PolyphonicKeyPressure { .. } => Ok(()),
            MidiEvent::AllNotesOff { channel } | MidiEvent::AllSoundOff { channel } => {
                self.stop_channel(when, channel)
            }
            MidiEvent::SystemReset => self.reset(when),
        }
    }

    fn control_change(
        &mut self,
        when: Timestamp,
        channel: u8,
        ctrl: u8,
        value: u8,
    ) -> Result<(), PlaybackError> {
        let Some(c) = self.channels.get_mut(channel as usize) else {
            return Ok(());
        };
        match ctrl {
            0 => c.bank = value as u16, // Bank select (MSB).
            7 => {
                c.volume = value;
                return self.update_channel(when, channel);
            }
            10 => {
                c.pan = value;
                return self.update_channel(when, channel);
            }
            11 => {
                c.expression = value;
                return self.update_channel(when, channel);
            }
            64 => {
                let down = value >= 64;
                c.sustain = down;
                if !down {
                    // Pedal up: release everything it was holding.
                    return self.release_pedal(when, channel);
                }
            }
            // All sound off / all notes off.
            120 | 123 => return self.stop_channel(when, channel),
            121 => {
                // Reset controllers (keep bank/program).
                *c = ChannelState {
                    bank: c.bank,
                    program: c.program,
                    ..ChannelState::default()
                };
                return self.update_channel(when, channel);
            }
            _ => {}
        }
        Ok(())
    }

    /// Pushes the current channel state to every gated voice on `channel`.
    fn update_channel(&mut self, when: Timestamp, channel: u8) -> Result<(), PlaybackError> {
        let chan = self.channels[channel as usize & 15];
        for v in &mut self.voices {
            if v.channel == channel {
                let params = v.params(&chan);
                self.backend.update(when, &mut v.handle, params)?;
            }
        }
        Ok(())
    }

    /// Stops and forgets every voice on `channel`.
    fn stop_channel(&mut self, when: Timestamp, channel: u8) -> Result<(), PlaybackError> {
        let mut kept = Vec::with_capacity(self.voices.len());
        for mut v in std::mem::take(&mut self.voices) {
            if v.channel == channel {
                self.backend.stop(when, &mut v.handle)?;
            } else {
                kept.push(v);
            }
        }
        self.voices = kept;
        Ok(())
    }

    /// Releases voices held by the sustain pedal on `channel`.
    fn release_pedal(&mut self, when: Timestamp, channel: u8) -> Result<(), PlaybackError> {
        let mut kept = Vec::with_capacity(self.voices.len());
        for mut v in std::mem::take(&mut self.voices) {
            if v.channel == channel && v.held_by_pedal {
                self.backend.stop(when, &mut v.handle)?;
            } else {
                kept.push(v);
            }
        }
        self.voices = kept;
        Ok(())
    }

    fn note_off(&mut self, when: Timestamp, channel: u8, key: u8) -> Result<(), PlaybackError> {
        let sustained = self
            .channels
            .get(channel as usize)
            .map(|c| c.sustain)
            .unwrap_or(false);

        let mut kept = Vec::with_capacity(self.voices.len());
        for mut v in std::mem::take(&mut self.voices) {
            if v.channel == channel && v.key == key && !v.held_by_pedal {
                if sustained {
                    v.held_by_pedal = true;
                    kept.push(v);
                } else {
                    self.backend.stop(when, &mut v.handle)?;
                }
            } else {
                kept.push(v);
            }
        }
        self.voices = kept;
        Ok(())
    }

    fn note_on(
        &mut self,
        when: Timestamp,
        channel: u8,
        key: u8,
        vel: u8,
    ) -> Result<(), PlaybackError> {
        let resolved = self.resolve_voices(channel, key, vel);
        if resolved.is_empty() {
            return Ok(());
        }
        let chan = self.channels[channel as usize & 15];

        for r in resolved {
            // Exclusive class: cut other voices of the same class on this channel.
            if r.exclusive_class != 0 {
                self.cut_exclusive(when, channel, r.exclusive_class)?;
            }
            self.make_room(when)?;

            let params = params_of(r.base_pitch, vel, r.att_gain, r.zone_pan, &chan);
            // Disjoint field borrows: `clip` reads `soundfont`, `start` takes
            // `&mut backend`.
            let clip = &self.soundfont.samples[r.sample_index].clip;
            let handle = self
                .backend
                .start(when, clip, params, r.loop_region.clone())?;

            self.voices.push(Live {
                channel,
                key,
                exclusive_class: r.exclusive_class,
                held_by_pedal: false,
                handle,
                base_pitch: r.base_pitch,
                vel,
                att_gain: r.att_gain,
                zone_pan: r.zone_pan,
            });
        }
        Ok(())
    }

    /// Stops voices on `channel` sharing `class` (an exclusive-class cut-off).
    fn cut_exclusive(
        &mut self,
        when: Timestamp,
        channel: u8,
        class: i32,
    ) -> Result<(), PlaybackError> {
        let mut kept = Vec::with_capacity(self.voices.len());
        for mut v in std::mem::take(&mut self.voices) {
            if v.channel == channel && v.exclusive_class == class {
                self.backend.stop(when, &mut v.handle)?;
            } else {
                kept.push(v);
            }
        }
        self.voices = kept;
        Ok(())
    }

    /// Steals the oldest voice if we are at the polyphony limit.
    fn make_room(&mut self, when: Timestamp) -> Result<(), PlaybackError> {
        if self.voices.len() < MAX_POLYPHONY {
            return Ok(());
        }
        let mut v = self.voices.remove(0);
        self.backend.stop(when, &mut v.handle)?;
        Ok(())
    }

    /// Resolves a note-on into one [`Resolved`] per matching sample zone.
    fn resolve_voices(&self, channel: u8, key: u8, vel: u8) -> Vec<Resolved> {
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
                if !is_playable(sample) {
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

                out.push(build_resolved(&g, sample, sample_idx as usize, key));
            }
        }
        out
    }
}

/// A resolved note-voice: enough to call [`AudioPlayback::start`].
struct Resolved {
    sample_index: usize,
    exclusive_class: i32,
    base_pitch: f32,
    att_gain: f32,
    zone_pan: f32,
    loop_region: Option<LoopRegion>,
}

/// Whether a sample carries usable PCM (not a ROM sample).
fn is_playable<C>(sample: &Sample<C>) -> bool {
    use rameau_soundfont::SampleType::*;
    !matches!(sample.kind, RomMono | RomRight | RomLeft | RomLinked)
}

/// Splits a zone list into its optional leading global zone and the rest.
fn split_global(zones: &[Zone], terminal: G) -> (Option<&Zone>, &[Zone]) {
    match zones.first() {
        Some(first) if gens::index_of(first, terminal).is_none() => (Some(first), &zones[1..]),
        _ => (None, zones),
    }
}

/// Builds [`Resolved`] from a fully merged generator set and its sample.
fn build_resolved<C>(g: &Gens, sample: &Sample<C>, sample_index: usize, key: u8) -> Resolved {
    let root = g.get(G::OVERRIDING_ROOT_KEY);
    let root_key = if root >= 0 {
        root as u8
    } else {
        sample.original_key
    };

    // Pitch in semitones: keyboard tracking (scale_tuning cents/key) plus the
    // fixed tuning (coarse/fine generators and sample correction).
    let scale_tuning = g.get(G::SCALE_TUNING);
    let key_cents = (key as f32 - root_key as f32) * scale_tuning as f32;
    let tune_cents =
        g.getf(G::COARSE_TUNE) * 100.0 + g.getf(G::FINE_TUNE) + sample.correction as f32;
    let base_pitch = (key_cents + tune_cents) / 100.0;

    // Initial attenuation in centibels -> linear gain.
    let att_gain = 10.0f32.powf(-g.getf(G::INITIAL_ATTENUATION).max(0.0) / 200.0);

    // Zone pan in 0.1% units -> -1.0..=1.0.
    let zone_pan = (g.getf(G::PAN) / 500.0).clamp(-1.0, 1.0);

    // SampleModes: 1 and 3 loop; 0 and 2 play through once.
    let mode = g.get(G::SAMPLE_MODES) & 0x3;
    let looping = mode == 1 || mode == 3;
    let loop_start = (sample.loop_start as i64
        + g.get(G::START_LOOP_ADDRESS_OFFSET) as i64
        + g.get(G::START_LOOP_ADDRESS_COARSE_OFFSET) as i64 * 32_768)
        .max(0) as u32;
    let loop_end = (sample.loop_end as i64
        + g.get(G::END_LOOP_ADDRESS_OFFSET) as i64
        + g.get(G::END_LOOP_ADDRESS_COARSE_OFFSET) as i64 * 32_768)
        .max(0) as u32;
    let loop_region = if looping && loop_end > loop_start {
        Some(loop_start..loop_end)
    } else {
        None
    };

    Resolved {
        sample_index,
        exclusive_class: g.get(G::EXCLUSIVE_CLASS),
        base_pitch,
        att_gain,
        zone_pan,
        loop_region,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rameau_soundfont::{Generator, GeneratorAmount, Instrument, Preset, Sample};
    use rameau_software::Software;

    /// A tiny one-preset SoundFont: a looped sine across the keyboard, key 60.
    fn test_soundfont(backend: &mut Software) -> SoundFont<<Software as AudioPlayback>::Clip> {
        let data: Vec<i16> = (0..100)
            .map(|i| ((i as f32 * 0.2).sin() * 10_000.0) as i16)
            .collect();
        let clip = backend.clip_from_pcm(&data, 44_100).unwrap();
        let sample = Sample {
            name: "sine".into(),
            clip,
            sample_rate: 44_100,
            frame_count: 100,
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

    fn rms(buf: &[f32]) -> f32 {
        let sum: f32 = buf.iter().map(|s| s * s).sum();
        (sum / buf.len() as f32).sqrt()
    }

    #[test]
    fn note_on_produces_sound_and_note_off_frees() {
        let mut backend = Software::new(44_100).with_envelope(0.0, 0.005);
        let sf = test_soundfont(&mut backend);
        let mut synth = Synthesizer::new(sf, backend, 44_100);

        synth
            .handle(Timestamp::Now, MidiEvent::NoteOn { channel: 0, key: 69, vel: 127 })
            .unwrap();
        assert_eq!(synth.active_voices(), 1);

        let mut block = rameau_clip::Clip::new(vec![0.0f32; 512 * 2], 44_100);
        synth.render(&mut block).unwrap();
        assert!(rms(&block.data) > 0.0, "a held note should produce audio");

        synth
            .handle(Timestamp::Now, MidiEvent::NoteOff { channel: 0, key: 69, vel: 0 })
            .unwrap();
        assert_eq!(synth.active_voices(), 0, "note-off forgets the voice");
        // Let the backend's release run out.
        for _ in 0..50 {
            synth.render(&mut block).unwrap();
        }
        assert_eq!(synth.backend().active_voices(), 0);
    }

    #[test]
    fn scheduled_offline_render_places_notes_in_time() {
        // Drive the software backend offline: schedule a note one second in and
        // confirm the first second is silent, the second is not.
        let mut backend = Software::new(1_000);
        let sf = test_soundfont(&mut backend);
        let mut synth = Synthesizer::new(sf, backend, 1_000);
        synth
            .handle(
                Timestamp::AtSeconds(1.0),
                MidiEvent::NoteOn { channel: 0, key: 60, vel: 110 },
            )
            .unwrap();
        let mut block = rameau_clip::Clip::new(vec![0.0f32; 1_000 * 2], 1_000);
        synth.render(&mut block).unwrap();
        assert_eq!(rms(&block.data), 0.0, "silent before the scheduled note");
        synth.render(&mut block).unwrap();
        assert!(rms(&block.data) > 0.0, "audible after the scheduled note");
    }
}
