# rameau_playback

Backend-independent audio playback traits for the rameau workspace. It defines
two layers and no backend of its own:

- **Device layer** — `Playback` + `PlaybackConfig`: "hand an output device a
  callback that fills interleaved `f32` buffers on demand". Implemented by
  backends such as [`rameau_tinyaudio`](../rameau_tinyaudio).
- **Sample-engine layer** — `AudioPlayback`: a higher-level voice engine
  (start / update / stop / render of voices, with `VoiceParams`, `Timestamp`,
  `LoopRegion` and `Vec3`). Implemented by [`rameau_kira`](../rameau_kira) and
  [`rameau_software`](../rameau_software).

## Graceful degradation

Whole missing capabilities return `PlaybackError::Unsupported` (e.g. offline
`render` on a real-time backend), while per-voice parameters degrade silently — a
backend without spatial audio collapses `position` to a stereo pan and ignores
`velocity`.

## License

AGPL-3.0-or-later. See [LICENSE](../../LICENSE).
