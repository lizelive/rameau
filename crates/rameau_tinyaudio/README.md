# rameau_tinyaudio

A `rameau_playback` device backend built on
[tinyaudio](https://crates.io/crates/tinyaudio).

`TinyAudio` implements the `Playback` trait, opening a `tinyaudio` output device
that drives the supplied fill callback on its own audio thread. The returned
`Stream` keeps the device alive; dropping it stops playback.

```rust,no_run
use rameau_tinyaudio::TinyAudio;
use rameau_playback::{Playback, PlaybackConfig};

let backend = TinyAudio;
let _stream = backend.open(PlaybackConfig::stereo_cd(), |buf: &mut [f32]| {
    buf.fill(0.0); // fill interleaved L, R, L, R, ...
})?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## License

AGPL-3.0-or-later. See [LICENSE](../../LICENSE).
