//! A playable keyboard piano over SoundFont synthesis.
//!
//! Plays from a MIDI controller if one is connected, and from the computer
//! keyboard either way. Held keys sustain, the pedal works, and velocity and
//! octave are adjustable, so this is an instrument rather than a demo reel.
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

mod audio;
mod input;

use std::path::PathBuf;
use std::sync::mpsc;

use rameau_playback::{AudioPlayback, Playback, PlaybackConfig};
use rameau_software::Software;
use rameau_soundfont::SoundFont;
use rameau_tinyaudio::TinyAudio;

/// Acoustic Grand Piano.
const DEFAULT_PROGRAM: u8 = 0;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let midi_filter = flag_value(&args, "--midi");
    let bank_path = args.iter().find(|a| !a.starts_with("--")).cloned();

    let config = PlaybackConfig {
        channels: 2,
        sample_rate: 48_000,
        // ~5 ms blocks: low latency while staying comfortably real-time.
        frames_per_buffer: 256,
    };

    let mut backend = Software::new(config.sample_rate);
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

    let (tx, rx) = mpsc::channel();

    // Held for the process lifetime: dropping the connections stops MIDI input.
    let _midi = match input::connect_midi(tx.clone(), midi_filter.as_deref()) {
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

    let render = audio::render_callback(
        soundfont,
        backend,
        config.sample_rate,
        config.buffer_len(),
        rx,
        DEFAULT_PROGRAM,
    );

    let _stream = match TinyAudio.open(config, render) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("could not open audio device: {e}");
            std::process::exit(1);
        }
    };

    print_help();
    // Raw mode is restored on the way out of this call, including on panic.
    if let Err(e) = input::run_keyboard(tx) {
        eprintln!("keyboard input failed: {e}");
    }
    println!("bye");
}

/// The value following `flag` in `args`, if present.
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

/// Tries an explicit path first, then the bundled banks under `assets/`,
/// loading each sample into the software backend's clip type.
fn load_soundfont(
    backend: &mut Software,
    explicit: Option<&str>,
) -> Result<SoundFont<<Software as AudioPlayback>::Clip>, rameau_soundfont::Error> {
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
}
