# `rameau_soundfont_convert` — SF2 → SF3 conversion

## Purpose

Convert an uncompressed `.sf2` bank into a `.sf3` bank, in which each sample's
audio is stored as an independent Ogg/Vorbis stream. The workspace can already
*read* both formats ([`rameau_soundfont`](../../../crates/rameau_soundfont));
it has no writer for either. This crate adds one, for `.sf3` only.

The motivating case is `assets/Unison.SF2` (29 MB, 655 samples), which should
come out several times smaller.

## Scope

In scope:

- A library API that writes a `SoundFont<Clip<i16>>` out as `.sf3`.
- A `sf2-to-sf3` CLI binary.
- Verification against the real `assets/Unison.SF2`.

Explicitly out of scope (YAGNI):

- The reverse direction (`.sf3` → `.sf2`).
- Writing uncompressed `.sf2`.
- 24-bit (`sm24`) sample support. Neither shipped asset uses it, and `.sf3`
  has no place to put the extra byte anyway: the loader already discards it.
- Preserving `INFO` chunks the loader does not model.

## Approach: model round-trip

Conversion goes through the abstract model rather than patching the container
in place:

```
SoundFont::load_file("in.sf2")  →  SoundFont<Clip<i16>>  →  write_sf3()  →  "out.sf3"
```

The alternative — copying the `pdta` chunks through byte-for-byte and rewriting
only `smpl` — is what Polyphone's `sf2convert` does and is trivially lossless.
The round-trip was chosen anyway because it gives the workspace a real
SoundFont *writer*, reusable for any future `SoundFont` value (synthesized
banks, edited banks) rather than only for files that already exist on disk.

### Why the round-trip is safe here

The loader is lossy in two places. Both were measured against the shipped
assets before committing to this approach:

| Loader behaviour | Risk | Unison.SF2 | FluidR3Mono_GM.sf3 |
| --- | --- | --- | --- |
| `pgen`/`igen` records whose operator is `>= 61` are dropped by `GeneratorType::from_u16` | generator silently lost | 0 of 13,685 | 0 of 14,761 |
| A modulator whose destination is a *link to another modulator* (bit 15 set) collapses to `UNUSED_END` | modulator corrupted | 0 of 2 | 0 of 391 |

Both counts are zero, so no data is lost converting these banks. This is a
property of the inputs, not a guarantee of the design — the limitation is
documented in the crate docs, and the integration test asserts structural
equality after a reload so a regression would be caught.

## Structure

New crate at `crates/rameau_soundfont_convert/`.

| Module | Responsibility |
| --- | --- |
| `lib.rs` | Public API and `Error`; crate docs stating the lossiness caveat above |
| `encode.rs` | i16 PCM → a single mono Ogg/Vorbis stream |
| `hydra.rs` | `SoundFont` → the nine `pdta` record arrays |
| `write.rs` | RIFF container assembly |
| `main.rs` | The `sf2-to-sf3` binary |

Each module is independently testable: `encode` takes samples and returns
bytes, `hydra` takes a model and returns byte vectors, `write` takes those and
frames them. None of them touch the filesystem except the two convenience
entry points in `lib.rs`.

### Public API

```rust
/// Ogg/Vorbis VBR quality, -0.1 (smallest) to 1.0 (best). Default 0.5.
pub struct Quality(f32);

/// Writes `sf` as a `.sf3` image into `out`.
pub fn write_sf3<W: Write>(sf: &SoundFont, quality: Quality, out: W) -> Result<(), Error>;

/// Writes `sf` as a `.sf3` file at `path`.
pub fn save_sf3(sf: &SoundFont, quality: Quality, path: impl AsRef<Path>) -> Result<(), Error>;

/// Loads `input` (`.sf2` or `.sf3`) and writes it to `output` as `.sf3`.
pub fn convert_file(
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
    quality: Quality,
) -> Result<(), Error>;
```

### `Error`

```rust
pub enum Error {
    Io(std::io::Error),
    Load(rameau_soundfont::Error),   // only from `convert_file`
    Encode(vorbis_rs::VorbisError),
    /// A count or offset exceeded what the SoundFont format can address.
    TooLarge(&'static str),
}
```

`TooLarge` covers the format's hard ceilings: bag, generator, modulator,
instrument and sample indices are all `u16`, and the sample pool is addressed
by `u32`. These are unreachable for realistic banks but must not be allowed to
wrap silently into a corrupt file.

## Data flow

### 1. Sample pool (`sdta` / `smpl`)

For each `Sample` in order, encode `clip.data` as one mono Vorbis stream at
`sample_rate` and append it to the pool. Record the `[start_byte, end_byte)`
range it occupies.

- `i16` → `f32` conversion divides by 32768.0.
- A zero-length sample still gets a valid (header-only) stream, so that every
  `shdr` record points at something decodable.
- Unlike `.sf2`, `.sf3` needs no zero padding between samples — each stream is
  self-delimiting.

Vorbis is lossy in amplitude but not in *length*: libvorbis encodes the exact
frame count in the final page's granule position, so a decoder returns the same
number of frames it was given. Loop points therefore stay valid. The
integration test asserts this rather than trusting it.

### 2. Record arrays (`pdta`)

Nine arrays, each a flat sequence of fixed-size little-endian records:

| Chunk | Record size | Notes |
| --- | --- | --- |
| `phdr` | 38 | + terminal `EOP` sentinel |
| `pbag` | 4 | + terminal sentinel |
| `pmod` | 10 | + terminal sentinel (all zero) |
| `pgen` | 4 | + terminal sentinel (all zero) |
| `inst` | 22 | + terminal `EOI` sentinel |
| `ibag` | 4 | + terminal sentinel |
| `imod` | 10 | + terminal sentinel (all zero) |
| `igen` | 4 | + terminal sentinel (all zero) |
| `shdr` | 46 | + terminal `EOS` sentinel |

The loader strips all of these sentinels, so the writer regenerates them. It
also rebuilds the bag index chains: each preset/instrument header stores the
index of its first bag, and each bag stores the index of its first generator
and first modulator, with the following record's index acting as the end bound.

Zone and generator order is preserved exactly as the loader read it, which
keeps the spec's ordering requirements intact — a global zone stays first, and
within a zone `KeyRange`/`VelocityRange` stay leading and
`Instrument`/`SampleID` stay trailing.

`GeneratorAmount` is written back per its variant: `Range` as two bytes,
`Word` as `u16` LE, `Short` as `i16` LE.

### 3. Sample headers (`shdr`)

This is where `.sf3` diverges from `.sf2`:

| Field | `.sf2` | `.sf3` (what we write) |
| --- | --- | --- |
| `dwStart` / `dwEnd` | frame index into a flat PCM pool | **byte** offset of the sample's Vorbis stream |
| `dwStartloop` / `dwEndloop` | frame index into the same flat pool | frame offset **relative to this sample** |
| `sfSampleType` | `1`/`2`/`4`/`8` | same, `| 0x10` (compressed) |

The loader already normalises loop points to be sample-relative on read, so
they are written straight through. `wSampleLink` is preserved so stereo pairs
survive.

### 4. `INFO`

Written from the model's `Info`, with two deliberate changes:

- `ifil` is forced to **3.0**, marking the file as `.sf3`.
- `ISFT` is stamped `rameau_soundfont_convert`.

`INAM` defaults to `"Untitled"` and `isng` to `"EMU8000"` when absent, since
the spec requires both. All strings are NUL-terminated and padded to an even
length; odd-sized chunks get a pad byte, which is not counted in the chunk's
declared size.

### 5. Container

```
RIFF <size> sfbk
  LIST <size> INFO   ifil isng INAM [irom iver ICRD IENG IPRD ICOP ICMT] ISFT
  LIST <size> sdta   smpl
  LIST <size> pdta   phdr pbag pmod pgen inst ibag imod igen shdr
```

## Build configuration

The codec crates are C libraries compiled through `cc`, and are unusably slow
at the default debug `opt-level = 0` — for a 655-sample bank that is the
difference between a test that runs and a test that appears to hang. The
workspace root gets per-package profile overrides so they are always built
optimised, in every profile:

```toml
[profile.dev.package.vorbis_rs]
opt-level = 3
[profile.dev.package.aotuv_lancer_vorbis_sys]
opt-level = 3
[profile.dev.package.ogg_next_sys]
opt-level = 3
[profile.dev.package.lewton]
opt-level = 3
```

`lewton` is included because the verification test decodes the whole converted
bank back through the existing loader.

`cc` reads Cargo's per-package `OPT_LEVEL`, so this raises the optimisation
level of the bundled C sources too, not just the Rust wrappers. `profile.test`
and `profile.bench` inherit from `profile.dev`, so the overrides apply to test
builds without being repeated. Exact package names are confirmed against
`Cargo.lock` during implementation.

## Testing

### Unit

- `encode`: round-trip a short synthetic tone; assert the decoded frame count
  equals the input frame count exactly, and that the result is not silence.
- `hydra`: build a small model by hand, serialise, and check record counts,
  sentinel presence, and that bag index chains are monotonic.
- `write`: assert a minimal bank produces a well-formed RIFF whose chunk sizes
  agree with its actual contents.

### Integration — `assets/Unison.SF2`

Following the pattern already in `crates/rameau_soundfont/tests/load.rs`,
which reads the assets directly.

1. Load `Unison.SF2`.
2. Convert to `.sf3` in a temp directory.
3. Reload the `.sf3` with `SoundFont::load_file`.
4. Assert:
   - preset, instrument and sample counts are identical;
   - every preset/instrument/sample **name** matches, in order;
   - every preset's bank/program matches;
   - every sample's `sample_rate`, `original_key`, `correction`, `link`,
     `kind`, `loop_start` and `loop_end` match;
   - every sample's decoded frame count matches the original exactly;
   - per-sample RMS error is within tolerance — lossy compression changes
     amplitudes, so exact PCM equality is the wrong assertion;
   - the output file is substantially smaller than the input.

The wall-clock cost of encoding 655 samples is measured and reported rather
than assumed; if the test is slow enough to be disruptive it will be marked
`#[ignore]` with a note on how to run it, and a fast synthetic test will cover
the same assertions.

## Documentation

- Crate-level docs covering usage, the `.sf2` vs `.sf3` sample-storage
  difference, and the round-trip caveat.
- A `rameau_soundfont_convert` row added to the crate table in the workspace
  `README.md`.
