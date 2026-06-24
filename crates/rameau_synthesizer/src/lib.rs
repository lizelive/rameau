//! A SoundFont synthesizer that *drives* an [`AudioPlayback`](rameau_playback::AudioPlayback) backend.
//!
//! [`Synthesizer`] is the MIDI brain, not the audio engine. It owns a
//! `SoundFont` and per-channel controller state, resolves each note-on through
//! the SoundFont generator hierarchy (preset zone → instrument → sample), and
//! turns the result into `start` / `update` / `stop` calls on a pluggable
//! backend `P`. The backend — software (`rameau_software`) or a real-time
//! engine (`rameau_kira`) — owns resampling, pitch, mixing and output.
//!
//! Because the SoundFont's samples are stored as the backend's own clip type
//! (`P::Clip`), load the bank *with* the backend:
//!
//! ```no_run
//! use rameau_playback::Timestamp;
//! use rameau_midi::event::MidiEvent;
//! use rameau_soundfont::SoundFont;
//! use rameau_synthesizer::Synthesizer;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut backend = rameau_software::Software::new(44_100);
//! let sf = SoundFont::load_file_with("bank.sf2", &mut backend)?;
//! let mut synth = Synthesizer::new(sf, backend, 44_100);
//! synth.handle(Timestamp::Now, MidiEvent::NoteOn { channel: 0, key: 60, vel: 100 })?;
//! # Ok(()) }
//! ```

mod gens;
mod synth;

pub use synth::Synthesizer;

pub use rameau_midi::event::MidiEvent as Event;
pub use rameau_playback::Timestamp as When;
