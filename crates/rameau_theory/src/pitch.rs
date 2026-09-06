//! Pitch classes and MIDI key numbers.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A MIDI key number, `0..=127`, where 60 is middle C.
pub type Midi = i32;

/// One of the twelve pitch classes, `0` = C … `11` = B.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PitchClass(u8);

impl PitchClass {
    /// C.
    pub const C: Self = Self(0);
    /// D.
    pub const D: Self = Self(2);
    /// E.
    pub const E: Self = Self(4);
    /// F.
    pub const F: Self = Self(5);
    /// G.
    pub const G: Self = Self(7);
    /// A.
    pub const A: Self = Self(9);
    /// B.
    pub const B: Self = Self(11);

    /// Wraps any semitone count into a pitch class.
    pub const fn new(semitones: i32) -> Self {
        Self(semitones.rem_euclid(12) as u8)
    }

    /// The pitch class of a MIDI key.
    pub const fn of_midi(midi: Midi) -> Self {
        Self::new(midi)
    }

    /// Semitones above C, `0..=11`.
    pub const fn semitones(self) -> u8 {
        self.0
    }

    /// The pitch class `semitones` above this one.
    pub const fn shifted(self, semitones: i32) -> Self {
        Self::new(self.0 as i32 + semitones)
    }

    /// Semitones from `self` up to `other`, `0..=11`.
    pub const fn up_to(self, other: Self) -> u8 {
        (other.0 as i32 - self.0 as i32).rem_euclid(12) as u8
    }

    /// Parses a note name such as `C`, `F#`, `Bb`, `B-` or `Eb`.
    ///
    /// Accepts `#`/`s` for sharp and `b`/`-` for flat, any number of times.
    pub fn parse(name: &str) -> Option<Self> {
        let mut chars = name.trim().chars();
        let letter = chars.next()?;
        let base = match letter.to_ascii_uppercase() {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => return None,
        };
        let mut alter = 0;
        for c in chars {
            match c {
                '#' | 's' | '♯' => alter += 1,
                'b' | '-' | '♭' => alter -= 1,
                ' ' => {}
                _ => return None,
            }
        }
        Some(Self::new(base + alter))
    }

    /// A conventional spelling, using sharps.
    pub const fn name_sharp(self) -> &'static str {
        match self.0 {
            0 => "C",
            1 => "C#",
            2 => "D",
            3 => "D#",
            4 => "E",
            5 => "F",
            6 => "F#",
            7 => "G",
            8 => "G#",
            9 => "A",
            10 => "A#",
            _ => "B",
        }
    }

    /// A conventional spelling, using flats.
    pub const fn name_flat(self) -> &'static str {
        match self.0 {
            0 => "C",
            1 => "Db",
            2 => "D",
            3 => "Eb",
            4 => "E",
            5 => "F",
            6 => "Gb",
            7 => "G",
            8 => "Ab",
            9 => "A",
            10 => "Bb",
            _ => "B",
        }
    }
}

impl core::fmt::Display for PitchClass {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name_sharp())
    }
}

/// The octave number of a MIDI key in scientific pitch notation (60 → 4).
pub const fn octave_of(midi: Midi) -> i32 {
    midi.div_euclid(12) - 1
}

/// Formats a MIDI key as scientific pitch notation, e.g. `60` → `C4`.
pub fn note_name(midi: Midi) -> String {
    format!("{}{}", PitchClass::of_midi(midi), octave_of(midi))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spellings() {
        assert_eq!(PitchClass::parse("F#"), Some(PitchClass::new(6)));
        assert_eq!(PitchClass::parse("Bb"), Some(PitchClass::new(10)));
        assert_eq!(PitchClass::parse("B-"), Some(PitchClass::new(10)));
        assert_eq!(PitchClass::parse("cb"), Some(PitchClass::B));
        assert_eq!(PitchClass::parse("H"), None);
    }

    #[test]
    fn names_and_octaves() {
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(61), "C#4");
        assert_eq!(note_name(59), "B3");
        assert_eq!(PitchClass::G.up_to(PitchClass::C), 5);
    }
}
