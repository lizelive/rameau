# rameau

A Rust workspace for SoundFont-based MIDI playback, plus the top-level `rameau`
crate that ties it together behind a high-level `MusicEngine`.

## MusicEngine

The `rameau` crate exposes a `MusicEngine` that plays Standard MIDI Files through
a SoundFont, using [`rameau_kira`](crates/rameau_kira) as its real-time audio
backend:

```rust,no_run
use rameau::MusicEngine;

let mut engine = MusicEngine::init("assets/FluidR3Mono_GM.sf3")?;
let song = engine.load_midi("song.mid")?;
engine.play_midi(&song)?;
# Ok::<(), rameau::EngineError>(())
```

- `MusicEngine::init(soundfont)` — open the audio device and load a `.sf2`/`.sf3`
  bank.
- `MusicEngine::load_midi(path)` — parse a `.mid` file into a `Song`.
- `MusicEngine::play_midi(&song)` — play it in real time.

### The `unison` feature

Enable it to embed a general-MIDI bank in the binary and get
`MusicEngine::new()`, which needs no SoundFont file:

```toml
rameau = { version = "0.1", features = ["unison"] }
```

```rust,no_run
use rameau::MusicEngine;

# fn main() -> Result<(), rameau::EngineError> {
let mut engine = MusicEngine::new()?;
# Ok(()) }
```

```console
$ cargo run --release --features unison --example play_unison -- assets/heist.midi
engine ready in 1.03s (168 presets)
playing assets/heist.midi: 608 events, 35.8s
```

It is off by default: the bank adds about 6.6 MiB to the binary, which is
wasted on any program that supplies its own. The bank is CC0-1.0 while the code
is AGPL — see [`rameau_unison`](crates/rameau_unison).

## Crates

| Crate | Purpose |
| --- | --- |
| [`rameau_clip`](crates/rameau_clip) | Format-independent audio sample container (`AudioClip` / `Clip`) |
| [`rameau_playback`](crates/rameau_playback) | Backend-independent device and sample-engine traits |
| [`rameau_kira`](crates/rameau_kira) | Real-time `AudioPlayback` backend on kira |
| [`rameau_software`](crates/rameau_software) | Pure-software mixing backend (offline rendering) |
| [`rameau_tinyaudio`](crates/rameau_tinyaudio) | `Playback` device backend on tinyaudio |
| [`rameau_wav`](crates/rameau_wav) | 16-bit PCM WAV writer |
| [`rameau_midi`](crates/rameau_midi) | MIDI types and Standard MIDI File parsing |
| [`rameau_soundfont`](crates/rameau_soundfont) | SoundFont model and `.sf2`/`.sf3` loader |
| [`rameau_soundfont_convert`](crates/rameau_soundfont_convert) | Convert `.sf2` banks to Ogg/Vorbis-compressed `.sf3` |
| [`rameau_unison`](crates/rameau_unison) | The Unison bank (CC0-1.0) embedded as compressed `.sf3` |
| [`rameau_synthesizer`](crates/rameau_synthesizer) | SoundFont synthesizer driving a backend |
| [`rameau_theory`](crates/rameau_theory) | Music theory primitives (placeholder) |
| [`rameau_chords`](crates/rameau_chords) | Chord construction/recognition (placeholder) |
| [`rameau_types`](crates/rameau_types) | Shared core types (placeholder) |

## License

AGPL-3.0-or-later. See [LICENSE](LICENSE).
