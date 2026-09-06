//! A playable keyboard piano over SoundFont synthesis.
//!
//! Plays from a MIDI controller if one is connected, and from the computer
//! keyboard either way. Held keys sustain, the pedal works, and velocity and
//! octave are adjustable, so this is an instrument rather than a demo reel.
//!
//! Built on [`rameau_kira::Kira`], the same backend as the `midi_play` demo:
//! kira owns the audio thread, so an event handed to the synth sounds
//! immediately. Interactive playing needs that. Driving a device callback by
//! hand instead means batching the events that arrived since the last block
//! and giving them one timestamp, which quantises every onset to the block
//! boundary and audibly stiffens the timing.
//!
//! ```text
//! cargo run -p rameau_synthesizer --example piano -- assets/FluidR3Mono_GM.sf3
//! cargo run -p rameau_synthesizer --example piano -- bank.sf2 --midi "Keystation"
//! ```
//!
//! ```text
//!   z s x d c v g b h n j m ,   lower octave, chromatic (z = C, s = C#, …)
//!   q 2 w 3 e r 5 t 6 y 7 u i   the octave above
//!   space                       sustain pedal
//!   up / down                   octave
//!   left / right                velocity
//!   [ / ]                       previous / next GM instrument
//!   .                           panic (all notes off)
//!   Esc                         quit
//! ```

#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "this is a command-line program; reporting progress and \n              errors on the standard streams is its interface"
)]

mod audio;
mod input;

use std::path::PathBuf;
use std::sync::mpsc;

use rameau_kira::Kira;
use rameau_playback::AudioPlayback;
use rameau_soundfont::SoundFont;
use rameau_synthesizer::Synthesizer;

/// Acoustic Grand Piano.
const DEFAULT_PROGRAM: u8 = 0;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Pure terminal diagnostics: no audio device, no SoundFont, nothing that
    // could fail first and mask the answer.
    if args.iter().any(|a| a == "--debug-input") {
        if let Err(e) = input::debug_input() {
            eprintln!("input diagnostics failed: {e}");
        }
        return;
    }

    let midi_filter = flag_value(&args, "--midi");
    let bank_path = args.iter().find(|a| !a.starts_with("--")).cloned();

    // Open the device first; the SoundFont is decoded into kira's clip type.
    let mut backend = match Kira::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("could not open the audio device: {e}");
            std::process::exit(1);
        }
    };

    let soundfont = match load_soundfont(&mut backend, bank_path.as_deref()) {
        Ok(sf) => sf,
        Err(e) => {
            eprintln!("could not load a SoundFont: {e}");
            eprintln!("pass one explicitly, e.g.:");
            eprintln!("  cargo run -p rameau_synthesizer --example piano -- path/to/bank.sf2");
            std::process::exit(1);
        }
    };
    println!(
        "loaded \"{}\" ({} presets, {} samples)",
        soundfont.info.name.as_deref().unwrap_or("unnamed"),
        soundfont.presets.len(),
        soundfont.samples.len()
    );

    let mut synth = Synthesizer::new(soundfont, backend, 48_000);
    if let Err(e) = audio::apply(&mut synth, input::Command::Program(DEFAULT_PROGRAM)) {
        eprintln!("could not select the default instrument: {e}");
    }

    let (tx, rx) = mpsc::channel();

    // Held for the process lifetime: dropping the connections stops MIDI input.
    let _midi = match input::connect_midi(&tx, midi_filter.as_deref()) {
        Ok(m) if m.ports.is_empty() => {
            println!("no MIDI input ports; playing from the computer keyboard");
            Some(m)
        }
        Ok(m) => {
            println!("MIDI in: {}", m.ports.join(", "));
            Some(m)
        }
        Err(e) => {
            println!("no MIDI input ({e}); playing from the computer keyboard");
            None
        }
    };

    if args.iter().any(|a| a == "--test-tone") {
        test_tone(&mut synth);
        return;
    }

    print_help();

    // The keyboard runs on its own thread so this one can block on the channel
    // and hand each command straight to the synth the moment it arrives —
    // MIDI input and typing share the same path.
    let keys = std::thread::spawn({
        let tx = tx;
        move || {
            if let Err(e) = input::run_keyboard(&tx) {
                eprintln!();
                eprintln!("could not read the keyboard: {e}");
                eprintln!("this needs a real terminal — not a pipe, task runner or IDE");
                eprintln!("output pane. Run `--debug-input` to see what this one reports.");
                let _ = tx.send(input::Command::Quit);
            }
        }
    });

    for cmd in rx {
        if matches!(cmd, input::Command::Quit) {
            break;
        }
        if let Err(e) = audio::apply(&mut synth, cmd) {
            eprintln!("audio error: {e}");
        }
    }

    // Joined so raw mode is restored before the process ends.
    let _ = keys.join();
    println!("bye");
}

/// Plays a C major chord through the full audio path, with no keyboard
/// involved. If this is silent, the problem is the device or the bank rather
/// than anything to do with reading keys.
fn test_tone(synth: &mut audio::Synth) {
    println!("playing a test tone (no keyboard input involved)...");
    for key in [60u8, 64, 67, 72] {
        let _ = audio::apply(synth, input::Command::NoteOn { key, vel: 100 });
        std::thread::sleep(core::time::Duration::from_millis(400));
    }
    std::thread::sleep(core::time::Duration::from_millis(800));
    for key in [60u8, 64, 67, 72] {
        let _ = audio::apply(synth, input::Command::NoteOff { key });
    }
    std::thread::sleep(core::time::Duration::from_millis(800));
    println!("done");
}

/// The value following `flag` in `args`, if present.
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

/// Tries an explicit path first, then the bundled banks under `assets/`,
/// loading each sample into kira's clip type.
fn load_soundfont(
    backend: &mut Kira,
    explicit: Option<&str>,
) -> Result<SoundFont<<Kira as AudioPlayback>::Clip>, rameau_soundfont::Error> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(arg) = explicit {
        candidates.push(PathBuf::from(arg));
    }
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    candidates.push(assets.join("FluidR3Mono_GM.sf3"));
    candidates.push(assets.join("Unison.SF2"));
    candidates.push(PathBuf::from("assets/FluidR3Mono_GM.sf3"));

    let mut last_err = None;
    for path in candidates {
        if !path.exists() {
            continue;
        }
        match SoundFont::load_file_with(&path, backend) {
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
    println!("keyboard piano — play directly, no Enter needed:");
    println!("  z s x d c v g b h n j m ,   lower octave, chromatic (z = C, s = C#, …)");
    println!("  q 2 w 3 e r 5 t 6 y 7 u i   the octave above");
    println!("  space                       sustain pedal");
    println!("  up / down                   octave");
    println!("  left / right                velocity");
    println!("  [ / ]                       previous / next GM instrument");
    println!("  .                           panic (all notes off)");
    println!("  Esc                         quit");
    println!();
    println!("note names print as you play. If nothing prints, keys are not");
    println!("reaching the program: run with --debug-input to see why.");
    println!();
}
