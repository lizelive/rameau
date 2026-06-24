//! Play a Standard MIDI File through the kira backend.
//!
//! This is the end-to-end demo of the new architecture: a [`Synthesizer`]
//! generic over [`rameau_kira::Kira`] loads a SoundFont *into kira's own clip
//! type*, flattens the `.mid` into a `(seconds, event)` timeline with
//! [`Smf::timed_events`], and drives the backend live — kira does the resampling,
//! pitch and mixing.
//!
//! ```text
//! cargo run -p rameau_synthesizer --example midi_play -- song.mid assets/FluidR3Mono_GM.sf3
//! ```

use std::path::PathBuf;
use std::time::{Duration, Instant};

use rameau_kira::Kira;
use rameau_midi::smf::Smf;
use rameau_playback::Timestamp;
use rameau_soundfont::SoundFont;
use rameau_synthesizer::Synthesizer;

/// Seconds of silence to hold at the end so release tails ring out.
const TAIL: f64 = 3.0;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(midi_path) = args.next() else {
        eprintln!("usage: midi_play <file.mid> [soundfont.sf2]");
        std::process::exit(2);
    };
    let sf_arg = args.next();

    if let Err(e) = run(&midi_path, sf_arg.as_deref()) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(midi_path: &str, sf_arg: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    // Open the audio device first; the SoundFont is decoded into kira clips.
    let mut backend = Kira::new()?;

    let sf = load_soundfont(&mut backend, sf_arg)?;
    println!(
        "loaded \"{}\": {} presets, {} samples",
        sf.info.name.as_deref().unwrap_or("unnamed"),
        sf.presets.len(),
        sf.samples.len()
    );

    let bytes = std::fs::read(midi_path)?;
    let smf = Smf::parse(&bytes)?;
    let events = smf.timed_events();
    let length = events.last().map(|&(t, _)| t).unwrap_or(0.0);
    println!(
        "playing {midi_path}: {} events over {length:.1}s",
        events.len()
    );

    let mut synth = Synthesizer::new(sf, backend, 48_000);

    // Drive the timeline on this thread: wait until each event is due, then hand
    // it to the synth "now". This bounds how many voices kira holds at once.
    let start = Instant::now();
    for (secs, event) in events {
        let due = Duration::from_secs_f64(secs);
        let elapsed = start.elapsed();
        if due > elapsed {
            std::thread::sleep(due - elapsed);
        }
        synth.handle(Timestamp::Now, event)?;
    }

    std::thread::sleep(Duration::from_secs_f64(TAIL));
    println!("done");
    Ok(())
}

/// Loads a SoundFont with the kira backend, trying the CLI argument first and
/// then the banks bundled under `assets/`.
fn load_soundfont(
    backend: &mut Kira,
    arg: Option<&str>,
) -> Result<SoundFont<<Kira as rameau_playback::AudioPlayback>::Clip>, Box<dyn std::error::Error>> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(arg) = arg {
        candidates.push(PathBuf::from(arg));
    }
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    candidates.push(assets.join("FluidR3Mono_GM.sf3"));
    candidates.push(assets.join("Unison.SF2"));

    let mut last_err: Option<Box<dyn std::error::Error>> = None;
    for path in candidates {
        if !path.exists() {
            continue;
        }
        match SoundFont::load_file_with(&path, backend) {
            Ok(sf) => return Ok(sf),
            Err(e) => last_err = Some(Box::new(e)),
        }
    }
    Err(last_err.unwrap_or_else(|| "no SoundFont found (pass one on the command line)".into()))
}
