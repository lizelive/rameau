# rameau_synthesizer

A SoundFont synthesizer that *drives* an `AudioPlayback` backend.

`Synthesizer` is the MIDI brain, not the audio engine. It owns a `SoundFont` and
per-channel controller state, resolves each note-on through the SoundFont
generator hierarchy (preset zone → instrument → sample) — handling key/velocity
ranges, tuning, panning, attenuation, looping, exclusive classes, sustain pedal
and voice stealing — and turns the result into `start` / `update` / `stop` calls
on a pluggable backend `P`. The backend (software or a real-time engine) owns
resampling, pitch, mixing and output.

Because samples are stored as the backend's own clip type (`P::Clip`), load the
bank *with* the backend:

```rust,no_run
use rameau_playback::Timestamp;
use rameau_midi::event::MidiEvent;
use rameau_soundfont::SoundFont;
use rameau_synthesizer::Synthesizer;

let mut backend = rameau_software::Software::new(44_100);
let sf = SoundFont::load_file_with("bank.sf2", &mut backend)?;
let mut synth = Synthesizer::new(sf, backend, 44_100);
synth.handle(Timestamp::Now, MidiEvent::NoteOn { channel: 0, key: 60, vel: 100 })?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Examples

- `interactive` — play notes from the keyboard.
- `render_score` — render a score offline to audio.
- `midi_play` — play a `.mid` file through the kira backend.

## License

AGPL-3.0-or-later. See [LICENSE](../../LICENSE).
