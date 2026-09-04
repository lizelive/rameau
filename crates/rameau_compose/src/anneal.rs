//! Simulated annealing of one bar.
//!
//! A bar is a grid of semiquaver slots per voice. Slots that carry idea
//! material are *fixed*; the rest are *free* and the annealer rewrites them:
//! re-pitching notes to chord or scale tones, splitting and merging
//! durations, resting and un-resting, shifting octaves. Every proposal is
//! scored by [`cost`], a weighted sum of what a counterpoint teacher, a
//! continuo player and the game each want, and accepted by the Metropolis
//! rule at a falling temperature.

use rameau_chords::RomanNumeral;
use rameau_theory::voice_leading::{self as vl, FaultKind};
use rameau_theory::{BeatStrength, Interval, Meter, Midi, Scale};
use rameau_types::Rng;

use crate::harmonize::{ChordSlot, chord_at};
use crate::instrument::Role;
use crate::state::RuleWeights;

/// One voice's grid for a bar.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceGrid {
    /// The pitch sounding in each slot, `None` for a rest.
    pub pitch: Vec<Option<Midi>>,
    /// Whether a new note begins in each slot.
    pub onset: Vec<bool>,
    /// Whether each slot is fixed (idea material the annealer may not touch).
    pub fixed: Vec<bool>,
    /// Whether the annealer may change this voice at all.
    pub free: bool,
    /// Playing range.
    pub range: (Midi, Midi),
    /// The voice's role.
    pub role: Role,
    /// The pitch sounding at the end of the previous bar, for voice leading
    /// across the barline.
    pub prev_pitch: Option<Midi>,
    /// Target number of onsets in the bar for this voice.
    pub target_onsets: f64,
}

impl VoiceGrid {
    /// An empty (resting), free voice of `slots` slots.
    pub fn empty(slots: usize, role: Role, range: (Midi, Midi)) -> Self {
        Self {
            pitch: vec![None; slots],
            onset: vec![false; slots],
            fixed: vec![false; slots],
            free: true,
            range,
            role,
            prev_pitch: None,
            target_onsets: 2.0,
        }
    }

    /// Writes a note covering `start..start + len`, marking it fixed if
    /// `fixed`. A `None` pitch writes a rest.
    pub fn write(&mut self, start: usize, len: usize, pitch: Option<Midi>, fixed: bool) {
        for k in start..(start + len).min(self.pitch.len()) {
            if let Some(p) = self.pitch.get_mut(k) {
                *p = pitch;
            }
            if let Some(o) = self.onset.get_mut(k) {
                *o = k == start && pitch.is_some();
            }
            if let Some(f) = self.fixed.get_mut(k) {
                *f = fixed;
            }
        }
    }

    /// Continues a note from the previous bar (sounding, no onset) for `len`
    /// slots.
    pub fn tie_in(&mut self, len: usize, pitch: Midi, fixed: bool) {
        self.write(0, len, Some(pitch), fixed);
        if let Some(o) = self.onset.first_mut() {
            *o = false;
        }
    }

    /// The notes in the bar as `(start slot, length in slots, pitch)`, rests
    /// omitted.
    pub fn notes(&self) -> Vec<(usize, usize, Midi)> {
        let mut out = Vec::new();
        let n = self.pitch.len();
        let mut k = 0;
        while k < n {
            let Some(Some(p)) = self.pitch.get(k).copied() else {
                k += 1;
                continue;
            };
            let start = k;
            k += 1;
            while k < n {
                match (self.pitch.get(k).copied().flatten(), self.onset.get(k).copied()) {
                    (Some(q), Some(false)) if q == p => k += 1,
                    _ => break,
                }
            }
            // A note tied from the previous bar has no onset at slot 0 but
            // still counts as sounding material.
            out.push((start, k - start, p));
        }
        out
    }

    /// Number of onsets.
    pub fn onset_count(&self) -> usize {
        self.onset.iter().filter(|o| **o).count()
    }

    /// Whether slot `k` may be edited.
    fn editable(&self, k: usize) -> bool {
        self.free && !self.fixed.get(k).copied().unwrap_or(true)
    }

    /// The slot range `[start, end)` of the note sounding at `k`.
    fn note_span(&self, k: usize) -> Option<(usize, usize)> {
        self.pitch.get(k).copied().flatten()?;
        let mut start = k;
        while start > 0 && !self.onset.get(start).copied().unwrap_or(true) {
            start -= 1;
        }
        let mut end = k + 1;
        while end < self.pitch.len()
            && !self.onset.get(end).copied().unwrap_or(true)
            && self.pitch.get(end).copied().flatten().is_some()
        {
            end += 1;
        }
        Some((start, end))
    }

    fn span_editable(&self, start: usize, end: usize) -> bool {
        (start..end).all(|k| self.editable(k))
    }

    /// The last sounding pitch before slot `k` (or the previous bar's).
    fn pitch_before(&self, k: usize) -> Option<Midi> {
        (0..k)
            .rev()
            .find_map(|j| self.pitch.get(j).copied().flatten())
            .or(self.prev_pitch)
    }
}

/// The bar being composed.
#[derive(Debug, Clone, PartialEq)]
pub struct BarGrid {
    /// Slots per bar.
    pub slots: usize,
    /// Crotchets per slot.
    pub slot_beats: f64,
    /// Voices, highest first.
    pub voices: Vec<VoiceGrid>,
}

impl BarGrid {
    /// An empty grid for `meter` with no voices.
    pub fn new(meter: &Meter) -> Self {
        Self {
            slots: meter.grid_slots(),
            slot_beats: meter.grid_quarters(),
            voices: Vec::new(),
        }
    }

    /// Adds a voice and returns its index.
    pub fn add_voice(&mut self, role: Role, range: (Midi, Midi)) -> usize {
        self.voices.push(VoiceGrid::empty(self.slots, role, range));
        self.voices.len() - 1
    }

    /// The simultaneity at slot `k`, highest voice first.
    pub fn simultaneity(&self, k: usize) -> Vec<Option<Midi>> {
        self.voices
            .iter()
            .map(|v| v.pitch.get(k).copied().flatten())
            .collect()
    }

    /// Slots at which any voice has an onset.
    pub fn change_points(&self) -> Vec<usize> {
        (0..self.slots)
            .filter(|&k| self.voices.iter().any(|v| v.onset.get(k).copied().unwrap_or(false)))
            .collect()
    }

    /// The last simultaneity of the bar.
    pub fn final_simultaneity(&self) -> Vec<Option<Midi>> {
        self.voices
            .iter()
            .map(|v| v.pitch.last().copied().flatten())
            .collect()
    }

    /// The bar as a fingerprint of onsets and contour per voice, for the
    /// repetition memory.
    pub fn fingerprint(&self) -> Vec<Fingerprint> {
        self.voices.iter().map(Fingerprint::of).collect()
    }
}

/// A compact description of one voice's bar: its onset pattern and the
/// signs of its melodic intervals.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fingerprint {
    /// One bit per slot.
    pub onsets: Vec<bool>,
    /// Interval in semitones between successive notes, clamped to ±12.
    pub intervals: Vec<i8>,
}

impl Fingerprint {
    /// The fingerprint of a voice.
    pub fn of(v: &VoiceGrid) -> Self {
        let notes = v.notes();
        let intervals = notes
            .windows(2)
            .filter_map(|w| match (w.first(), w.get(1)) {
                (Some(a), Some(b)) => Some((b.2 - a.2).clamp(-12, 12) as i8),
                _ => None,
            })
            .collect();
        Self {
            onsets: v.onset.clone(),
            intervals,
        }
    }

    /// Similarity in `0..=1`: half rhythm agreement, half contour agreement.
    pub fn similarity(&self, other: &Self) -> f64 {
        let n = self.onsets.len().max(1);
        let rhythm = self
            .onsets
            .iter()
            .zip(&other.onsets)
            .filter(|(a, b)| a == b)
            .count() as f64
            / n as f64;
        let m = self.intervals.len().max(other.intervals.len());
        let contour = if m == 0 {
            1.0
        } else {
            self.intervals
                .iter()
                .zip(&other.intervals)
                .filter(|(a, b)| a == b)
                .count() as f64
                / m as f64
        };
        // An all-rest bar matches every other all-rest bar; that is fine.
        0.5 * rhythm + 0.5 * contour
    }
}

/// Everything the cost function needs besides the grid.
#[derive(Debug, Clone)]
pub struct Context<'a> {
    /// The key.
    pub scale: Scale,
    /// The meter.
    pub meter: Meter,
    /// The chords of the bar.
    pub chords: &'a [ChordSlot],
    /// Rule weights.
    pub weights: RuleWeights,
    /// The last simultaneity of the previous bar (highest voice first).
    pub previous: &'a [Option<Midi>],
    /// Recent bars' fingerprints, per voice, most recent last.
    pub history: &'a [Vec<Fingerprint>],
    /// How much chromatic writing to allow in free voices, `0..=1`.
    pub chromaticism: f64,
}

/// Named cost terms, for display.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Breakdown {
    /// `(rule, weighted cost, raw count)`.
    pub terms: Vec<(&'static str, f64, f64)>,
}

impl Breakdown {
    fn add(&mut self, name: &'static str, weighted: f64, raw: f64) {
        if raw == 0.0 {
            return;
        }
        if let Some(t) = self.terms.iter_mut().find(|t| t.0 == name) {
            t.1 += weighted;
            t.2 += raw;
        } else {
            self.terms.push((name, weighted, raw));
        }
    }

    /// Total weighted cost.
    pub fn total(&self) -> f64 {
        self.terms.iter().map(|t| t.1).sum()
    }
}

fn fault_weight(kind: FaultKind, w: &RuleWeights) -> (&'static str, f64) {
    match kind {
        FaultKind::ParallelFifths => ("parallel fifths", w.parallels),
        FaultKind::ParallelOctaves => ("parallel octaves", w.parallels * 1.2),
        FaultKind::DirectFifth | FaultKind::DirectOctave => ("direct perfects", w.direct),
        FaultKind::VoiceCrossing => ("voice crossing", w.crossing),
        FaultKind::VoiceOverlap => ("voice overlap", w.crossing * 0.4),
        FaultKind::WideSpacing => ("wide spacing", w.spacing),
        FaultKind::AwkwardLeap => ("awkward leap", w.leaps),
        FaultKind::UnresolvedLeadingTone => ("leading tone", w.leading_tone),
        FaultKind::OutOfKey => ("out of key", w.out_of_key),
        FaultKind::Dissonance => ("dissonance", w.strong_dissonance),
    }
}

/// Whether voice `v` is free (its faults count) — fixed material is the
/// composer's responsibility, not the annealer's, but a fault *between* a
/// free and a fixed voice still counts.
fn involves_free(grid: &BarGrid, voices: (usize, usize)) -> bool {
    let free = |i: usize| grid.voices.get(i).is_some_and(|v| v.free);
    free(voices.0) || free(voices.1)
}

/// Scores the bar.
pub fn cost(grid: &BarGrid, ctx: &Context<'_>) -> Breakdown {
    let mut b = Breakdown::default();
    let w = &ctx.weights;
    let points = grid.change_points();
    let slot_beats = grid.slot_beats;

    // Voice-leading between successive change points (and across the barline).
    let mut prev_sim: Vec<Option<Midi>> = ctx.previous.to_vec();
    let mut have_prev = !prev_sim.is_empty() && prev_sim.iter().any(Option::is_some);
    for &k in &points {
        let sim = grid.simultaneity(k);
        if have_prev {
            let onset_at = |i: usize| grid.voices.get(i).is_some_and(|v| v.onset.get(k).copied().unwrap_or(false));
            for f in vl::perfect_parallels(&prev_sim, &sim) {
                if involves_free(grid, f.voices) && (onset_at(f.voices.0) || onset_at(f.voices.1)) {
                    let (name, wt) = fault_weight(f.kind, w);
                    b.add(name, wt, 1.0);
                }
            }
            for f in vl::overlaps(&prev_sim, &sim) {
                if involves_free(grid, f.voices) {
                    let (name, wt) = fault_weight(f.kind, w);
                    b.add(name, wt, 1.0);
                }
            }
            for f in vl::melodic_faults(&prev_sim, &sim) {
                if grid.voices.get(f.voices.0).is_some_and(|v| v.free) && onset_at(f.voices.0) {
                    let (name, wt) = fault_weight(f.kind, w);
                    b.add(name, wt, 1.0);
                }
            }
            for f in vl::leading_tone_faults(&ctx.scale, &prev_sim, &sim) {
                if grid.voices.get(f.voices.0).is_some_and(|v| v.free) && onset_at(f.voices.0) {
                    let (name, wt) = fault_weight(f.kind, w);
                    b.add(name, wt, 1.0);
                }
            }
        }
        // Vertical.
        for f in vl::vertical_faults(&sim) {
            if involves_free(grid, f.voices) {
                let (name, wt) = fault_weight(f.kind, w);
                b.add(name, wt, 1.0);
            }
        }
        let beat = k as f64 * slot_beats;
        let strength = ctx.meter.strength_at(beat);
        let strong = matches!(strength, BeatStrength::Downbeat | BeatStrength::Strong);
        let chord = chord_at(ctx.chords, beat);
        for (i, v) in grid.voices.iter().enumerate() {
            if !v.free || !v.onset.get(k).copied().unwrap_or(false) {
                continue;
            }
            let Some(p) = v.pitch.get(k).copied().flatten() else { continue };
            // Foreign notes.
            if !ctx.scale.contains(p) {
                let allowed = ctx.chromaticism;
                b.add("out of key", w.out_of_key * (1.0 - 0.8 * allowed), 1.0);
            }
            // Chord tones on onsets; passing tones tolerated off the beat.
            if let Some(c) = chord
                && !c.contains(&ctx.scale, p)
            {
                let before = v.pitch_before(k);
                let after = (k + 1..grid.slots).find_map(|j| {
                    v.onset.get(j).copied().unwrap_or(false).then(|| v.pitch.get(j).copied().flatten()).flatten()
                });
                let stepwise = before.is_some_and(|q| Interval::between(q, p).is_step())
                    && after.is_none_or(|q| Interval::between(p, q).is_step());
                let (name, wt) = if strong {
                    ("non-chord tone (strong)", w.non_chord_tone * if stepwise { 0.5 } else { 1.0 })
                } else if stepwise {
                    ("passing tone", w.non_chord_tone * 0.1)
                } else {
                    ("non-chord tone (weak)", w.non_chord_tone * 0.5)
                };
                b.add(name, wt, 1.0);
            }
            // Range.
            if p < v.range.0 {
                b.add("range", w.range * f64::from(v.range.0 - p) / 3.0, 1.0);
            } else if p > v.range.1 {
                b.add("range", w.range * f64::from(p - v.range.1) / 3.0, 1.0);
            }
            let _ = i;
        }
        // Dissonance against the bass on strong beats.
        if strong {
            for f in vl::dissonances_against_bass(&sim) {
                if involves_free(grid, f.voices) {
                    b.add("strong-beat dissonance", w.strong_dissonance, 1.0);
                }
            }
        } else {
            for f in vl::dissonances_against_bass(&sim) {
                if involves_free(grid, f.voices) {
                    b.add("weak-beat dissonance", w.weak_dissonance * 0.5, 1.0);
                }
            }
        }
        prev_sim = sim;
        have_prev = true;
    }

    // Bass on the chord's bass at chord changes.
    if let Some(bass_i) = grid.voices.iter().rposition(|v| matches!(v.role, Role::Bass))
        && let Some(bass) = grid.voices.get(bass_i)
        && bass.free
    {
        for slot in ctx.chords {
            let k = (slot.onset / slot_beats).round() as usize;
            let Some(p) = bass.pitch.get(k).copied().flatten() else { continue };
            if rameau_theory::PitchClass::of_midi(p) != slot.chord.bass(&ctx.scale) {
                let in_chord = slot.chord.contains(&ctx.scale, p);
                b.add("bass mismatch", w.bass_mismatch * if in_chord { 0.35 } else { 1.0 }, 1.0);
            }
        }
    }

    // Density and silence per free voice.
    for v in &grid.voices {
        if !v.free {
            continue;
        }
        let onsets = v.onset_count() as f64;
        let diff = (onsets - v.target_onsets).abs() / grid.slots as f64;
        b.add("density", w.density * diff * 4.0, diff);
        let rests = v.pitch.iter().filter(|p| p.is_none()).count() as f64 / grid.slots as f64;
        if rests > 0.6 && v.target_onsets >= 1.0 {
            b.add("silence", w.silence * (rests - 0.6) * 2.5, 1.0);
        }
        // Monotony: three or more repeated pitches in a row.
        let notes = v.notes();
        let mut run = 1;
        for pair in notes.windows(2) {
            if let (Some(a), Some(c)) = (pair.first(), pair.get(1)) {
                if a.2 == c.2 {
                    run += 1;
                    if run >= 3 {
                        b.add("monotony", w.monotony, 1.0);
                    }
                } else {
                    run = 1;
                }
            }
        }
        // Too many leaps in a row.
        let mut leaps = 0;
        for pair in notes.windows(2) {
            if let (Some(a), Some(c)) = (pair.first(), pair.get(1)) {
                if Interval::between(a.2, c.2).is_leap() {
                    leaps += 1;
                    if leaps >= 3 {
                        b.add("leapy", w.leaps * 0.4, 1.0);
                    }
                } else {
                    leaps = 0;
                }
            }
        }
    }

    // Repetition against recent bars, free voices only.
    if !ctx.history.is_empty() {
        let now = grid.fingerprint();
        for (i, v) in grid.voices.iter().enumerate() {
            if !v.free || v.onset_count() == 0 {
                continue;
            }
            let Some(fp) = now.get(i) else { continue };
            let worst = ctx
                .history
                .iter()
                .filter_map(|bar| bar.get(i))
                .map(|old| fp.similarity(old))
                .fold(0.0, f64::max);
            if worst > 0.72 {
                b.add("repetition", w.repetition * (worst - 0.72) * 6.0, worst);
            }
        }
    }

    b
}

/// A rewrite proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Move {
    Repitch,
    Step,
    Split,
    Merge,
    Rest,
    Unrest,
    Octave,
}

/// Applies a random move to a free voice. Returns `false` if nothing
/// changed.
fn propose(rng: &mut impl Rng, grid: &mut BarGrid, ctx: &Context<'_>) -> bool {
    let free: Vec<usize> = grid
        .voices
        .iter()
        .enumerate()
        .filter(|(_, v)| v.free && v.fixed.iter().any(|f| !f))
        .map(|(i, _)| i)
        .collect();
    let Some(&vi) = rng.pick(&free) else { return false };
    let slots = grid.slots;
    let slot_beats = grid.slot_beats;
    let moves = [
        (Move::Repitch, 4.0),
        (Move::Step, 4.0),
        (Move::Split, 1.5),
        (Move::Merge, 1.5),
        (Move::Rest, 0.6),
        (Move::Unrest, 1.2),
        (Move::Octave, 0.5),
    ];
    let weights: Vec<f64> = moves.iter().map(|m| m.1).collect();
    let mv = moves.get(rng.weighted(&weights).unwrap_or(0)).map_or(Move::Step, |m| m.0);
    let Some(v) = grid.voices.get_mut(vi) else { return false };
    let k = rng.below(slots);

    match mv {
        Move::Repitch | Move::Step | Move::Octave => {
            let Some((start, end)) = v.note_span(k) else { return false };
            if !v.span_editable(start, end) {
                return false;
            }
            let Some(p) = v.pitch.get(start).copied().flatten() else { return false };
            let new = match mv {
                Move::Step => ctx.scale.step_from(p, if rng.chance(0.5) { 1 } else { -1 }),
                Move::Octave => {
                    if rng.chance(0.5) { p + 12 } else { p - 12 }
                }
                _ => {
                    let beat = start as f64 * slot_beats;
                    let anchor = v.pitch_before(start).unwrap_or(p);
                    let chord: Option<&RomanNumeral> = chord_at(ctx.chords, beat);
                    let use_chord = chord.is_some() && rng.chance(0.7);
                    let lo = (anchor - 9).max(v.range.0 - 2);
                    let hi = (anchor + 9).min(v.range.1 + 2);
                    let pool: Vec<Midi> = if use_chord {
                        chord.map(|c| c.tones_in_range(&ctx.scale, lo, hi)).unwrap_or_default()
                    } else if rng.chance(ctx.chromaticism * 0.5) {
                        (lo..=hi).collect()
                    } else {
                        (lo..=hi).filter(|m| ctx.scale.contains(*m)).collect()
                    };
                    if pool.is_empty() {
                        return false;
                    }
                    // Prefer nearby tones.
                    let ws: Vec<f64> = pool.iter().map(|m| 1.0 / (1.0 + f64::from((m - anchor).abs()) * 0.35)).collect();
                    pool.get(rng.weighted(&ws).unwrap_or(0)).copied().unwrap_or(p)
                }
            };
            if new == p || new < v.range.0 - 3 || new > v.range.1 + 3 {
                return false;
            }
            for j in start..end {
                if let Some(slot) = v.pitch.get_mut(j) {
                    *slot = Some(new);
                }
            }
            true
        }
        Move::Split => {
            let Some((start, end)) = v.note_span(k) else { return false };
            if end - start < 2 || !v.span_editable(start, end) {
                return false;
            }
            // Split on a musically sensible point: the midpoint, snapped to
            // an even slot when possible.
            let mut mid = start + (end - start) / 2;
            if (end - start) >= 4 && mid % 2 == 1 {
                mid += 1;
            }
            if mid <= start || mid >= end {
                return false;
            }
            let Some(p) = v.pitch.get(start).copied().flatten() else { return false };
            let new = match rng.below(3) {
                0 => p,
                1 => ctx.scale.step_from(p, 1),
                _ => ctx.scale.step_from(p, -1),
            };
            if let Some(o) = v.onset.get_mut(mid) {
                *o = true;
            }
            for j in mid..end {
                if let Some(slot) = v.pitch.get_mut(j) {
                    *slot = Some(new);
                }
            }
            true
        }
        Move::Merge => {
            if k == 0 || !v.onset.get(k).copied().unwrap_or(false) {
                return false;
            }
            let Some((start, end)) = v.note_span(k) else { return false };
            let Some(prev_p) = v.pitch.get(k - 1).copied().flatten() else { return false };
            if !v.span_editable(start, end) || !v.editable(k - 1) {
                return false;
            }
            if let Some(o) = v.onset.get_mut(k) {
                *o = false;
            }
            for j in start..end {
                if let Some(slot) = v.pitch.get_mut(j) {
                    *slot = Some(prev_p);
                }
            }
            true
        }
        Move::Rest => {
            let Some((start, end)) = v.note_span(k) else { return false };
            if !v.span_editable(start, end) {
                return false;
            }
            for j in start..end {
                if let Some(slot) = v.pitch.get_mut(j) {
                    *slot = None;
                }
                if let Some(o) = v.onset.get_mut(j) {
                    *o = false;
                }
            }
            true
        }
        Move::Unrest => {
            if v.pitch.get(k).copied().flatten().is_some() || !v.editable(k) {
                return false;
            }
            // Fill the rest run from k to the next sound or bar end.
            let mut end = k;
            while end < slots && v.pitch.get(end).copied().flatten().is_none() && v.editable(end) {
                end += 1;
            }
            // Only fill a run that starts on an even slot boundary or at a
            // rest boundary.
            let beat = k as f64 * slot_beats;
            let anchor = v.pitch_before(k).unwrap_or((v.range.0 + v.range.1) / 2);
            let lo = (anchor - 7).max(v.range.0);
            let hi = (anchor + 7).min(v.range.1);
            let pool: Vec<Midi> = chord_at(ctx.chords, beat)
                .map(|c| c.tones_in_range(&ctx.scale, lo, hi))
                .unwrap_or_default();
            let Some(&new) = rng.pick(&pool) else { return false };
            let len = (end - k).min(rng.range_i32(1, 4) as usize * 2).max(1);
            for j in k..k + len {
                if let Some(slot) = v.pitch.get_mut(j) {
                    *slot = Some(new);
                }
            }
            if let Some(o) = v.onset.get_mut(k) {
                *o = true;
            }
            true
        }
    }
}

/// The result of an annealing run.
#[derive(Debug, Clone, PartialEq)]
pub struct Annealed {
    /// The best bar found.
    pub grid: BarGrid,
    /// Its cost breakdown.
    pub breakdown: Breakdown,
    /// Cost of the starting bar.
    pub initial_cost: f64,
    /// Number of accepted moves.
    pub accepted: usize,
    /// Iterations run.
    pub iterations: usize,
}

/// Anneals `grid` for `iterations` proposals.
pub fn anneal(rng: &mut impl Rng, mut grid: BarGrid, ctx: &Context<'_>, iterations: usize) -> Annealed {
    let mut current = cost(&grid, ctx);
    let initial_cost = current.total();
    let mut best = grid.clone();
    let mut best_cost = current.clone();
    let mut accepted = 0;
    let t0: f64 = 2.5;
    let t1: f64 = 0.02;
    let n = iterations.max(1);
    for i in 0..n {
        let t = t0 * (t1 / t0).powf(i as f64 / n as f64);
        let candidate_grid = {
            let mut g = grid.clone();
            if !propose(rng, &mut g, ctx) {
                continue;
            }
            g
        };
        let candidate = cost(&candidate_grid, ctx);
        let delta = candidate.total() - current.total();
        if delta <= 0.0 || rng.chance((-delta / t).exp()) {
            grid = candidate_grid;
            current = candidate;
            accepted += 1;
            if current.total() < best_cost.total() {
                best = grid.clone();
                best_cost = current.clone();
            }
        }
    }
    Annealed {
        grid: best,
        breakdown: best_cost,
        initial_cost,
        accepted,
        iterations: n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rameau_chords::RomanNumeral;
    use rameau_theory::{Mode, PitchClass};
    use rameau_types::SplitMix64;

    fn ctx_c_major<'a>(chords: &'a [ChordSlot], previous: &'a [Option<Midi>]) -> Context<'a> {
        let s = crate::state::MusicState::default();
        Context {
            scale: Scale::new(PitchClass::C, Mode::Major),
            meter: Meter::COMMON,
            chords,
            weights: RuleWeights::from_state(&s),
            previous,
            history: &[],
            chromaticism: 0.0,
        }
    }

    #[test]
    fn annealing_removes_parallel_fifths_and_meets_density() {
        let c = Scale::new(PitchClass::C, Mode::Major);
        let chords = vec![
            ChordSlot { onset: 0.0, duration: 2.0, chord: RomanNumeral::diatonic(&c, 0) },
            ChordSlot { onset: 2.0, duration: 2.0, chord: RomanNumeral::diatonic(&c, 4) },
        ];
        let mut grid = BarGrid::new(&Meter::COMMON);
        let lead = grid.add_voice(Role::Lead, (60, 84));
        let bass = grid.add_voice(Role::Bass, (36, 60));
        // Fixed lead: C D E F | G G G G in crotchets (I then V).
        let lead_notes = [72, 74, 76, 77, 79, 79, 79, 79];
        for (i, p) in lead_notes.iter().enumerate() {
            grid.voices[lead].write(i * 2, 2, Some(*p), true);
        }
        grid.voices[lead].free = false;
        // Free bass starts in blatant parallel octaves with the lead.
        for (i, p) in lead_notes.iter().enumerate() {
            grid.voices[bass].write(i * 2, 2, Some(*p - 24), false);
        }
        grid.voices[bass].target_onsets = 4.0;
        let ctx = ctx_c_major(&chords, &[]);
        let before = cost(&grid, &ctx);
        assert!(before.terms.iter().any(|t| t.0 == "parallel octaves"), "{:?}", before.terms);
        let mut rng = SplitMix64::new(11);
        let out = anneal(&mut rng, grid, &ctx, 1500);
        assert!(out.breakdown.total() < before.total() * 0.5, "{} -> {} {:?}", before.total(), out.breakdown.total(), out.breakdown.terms);
        let par = out.breakdown.terms.iter().filter(|t| t.0.starts_with("parallel")).map(|t| t.2).sum::<f64>();
        assert!(par <= 1.0, "parallels left: {par} {:?}", out.breakdown.terms);
        let onsets = out.grid.voices[bass].onset_count();
        assert!((2..=6).contains(&onsets), "bass onsets {onsets}");
        // Fixed voice untouched.
        assert_eq!(out.grid.voices[lead].notes().len(), 8);
    }

    #[test]
    fn notes_and_fingerprints() {
        let mut v = VoiceGrid::empty(8, Role::Lead, (60, 84));
        v.write(0, 4, Some(60), false);
        v.write(4, 2, Some(62), false);
        v.write(6, 2, None, false);
        assert_eq!(v.notes(), vec![(0, 4, 60), (4, 2, 62)]);
        let fp = Fingerprint::of(&v);
        assert_eq!(fp.intervals, vec![2]);
        assert_eq!(fp.similarity(&fp), 1.0);
        let mut w = v.clone();
        w.write(4, 2, Some(59), false);
        assert!(fp.similarity(&Fingerprint::of(&w)) < 1.0);
        v.tie_in(2, 60, true);
        assert!(!v.onset[0]);
        assert_eq!(v.notes().len(), 2);
    }
}
