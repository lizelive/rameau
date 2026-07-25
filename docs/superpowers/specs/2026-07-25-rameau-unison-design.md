# `rameau_unison` — an embedded default SoundFont

## Purpose

Give `rameau` a zero-configuration SoundFont so `MusicEngine` can be constructed
without the caller supplying a bank file. The bank is Ogg/Vorbis-compressed
`.sf3` produced by
[`rameau_soundfont_convert`](../specs/2026-07-25-sf2-to-sf3-conversion-design.md)
and embedded in the binary with `include_bytes!`.

## Scope

In scope:

- A `rameau_unison` crate exposing the embedded bank.
- An optional, **off-by-default** `unison` feature on `rameau`, adding
  `MusicEngine::new()`.
- Lazy, cached decoding.

Out of scope (YAGNI):

- Multiple embedded banks, or bank selection.
- Runtime download or caching of banks.
- Embedding `FluidR3Mono_GM.sf3` as well.

## Licensing

This matters more than the code and is the reason the crate carries its own
licence file.

`Unison.SF2` is **CC0-1.0** (public domain dedication), sourced from
`https://rkhive.com/new/new_banks/unison.zip`, as recorded in `assets/README.md`.
The workspace is AGPL-3.0-or-later. Those are different terms covering
different things, and the distinction has to survive redistribution: someone
who vendors `rameau_unison` from crates.io never sees `assets/README.md`.

Therefore `crates/rameau_unison/` carries:

- `Unison.LICENSE` — the CC0 dedication and the upstream source URL, sitting
  next to the binary it describes.
- Crate-level documentation stating the bank is CC0 and the code is AGPL.
- A `description` naming the embedded bank.

## Size

Measured for `assets/Unison.SF2` converted at each quality. "raw" is what
`include_bytes!` adds to a dependent's binary; "gzipped" approximates the
crates.io package, whose limit is 10 MiB.

| Quality | raw | gzipped | headroom |
| --- | --- | --- | --- |
| -0.2 | 3.72 MiB | 1.49 MiB | 8.51 MiB |
| 0.1 | 4.79 MiB | 2.75 MiB | 7.25 MiB |
| 0.3 | 5.51 MiB | 3.34 MiB | 6.66 MiB |
| **0.5** | **6.65 MiB** | **4.18 MiB** | **5.82 MiB** |
| 0.7 | 7.68 MiB | 5.33 MiB | 4.67 MiB |

**Quality 0.5** is chosen: it is the converter's default, it is the setting the
existing `converts_unison_sf2_to_sf3` integration test already exercises, and
it leaves comfortable headroom under the crates.io limit.

The `.sf3` is committed at `crates/rameau_unison/Unison.sf3` as the **single**
copy in the repository. The `assets/Unison.sf3` produced while exploring is
deleted rather than left as a 6.6 MiB duplicate. `assets/Unison.SF2` remains the
source of truth, and the crate documents how to regenerate:

```
cargo run --release -p rameau_soundfont_convert --bin sf2-to-sf3 -- \
    assets/Unison.SF2 crates/rameau_unison/Unison.sf3
```

## Structure

New crate at `crates/rameau_unison/`, picked up by the existing
`members = ["crates/*"]` glob.

```
crates/rameau_unison/
  Cargo.toml
  Unison.LICENSE     CC0 dedication + upstream source
  Unison.sf3         6.65 MiB, quality 0.5
  src/lib.rs         the whole crate
```

One file of code. The crate does no parsing of its own — it delegates entirely
to `rameau_soundfont`.

### Public API

```rust
/// The raw `.sf3` image.
pub const SOUNDFONT: &[u8];

/// The bank, decoded to PCM. Decoded on first call and cached thereafter.
pub fn soundfont() -> &'static SoundFont;

/// Loads the bank into a playback backend's native clip type.
pub fn load_with<P: AudioPlayback>(backend: &mut P) -> Result<SoundFont<P::Clip>, Error>;
```

Three entry points, each with a distinct job:

- `SOUNDFONT` is the escape hatch — raw bytes, free, for anyone who wants to
  write them to disk or parse them their own way.
- `soundfont()` is the common case: a shared, already-decoded bank.
- `load_with` is what `MusicEngine` needs, and is the one thing that *cannot*
  be cached, because the clip type depends on the backend and the resulting
  clips are owned by it.

### Laziness

`include_bytes!` places the bank in read-only data. It costs nothing at
startup and there is nothing to defer about it.

The decode is the expensive part, and it is deferred with `LazyLock`:

```rust
static BANK: LazyLock<SoundFont> = LazyLock::new(|| {
    SoundFont::from_bytes(SOUNDFONT).expect("the embedded Unison bank is corrupt")
});

pub fn soundfont() -> &'static SoundFont {
    &BANK
}
```

This gives three properties that matter:

1. Nothing is decoded at static-initialisation time — a program that links the
   crate but never calls `soundfont()` pays only binary size.
2. The decode happens at most once, however many callers there are.
3. `LazyLock` handles concurrent first-calls; the decode is not repeated per
   thread.

The closure is infallible by design. `SOUNDFONT` is a compile-time constant
that the crate's own tests parse and validate, so a failure here means a
corrupted build artifact rather than a runtime condition, and the panic message
says so. Callers who want a fallible path use `load_with`, which returns
`Result`.

`load_with` is deliberately *not* lazy or cached: each backend needs its own
clips, and a `&'static` cache cannot be keyed on an arbitrary backend type.

## `rameau` wiring

```toml
[dependencies]
rameau_unison = { path = "crates/rameau_unison", version = "0.1.0", optional = true }

[features]
default = []
unison = ["dep:rameau_unison"]
```

Off by default, so `cargo add rameau` stays lean and only users who ask for the
built-in bank pay the 6.65 MiB. Opting in is `features = ["unison"]`.

```rust
#[cfg(feature = "unison")]
impl MusicEngine {
    /// Opens the default audio device with the built-in Unison bank.
    pub fn new() -> Result<Self, EngineError> {
        let mut backend = Kira::new()?;
        let sf = rameau_unison::load_with(&mut backend)?;
        Ok(Self { synth: Synthesizer::new(sf, backend, SAMPLE_RATE) })
    }
}
```

`MusicEngine::init(path)` is unchanged.

Note that `rameau_kira` implements `clip_from_vorbis` natively, handing each
Ogg stream to kira's `StaticSoundData`. So the `MusicEngine` path never decodes
through lewton and never builds a PCM `SoundFont` — it goes straight from the
embedded bytes to kira's clips. `MusicEngine::new()` therefore does not benefit
from the `soundfont()` cache, and its cost is kira decoding 655 streams. That
figure is measured during implementation and recorded in the documentation
rather than left as a surprise.

## Error handling

No new error type. `load_with` returns `rameau_soundfont::Error`, and
`MusicEngine::new` folds it into the existing `EngineError::SoundFont` through
the `From` impl that is already there.

## Testing

`rameau_unison` — none of these need audio hardware:

- The embedded bytes parse, and report `ifil` major version 3.
- Counts match the source bank exactly: **168 presets, 231 instruments, 655
  samples**. These are the figures the conversion integration test reports for
  `assets/Unison.SF2`, so a mismatch means the committed `.sf3` has drifted
  from its source.
- A named spot-check: a preset called `ACOUSTIC GRAND PIANO` exists. This
  catches a truncated or wrong bank being committed.
- `soundfont()` returns the same instance across calls, confirming the cache
  rather than merely that it works — compared by pointer.
- Every sample has non-empty audio and a non-zero sample rate.

`rameau`:

- `cargo check --features unison` must pass. `MusicEngine::new()` opens a real
  audio device, so it cannot be unit-tested in this environment — consistent
  with there being no `MusicEngine` tests today. A `no_run` doc example covers
  the call shape and is compiled by `cargo test --doc`.
- `cargo check` without the feature must also pass, confirming the gate.

## Documentation

- Crate-level docs for `rameau_unison`: usage, licence split, regeneration
  command, and the size cost.
- `rameau`'s crate docs gain a section on the `unison` feature and
  `MusicEngine::new()`.
- A `rameau_unison` row in the workspace `README.md` crate table.
