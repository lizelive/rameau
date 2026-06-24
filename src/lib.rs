//! The top-level rameau library: a high-level [`MusicEngine`] that plays
//! Standard MIDI Files through a SoundFont.
//!
//! [`MusicEngine`] ties the workspace together behind three calls:
//!
//! * [`init`](MusicEngine::init) — open the audio device and load a SoundFont.
//! * [`load_midi`](MusicEngine::load_midi) — parse a `.mid` file.
//! * [`play_midi`](MusicEngine::play_midi) — play it in real time.
//!
//! The engine uses [`rameau_kira`] as its audio backend, so kira owns the
//! resampling, pitch-shifting, mixing and device output while a
//! [`Synthesizer`](rameau_synthesizer::Synthesizer) translates MIDI events into
//! voice commands.
//!
//! ```no_run
//! use rameau::MusicEngine;
//!
//! # fn main() -> Result<(), rameau::EngineError> {
//! let mut engine = MusicEngine::init("assets/FluidR3Mono_GM.sf3")?;
//! let song = engine.load_midi("song.mid")?;
//! engine.play_midi(&song)?;
//! # Ok(()) }
//! ```

use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use rameau_kira::Kira;
use rameau_midi::smf::Smf;
use rameau_playback::{PlaybackError, Timestamp};
use rameau_soundfont::SoundFont;
use rameau_synthesizer::Synthesizer;

/// The sample rate the engine asks its backend to render at, in Hz.
const SAMPLE_RATE: u32 = 48_000;

/// Seconds of silence held after the last event so release tails ring out.
const TAIL: f64 = 3.0;

/// A parsed song ready to be played by a [`MusicEngine`].
///
/// Produced by [`MusicEngine::load_midi`]; it owns the decoded MIDI events as a
/// `(seconds, event)` timeline.
pub struct Song {
    events: Vec<(f64, rameau_midi::event::MidiEvent)>,
}

impl Song {
    /// The total length of the song in seconds (the time of its last event).
    pub fn duration_secs(&self) -> f64 {
        self.events.last().map(|&(t, _)| t).unwrap_or(0.0)
    }

    /// The number of MIDI events in the song.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the song has no events.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// A high-level music player: a SoundFont synthesizer driving the kira backend.
pub struct MusicEngine {
    synth: Synthesizer<Kira>,
}

impl MusicEngine {
    /// Opens the default audio device and loads the SoundFont at `soundfont`.
    ///
    /// Accepts `.sf2` and `.sf3` banks. The samples are decoded straight into
    /// kira's own clip type, so playback never has to touch raw PCM again.
    pub fn init(soundfont: impl AsRef<Path>) -> Result<Self, EngineError> {
        let mut backend = Kira::new()?;
        let sf: SoundFont<_> = SoundFont::load_file_with(soundfont, &mut backend)?;
        let synth = Synthesizer::new(sf, backend, SAMPLE_RATE);
        Ok(Self { synth })
    }

    /// Parses a Standard MIDI File into a [`Song`].
    ///
    /// This only reads and decodes the file; nothing is played until
    /// [`play_midi`](Self::play_midi).
    pub fn load_midi(&self, path: impl AsRef<Path>) -> Result<Song, EngineError> {
        let bytes = std::fs::read(path)?;
        let smf = Smf::parse(&bytes)?;
        Ok(Song {
            events: smf.timed_events(),
        })
    }

    /// Plays `song` in real time, blocking until it finishes.
    ///
    /// Events are dispatched on the calling thread as each one comes due, then
    /// the engine waits a short tail so release tails can ring out.
    pub fn play_midi(&mut self, song: &Song) -> Result<(), EngineError> {
        let start = Instant::now();
        for &(secs, event) in &song.events {
            let due = Duration::from_secs_f64(secs);
            let elapsed = start.elapsed();
            if due > elapsed {
                thread::sleep(due - elapsed);
            }
            self.synth.handle(Timestamp::Now, event)?;
        }
        thread::sleep(Duration::from_secs_f64(TAIL));
        Ok(())
    }

    /// The SoundFont loaded into this engine.
    pub fn soundfont(&self) -> &SoundFont<<Kira as rameau_playback::AudioPlayback>::Clip> {
        self.synth.soundfont()
    }
}

/// Anything that can go wrong driving a [`MusicEngine`].
#[derive(Debug)]
pub enum EngineError {
    /// The audio backend failed to open or play.
    Playback(PlaybackError),
    /// The SoundFont could not be loaded.
    SoundFont(rameau_soundfont::Error),
    /// A MIDI file could not be parsed.
    Midi(rameau_midi::error::MidiError),
    /// An I/O error reading a file from disk.
    Io(std::io::Error),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::Playback(e) => write!(f, "playback error: {e}"),
            EngineError::SoundFont(e) => write!(f, "soundfont error: {e}"),
            EngineError::Midi(e) => write!(f, "midi error: {e}"),
            EngineError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<PlaybackError> for EngineError {
    fn from(e: PlaybackError) -> Self {
        EngineError::Playback(e)
    }
}

impl From<rameau_soundfont::Error> for EngineError {
    fn from(e: rameau_soundfont::Error) -> Self {
        EngineError::SoundFont(e)
    }
}

impl From<rameau_midi::error::MidiError> for EngineError {
    fn from(e: rameau_midi::error::MidiError) -> Self {
        EngineError::Midi(e)
    }
}

impl From<std::io::Error> for EngineError {
    fn from(e: std::io::Error) -> Self {
        EngineError::Io(e)
    }
}
