//! Real-time interactive scoring + SoundFont synthesis.
//!
//! This demo wires [`Synthesizer`] to a live audio device through
//! [`rameau_tinyaudio`] and shows the two halves of "interactive scoring":
//!
//! * **A score that plays itself.** A looping I-V-vi-IV chord progression with a
//!   bass line is scheduled on a sample-accurate timeline. The audio callback
//!   owns the clock: each block it gathers the score events that fall inside the
//!   block and hands them to [`Synthesizer::render`] with block-relative
//!   timestamps, so timing never drifts with buffer size.
//!
//! * **Live playing on top.** You type notes at the terminal and they are mixed
//!   in immediately, transposed and routed to whatever instrument is selected.
//!
//! Run it with an optional SoundFont path (a General MIDI bank works best):
//!
//! ```text
//! cargo run -p rameau_synthesizer --example interactive -- assets/FluidR3Mono_GM.sf3
//! ```
//!
//! Then type and press Enter:
//!
//! ```text
//!   a s d f g h j k   -> play the C major scale (one octave up: q w e r t y u i)
//!   +  /  -           -> transpose up / down a semitone
//!   p <0-127>         -> change the live instrument (General MIDI program)
//!   space             -> toggle the backing score on/off
//!   .                 -> panic (all notes off)
//!   q                 -> quit
//! ```

use std::io::BufRead;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use rameau_midi::event::MidiEvent;
use rameau_playback::{Playback, PlaybackConfig};
use rameau_soundfont::SoundFont;
use rameau_synthesizer::{Clip, Synthesizer};
use rameau_tinyaudio::TinyAudio;

/// A command sent from the keyboard thread to the audio thread.
enum Command {
    /// Play `key` for `gate` seconds at `vel` on the live channel.
    Note { key: u8, vel: u8, gate: f32 },
    /// Select the live channel's General MIDI program.
    Program(u8),
    /// Shift all played pitches by `semitones`.
    Transpose(i8),
    /// Turn the backing score on or off.
    ToggleScore,
    /// All notes off, everywhere.
    Panic,
}

/// The MIDI channel the backing score plays on.
const SCORE_CHANNEL: u8 = 0;
/// The MIDI channel live keyboard input plays on.
const LIVE_CHANNEL: u8 = 1;

fn main() {
    let config = PlaybackConfig {
        channels: 2,
        sample_rate: 48_000,
        // ~5 ms blocks: low latency while staying comfortably real-time.
        frames_per_buffer: 256,
    };

    let soundfont = match load_soundfont() {
        Ok(sf) => sf,
        Err(e) => {
            eprintln!("could not load a SoundFont: {e}");
            eprintln!("pass one explicitly, e.g.:");
            eprintln!(
                "  cargo run -p rameau_synthesizer --example interactive -- path/to/bank.sf2"
            );
            std::process::exit(1);
        }
    };
    println!(
        "loaded \"{}\" ({} presets, {} samples)",
        soundfont.info.name.as_deref().unwrap_or("unnamed"),
        soundfont.presets.len(),
        soundfont.samples.len()
    );

    let mut synth = Synthesizer::new(soundfont, config.sample_rate);
    // Give the live channel a bright lead so it stands out over the score.
    let mut bootstrap = vec![program_change(LIVE_CHANNEL, 80)]; // Lead 1 (square)
    bootstrap.push(program_change(SCORE_CHANNEL, 0)); // Acoustic grand piano

    let sample_rate = config.sample_rate;
    let score = build_score(sample_rate);
    let loop_samples = score_loop_samples(sample_rate);

    let (tx, rx) = mpsc::channel::<Command>();

    // Everything below is moved into the real-time audio callback.
    let mut scratch = Clip::new(vec![0.0f32; config.buffer_len()], sample_rate);
    let mut clock: u64 = 0; // absolute sample time
    let mut score_idx = 0usize; // next score event
    let mut loop_base: u64 = 0; // sample time of the current score-loop start
    let mut pending: Vec<(u64, MidiEvent)> = Vec::new(); // future note-offs
    let mut transpose: i32 = 0;
    let mut score_on = true;

    // Apply the bootstrap program changes on the very first block.
    let mut bootstrap = std::mem::take(&mut bootstrap);

    let render = move |buf: &mut [f32]| {
        let frames = (buf.len() / 2) as u64;
        let block_start = clock;
        let block_end = clock + frames;

        let mut events: Vec<(u64, MidiEvent)> = Vec::new();

        for ev in bootstrap.drain(..) {
            events.push((0, ev));
        }

        // (a) Backing score, looped on the sample timeline.
        if score_on {
            collect_score(
                &score,
                loop_samples,
                block_start,
                block_end,
                &mut score_idx,
                &mut loop_base,
                transpose,
                &mut events,
            );
        }

        // (b) Note-offs scheduled by earlier live notes.
        pending.retain(|&(t, ev)| {
            if t < block_end {
                events.push((t.saturating_sub(block_start), ev));
                false
            } else {
                true
            }
        });

        // (c) Live commands from the keyboard thread.
        drain_commands(
            &rx,
            block_start,
            sample_rate,
            &mut transpose,
            &mut score_on,
            &mut events,
            &mut pending,
        );

        events.sort_by_key(|&(t, _)| t);

        scratch.data.resize(buf.len(), 0.0);
        synth.render(events, &mut scratch);
        buf.copy_from_slice(&scratch.data);

        clock = block_end;
    };

    let _stream = match TinyAudio.open(config, render) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("could not open audio device: {e}");
            std::process::exit(1);
        }
    };

    print_help();
    run_keyboard(tx);
    println!("bye");
}

/// Reads lines from stdin and turns them into [`Command`]s until "q".
fn run_keyboard(tx: mpsc::Sender<Command>) {
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix('p') {
            if let Ok(prog) = rest.trim().parse::<u8>() {
                let _ = tx.send(Command::Program(prog.min(127)));
                println!("  program -> {prog}");
            }
            continue;
        }

        match line {
            "q" | "quit" => break,
            " " | "space" => {
                let _ = tx.send(Command::ToggleScore);
            }
            "+" => {
                let _ = tx.send(Command::Transpose(1));
                println!("  transpose +1");
            }
            "-" => {
                let _ = tx.send(Command::Transpose(-1));
                println!("  transpose -1");
            }
            "." => {
                let _ = tx.send(Command::Panic);
                println!("  panic");
            }
            _ => {
                let mut any = false;
                for ch in line.chars() {
                    if ch == ' ' {
                        let _ = tx.send(Command::ToggleScore);
                        continue;
                    }
                    if let Some(key) = key_for_char(ch) {
                        let _ = tx.send(Command::Note {
                            key,
                            vel: 100,
                            gate: 0.4,
                        });
                        any = true;
                    }
                }
                if any {
                    println!("  {line}");
                }
            }
        }
    }
}

/// Maps a typing-keyboard character to a MIDI key (two rows = two octaves).
fn key_for_char(ch: char) -> Option<u8> {
    // Home row: C major scale from C4 (60). Top row: the octave above.
    const LOWER: &[(char, u8)] = &[
        ('a', 60),
        ('s', 62),
        ('d', 64),
        ('f', 65),
        ('g', 67),
        ('h', 69),
        ('j', 71),
        ('k', 72),
    ];
    const UPPER: &[(char, u8)] = &[
        ('q', 72),
        ('w', 74),
        ('e', 76),
        ('r', 77),
        ('t', 79),
        ('y', 81),
        ('u', 83),
        ('i', 84),
    ];
    LOWER
        .iter()
        .chain(UPPER)
        .find(|&&(c, _)| c == ch)
        .map(|&(_, k)| k)
}

/// Drains keyboard commands into this block's event list (and `pending`).
#[allow(clippy::too_many_arguments)]
fn drain_commands(
    rx: &Receiver<Command>,
    block_start: u64,
    sample_rate: u32,
    transpose: &mut i32,
    score_on: &mut bool,
    events: &mut Vec<(u64, MidiEvent)>,
    pending: &mut Vec<(u64, MidiEvent)>,
) {
    while let Ok(cmd) = rx.try_recv() {
        match cmd {
            Command::Note { key, vel, gate } => {
                let key = (key as i32 + *transpose).clamp(0, 127) as u8;
                events.push((
                    0,
                    MidiEvent::NoteOn {
                        channel: LIVE_CHANNEL,
                        key,
                        vel,
                    },
                ));
                let off_at = block_start + (gate * sample_rate as f32) as u64;
                pending.push((
                    off_at,
                    MidiEvent::NoteOff {
                        channel: LIVE_CHANNEL,
                        key,
                    },
                ));
            }
            Command::Program(p) => events.push((0, program_change(LIVE_CHANNEL, p))),
            Command::Transpose(d) => *transpose += d as i32,
            Command::ToggleScore => *score_on = !*score_on,
            Command::Panic => {
                for ch in 0..16 {
                    events.push((0, MidiEvent::AllNotesOff { channel: ch }));
                }
            }
        }
    }
}

/// Collects score events overlapping `[block_start, block_end)`, looping the
/// score and advancing `score_idx` / `loop_base` as the timeline crosses it.
#[allow(clippy::too_many_arguments)]
fn collect_score(
    score: &[(u64, MidiEvent)],
    loop_samples: u64,
    block_start: u64,
    block_end: u64,
    score_idx: &mut usize,
    loop_base: &mut u64,
    transpose: i32,
    events: &mut Vec<(u64, MidiEvent)>,
) {
    loop {
        if *score_idx >= score.len() {
            // Wrap to the next loop iteration.
            *score_idx = 0;
            *loop_base += loop_samples;
        }
        let (rel, ev) = score[*score_idx];
        let abs = *loop_base + rel;
        if abs >= block_end {
            break;
        }
        if abs >= block_start {
            events.push((abs - block_start, transpose_event(ev, transpose)));
        }
        *score_idx += 1;
    }
}

/// Applies a transpose to note events; leaves other events untouched.
fn transpose_event(ev: MidiEvent, semitones: i32) -> MidiEvent {
    match ev {
        MidiEvent::NoteOn { channel, key, vel } => MidiEvent::NoteOn {
            channel,
            key: (key as i32 + semitones).clamp(0, 127) as u8,
            vel,
        },
        MidiEvent::NoteOff { channel, key } => MidiEvent::NoteOff {
            channel,
            key: (key as i32 + semitones).clamp(0, 127) as u8,
        },
        other => other,
    }
}

fn program_change(channel: u8, program: u8) -> MidiEvent {
    MidiEvent::ProgramChange {
        channel,
        program: program.into(),
    }
}

/// Length of one loop of the backing score, in samples (4 bars at 100 BPM).
fn score_loop_samples(sample_rate: u32) -> u64 {
    let beats = 16.0; // 4 bars of 4/4
    (beats * 60.0 / 100.0 * sample_rate as f64) as u64
}

/// Builds a looping I-V-vi-IV progression in C with a root bass line.
///
/// Returns `(sample_offset, event)` pairs sorted by time, relative to the start
/// of the loop.
fn build_score(sample_rate: u32) -> Vec<(u64, MidiEvent)> {
    let beat = (60.0 / 100.0 * sample_rate as f64) as u64; // one beat in samples
    let bar = beat * 4;

    // Chord roots (C major: C, G, A minor, F) and their triads.
    let chords: [[u8; 3]; 4] = [
        [60, 64, 67], // C  (I)
        [55, 62, 67], // G  (V)
        [57, 60, 64], // Am (vi)
        [53, 57, 60], // F  (IV)
    ];
    let bass: [u8; 4] = [36, 31, 33, 29];

    let mut score: Vec<(u64, MidiEvent)> = Vec::new();
    for (i, (chord, &root)) in chords.iter().zip(bass.iter()).enumerate() {
        let t0 = i as u64 * bar;

        // Pad chord: held for almost the whole bar.
        for &key in chord {
            score.push((t0, note_on(SCORE_CHANNEL, key, 70)));
            score.push((t0 + bar - beat / 4, note_off(SCORE_CHANNEL, key)));
        }

        // Bass: a note on each beat.
        for b in 0..4 {
            let t = t0 + b * beat;
            score.push((t, note_on(LIVE_CHANNEL + 1, root, 90)));
            score.push((t + beat - beat / 8, note_off(LIVE_CHANNEL + 1, root)));
        }
    }

    score.sort_by_key(|&(t, _)| t);
    score
}

fn note_on(channel: u8, key: u8, vel: u8) -> MidiEvent {
    MidiEvent::NoteOn { channel, key, vel }
}

fn note_off(channel: u8, key: u8) -> MidiEvent {
    MidiEvent::NoteOff { channel, key }
}

/// Tries the CLI argument, then the bundled banks under `assets/`.
fn load_soundfont() -> Result<SoundFont, rameau_soundfont::Error> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(arg) = std::env::args().nth(1) {
        candidates.push(PathBuf::from(arg));
    }
    // `assets/` sits at the workspace root, two levels above this crate.
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    candidates.push(assets.join("FluidR3Mono_GM.sf3"));
    candidates.push(assets.join("Unison.SF2"));
    candidates.push(PathBuf::from("assets/FluidR3Mono_GM.sf3"));

    let mut last_err = None;
    for path in candidates {
        if !path.exists() {
            continue;
        }
        match SoundFont::load_file(&path) {
            Ok(sf) => return Ok(sf),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        rameau_soundfont::Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no SoundFont found in assets/ and none given on the command line",
        ))
    }))
}

fn print_help() {
    println!();
    println!("interactive scoring + synth — type and press Enter:");
    println!("  a s d f g h j k   play C major (q w e r t y u i = octave up)");
    println!("  +  /  -           transpose up / down a semitone");
    println!("  p <0-127>         change the live instrument (GM program)");
    println!("  space             toggle the backing score");
    println!("  .                 panic (all notes off)");
    println!("  q                 quit");
    println!();
}
