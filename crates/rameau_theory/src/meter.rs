//! Time signatures and metric accent.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A time signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Meter {
    /// Beats per bar (the numerator).
    pub beats: u8,
    /// The note value of one beat (the denominator: 4 = crotchet).
    pub unit: u8,
}

/// How strong a metric position is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BeatStrength {
    /// Between beats.
    Off,
    /// A weak beat.
    Weak,
    /// A secondary strong beat (beat 3 of 4, beat 4 of 6/8).
    Strong,
    /// The first beat of the bar.
    Downbeat,
}

impl Meter {
    /// Creates a meter.
    pub const fn new(beats: u8, unit: u8) -> Self {
        Self { beats, unit }
    }

    /// Common time.
    pub const COMMON: Self = Self::new(4, 4);

    /// Parses `"6/8"`.
    pub fn parse(text: &str) -> Option<Self> {
        let (b, u) = text.trim().trim_start_matches("*M").split_once('/')?;
        Some(Self::new(b.trim().parse().ok()?, u.trim().parse().ok()?))
    }

    /// Length of one bar in crotchets.
    pub fn bar_quarters(&self) -> f64 {
        f64::from(self.beats) * 4.0 / f64::from(self.unit.max(1))
    }

    /// Compound meters (6/8, 9/8, 12/8) group their beats in threes.
    pub const fn is_compound(&self) -> bool {
        self.unit >= 8 && self.beats >= 6 && self.beats.is_multiple_of(3)
    }

    /// Length of the felt pulse in crotchets: a dotted crotchet in 6/8, a
    /// crotchet in 4/4, a minim in 2/2.
    pub fn pulse_quarters(&self) -> f64 {
        if self.is_compound() {
            3.0 * 4.0 / f64::from(self.unit.max(1))
        } else {
            4.0 / f64::from(self.unit.max(1))
        }
    }

    /// Number of felt pulses per bar.
    pub fn pulses(&self) -> u8 {
        if self.is_compound() { self.beats / 3 } else { self.beats }
    }

    /// The smallest subdivision the composer writes on: a semiquaver,
    /// expressed in crotchets.
    pub const fn grid_quarters(&self) -> f64 {
        0.25
    }

    /// Number of grid slots in a bar.
    pub fn grid_slots(&self) -> usize {
        (self.bar_quarters() / self.grid_quarters()).round() as usize
    }

    /// Metric strength of a position given in crotchets from the downbeat.
    pub fn strength_at(&self, quarters: f64) -> BeatStrength {
        let pulse = self.pulse_quarters();
        let pos = quarters.rem_euclid(self.bar_quarters());
        let pulse_index = pos / pulse;
        let frac = pulse_index - pulse_index.floor();
        if frac > 1e-6 && (1.0 - frac) > 1e-6 {
            return BeatStrength::Off;
        }
        let beat = pulse_index.round() as u8;
        if beat == 0 {
            BeatStrength::Downbeat
        } else if self.pulses() >= 4 && beat == self.pulses() / 2 {
            BeatStrength::Strong
        } else {
            BeatStrength::Weak
        }
    }
}

impl Default for Meter {
    fn default() -> Self {
        Self::COMMON
    }
}

impl core::fmt::Display for Meter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}/{}", self.beats, self.unit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compound_and_simple() {
        let m = Meter::parse("6/8").unwrap();
        assert!(m.is_compound());
        assert_eq!(m.bar_quarters(), 3.0);
        assert_eq!(m.pulse_quarters(), 1.5);
        assert_eq!(m.pulses(), 2);
        assert_eq!(m.grid_slots(), 12);
        assert_eq!(m.strength_at(0.0), BeatStrength::Downbeat);
        assert_eq!(m.strength_at(1.5), BeatStrength::Weak);
        assert_eq!(m.strength_at(0.5), BeatStrength::Off);

        let c = Meter::COMMON;
        assert_eq!(c.strength_at(2.0), BeatStrength::Strong);
        assert_eq!(c.strength_at(1.0), BeatStrength::Weak);
        assert_eq!(c.grid_slots(), 16);
        let cut = Meter::parse("2/2").unwrap();
        assert_eq!(cut.bar_quarters(), 4.0);
        assert_eq!(cut.pulse_quarters(), 2.0);
    }
}
