//! The parsed score model.

use rameau_theory::{Meter, Midi, Scale};

/// Tie state of a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tie {
    /// Not tied.
    #[default]
    None,
    /// First note of a tie (`[`).
    Start,
    /// Middle of a tie (`_`).
    Continue,
    /// Last note of a tie (`]`).
    End,
}

/// One note or rest, timed in crotchets.
#[derive(Debug, Clone, PartialEq)]
pub struct KernNote {
    /// Onset in crotchets from the start of the piece.
    pub onset: f64,
    /// Duration in crotchets.
    pub duration: f64,
    /// MIDI key, or `None` for a rest.
    pub midi: Option<Midi>,
    /// Bar number as printed on the preceding barline (0 before any).
    pub measure: u32,
    /// Position in the bar, in crotchets from the barline.
    pub beat: f64,
    /// Tie state.
    pub tie: Tie,
    /// Ornament signs on the note (`T` trill, `M` mordent, `W` inverted
    /// mordent, `S` turn, `$` inverted turn, `w`/`m` half-step variants).
    pub ornaments: Vec<char>,
    /// The raw token this note came from.
    pub token: String,
}

impl KernNote {
    /// Whether the note is a rest.
    pub const fn is_rest(&self) -> bool {
        self.midi.is_none()
    }

    /// End time in crotchets.
    pub fn end(&self) -> f64 {
        self.onset + self.duration
    }
}

/// One spine's worth of notes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Voice {
    /// The spine's column at the start of the file (0-based, left to right),
    /// or the parent's for a spine created by a split.
    pub spine: usize,
    /// Part name (`*part1`) or instrument (`*I"Bass`), when given.
    pub name: Option<String>,
    /// Notes and rests in order; chord notes share an onset.
    pub notes: Vec<KernNote>,
}

impl Voice {
    /// The voice reduced to a single line: the highest note at each onset,
    /// with ties merged into one note and consecutive rests merged.
    pub fn melody(&self) -> Vec<KernNote> {
        self.line(true)
    }

    /// Like [`melody`](Self::melody) but keeping the lowest note of chords.
    pub fn bass_line(&self) -> Vec<KernNote> {
        self.line(false)
    }

    fn line(&self, top: bool) -> Vec<KernNote> {
        let mut out: Vec<KernNote> = Vec::new();
        let mut i = 0;
        while i < self.notes.len() {
            let Some(first) = self.notes.get(i) else { break };
            // Collect the chord at this onset.
            let mut j = i + 1;
            let mut pick = first.clone();
            while let Some(n) = self.notes.get(j) {
                if (n.onset - first.onset).abs() > 1e-9 {
                    break;
                }
                let better = match (pick.midi, n.midi) {
                    (None, Some(_)) => true,
                    (Some(a), Some(b)) => {
                        if top { b > a } else { b < a }
                    }
                    _ => false,
                };
                if better {
                    pick = n.clone();
                }
                j += 1;
            }
            i = j;
            // Merge into the previous note if tied or both rests.
            if let Some(prev) = out.last_mut() {
                let tied = prev.midi.is_some()
                    && prev.midi == pick.midi
                    && matches!(prev.tie, Tie::Start | Tie::Continue)
                    && matches!(pick.tie, Tie::Continue | Tie::End);
                let rests = prev.is_rest() && pick.is_rest();
                if (tied || rests) && (prev.end() - pick.onset).abs() < 1e-6 {
                    prev.duration += pick.duration;
                    prev.tie = if tied && pick.tie == Tie::End { Tie::None } else { prev.tie };
                    continue;
                }
            }
            out.push(pick);
        }
        out
    }

    /// Notes whose bar number lies in `first..=last`.
    pub fn bars(&self, first: u32, last: u32) -> Vec<KernNote> {
        self.notes
            .iter()
            .filter(|n| n.measure >= first && n.measure <= last)
            .cloned()
            .collect()
    }

    /// Whether the voice contains any sounding note.
    pub fn has_notes(&self) -> bool {
        self.notes.iter().any(|n| n.midi.is_some())
    }

    /// Lowest and highest sounding MIDI keys.
    pub fn range(&self) -> Option<(Midi, Midi)> {
        let mut it = self.notes.iter().filter_map(|n| n.midi);
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), m| (lo.min(m), hi.max(m))))
    }
}

/// A parsed `**kern` file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct KernScore {
    /// `!!!OTL` title, if any.
    pub title: Option<String>,
    /// `!!!COM` composer, if any.
    pub composer: Option<String>,
    /// The key from the first `*X:` token.
    pub key: Option<Scale>,
    /// Sharps (negative for flats) from the first `*k[...]` token.
    pub signature: Option<i32>,
    /// The first `*M` meter.
    pub meter: Option<Meter>,
    /// The first `*MM` tempo, in crotchets per minute.
    pub tempo: Option<f64>,
    /// The voices, left to right.
    pub voices: Vec<Voice>,
}

impl KernScore {
    /// The key: the explicit one, or a major key guessed from the signature,
    /// or C major.
    pub fn key_or_guess(&self) -> Scale {
        if let Some(k) = self.key {
            return k;
        }
        let sharps = self.signature.unwrap_or(0);
        Scale::new(
            rameau_theory::key::major_tonic_of_signature(sharps),
            rameau_theory::Mode::Major,
        )
    }

    /// Number of bars (the highest printed bar number).
    pub fn bar_count(&self) -> u32 {
        self.voices
            .iter()
            .flat_map(|v| v.notes.iter().map(|n| n.measure))
            .max()
            .unwrap_or(0)
    }

    /// The voice with the highest average pitch, the usual melody carrier.
    pub fn top_voice(&self) -> Option<&Voice> {
        self.voices
            .iter()
            .filter(|v| v.has_notes())
            .max_by(|a, b| mean_pitch(a).total_cmp(&mean_pitch(b)))
    }

    /// The voice with the lowest average pitch.
    pub fn bottom_voice(&self) -> Option<&Voice> {
        self.voices
            .iter()
            .filter(|v| v.has_notes())
            .min_by(|a, b| mean_pitch(a).total_cmp(&mean_pitch(b)))
    }
}

fn mean_pitch(v: &Voice) -> f64 {
    let (sum, n) = v
        .notes
        .iter()
        .filter_map(|n| n.midi)
        .fold((0.0, 0usize), |(s, c), m| (s + f64::from(m), c + 1));
    if n == 0 { f64::NEG_INFINITY } else { sum / n as f64 }
}
