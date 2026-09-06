//! Reducing French text to the vowels a singer would hold.
//!
//! Consonants are dropped, digraphs are read the French way (`ou` → /u/,
//! `eau` → /o/, `an` → /ɑ̃/ before a consonant …) and a final mute *e* is
//! kept only when it is the whole syllable count of the word, so *libre*
//! gives one vowel and *liberté* three. The result sounds like French and
//! means nothing, which is the point.

use crate::vowel::Vowel;

/// Reduces `text` to the vowel of each syllable.
pub fn vowels(text: &str) -> Vec<Vowel> {
    let mut out = Vec::new();
    for word in text.split(|c: char| !c.is_alphabetic() && c != '\'' && c != '’') {
        if word.is_empty() {
            continue;
        }
        let lower: Vec<char> = word
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphabetic())
            .collect();
        let mut word_vowels = Vec::new();
        let mut i = 0;
        while i < lower.len() {
            let c = lower.get(i).copied().unwrap_or(' ');
            let next = lower.get(i + 1).copied();
            let next2 = lower.get(i + 2).copied();
            let after_nasal_ok = |k: usize| {
                // `an` is nasal before a consonant or at the end, not before
                // a vowel (as in *année*) or a second n/m.
                match lower.get(k) {
                    None => true,
                    Some(ch) => !is_vowel_letter(*ch) && *ch != 'n' && *ch != 'm',
                }
            };
            let (v, len) = match (c, next, next2) {
                ('e', Some('a'), Some('u')) => (Some(Vowel::O), 3),
                ('a' | 'e' | 'o', Some('i'), Some('n')) if after_nasal_ok(i + 3) => {
                    (Some(Vowel::In), 3)
                }
                ('o', Some('u'), _) => (Some(Vowel::U), 2),
                ('a', Some('u'), _) => (Some(Vowel::O), 2),
                ('e', Some('u'), _) | ('œ', Some('u'), _) => (Some(Vowel::Eu), 2),
                ('o', Some('i'), _) => (Some(Vowel::A), 2),
                ('a', Some('i'), _) | ('e', Some('i'), _) => (Some(Vowel::Eh), 2),
                ('a' | 'e', Some('n' | 'm'), _) if after_nasal_ok(i + 2) => (Some(Vowel::An), 2),
                ('o', Some('n' | 'm'), _) if after_nasal_ok(i + 2) => (Some(Vowel::On), 2),
                ('i', Some('n' | 'm'), _) | ('u', Some('n'), _) | ('y', Some('n'), _)
                    if after_nasal_ok(i + 2) =>
                {
                    (Some(Vowel::In), 2)
                }
                ('a' | 'à' | 'â', _, _) => (Some(Vowel::A), 1),
                ('é', _, _) => (Some(Vowel::E), 1),
                ('è' | 'ê' | 'ë', _, _) => (Some(Vowel::Eh), 1),
                ('e', _, _) => {
                    // Mute e at the end of a word; /ɛ/ before two consonants.
                    let at_end = i + 1 == lower.len()
                        || (i + 2 == lower.len() && matches!(next, Some('s' | 't')));
                    if at_end {
                        (Some(Vowel::Schwa), 1)
                    } else if next.is_some_and(|n| !is_vowel_letter(n))
                        && next2.is_some_and(|n| !is_vowel_letter(n))
                    {
                        (Some(Vowel::Eh), 1)
                    } else {
                        (Some(Vowel::Eu), 1)
                    }
                }
                ('i' | 'î' | 'ï' | 'y', _, _) => (Some(Vowel::I), 1),
                ('o' | 'ô', _, _) => (Some(Vowel::O), 1),
                ('u' | 'û' | 'ù', _, _) => (Some(Vowel::Y), 1),
                _ => (None, 1),
            };
            if let Some(v) = v {
                word_vowels.push(v);
            }
            i += len;
        }
        // A final mute e only counts when it is the only vowel of the word.
        if word_vowels.len() > 1 && word_vowels.last() == Some(&Vowel::Schwa) {
            word_vowels.pop();
        }
        out.extend(word_vowels);
    }
    out
}

fn is_vowel_letter(c: char) -> bool {
    matches!(
        c,
        'a' | 'e' | 'i' | 'o' | 'u' | 'y' | 'à' | 'â' | 'é' | 'è' | 'ê' | 'ë' | 'î' | 'ï' | 'ô' | 'û' | 'ù' | 'œ'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_french() {
        assert_eq!(vowels("Ah ! ça ira"), vec![Vowel::A, Vowel::A, Vowel::I, Vowel::A]);
        assert_eq!(
            vowels("Dansons la carmagnole"),
            vec![Vowel::An, Vowel::On, Vowel::A, Vowel::A, Vowel::A, Vowel::O]
        );
        assert_eq!(vowels("vive le son du canon"), vec![Vowel::I, Vowel::Schwa, Vowel::On, Vowel::Y, Vowel::A, Vowel::On]);
        assert_eq!(vowels("Allons enfants"), vec![Vowel::A, Vowel::On, Vowel::An, Vowel::An]);
        assert_eq!(vowels("le"), vec![Vowel::Schwa]);
        assert_eq!(vowels("liberté"), vec![Vowel::I, Vowel::Eh, Vowel::E]);
        assert_eq!(vowels("eau"), vec![Vowel::O]);
        assert!(vowels("!!! ...").is_empty());
    }
}
