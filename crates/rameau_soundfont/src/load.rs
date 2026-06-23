//! Loading of `.sf2`/`.sf3` files into the abstract [`SoundFont`] model.
//!
//! Both formats share the same RIFF/hydra layout; they differ only in how the
//! sample audio is stored. In `.sf2` the `smpl` chunk is a single pool of raw
//! little-endian 16-bit PCM, and each sample header indexes into it by sample
//! frame. In `.sf3` the `smpl` chunk is a concatenation of independent
//! Ogg/Vorbis streams, and each sample header gives the *byte* range of its
//! stream. This module hides that distinction: every [`Sample`] comes out with
//! its audio decoded to PCM and its loop points rebased to its own data.

use std::io::{Cursor, Read, Seek};
use std::path::Path;

use rameau_clip::Clip;
use riff::{Chunk, ChunkId};

use crate::generator::GeneratorType;
use crate::soundfont::{
    Generator, GeneratorAmount, Info, Instrument, Modulator, Preset, Range, Sample, SampleType,
    SoundFont, Version, Zone,
};

/// An error encountered while loading a SoundFont.
#[derive(Debug)]
pub enum Error {
    /// An I/O error reading the file or stream.
    Io(std::io::Error),
    /// The file is not a well-formed SoundFont.
    Format(String),
    /// An Ogg/Vorbis sample (in an `.sf3` file) could not be decoded.
    Vorbis(lewton::VorbisError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::Format(m) => write!(f, "malformed soundfont: {m}"),
            Error::Vorbis(e) => write!(f, "vorbis decode error: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Vorbis(e) => Some(e),
            Error::Format(_) => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<lewton::VorbisError> for Error {
    fn from(e: lewton::VorbisError) -> Self {
        Error::Vorbis(e)
    }
}

fn format(msg: impl Into<String>) -> Error {
    Error::Format(msg.into())
}

impl SoundFont {
    /// Loads a SoundFont from a `.sf2`/`.sf3` file on disk.
    pub fn load_file(path: impl AsRef<Path>) -> Result<Self, Error> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes)
    }

    /// Loads a SoundFont from a seekable reader.
    pub fn load(mut reader: impl Read + Seek) -> Result<Self, Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Self::from_bytes(&bytes)
    }

    /// Loads a SoundFont from an in-memory `.sf2`/`.sf3` image.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let mut cursor = Cursor::new(bytes);

        let riff = Chunk::read(&mut cursor, 0)?;
        if riff.id() != id(b"RIFF") {
            return Err(format("missing RIFF header"));
        }
        if riff.read_type(&mut cursor)? != id(b"sfbk") {
            return Err(format("not a soundfont (form type is not 'sfbk')"));
        }

        let mut info_list = None;
        let mut sdta_list = None;
        let mut pdta_list = None;
        for child in children(&riff, &mut cursor)? {
            if child.id() != id(b"LIST") {
                continue;
            }
            match child.read_type(&mut cursor)?.value {
                b if &b == b"INFO" => info_list = Some(child),
                b if &b == b"sdta" => sdta_list = Some(child),
                b if &b == b"pdta" => pdta_list = Some(child),
                _ => {}
            }
        }

        let info = match info_list {
            Some(list) => read_info(&list, &mut cursor)?,
            None => return Err(format("missing INFO list")),
        };
        let sdta = sdta_list.ok_or_else(|| format("missing sdta list"))?;
        let pdta = pdta_list.ok_or_else(|| format("missing pdta list"))?;

        let smpl = leaf(&sdta, &mut cursor, b"smpl")?.unwrap_or_default();
        let hydra = Hydra::read(&pdta, &mut cursor)?;

        let samples = build_samples(&hydra.shdr, &smpl)?;
        let instruments = build_instruments(&hydra)?;
        let presets = build_presets(&hydra)?;

        Ok(SoundFont {
            info,
            presets,
            instruments,
            samples,
        })
    }
}

// ----- RIFF helpers ---------------------------------------------------------

fn id(bytes: &[u8; 4]) -> ChunkId {
    ChunkId { value: *bytes }
}

/// Collects the direct children of a `RIFF`/`LIST` chunk, releasing the borrow
/// on the stream so their contents can then be read.
fn children<R: Read + Seek>(chunk: &Chunk, stream: &mut R) -> Result<Vec<Chunk>, Error> {
    chunk
        .iter(stream)
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(Error::from)
}

/// Reads the raw contents of the named leaf chunk within `list`, if present.
fn leaf<R: Read + Seek>(
    list: &Chunk,
    stream: &mut R,
    name: &[u8; 4],
) -> Result<Option<Vec<u8>>, Error> {
    for child in children(list, stream)? {
        if child.id() == id(name) {
            return Ok(Some(child.read_contents(stream)?));
        }
    }
    Ok(None)
}

// ----- INFO -----------------------------------------------------------------

fn read_info<R: Read + Seek>(list: &Chunk, stream: &mut R) -> Result<Info, Error> {
    let mut info = Info::default();
    for child in children(list, stream)? {
        let data = child.read_contents(stream)?;
        match &child.id().value {
            b"ifil" => info.version = parse_version(&data),
            b"iver" => info.rom_version = Some(parse_version(&data)),
            b"isng" => info.engine = Some(parse_zstr(&data)),
            b"INAM" => info.name = Some(parse_zstr(&data)),
            b"irom" => info.rom_name = Some(parse_zstr(&data)),
            b"ICRD" => info.creation_date = Some(parse_zstr(&data)),
            b"IENG" => info.engineers = Some(parse_zstr(&data)),
            b"IPRD" => info.product = Some(parse_zstr(&data)),
            b"ICOP" => info.copyright = Some(parse_zstr(&data)),
            b"ICMT" => info.comments = Some(parse_zstr(&data)),
            b"ISFT" => info.software = Some(parse_zstr(&data)),
            _ => {}
        }
    }
    Ok(info)
}

fn parse_version(data: &[u8]) -> Version {
    Version {
        major: le_u16(data, 0),
        minor: le_u16(data, 2),
    }
}

/// Parses a NUL-terminated, possibly padded string, trimming trailing
/// whitespace and NULs.
fn parse_zstr(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).trim_end().to_string()
}

// ----- pdta (the "hydra") ---------------------------------------------------

/// One `phdr` record (`SFPresetHeader`).
struct PresetHeader {
    name: String,
    program: u16,
    bank: u16,
    bag: u16,
    library: u32,
    genre: u32,
    morphology: u32,
}

/// One `inst` record (`SFInst`).
struct InstHeader {
    name: String,
    bag: u16,
}

/// One `pbag`/`ibag` record (`SFBag`).
struct Bag {
    gen_ndx: u16,
    mod_ndx: u16,
}

/// One `pmod`/`imod` record (`SFModList`).
struct ModRecord {
    src: u16,
    dest: u16,
    amount: i16,
    amt_src: u16,
    trans: u16,
}

/// One `pgen`/`igen` record (`SFGenList`).
struct GenRecord {
    oper: u16,
    amount: [u8; 2],
}

/// One `shdr` record (`SFSample`).
struct SampleHeader {
    name: String,
    start: u32,
    end: u32,
    start_loop: u32,
    end_loop: u32,
    sample_rate: u32,
    original_key: u8,
    correction: i8,
    link: u16,
    sample_type: u16,
}

/// The decoded `pdta` sub-chunks.
struct Hydra {
    phdr: Vec<PresetHeader>,
    pbag: Vec<Bag>,
    pmod: Vec<ModRecord>,
    pgen: Vec<GenRecord>,
    inst: Vec<InstHeader>,
    ibag: Vec<Bag>,
    imod: Vec<ModRecord>,
    igen: Vec<GenRecord>,
    shdr: Vec<SampleHeader>,
}

impl Hydra {
    fn read<R: Read + Seek>(pdta: &Chunk, stream: &mut R) -> Result<Self, Error> {
        let mut chunks = std::collections::HashMap::new();
        for child in children(pdta, stream)? {
            chunks.insert(child.id().value, child.read_contents(stream)?);
        }
        let get = |name: &[u8; 4]| -> Result<&Vec<u8>, Error> {
            chunks
                .get(name)
                .ok_or_else(|| format(format!("missing '{}' chunk", String::from_utf8_lossy(name))))
        };

        Ok(Hydra {
            phdr: records(get(b"phdr")?, 38, parse_phdr)?,
            pbag: records(get(b"pbag")?, 4, parse_bag)?,
            pmod: records(get(b"pmod")?, 10, parse_mod)?,
            pgen: records(get(b"pgen")?, 4, parse_gen)?,
            inst: records(get(b"inst")?, 22, parse_inst)?,
            ibag: records(get(b"ibag")?, 4, parse_bag)?,
            imod: records(get(b"imod")?, 10, parse_mod)?,
            igen: records(get(b"igen")?, 4, parse_gen)?,
            shdr: records(get(b"shdr")?, 46, parse_shdr)?,
        })
    }
}

/// Splits a hydra chunk into fixed-size records and parses each one.
fn records<T>(data: &[u8], size: usize, parse: impl Fn(&[u8]) -> T) -> Result<Vec<T>, Error> {
    if data.len() % size != 0 {
        return Err(format(
            "hydra chunk length is not a multiple of the record size",
        ));
    }
    Ok(data.chunks_exact(size).map(parse).collect())
}

fn parse_phdr(r: &[u8]) -> PresetHeader {
    PresetHeader {
        name: parse_zstr(&r[0..20]),
        program: le_u16(r, 20),
        bank: le_u16(r, 22),
        bag: le_u16(r, 24),
        library: le_u32(r, 26),
        genre: le_u32(r, 30),
        morphology: le_u32(r, 34),
    }
}

fn parse_inst(r: &[u8]) -> InstHeader {
    InstHeader {
        name: parse_zstr(&r[0..20]),
        bag: le_u16(r, 20),
    }
}

fn parse_bag(r: &[u8]) -> Bag {
    Bag {
        gen_ndx: le_u16(r, 0),
        mod_ndx: le_u16(r, 2),
    }
}

fn parse_mod(r: &[u8]) -> ModRecord {
    ModRecord {
        src: le_u16(r, 0),
        dest: le_u16(r, 2),
        amount: le_i16(r, 4),
        amt_src: le_u16(r, 6),
        trans: le_u16(r, 8),
    }
}

fn parse_gen(r: &[u8]) -> GenRecord {
    GenRecord {
        oper: le_u16(r, 0),
        amount: [r[2], r[3]],
    }
}

fn parse_shdr(r: &[u8]) -> SampleHeader {
    SampleHeader {
        name: parse_zstr(&r[0..20]),
        start: le_u32(r, 20),
        end: le_u32(r, 24),
        start_loop: le_u32(r, 28),
        end_loop: le_u32(r, 32),
        sample_rate: le_u32(r, 36),
        original_key: r[40],
        correction: r[41] as i8,
        link: le_u16(r, 42),
        sample_type: le_u16(r, 44),
    }
}

// ----- assembling zones -----------------------------------------------------

/// Builds the list of zones for one preset/instrument, given the half-open
/// `[from, to)` range of bag records that belong to it.
fn build_zones(
    bags: &[Bag],
    gens: &[GenRecord],
    mods: &[ModRecord],
    from: usize,
    to: usize,
) -> Result<Vec<Zone>, Error> {
    let mut zones = Vec::with_capacity(to.saturating_sub(from));
    for b in from..to {
        let bag = bags
            .get(b)
            .ok_or_else(|| format("bag index out of range"))?;
        let next = bags
            .get(b + 1)
            .ok_or_else(|| format("bag index out of range"))?;

        let gen_slice = slice(gens, bag.gen_ndx as usize, next.gen_ndx as usize)?;
        let mod_slice = slice(mods, bag.mod_ndx as usize, next.mod_ndx as usize)?;

        let generators = gen_slice
            .iter()
            .filter_map(|g| {
                GeneratorType::from_u16(g.oper).map(|kind| Generator {
                    kind,
                    amount: gen_amount(kind, g.amount),
                })
            })
            .collect();

        let modulators = mod_slice
            .iter()
            .map(|m| Modulator {
                source: m.src,
                // An unknown destination is preserved as `UNUSED_END`; the raw
                // operator stays available via the modulator's own fields.
                destination: GeneratorType::from_u16(m.dest).unwrap_or(GeneratorType::UNUSED_END),
                amount: m.amount,
                amount_source: m.amt_src,
                transform: m.trans,
            })
            .collect();

        zones.push(Zone {
            generators,
            modulators,
        });
    }
    Ok(zones)
}

/// Interprets a raw generator amount according to its generator type.
fn gen_amount(kind: GeneratorType, raw: [u8; 2]) -> GeneratorAmount {
    use GeneratorType::*;
    match kind {
        KEY_RANGE | VELOCITY_RANGE => GeneratorAmount::Range(Range {
            low: raw[0],
            high: raw[1],
        }),
        INSTRUMENT | SAMPLE_ID => GeneratorAmount::Word(u16::from_le_bytes(raw)),
        _ => GeneratorAmount::Short(i16::from_le_bytes(raw)),
    }
}

fn build_presets(hydra: &Hydra) -> Result<Vec<Preset>, Error> {
    // The final phdr record is the terminal "EOP" sentinel; it only bounds the
    // zones of the preceding preset.
    let count = hydra.phdr.len().saturating_sub(1);
    let mut presets = Vec::with_capacity(count);
    for i in 0..count {
        let header = &hydra.phdr[i];
        let next = &hydra.phdr[i + 1];
        let zones = build_zones(
            &hydra.pbag,
            &hydra.pgen,
            &hydra.pmod,
            header.bag as usize,
            next.bag as usize,
        )?;
        presets.push(Preset {
            name: header.name.clone(),
            program: header.program,
            bank: header.bank,
            library: header.library,
            genre: header.genre,
            morphology: header.morphology,
            zones,
        });
    }
    Ok(presets)
}

fn build_instruments(hydra: &Hydra) -> Result<Vec<Instrument>, Error> {
    // The final inst record is the terminal "EOI" sentinel.
    let count = hydra.inst.len().saturating_sub(1);
    let mut instruments = Vec::with_capacity(count);
    for i in 0..count {
        let header = &hydra.inst[i];
        let next = &hydra.inst[i + 1];
        let zones = build_zones(
            &hydra.ibag,
            &hydra.igen,
            &hydra.imod,
            header.bag as usize,
            next.bag as usize,
        )?;
        instruments.push(Instrument {
            name: header.name.clone(),
            zones,
        });
    }
    Ok(instruments)
}

// ----- samples --------------------------------------------------------------

/// Bit in `sfSampleType` that marks an Ogg/Vorbis-compressed sample (`.sf3`).
const SAMPLE_TYPE_COMPRESSED: u16 = 0x10;

fn build_samples(headers: &[SampleHeader], smpl: &[u8]) -> Result<Vec<Sample>, Error> {
    // The final shdr record is the terminal "EOS" sentinel.
    let count = headers.len().saturating_sub(1);
    let mut samples = Vec::with_capacity(count);
    for header in &headers[..count] {
        let (data, loop_start, loop_end) = if header.sample_type & SAMPLE_TYPE_COMPRESSED != 0 {
            // `.sf3`: an independent Ogg/Vorbis stream delimited by byte
            // offsets; its loop points are already relative to the sample.
            let blob = slice(smpl, header.start as usize, header.end as usize)?;
            let data = decode_vorbis(blob)?;
            (data, header.start_loop, header.end_loop)
        } else {
            // `.sf2`: raw i16 PCM addressed by sample frame. A looping sample's
            // loop may extend past `end`, so include up to the loop end.
            let begin = header.start as usize * 2;
            let finish = header.end.max(header.end_loop) as usize * 2;
            let bytes = slice(smpl, begin, finish)?;
            let data = bytes.chunks_exact(2).map(|c| le_i16(c, 0)).collect();
            (
                data,
                header.start_loop.saturating_sub(header.start),
                header.end_loop.saturating_sub(header.start),
            )
        };

        samples.push(Sample {
            name: header.name.clone(),
            clip: Clip::new(data, header.sample_rate),
            loop_start,
            loop_end,
            original_key: header.original_key,
            correction: header.correction,
            link: header.link,
            // Mask off the compression flag to recover the stereo/ROM type.
            kind: SampleType::from(header.sample_type & !SAMPLE_TYPE_COMPRESSED),
        });
    }
    Ok(samples)
}

/// Decodes a single mono Ogg/Vorbis stream into 16-bit PCM.
fn decode_vorbis(blob: &[u8]) -> Result<Vec<i16>, Error> {
    let mut reader = lewton::inside_ogg::OggStreamReader::new(Cursor::new(blob))?;
    let mut out = Vec::new();
    while let Some(packet) = reader.read_dec_packet_itl()? {
        out.extend_from_slice(&packet);
    }
    Ok(out)
}

// ----- little-endian readers ------------------------------------------------

fn slice<T>(data: &[T], from: usize, to: usize) -> Result<&[T], Error> {
    data.get(from..to)
        .ok_or_else(|| format("record offset out of range"))
}

fn le_u16(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}

fn le_i16(d: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([d[at], d[at + 1]])
}

fn le_u32(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}
