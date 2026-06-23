//! An in-memory, format-independent representation of a SoundFont.
//!
//! The types in this module describe *what* a SoundFont contains, not *how* it
//! was stored on disk. Whether a bank was loaded from an uncompressed `.sf2`
//! file or an Ogg/Vorbis-compressed `.sf3` file, it ends up in the same
//! [`SoundFont`] structure with sample audio decoded to plain 16-bit PCM.
//!
//! See [`crate::load`] for the loader that produces these values.

use crate::generator::GeneratorType;
use rameau_clip::{AudioClip, Clip};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A complete SoundFont bank.
///
/// This is the abstract representation that the rest of the crate works with.
/// It deliberately carries no information about the on-disk container, so two
/// banks that sound identical compare equal regardless of how they were loaded.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SoundFont {
    /// Bank-level metadata (the `INFO` list).
    pub info: Info,
    /// Presets, addressable by MIDI bank/program (the `phdr` hydra).
    pub presets: Vec<Preset>,
    /// Instruments referenced by preset zones (the `inst` hydra).
    pub instruments: Vec<Instrument>,
    /// Samples referenced by instrument zones, with audio decoded to PCM.
    pub samples: Vec<Sample>,
}

/// A `major.minor` version number, as stored in the `ifil`/`iver` records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Version {
    pub major: u16,
    pub minor: u16,
}

/// Bank-level metadata from the `INFO` list.
///
/// Every field other than [`Info::version`] is optional because the SoundFont
/// specification only requires `ifil`, `isng` and `INAM` to be present, and
/// even those are not always well-formed in the wild.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Info {
    /// SoundFont specification version (`ifil`).
    pub version: Version,
    /// Target wavetable sound engine (`isng`).
    pub engine: Option<String>,
    /// Bank name (`INAM`).
    pub name: Option<String>,
    /// Wavetable ROM name (`irom`).
    pub rom_name: Option<String>,
    /// Wavetable ROM version (`iver`).
    pub rom_version: Option<Version>,
    /// Creation date (`ICRD`).
    pub creation_date: Option<String>,
    /// Sound designers and engineers (`IENG`).
    pub engineers: Option<String>,
    /// Intended product / target (`IPRD`).
    pub product: Option<String>,
    /// Copyright notice (`ICOP`).
    pub copyright: Option<String>,
    /// Free-form comments (`ICMT`).
    pub comments: Option<String>,
    /// Software used to create or edit the bank (`ISFT`).
    pub software: Option<String>,
}

/// A preset: the unit addressed by a MIDI bank-select + program-change.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Preset {
    /// Preset name.
    pub name: String,
    /// MIDI program number (`wPreset`).
    pub program: u16,
    /// MIDI bank number (`wBank`).
    pub bank: u16,
    /// Reserved, intended for sound-library management (`dwLibrary`).
    pub library: u32,
    /// Reserved, intended for genre classification (`dwGenre`).
    pub genre: u32,
    /// Reserved, intended for morphology (`dwMorphology`).
    pub morphology: u32,
    /// Preset zones. The first zone may be a global zone (one with no
    /// `Instrument` generator) whose settings apply to all following zones.
    pub zones: Vec<Zone>,
}

/// An instrument: a layer of sample zones shared by one or more presets.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Instrument {
    /// Instrument name.
    pub name: String,
    /// Instrument zones. As with presets, the first zone may be global (one
    /// with no `SampleID` generator).
    pub zones: Vec<Zone>,
}

/// A zone: a set of generators and modulators that together define a region of
/// the key/velocity space and how it is articulated.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Zone {
    /// Generators in this zone, in file order.
    pub generators: Vec<Generator>,
    /// Modulators in this zone, in file order.
    pub modulators: Vec<Modulator>,
}

/// A single generator: a synthesis parameter and its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Generator {
    /// Which parameter this generator sets.
    pub kind: GeneratorType,
    /// The value, interpreted according to `kind`.
    pub amount: GeneratorAmount,
}

/// The value of a [`Generator`].
///
/// A generator amount is a two-byte union in the file; the meaning of those
/// bytes depends on the generator. This enum captures the interpreted value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum GeneratorAmount {
    /// A signed 16-bit amount (the common case).
    Short(i16),
    /// An unsigned 16-bit amount, used for index generators such as
    /// `Instrument` and `SampleID`.
    Word(u16),
    /// An inclusive `low..=high` range, used for `KeyRange`/`VelocityRange`.
    Range(Range),
}

/// An inclusive byte range, as used by the range generators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Range {
    pub low: u8,
    pub high: u8,
}

/// A single modulator (an `SFModList`/`SFInstModList` record).
///
/// The controller operators are kept in their raw 16-bit encoded form. They
/// are self-contained bit fields that do not reference the file layout, so they
/// remain valid in the abstract representation; decoding them into concrete
/// controller sources is left to the synthesizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Modulator {
    /// Source controller operator (`sfModSrcOper`).
    pub source: u16,
    /// Destination generator being modulated (`sfModDestOper`).
    pub destination: GeneratorType,
    /// Modulation amount (`modAmount`).
    pub amount: i16,
    /// Secondary source controller, scaling `amount` (`sfModAmtSrcOper`).
    pub amount_source: u16,
    /// Transform applied to the modulation (`sfModTransOper`).
    pub transform: u16,
}

/// How a [`Sample`] participates in stereo and ROM playback (`sfSampleType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum SampleType {
    #[default]
    Mono,
    Right,
    Left,
    Linked,
    RomMono,
    RomRight,
    RomLeft,
    RomLinked,
    /// An unrecognized type flag, preserved verbatim.
    Other(u16),
}

impl From<u16> for SampleType {
    fn from(value: u16) -> Self {
        match value {
            1 => SampleType::Mono,
            2 => SampleType::Right,
            4 => SampleType::Left,
            8 => SampleType::Linked,
            0x8001 => SampleType::RomMono,
            0x8002 => SampleType::RomRight,
            0x8004 => SampleType::RomLeft,
            0x8008 => SampleType::RomLinked,
            other => SampleType::Other(other),
        }
    }
}

/// A sample: a single channel of audio plus its playback parameters.
///
/// The audio is always decoded 16-bit PCM, regardless of whether the source
/// file stored it as raw samples (`.sf2`) or Ogg/Vorbis (`.sf3`). The decoded
/// PCM and its sample rate are held in [`clip`](Sample::clip); all loop offsets
/// are expressed in sample frames relative to that clip's data, so nothing here
/// depends on the global sample pool of the original file.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Sample {
    /// Sample name.
    pub name: String,
    /// Decoded 16-bit mono PCM and its sample rate.
    pub clip: Clip<i16>,
    /// Loop start, in sample frames into [`clip`](Sample::clip)'s data.
    pub loop_start: u32,
    /// Loop end, in sample frames into [`clip`](Sample::clip)'s data.
    pub loop_end: u32,
    /// MIDI key number of the recorded pitch (`byOriginalKey`).
    pub original_key: u8,
    /// Pitch correction in cents (`chCorrection`).
    pub correction: i8,
    /// Index of the linked sample for stereo pairs (`wSampleLink`).
    pub link: u16,
    /// Stereo/ROM classification (`sfSampleType`).
    pub kind: SampleType,
}

impl AudioClip for Sample {
    type Value = i16;

    fn data(&self) -> &[i16] {
        self.clip.data()
    }

    fn sample_rate(&self) -> u32 {
        self.clip.sample_rate()
    }
}
