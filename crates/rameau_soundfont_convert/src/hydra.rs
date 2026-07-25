//! Serialising the abstract model back into the `pdta` record arrays.
//!
//! The loader reads these arrays into presets, instruments and zones, dropping
//! the format's bookkeeping as it goes: the index chains that delimit each
//! record's range, and the terminal sentinel record that bounds the last real
//! one. This module puts both back.
//!
//! Every array is a flat sequence of fixed-size little-endian records. A
//! preset header stores the index of its first bag; the *next* header's index
//! bounds it. A bag stores the index of its first generator and first
//! modulator; the next bag's indices bound it. That is why each array needs a
//! terminal record: without one, the last real record would have no end bound.

use rameau_soundfont::{GeneratorAmount, Instrument, Preset, SampleType, SoundFont, Zone};

use crate::Error;

/// The `.sf3` flag marking a sample as Ogg/Vorbis-compressed.
const SAMPLE_TYPE_COMPRESSED: u16 = 0x10;

/// Where one sample ended up in the encoded pool, and its loop points.
///
/// In `.sf3` `start`/`end` are *byte* offsets of the sample's Vorbis stream
/// within the pool, while the loop points remain frame offsets relative to that
/// sample. This is the one place the two formats genuinely diverge.
pub(crate) struct SampleRegion {
    pub start: u32,
    pub end: u32,
    pub loop_start: u32,
    pub loop_end: u32,
}

/// The nine serialised `pdta` sub-chunks.
pub(crate) struct Pdta {
    pub phdr: Vec<u8>,
    pub pbag: Vec<u8>,
    pub pmod: Vec<u8>,
    pub pgen: Vec<u8>,
    pub inst: Vec<u8>,
    pub ibag: Vec<u8>,
    pub imod: Vec<u8>,
    pub igen: Vec<u8>,
    pub shdr: Vec<u8>,
}

/// Serialises `sf`'s presets, instruments and samples into the `pdta` arrays.
///
/// `regions` must be parallel to `sf.samples` — one entry per sample, giving
/// its byte range in the encoded pool.
pub(crate) fn build_pdta(sf: &SoundFont, regions: &[SampleRegion]) -> Result<Pdta, Error> {
    let (phdr, pbag, pmod, pgen) = build_presets(&sf.presets)?;
    let (inst, ibag, imod, igen) = build_instruments(&sf.instruments)?;

    Ok(Pdta {
        phdr,
        pbag,
        pmod,
        pgen,
        inst,
        ibag,
        imod,
        igen,
        shdr: build_shdr(sf, regions)?,
    })
}

/// Accumulates the bag, generator and modulator arrays that the preset and
/// instrument hierarchies build identically.
#[derive(Default)]
struct Zones {
    bags: Vec<u8>,
    gens: Vec<u8>,
    mods: Vec<u8>,
}

impl Zones {
    /// The bag index the next zone will occupy — what a header records.
    fn next_bag(&self) -> Result<u16, Error> {
        index(self.bags.len() / BAG_SIZE, "zones")
    }

    /// Appends one record's worth of zones, in file order.
    ///
    /// Order is preserved exactly as the loader read it, which keeps the
    /// specification's ordering rules intact: a global zone stays first, and
    /// within a zone the range generators stay leading while `Instrument` and
    /// `SampleID` stay trailing.
    fn push(&mut self, zones: &[Zone]) -> Result<(), Error> {
        for zone in zones {
            self.bags
                .extend_from_slice(&index(self.gens.len() / GEN_SIZE, "generators")?.to_le_bytes());
            self.bags
                .extend_from_slice(&index(self.mods.len() / MOD_SIZE, "modulators")?.to_le_bytes());

            for g in &zone.generators {
                self.gens.extend_from_slice(&(g.kind as u16).to_le_bytes());
                self.gens.extend_from_slice(&raw_amount(g.amount));
            }
            for m in &zone.modulators {
                self.mods.extend_from_slice(&m.source.to_le_bytes());
                self.mods
                    .extend_from_slice(&(m.destination as u16).to_le_bytes());
                self.mods.extend_from_slice(&m.amount.to_le_bytes());
                self.mods.extend_from_slice(&m.amount_source.to_le_bytes());
                self.mods.extend_from_slice(&m.transform.to_le_bytes());
            }
        }
        Ok(())
    }

    /// Appends the terminal bag, generator and modulator records that bound the
    /// last real zone, and yields the finished arrays.
    fn finish(mut self) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>), Error> {
        self.bags
            .extend_from_slice(&index(self.gens.len() / GEN_SIZE, "generators")?.to_le_bytes());
        self.bags
            .extend_from_slice(&index(self.mods.len() / MOD_SIZE, "modulators")?.to_le_bytes());
        self.gens.extend_from_slice(&[0u8; GEN_SIZE]);
        self.mods.extend_from_slice(&[0u8; MOD_SIZE]);
        Ok((self.bags, self.mods, self.gens))
    }
}

/// Builds `phdr` plus the preset-side bag, modulator and generator arrays.
fn build_presets(presets: &[Preset]) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>), Error> {
    let mut phdr = Vec::with_capacity((presets.len() + 1) * PHDR_SIZE);
    let mut zones = Zones::default();

    for preset in presets {
        phdr.extend_from_slice(&name20(&preset.name));
        phdr.extend_from_slice(&preset.program.to_le_bytes());
        phdr.extend_from_slice(&preset.bank.to_le_bytes());
        phdr.extend_from_slice(&zones.next_bag()?.to_le_bytes());
        phdr.extend_from_slice(&preset.library.to_le_bytes());
        phdr.extend_from_slice(&preset.genre.to_le_bytes());
        phdr.extend_from_slice(&preset.morphology.to_le_bytes());

        zones.push(&preset.zones)?;
    }

    // The terminal "EOP" record: only its bag index matters, bounding the zones
    // of the last real preset.
    phdr.extend_from_slice(&name20("EOP"));
    phdr.extend_from_slice(&0u16.to_le_bytes()); // program
    phdr.extend_from_slice(&0u16.to_le_bytes()); // bank
    phdr.extend_from_slice(&zones.next_bag()?.to_le_bytes());
    phdr.extend_from_slice(&[0u8; 12]); // library, genre, morphology

    let (pbag, pmod, pgen) = zones.finish()?;
    Ok((phdr, pbag, pmod, pgen))
}

/// Builds `inst` plus the instrument-side bag, modulator and generator arrays.
fn build_instruments(
    instruments: &[Instrument],
) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>), Error> {
    let mut inst = Vec::with_capacity((instruments.len() + 1) * INST_SIZE);
    let mut zones = Zones::default();

    for instrument in instruments {
        inst.extend_from_slice(&name20(&instrument.name));
        inst.extend_from_slice(&zones.next_bag()?.to_le_bytes());

        zones.push(&instrument.zones)?;
    }

    // The terminal "EOI" record.
    inst.extend_from_slice(&name20("EOI"));
    inst.extend_from_slice(&zones.next_bag()?.to_le_bytes());

    let (ibag, imod, igen) = zones.finish()?;
    Ok((inst, ibag, imod, igen))
}

/// Builds `shdr`, addressing the encoded pool the `.sf3` way.
fn build_shdr(sf: &SoundFont, regions: &[SampleRegion]) -> Result<Vec<u8>, Error> {
    if regions.len() != sf.samples.len() {
        return Err(Error::TooLarge("sample regions do not match samples"));
    }
    let mut shdr = Vec::with_capacity((sf.samples.len() + 1) * SHDR_SIZE);

    for (sample, region) in sf.samples.iter().zip(regions) {
        shdr.extend_from_slice(&name20(&sample.name));
        shdr.extend_from_slice(&region.start.to_le_bytes());
        shdr.extend_from_slice(&region.end.to_le_bytes());
        shdr.extend_from_slice(&region.loop_start.to_le_bytes());
        shdr.extend_from_slice(&region.loop_end.to_le_bytes());
        shdr.extend_from_slice(&sample.sample_rate.to_le_bytes());
        shdr.push(sample.original_key);
        shdr.push(sample.correction as u8);
        shdr.extend_from_slice(&sample.link.to_le_bytes());
        shdr.extend_from_slice(&(kind_bits(sample.kind) | SAMPLE_TYPE_COMPRESSED).to_le_bytes());
    }

    // The terminal "EOS" record.
    shdr.extend_from_slice(&name20("EOS"));
    shdr.extend_from_slice(&[0u8; SHDR_SIZE - 20]);

    Ok(shdr)
}

/// Encodes a generator amount back into its raw two bytes, per its variant.
fn raw_amount(amount: GeneratorAmount) -> [u8; 2] {
    match amount {
        GeneratorAmount::Short(v) => v.to_le_bytes(),
        GeneratorAmount::Word(v) => v.to_le_bytes(),
        GeneratorAmount::Range(r) => [r.low, r.high],
    }
}

fn kind_bits(kind: SampleType) -> u16 {
    match kind {
        SampleType::Mono => 1,
        SampleType::Right => 2,
        SampleType::Left => 4,
        SampleType::Linked => 8,
        SampleType::RomMono => 0x8001,
        SampleType::RomRight => 0x8002,
        SampleType::RomLeft => 0x8004,
        SampleType::RomLinked => 0x8008,
        SampleType::Other(bits) => bits,
    }
}

/// A fixed 20-byte name field: truncated if too long, always NUL-terminated,
/// zero-padded to the full width.
fn name20(name: &str) -> [u8; 20] {
    let mut out = [0u8; 20];
    // Truncate on a character boundary so multi-byte UTF-8 is never split.
    let mut end = name.len().min(19);
    while end > 0 && !name.is_char_boundary(end) {
        end -= 1;
    }
    out[..end].copy_from_slice(&name.as_bytes()[..end]);
    out
}

/// Narrows a record count to the 16-bit index the format uses.
fn index(count: usize, what: &'static str) -> Result<u16, Error> {
    u16::try_from(count).map_err(|_| Error::TooLarge(what))
}

// Record sizes, in bytes, as laid down by the SoundFont specification.
const PHDR_SIZE: usize = 38;
const INST_SIZE: usize = 22;
const BAG_SIZE: usize = 4;
const MOD_SIZE: usize = 10;
const GEN_SIZE: usize = 4;
const SHDR_SIZE: usize = 46;

#[cfg(test)]
mod tests {
    use super::*;
    use rameau_clip::Clip;
    use rameau_soundfont::{
        Generator, GeneratorAmount, GeneratorType, Instrument, Preset, Range, Sample, SampleType,
        SoundFont, Zone,
    };

    /// A bank with one preset (a global zone plus a real zone) and one
    /// instrument, referencing one sample.
    fn bank() -> SoundFont {
        SoundFont {
            info: Default::default(),
            presets: vec![Preset {
                name: "Piano".into(),
                program: 0,
                bank: 0,
                library: 0,
                genre: 0,
                morphology: 0,
                zones: vec![
                    Zone {
                        // global zone: no Instrument generator
                        generators: vec![Generator {
                            kind: GeneratorType::INITIAL_ATTENUATION,
                            amount: GeneratorAmount::Short(50),
                        }],
                        modulators: vec![],
                    },
                    Zone {
                        generators: vec![
                            Generator {
                                kind: GeneratorType::KEY_RANGE,
                                amount: GeneratorAmount::Range(Range { low: 0, high: 127 }),
                            },
                            Generator {
                                kind: GeneratorType::INSTRUMENT,
                                amount: GeneratorAmount::Word(0),
                            },
                        ],
                        modulators: vec![],
                    },
                ],
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
                clip: Clip::new(vec![0i16; 100], 44_100),
                sample_rate: 44_100,
                frame_count: 100,
                loop_start: 10,
                loop_end: 90,
                original_key: 69,
                correction: 0,
                link: 0,
                kind: SampleType::Mono,
            }],
        }
    }

    fn regions() -> Vec<SampleRegion> {
        vec![SampleRegion {
            start: 0,
            end: 500,
            loop_start: 10,
            loop_end: 90,
        }]
    }

    #[test]
    fn appends_terminal_sentinel_records() {
        let p = build_pdta(&bank(), &regions()).unwrap();
        // One preset + EOP, one instrument + EOI, one sample + EOS.
        assert_eq!(p.phdr.len(), 2 * PHDR_SIZE);
        assert_eq!(p.inst.len(), 2 * INST_SIZE);
        assert_eq!(p.shdr.len(), 2 * SHDR_SIZE);
        assert_eq!(&p.phdr[PHDR_SIZE..PHDR_SIZE + 3], b"EOP");
        assert_eq!(&p.inst[INST_SIZE..INST_SIZE + 3], b"EOI");
        assert_eq!(&p.shdr[SHDR_SIZE..SHDR_SIZE + 3], b"EOS");
    }

    #[test]
    fn bag_chains_are_monotonic_and_terminated() {
        let p = build_pdta(&bank(), &regions()).unwrap();
        // Two preset zones + terminal bag.
        assert_eq!(p.pbag.len(), 3 * BAG_SIZE);
        // One instrument zone + terminal bag.
        assert_eq!(p.ibag.len(), 2 * BAG_SIZE);

        let gen_ndx = |bag: &[u8], i: usize| u16::from_le_bytes([bag[i * 4], bag[i * 4 + 1]]);
        assert_eq!(gen_ndx(&p.pbag, 0), 0); // global zone starts at 0
        assert_eq!(gen_ndx(&p.pbag, 1), 1); // after the global zone's 1 generator
        assert_eq!(gen_ndx(&p.pbag, 2), 3); // after the second zone's 2 generators
    }

    #[test]
    fn generator_amounts_round_trip_by_variant() {
        let p = build_pdta(&bank(), &regions()).unwrap();
        // pgen: [attenuation=50] [keyrange 0..127] [instrument 0] + terminal.
        assert_eq!(p.pgen.len(), 4 * GEN_SIZE);

        let oper = |i: usize| u16::from_le_bytes([p.pgen[i * 4], p.pgen[i * 4 + 1]]);
        let amount = |i: usize| [p.pgen[i * 4 + 2], p.pgen[i * 4 + 3]];

        assert_eq!(oper(0), GeneratorType::INITIAL_ATTENUATION as u16);
        assert_eq!(i16::from_le_bytes(amount(0)), 50);

        assert_eq!(oper(1), GeneratorType::KEY_RANGE as u16);
        assert_eq!(amount(1), [0, 127]); // range: two raw bytes, not an i16

        assert_eq!(oper(2), GeneratorType::INSTRUMENT as u16);
        assert_eq!(u16::from_le_bytes(amount(2)), 0);
    }

    /// `.sf3` sample headers address the Vorbis pool by byte, while loop points
    /// stay frame offsets relative to the sample, and the compressed bit is set.
    #[test]
    fn sample_headers_use_sf3_addressing() {
        let p = build_pdta(&bank(), &regions()).unwrap();
        let g = |at: usize| u32::from_le_bytes(p.shdr[at..at + 4].try_into().unwrap());

        assert_eq!(g(20), 0); // start: byte offset
        assert_eq!(g(24), 500); // end: byte offset
        assert_eq!(g(28), 10); // startloop: frames, sample-relative
        assert_eq!(g(32), 90); // endloop: frames, sample-relative
        assert_eq!(g(36), 44_100); // sample rate
        assert_eq!(p.shdr[40], 69); // original key

        let kind = u16::from_le_bytes([p.shdr[44], p.shdr[45]]);
        assert_eq!(kind, 1 | 0x10, "expected mono with the compressed bit set");
    }

    #[test]
    fn names_are_truncated_to_twenty_bytes_and_nul_terminated() {
        let mut sf = bank();
        sf.presets[0].name = "A very long preset name indeed".into();
        let p = build_pdta(&sf, &regions()).unwrap();
        assert_eq!(&p.phdr[..19], b"A very long preset ");
        assert_eq!(p.phdr[19], 0, "the 20th byte must be the NUL terminator");
    }

    /// A multi-byte character straddling the 19-byte cut must not be split into
    /// invalid UTF-8.
    #[test]
    fn truncation_respects_character_boundaries() {
        let mut sf = bank();
        // 18 ASCII bytes, then a 2-byte character crossing the boundary.
        sf.presets[0].name = format!("{}é", "x".repeat(18));
        let p = build_pdta(&sf, &regions()).unwrap();
        let name = &p.phdr[..20];
        let end = name.iter().position(|&b| b == 0).unwrap();
        std::str::from_utf8(&name[..end]).expect("truncated name must stay valid UTF-8");
        assert_eq!(end, 18, "the split character should be dropped entirely");
    }

    /// The reserved preset fields are part of the model and must survive.
    #[test]
    fn preserves_reserved_preset_fields() {
        let mut sf = bank();
        sf.presets[0].library = 0x1111_2222;
        sf.presets[0].genre = 0x3333_4444;
        sf.presets[0].morphology = 0x5555_6666;
        let p = build_pdta(&sf, &regions()).unwrap();

        let g = |at: usize| u32::from_le_bytes(p.phdr[at..at + 4].try_into().unwrap());
        assert_eq!(g(26), 0x1111_2222);
        assert_eq!(g(30), 0x3333_4444);
        assert_eq!(g(34), 0x5555_6666);
    }
}
