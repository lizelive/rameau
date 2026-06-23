//! Offline rendering: bake a short MIDI score to a WAV file.
//!
//! The same [`Synthesizer::render`] entry point used for real-time playback
//! also renders offline — here the whole piece is one big block, so every event
//! timestamp is just its absolute sample position. This is the simplest way to
//! sanity-check a SoundFont and the synthesizer end to end.
//!
//! ```text
//! cargo run -p rameau_synthesizer --example render_score -- assets/FluidR3Mono_GM.sf3 out.wav
//! ```

use std::path::PathBuf;

use rameau_clip::Clip;
use rameau_midi::event::MidiEvent;
use rameau_soundfont::SoundFont;
use rameau_synthesizer::Synthesizer;

const SAMPLE_RATE: u32 = 44_100;

fn main() {
    let mut args = std::env::args().skip(1);
    let sf_path = args.next().unwrap_or_else(default_soundfont_path);
    let out_path = args.next().unwrap_or_else(|| "score.wav".to_string());

    let soundfont = match SoundFont::load_file(&sf_path) {
        Ok(sf) => sf,
        Err(e) => {
            eprintln!("failed to load SoundFont {sf_path:?}: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "loaded {:?}: {} presets, {} samples",
        soundfont.info.name.as_deref().unwrap_or("?"),
        soundfont.presets.len(),
        soundfont.samples.len()
    );

    let mut synth = Synthesizer::new(soundfont, SAMPLE_RATE);

    let events = score();
    // Total length: the last event plus two seconds of tail for releases.
    let last = events.iter().map(|&(t, _)| t).max().unwrap_or(0);
    let frames = last as usize + 2 * SAMPLE_RATE as usize;

    let mut stereo = Clip::new(vec![0.0f32; frames * 2], SAMPLE_RATE);
    synth.render(events, &mut stereo);

    // Mix the interleaved stereo render down to the mono i16 the WAV writer
    // expects, with simple peak normalisation and clipping protection.
    let peak = stereo
        .data
        .iter()
        .fold(0.0f32, |m, &s| m.max(s.abs()))
        .max(1e-6);
    let norm = 0.9 / peak;
    let mono: Vec<i16> = stereo
        .data
        .chunks_exact(2)
        .map(|lr| {
            let m = (lr[0] + lr[1]) * 0.5 * norm;
            (m.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
        })
        .collect();
    let clip = Clip::new(mono, SAMPLE_RATE);

    match rameau_wav::save(&clip, &out_path) {
        Ok(()) => println!(
            "wrote {out_path} ({:.1}s, peak {peak:.3})",
            frames as f32 / SAMPLE_RATE as f32
        ),
        Err(e) => {
            eprintln!("failed to write {out_path}: {e}");
            std::process::exit(1);
        }
    }
}

/// A two-bar piano cadence: a C major scale answered by a C major chord.
fn score() -> Vec<(u64, MidiEvent)> {
    let beat = SAMPLE_RATE as u64 / 2; // 120 BPM
    let mut events = Vec::new();

    // Program 0 (acoustic grand piano) on channel 0.
    events.push((
        0,
        MidiEvent::ProgramChange {
            channel: 0,
            program: 0.into(),
        },
    ));

    // Ascending C major scale, one note per eighth.
    let scale = [60, 62, 64, 65, 67, 69, 71, 72];
    for (i, &key) in scale.iter().enumerate() {
        let on = (i as u64 + 1) * (beat / 2);
        events.push((
            on,
            MidiEvent::NoteOn {
                channel: 0,
                key,
                vel: 100,
            },
        ));
        events.push((on + beat / 2, MidiEvent::NoteOff { channel: 0, key }));
    }

    // A held C major chord to finish.
    let chord_on = (scale.len() as u64 + 1) * (beat / 2);
    for &key in &[60, 64, 67, 72] {
        events.push((
            chord_on,
            MidiEvent::NoteOn {
                channel: 0,
                key,
                vel: 110,
            },
        ));
        events.push((chord_on + beat * 2, MidiEvent::NoteOff { channel: 0, key }));
    }

    events.sort_by_key(|&(t, _)| t);
    events
}

fn default_soundfont_path() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/FluidR3Mono_GM.sf3")
        .to_string_lossy()
        .into_owned()
}
