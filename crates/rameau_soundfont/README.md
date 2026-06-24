# rameau_soundfont

A format-independent SoundFont representation and a loader for `.sf2`/`.sf3`.

`SoundFont` is the abstract model: a bank's presets, instruments and
(PCM-decoded) samples, with no trace of how the bank was stored on disk. Both
`.sf2` (raw 16-bit PCM samples) and `.sf3` (Ogg/Vorbis-compressed samples) parse
into the same model, with audio decoded to PCM and loop points rebased.

`SoundFont` is generic over the sample *clip* type: by default samples hold
`Clip<i16>`, but `load_file_with` / `load_with` let a playback backend store its
own native clip type so the synthesizer never touches raw audio.

```rust,no_run
use rameau_soundfont::SoundFont;

// Decode samples into the default Clip<i16>:
let sf = SoundFont::load_file("bank.sf2")?;
println!("{} presets, {} samples", sf.presets.len(), sf.samples.len());

// ...or into a backend's own clip type:
let mut backend = rameau_software::Software::new(44_100);
let sf = SoundFont::load_file_with("bank.sf2", &mut backend)?;
# Ok::<(), rameau_soundfont::Error>(())
```

## Features

- `serde` — derive `Serialize`/`Deserialize` for the whole model.

## License

AGPL-3.0-or-later. See [LICENSE](../../LICENSE).
