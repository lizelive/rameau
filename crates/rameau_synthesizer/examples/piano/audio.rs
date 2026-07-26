//! Applying input commands to the synthesizer.
//!
//! There is no render callback here. The kira backend owns its own audio
//! thread, so a command is handed to the synth the instant it arrives and
//! sounds immediately. That matters for playing: a callback-driven design has
//! to batch the events that arrived since the last block and stamp them with
//! one timestamp, which quantises every note onset to the block boundary.

use rameau_midi::event::MidiEvent;
use rameau_playback::{PlaybackError, Timestamp};

use crate::input::Command;

/// The MIDI channel the piano plays on.
pub const CHANNEL: u8 = 0;

/// The synthesizer type this demo drives.
pub type Synth = rameau_synthesizer::Synthesizer<rameau_kira::Kira>;

/// Hands one command to the synth, to sound now.
pub fn apply(synth: &mut Synth, cmd: Command) -> Result<(), PlaybackError> {
    let pedal = |value| MidiEvent::ControlChange {
        channel: CHANNEL,
        ctrl: 64,
        value,
    };
    match cmd {
        Command::NoteOn { key, vel } => synth.handle(
            Timestamp::Now,
            MidiEvent::NoteOn {
                channel: CHANNEL,
                key,
                vel,
            },
        ),
        Command::NoteOff { key } => synth.handle(
            Timestamp::Now,
            MidiEvent::NoteOff {
                channel: CHANNEL,
                key,
                vel: 0,
            },
        ),
        // The synth implements CC64 itself: it defers note-offs while the pedal
        // is down and releases them together when it comes up.
        Command::Sustain(down) => synth.handle(Timestamp::Now, pedal(if down { 127 } else { 0 })),
        Command::Program(p) => synth.handle(
            Timestamp::Now,
            MidiEvent::ProgramChange {
                channel: CHANNEL,
                program: p.into(),
            },
        ),
        Command::Panic => {
            // Lift the pedal too, or notes it is holding survive the panic.
            synth.handle(Timestamp::Now, pedal(0))?;
            synth.handle(Timestamp::Now, MidiEvent::AllNotesOff { channel: CHANNEL })
        }
        // Handled by the caller, which stops the loop.
        Command::Quit => Ok(()),
    }
}
