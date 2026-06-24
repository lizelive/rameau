//! Offline rendering: bake a short MIDI score to a WAV file.
//!
//! The [`rameau_software::Software`] backend is a pure mixer with no output
//! device, which makes it the one that can render *offline*: every note is
//! scheduled at its absolute [`Timestamp::AtSeconds`], then the whole piece is
//! rendered in a single [`Synthesizer::render`] call.
//!
//! ```text
//! cargo run -p rameau_synthesizer --example render_score -- assets/FluidR3Mono_GM.sf3 out.wav
//! ```

use std::path::PathBuf;

use rameau_clip::Clip;
use rameau_midi::event::MidiEvent;
use rameau_playback::Timestamp;
use rameau_software::Software;
use rameau_soundfont::SoundFont;
use rameau_synthesizer::Synthesizer;

const SAMPLE_RATE: u32 = 44_100;

fn main() {
    let mut args = std::env::args().skip(1);
    let sf_path = args.next().unwrap_or_else(default_soundfont_path);
    let out_path = args.next().unwrap_or_else(|| "score.wav".to_string());

    let mut backend = Software::new(SAMPLE_RATE);
    let soundfont = match SoundFont::load_file_with(&sf_path, &mut backend) {
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

    let mut synth = Synthesizer::new(soundfont, backend, SAMPLE_RATE);

    // Score events are (absolute sample position, event); schedule each in time.
    let events = score();
    let last = events.iter().map(|&(t, _)| t).max().unwrap_or(0);
    let frames = last as usize + 2 * SAMPLE_RATE as usize; // 2s tail for releases.
    for (pos, event) in &events {
        let when = Timestamp::AtSeconds(*pos as f64 / SAMPLE_RATE as f64);
        synth
            .handle(when, *event)
            .expect("software backend is infallible");
    }

    // One big offline block: the software backend honours each note's schedule.
    let mut stereo = Clip::new(vec![0.0f32; frames * 2], SAMPLE_RATE);
    synth.render(&mut stereo).expect("software render");

    // Mix the interleaved stereo render down to mono i16 with peak normalisation.
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

    events.push((
        0,
        MidiEvent::ProgramChange {
            channel: 0,
            program: 0.into(),
        },
    ));

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
        events.push((
            on + beat / 2,
            MidiEvent::NoteOff {
                channel: 0,
                key,
                vel: 0,
            },
        ));
    }

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
        events.push((
            chord_on + beat * 2,
            MidiEvent::NoteOff {
                channel: 0,
                key,
                vel: 0,
            },
        ));
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
