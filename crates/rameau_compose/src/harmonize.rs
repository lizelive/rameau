//! Harmonising a fixed melody with chords the period would have used.
//!
//! A dynamic programme over the grammar: every half-bar (or bar) chooses one
//! chord from a candidate set, paying for melody notes that are not chord
//! tones (more on strong beats, less for short passing notes), for
//! transitions the grammar dislikes, and for failing the cadence asked for
//! at the end.

use rameau_chords::{Cadence, Grammar, RomanNumeral};
use rameau_theory::{BeatStrength, Meter, Midi, Scale};
use rameau_types::Rng;

use crate::idea::IdeaEvent;

/// A melody note within a bar: onset (crotchets from the barline), duration
/// and MIDI key.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MelodyNote {
    /// Onset in crotchets from the barline.
    pub onset: f64,
    /// Duration in crotchets.
    pub duration: f64,
    /// MIDI key.
    pub midi: Midi,
}

/// A chord and where in the bar it starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChordSlot {
    /// Onset in crotchets from the barline.
    pub onset: f64,
    /// Length in crotchets.
    pub duration: f64,
    /// The chord.
    pub chord: RomanNumeral,
}

/// The candidate chords in `scale`, in a fixed order.
pub fn candidates(scale: &Scale) -> Vec<RomanNumeral> {
    let d = |k: i32| RomanNumeral::diatonic(scale, k);
    let mut out = vec![
        d(0),
        d(1),
        d(2),
        d(3),
        d(4),
        d(5),
        d(6).inv(1),
        d(0).inv(1),
        d(1).inv(1),
        d(3).inv(1),
        d(4).inv(1),
        RomanNumeral::diatonic_seventh(scale, 4),
        RomanNumeral::diatonic_seventh(scale, 4).inv(1),
        d(0).inv(2),
    ];
    if scale.mode.is_minor() {
        out.push(RomanNumeral::subtonic());
    }
    out
}

/// Cost of `chord` against the melody notes overlapping `[onset, onset + duration)`.
fn melody_cost(
    scale: &Scale,
    meter: &Meter,
    chord: &RomanNumeral,
    notes: &[MelodyNote],
    onset: f64,
    duration: f64,
) -> f64 {
    let end = onset + duration;
    let mut cost = 0.0;
    let mut covered = 0.0;
    let mut has_third = false;
    let pcs = chord.pitch_classes(scale);
    let third = pcs.get(1).copied();
    for n in notes {
        let overlap = (n.onset + n.duration).min(end) - n.onset.max(onset);
        if overlap <= 1e-9 {
            continue;
        }
        let strength = meter.strength_at(n.onset);
        let weight = overlap
            * match strength {
                BeatStrength::Downbeat => 2.5,
                BeatStrength::Strong => 1.8,
                BeatStrength::Weak => 1.2,
                BeatStrength::Off => 0.7,
            };
        covered += overlap;
        if chord.contains(scale, n.midi) {
            if third == Some(rameau_theory::PitchClass::of_midi(n.midi)) {
                has_third = true;
            }
            continue;
        }
        // Short notes off the beat are passing tones; long ones on the beat
        // are wrong.
        let short = n.duration <= 0.5 && strength == BeatStrength::Off;
        cost += weight * if short { 0.35 } else { 1.6 };
    }
    // Reward a chord whose third the melody actually states.
    if has_third {
        cost -= 0.2 * covered.min(1.0);
    }
    cost
}

/// Chooses chords for `bars` of melody in `scale`.
///
/// `slots_per_bar` is the harmonic rhythm (1 or 2 chords per bar). If a
/// `cadence` is given, the last two slots are pushed to end on it. `start`
/// is the chord the previous phrase ended on, if any. `noise` in `0..1`
/// adds random jitter so repeated harmonisations of the same tune differ.
#[expect(
    clippy::too_many_arguments,
    reason = "a harmonisation is defined by exactly these independent inputs"
)]
pub fn harmonize(
    rng: &mut impl Rng,
    grammar: &Grammar,
    scale: &Scale,
    meter: &Meter,
    bars: &[Vec<MelodyNote>],
    slots_per_bar: usize,
    cadence: Option<Cadence>,
    start: Option<RomanNumeral>,
    noise: f64,
) -> Vec<Vec<ChordSlot>> {
    let cands = candidates(scale);
    let n_c = cands.len();
    let spb = slots_per_bar.max(1);
    let bar_len = meter.bar_quarters();
    let slot_len = bar_len / spb as f64;
    let n_slots = bars.len() * spb;
    if n_slots == 0 {
        return Vec::new();
    }

    // Per-slot local costs.
    let mut local: Vec<Vec<f64>> = Vec::with_capacity(n_slots);
    for (b, notes) in bars.iter().enumerate() {
        for s in 0..spb {
            let onset = s as f64 * slot_len;
            let row: Vec<f64> = cands
                .iter()
                .map(|c| {
                    let mut cost = melody_cost(scale, meter, c, notes, onset, slot_len);
                    // Second-inversion tonic only as a cadential 6-4.
                    if c.is_tonic() && c.inversion == 2 {
                        cost += 1.5;
                    }
                    // Prefer to open the phrase on the tonic.
                    if b == 0 && s == 0 && !c.is_tonic() {
                        cost += 1.0;
                    }
                    cost + noise * rng.next_f64() * 0.6
                })
                .collect();
            local.push(row);
        }
    }

    // Cadence: bias the final slots.
    let cadence_tail: Vec<RomanNumeral> = cadence.map(|c| c.chords(scale)).unwrap_or_default();
    let tail_len = cadence_tail.len().min(2).min(n_slots);
    for (k, target) in cadence_tail.iter().rev().take(tail_len).enumerate() {
        let slot = n_slots - 1 - k;
        if let Some(row) = local.get_mut(slot) {
            for (ci, c) in cands.iter().enumerate() {
                let same = c.degree == target.degree && c.root_alteration == target.root_alteration;
                if let Some(v) = row.get_mut(ci) {
                    if !same {
                        *v += 4.0;
                    } else if c.inversion != target.inversion {
                        *v += 0.8;
                    }
                }
            }
        }
    }

    // Transition costs from the grammar.
    let trans = |from: &RomanNumeral, to: &RomanNumeral| -> f64 {
        let w = grammar.weight(from.degree, to.degree).max(0.05);
        let mut c = -(w / 6.0).ln() * 0.6;
        if from.degree == to.degree && from.root_alteration == to.root_alteration {
            // Holding a chord is fine; changing only its inversion is nice.
            c = if from.inversion == to.inversion { 0.45 } else { 0.15 };
        }
        // V7 wants to resolve.
        if from.quality.has_seventh() && from.degree == 4 && !(to.degree == 0 || to.degree == 5) {
            c += 1.5;
        }
        c
    };

    // Viterbi.
    let mut best: Vec<Vec<f64>> = vec![vec![0.0; n_c]; n_slots];
    let mut back: Vec<Vec<usize>> = vec![vec![0; n_c]; n_slots];
    for ci in 0..n_c {
        let l = local.first().and_then(|r| r.get(ci)).copied().unwrap_or(0.0);
        let entry = match (&start, cands.get(ci)) {
            (Some(s), Some(c)) => trans(s, c),
            _ => 0.0,
        };
        if let Some(slot) = best.first_mut().and_then(|r| r.get_mut(ci)) {
            *slot = l + entry;
        }
    }
    for t in 1..n_slots {
        for ci in 0..n_c {
            let mut bmin = f64::INFINITY;
            let mut barg = 0;
            for pi in 0..n_c {
                let prev = best.get(t - 1).and_then(|r| r.get(pi)).copied().unwrap_or(f64::INFINITY);
                let (Some(pc), Some(cc)) = (cands.get(pi), cands.get(ci)) else { continue };
                let v = prev + trans(pc, cc);
                if v < bmin {
                    bmin = v;
                    barg = pi;
                }
            }
            let l = local.get(t).and_then(|r| r.get(ci)).copied().unwrap_or(0.0);
            if let Some(slot) = best.get_mut(t).and_then(|r| r.get_mut(ci)) {
                *slot = bmin + l;
            }
            if let Some(slot) = back.get_mut(t).and_then(|r| r.get_mut(ci)) {
                *slot = barg;
            }
        }
    }
    // Trace back.
    let last = best
        .last()
        .map(|row| {
            row.iter()
                .enumerate()
                .min_by(|a, b| a.1.total_cmp(b.1))
                .map_or(0, |(i, _)| i)
        })
        .unwrap_or(0);
    let mut path = vec![0usize; n_slots];
    let mut cur = last;
    for t in (0..n_slots).rev() {
        if let Some(p) = path.get_mut(t) {
            *p = cur;
        }
        cur = back.get(t).and_then(|r| r.get(cur)).copied().unwrap_or(0);
    }

    path.chunks(spb)
        .map(|chunk| {
            chunk
                .iter()
                .enumerate()
                .map(|(s, &ci)| ChordSlot {
                    onset: s as f64 * slot_len,
                    duration: slot_len,
                    chord: cands.get(ci).copied().unwrap_or_default(),
                })
                .collect()
        })
        .collect()
}

/// The chords a ground bass implies, one per bass note: the triad on the
/// bass degree, with the usual first-inversion readings of the third,
/// sixth and seventh degrees.
pub fn ground_chords(scale: &Scale, events: &[IdeaEvent], bar_len: f64) -> Vec<Vec<ChordSlot>> {
    let mut bars: Vec<Vec<ChordSlot>> = Vec::new();
    let mut t = 0.0;
    for e in events {
        let bar = (t / bar_len + 1e-9).floor() as usize;
        while bars.len() <= bar {
            bars.push(Vec::new());
        }
        if let Some(n) = e.note {
            let d = n.degree as i32;
            let chord = match d {
                2 => RomanNumeral::diatonic(scale, 0).inv(1),
                6 if scale.mode.is_minor() && n.alteration == 0 => RomanNumeral::subtonic(),
                6 => RomanNumeral::diatonic(scale, 4).inv(1),
                _ => RomanNumeral::diatonic(scale, d),
            };
            if let Some(b) = bars.get_mut(bar) {
                b.push(ChordSlot {
                    onset: t - bar as f64 * bar_len,
                    duration: e.beats,
                    chord,
                });
            }
        }
        t += e.beats;
    }
    bars
}

/// The chord sounding at `onset` within a bar's slots.
pub fn chord_at(slots: &[ChordSlot], onset: f64) -> Option<&RomanNumeral> {
    slots
        .iter()
        .rev()
        .find(|s| s.onset <= onset + 1e-9)
        .or_else(|| slots.first())
        .map(|s| &s.chord)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rameau_theory::{Mode, PitchClass};
    use rameau_types::SplitMix64;

    fn n(onset: f64, duration: f64, midi: Midi) -> MelodyNote {
        MelodyNote {
            onset,
            duration,
            midi,
        }
    }

    #[test]
    fn harmonises_ca_ira_head_on_tonic_and_dominant() {
        // G major, 2/4: bars G G A G | G G A G(long) | ... a very tonic tune.
        let g = Scale::new(PitchClass::G, Mode::Major);
        let m = Meter::new(2, 4);
        let bar1 = vec![n(0.0, 0.5, 67), n(0.5, 0.25, 67), n(0.75, 0.25, 69), n(1.0, 0.5, 67), n(1.5, 0.25, 67), n(1.75, 0.25, 69)];
        let bar2 = vec![n(0.0, 0.5, 67), n(0.5, 0.25, 67), n(0.75, 0.25, 69), n(1.0, 1.0, 67)];
        let bar3 = vec![n(0.0, 1.0, 74), n(1.0, 1.0, 72)];
        let bar4 = vec![n(0.0, 1.0, 71), n(1.0, 1.0, 67)];
        let mut rng = SplitMix64::new(4);
        let out = harmonize(
            &mut rng,
            &Grammar::period(),
            &g,
            &m,
            &[bar1, bar2, bar3, bar4],
            1,
            Some(Cadence::Authentic),
            None,
            0.0,
        );
        assert_eq!(out.len(), 4);
        assert!(out[0][0].chord.is_tonic(), "{}", out[0][0].chord);
        assert!(out[3][0].chord.is_tonic());
        assert_eq!(out[2][0].chord.degree, 4, "D and C over V7: {}", out[2][0].chord);
    }

    #[test]
    fn ground_implies_chords() {
        let d = Scale::new(PitchClass::D, Mode::Minor);
        let ev = vec![
            IdeaEvent::note(0, 0, 0, 3.0),
            IdeaEvent::note(6, 0, -1, 3.0),
            IdeaEvent::note(5, 0, -1, 3.0),
            IdeaEvent::note(4, 0, -1, 3.0),
        ];
        let bars = ground_chords(&d, &ev, 3.0);
        assert_eq!(bars.len(), 4);
        assert_eq!(bars[1][0].chord, RomanNumeral::subtonic());
        assert_eq!(bars[3][0].chord.degree, 4);
        assert!(chord_at(&bars[0], 1.0).unwrap().is_tonic());
    }
}
