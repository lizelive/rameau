//! Line-by-line parsing of a `**kern` file, following spine splits and
//! merges.

use rameau_theory::key::{parse_kern_signature, parse_key};
use rameau_theory::Meter;

use crate::score::{KernNote, KernScore, Voice};
use crate::token::parse_token;

/// Why a file could not be parsed at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// No `**kern` spine was declared.
    NoKernSpine,
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ParseError::NoKernSpine => f.write_str("no **kern spine in file"),
        }
    }
}

impl core::error::Error for ParseError {}

/// The live state of one column of the file.
#[derive(Debug, Clone)]
struct Column {
    /// Which voice this column feeds, or `None` for a non-kern spine.
    voice: Option<usize>,
    /// Time cursor in crotchets.
    cursor: f64,
    /// Where the current bar started for this column.
    bar_start: f64,
    /// Current bar number.
    measure: u32,
}

/// Parses `**kern` text into a [`KernScore`].
///
/// # Errors
///
/// Returns [`ParseError::NoKernSpine`] if the file declares no `**kern`
/// spine.
pub fn parse(text: &str) -> Result<KernScore, ParseError> {
    let mut score = KernScore::default();
    let mut columns: Vec<Column> = Vec::new();
    let mut started = false;

    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("!!!") {
            reference_record(&mut score, rest);
            continue;
        }
        if line.starts_with('!') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();

        if line.starts_with("**") {
            if started {
                continue;
            }
            started = true;
            for f in &fields {
                let voice = if f.starts_with("**kern") {
                    score.voices.push(Voice {
                        spine: columns.len(),
                        name: None,
                        notes: Vec::new(),
                    });
                    Some(score.voices.len() - 1)
                } else {
                    None
                };
                columns.push(Column {
                    voice,
                    cursor: 0.0,
                    bar_start: 0.0,
                    measure: 0,
                });
            }
            continue;
        }
        if !started {
            continue;
        }

        if line.starts_with('*') {
            interpretations(&mut score, &mut columns, &fields);
            continue;
        }
        if line.starts_with('=') {
            barline(&mut columns, &fields);
            continue;
        }
        data(&mut score, &mut columns, &fields);
    }

    if score.voices.is_empty() {
        return Err(ParseError::NoKernSpine);
    }
    Ok(score)
}

fn reference_record(score: &mut KernScore, rest: &str) {
    let Some((key, value)) = rest.split_once(':') else {
        return;
    };
    let value = value.trim();
    match key.trim() {
        "OTL" if score.title.is_none() && !value.is_empty() => {
            score.title = Some(value.to_owned());
        }
        "COM" if score.composer.is_none() && !value.is_empty() => {
            score.composer = Some(value.to_owned());
        }
        _ => {}
    }
}

fn interpretations(score: &mut KernScore, columns: &mut Vec<Column>, fields: &[&str]) {
    // Spine manipulators first: they change the column layout.
    if fields.iter().any(|f| *f == "*^" || *f == "*v" || *f == "*-") {
        let mut next: Vec<Column> = Vec::with_capacity(columns.len());
        let mut merging: Option<Column> = None;
        for (i, f) in fields.iter().enumerate() {
            let Some(col) = columns.get(i).cloned() else { continue };
            match *f {
                "*^" => {
                    if let Some(m) = merging.take() {
                        next.push(m);
                    }
                    next.push(col.clone());
                    // The new sub-spine is its own voice, owned by the same
                    // spine number so callers can group them.
                    let voice = col.voice.map(|parent| {
                        let spine = score.voices.get(parent).map_or(0, |v| v.spine);
                        let name = score.voices.get(parent).and_then(|v| v.name.clone());
                        score.voices.push(Voice {
                            spine,
                            name,
                            notes: Vec::new(),
                        });
                        score.voices.len() - 1
                    });
                    next.push(Column { voice, ..col });
                }
                "*v" => match &mut merging {
                    // First of a run of merges keeps its identity; the
                    // others fold into it (their cursor is taken if later).
                    None => merging = Some(col),
                    Some(m) => {
                        if col.cursor > m.cursor {
                            m.cursor = col.cursor;
                        }
                    }
                },
                "*-" => {
                    if let Some(m) = merging.take() {
                        next.push(m);
                    }
                    // Terminated: drop the column.
                }
                _ => {
                    if let Some(m) = merging.take() {
                        next.push(m);
                    }
                    next.push(col);
                }
            }
        }
        if let Some(m) = merging.take() {
            next.push(m);
        }
        *columns = next;
        return;
    }

    for (i, f) in fields.iter().enumerate() {
        let Some(col) = columns.get(i) else { continue };
        let Some(vi) = col.voice else { continue };
        if let Some(sig) = parse_kern_signature(f) {
            if score.signature.is_none() {
                score.signature = Some(sig);
            }
        } else if let Some(m) = f.strip_prefix("*M") {
            if let Some(bpm) = m.strip_prefix('M') {
                if score.tempo.is_none() {
                    score.tempo = bpm.parse().ok();
                }
            } else if score.meter.is_none() {
                score.meter = Meter::parse(m);
            }
        } else if let Some(name) = f.strip_prefix("*I\"") {
            if let Some(v) = score.voices.get_mut(vi) {
                v.name = Some(name.to_owned());
            }
        } else if let Some(name) = f.strip_prefix("*part") {
            if let Some(v) = score.voices.get_mut(vi)
                && v.name.is_none()
            {
                v.name = Some(format!("part{name}"));
            }
        } else if f.ends_with(':') && f.len() >= 3 && score.key.is_none()
            && let Ok(k) = parse_key(f)
        {
            score.key = Some(k);
        }
    }
}

fn barline(columns: &mut [Column], fields: &[&str]) {
    for (i, f) in fields.iter().enumerate() {
        let Some(col) = columns.get_mut(i) else { continue };
        let digits: String = f
            .trim_start_matches('=')
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(n) = digits.parse::<u32>() {
            col.measure = n;
        } else if col.cursor > 0.0 {
            // A numberless barline before any music is the pickup bar's
            // opening line; after music it is an unnumbered bar.
            col.measure += 1;
        }
        col.bar_start = col.cursor;
    }
}

fn data(score: &mut KernScore, columns: &mut [Column], fields: &[&str]) {
    for (i, f) in fields.iter().enumerate() {
        let Some(col) = columns.get_mut(i) else { continue };
        let Some(vi) = col.voice else { continue };
        let parsed = parse_token(f);
        if parsed.is_empty() {
            continue;
        }
        let onset = col.cursor;
        let mut advance: f64 = 0.0;
        let Some(voice) = score.voices.get_mut(vi) else { continue };
        for p in parsed {
            if p.grace {
                continue;
            }
            advance = advance.max(p.duration);
            voice.notes.push(KernNote {
                onset,
                duration: p.duration,
                midi: p.midi,
                measure: col.measure,
                beat: onset - col.bar_start,
                tie: p.tie,
                ornaments: p.ornaments,
                token: (*f).to_owned(),
            });
        }
        col.cursor += advance;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "!!!COM: someone\n!!!OTL: A tune\n**kern\t**kern\n*k[f#]\t*k[f#]\n*G:\t*G:\n*M2/4\t*M2/4\n=1\t=1\n4G\t8g\n.\t8a\n4D\t[4b\n=2\t=2\n4G\t4b]\n4r\t4dd\n*-\t*-\n";

    #[test]
    fn parses_header_and_notes() {
        let s = parse(SAMPLE).unwrap();
        assert_eq!(s.title.as_deref(), Some("A tune"));
        assert_eq!(s.composer.as_deref(), Some("someone"));
        assert_eq!(s.signature, Some(1));
        assert_eq!(s.key.map(|k| k.tonic), Some(rameau_theory::PitchClass::G));
        assert_eq!(s.meter.map(|m| m.beats), Some(2));
        assert_eq!(s.voices.len(), 2);
        let top = &s.voices[1];
        assert_eq!(top.notes.len(), 5);
        assert_eq!(top.notes[1].onset, 0.5);
        assert_eq!(top.notes[2].measure, 1);
        assert_eq!(top.notes[3].measure, 2);
        assert_eq!(top.notes[3].beat, 0.0);
        let mel = top.melody();
        assert_eq!(mel.len(), 4, "tie merged");
        assert_eq!(mel[2].duration, 2.0);
        assert_eq!(mel[2].tie, crate::score::Tie::None);
        assert_eq!(s.top_voice().map(|v| v.spine), Some(1));
        assert_eq!(s.bar_count(), 2);
    }

    #[test]
    fn follows_splits_and_merges() {
        let text = "**kern\n*M4/4\n=1\n*^\n4c\t4e\n4d\t4f\n*v\t*v\n2g\n=2\n1c\n*-\n";
        let s = parse(text).unwrap();
        assert_eq!(s.voices.len(), 2);
        assert_eq!(s.voices[0].notes.len(), 4);
        assert_eq!(s.voices[1].notes.len(), 2);
        assert_eq!(s.voices[0].notes[2].onset, 2.0);
        assert_eq!(s.voices[0].notes[3].measure, 2);
        assert_eq!(s.voices[1].spine, 0);
    }
}
