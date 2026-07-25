//! The format-independent MIDI event type.
//!
//! [`MidiEvent`] is what a parsed Standard MIDI File decodes into and what the
//! synthesizer consumes. Channel-voice messages carry their raw 7- or 14-bit
//! data values; nothing here depends on the on-disk encoding.

use crate::program::MidiProgram;

/// A 7-bit MIDI data value, `0..=127`.
pub type U7 = u8;

/// A 14-bit MIDI data value, `0..=16383`, as used by pitch bend.
pub type U14 = u16;

/// A MIDI channel number, `0..=15`.
pub type Channel = u8;

/// A MIDI key (note) number, `0..=127`, where 60 is middle C.
pub type Key = u8;

/// A MIDI continuous-controller number, `0..=127`.
pub type ControlFunction = U7;

/// A single MIDI event.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MidiEvent {
    /// Send a noteon message.
    NoteOn {
        /// Channel the note sounds on.
        channel: Channel,
        /// Key to sound.
        key: Key,
        /// Attack velocity.
        vel: U7,
    },
    /// Send a noteoff message.
    NoteOff {
        /// Channel the note is sounding on.
        channel: Channel,
        /// Key to release.
        key: Key,
        /// Release velocity (`0` when the source did not specify one).
        vel: U7,
    },
    /// Send a control change message.
    ControlChange {
        /// Channel the controller applies to.
        channel: Channel,
        /// Controller number being set.
        ctrl: ControlFunction,
        /// New controller value.
        value: U7,
    },
    /// Release every sounding note on a channel, letting them ring out.
    AllNotesOff {
        /// Channel to silence.
        channel: Channel,
    },
    /// Stop every sounding note on a channel immediately, with no release.
    AllSoundOff {
        /// Channel to silence.
        channel: Channel,
    },
    /// Send a pitch bend message.
    PitchBend {
        /// Channel to bend.
        channel: Channel,
        /// Bend amount; `8192` is centre (no bend).
        value: U14,
    },
    /// Send a program change message.
    ProgramChange {
        /// Channel whose instrument changes.
        channel: Channel,
        /// Instrument to select.
        program: MidiProgram,
    },
    /// Set channel pressure
    ChannelPressure {
        /// Channel the pressure applies to.
        channel: Channel,
        /// Pressure amount.
        value: Key,
    },
    /// Set key pressure (aftertouch)
    PolyphonicKeyPressure {
        /// Channel the key is sounding on.
        channel: Channel,
        /// Key the pressure applies to.
        key: Key,
        /// Pressure amount.
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
