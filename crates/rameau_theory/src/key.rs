//! Parsing key names and key signatures into [`Scale`]s.

use crate::pitch::PitchClass;
use crate::scale::{Mode, Scale};

/// A musical key; the same thing as a [`Scale`] seen from the outside.
pub type Key = Scale;

/// Why a key string could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyParseError(pub String);

impl core::fmt::Display for KeyParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "cannot parse key {:?}", self.0)
    }
}

impl core::error::Error for KeyParseError {}

/// Parses `"G major"`, `"D minor"`, `"A- major"`, `"Eb minor"`, `"g"` or the
/// Humdrum forms `"*G:"` (major) and `"*g:"` (minor).
///
/// A bare note name is read the Humdrum way: uppercase is major, lowercase
/// is minor.
///
/// # Errors
///
/// Returns [`KeyParseError`] when the tonic or mode is unrecognised.
pub fn parse_key(text: &str) -> Result<Scale, KeyParseError> {
    let t = text.trim().trim_start_matches('*').trim_end_matches(':').trim();
    if t.is_empty() {
        return Err(KeyParseError(text.to_owned()));
    }
    let mut parts = t.splitn(2, char::is_whitespace);
    let tonic_str = parts.next().unwrap_or_default();
    let mode_str = parts.next().map(str::trim);
    let tonic =
        PitchClass::parse(tonic_str).ok_or_else(|| KeyParseError(text.to_owned()))?;
    let mode = match mode_str {
        Some(m) if !m.is_empty() => {
            Mode::parse(m).ok_or_else(|| KeyParseError(text.to_owned()))?
        }
        _ => {
            let first = tonic_str.chars().next().unwrap_or('C');
            if first.is_ascii_lowercase() { Mode::Minor } else { Mode::Major }
        }
    };
    Ok(Scale::new(tonic, mode))
}

/// The major tonic implied by a key signature of `sharps` sharps (negative
/// for flats).
pub const fn major_tonic_of_signature(sharps: i32) -> PitchClass {
    PitchClass::new(7 * sharps)
}

/// The number of sharps (negative for flats) in the signature of `scale`,
/// for the major and natural-minor modes; other modes use their nearest
/// major/minor relative.
pub fn signature_of(scale: &Scale) -> i32 {
    let major = if scale.mode.is_minor() { scale.relative() } else { *scale };
    // Position of the tonic in the circle of fifths, folded to -6..=6.
    let fifths = (0..12)
        .find(|&k| PitchClass::new(7 * k) == major.tonic)
        .unwrap_or(0);
    if fifths > 6 { fifths - 12 } else { fifths }
}

/// Parses a Humdrum key-signature token such as `*k[f#c#]` or `*k[b-e-]`,
/// returning the number of sharps (negative for flats).
pub fn parse_kern_signature(token: &str) -> Option<i32> {
    let inner = token.strip_prefix("*k[")?.strip_suffix(']')?;
    let sharps = inner.matches('#').count() as i32;
    let flats = inner.matches('-').count() as i32;
    Some(sharps - flats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names() {
        assert_eq!(parse_key("G major").unwrap(), Scale::new(PitchClass::G, Mode::Major));
        assert_eq!(parse_key("A- major").unwrap(), Scale::new(PitchClass::new(8), Mode::Major));
        assert_eq!(parse_key("*g:").unwrap(), Scale::new(PitchClass::G, Mode::Minor));
        assert_eq!(parse_key("*E-:").unwrap(), Scale::new(PitchClass::new(3), Mode::Major));
        assert!(parse_key("H dur").is_err());
    }

    #[test]
    fn signatures() {
        assert_eq!(signature_of(&Scale::new(PitchClass::G, Mode::Major)), 1);
        assert_eq!(signature_of(&Scale::new(PitchClass::F, Mode::Major)), -1);
        assert_eq!(signature_of(&Scale::new(PitchClass::A, Mode::Minor)), 0);
        assert_eq!(parse_kern_signature("*k[b-e-]"), Some(-2));
        assert_eq!(major_tonic_of_signature(-2), PitchClass::new(10));
    }
}
