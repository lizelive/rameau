//! A format-independent SoundFont representation and a loader for `.sf2`/`.sf3`.
//!
//! [`SoundFont`] is the abstract model: it describes a bank's presets,
//! instruments and (PCM-decoded) samples without exposing how the bank was
//! stored. [`SoundFont::load_file`] and friends parse `.sf2` and `.sf3` files
//! into that model.
//!
//! Enable the `serde` feature to derive [`serde::Serialize`]/[`serde::Deserialize`]
//! for the whole model.

mod generator;
mod load;
mod soundfont;

pub use rameau_clip::{AudioClip, Clip};

pub use generator::GeneratorType;
pub use load::Error;
pub use soundfont::{
    Generator, GeneratorAmount, Info, Instrument, Modulator, Preset, Range, Sample, SampleType,
    SoundFont, Version, Zone,
};
