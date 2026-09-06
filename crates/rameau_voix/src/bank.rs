//! Packaging rendered vowels as a SoundFont bank.

use rameau_midi::event::MidiEvent;
use rameau_playback::{AudioPlayback, PlaybackError};
use rameau_soundfont::{
    Generator, GeneratorAmount, GeneratorType as G, Instrument, Preset, Range, Sample,
    SampleType, SoundFont, Zone,
};

use crate::synth::{RenderSpec, Singer, render};
use crate::vowel::Vowel;

/// The MIDI bank holding the solo singer presets.
pub const SOLO_BANK: u16 = 64;
/// The MIDI bank holding the choir presets.
pub const CHOIR_BANK: u16 = 65;

/// The registers a vowel is rendered at: the sample's root key and the key
/// range it covers.
const REGISTERS: [(u8, u8, u8, f32, f32); 3] = [
    // (root key, low key, high key, f0 Hz, formant scale)
    (45, 0, 51, 110.0, 0.94),
    (57, 52, 63, 220.0, 1.0),
    (69, 64, 127, 440.0, 1.16),
];

/// How to build the voice banks.
#[derive(Debug, Clone, PartialEq)]
pub struct VoixConfig {
    /// Sample rate of the rendered vowels.
    pub sample_rate: u32,
    /// Singers layered in the choir bank.
    pub choir_size: usize,
    /// Vibrato depth as a fraction of the fundamental.
    pub vibrato_depth: f32,
    /// Aspiration noise level.
    pub breath: f32,
    /// Seed for the choir's detuning and jitter.
    pub seed: u64,
    /// Whether to include the choir bank at all.
    pub choir: bool,
}

impl Default for VoixConfig {
    fn default() -> Self {
        Self {
            sample_rate: 32_000,
            choir_size: 5,
            vibrato_depth: 0.017,
            breath: 0.025,
            seed: 1789,
            choir: true,
        }
    }
}

/// Renders every vowel at every register and returns the two banks as one
/// [`SoundFont`] whose samples are clips of `backend`'s type.
///
/// # Errors
///
/// Returns [`PlaybackError`] if the backend cannot build a clip.
pub fn build_bank<P: AudioPlayback>(
    backend: &mut P,
    config: &VoixConfig,
) -> Result<SoundFont<P::Clip>, PlaybackError> {
    let mut sf = SoundFont::<P::Clip> {
        info: rameau_soundfont::Info {
            name: Some("rameau voix".to_owned()),
            comments: Some("formant-synthesized French vowels".to_owned()),
            ..Default::default()
        },
        presets: Vec::new(),
        instruments: Vec::new(),
        samples: Vec::new(),
    };
    let banks: Vec<(u16, usize, &str)> = if config.choir {
        vec![(SOLO_BANK, 1, "Voix"), (CHOIR_BANK, config.choir_size.max(2), "Chœur")]
    } else {
        vec![(SOLO_BANK, 1, "Voix")]
    };
    for (bank, voices, label) in banks {
        for vowel in Vowel::ALL {
            let mut inst = Instrument {
                name: format!("{label} {}", vowel.ipa()),
                zones: Vec::new(),
            };
            for (i, &(root, low, high, f0, scale)) in REGISTERS.iter().enumerate() {
                let singer = Singer {
                    f0,
                    formant_scale: scale,
                    vibrato_depth: config.vibrato_depth,
                    breath: config.breath,
                };
                let spec = RenderSpec {
                    sample_rate: config.sample_rate,
                    voices,
                    seed: config.seed.wrapping_add(i as u64 * 977 + bank as u64 * 31),
                    ..RenderSpec::default()
                };
                let r = render(vowel, singer, &spec);
                let clip = backend.clip_from_pcm(&r.pcm, r.sample_rate)?;
                let sample_index = sf.samples.len() as u16;
                sf.samples.push(Sample {
                    name: format!("{label} {} {root}", vowel.ipa()),
                    clip,
                    sample_rate: r.sample_rate,
                    frame_count: r.pcm.len() as u32,
                    loop_start: r.loop_start,
                    loop_end: r.loop_end,
                    original_key: root,
                    correction: 0,
                    link: 0,
                    kind: SampleType::Mono,
                });
                inst.zones.push(Zone {
                    generators: vec![
                        Generator {
                            kind: G::KEY_RANGE,
                            amount: GeneratorAmount::Range(Range { low, high }),
                        },
                        Generator {
                            kind: G::SAMPLE_MODES,
                            amount: GeneratorAmount::Short(1),
                        },
                        Generator {
                            kind: G::SAMPLE_ID,
                            amount: GeneratorAmount::Word(sample_index),
                        },
                    ],
                    modulators: Vec::new(),
                });
            }
            let inst_index = sf.instruments.len() as u16;
            sf.instruments.push(inst);
            sf.presets.push(Preset {
                name: format!("{label} [{}]", vowel.ipa()),
                program: vowel.program() as u16,
                bank,
                zones: vec![Zone {
                    generators: vec![Generator {
                        kind: G::INSTRUMENT,
                        amount: GeneratorAmount::Word(inst_index),
                    }],
                    modulators: Vec::new(),
                }],
                ..Default::default()
            });
        }
    }
    Ok(sf)
}

/// The two events that select `vowel` on `channel`: a bank select and a
/// program change. Send them before the note-on.
pub fn select_vowel(channel: u8, vowel: Vowel, choir: bool) -> [MidiEvent; 2] {
    let bank = if choir { CHOIR_BANK } else { SOLO_BANK };
    [
        MidiEvent::ControlChange {
            channel,
            ctrl: 0,
            value: bank as u8,
        },
        MidiEvent::ProgramChange {
            channel,
            program: vowel.program().into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rameau_clip::Clip;
    use rameau_playback::Timestamp;
    use rameau_software::Software;
    use rameau_synthesizer::Synthesizer;

    #[test]
    fn bank_sings_through_the_synthesizer() {
        let mut backend = Software::new(16_000);
        let config = VoixConfig {
            sample_rate: 16_000,
            choir_size: 2,
            ..VoixConfig::default()
        };
        let sf = build_bank(&mut backend, &config).unwrap();
        assert_eq!(sf.presets.len(), 26);
        assert_eq!(sf.samples.len(), 26 * 3);
        let mut synth = Synthesizer::new(sf, backend, 16_000);
        for e in select_vowel(0, Vowel::A, false) {
            synth.handle(Timestamp::Now, e).unwrap();
        }
        synth
            .handle(Timestamp::Now, MidiEvent::NoteOn { channel: 0, key: 62, vel: 100 })
            .unwrap();
        assert_eq!(synth.active_voices(), 1);
        let mut block = Clip::new(vec![0.0f32; 4_000 * 2], 16_000);
        synth.render(&mut block).unwrap();
        let rms = (block.data.iter().map(|s| s * s).sum::<f32>() / block.data.len() as f32).sqrt();
        assert!(rms > 0.01, "the vowel should sound, rms {rms}");
        // Choir on another channel.
        for e in select_vowel(1, Vowel::On, true) {
            synth.handle(Timestamp::Now, e).unwrap();
        }
        synth
            .handle(Timestamp::Now, MidiEvent::NoteOn { channel: 1, key: 45, vel: 100 })
            .unwrap();
        assert_eq!(synth.active_voices(), 2);
    }
}
