//! Assembling the `.sf3` RIFF container.

use std::io::Write;
use std::path::Path;

use rameau_clip::AudioClip;
use rameau_soundfont::{Info, SoundFont};

use crate::Error;
use crate::encode::{Quality, encode_sample};
use crate::hydra::{SampleRegion, build_pdta};

/// Stamped into the `ISFT` field of every bank this crate writes.
const SOFTWARE: &str = "rameau_soundfont_convert";

/// The `ifil` version that marks a bank as `.sf3`.
const SF3_VERSION: (u16, u16) = (3, 0);

/// Loads `input` (`.sf2` or `.sf3`) and writes it to `output` as `.sf3`.
///
/// # Errors
///
/// Returns [`Error::Load`] if `input` cannot be read or parsed, and otherwise
/// as [`save_sf3`].
pub fn convert_file(
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
    quality: Quality,
) -> Result<(), Error> {
    let sf = SoundFont::load_file(input)?;
    save_sf3(&sf, quality, output)
}

/// Writes `sf` as a `.sf3` file at `path`.
///
/// # Errors
///
/// Returns [`Error::Io`] if `path` cannot be created or written, and otherwise
/// as [`write_sf3`].
pub fn save_sf3(sf: &SoundFont, quality: Quality, path: impl AsRef<Path>) -> Result<(), Error> {
    let file = std::fs::File::create(path)?;
    write_sf3(sf, quality, std::io::BufWriter::new(file))
}

/// Writes `sf` as a `.sf3` image into `out`.
///
/// Each sample is encoded as an independent Ogg/Vorbis stream, so unlike `.sf2`
/// the sample pool needs no padding between samples and the sample headers
/// address it by byte rather than by frame.
///
/// # Errors
///
/// Returns [`Error::Encode`] if a sample could not be encoded to Ogg/Vorbis,
/// [`Error::TooLarge`] if the bank exceeds what the format can address, and
/// [`Error::Io`] from writing to `out`.
pub fn write_sf3<W: Write>(sf: &SoundFont, quality: Quality, mut out: W) -> Result<(), Error> {
    let (smpl, regions) = build_sample_pool(sf, quality)?;
    let pdta = build_pdta(sf, &regions)?;

    let info = build_info(&sf.info);
    let sdta = list(b"sdta", &[(b"smpl", &smpl)]);
    let pdta = list(
        b"pdta",
        &[
            (b"phdr", &pdta.phdr),
            (b"pbag", &pdta.pbag),
            (b"pmod", &pdta.pmod),
            (b"pgen", &pdta.pgen),
            (b"inst", &pdta.inst),
            (b"ibag", &pdta.ibag),
            (b"imod", &pdta.imod),
            (b"igen", &pdta.igen),
            (b"shdr", &pdta.shdr),
        ],
    );

    // The RIFF size covers the form type and every list that follows it.
    let body_len = 4 + info.len() + sdta.len() + pdta.len();
    let body_len = u32::try_from(body_len).map_err(|_| Error::TooLarge("total file size"))?;

    out.write_all(b"RIFF")?;
    out.write_all(&body_len.to_le_bytes())?;
    out.write_all(b"sfbk")?;
    out.write_all(&info)?;
    out.write_all(&sdta)?;
    out.write_all(&pdta)?;
    out.flush()?;
    Ok(())
}

/// Encodes every sample and concatenates the streams, recording where each one
/// landed.
fn build_sample_pool(
    sf: &SoundFont,
    quality: Quality,
) -> Result<(Vec<u8>, Vec<SampleRegion>), Error> {
    let mut pool = Vec::new();
    let mut regions = Vec::with_capacity(sf.samples.len());

    for sample in &sf.samples {
        let start = u32::try_from(pool.len()).map_err(|_| Error::TooLarge("sample pool"))?;
        pool.extend_from_slice(&encode_sample(
            sample.clip.data(),
            sample.sample_rate,
            quality,
        )?);
        let end = u32::try_from(pool.len()).map_err(|_| Error::TooLarge("sample pool"))?;

        regions.push(SampleRegion {
            start,
            end,
            loop_start: sample.loop_start,
            loop_end: sample.loop_end,
        });
    }

    Ok((pool, regions))
}

/// Builds the `INFO` list.
///
/// `ifil` is forced to 3.0 — that is what marks the file as `.sf3` — and `ISFT`
/// records this crate. `isng` and `INAM` are required by the specification, so
/// they fall back to conventional defaults when the model has none.
fn build_info(info: &Info) -> Vec<u8> {
    let mut chunks: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"ifil", version_bytes(SF3_VERSION.0, SF3_VERSION.1)),
        (b"isng", zstr(info.engine.as_deref().unwrap_or("EMU8000"))),
        (b"INAM", zstr(info.name.as_deref().unwrap_or("Untitled"))),
    ];

    if let Some(v) = &info.rom_name {
        chunks.push((b"irom", zstr(v)));
    }
    if let Some(v) = info.rom_version {
        chunks.push((b"iver", version_bytes(v.major, v.minor)));
    }
    if let Some(v) = &info.creation_date {
        chunks.push((b"ICRD", zstr(v)));
    }
    if let Some(v) = &info.engineers {
        chunks.push((b"IENG", zstr(v)));
    }
    if let Some(v) = &info.product {
        chunks.push((b"IPRD", zstr(v)));
    }
    if let Some(v) = &info.copyright {
        chunks.push((b"ICOP", zstr(v)));
    }
    if let Some(v) = &info.comments {
        chunks.push((b"ICMT", zstr(v)));
    }
    chunks.push((b"ISFT", zstr(SOFTWARE)));

    let refs: Vec<(&[u8; 4], &Vec<u8>)> = chunks.iter().map(|(id, data)| (*id, data)).collect();
    list(b"INFO", &refs)
}

fn version_bytes(major: u16, minor: u16) -> Vec<u8> {
    let mut v = Vec::with_capacity(4);
    v.extend_from_slice(&major.to_le_bytes());
    v.extend_from_slice(&minor.to_le_bytes());
    v
}

/// A NUL-terminated string, padded to an even length as the format requires.
fn zstr(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    if !v.len().is_multiple_of(2) {
        v.push(0);
    }
    v
}

/// Wraps `chunks` in a `LIST` of the given type.
fn list(kind: &[u8; 4], chunks: &[(&[u8; 4], &Vec<u8>)]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(kind);
    for (id, data) in chunks {
        body.extend_from_slice(*id);
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
        // Chunks are word-aligned. The pad byte is not counted in the size.
        if !data.len().is_multiple_of(2) {
            body.push(0);
        }
    }

    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(b"LIST");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rameau_clip::Clip;
    use rameau_soundfont::{
        Generator, GeneratorAmount, GeneratorType, Info, Instrument, Preset, Sample, SampleType,
        SoundFont, Zone,
    };

    fn bank() -> SoundFont {
        SoundFont {
            info: Info {
                name: Some("Test Bank".into()),
                ..Default::default()
            },
            presets: vec![Preset {
                name: "Piano".into(),
                program: 4,
                bank: 0,
                library: 0,
                genre: 0,
                morphology: 0,
                zones: vec![Zone {
                    generators: vec![Generator {
                        kind: GeneratorType::INSTRUMENT,
                        amount: GeneratorAmount::Word(0),
                    }],
                    modulators: vec![],
                }],
            }],
            instruments: vec![Instrument {
                name: "PianoInst".into(),
                zones: vec![Zone {
                    generators: vec![Generator {
                        kind: GeneratorType::SAMPLE_ID,
                        amount: GeneratorAmount::Word(0),
                    }],
                    modulators: vec![],
                }],
            }],
            samples: vec![Sample {
                name: "A4".into(),
                clip: Clip::new(
                    (0..2000)
                        .map(|i| ((i as f32 * 0.05).sin() * 12000.0) as i16)
                        .collect(),
                    44_100,
                ),
                sample_rate: 44_100,
                frame_count: 2000,
                loop_start: 100,
                loop_end: 1900,
                original_key: 69,
                correction: -3,
                link: 0,
                kind: SampleType::Mono,
            }],
        }
    }

    fn to_bytes(sf: &SoundFont) -> Vec<u8> {
        let mut out = Vec::new();
        write_sf3(sf, Quality::default(), &mut out).unwrap();
        out
    }

    #[test]
    fn writes_a_well_formed_riff_container() {
        let bytes = to_bytes(&bank());
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"sfbk");
        let declared = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert_eq!(
            declared + 8,
            bytes.len(),
            "the declared RIFF size must cover everything after the size field"
        );
    }

    /// The whole point: the result must load back through the existing loader.
    #[test]
    fn round_trips_through_the_loader() {
        let original = bank();
        let reloaded = SoundFont::from_bytes(&to_bytes(&original)).expect("reload written sf3");

        assert_eq!(reloaded.info.version.major, 3, "expected an sf3 ifil");
        assert_eq!(reloaded.info.name.as_deref(), Some("Test Bank"));
        assert_eq!(reloaded.presets.len(), 1);
        assert_eq!(reloaded.presets[0].name, "Piano");
        assert_eq!(reloaded.presets[0].program, 4);
        assert_eq!(reloaded.instruments.len(), 1);
        assert_eq!(reloaded.instruments[0].name, "PianoInst");
        assert_eq!(reloaded.samples.len(), 1);

        let (a, b) = (&original.samples[0], &reloaded.samples[0]);
        assert_eq!(b.name, a.name);
        assert_eq!(b.sample_rate, a.sample_rate);
        assert_eq!(b.original_key, a.original_key);
        assert_eq!(b.correction, a.correction);
        assert_eq!(b.kind, a.kind);
        assert_eq!(b.loop_start, a.loop_start);
        assert_eq!(b.loop_end, a.loop_end);
        assert_eq!(
            b.clip.data.len(),
            a.clip.data.len(),
            "frame count must survive encoding, or the loop points move"
        );
    }

    #[test]
    fn preserves_zone_structure() {
        let reloaded = SoundFont::from_bytes(&to_bytes(&bank())).unwrap();
        assert_eq!(reloaded.presets[0].zones.len(), 1);
        assert_eq!(reloaded.presets[0].zones[0].generators.len(), 1);
        assert_eq!(
            reloaded.presets[0].zones[0].generators[0].kind,
            GeneratorType::INSTRUMENT
        );
        assert_eq!(reloaded.instruments[0].zones[0].generators.len(), 1);
    }

    /// An odd-length string chunk needs a pad byte that is not counted in the
    /// declared chunk size; getting this wrong desynchronises every later chunk.
    #[test]
    fn handles_odd_length_info_strings() {
        let mut sf = bank();
        sf.info.name = Some("odd".into()); // 3 chars + NUL = 4, even
        assert!(SoundFont::from_bytes(&to_bytes(&sf)).is_ok());

        sf.info.name = Some("even".into()); // 4 chars + NUL = 5, odd -> needs padding
        let reloaded = SoundFont::from_bytes(&to_bytes(&sf)).expect("odd-length chunk");
        assert_eq!(reloaded.info.name.as_deref(), Some("even"));
    }

    #[test]
    fn stamps_the_software_field() {
        let reloaded = SoundFont::from_bytes(&to_bytes(&bank())).unwrap();
        assert_eq!(reloaded.info.software.as_deref(), Some(SOFTWARE));
    }

    /// Optional `INFO` fields must survive, and be omitted entirely when unset
    /// rather than written as empty chunks.
    #[test]
    fn carries_optional_info_fields() {
        let mut sf = bank();
        sf.info.copyright = Some("Public domain".into());
        sf.info.comments = Some("A test bank".into());
        sf.info.engineers = Some("Nobody".into());

        let reloaded = SoundFont::from_bytes(&to_bytes(&sf)).unwrap();
        assert_eq!(reloaded.info.copyright.as_deref(), Some("Public domain"));
        assert_eq!(reloaded.info.comments.as_deref(), Some("A test bank"));
        assert_eq!(reloaded.info.engineers.as_deref(), Some("Nobody"));
        assert_eq!(reloaded.info.product, None);
    }

    /// Several samples must land at distinct, non-overlapping byte ranges in the
    /// pool, and come back in the same order.
    #[test]
    fn keeps_multiple_samples_distinct() {
        let mut sf = bank();
        let base = sf.samples[0].clone();
        for (i, name) in ["B4", "C5"].iter().enumerate() {
            let mut s = base.clone();
            s.name = (*name).into();
            // Give each a distinct length and pitch so a mix-up is visible.
            let n = 3000 + i * 500;
            s.clip = Clip::new(
                (0..n)
                    .map(|k| ((k as f32 * (0.05 + i as f32 * 0.02)).sin() * 12000.0) as i16)
                    .collect(),
                44_100,
            );
            s.frame_count = n as u32;
            s.loop_start = 10;
            s.loop_end = n as u32 - 10;
            s.original_key = 71 + i as u8;
            sf.samples.push(s);
        }

        let reloaded = SoundFont::from_bytes(&to_bytes(&sf)).unwrap();
        assert_eq!(reloaded.samples.len(), 3);
        for (a, b) in sf.samples.iter().zip(&reloaded.samples) {
            assert_eq!(b.name, a.name);
            assert_eq!(b.original_key, a.original_key);
            assert_eq!(b.clip.data.len(), a.clip.data.len(), "sample '{}'", a.name);
        }
    }

    /// A bank with no samples at all must still produce a loadable file.
    #[test]
    fn writes_a_bank_with_no_samples() {
        let sf = SoundFont {
            info: Info::default(),
            presets: vec![],
            instruments: vec![],
            samples: vec![],
        };
        let reloaded = SoundFont::from_bytes(&to_bytes(&sf)).expect("empty bank should load");
        assert!(reloaded.presets.is_empty());
        assert!(reloaded.samples.is_empty());
    }
}
