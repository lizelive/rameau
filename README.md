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

## Composing, not just playing

The workspace also contains a real-time composition engine, built for the
dynamic score of a French Revolution game but general in its parts:

```rust,no_run
use rameau_compose::{Composer, IdeaLibrary, MusicState};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut composer = Composer::new(IdeaLibrary::load_dir("ideas")?, 1789);
composer.set_state(MusicState { tempo_bpm: 120.0, voices: 3, ..MusicState::default() });
let bar = composer.next_bar(); // notes, programs, and why it wrote them
# Ok(()) }
```

`rameau_compose` plans each bar with a form (fugue, rondeau, chaconne, air,
contredanse), harmonises the fixed tune over a period chord grammar, writes
the free voices by simulated annealing against counterpoint rules and the
sliders' targets, and orchestrates from a catalogue of period instruments.
`rameau_voix` sings it on synthesized French vowels. See the crate READMEs.

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
| [`rameau_theory`](crates/rameau_theory) | Pitch classes, modes, degree-encoded notes, keys, meters and the voice-leading rules |
| [`rameau_chords`](crates/rameau_chords) | Roman numerals, cadences, a period harmonic grammar and the stock grounds |
| [`rameau_kern`](crates/rameau_kern) | Humdrum `**kern` parser |
| [`rameau_voix`](crates/rameau_voix) | Formant-synthesized singing voice as a SoundFont bank, with text-to-vowel reduction |
| [`rameau_compose`](crates/rameau_compose) | The composition engine: ideas, mutations, forms, harmonisation, annealing, instruments, conducting |
| [`rameau_types`](crates/rameau_types) | Shared core types: a deterministic RNG |

## License

AGPL-3.0-or-later. See [LICENSE](LICENSE).
