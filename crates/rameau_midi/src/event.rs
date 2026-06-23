use crate::program::MidiProgram;

pub type U7 = u8;
pub type U14 = u16;

pub type Channel = u8;

pub type Key = u8;

pub type ControlFunction = U7;

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum MidiEvent {
    /// Send a noteon message.
    NoteOn {
        channel: Channel,
        key: Key,
        vel: U7,
    },
    /// Send a noteoff message.
    NoteOff {
        channel: Channel,
        key: Key,
    },
    /// Send a control change message.
    ControlChange {
        channel: Channel,
        ctrl: ControlFunction,
        value: U7,
    },
    AllNotesOff {
        channel: Channel,
    },
    AllSoundOff {
        channel: Channel,
    },
    /// Send a pitch bend message.
    PitchBend {
        channel: Channel,
        value: U14,
    },
    /// Send a program change message.
    ProgramChange {
        channel: Channel,
        program: MidiProgram,
    },
    /// Set channel pressure
    ChannelPressure {
        channel: Channel,
        value: Key,
    },
    /// Set key pressure (aftertouch)
    PolyphonicKeyPressure {
        channel: Channel,
        key: Key,
        value: U7,
    },
    /// Send a reset.
    ///
    /// A reset turns all the notes off and resets the controller values.
    ///
    /// Purpose:
    /// Respond to the MIDI command 'system reset' (0xFF, big red 'panic' button)
    SystemReset,
}
