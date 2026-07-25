//! Plays a MIDI file through the built-in Unison bank — no SoundFont file
//! needed anywhere.
//!
//! ```text
//! cargo run --release --features unison --example play_unison -- assets/heist.midi
//! ```

use std::time::Instant;

use rameau::MusicEngine;

fn main() -> Result<(), rameau::EngineError> {
    let midi = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "assets/heist.midi".to_string());

    // The bank is compiled into this binary; nothing is read from disk.
    let start = Instant::now();
    let mut engine = MusicEngine::new()?;
    println!(
        "engine ready in {:.2}s ({} presets)",
        start.elapsed().as_secs_f64(),
        engine.soundfont().presets.len()
    );

    let song = engine.load_midi(&midi)?;
    println!(
        "playing {midi}: {} events, {:.1}s",
        song.len(),
        song.duration_secs()
    );
    engine.play_midi(&song)
}
