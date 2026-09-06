//! The red-pen rules: parallel perfects, crossing, awkward leaps, foreign
//! notes.
//!
//! Every function here is pure and returns *what* it found, never a score;
//! deciding how much a parallel fifth costs is the composer's business (and,
//! in a game, may depend on how much of the town is on fire).
//!
//! A *simultaneity* is a slice with one optional MIDI key per voice,
//! **highest voice first**. `None` is a rest.

use crate::interval::Interval;
use crate::pitch::Midi;
use crate::scale::Scale;

/// Relative motion between two voices across two simultaneities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// Both move the same direction by the same interval.
    Parallel,
    /// Both move the same direction by different intervals.
    Similar,
    /// They move in opposite directions.
    Contrary,
    /// One holds while the other moves.
    Oblique,
    /// Neither moves.
    Static,
}

/// Classifies the motion of a voice going `a0 → a1` against one going
/// `b0 → b1`.
pub fn motion(a0: Midi, a1: Midi, b0: Midi, b1: Midi) -> Motion {
    let da = a1 - a0;
    let db = b1 - b0;
    match (da.signum(), db.signum()) {
        (0, 0) => Motion::Static,
        (0, _) | (_, 0) => Motion::Oblique,
        (x, y) if x == y => {
            if da == db { Motion::Parallel } else { Motion::Similar }
        }
        _ => Motion::Contrary,
    }
}

/// What a rule found wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaultKind {
    /// Two voices moved in parallel perfect fifths.
    ParallelFifths,
    /// Two voices moved in parallel octaves or unisons.
    ParallelOctaves,
    /// Outer voices reached a fifth by similar motion with the top leaping.
    DirectFifth,
    /// Outer voices reached an octave by similar motion with the top leaping.
    DirectOctave,
    /// A lower voice sounds above a higher one.
    VoiceCrossing,
    /// A voice moved past where its neighbour just was.
    VoiceOverlap,
    /// Adjacent upper voices more than an octave apart.
    WideSpacing,
    /// A melodic tritone, seventh, or leap past an octave.
    AwkwardLeap,
    /// The leading tone in an outer voice did not rise to the tonic.
    UnresolvedLeadingTone,
    /// A note foreign to the scale.
    OutOfKey,
    /// A dissonance against the bass.
    Dissonance,
}

/// One rule violation, with the voices involved (`(i, i)` for a single voice).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fault {
    /// Which rule.
    pub kind: FaultKind,
    /// Indices of the voices involved, highest voice first.
    pub voices: (usize, usize),
}

impl Fault {
    const fn new(kind: FaultKind, i: usize, j: usize) -> Self {
        Self { kind, voices: (i, j) }
    }
}

/// Pairs of voices sounding in both simultaneities.
fn sounding_pairs<'a>(
    prev: &'a [Option<Midi>],
    next: &'a [Option<Midi>],
) -> impl Iterator<Item = (usize, usize, Midi, Midi, Midi, Midi)> + 'a {
    let n = prev.len().min(next.len());
    (0..n).flat_map(move |i| {
        (i + 1..n).filter_map(move |j| {
            let a0 = prev.get(i).copied().flatten()?;
            let a1 = next.get(i).copied().flatten()?;
            let b0 = prev.get(j).copied().flatten()?;
            let b1 = next.get(j).copied().flatten()?;
            Some((i, j, a0, a1, b0, b1))
        })
    })
}

/// Parallel and direct perfect intervals between two successive
/// simultaneities.
///
/// Parallels are counted between any pair of voices; direct (hidden) perfects
/// only between the outer voices, and only when the top voice leaps, which
/// is the usual eighteenth-century allowance.
pub fn perfect_parallels(prev: &[Option<Midi>], next: &[Option<Midi>]) -> Vec<Fault> {
    let mut out = Vec::new();
    let last = prev.len().min(next.len()).saturating_sub(1);
    for (i, j, a0, a1, b0, b1) in sounding_pairs(prev, next) {
        let before = Interval::between(b0, a0);
        let after = Interval::between(b1, a1);
        let m = motion(a0, a1, b0, b1);
        // A repeated chord is not a parallel.
        if matches!(m, Motion::Static | Motion::Oblique | Motion::Contrary) {
            continue;
        }
        if after.is_perfect_fifth() && before.is_perfect_fifth() {
            out.push(Fault::new(FaultKind::ParallelFifths, i, j));
        } else if after.is_perfect_octave() && before.is_perfect_octave() {
            out.push(Fault::new(FaultKind::ParallelOctaves, i, j));
        } else if i == 0 && j == last && !Interval::between(a0, a1).is_step() {
            if after.is_perfect_fifth() {
                out.push(Fault::new(FaultKind::DirectFifth, i, j));
            } else if after.is_perfect_octave() {
                out.push(Fault::new(FaultKind::DirectOctave, i, j));
            }
        }
    }
    out
}

/// Crossing and spacing faults inside one simultaneity.
///
/// Voices are expected highest-first; a lower-indexed voice sounding below a
/// higher-indexed one is a crossing. Upper voices (all but the lowest) more
/// than an octave from their neighbour are flagged as widely spaced.
pub fn vertical_faults(chord: &[Option<Midi>]) -> Vec<Fault> {
    let mut out = Vec::new();
    let sounding: Vec<(usize, Midi)> = chord
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.map(|m| (i, m)))
        .collect();
    let n = sounding.len();
    for (k, &(i, upper)) in sounding.iter().enumerate() {
        for &(j, lower) in sounding.iter().skip(k + 1) {
            if lower > upper {
                out.push(Fault::new(FaultKind::VoiceCrossing, i, j));
            }
        }
        if let Some(&(j, below)) = sounding.get(k + 1) {
            // The gap above the bass may be wide; gaps between upper voices
            // should stay within an octave.
            let is_bass_gap = k + 2 == n;
            if !is_bass_gap && upper - below > 12 {
                out.push(Fault::new(FaultKind::WideSpacing, i, j));
            }
        }
    }
    out
}

/// Overlaps between successive simultaneities: a voice moving past the
/// position its neighbour just left.
pub fn overlaps(prev: &[Option<Midi>], next: &[Option<Midi>]) -> Vec<Fault> {
    let mut out = Vec::new();
    for (i, j, a0, a1, b0, b1) in sounding_pairs(prev, next) {
        if j != i + 1 {
            continue;
        }
        if a1 < b0 || b1 > a0 {
            out.push(Fault::new(FaultKind::VoiceOverlap, i, j));
        }
    }
    out
}

/// Melodic faults for every voice that moves between `prev` and `next`.
pub fn melodic_faults(prev: &[Option<Midi>], next: &[Option<Midi>]) -> Vec<Fault> {
    prev.iter()
        .zip(next)
        .enumerate()
        .filter_map(|(i, (a, b))| {
            let (a, b) = ((*a)?, (*b)?);
            Interval::between(a, b)
                .is_awkward_leap()
                .then_some(Fault::new(FaultKind::AwkwardLeap, i, i))
        })
        .collect()
}

/// Leading tones in the outer voices of `prev` that fail to rise by semitone
/// to the tonic in `next` (or hold).
pub fn leading_tone_faults(
    scale: &Scale,
    prev: &[Option<Midi>],
    next: &[Option<Midi>],
) -> Vec<Fault> {
    let Some(lt_degree) = scale.mode.leading_tone_degree() else {
        return Vec::new();
    };
    let lt = scale.pitch_class(lt_degree as i32);
    let n = prev.len().min(next.len());
    let outer: [usize; 2] = [0, n.saturating_sub(1)];
    let mut out = Vec::new();
    for &i in outer.iter().take(if n > 1 { 2 } else { 1 }) {
        let (Some(a), Some(b)) = (
            prev.get(i).copied().flatten(),
            next.get(i).copied().flatten(),
        ) else {
            continue;
        };
        if crate::pitch::PitchClass::of_midi(a) == lt && b != a && b != a + 1 {
            out.push(Fault::new(FaultKind::UnresolvedLeadingTone, i, i));
        }
    }
    out
}

/// Voices in `chord` sounding a note foreign to `scale`.
pub fn out_of_key(scale: &Scale, chord: &[Option<Midi>]) -> Vec<Fault> {
    chord
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let m = (*p)?;
            (!scale.contains(m)).then_some(Fault::new(FaultKind::OutOfKey, i, i))
        })
        .collect()
}

/// Upper voices dissonant against the lowest sounding voice (seconds,
/// sevenths, tritones and — in strict style — perfect fourths).
pub fn dissonances_against_bass(chord: &[Option<Midi>]) -> Vec<Fault> {
    let sounding: Vec<(usize, Midi)> = chord
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.map(|m| (i, m)))
        .collect();
    let Some(&(bass_i, bass)) = sounding.last() else {
        return Vec::new();
    };
    sounding
        .iter()
        .filter(|(i, _)| *i != bass_i)
        .filter_map(|&(i, m)| {
            (!Interval::between(bass, m).is_consonant())
                .then_some(Fault::new(FaultKind::Dissonance, i, bass_i))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::PitchClass;
    use crate::scale::Mode;

    #[test]
    fn finds_parallel_fifths_and_octaves() {
        // C-G moving to D-A: parallel fifths between the two voices.
        let f = perfect_parallels(&[Some(67), Some(60)], &[Some(69), Some(62)]);
        assert_eq!(f, vec![Fault::new(FaultKind::ParallelFifths, 0, 1)]);
        let o = perfect_parallels(&[Some(72), Some(60)], &[Some(74), Some(62)]);
        assert_eq!(o.first().map(|f| f.kind), Some(FaultKind::ParallelOctaves));
        // Contrary motion into an octave is fine.
        assert!(perfect_parallels(&[Some(67), Some(64)], &[Some(72), Some(60)]).is_empty());
        // Repeating a fifth is not a parallel.
        assert!(perfect_parallels(&[Some(67), Some(60)], &[Some(67), Some(60)]).is_empty());
    }

    #[test]
    fn direct_fifths_only_with_leaping_top() {
        let d = perfect_parallels(&[Some(64), Some(60)], &[Some(69), Some(62)]);
        assert_eq!(d.first().map(|f| f.kind), Some(FaultKind::DirectFifth));
        let ok = perfect_parallels(&[Some(67), Some(57)], &[Some(69), Some(62)]);
        assert!(ok.is_empty());
    }

    #[test]
    fn vertical() {
        let f = vertical_faults(&[Some(60), Some(64), Some(48)]);
        assert_eq!(f, vec![Fault::new(FaultKind::VoiceCrossing, 0, 1)]);
        let w = vertical_faults(&[Some(79), Some(60), Some(48)]);
        assert_eq!(w, vec![Fault::new(FaultKind::WideSpacing, 0, 1)]);
        assert!(vertical_faults(&[Some(67), Some(64), Some(36)]).is_empty());
    }

    #[test]
    fn leading_tone_and_key() {
        let c = Scale::new(PitchClass::C, Mode::Major);
        let bad = leading_tone_faults(&c, &[Some(71), Some(55)], &[Some(67), Some(60)]);
        assert_eq!(bad.len(), 1);
        let good = leading_tone_faults(&c, &[Some(71), Some(55)], &[Some(72), Some(60)]);
        assert!(good.is_empty());
        assert_eq!(out_of_key(&c, &[Some(61), Some(60)]).len(), 1);
        assert_eq!(dissonances_against_bass(&[Some(65), Some(60)]).len(), 1);
        assert!(dissonances_against_bass(&[Some(64), Some(60)]).is_empty());
        assert_eq!(overlaps(&[Some(67), Some(60)], &[Some(58), Some(55)]).len(), 1);
    }
}
