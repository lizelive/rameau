//! The harmonic grammar of the period, and the stock ground-bass patterns.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use rameau_theory::Scale;
use rameau_types::Rng;

use crate::chord::{Cadence, RomanNumeral};

/// A named chord progression, one chord per slot.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Progression {
    /// The chords, in order.
    pub chords: Vec<RomanNumeral>,
    /// Where it came from: a stock name or `"grammar"`.
    pub source: String,
}

impl Progression {
    /// Labels joined with spaces, e.g. `"I IV V I"`.
    pub fn labels(&self) -> String {
        self.chords
            .iter()
            .map(RomanNumeral::label)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The stock progressions of the seventeenth and eighteenth centuries: the
/// basses everyone improvised over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum StockProgression {
    /// La Folia (later form): i V i VII III VII i V, twice, closing on i.
    Folia,
    /// Romanesca: III VII i V III VII i V i — in major, I V vi iii IV I IV V.
    Romanesca,
    /// Passamezzo antico: i VII i V III VII i V i.
    PassamezzoAntico,
    /// Passamezzo moderno: I IV I V I IV I V I.
    PassamezzoModerno,
    /// The lament: a chromatic-or-diatonic descending tetrachord in the bass.
    LamentTetrachord,
    /// The chaconne bass of Lully's stage: I V vi IV (or i V VI iv) then a
    /// cadence.
    Chaconne,
    /// Descending-fifths sequence: I IV vii° iii vi ii V I.
    CircleOfFifths,
    /// Pachelbel-style: I V vi iii IV I IV V.
    Canon,
}

impl StockProgression {
    /// All stock progressions.
    pub const ALL: [StockProgression; 8] = [
        StockProgression::Folia,
        StockProgression::Romanesca,
        StockProgression::PassamezzoAntico,
        StockProgression::PassamezzoModerno,
        StockProgression::LamentTetrachord,
        StockProgression::Chaconne,
        StockProgression::CircleOfFifths,
        StockProgression::Canon,
    ];

    /// A short name.
    pub const fn name(self) -> &'static str {
        match self {
            StockProgression::Folia => "folia",
            StockProgression::Romanesca => "romanesca",
            StockProgression::PassamezzoAntico => "passamezzo antico",
            StockProgression::PassamezzoModerno => "passamezzo moderno",
            StockProgression::LamentTetrachord => "lament tetrachord",
            StockProgression::Chaconne => "chaconne",
            StockProgression::CircleOfFifths => "circle of fifths",
            StockProgression::Canon => "canon",
        }
    }

    /// Whether the pattern belongs to minor keys.
    pub const fn prefers_minor(self) -> bool {
        matches!(
            self,
            StockProgression::Folia
                | StockProgression::PassamezzoAntico
                | StockProgression::LamentTetrachord
        )
    }

    /// The pattern realised in `scale`.
    pub fn realise(self, scale: &Scale) -> Progression {
        let d = |deg: i32| RomanNumeral::diatonic(scale, deg);
        let d7 = |deg: i32| RomanNumeral::diatonic_seventh(scale, deg);
        let minor = scale.mode.is_minor();
        let chords = match self {
            StockProgression::Folia => {
                vec![d(0), d(4), d(0), d(6), d(2), d(6), d(0), d(4), d(0), d(4), d(0), d(6), d(2), d(6), d(0).inv(0), d(4), d(0)]
            }
            StockProgression::Romanesca => {
                if minor {
                    vec![d(2), d(6), d(0), d(4), d(2), d(6), d(0), d(4), d(0)]
                } else {
                    vec![d(0), d(4), d(5), d(2), d(3), d(0), d(3), d(4), d(0)]
                }
            }
            StockProgression::PassamezzoAntico => {
                vec![d(0), d(6), d(0), d(4), d(2), d(6), d(0), d(4), d(0)]
            }
            StockProgression::PassamezzoModerno => {
                vec![d(0), d(3), d(0), d(4), d(0), d(3), d(0), d(4), d(0)]
            }
            StockProgression::LamentTetrachord => {
                // Bass 1 - 7 - 6 - 5: i, v6 (or V6 in major), iv6 (IV6), V.
                vec![d(0), d(4).inv(1), d(3).inv(1), d(4), d(0), d(4).inv(1), d(3).inv(1), d(4)]
            }
            StockProgression::Chaconne => {
                vec![d(0), d(4), d(5), d(3), d(0).inv(1), d(3), d7(4), d(0)]
            }
            StockProgression::CircleOfFifths => {
                vec![d(0), d(3), d(6), d(2), d(5), d(1), d7(4), d(0)]
            }
            StockProgression::Canon => {
                vec![d(0), d(4), d(5), d(2), d(3), d(0), d(3), d(4)]
            }
        };
        // In minor the subtonic VII (natural seventh, a major triad) is the
        // period's choice for the folia / passamezzo, not the raised
        // leading-tone chord.
        let chords = if minor
            && !matches!(self, StockProgression::Chaconne | StockProgression::CircleOfFifths)
        {
            chords
                .into_iter()
                .map(|c| {
                    if c.degree == 6 && c.inversion == 0 {
                        RomanNumeral::subtonic()
                    } else {
                        c
                    }
                })
                .collect()
        } else {
            chords
        };
        Progression {
            chords,
            source: self.name().to_owned(),
        }
    }
}

impl RomanNumeral {
    /// The subtonic ♭VII of a minor key: the major triad on the *natural*
    /// seventh degree, as against the diminished chord on the raised one.
    pub const fn subtonic() -> Self {
        RomanNumeral::new(6, crate::chord::Quality::Major)
    }
}

/// Harmonic function, the coarse grammar behind the chord-to-chord table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Function {
    /// Tonic: I, vi (and iii, weakly).
    Tonic,
    /// Predominant: ii, IV (and vi on its way somewhere).
    Predominant,
    /// Dominant: V, vii°.
    Dominant,
}

/// A weighted chord-to-chord table for the style.
#[derive(Debug, Clone)]
pub struct Grammar {
    /// `weights[from][to]` for root-position diatonic triads, degree-indexed.
    weights: [[f64; 7]; 7],
    /// Probability of adding a seventh to a dominant.
    seventh_chance: f64,
    /// Probability of a first inversion where the bass would otherwise leap.
    inversion_chance: f64,
}

impl Grammar {
    /// The common-practice table: strong pulls to the dominant and tonic,
    /// descending fifths favoured, retrogressions (V → IV) rare.
    pub fn period() -> Self {
        // rows: from I ii iii IV V vi vii° ; cols: to the same.
        let weights = [
            [1.0, 3.0, 1.0, 4.0, 5.0, 2.5, 1.0], // from I
            [0.5, 0.5, 0.2, 0.5, 6.0, 0.3, 2.0], // from ii
            [0.5, 0.5, 0.2, 3.0, 0.5, 3.0, 0.3], // from iii
            [3.0, 2.5, 0.3, 0.5, 5.0, 0.3, 1.5], // from IV
            [6.0, 0.2, 0.3, 0.6, 1.0, 2.0, 0.2], // from V
            [0.8, 3.0, 0.5, 3.0, 2.0, 0.3, 0.3], // from vi
            [5.0, 0.1, 0.5, 0.1, 0.3, 0.5, 0.1], // from vii°
        ];
        Self {
            weights,
            seventh_chance: 0.45,
            inversion_chance: 0.35,
        }
    }

    /// The harmonic function of a degree.
    pub const fn function(degree: u8) -> Function {
        match degree {
            0 | 2 => Function::Tonic,
            1 | 3 | 5 => Function::Predominant,
            _ => Function::Dominant,
        }
    }

    /// The weight of moving from chord `from` to chord `to` (root degrees).
    pub fn weight(&self, from: u8, to: u8) -> f64 {
        self.weights
            .get(from as usize % 7)
            .and_then(|row| row.get(to as usize % 7))
            .copied()
            .unwrap_or(0.0)
    }

    /// Picks the chord that follows `from`.
    pub fn next(&self, rng: &mut impl Rng, scale: &Scale, from: &RomanNumeral) -> RomanNumeral {
        let row = self
            .weights
            .get(from.degree as usize % 7)
            .copied()
            .unwrap_or([1.0; 7]);
        let deg = rng.weighted(&row).unwrap_or(0) as i32;
        self.decorate(rng, scale, from, deg)
    }

    /// Chooses seventh and inversion for the chord on `deg` following `from`.
    fn decorate(
        &self,
        rng: &mut impl Rng,
        scale: &Scale,
        from: &RomanNumeral,
        deg: i32,
    ) -> RomanNumeral {
        let mut chord = if deg == 4 && rng.chance(self.seventh_chance) {
            RomanNumeral::diatonic_seventh(scale, deg)
        } else {
            RomanNumeral::diatonic(scale, deg)
        };
        // vii° lives in first inversion; other chords take a 6 when the
        // bass would otherwise leap a fourth or more.
        let bass_leap = (from.bass(scale).up_to(chord.bass(scale)) as i32).min(
            chord.bass(scale).up_to(from.bass(scale)) as i32,
        );
        if deg == 6 || (bass_leap >= 5 && deg != 0 && deg != 4 && rng.chance(self.inversion_chance)) {
            chord = chord.inv(1);
        }
        chord
    }

    /// Writes a phrase of `len` chords in `scale` that ends with `cadence`.
    ///
    /// The cadence formula is placed at the end; the chords before it are
    /// walked from the table starting on `start` (the tonic if `None`), with
    /// the last free chord nudged toward something that leads into the
    /// formula.
    pub fn phrase(
        &self,
        rng: &mut impl Rng,
        scale: &Scale,
        len: usize,
        cadence: Cadence,
        start: Option<RomanNumeral>,
    ) -> Progression {
        let tail = cadence.chords(scale);
        let len = len.max(1);
        let mut chords: Vec<RomanNumeral> = Vec::with_capacity(len);
        let free = len.saturating_sub(tail.len());
        let mut current = start.unwrap_or_else(|| RomanNumeral::diatonic(scale, 0));
        if free > 0 {
            chords.push(current);
        }
        for i in 1..free {
            let next = if i + 1 == free {
                // Lead into the cadence: land on a predominant or tonic.
                let candidates = [0, 1, 3, 5];
                let weights: Vec<f64> = candidates
                    .iter()
                    .map(|&d| self.weight(current.degree, d as u8))
                    .collect();
                let d = candidates
                    .get(rng.weighted(&weights).unwrap_or(0))
                    .copied()
                    .unwrap_or(0);
                self.decorate(rng, scale, &current, d)
            } else {
                self.next(rng, scale, &current)
            };
            chords.push(next);
            current = next;
        }
        // If the phrase is shorter than the formula, keep the formula's end.
        let skip = tail.len().saturating_sub(len);
        chords.extend(tail.into_iter().skip(skip));
        Progression {
            chords,
            source: "grammar".to_owned(),
        }
    }

    /// A stock progression, or `None` if none suits the mode.
    pub fn stock(
        rng: &mut impl Rng,
        scale: &Scale,
        allow: &[StockProgression],
    ) -> Option<Progression> {
        let fitting: Vec<StockProgression> = allow
            .iter()
            .copied()
            .filter(|p| !p.prefers_minor() || scale.mode.is_minor())
            .collect();
        rng.pick(&fitting).map(|p| p.realise(scale))
    }
}

impl Default for Grammar {
    fn default() -> Self {
        Self::period()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rameau_theory::{Mode, PitchClass};
    use rameau_types::SplitMix64;

    #[test]
    fn phrases_end_in_their_cadence() {
        let g = Grammar::period();
        let c = Scale::new(PitchClass::C, Mode::Major);
        let mut rng = SplitMix64::new(3);
        for cadence in [Cadence::Authentic, Cadence::Half, Cadence::Deceptive, Cadence::Phrygian] {
            let p = g.phrase(&mut rng, &c, 8, cadence, None);
            assert_eq!(p.chords.len(), 8, "{}", p.labels());
            let last = p.chords.last().unwrap();
            match cadence {
                Cadence::Authentic => assert!(last.is_tonic()),
                Cadence::Half | Cadence::Phrygian => assert_eq!(last.degree, 4),
                Cadence::Deceptive => assert_eq!(last.degree, 5),
                _ => {}
            }
            assert!(p.chords.first().unwrap().is_tonic());
        }
        let short = g.phrase(&mut rng, &c, 2, Cadence::Authentic, None);
        assert_eq!(short.chords.len(), 2);
        assert!(short.chords.last().unwrap().is_tonic());
    }

    #[test]
    fn grammar_avoids_retrogression() {
        let g = Grammar::period();
        assert!(g.weight(4, 0) > g.weight(4, 3) * 5.0);
        assert!(g.weight(1, 4) > g.weight(1, 0));
    }

    #[test]
    fn stock_patterns_realise() {
        let d_minor = Scale::new(PitchClass::D, Mode::Minor);
        let folia = StockProgression::Folia.realise(&d_minor);
        assert_eq!(folia.chords.len(), 17);
        // Second chord is the (major) dominant, fourth is the subtonic C major.
        assert_eq!(folia.chords[1].root(&d_minor), PitchClass::A);
        assert_eq!(folia.chords[3].root(&d_minor), PitchClass::C, "{}", folia.labels());
        assert_eq!(folia.chords[3].quality, crate::chord::Quality::Major);
        let lament = StockProgression::LamentTetrachord.realise(&d_minor);
        let basses: Vec<PitchClass> = lament.chords.iter().take(4).map(|c| c.bass(&d_minor)).collect();
        assert_eq!(basses, vec![PitchClass::D, PitchClass::new(1), PitchClass::new(10), PitchClass::A]);
        let g = Scale::new(PitchClass::G, Mode::Major);
        let canon = StockProgression::Canon.realise(&g);
        assert_eq!(canon.labels(), "I V vi iii IV I IV V");
        let mut rng = SplitMix64::new(1);
        assert!(Grammar::stock(&mut rng, &g, &[StockProgression::Folia]).is_none());
    }
}
