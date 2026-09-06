//! Parsing a single `**kern` data token.

use rameau_theory::Midi;

use crate::score::Tie;

/// One sounding element of a data token.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// Duration in crotchets (0 for grace notes).
    pub duration: f64,
    /// MIDI key, or `None` for a rest.
    pub midi: Option<Midi>,
    /// Tie state.
    pub tie: Tie,
    /// Ornament signs.
    pub ornaments: Vec<char>,
    /// Whether this is a grace note (`q`/`Q`).
    pub grace: bool,
}

/// Parses a data token — possibly a chord of space-separated notes — into
/// its notes. Returns an empty vector for a null token (`.`) or anything
/// unrecognisable.
pub fn parse_token(token: &str) -> Vec<Parsed> {
    if token == "." || token.is_empty() {
        return Vec::new();
    }
    token.split(' ').filter_map(parse_note).collect()
}

/// Parses one note of a token.
fn parse_note(s: &str) -> Option<Parsed> {
    let mut recip = String::new();
    let mut dots = 0u32;
    let mut letter: Option<char> = None;
    let mut letter_count = 0i32;
    let mut accidental = 0i32;
    let mut rest = false;
    let mut tie = Tie::None;
    let mut ornaments = Vec::new();
    let mut grace = false;
    let mut in_recip = true;

    for c in s.chars() {
        match c {
            '0'..='9' | '%' if in_recip => recip.push(c),
            '.' if in_recip && !recip.is_empty() => dots += 1,
            'a'..='g' | 'A'..='G' => {
                in_recip = false;
                match letter {
                    Some(l) if l == c => letter_count += 1,
                    Some(_) => return None,
                    None => {
                        letter = Some(c);
                        letter_count = 1;
                    }
                }
            }
            'r' => {
                in_recip = false;
                rest = true;
            }
            '#' => accidental += 1,
            '-' => accidental -= 1,
            'n' => accidental = 0,
            '[' => tie = Tie::Start,
            '_' => tie = Tie::Continue,
            ']' => tie = Tie::End,
            'q' | 'Q' => grace = true,
            'T' | 't' | 'M' | 'm' | 'W' | 'w' | 'S' | '$' | 'O' | 'o' | 'P' | 'p' => {
                ornaments.push(c);
            }
            // Beams, stems, slurs, phrase marks, articulations, editorial
            // marks: all layout, none of it timing.
            _ => {
                // A slur or phrase mark before the duration (`(16g`) must
                // not end the duration field before it has begun.
                if !recip.is_empty() {
                    in_recip = false;
                }
            }
        }
    }

    let duration = if grace { 0.0 } else { recip_to_quarters(&recip, dots)? };
    let midi = if rest {
        None
    } else {
        let l = letter?;
        let base = match l.to_ascii_lowercase() {
            'c' => 0,
            'd' => 2,
            'e' => 4,
            'f' => 5,
            'g' => 7,
            'a' => 9,
            _ => 11,
        };
        let octave = if l.is_ascii_lowercase() {
            4 + (letter_count - 1)
        } else {
            4 - letter_count
        };
        Some(12 * (octave + 1) + base + accidental)
    };
    Some(Parsed {
        duration,
        midi,
        tie,
        ornaments,
        grace,
    })
}

/// Converts a `**recip` value (`4`, `8.`, `0`, `12`, `3%2`) and dot count to
/// crotchets.
fn recip_to_quarters(recip: &str, dots: u32) -> Option<f64> {
    if recip.is_empty() {
        return None;
    }
    let base = if let Some((a, b)) = recip.split_once('%') {
        let a: f64 = a.parse().ok()?;
        let b: f64 = b.parse().ok()?;
        if a == 0.0 { return None; }
        4.0 * b / a
    } else if recip == "0" {
        8.0
    } else if recip == "00" {
        16.0
    } else if recip == "000" {
        32.0
    } else {
        let n: f64 = recip.parse().ok()?;
        if n == 0.0 { return None; }
        4.0 / n
    };
    // Each dot adds half of the previous addition.
    let mut total = base;
    let mut add = base;
    for _ in 0..dots {
        add /= 2.0;
        total += add;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitches_and_durations() {
        let n = parse_token("8g/L").remove(0);
        assert_eq!(n.midi, Some(67));
        assert_eq!(n.duration, 0.5);
        assert_eq!(parse_token("16cc#X\\JJ")[0].midi, Some(73));
        assert_eq!(parse_token("4B-")[0].midi, Some(58));
        assert_eq!(parse_token("2.r")[0].midi, None);
        assert_eq!(parse_token("2.r")[0].duration, 3.0);
        assert_eq!(parse_token("8..a")[0].duration, 0.875);
        assert_eq!(parse_token("0CC")[0].duration, 8.0);
        assert_eq!(parse_token("0CC")[0].midi, Some(36));
        assert_eq!(parse_token("12e")[0].duration, 4.0 / 12.0);
        assert_eq!(parse_token("(16g/LL")[0].duration, 0.25);
        assert!(parse_token(".").is_empty());
        assert!(parse_token("*").is_empty());
    }

    #[test]
    fn ties_chords_ornaments() {
        let n = parse_token("[8dd\\J")[0].clone();
        assert_eq!(n.tie, Tie::Start);
        let n = parse_token("8dd\\L]")[0].clone();
        assert_eq!(n.tie, Tie::End);
        let chord = parse_token("4g 4dd");
        assert_eq!(chord.len(), 2);
        assert_eq!(chord[1].midi, Some(74));
        let t = parse_token("4gT")[0].clone();
        assert_eq!(t.ornaments, vec!['T']);
        let g = parse_token("8qcc")[0].clone();
        assert!(g.grace);
        assert_eq!(g.duration, 0.0);
    }
}
