//! The Unison SoundFont bank, embedded as a compressed `.sf3` image.
//!
//! This crate exists so that a program can play something without first having
//! to find a SoundFont. It carries one general-MIDI bank — 168 presets, 231
//! instruments, 655 samples — compiled directly into the binary.
//!
//! ```no_run
//! // The decoded bank, shared and decoded at most once.
//! let bank = rameau_unison::soundfont();
//! assert!(!bank.presets.is_empty());
//! ```
//!
//! # Licensing
//!
//! The bank and the code are under **different** licences, and the difference
//! matters if you redistribute this crate:
//!
//! * The Rust source is AGPL-3.0-or-later, like the rest of the workspace.
//! * `Unison.sf3` is **CC0-1.0** — a public domain dedication — sourced from
//!   <https://rkhive.com/new/new_banks/unison.zip>.
//!
//! See the `Unison.LICENSE` file shipped alongside the bank.
//!
//! # Cost
//!
//! The bank adds about **6.6 MiB** to any binary that links this crate,
//! whether or not it is ever used: [`SOUNDFONT`] is `include_bytes!`, so the
//! data lives in read-only memory from the moment the program is loaded.
//!
//! Decoding, which is the expensive part, is deferred. See [`soundfont`].
//!
//! # Regenerating the bank
//!
//! `Unison.sf3` is generated from `assets/Unison.SF2` in this workspace:
//!
//! ```text
//! cargo run --release -p rameau_soundfont_convert --bin sf2-to-sf3 -- \
//!     assets/Unison.SF2 crates/rameau_unison/Unison.sf3
//! ```
//!
//! Quality 0.5 is the converter's default. Raising it grows the binary;
//! lowering it shrinks the binary and coarsens the audio.

use std::sync::LazyLock;

use rameau_playback::AudioPlayback;
use rameau_soundfont::{Error, SoundFont};

/// The raw `.sf3` image of the Unison bank.
///
/// About 6.6 MiB of Ogg/Vorbis-compressed SoundFont. Use this if you want to
/// write the bank to disk or parse it yourself; for the common case use
/// [`soundfont`] instead.
pub const SOUNDFONT: &[u8] = include_bytes!("../Unison.sf3");

/// The bank, decoded once on first use.
///
/// The panic is not a runtime condition worth handling: [`SOUNDFONT`] is a
/// compile-time constant that this crate's own tests parse and validate, so a
/// failure here means the compiled artifact is corrupt. Callers who want a
/// fallible path should use [`load_with`].
#[expect(
    clippy::expect_used,
    reason = "SOUNDFONT is a compile-time constant this crate's own tests parse; \
              a failure here means the build artifact is corrupt, which no \
              caller could meaningfully recover from"
)]
static BANK: LazyLock<SoundFont> = LazyLock::new(|| {
    SoundFont::from_bytes(SOUNDFONT)
        .expect("the embedded Unison bank failed to parse; the build artifact is corrupt")
});

/// The Unison bank, with its samples decoded to PCM.
///
/// The audio is decoded on the **first** call and cached, so a program that
/// links this crate but never calls this function pays only the binary size.
/// Concurrent first calls decode once and share the result.
///
/// Decoding 655 Ogg/Vorbis streams is not instant. If you are driving a
/// playback backend, prefer [`load_with`], which lets the backend decode into
/// its own clip type and skips building this intermediate PCM copy entirely.
pub fn soundfont() -> &'static SoundFont {
    &BANK
}

/// Loads the Unison bank into `backend`'s native clip type.
///
/// Unlike [`soundfont`] this is not cached: the clip type depends on the
/// backend, and the resulting clips are owned by it, so every backend needs its
/// own copy. A backend that decodes Ogg/Vorbis itself — as the kira backend
/// does — never materialises PCM at all.
///
/// # Errors
///
/// Returns [`Error::Backend`] if `backend` could not build a clip from a
/// sample. The embedded bank itself always parses, so the parsing variants of
/// [`Error`] do not arise here in practice.
pub fn load_with<P: AudioPlayback>(backend: &mut P) -> Result<SoundFont<P::Clip>, Error> {
    SoundFont::from_bytes_with(SOUNDFONT, backend)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of the bank, as reported by the conversion integration test
    /// for `assets/Unison.SF2`. A mismatch means the committed `.sf3` has
    /// drifted from the source bank it is generated from.
    const PRESETS: usize = 168;
    const INSTRUMENTS: usize = 231;
    const SAMPLES: usize = 655;

    #[test]
    fn embedded_bank_is_an_sf3() {
        assert!(!SOUNDFONT.is_empty());
        assert_eq!(&SOUNDFONT[0..4], b"RIFF");
        assert_eq!(&SOUNDFONT[8..12], b"sfbk");
        assert_eq!(soundfont().info.version.major, 3);
    }

    #[test]
    fn embedded_bank_matches_its_source() {
        let sf = soundfont();
        assert_eq!(sf.presets.len(), PRESETS);
        assert_eq!(sf.instruments.len(), INSTRUMENTS);
        assert_eq!(sf.samples.len(), SAMPLES);
    }

    /// A truncated or substituted bank would still parse; checking for a known
    /// preset by name catches that.
    #[test]
    fn contains_a_known_preset() {
        assert!(
            soundfont()
                .presets
                .iter()
                .any(|p| p.name == "ACOUSTIC GRAND PIANO"),
            "expected the general-MIDI grand piano preset"
        );
    }

    #[test]
    fn every_sample_has_audio() {
        for sample in &soundfont().samples {
            assert!(
                !sample.clip.data.is_empty(),
                "sample '{}' has no audio",
                sample.name
            );
            assert!(
                sample.sample_rate > 0,
                "sample '{}' has no rate",
                sample.name
            );
        }
    }

    /// The point of `LazyLock`: repeated calls must share one decode rather
    /// than redo it. Comparing addresses proves the cache, where comparing
    /// values would only prove determinism.
    #[test]
    fn decodes_once_and_caches() {
        let a: *const SoundFont = soundfont();
        let b: *const SoundFont = soundfont();
        assert_eq!(a, b, "each call re-decoded the bank instead of caching it");
    }
}
