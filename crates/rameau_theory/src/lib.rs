//! Music theory primitives for the rameau workspace.
//!
//! The crate is built around one idea: music is stored as **scale degrees**,
//! not pitches. A [`DegreeNote`] is a degree of some [`Scale`] plus a
//! chromatic alteration and an octave, and only becomes a MIDI key when
//! resolved against a concrete [`Scale`]. That is what lets a motif survive
//! transposition, inversion and a change of mode without being re-edited.
//!
//! On top of that sit the rules a species-counterpoint teacher would mark
//! with a red pen — parallel fifths and octaves, voice crossing, awkward
//! leaps, notes foreign to the key — as pure functions in [`voice_leading`]
//! that a composer can weigh however it likes.

#![forbid(unsafe_code)]

pub mod interval;
pub mod key;
pub mod meter;
pub mod pitch;
pub mod scale;
pub mod voice_leading;

pub use interval::Interval;
pub use key::{Key, KeyParseError, parse_key};
pub use meter::{BeatStrength, Meter};
pub use pitch::{Midi, PitchClass};
pub use scale::{DegreeNote, Mode, Scale};
