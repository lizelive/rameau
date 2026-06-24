//! Writing [`AudioClip`](rameau_clip::AudioClip)s to canonical 16-bit PCM WAV
//! files.
//!
//! Only the mono, 16-bit little-endian PCM case is produced — the same format
//! the rest of this workspace decodes audio to. Any `AudioClip` whose samples
//! are `i16` can be saved:
//!
//! ```no_run
//! use rameau_clip::Clip;
//! let clip = Clip::new(vec![0i16, 16384, -16384, 0], 44_100);
//! rameau_wav::save(&clip, "tone.wav").unwrap();
//! ```

mod write;

pub use write::{save, write};
