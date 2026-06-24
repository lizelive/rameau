# rameau_clip

A minimal, format-independent representation of a block of audio samples.

The `AudioClip` trait abstracts over "a contiguous run of PCM samples at a known
sample rate". `Clip<T>` is the simple owned container that implements it, but
other crates can implement `AudioClip` for their own types so that tools such as
a WAV writer can consume them without copying.

```rust
use rameau_clip::{AudioClip, Clip};

let clip = Clip::new(vec![0i16, 16384, -16384, 0], 44_100);
assert_eq!(clip.sample_rate(), 44_100);
assert_eq!(clip.data().len(), 4);
```

## Features

- `serde` — derive `Serialize`/`Deserialize` for `Clip<T>`.

## License

AGPL-3.0-or-later. See [LICENSE](../../LICENSE).
