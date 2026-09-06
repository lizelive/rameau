//! A formant-synthesized singing voice as a SoundFont bank.
//!
//! See the crate README for the layout. The pieces:
//!
//! * [`Vowel`] — the thirteen French vowels and their formants.
//! * [`synth`] — the formant synthesizer that renders one looping sample.
//! * [`build_bank`] — packages every vowel at three registers, solo and
//!   choir, into a [`SoundFont`] a synthesizer can play.
//! * [`lyric`] — reduces French text to a vowel sequence.

#![forbid(unsafe_code)]

pub mod bank;
pub mod lyric;
pub mod synth;
pub mod vowel;

pub use bank::{CHOIR_BANK, SOLO_BANK, VoixConfig, build_bank, select_vowel};
pub use lyric::vowels;
pub use rameau_soundfont::SoundFont;
pub use vowel::Vowel;
