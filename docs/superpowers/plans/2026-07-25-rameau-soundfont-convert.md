# `rameau_soundfont_convert` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `rameau_soundfont_convert` crate that converts an uncompressed `.sf2` bank into an Ogg/Vorbis-compressed `.sf3` bank, verified against `assets/Unison.SF2`.

**Architecture:** Conversion goes through the existing abstract model — `SoundFont::load_file(sf2)` produces a `SoundFont<Clip<i16>>`, and a new writer serialises that back out as `.sf3`. The writer is three independent, separately testable layers: sample encoding (PCM → Vorbis), hydra serialisation (model → `pdta` record arrays), and RIFF assembly.

**Tech Stack:** Rust 2024 edition, `vorbis_rs` 0.5 (aoTuV/Lancer libvorbis via `cc`), existing `rameau_soundfont` / `rameau_clip` crates, `lewton` 0.10 for decode in tests.

**Spec:** [`docs/superpowers/specs/2026-07-25-sf2-to-sf3-conversion-design.md`](../specs/2026-07-25-sf2-to-sf3-conversion-design.md)

## Global Constraints

- Rust edition **2024**; all crate manifest fields use `version.workspace = true` style inheritance, matching every existing member.
- License **AGPL-3.0-or-later**, inherited via `license.workspace = true`.
- New crate lives at `crates/rameau_soundfont_convert/` and is picked up by the existing `members = ["crates/*"]` glob — do **not** edit the members list.
- `[profile.*]` tables go **only** in the workspace root `d:/source/rameau/Cargo.toml`. Cargo ignores them in member manifests. Never add a `[profile]` table to a member crate.
- Ogg/Vorbis VBR quality range is **-0.2 ..= 1.0** (libvorbis); default **0.5**.
- The `.sf3` compressed-sample flag is `0x10` in `sfSampleType`.
- `.sf3` `ifil` version is **3.0**.
- Every SoundFont record array carries a **terminal sentinel record** that the loader strips on read and the writer must regenerate.
- All RIFF chunks are padded to an even byte length; the pad byte is **not** counted in the chunk's declared size.
- No `unsafe` in the new crate.

---

### Task 1: Fix the Vorbis over-read in `rameau_soundfont`

The loader accumulates decoded Vorbis packets without truncating to the stream's
final granule position, so 318 of 1037 samples in `FluidR3Mono_GM.sf3` decode up
to 1020 frames too long. This must be fixed first: Task 7 asserts exact
frame-count equality across the conversion round-trip, which is the property
that proves loop points survive, and it cannot hold while the decoder over-reads.

**Files:**
- Modify: `crates/rameau_soundfont/src/load.rs:632-640` (`decode_vorbis`)
- Test: `crates/rameau_soundfont/tests/load.rs` (append)

**Interfaces:**
- Consumes: nothing (first task).
- Produces: a corrected `decode_vorbis` — private, but every later task depends on `SoundFont::load_file` returning exact sample lengths for `.sf3` input.

- [ ] **Step 1: Write the failing test**

Append to `crates/rameau_soundfont/tests/load.rs`:

```rust
/// Ogg/Vorbis codes audio in blocks, so the final block overshoots the real end
/// of the signal. The true length is carried in the last page's granule
/// position. A decoder that simply concatenates packets returns samples that
/// are too long — up to a full block of trailing audio past where the sample
/// should stop.
///
/// `.sf3` stores each sample's `loop_end` relative to that sample, so a correct
/// decode leaves every loop point inside the audio while an over-read inflates
/// the gap between `loop_end` and the end of the data. Real banks loop close to
/// the end of a sample, so an over-read shows up as an implausibly large tail.
#[test]
fn sf3_samples_decode_to_their_true_length() {
    let sf = SoundFont::load_file(asset("FluidR3Mono_GM.sf3")).expect("load sf3");

    // A looping sample's data should end at or very shortly after `loop_end`.
    // One Vorbis long block is 1024 frames; allow a generous margin over that
    // for banks that genuinely keep a short release tail, but not the multiple
    // blocks an over-read produces.
    const MAX_TAIL: usize = 2048;

    let mut worst = 0usize;
    let mut worst_name = String::new();
    for sample in &sf.samples {
        if sample.loop_end == 0 || sample.loop_end <= sample.loop_start {
            continue; // not a looping sample; nothing to anchor against
        }
        let tail = sample.clip.data.len().saturating_sub(sample.loop_end as usize);
        if tail > worst {
            worst = tail;
            worst_name = sample.name.clone();
        }
    }

    assert!(
        worst <= MAX_TAIL,
        "sample '{worst_name}' decodes {worst} frames past its loop end; \
         decoded audio is not being truncated to the stream's granule position"
    );
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:
```
cargo test -p rameau_soundfont --test load sf3_samples_decode_to_their_true_length
```
Expected: **FAIL**, reporting a tail well above 2048 frames.

- [ ] **Step 3: Fix `decode_vorbis`**

In `crates/rameau_soundfont/src/load.rs`, replace the body of `decode_vorbis`:

```rust
/// Decodes a single mono Ogg/Vorbis stream into 16-bit PCM.
///
/// Vorbis codes audio in blocks, so decoding the last packet yields more frames
/// than the stream actually contains. The real length is the granule position
/// of the final page, so the accumulated data is truncated to it; without this
/// every sample would carry up to a block of spurious trailing audio.
fn decode_vorbis(blob: &[u8]) -> Result<Vec<i16>, Error> {
    let mut reader = lewton::inside_ogg::OggStreamReader::new(Cursor::new(blob))?;
    let mut out = Vec::new();
    let mut frames = 0u64;
    while let Some(packet) = reader.read_dec_packet_itl()? {
        out.extend_from_slice(&packet);
        if let Some(absgp) = reader.get_last_absgp() {
            frames = absgp;
        }
    }
    // A stream with no granule position (nothing decoded) leaves `frames` at 0
    // and correctly truncates to empty.
    out.truncate(frames as usize);
    Ok(out)
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run:
```
cargo test -p rameau_soundfont --test load
```
Expected: **PASS** — all four tests, including the pre-existing `loads_sf3` and `both_formats_yield_decoded_pcm`.

- [ ] **Step 5: Commit**

```bash
git add crates/rameau_soundfont/src/load.rs crates/rameau_soundfont/tests/load.rs
git commit -m "fix(soundfont): truncate decoded Vorbis samples to their granule position"
```

---

### Task 2: Scaffold the crate and the workspace profile overrides

**Files:**
- Create: `crates/rameau_soundfont_convert/Cargo.toml`
- Create: `crates/rameau_soundfont_convert/src/lib.rs`
- Modify: `Cargo.toml` (workspace root — append profile overrides)

**Interfaces:**
- Consumes: nothing.
- Produces: the crate `rameau_soundfont_convert`, compiling and empty; the `Error` enum used by every later task.

- [ ] **Step 1: Confirm the exact codec package names**

Run:
```
cargo add vorbis_rs --package rameau_soundfont_convert --dry-run
```
(or after Step 2, inspect `Cargo.lock`). The expected transitive C packages are
`vorbis_rs`, `aotuv_lancer_vorbis_sys`, `ogg_next_sys`. Verify with:
```
grep -E '^name = "(vorbis_rs|aotuv_lancer_vorbis_sys|ogg_next_sys|lewton)"' Cargo.lock
```
Use the names as they actually appear; a `[profile.dev.package.X]` for a package
that is not in the graph is a hard error.

- [ ] **Step 2: Create the manifest**

`crates/rameau_soundfont_convert/Cargo.toml`:

```toml
[package]
name = "rameau_soundfont_convert"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
homepage.workspace = true
description = "Convert SoundFont banks from .sf2 to Ogg/Vorbis-compressed .sf3"

[dependencies]
rameau_clip = { path = "../rameau_clip", version = "0.1.0" }
rameau_soundfont = { path = "../rameau_soundfont", version = "0.1.0" }
vorbis_rs = "0.5.5"

[[bin]]
name = "sf2-to-sf3"
path = "src/main.rs"
```

- [ ] **Step 3: Add the workspace-wide profile overrides**

Append to the **workspace root** `d:/source/rameau/Cargo.toml`. These must live
here and nowhere else: Cargo only honours `[profile.*]` in a workspace's root
manifest, and putting them here makes them apply to every member, however the
build was invoked.

```toml
# The Ogg/Vorbis codecs are C libraries compiled through `cc`, and are unusably
# slow at the debug default of `opt-level = 0` — converting a 655-sample bank
# goes from seconds to many minutes, which makes the conversion tests look like
# they have hung. `cc` reads Cargo's per-package `OPT_LEVEL`, so these raise the
# optimisation level of the bundled C sources too, not just the Rust wrappers.
#
# `profile.test` and `profile.bench` inherit from `profile.dev`, so test builds
# are covered without repeating this. `profile.release` is already `opt-level = 3`.
[profile.dev.package.vorbis_rs]
opt-level = 3

[profile.dev.package.aotuv_lancer_vorbis_sys]
opt-level = 3

[profile.dev.package.ogg_next_sys]
opt-level = 3

# The decoder side: `rameau_soundfont` decodes .sf3 banks with lewton, and the
# conversion tests decode a whole bank back to verify the round-trip.
[profile.dev.package.lewton]
opt-level = 3
```

- [ ] **Step 4: Create the library root with the error type**

`crates/rameau_soundfont_convert/src/lib.rs`:

```rust
//! Conversion of SoundFont banks to the Ogg/Vorbis-compressed `.sf3` format.
//!
//! [`rameau_soundfont`] reads both `.sf2` and `.sf3` into the same abstract
//! [`SoundFont`] model; this crate writes that model back out as `.sf3`, in
//! which each sample is stored as an independent Ogg/Vorbis stream rather than
//! as raw PCM in one shared pool. For a typical bank that is a severalfold size
//! reduction, at the cost of lossy audio.
//!
//! ```no_run
//! use rameau_soundfont_convert::{convert_file, Quality};
//!
//! convert_file("bank.sf2", "bank.sf3", Quality::default())?;
//! # Ok::<(), rameau_soundfont_convert::Error>(())
//! ```
//!
//! # Round-trip fidelity
//!
//! Conversion goes through the abstract model, so anything the loader does not
//! model is not carried across. In practice that is two things, both of which
//! are absent from every bank tested: generators whose operator is outside the
//! known `SFGenerator` enumeration, and modulators whose destination is a link
//! to another modulator rather than a generator. Banks using either would lose
//! those records. Everything else — presets, instruments, zones, generators,
//! modulators, sample metadata and loop points — survives intact.

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
```

Create empty placeholder modules so the crate compiles — `src/encode.rs`,
`src/hydra.rs`, `src/write.rs` are filled in by Tasks 3-5. For this task only,
stub them:

```rust
// src/encode.rs
/// Ogg/Vorbis VBR quality.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quality(f32);

impl Default for Quality {
    fn default() -> Self {
        Quality(0.5)
    }
}
```

```rust
// src/hydra.rs  (empty for now)
```

```rust
// src/write.rs
use crate::Error;
use rameau_soundfont::SoundFont;
use std::io::Write;
use std::path::Path;

pub fn write_sf3<W: Write>(_sf: &SoundFont, _q: crate::Quality, _out: W) -> Result<(), Error> {
    todo!()
}

pub fn save_sf3(_sf: &SoundFont, _q: crate::Quality, _path: impl AsRef<Path>) -> Result<(), Error> {
    todo!()
}

pub fn convert_file(
    _input: impl AsRef<Path>,
    _output: impl AsRef<Path>,
    _q: crate::Quality,
) -> Result<(), Error> {
    todo!()
}
```

Also create a minimal `src/main.rs` so the declared `[[bin]]` resolves; Task 6
replaces it:

```rust
fn main() {}
```

- [ ] **Step 5: Verify it builds**

Run:
```
cargo build -p rameau_soundfont_convert
```
Expected: **success**. Warnings about unused `todo!()` parameters are fine.

Confirm the profile overrides were accepted with no "unused manifest key" or
"profile package spec did not match" warnings.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/rameau_soundfont_convert
git commit -m "feat(convert): scaffold rameau_soundfont_convert; always optimise codec crates"
```

---

### Task 3: Sample encoding (`encode.rs`)

**Files:**
- Modify: `crates/rameau_soundfont_convert/src/encode.rs` (replace the Task 2 stub)

**Interfaces:**
- Consumes: `crate::Error`.
- Produces:
  - `pub struct Quality(f32)`, `Quality::new(f32) -> Quality` (clamping), `Quality::default() -> Quality` (0.5), `Quality::get(&self) -> f32`
  - `pub(crate) fn encode_sample(pcm: &[i16], sample_rate: u32, quality: Quality) -> Result<Vec<u8>, Error>`

- [ ] **Step 1: Write the failing tests**

Replace `crates/rameau_soundfont_convert/src/encode.rs` test module (append at
the bottom of the file):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Decodes a mono Ogg/Vorbis stream back to PCM, truncating to the granule
    /// position exactly as `rameau_soundfont`'s loader does.
    fn decode(ogg: &[u8]) -> Vec<i16> {
        let mut reader =
            lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(ogg)).unwrap();
        let mut out = Vec::new();
        let mut frames = 0u64;
        while let Some(packet) = reader.read_dec_packet_itl().unwrap() {
            out.extend_from_slice(&packet);
            if let Some(absgp) = reader.get_last_absgp() {
                frames = absgp;
            }
        }
        out.truncate(frames as usize);
        out
    }

    fn tone(n: usize) -> Vec<i16> {
        (0..n)
            .map(|i| ((i as f32 * 0.05).sin() * 12000.0) as i16)
            .collect()
    }

    /// The whole design rests on Vorbis preserving the exact frame count: loop
    /// points are frame offsets, so a length change would silently move them.
    /// The awkward cases are lengths straddling the 1024-frame block size.
    #[test]
    fn preserves_frame_count_exactly() {
        for &n in &[1usize, 7, 100, 1023, 1024, 1025, 4096, 44_100] {
            for &rate in &[8000u32, 22_050, 44_100] {
                let pcm = tone(n);
                let ogg = encode_sample(&pcm, rate, Quality::default()).unwrap();
                assert_eq!(
                    decode(&ogg).len(),
                    n,
                    "frame count changed for n={n} rate={rate}"
                );
            }
        }
    }

    #[test]
    fn round_trip_is_audibly_close() {
        let pcm = tone(44_100);
        let ogg = encode_sample(&pcm, 44_100, Quality::default()).unwrap();
        let back = decode(&ogg);
        let err: f64 = pcm
            .iter()
            .zip(&back)
            .map(|(&a, &b)| {
                let d = (a as f64 - b as f64) / 32768.0;
                d * d
            })
            .sum::<f64>()
            / pcm.len() as f64;
        assert!(err.sqrt() < 0.05, "rms error too high: {}", err.sqrt());
    }

    /// A zero-length sample must still produce a decodable stream, so that every
    /// `shdr` record points at something valid.
    #[test]
    fn encodes_empty_sample() {
        let ogg = encode_sample(&[], 44_100, Quality::default()).unwrap();
        assert!(!ogg.is_empty(), "expected at least the Vorbis headers");
        assert_eq!(decode(&ogg).len(), 0);
    }

    #[test]
    fn rejects_zero_sample_rate() {
        assert!(matches!(
            encode_sample(&[0, 1, 2], 0, Quality::default()),
            Err(Error::TooLarge(_))
        ));
    }

    #[test]
    fn quality_is_clamped_to_the_libvorbis_range() {
        assert_eq!(Quality::new(5.0).get(), 1.0);
        assert_eq!(Quality::new(-9.0).get(), -0.2);
        assert_eq!(Quality::default().get(), 0.5);
    }
}
```

Add the dev-dependency needed by these tests to
`crates/rameau_soundfont_convert/Cargo.toml`:

```toml
[dev-dependencies]
lewton = "0.10.2"
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:
```
cargo test -p rameau_soundfont_convert --lib
```
Expected: **compile error** — `encode_sample` not found.

- [ ] **Step 3: Implement `encode.rs`**

Replace the whole of `crates/rameau_soundfont_convert/src/encode.rs`:

```rust
//! Encoding sample audio to the Ogg/Vorbis streams a `.sf3` bank stores.

use std::num::{NonZeroU32, NonZeroU8};

use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};

use crate::Error;

/// libvorbis accepts VBR quality factors in this range.
const MIN_QUALITY: f32 = -0.2;
const MAX_QUALITY: f32 = 1.0;

/// How many frames are handed to the encoder at a time.
///
/// libvorbis slows down dramatically when given very large blocks, and the
/// upstream documentation suggests around 1024 frames; the largest Vorbis
/// analysis window is 8192.
const BLOCK_FRAMES: usize = 1024;

/// Ogg/Vorbis VBR quality: `-0.2` (smallest) to `1.0` (best).
///
/// Higher values produce larger files. The default, `0.5`, is a reasonable
/// balance for instrument samples.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quality(f32);

impl Quality {
    /// Creates a quality factor, clamping to the range libvorbis accepts.
    pub fn new(quality: f32) -> Self {
        Quality(quality.clamp(MIN_QUALITY, MAX_QUALITY))
    }

    /// The quality factor.
    pub fn get(&self) -> f32 {
        self.0
    }
}

impl Default for Quality {
    fn default() -> Self {
        Quality(0.5)
    }
}

/// Encodes one mono sample as a self-contained Ogg/Vorbis stream.
///
/// The encoder records the exact frame count in the final page's granule
/// position, so a decoder that honours it recovers precisely `pcm.len()`
/// frames. That matters because a sample's loop points are frame offsets: if
/// the length moved, the loop would move with it.
pub(crate) fn encode_sample(
    pcm: &[i16],
    sample_rate: u32,
    quality: Quality,
) -> Result<Vec<u8>, Error> {
    let rate = NonZeroU32::new(sample_rate)
        .ok_or(Error::TooLarge("sample rate of zero cannot be encoded"))?;

    let mut ogg = Vec::new();
    let mut encoder = VorbisEncoderBuilder::new(rate, NonZeroU8::new(1).unwrap(), &mut ogg)?
        .bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
            target_quality: quality.get(),
        })
        .build()?;

    // An empty block signals end-of-stream to libvorbis, so a zero-length
    // sample must skip the loop entirely and go straight to `finish`, which
    // emits a header-only — but still decodable — stream.
    let mut block = Vec::with_capacity(BLOCK_FRAMES);
    for chunk in pcm.chunks(BLOCK_FRAMES) {
        block.clear();
        block.extend(chunk.iter().map(|&s| s as f32 / 32768.0));
        encoder.encode_audio_block([&block[..]])?;
    }

    encoder.finish()?;
    Ok(ogg)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run:
```
cargo test -p rameau_soundfont_convert --lib
```
Expected: **PASS** — 5 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rameau_soundfont_convert
git commit -m "feat(convert): encode samples to Ogg/Vorbis streams"
```

---

### Task 4: Hydra serialisation (`hydra.rs`)

Serialises the model back into the nine `pdta` record arrays, regenerating the
bag index chains and terminal sentinel records that the loader strips on read.

**Files:**
- Modify: `crates/rameau_soundfont_convert/src/hydra.rs` (replace the Task 2 stub)

**Interfaces:**
- Consumes: `crate::Error`.
- Produces:
  - `pub(crate) struct SampleRegion { pub start: u32, pub end: u32, pub loop_start: u32, pub loop_end: u32 }`
  - `pub(crate) struct Pdta { pub phdr: Vec<u8>, pub pbag: Vec<u8>, pub pmod: Vec<u8>, pub pgen: Vec<u8>, pub inst: Vec<u8>, pub ibag: Vec<u8>, pub imod: Vec<u8>, pub igen: Vec<u8>, pub shdr: Vec<u8> }`
  - `pub(crate) fn build_pdta(sf: &SoundFont, regions: &[SampleRegion]) -> Result<Pdta, Error>`

- [ ] **Step 1: Write the failing tests**

Append to `crates/rameau_soundfont_convert/src/hydra.rs`:

```rust
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
        assert_eq!(p.phdr.len(), 2 * 38);
        assert_eq!(p.inst.len(), 2 * 22);
        assert_eq!(p.shdr.len(), 2 * 46);
        assert_eq!(&p.phdr[38..41], b"EOP");
        assert_eq!(&p.inst[22..25], b"EOI");
        assert_eq!(&p.shdr[46..49], b"EOS");
    }

    #[test]
    fn bag_chains_are_monotonic_and_terminated() {
        let p = build_pdta(&bank(), &regions()).unwrap();
        // Two preset zones + terminal bag.
        assert_eq!(p.pbag.len(), 3 * 4);
        // One instrument zone + terminal bag.
        assert_eq!(p.ibag.len(), 2 * 4);

        let gen_ndx = |bag: &[u8], i: usize| u16::from_le_bytes([bag[i * 4], bag[i * 4 + 1]]);
        assert_eq!(gen_ndx(&p.pbag, 0), 0); // global zone starts at 0
        assert_eq!(gen_ndx(&p.pbag, 1), 1); // after the global zone's 1 generator
        assert_eq!(gen_ndx(&p.pbag, 2), 3); // after the second zone's 2 generators
    }

    #[test]
    fn generator_amounts_round_trip_by_variant() {
        let p = build_pdta(&bank(), &regions()).unwrap();
        // pgen: [attenuation=50] [keyrange 0..127] [instrument 0] + terminal.
        assert_eq!(p.pgen.len(), 4 * 4);

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
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:
```
cargo test -p rameau_soundfont_convert --lib hydra
```
Expected: **compile error** — `build_pdta` not found.

- [ ] **Step 3: Implement `hydra.rs`**

Replace the whole of `crates/rameau_soundfont_convert/src/hydra.rs`:

```rust
//! Serialising the abstract model back into the `pdta` record arrays.
//!
//! The loader reads these arrays into presets, instruments and zones, dropping
//! the format's bookkeeping: the index chains that delimit each record's range,
//! and the terminal sentinel record that bounds the last real one. This module
//! puts both back.

use rameau_soundfont::{GeneratorAmount, SampleType, SoundFont, Zone};

use crate::Error;

/// The `.sf3` flag marking a sample as Ogg/Vorbis-compressed.
const SAMPLE_TYPE_COMPRESSED: u16 = 0x10;

/// Where one sample ended up in the encoded pool, and its loop points.
///
/// In `.sf3` `start`/`end` are *byte* offsets of the sample's Vorbis stream,
/// while the loop points remain frame offsets relative to that sample.
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
    let (phdr, pbag, pmod, pgen) = build_hierarchy(
        sf.presets.iter().map(|p| (&p.zones[..], preset_header(p))),
        b"EOP",
        38,
    )?;
    let (inst, ibag, imod, igen) = build_hierarchy(
        sf.instruments.iter().map(|i| (&i.zones[..], inst_header(i))),
        b"EOI",
        22,
    )?;

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

/// The fixed part of a `phdr` record: name, program, bank and the three
/// reserved words. The bag index is appended by `build_hierarchy`.
fn preset_header(p: &rameau_soundfont::Preset) -> Vec<u8> {
    let mut r = Vec::with_capacity(36);
    r.extend_from_slice(&name20(&p.name));
    r.extend_from_slice(&p.program.to_le_bytes());
    r.extend_from_slice(&p.bank.to_le_bytes());
    r
}

/// The fixed part of an `inst` record: just the name.
fn inst_header(i: &rameau_soundfont::Instrument) -> Vec<u8> {
    name20(&i.name).to_vec()
}

/// Builds a header/bag/modulator/generator quartet.
///
/// Each header stores the index of its first bag; each bag stores the index of
/// its first generator and modulator. The *following* record's index acts as
/// the end bound, which is why every array gets a terminal sentinel.
fn build_hierarchy<'a>(
    items: impl Iterator<Item = (&'a [Zone], Vec<u8>)>,
    sentinel: &[u8; 3],
    header_size: usize,
) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>), Error> {
    let (mut headers, mut bags, mut mods, mut gens) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());

    for (zones, fixed) in items {
        headers.extend_from_slice(&fixed);
        headers.extend_from_slice(&index(bags.len() / 4, "zones")?.to_le_bytes());
        // Presets carry three reserved dwords after the bag index; instruments
        // carry none. The header size tells us which.
        if header_size == 38 {
            headers.extend_from_slice(&[0u8; 12]);
        }

        for zone in zones {
            bags.extend_from_slice(&index(gens.len() / 4, "generators")?.to_le_bytes());
            bags.extend_from_slice(&index(mods.len() / 10, "modulators")?.to_le_bytes());

            for g in &zone.generators {
                gens.extend_from_slice(&(g.kind as u16).to_le_bytes());
                gens.extend_from_slice(&raw_amount(g.amount));
            }
            for m in &zone.modulators {
                mods.extend_from_slice(&m.source.to_le_bytes());
                mods.extend_from_slice(&(m.destination as u16).to_le_bytes());
                mods.extend_from_slice(&m.amount.to_le_bytes());
                mods.extend_from_slice(&m.amount_source.to_le_bytes());
                mods.extend_from_slice(&m.transform.to_le_bytes());
            }
        }
    }

    // Terminal records. The sentinel header's bag index bounds the last real
    // header's zones; the sentinel bag's indices bound the last real zone.
    headers.extend_from_slice(&name20(&String::from_utf8_lossy(sentinel)));
    if header_size == 38 {
        headers.extend_from_slice(&[0u8; 4]); // program, bank
    }
    headers.extend_from_slice(&index(bags.len() / 4, "zones")?.to_le_bytes());
    if header_size == 38 {
        headers.extend_from_slice(&[0u8; 12]);
    }

    bags.extend_from_slice(&index(gens.len() / 4, "generators")?.to_le_bytes());
    bags.extend_from_slice(&index(mods.len() / 10, "modulators")?.to_le_bytes());

    gens.extend_from_slice(&[0u8; 4]);
    mods.extend_from_slice(&[0u8; 10]);

    Ok((headers, bags, mods, gens))
}

/// Encodes a generator amount back into its raw two bytes, per its variant.
fn raw_amount(amount: GeneratorAmount) -> [u8; 2] {
    match amount {
        GeneratorAmount::Short(v) => v.to_le_bytes(),
        GeneratorAmount::Word(v) => v.to_le_bytes(),
        GeneratorAmount::Range(r) => [r.low, r.high],
    }
}

fn build_shdr(sf: &SoundFont, regions: &[SampleRegion]) -> Result<Vec<u8>, Error> {
    let mut shdr = Vec::with_capacity((sf.samples.len() + 1) * 46);

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
    shdr.extend_from_slice(&[0u8; 26]);

    Ok(shdr)
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run:
```
cargo test -p rameau_soundfont_convert --lib
```
Expected: **PASS** — the 5 encode tests plus 5 hydra tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rameau_soundfont_convert
git commit -m "feat(convert): serialise the model back to pdta record arrays"
```

---

### Task 5: RIFF assembly and the public API (`write.rs`)

**Files:**
- Modify: `crates/rameau_soundfont_convert/src/write.rs` (replace the Task 2 stub)

**Interfaces:**
- Consumes: `encode::{Quality, encode_sample}`, `hydra::{Pdta, SampleRegion, build_pdta}`, `crate::Error`.
- Produces: `write_sf3`, `save_sf3`, `convert_file` with the signatures declared in Task 2.

- [ ] **Step 1: Write the failing tests**

Append to `crates/rameau_soundfont_convert/src/write.rs`:

```rust
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
        assert_eq!(
            reloaded.info.software.as_deref(),
            Some("rameau_soundfont_convert")
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:
```
cargo test -p rameau_soundfont_convert --lib write
```
Expected: **FAIL** — panics at the `todo!()` in `write_sf3`.

- [ ] **Step 3: Implement `write.rs`**

Replace the whole of `crates/rameau_soundfont_convert/src/write.rs`:

```rust
//! Assembling the `.sf3` RIFF container.

use std::io::Write;
use std::path::Path;

use rameau_clip::AudioClip;
use rameau_soundfont::{Info, SoundFont};

use crate::encode::{Quality, encode_sample};
use crate::hydra::{Pdta, SampleRegion, build_pdta};
use crate::Error;

/// Stamped into the `ISFT` field of every bank this crate writes.
const SOFTWARE: &str = "rameau_soundfont_convert";

/// Loads `input` (`.sf2` or `.sf3`) and writes it to `output` as `.sf3`.
pub fn convert_file(
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
    quality: Quality,
) -> Result<(), Error> {
    let sf = SoundFont::load_file(input)?;
    save_sf3(&sf, quality, output)
}

/// Writes `sf` as a `.sf3` file at `path`.
pub fn save_sf3(sf: &SoundFont, quality: Quality, path: impl AsRef<Path>) -> Result<(), Error> {
    let file = std::fs::File::create(path)?;
    write_sf3(sf, quality, std::io::BufWriter::new(file))
}

/// Writes `sf` as a `.sf3` image into `out`.
///
/// Each sample is encoded as an independent Ogg/Vorbis stream, so unlike `.sf2`
/// the sample pool needs no padding between samples and the sample headers
/// address it by byte rather than by frame.
pub fn write_sf3<W: Write>(sf: &SoundFont, quality: Quality, mut out: W) -> Result<(), Error> {
    let (smpl, regions) = build_sample_pool(sf, quality)?;
    let pdta = build_pdta(sf, &regions)?;

    let info = build_info(&sf.info);
    let sdta = list(b"sdta", &[(b"smpl", &smpl[..])]);
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

    let body_len = 4 + info.len() + sdta.len() + pdta.len();
    out.write_all(b"RIFF")?;
    out.write_all(&u32::try_from(body_len).map_err(|_| Error::TooLarge("total file size"))?.to_le_bytes())?;
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
    let mut chunks: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();

    chunks.push((b"ifil", version_bytes(3, 0)));
    chunks.push((
        b"isng",
        zstr(info.engine.as_deref().unwrap_or("EMU8000")),
    ));
    chunks.push((b"INAM", zstr(info.name.as_deref().unwrap_or("Untitled"))));

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

    let refs: Vec<(&[u8; 4], &[u8])> = chunks.iter().map(|(id, d)| (*id, &d[..])).collect();
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
    if v.len() % 2 != 0 {
        v.push(0);
    }
    v
}

/// Wraps `chunks` in a `LIST` of the given type.
fn list(kind: &[u8; 4], chunks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(kind);
    for (id, data) in chunks {
        body.extend_from_slice(*id);
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
        // Chunks are word-aligned. The pad byte is not counted in the size.
        if data.len() % 2 != 0 {
            body.push(0);
        }
    }

    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(b"LIST");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run:
```
cargo test -p rameau_soundfont_convert --lib
```
Expected: **PASS** — all encode, hydra and write tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rameau_soundfont_convert
git commit -m "feat(convert): assemble the sf3 RIFF container"
```

---

### Task 6: The `sf2-to-sf3` binary

**Files:**
- Modify: `crates/rameau_soundfont_convert/src/main.rs` (replace the Task 2 stub)

**Interfaces:**
- Consumes: `rameau_soundfont_convert::{convert_file, Quality}`.
- Produces: the `sf2-to-sf3` executable. No later task depends on it.

Argument parsing is hand-rolled: the workspace has no CLI-argument dependency,
and three arguments do not justify adding one.

- [ ] **Step 1: Implement the binary**

Replace `crates/rameau_soundfont_convert/src/main.rs`:

```rust
//! `sf2-to-sf3` — convert a SoundFont bank to the Ogg/Vorbis-compressed format.

use std::process::ExitCode;

use rameau_soundfont_convert::{Quality, convert_file};

const USAGE: &str = "\
usage: sf2-to-sf3 <input.sf2> <output.sf3> [-q <quality>]

  -q <quality>   Ogg/Vorbis VBR quality, -0.2 (smallest) to 1.0 (best).
                 Defaults to 0.5.
";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("sf2-to-sf3: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut positional = Vec::new();
    let mut quality = Quality::default();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            "-q" | "--quality" => {
                let value = args.next().ok_or("-q needs a value")?;
                let parsed: f32 = value.parse().map_err(|_| format!("bad quality: {value}"))?;
                quality = Quality::new(parsed);
            }
            other => positional.push(other.to_string()),
        }
    }

    let [input, output] = positional.as_slice() else {
        return Err(format!("expected an input and an output path\n\n{USAGE}"));
    };

    convert_file(input, output, quality).map_err(|e| format!("{input}: {e}"))?;

    let before = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    let after = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    if before > 0 && after > 0 {
        println!(
            "{input} -> {output}  ({:.1} MiB -> {:.1} MiB, {:.0}% smaller)",
            before as f64 / (1024.0 * 1024.0),
            after as f64 / (1024.0 * 1024.0),
            100.0 - (after as f64 / before as f64 * 100.0)
        );
    }
    Ok(())
}
```

- [ ] **Step 2: Verify it builds and reports usage**

Run:
```
cargo run -p rameau_soundfont_convert --bin sf2-to-sf3 -- --help
```
Expected: the usage text, exit code 0.

Run:
```
cargo run -p rameau_soundfont_convert --bin sf2-to-sf3
```
Expected: the "expected an input and an output path" error, exit code 1.

- [ ] **Step 3: Convert the real bank end to end**

Run (release, so the conversion is quick):
```
cargo run --release -p rameau_soundfont_convert --bin sf2-to-sf3 -- assets/Unison.SF2 target/Unison.sf3
```
Expected: a size-reduction line. **Record the wall-clock time and the output
size** — Task 7 needs them to decide whether the integration test is fast
enough to run by default.

- [ ] **Step 4: Commit**

```bash
git add crates/rameau_soundfont_convert/src/main.rs
git commit -m "feat(convert): add the sf2-to-sf3 command-line tool"
```

---

### Task 7: Integration test against `assets/Unison.SF2`, and docs

**Files:**
- Create: `crates/rameau_soundfont_convert/tests/convert.rs`
- Modify: `README.md` (crate table)

**Interfaces:**
- Consumes: the whole public API.
- Produces: nothing further.

- [ ] **Step 1: Write the integration test**

Create `crates/rameau_soundfont_convert/tests/convert.rs`:

```rust
//! End-to-end conversion of the real `.sf2` bank shipped in `assets/`.

use std::path::PathBuf;

use rameau_soundfont::SoundFont;
use rameau_soundfont_convert::{Quality, save_sf3};

fn asset(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .join(name)
}

/// Root-mean-square difference between two signals, normalised to full scale.
fn rms_error(a: &[i16], b: &[i16]) -> f64 {
    if a.is_empty() {
        return 0.0;
    }
    let sum: f64 = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| {
            let d = (x as f64 - y as f64) / 32768.0;
            d * d
        })
        .sum();
    (sum / a.len() as f64).sqrt()
}

/// Converts the real bank and reloads it, asserting that everything the model
/// carries survives the trip and that only the audio changed — lossily, but
/// within tolerance, and without any change in length.
#[test]
fn converts_unison_sf2_to_sf3() {
    let original = SoundFont::load_file(asset("Unison.SF2")).expect("load Unison.SF2");

    let out = std::env::temp_dir().join("rameau_unison_convert_test.sf3");
    save_sf3(&original, Quality::default(), &out).expect("write sf3");

    let converted = SoundFont::load_file(&out).expect("reload the converted sf3");

    assert_eq!(converted.info.version.major, 3, "expected an sf3 ifil");
    assert_eq!(converted.presets.len(), original.presets.len());
    assert_eq!(converted.instruments.len(), original.instruments.len());
    assert_eq!(converted.samples.len(), original.samples.len());

    for (a, b) in original.presets.iter().zip(&converted.presets) {
        assert_eq!(b.name, a.name);
        assert_eq!(b.bank, a.bank);
        assert_eq!(b.program, a.program);
        assert_eq!(b.zones.len(), a.zones.len(), "preset '{}'", a.name);
    }

    for (a, b) in original.instruments.iter().zip(&converted.instruments) {
        assert_eq!(b.name, a.name);
        assert_eq!(b.zones.len(), a.zones.len(), "instrument '{}'", a.name);
    }

    let mut worst_rms = 0.0f64;
    let mut worst_name = String::new();
    for (a, b) in original.samples.iter().zip(&converted.samples) {
        assert_eq!(b.name, a.name);
        assert_eq!(b.sample_rate, a.sample_rate, "sample '{}'", a.name);
        assert_eq!(b.original_key, a.original_key, "sample '{}'", a.name);
        assert_eq!(b.correction, a.correction, "sample '{}'", a.name);
        assert_eq!(b.link, a.link, "sample '{}'", a.name);
        assert_eq!(b.kind, a.kind, "sample '{}'", a.name);
        assert_eq!(b.loop_start, a.loop_start, "sample '{}'", a.name);
        assert_eq!(b.loop_end, a.loop_end, "sample '{}'", a.name);

        // Loop points are frame offsets, so a length change would move them.
        assert_eq!(
            b.clip.data.len(),
            a.clip.data.len(),
            "sample '{}' changed length",
            a.name
        );

        let rms = rms_error(&a.clip.data, &b.clip.data);
        if rms > worst_rms {
            worst_rms = rms;
            worst_name = a.name.clone();
        }
    }

    // Vorbis is lossy, so exact PCM equality is the wrong assertion; what
    // matters is that no sample is grossly wrong (which would indicate streams
    // being mis-sliced or samples swapped, not codec loss).
    assert!(
        worst_rms < 0.1,
        "sample '{worst_name}' differs by rms {worst_rms:.4}"
    );

    let before = std::fs::metadata(asset("Unison.SF2")).unwrap().len();
    let after = std::fs::metadata(&out).unwrap().len();
    assert!(
        after < before / 2,
        "expected substantial compression, got {before} -> {after} bytes"
    );

    let _ = std::fs::remove_file(&out);
}
```

- [ ] **Step 2: Run the integration test and time it**

Run:
```
cargo test -p rameau_soundfont_convert --test convert -- --nocapture
```
Expected: **PASS**.

**Decision point.** Time this run.
- Under ~2 minutes: leave it enabled and move on.
- Slower than that: add `#[ignore = "converts a 29 MB bank; run with --ignored"]`
  above the `#[test]`, and note the command in the crate docs. Do **not** weaken
  any assertion to make it faster.

Record the actual figure — it goes in the summary.

- [ ] **Step 3: Add the crate to the README table**

In `README.md`, add a row to the crate table, immediately after the
`rameau_soundfont` row so the two sit together:

```markdown
| [`rameau_soundfont_convert`](crates/rameau_soundfont_convert) | Convert `.sf2` banks to Ogg/Vorbis-compressed `.sf3` |
```

- [ ] **Step 4: Verify the whole workspace is green**

Run:
```
cargo test --workspace
```
Expected: **PASS**, including `rameau_soundfont`'s existing tests and the new
regression test from Task 1.

Run:
```
cargo clippy --workspace --all-targets
```
Expected: no new warnings from `rameau_soundfont_convert`.

- [ ] **Step 5: Commit**

```bash
git add crates/rameau_soundfont_convert/tests README.md
git commit -m "test(convert): verify sf2 -> sf3 round-trip against Unison.SF2"
```

---

## Self-Review

**Spec coverage**

| Spec section | Task |
| --- | --- |
| Prerequisite decoder bug fix | 1 |
| Crate structure, `Error` | 2 |
| Build configuration (workspace-wide profile overrides) | 2 |
| Sample pool / Vorbis encoding, empty-sample guard | 3 |
| Record arrays, sentinels, bag chains, generator amounts | 4 |
| Sample headers, `.sf3` byte addressing, compressed bit | 4 |
| `INFO` list, `ifil` 3.0, `ISFT` stamp | 5 |
| Container assembly, even-length padding | 5 |
| Public API (`write_sf3`, `save_sf3`, `convert_file`) | 5 |
| CLI binary | 6 |
| Unit tests (encode / hydra / write) | 3, 4, 5 |
| Integration test against `Unison.SF2` | 7 |
| Documentation, README row | 2 (crate docs), 7 (README) |

No spec requirement is unassigned.

**Type consistency**

- `Quality` is defined in Task 2's stub and completed in Task 3; `Quality::new`,
  `Quality::get` and `Quality::default` are used consistently in Tasks 3, 5, 6.
- `SampleRegion` fields (`start`, `end`, `loop_start`, `loop_end`) are declared
  in Task 4 and constructed with exactly those names in Task 5.
- `Pdta` field names match the chunk names used in Task 5's `list` call.
- `encode_sample(&[i16], u32, Quality) -> Result<Vec<u8>, Error>` is declared in
  Task 3 and called with that signature in Task 5.
- `Error::TooLarge(&'static str)` is constructed in Tasks 3, 4 and 5 and matched
  in Task 3's test.

**Open risks, to confirm during implementation rather than assume**

1. Task 2 Step 1 verifies the codec package names against `Cargo.lock` before
   the profile overrides are trusted; a name that is not in the graph is an
   error, not a silent no-op.
2. Task 4's `build_hierarchy` distinguishes preset from instrument records by
   `header_size`. If that reads awkwardly once written, splitting it into two
   functions is a fine simplification — the tests pin the behaviour either way.
3. Task 7 Step 2 measures the test's runtime and decides on `#[ignore]` from the
   real figure.
