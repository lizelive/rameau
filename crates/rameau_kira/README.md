# rameau_kira

A real-time `AudioPlayback` backend built on [kira](https://crates.io/crates/kira).

`Kira` owns a `kira::AudioManager` and plays each voice as a `StaticSoundData`;
kira does the resampling, pitch-shifting, mixing and device output. Starting a
voice returns a handle that `update` and `stop` drive with tweens.

```rust,no_run
use rameau_kira::Kira;
use rameau_playback::AudioPlayback;

let mut backend = Kira::new()?;
let clip = backend.clip_from_pcm(&[0i16; 1024], 44_100)?;
# Ok::<(), rameau_playback::PlaybackError>(())
```

## Capability mapping

- `pitch` → playback rate (semitones)
- `volume` (linear) → decibels
- `position` → stereo panning (`position.x`); 3D spatialization and Doppler are
  degraded away, not errored
- offline `render` is unsupported (real-time only) — use
  [`rameau_software`](../rameau_software) for offline rendering

## License

AGPL-3.0-or-later. See [LICENSE](../../LICENSE).
