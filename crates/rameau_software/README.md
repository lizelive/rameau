# rameau_software

A pure-software `AudioPlayback` backend: a small polyphonic sample mixer.

`Software` keeps a pool of voices, each playing one clip with linear-interpolated
resampling, looping, equal-power panning and an attack/release envelope, and
mixes them into an interleaved-stereo `f32` buffer in `render`.

Unlike a real-time engine it does not own an output device — it just fills a
buffer on demand, which makes it the natural backend for **offline rendering**:
start every note at its scheduled `Timestamp::AtSeconds`, then call `render` once
over the whole piece. It can equally drive a live device by being fed small
blocks from an audio callback (e.g. [`rameau_tinyaudio`](../rameau_tinyaudio)).

```rust
use rameau_software::Software;
use rameau_playback::AudioPlayback;
use rameau_clip::Clip;

let mut sw = Software::new(44_100);
let mut block = Clip::new(vec![0.0f32; 512 * 2], 44_100); // interleaved stereo
sw.render(&mut block).unwrap();
```

Per-voice spatial parameters degrade gracefully: a `Vec3` position collapses to a
stereo pan (its `x`), and velocity (Doppler) is ignored.

## License

AGPL-3.0-or-later. See [LICENSE](../../LICENSE).
