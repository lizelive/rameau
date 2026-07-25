//! The error type shared by the MIDI parsers.

use thiserror::Error;

/// Why a MIDI message or Standard MIDI File could not be parsed.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum MidiError {
    /// A field held a value outside its permitted range.
    #[error("bad value")]
    BadValue,
    /// The input ended in the middle of a message or chunk.
    #[error("unexpected end of input")]
    UnexpectedEof,
    /// The file did not begin with a well-formed `MThd` header chunk.
    #[error("invalid standard midi file header")]
    InvalidHeader,
    /// A variable-length quantity ran longer than the four bytes a valid file
    /// may use.
    #[error("malformed variable-length quantity")]
    InvalidVarLen,
    /// A data byte appeared before any status byte, so there was no running
    /// status to inherit.
    #[error("data byte encountered before any status byte")]
    RunningStatus,
    /// The file declared a Standard MIDI File format this parser does not
    /// handle.
    #[error("unsupported standard midi file format {0}")]
    UnsupportedFormat(u16),
}
