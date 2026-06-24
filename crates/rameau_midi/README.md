# rameau_midi

MIDI types and Standard MIDI File (SMF) parsing.

- `event::MidiEvent` — a decoded channel-voice / channel-mode event.
- `program::MidiProgram` — a General MIDI program number, plus the
  `midi_program!` macro for naming banks of instruments.
- `smf::Smf` — `Smf::parse` turns the bytes of a `.mid`/`.midi` file into a
  header (format + time division) and a list of tracks. Running status,
  variable-length quantities, meta events and system exclusive are all handled.
  `Smf::timed_events` flattens a parsed file into a `(seconds, MidiEvent)`
  timeline ready to feed a synthesizer.
- `error::MidiError` — parse error type.

```rust,no_run
use rameau_midi::smf::Smf;

let bytes = std::fs::read("song.mid")?;
let smf = Smf::parse(&bytes)?;
for (secs, event) in smf.timed_events() {
    // dispatch `event` at `secs`
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Features

- `serde` — serializable event/file types via serde.

## License

AGPL-3.0-or-later. See [LICENSE](../../LICENSE).
