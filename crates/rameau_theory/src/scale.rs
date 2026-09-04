//! Modes, scales, and the degree-based note encoding.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::pitch::{Midi, PitchClass};

/// A seven-note mode, given as the semitone offset of each degree from the
/// tonic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Mode {
    /// Ionian: the major scale.
    #[default]
    Major,
    /// Aeolian: the natural minor scale.
    Minor,
    /// Natural minor with a raised seventh.
    HarmonicMinor,
    /// Natural minor with raised sixth and seventh (ascending form).
    MelodicMinor,
    /// Dorian: minor with a raised sixth.
    Dorian,
    /// Phrygian: minor with a lowered second.
    Phrygian,
    /// Lydian: major with a raised fourth.
    Lydian,
    /// Mixolydian: major with a lowered seventh.
    Mixolydian,
}

impl Mode {
    /// Every mode, in a stable order.
    pub const ALL: [Mode; 8] = [
        Mode::Major,
        Mode::Minor,
        Mode::HarmonicMinor,
        Mode::MelodicMinor,
        Mode::Dorian,
        Mode::Phrygian,
        Mode::Lydian,
        Mode::Mixolydian,
    ];

    /// Semitone offsets of the seven degrees from the tonic.
    pub const fn steps(self) -> [i32; 7] {
        match self {
            Mode::Major => [0, 2, 4, 5, 7, 9, 11],
            Mode::Minor => [0, 2, 3, 5, 7, 8, 10],
            Mode::HarmonicMinor => [0, 2, 3, 5, 7, 8, 11],
            Mode::MelodicMinor => [0, 2, 3, 5, 7, 9, 11],
            Mode::Dorian => [0, 2, 3, 5, 7, 9, 10],
            Mode::Phrygian => [0, 1, 3, 5, 7, 8, 10],
            Mode::Lydian => [0, 2, 4, 6, 7, 9, 11],
            Mode::Mixolydian => [0, 2, 4, 5, 7, 9, 10],
        }
    }

    /// Whether the third degree is minor.
    pub const fn is_minor(self) -> bool {
        matches!(
            self,
            Mode::Minor | Mode::HarmonicMinor | Mode::MelodicMinor | Mode::Dorian | Mode::Phrygian
        )
    }

    /// The name used in key strings (`"major"`, `"minor"`, …).
    pub const fn name(self) -> &'static str {
        match self {
            Mode::Major => "major",
            Mode::Minor => "minor",
            Mode::HarmonicMinor => "harmonic minor",
            Mode::MelodicMinor => "melodic minor",
            Mode::Dorian => "dorian",
            Mode::Phrygian => "phrygian",
            Mode::Lydian => "lydian",
            Mode::Mixolydian => "mixolydian",
        }
    }

    /// Parses `"major"`, `"minor"`, `"dorian"`, … (case-insensitive).
    pub fn parse(name: &str) -> Option<Self> {
        let n = name.trim().to_ascii_lowercase();
        Some(match n.as_str() {
            "major" | "ionian" | "maj" | "dur" => Mode::Major,
            "minor" | "aeolian" | "min" | "moll" | "natural minor" => Mode::Minor,
            "harmonic minor" | "harmonic_minor" => Mode::HarmonicMinor,
            "melodic minor" | "melodic_minor" => Mode::MelodicMinor,
            "dorian" => Mode::Dorian,
            "phrygian" => Mode::Phrygian,
            "lydian" => Mode::Lydian,
            "mixolydian" => Mode::Mixolydian,
            _ => return None,
        })
    }

    /// The degree (0-based) that acts as leading tone, if the mode has a
    /// semitone below the tonic.
    pub const fn leading_tone_degree(self) -> Option<usize> {
        match self {
            Mode::Major | Mode::HarmonicMinor | Mode::MelodicMinor | Mode::Lydian => Some(6),
            Mode::Minor | Mode::Dorian | Mode::Phrygian | Mode::Mixolydian => None,
        }
    }
}

/// A note stored as a scale degree rather than a pitch.
///
/// `degree` is `0..=6` (tonic = 0), `alteration` is a chromatic offset in
/// semitones (so a raised fourth is `{degree: 3, alteration: 1}`), and
/// `octave` counts octaves above the reference tonic. The struct is kept
/// *normalised*: any arithmetic that would push `degree` outside `0..=6`
/// carries into `octave`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DegreeNote {
    /// Scale degree, `0..=6`.
    pub degree: u8,
    /// Chromatic alteration in semitones, usually `-1..=1`.
    pub alteration: i8,
    /// Octave offset from the reference tonic octave.
    pub octave: i8,
}

impl DegreeNote {
    /// Builds a normalised note from a possibly out-of-range degree.
    pub const fn new(degree: i32, alteration: i8, octave: i32) -> Self {
        let carry = degree.div_euclid(7);
        Self {
            degree: degree.rem_euclid(7) as u8,
            alteration,
            octave: (octave + carry) as i8,
        }
    }

    /// The note's position on the diatonic staff: `degree + 7 * octave`.
    ///
    /// Diatonic transposition and inversion are plain arithmetic on this.
    pub const fn staff_index(self) -> i32 {
        self.degree as i32 + 7 * self.octave as i32
    }

    /// Builds a note from a staff index (the inverse of
    /// [`staff_index`](Self::staff_index)).
    pub const fn from_staff_index(index: i32, alteration: i8) -> Self {
        Self::new(index, alteration, 0)
    }

    /// The note moved `steps` diatonic steps (alteration preserved).
    pub const fn transposed(self, steps: i32) -> Self {
        Self::from_staff_index(self.staff_index() + steps, self.alteration)
    }

    /// The note mirrored about `axis` on the diatonic staff (alteration
    /// negated, so a raised note becomes a lowered one).
    pub const fn inverted_about(self, axis: i32) -> Self {
        Self::from_staff_index(2 * axis - self.staff_index(), -self.alteration)
    }
}

/// A scale: a tonic pitch class and a mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Scale {
    /// The tonic.
    pub tonic: PitchClass,
    /// The mode.
    pub mode: Mode,
}

impl Scale {
    /// Creates a scale.
    pub const fn new(tonic: PitchClass, mode: Mode) -> Self {
        Self { tonic, mode }
    }

    /// Semitones above the tonic of each degree.
    pub const fn steps(&self) -> [i32; 7] {
        self.mode.steps()
    }

    /// Semitone offset of `degree` (any integer; wraps with octave carry).
    pub const fn degree_semitones(&self, degree: i32) -> i32 {
        let steps = self.steps();
        let d = degree.rem_euclid(7) as usize;
        let carry = degree.div_euclid(7);
        // `d` is in `0..7` by construction, so every arm is reachable and the
        // match keeps the lookup free of a bounds check.
        let s = match d {
            0 => steps[0],
            1 => steps[1],
            2 => steps[2],
            3 => steps[3],
            4 => steps[4],
            5 => steps[5],
            _ => steps[6],
        };
        s + 12 * carry
    }

    /// The pitch class of a degree.
    pub const fn pitch_class(&self, degree: i32) -> PitchClass {
        self.tonic.shifted(self.degree_semitones(degree))
    }

    /// The pitch classes of all seven degrees.
    pub fn pitch_classes(&self) -> [PitchClass; 7] {
        let mut out = [PitchClass::C; 7];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = self.pitch_class(i as i32);
        }
        out
    }

    /// Whether `midi` lies in the scale.
    pub fn contains(&self, midi: Midi) -> bool {
        let pc = PitchClass::of_midi(midi);
        self.pitch_classes().contains(&pc)
    }

    /// The MIDI key of the tonic in octave 0 of this scale, given the MIDI
    /// key the reference octave is anchored to.
    ///
    /// `anchor` is the lowest MIDI key that octave 0 may start at; the tonic
    /// is placed at or above it. With `anchor = 60`, a C scale has its
    /// reference tonic at 60 and a B scale at 71.
    pub const fn tonic_midi(&self, anchor: Midi) -> Midi {
        let base = anchor - anchor.rem_euclid(12);
        base + self.tonic.semitones() as i32
    }

    /// Resolves a [`DegreeNote`] to a MIDI key with the reference tonic at or
    /// above `anchor`.
    pub const fn midi(&self, note: DegreeNote, anchor: Midi) -> Midi {
        self.tonic_midi(anchor)
            + 12 * note.octave as i32
            + self.degree_semitones(note.degree as i32)
            + note.alteration as i32
    }

    /// The degree spelling of a MIDI key relative to the reference tonic at
    /// or above `anchor`.
    ///
    /// Diatonic notes get `alteration == 0`. A chromatic note is spelled as
    /// the raised form of the degree below it, except where that degree is a
    /// semitone below the next (then it is the lowered form of the degree
    /// above).
    pub fn degree_of(&self, midi: Midi, anchor: Midi) -> DegreeNote {
        let rel = midi - self.tonic_midi(anchor);
        let octave = rel.div_euclid(12);
        let within = rel.rem_euclid(12);
        let steps = self.steps();
        // Exact match first.
        if let Some(d) = steps.iter().position(|&s| s == within) {
            return DegreeNote::new(d as i32, 0, octave);
        }
        // Chromatic: find the degree just below.
        let below = steps
            .iter()
            .rposition(|&s| s < within)
            .unwrap_or(0);
        let below_semis = self.degree_semitones(below as i32);
        let above_semis = self.degree_semitones(below as i32 + 1);
        if above_semis - below_semis == 2 {
            DegreeNote::new(below as i32, (within - below_semis) as i8, octave)
        } else {
            DegreeNote::new(below as i32 + 1, (within - above_semis) as i8, octave)
        }
    }

    /// The nearest scale tone to `midi`, preferring the one below on ties.
    pub fn snap(&self, midi: Midi) -> Midi {
        if self.contains(midi) {
            return midi;
        }
        if self.contains(midi - 1) { midi - 1 } else { midi + 1 }
    }

    /// The scale tone `steps` diatonic steps away from `midi` (which is
    /// snapped into the scale first).
    pub fn step_from(&self, midi: Midi, steps: i32) -> Midi {
        let anchor = 60;
        let d = self.degree_of(self.snap(midi), anchor);
        self.midi(d.transposed(steps), anchor)
    }

    /// The same tonic in another mode.
    pub const fn with_mode(&self, mode: Mode) -> Self {
        Self::new(self.tonic, mode)
    }

    /// The scale `semitones` higher.
    pub const fn transposed(&self, semitones: i32) -> Self {
        Self::new(self.tonic.shifted(semitones), self.mode)
    }

    /// The relative major of a minor scale, or the relative minor of a major
    /// one (other modes go to their nearest major/minor relative).
    pub const fn relative(&self) -> Self {
        if self.mode.is_minor() {
            Self::new(self.tonic.shifted(3), Mode::Major)
        } else {
            Self::new(self.tonic.shifted(-3), Mode::Minor)
        }
    }

    /// The parallel key: same tonic, major ↔ minor.
    pub const fn parallel(&self) -> Self {
        if self.mode.is_minor() {
            Self::new(self.tonic, Mode::Major)
        } else {
            Self::new(self.tonic, Mode::Minor)
        }
    }

    /// The dominant key (a fifth up, same mode).
    pub const fn dominant(&self) -> Self {
        self.transposed(7)
    }

    /// The subdominant key (a fourth up, same mode).
    pub const fn subdominant(&self) -> Self {
        self.transposed(5)
    }

    /// The scale built on `degree` with the same pitch content (its diatonic
    /// mode), useful for answering a fugue subject in the dominant.
    pub fn on_degree(&self, degree: i32) -> Self {
        let tonic = self.pitch_class(degree);
        let mode = if self.mode.is_minor() { Mode::Minor } else { Mode::Major };
        Self::new(tonic, mode)
    }
}

impl core::fmt::Display for Scale {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name = if crate::key::signature_of(self) < 0 {
            self.tonic.name_flat()
        } else {
            self.tonic.name_sharp()
        };
        write!(f, "{} {}", name, self.mode.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degree_round_trip() {
        let g = Scale::new(PitchClass::G, Mode::Major);
        assert_eq!(g.tonic_midi(60), 67);
        let d = DegreeNote::new(1, 0, 0);
        assert_eq!(g.midi(d, 60), 69);
        assert_eq!(g.degree_of(69, 60), d);
        // F# in G major is degree 6.
        assert_eq!(g.degree_of(66, 60), DegreeNote::new(6, 0, -1));
        // G# in G major is raised tonic.
        assert_eq!(g.degree_of(68, 60), DegreeNote::new(0, 1, 0));
        // C in G major (degree 3 = C) fine; C# is raised 3.
        assert_eq!(g.degree_of(73, 60), DegreeNote::new(3, 1, 0));
        // Eb in C major: E-F is a semitone so D# is raised 1 (D).
        let c = Scale::new(PitchClass::C, Mode::Major);
        assert_eq!(c.degree_of(63, 60), DegreeNote::new(1, 1, 0));
        // B-C is a semitone: the note between A and B is raised A.
        assert_eq!(c.degree_of(70, 60), DegreeNote::new(5, 1, 0));
    }

    #[test]
    fn staff_arithmetic_carries_octaves() {
        let n = DegreeNote::new(6, 0, 0);
        assert_eq!(n.transposed(1), DegreeNote::new(0, 0, 1));
        assert_eq!(n.transposed(-7), DegreeNote::new(6, 0, -1));
        assert_eq!(DegreeNote::new(2, 1, 0).inverted_about(0), DegreeNote::new(5, -1, -1));
    }

    #[test]
    fn related_keys() {
        let a_minor = Scale::new(PitchClass::A, Mode::Minor);
        assert_eq!(a_minor.relative(), Scale::new(PitchClass::C, Mode::Major));
        assert_eq!(a_minor.dominant().tonic, PitchClass::E);
        assert_eq!(a_minor.on_degree(4).tonic, PitchClass::E);
        assert!(Mode::parse("Dorian") == Some(Mode::Dorian));
    }

    #[test]
    fn displays_flat_keys_with_flats() {
        assert_eq!(Scale::new(PitchClass::new(10), Mode::Major).to_string(), "Bb major");
        assert_eq!(Scale::new(PitchClass::new(6), Mode::Major).to_string(), "F# major");
        assert_eq!(Scale::new(PitchClass::G, Mode::Minor).to_string(), "G minor");
        assert_eq!(Scale::new(PitchClass::new(1), Mode::Minor).to_string(), "C# minor");
    }

    #[test]
    fn snapping_and_stepping() {
        let c = Scale::new(PitchClass::C, Mode::Major);
        assert_eq!(c.snap(61), 60);
        assert_eq!(c.step_from(60, 1), 62);
        assert_eq!(c.step_from(60, -1), 59);
        assert_eq!(c.step_from(71, 1), 72);
    }
}
