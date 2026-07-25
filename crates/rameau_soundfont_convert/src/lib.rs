//! Conversion of SoundFont banks to the Ogg/Vorbis-compressed `.sf3` format.
//!
//! [`rameau_soundfont`] reads both `.sf2` and `.sf3` into the same abstract
//! `SoundFont` model; this crate writes that model back out as `.sf3`, in which
//! each sample is stored as an independent Ogg/Vorbis stream rather than as raw
//! PCM in one shared pool. For a typical bank that is a severalfold size
//! reduction, at the cost of lossy audio.
//!
//! ```no_run
//! use rameau_soundfont_convert::{Quality, convert_file};
//!
//! convert_file("bank.sf2", "bank.sf3", Quality::default())?;
//! # Ok::<(), rameau_soundfont_convert::Error>(())
//! ```
//!
//! A `sf2-to-sf3` command-line tool is included:
//!
//! ```text
//! sf2-to-sf3 assets/Unison.SF2 Unison.sf3 -q 0.5
//! ```
//!
//! # Round-trip fidelity
//!
//! Conversion goes through the abstract model, so anything the loader does not
//! model is not carried across. In practice that is two things, both absent
//! from every bank tested: generators whose operator falls outside the known
//! `SFGenerator` enumeration, and modulators whose destination is a link to
//! another modulator rather than to a generator. A bank using either would lose
//! those records. Everything else — presets, instruments, zones, generators,
//! modulators, sample metadata and loop points — survives intact.
//!
//! Sample audio is re-encoded, so it is not bit-identical. The frame *count* is
//! preserved exactly, which is what keeps loop points valid.

mod encode;
mod hydra;
mod write;

pub use encode::Quality;
pub use write::{convert_file, save_sf3, write_sf3};

/// An error encountered while converting or writing a SoundFont.
#[derive(Debug)]
pub enum Error {
    /// An I/O error writing the output.
    Io(std::io::Error),
    /// The input bank could not be loaded (only from [`convert_file`]).
    Load(rameau_soundfont::Error),
    /// A sample could not be encoded to Ogg/Vorbis.
    Encode(vorbis_rs::VorbisError),
    /// A count or offset exceeded what the SoundFont format can address.
    ///
    /// Bag, generator, modulator, instrument and sample indices are all 16-bit,
    /// and the sample pool is addressed by a 32-bit byte offset. These ceilings
    /// are far above any realistic bank, but must not be allowed to wrap
    /// silently into a corrupt file.
    TooLarge(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::Load(e) => write!(f, "could not load input soundfont: {e}"),
            Error::Encode(e) => write!(f, "vorbis encode error: {e}"),
            Error::TooLarge(what) => write!(f, "too large for the soundfont format: {what}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Load(e) => Some(e),
            Error::Encode(e) => Some(e),
            Error::TooLarge(_) => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<rameau_soundfont::Error> for Error {
    fn from(e: rameau_soundfont::Error) -> Self {
        Error::Load(e)
    }
}

impl From<vorbis_rs::VorbisError> for Error {
    fn from(e: vorbis_rs::VorbisError) -> Self {
        Error::Encode(e)
    }
}
