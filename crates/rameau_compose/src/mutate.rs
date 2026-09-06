//! Motivic mutation: transposition, inversion, retrograde, augmentation,
//! diminution, and the rest of the toolbox.
//!
//! All mutations act on degree-encoded [`IdeaEvent`]s, so transposition and
//! inversion are diatonic and stay in key by construction; chromatic
//! alterations ride along as alterations of a degree, which is how a raised
//! second survives being inverted into a lowered seventh.

use serde::{Deserialize, Serialize};

use rameau_theory::DegreeNote;

use crate::idea::{IdeaEvent, IdeaLibrary};

/// One transformation of an idea.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum Mutation {
    /// Diatonic transposition by `steps` scale steps.
    Transposition {
        /// Steps up the scale (negative for down).
        steps: i32,
    },
    /// Chromatic transposition by `semitones`, stored as alterations.
    ChromaticShift {
        /// Semitones up (negative for down), `-2..=2`.
        semitones: i8,
    },
    /// Melodic inversion about the first note.
    Inversion,
    /// The notes in reverse order (rhythm reversed with them).
    Retrograde,
    /// Durations multiplied by `factor` (2 doubles every value).
    Augmentation {
        /// The factor, > 1.
        factor: f64,
    },
    /// Durations divided by `factor` (2 halves every value).
    Diminution {
        /// The factor, > 1.
        factor: f64,
    },
    /// Keep only the first `beats` crotchets (the head of a tune, as a
    /// fugue subject).
    Head {
        /// Crotchets kept.
        beats: f64,
    },
    /// Keep only events `start..start + count`.
    Fragment {
        /// First event kept.
        start: usize,
        /// Number of events kept.
        count: usize,
    },
    /// Repeat the material `times` times, each `steps` scale steps further.
    Sequence {
        /// Repetitions after the original.
        times: usize,
        /// Diatonic step between repetitions.
        steps: i32,
    },
    /// Move everything by `octaves`.
    OctaveShift {
        /// Octaves up (negative for down).
        octaves: i32,
    },
    /// Fill leaps of a third with a passing note (the coulé).
    Ornamentation,
    /// Drop notes shorter than `min_beats`, lengthening their neighbours.
    Simplification {
        /// Shortest note kept, in crotchets.
        min_beats: f64,
    },
}

impl Mutation {
    /// A short label such as `T+4` or `inv`.
    pub fn label(&self) -> String {
        match self {
            Mutation::Transposition { steps } => format!("T{steps:+}"),
            Mutation::ChromaticShift { semitones } => format!("chr{semitones:+}"),
            Mutation::Inversion => "inv".to_owned(),
            Mutation::Retrograde => "retro".to_owned(),
            Mutation::Augmentation { factor } => format!("aug×{factor}"),
            Mutation::Diminution { factor } => format!("dim÷{factor}"),
            Mutation::Head { beats } => format!("head[{beats}]"),
            Mutation::Fragment { start, count } => format!("frag[{start}+{count}]"),
            Mutation::Sequence { times, steps } => format!("seq×{times}{steps:+}"),
            Mutation::OctaveShift { octaves } => format!("8va{octaves:+}"),
            Mutation::Ornamentation => "orn".to_owned(),
            Mutation::Simplification { .. } => "simp".to_owned(),
        }
    }

    /// Applies the mutation to `events`.
    pub fn apply(&self, events: &[IdeaEvent]) -> Vec<IdeaEvent> {
        match *self {
            Mutation::Transposition { steps } => map_notes(events, |n| n.transposed(steps)),
            Mutation::ChromaticShift { semitones } => map_notes(events, |n| DegreeNote {
                alteration: n.alteration.saturating_add(semitones),
                ..n
            }),
            Mutation::Inversion => {
                let axis = events
                    .iter()
                    .find_map(|e| e.note)
                    .map_or(0, |n| n.staff_index());
                map_notes(events, |n| n.inverted_about(axis))
            }
            Mutation::Retrograde => events.iter().rev().copied().collect(),
            Mutation::Augmentation { factor } => scale_durations(events, factor.max(1e-3)),
            Mutation::Diminution { factor } => scale_durations(events, 1.0 / factor.max(1e-3)),
            Mutation::Head { beats } => {
                let mut out = Vec::new();
                let mut t = 0.0;
                for e in events {
                    if t >= beats - 1e-9 {
                        break;
                    }
                    let keep = e.beats.min(beats - t);
                    out.push(IdeaEvent { beats: keep, ..*e });
                    t += keep;
                }
                if out.is_empty() { events.to_vec() } else { out }
            }
            Mutation::Fragment { start, count } => {
                let out: Vec<IdeaEvent> =
                    events.iter().skip(start).take(count.max(1)).copied().collect();
                if out.is_empty() { events.to_vec() } else { out }
            }
            Mutation::Sequence { times, steps } => {
                let mut out = events.to_vec();
                for k in 1..=times {
                    out.extend(map_notes(events, |n| n.transposed(steps * k as i32)));
                }
                out
            }
            Mutation::OctaveShift { octaves } => map_notes(events, |n| DegreeNote {
                octave: n.octave.saturating_add(octaves as i8),
                ..n
            }),
            Mutation::Ornamentation => ornament(events),
            Mutation::Simplification { min_beats } => simplify(events, min_beats),
        }
    }
}

fn map_notes(events: &[IdeaEvent], f: impl Fn(DegreeNote) -> DegreeNote) -> Vec<IdeaEvent> {
    events
        .iter()
        .map(|e| IdeaEvent {
            note: e.note.map(&f),
            ..*e
        })
        .collect()
}

fn scale_durations(events: &[IdeaEvent], factor: f64) -> Vec<IdeaEvent> {
    events
        .iter()
        .map(|e| IdeaEvent {
            beats: e.beats * factor,
            ..*e
        })
        .collect()
}

/// Splits each note that leaps a third to the next into two, inserting the
/// step between.
fn ornament(events: &[IdeaEvent]) -> Vec<IdeaEvent> {
    let mut out = Vec::with_capacity(events.len() * 2);
    for (i, e) in events.iter().enumerate() {
        let next = events.get(i + 1).and_then(|n| n.note);
        match (e.note, next) {
            (Some(a), Some(b)) if (b.staff_index() - a.staff_index()).abs() == 2 && e.beats >= 0.5 => {
                let dir = (b.staff_index() - a.staff_index()).signum();
                out.push(IdeaEvent {
                    beats: e.beats / 2.0,
                    ..*e
                });
                out.push(IdeaEvent {
                    note: Some(a.transposed(dir)),
                    beats: e.beats / 2.0,
                    ornament: None,
                });
            }
            _ => out.push(*e),
        }
    }
    out
}

/// Removes notes shorter than `min_beats`, giving their time to the note
/// before them (or after, at the start).
fn simplify(events: &[IdeaEvent], min_beats: f64) -> Vec<IdeaEvent> {
    let mut out: Vec<IdeaEvent> = Vec::with_capacity(events.len());
    let mut carry = 0.0;
    for e in events {
        if e.beats + 1e-9 < min_beats {
            match out.last_mut() {
                Some(prev) => prev.beats += e.beats,
                None => carry += e.beats,
            }
        } else {
            out.push(IdeaEvent {
                beats: e.beats + carry,
                ..*e
            });
            carry = 0.0;
        }
    }
    if out.is_empty() { events.to_vec() } else { out }
}

/// An idea plus the mutations applied to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variant {
    /// The idea id.
    pub idea: String,
    /// Mutations, applied in order.
    #[serde(default)]
    pub mutations: Vec<Mutation>,
}

impl Variant {
    /// The plain idea.
    pub fn plain(idea: &str) -> Self {
        Self {
            idea: idea.to_owned(),
            mutations: Vec::new(),
        }
    }

    /// The idea with one more mutation.
    pub fn with(mut self, m: Mutation) -> Self {
        self.mutations.push(m);
        self
    }

    /// The idea id followed by the mutation labels, e.g. `caira.A inv T+2`.
    pub fn label(&self) -> String {
        let mut s = self.idea.clone();
        for m in &self.mutations {
            s.push(' ');
            s.push_str(&m.label());
        }
        s
    }

    /// The events after mutation, or `None` if the idea is unknown.
    pub fn realize(&self, lib: &IdeaLibrary) -> Option<Vec<IdeaEvent>> {
        let idea = lib.get(&self.idea)?;
        Some(self.apply_to(&idea.events))
    }

    /// Applies the mutations to arbitrary events.
    pub fn apply_to(&self, events: &[IdeaEvent]) -> Vec<IdeaEvent> {
        self.mutations
            .iter()
            .fold(events.to_vec(), |acc, m| m.apply(&acc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev() -> Vec<IdeaEvent> {
        vec![
            IdeaEvent::note(0, 0, 0, 1.0),
            IdeaEvent::note(2, 0, 0, 0.5),
            IdeaEvent::note(1, 1, 0, 0.5),
            IdeaEvent::rest(1.0),
            IdeaEvent::note(4, 0, 0, 1.0),
        ]
    }

    fn staff(events: &[IdeaEvent]) -> Vec<Option<i32>> {
        events.iter().map(|e| e.note.map(|n| n.staff_index())).collect()
    }

    #[test]
    fn transposition_inversion_retrograde() {
        let t = Mutation::Transposition { steps: 4 }.apply(&ev());
        assert_eq!(staff(&t), vec![Some(4), Some(6), Some(5), None, Some(8)]);
        let inv = Mutation::Inversion.apply(&ev());
        assert_eq!(staff(&inv), vec![Some(0), Some(-2), Some(-1), None, Some(-4)]);
        assert_eq!(inv[2].note.unwrap().alteration, -1, "alteration flips under inversion");
        let r = Mutation::Retrograde.apply(&ev());
        assert_eq!(staff(&r), vec![Some(4), None, Some(1), Some(2), Some(0)]);
        assert_eq!(r[0].beats, 1.0);
    }

    #[test]
    fn durations_and_shape() {
        let a = Mutation::Augmentation { factor: 2.0 }.apply(&ev());
        assert_eq!(a[1].beats, 1.0);
        let d = Mutation::Diminution { factor: 2.0 }.apply(&ev());
        assert_eq!(d[0].beats, 0.5);
        let f = Mutation::Fragment { start: 1, count: 2 }.apply(&ev());
        assert_eq!(f.len(), 2);
        let h = Mutation::Head { beats: 2.0 }.apply(&ev());
        assert_eq!(h.len(), 3);
        assert_eq!(h[2].beats, 0.5);
        assert_eq!(h.iter().map(|e| e.beats).sum::<f64>(), 2.0);
        let s = Mutation::Sequence { times: 2, steps: -1 }.apply(&ev());
        assert_eq!(s.len(), 15);
        assert_eq!(s[10].note.unwrap().staff_index(), -2);
        let o = Mutation::Ornamentation.apply(&ev());
        assert_eq!(o.len(), 6, "the leap of a third at the start gets a passing note");
        assert_eq!(o[1].note.unwrap().staff_index(), 1);
        let simp = Mutation::Simplification { min_beats: 1.0 }.apply(&ev());
        assert_eq!(simp.len(), 3);
        assert_eq!(simp[0].beats, 2.0);
    }

    #[test]
    fn variant_labels_and_serialises() {
        let v = Variant::plain("caira.A")
            .with(Mutation::Inversion)
            .with(Mutation::Transposition { steps: 2 });
        assert_eq!(v.label(), "caira.A inv T+2");
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains(r#""op":"inversion""#));
        let back: Variant = serde_json::from_str(&json).unwrap();
        assert_eq!(back, v);
    }
}
