# rameau_voix

A singing voice that is *not* a sample library: every vowel is synthesized
by a small Klatt-style formant model (a glottal pulse train through a
cascade of resonators, with vibrato and a little breath) and packaged as a
SoundFont bank that `rameau_synthesizer` plays like any other instrument.

Two banks, thirteen French vowels each:

| bank | preset | sound |
|------|--------|-------|
| 64 | 0–12 | one singer, three registers (bass, tenor, soprano samples keyed at A2, A3, A4) |
| 65 | 0–12 | a crowd: several detuned singers with drifting vibrato |

Singing a word is a sequence of program changes: `lyric::vowels("Ah ! ça
ira")` reduces text to `[A, A, I, A]`, the consonants are dropped, and the
sequencer selects the vowel preset before each note-on. The result reads as
French without being words, which is the intent.

```rust,no_run
use rameau_soundfont::SoundFont;
use rameau_voix::{VoixConfig, build_bank};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut backend = rameau_software::Software::new(44_100);
let mut bank: SoundFont<_> = SoundFont::load_file_with("gm.sf2", &mut backend)?;
bank.merge(build_bank(&mut backend, &VoixConfig::default())?);
# Ok(()) }
```
