//! Intervals in semitones, with the classifications counterpoint cares about.

/// A signed interval in semitones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Interval(pub i32);

impl Interval {
    /// The interval from `a` up to `b` (negative if `b` is lower).
    pub const fn between(a: i32, b: i32) -> Self {
        Self(b - a)
    }

    /// Size in semitones, sign preserved.
    pub const fn semitones(self) -> i32 {
        self.0
    }

    /// Unsigned size in semitones.
    pub const fn abs(self) -> i32 {
        self.0.abs()
    }

    /// The interval reduced to within an octave, `0..=11`.
    pub const fn simple(self) -> i32 {
        self.0.rem_euclid(12)
    }

    /// A unison or any number of octaves.
    pub const fn is_perfect_octave(self) -> bool {
        self.simple() == 0
    }

    /// A perfect fifth, compound or not (a twelfth counts).
    pub const fn is_perfect_fifth(self) -> bool {
        self.simple() == 7
    }

    /// A perfect fourth (dissonant against the bass in strict style).
    pub const fn is_perfect_fourth(self) -> bool {
        self.simple() == 5
    }

    /// Any perfect consonance: unison, octave or fifth.
    pub const fn is_perfect_consonance(self) -> bool {
        self.is_perfect_octave() || self.is_perfect_fifth()
    }

    /// A major or minor third or sixth.
    pub const fn is_imperfect_consonance(self) -> bool {
        matches!(self.simple(), 3 | 4 | 8 | 9)
    }

    /// Consonant in the strict sense (fourths are treated as dissonant).
    pub const fn is_consonant(self) -> bool {
        self.is_perfect_consonance() || self.is_imperfect_consonance()
    }

    /// A tritone (augmented fourth / diminished fifth).
    pub const fn is_tritone(self) -> bool {
        self.simple() == 6
    }

    /// Melodic motion by step: a minor or major second.
    pub const fn is_step(self) -> bool {
        matches!(self.abs(), 1 | 2)
    }

    /// Melodic motion by leap: anything beyond a second.
    pub const fn is_leap(self) -> bool {
        self.abs() > 2
    }

    /// A leap counterpoint frowns on: sevenths, anything past an octave, or a
    /// tritone.
    pub const fn is_awkward_leap(self) -> bool {
        let a = self.abs();
        a == 6 || a == 10 || a == 11 || a > 12
    }

    /// Direction of motion: `-1`, `0` or `1`.
    pub const fn direction(self) -> i32 {
        self.0.signum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies() {
        assert!(Interval(7).is_perfect_fifth());
        assert!(Interval(19).is_perfect_fifth());
        assert!(Interval(-12).is_perfect_octave());
        assert!(Interval(4).is_imperfect_consonance());
        assert!(!Interval(5).is_consonant());
        assert!(Interval(6).is_awkward_leap());
        assert!(Interval(13).is_awkward_leap());
        assert!(!Interval(12).is_awkward_leap());
        assert!(Interval(-2).is_step());
    }
}
