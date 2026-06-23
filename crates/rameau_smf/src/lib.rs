//! convert a stream of midi event to/from bytes
//! basicly parses standard midi file

use std::iter;

use rameau_midi::{error::MidiError, event::MidiEvent};

trait SmfExt {
    /// parse a stream of midi events
    fn parse_midi(self) -> impl Iterator<Item = Result<MidiError, MidiEvent>>;
}

impl<T: Iterator<Item = u8>> SmfExt for T {
    fn parse_midi(self) -> impl Iterator<Item = Result<MidiError, MidiEvent>> {
        iter::empty()
    }
}
