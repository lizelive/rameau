//! Chord qualities and roman numerals.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use rameau_theory::{Midi, Mode, PitchClass, Scale};

/// The quality of a triad or seventh chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Quality {
    /// Major triad.
    #[default]
    Major,
    /// Minor triad.
    Minor,
    /// Diminished triad.
    Diminished,
    /// Augmented triad.
    Augmented,
    /// Major triad with a minor seventh (the dominant seventh).
    DominantSeventh,
    /// Major triad with a major seventh.
    MajorSeventh,
    /// Minor triad with a minor seventh.
    MinorSeventh,
    /// Diminished triad with a minor seventh.
    HalfDiminishedSeventh,
    /// Diminished triad with a diminished seventh.
    DiminishedSeventh,
}

impl Quality {
    /// Semitone offsets of the chord members above the root.
    pub const fn intervals(self) -> &'static [i32] {
        match self {
            Quality::Major => &[0, 4, 7],
            Quality::Minor => &[0, 3, 7],
            Quality::Diminished => &[0, 3, 6],
            Quality::Augmented => &[0, 4, 8],
            Quality::DominantSeventh => &[0, 4, 7, 10],
            Quality::MajorSeventh => &[0, 4, 7, 11],
            Quality::MinorSeventh => &[0, 3, 7, 10],
            Quality::HalfDiminishedSeventh => &[0, 3, 6, 10],
            Quality::DiminishedSeventh => &[0, 3, 6, 9],
        }
    }

    /// Whether the chord has a seventh.
    pub const fn has_seventh(self) -> bool {
        self.intervals().len() == 4
    }

    /// The triad quality implied by a third of `third` semitones and a fifth
    /// of `fifth` semitones above the root.
    pub const fn from_stack(third: i32, fifth: i32) -> Self {
        match (third, fifth) {
            (3, 7) => Quality::Minor,
            (3, 6) => Quality::Diminished,
            (4, 8) => Quality::Augmented,
            // (4, 7) is major; anything odder (a suspended or quartal stack)
            // is treated as major too so it still realises to something
            // playable.
            _ => Quality::Major,
        }
    }

    /// Whether the triad's third is minor.
    pub const fn is_minorish(self) -> bool {
        matches!(
            self,
            Quality::Minor
                | Quality::Diminished
                | Quality::MinorSeventh
                | Quality::HalfDiminishedSeventh
                | Quality::DiminishedSeventh
        )
    }
}

/// A chord relative to a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RomanNumeral {
    /// Scale degree of the root, `0..=6`.
    pub degree: u8,
    /// Chromatic alteration of the root in semitones (e.g. the Neapolitan is
    /// degree 1, alteration -1).
    pub root_alteration: i8,
    /// Chord quality.
    pub quality: Quality,
    /// Inversion: 0 root position, 1 first, 2 second, 3 third (sevenths).
    pub inversion: u8,
}

impl RomanNumeral {
    /// A root-position chord.
    pub const fn new(degree: u8, quality: Quality) -> Self {
        Self {
            degree,
            root_alteration: 0,
            quality,
            inversion: 0,
        }
    }

    /// The same chord in `inversion`.
    pub const fn inv(mut self, inversion: u8) -> Self {
        self.inversion = inversion;
        self
    }

    /// The same chord with its root altered by `semitones`.
    pub const fn altered(mut self, semitones: i8) -> Self {
        self.root_alteration = semitones;
        self
    }

    /// The diatonic triad on `degree` of `scale`, quality read from the scale.
    ///
    /// In a minor key the dominant and leading-tone chords are taken from the
    /// harmonic minor (so V is major and vii is diminished), as the period
    /// did.
    pub fn diatonic(scale: &Scale, degree: i32) -> Self {
        let harmonic = harmonic_scale(scale);
        let root = harmonic.degree_semitones(degree);
        let third = harmonic.degree_semitones(degree + 2) - root;
        let fifth = harmonic.degree_semitones(degree + 4) - root;
        Self::new(degree.rem_euclid(7) as u8, Quality::from_stack(third, fifth))
            .altered(raised_root(scale, degree))
    }

    /// The diatonic seventh chord on `degree` of `scale`.
    pub fn diatonic_seventh(scale: &Scale, degree: i32) -> Self {
        let harmonic = harmonic_scale(scale);
        let root = harmonic.degree_semitones(degree);
        let third = harmonic.degree_semitones(degree + 2) - root;
        let fifth = harmonic.degree_semitones(degree + 4) - root;
        let seventh = harmonic.degree_semitones(degree + 6) - root;
        let q = match (Quality::from_stack(third, fifth), seventh) {
            (Quality::Major, 10) => Quality::DominantSeventh,
            (Quality::Major | Quality::Augmented, _) => Quality::MajorSeventh,
            (Quality::Minor, _) => Quality::MinorSeventh,
            (Quality::Diminished, 9) => Quality::DiminishedSeventh,
            (Quality::Diminished, _) => Quality::HalfDiminishedSeventh,
            (q, _) => q,
        };
        Self::new(degree.rem_euclid(7) as u8, q).altered(raised_root(scale, degree))
    }

    /// The pitch class of the root in `scale`.
    pub fn root(&self, scale: &Scale) -> PitchClass {
        scale
            .pitch_class(self.degree as i32)
            .shifted(self.root_alteration as i32)
    }

    /// The chord's pitch classes, root first.
    pub fn pitch_classes(&self, scale: &Scale) -> Vec<PitchClass> {
        let root = self.root(scale);
        self.quality
            .intervals()
            .iter()
            .map(|&i| root.shifted(i))
            .collect()
    }

    /// The pitch class in the bass, given the inversion.
    pub fn bass(&self, scale: &Scale) -> PitchClass {
        let pcs = self.pitch_classes(scale);
        let i = (self.inversion as usize) % pcs.len().max(1);
        pcs.get(i).copied().unwrap_or(self.root(scale))
    }

    /// Whether `midi` is a member of the chord.
    pub fn contains(&self, scale: &Scale, midi: Midi) -> bool {
        let pc = PitchClass::of_midi(midi);
        self.pitch_classes(scale).contains(&pc)
    }

    /// Every chord tone between `low` and `high` inclusive, ascending.
    pub fn tones_in_range(&self, scale: &Scale, low: Midi, high: Midi) -> Vec<Midi> {
        let pcs = self.pitch_classes(scale);
        (low..=high)
            .filter(|m| pcs.contains(&PitchClass::of_midi(*m)))
            .collect()
    }

    /// The nearest chord tone to `midi` (ties resolve downward).
    pub fn nearest_tone(&self, scale: &Scale, midi: Midi) -> Midi {
        let pcs = self.pitch_classes(scale);
        (0..=6)
            .flat_map(|d| [midi - d, midi + d])
            .find(|m| pcs.contains(&PitchClass::of_midi(*m)))
            .unwrap_or(midi)
    }

    /// Whether this is a dominant-function chord (V, V7 or vii°).
    pub const fn is_dominant(&self) -> bool {
        (self.degree == 4 || self.degree == 6) && self.root_alteration == 0
    }

    /// Whether this is the tonic chord in any inversion.
    pub const fn is_tonic(&self) -> bool {
        self.degree == 0 && self.root_alteration == 0
    }

    /// A conventional label such as `V7`, `ii6`, `vii°`, `♭II6`.
    pub fn label(&self) -> String {
        let numeral = match self.degree {
            0 => "I",
            1 => "II",
            2 => "III",
            3 => "IV",
            4 => "V",
            5 => "VI",
            _ => "VII",
        };
        let mut s = String::new();
        let raised_leading_tone = self.degree == 6
            && self.root_alteration == 1
            && matches!(self.quality, Quality::Diminished | Quality::DiminishedSeventh);
        match self.root_alteration {
            a if a < 0 => s.push('♭'),
            a if a > 0 && !raised_leading_tone => s.push('♯'),
            _ => {}
        }
        if self.quality.is_minorish() {
            s.push_str(&numeral.to_ascii_lowercase());
        } else {
            s.push_str(numeral);
        }
        match self.quality {
            Quality::Diminished => s.push('°'),
            Quality::DiminishedSeventh => s.push_str("°7"),
            Quality::HalfDiminishedSeventh => s.push_str("ø7"),
            Quality::Augmented => s.push('+'),
            Quality::DominantSeventh | Quality::MinorSeventh | Quality::MajorSeventh => {
                s.push('7');
            }
            Quality::Major | Quality::Minor => {}
        }
        match (self.quality.has_seventh(), self.inversion) {
            (_, 0) => {}
            (false, 1) => s.push('6'),
            (false, _) => s.push_str("64"),
            (true, 1) => s.push_str("65"),
            (true, 2) => s.push_str("43"),
            (true, _) => s.push_str("42"),
        }
        s
    }
}

impl core::fmt::Display for RomanNumeral {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.label())
    }
}

/// Minor keys harmonise with a raised leading tone.
fn harmonic_scale(scale: &Scale) -> Scale {
    match scale.mode {
        Mode::Minor => scale.with_mode(Mode::HarmonicMinor),
        _ => *scale,
    }
}

/// How far the harmonised root of `degree` sits above the scale's own note
/// on that degree: `1` for the leading tone of a natural-minor key, else `0`.
fn raised_root(scale: &Scale, degree: i32) -> i8 {
    let harmonic = harmonic_scale(scale);
    (harmonic.degree_semitones(degree) - scale.degree_semitones(degree)) as i8
}

/// The formulas that close a phrase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Cadence {
    /// Perfect authentic: V (root) → I (root).
    #[default]
    Authentic,
    /// Imperfect authentic: V → I with an inversion somewhere.
    Imperfect,
    /// Half: ends on V.
    Half,
    /// Deceptive: V → vi.
    Deceptive,
    /// Plagal: IV → I.
    Plagal,
    /// Phrygian half cadence: iv6 → V, the minor-key sigh.
    Phrygian,
}

impl Cadence {
    /// The closing chords of this cadence in `scale`.
    pub fn chords(self, scale: &Scale) -> Vec<RomanNumeral> {
        let v = RomanNumeral::diatonic(scale, 4);
        let v7 = RomanNumeral::diatonic_seventh(scale, 4);
        let i = RomanNumeral::diatonic(scale, 0);
        let iv = RomanNumeral::diatonic(scale, 3);
        let vi = RomanNumeral::diatonic(scale, 5);
        let ii = RomanNumeral::diatonic(scale, 1);
        match self {
            Cadence::Authentic => vec![ii.inv(1), v7, i],
            Cadence::Imperfect => vec![iv, v.inv(1), i],
            Cadence::Half => vec![i.inv(1), ii.inv(1), v],
            Cadence::Deceptive => vec![iv, v7, vi],
            Cadence::Plagal => vec![i, iv, i],
            Cadence::Phrygian => vec![i, iv.inv(1), v],
        }
    }

    /// Whether the phrase comes to rest on the tonic.
    pub const fn closes(self) -> bool {
        matches!(self, Cadence::Authentic | Cadence::Imperfect | Cadence::Plagal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diatonic_qualities() {
        let c = Scale::new(PitchClass::C, Mode::Major);
        assert_eq!(RomanNumeral::diatonic(&c, 0).quality, Quality::Major);
        assert_eq!(RomanNumeral::diatonic(&c, 1).quality, Quality::Minor);
        assert_eq!(RomanNumeral::diatonic(&c, 6).quality, Quality::Diminished);
        assert_eq!(RomanNumeral::diatonic_seventh(&c, 4).quality, Quality::DominantSeventh);
        let a = Scale::new(PitchClass::A, Mode::Minor);
        assert_eq!(RomanNumeral::diatonic(&a, 4).quality, Quality::Major, "V in minor is major");
        assert_eq!(RomanNumeral::diatonic(&a, 6).quality, Quality::Diminished);
        assert_eq!(RomanNumeral::diatonic_seventh(&a, 6).quality, Quality::DiminishedSeventh);
        assert_eq!(RomanNumeral::diatonic(&a, 6).root(&a), PitchClass::new(8), "vii° in A minor is on G#");
        assert_eq!(RomanNumeral::diatonic(&a, 6).label(), "vii°");
        assert_eq!(RomanNumeral::diatonic(&a, 4).root(&a), PitchClass::E);
        assert_eq!(RomanNumeral::diatonic(&a, 2).quality, Quality::Augmented);
    }

    #[test]
    fn realisation_and_labels() {
        let g = Scale::new(PitchClass::G, Mode::Major);
        let v7 = RomanNumeral::diatonic_seventh(&g, 4);
        assert_eq!(
            v7.pitch_classes(&g),
            vec![PitchClass::D, PitchClass::new(6), PitchClass::A, PitchClass::C]
        );
        assert_eq!(v7.label(), "V7");
        assert_eq!(RomanNumeral::diatonic(&g, 1).inv(1).label(), "ii6");
        assert_eq!(RomanNumeral::diatonic(&g, 6).label(), "vii°");
        assert_eq!(v7.inv(1).bass(&g), PitchClass::new(6));
        assert!(v7.contains(&g, 72));
        assert_eq!(v7.nearest_tone(&g, 71), 72);
        assert_eq!(RomanNumeral::new(1, Quality::Major).altered(-1).label(), "♭II");
    }
}
