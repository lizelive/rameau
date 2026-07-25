//! MIDI types and Standard MIDI File parsing.
//!
//! [`event::MidiEvent`] is the format-independent event the rest of the
//! workspace plays; [`smf`] parses a `.mid` file into a timeline of them.

/// Errors produced while parsing MIDI data.
pub mod error;
/// Format-independent MIDI event types.
pub mod event;
/// General MIDI program (instrument) numbers.
pub mod program;
/// Standard MIDI File parsing.
pub mod smf;
