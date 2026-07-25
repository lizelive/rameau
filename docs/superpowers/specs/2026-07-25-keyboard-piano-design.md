# Keyboard piano demo, and fixing the distortion behind it

## Problem

The `interactive` example sounds bad, for two independent reasons.

**The signal path is unbounded.** `Software::render` sums voices straight into
the output buffer with no limit, and the example copies that buffer to the
device unchanged. Nothing bounds voice count either: `resolve_voices` returns an
unbounded `Vec`, so held notes accumulate without limit.

Measured against `FluidR3Mono_GM.sf3`, though, this is not what made the old demo
sound bad. Ordinary playing stays comfortably clean:

| case | peak, unlimited |
| --- | --- |
| 4-note piano chord | 0.261 |
| 4-note square lead chord | 0.253 |
| 4-note sawtooth lead chord | 0.476 |
| 49 keys held, piano | 0.739 |
| 49 keys held, square lead | **1.254** |

Only the extreme case clips, and the old demo — a three-note chord, a bass note
and a few typed notes — never came close to it. The limiter is therefore
protection for a case this work newly makes reachable rather than a fix for
audible distortion: wiring up the sustain pedal turns "hold the pedal and keep
playing" into an ordinary gesture, and that is precisely the 1.254 case.

Because normal levels have this much headroom already, the master gain defaults
to unity. Pre-attenuating would throw away level the limiter never needed.

**The demo is not an instrument**, and per the measurements above this is the
whole of why it sounds bad. Input is line-buffered stdin, so every note in a
typed line starts on the *same sample* and is cut off at exactly 0.4 s — a
machine-gun staccato with no sustain and no dynamics. The mapping is diatonic
only, so black keys cannot be played at all. Velocity reaches the voice as a
linear `vel / 127`, which leaves soft notes far too loud and flattens everything
to one level. A square-wave lead (program 80) sits over a looping I-V-vi-IV
backing score, whose bass plays on channel 2 — a channel that never receives a
program change, so it sounds on whatever preset happened to be default.

## Goals

Replace the demo with a playable keyboard piano driven by a MIDI controller,
with a full-featured computer-keyboard fallback, and fix the distortion in the
mixer so the output is clean.

## Design

### 1. Peak limiter and voice cap (`rameau_software`)

A new `limiter.rs` holds a feed-forward brickwall limiter: instant attack,
exponential release, no lookahead.

```
target = if peak > threshold { threshold / peak } else { 1.0 }
if target < gain { gain = target }                  // instant attack
else { gain += (target - gain) * release_coef }     // smooth recovery
```

`peak` is `max(|left|, |right|)` for the frame, measured after master gain, and
the resulting gain is applied to that same frame. Applying the gain to the frame
that produced the measurement is what makes this a true brickwall without a
lookahead buffer. The cost is a small amount of transient distortion on instant
gain drops, which is far below what hard clipping produces today.

Signal order in `Software::render`: sum voices → `× master_gain` → limiter →
output.

Defaults: `master_gain` 1.0, `threshold` 0.95, release ~100 ms. Configurable
through `with_master_gain` and `with_limiter`; `with_limiter(None)` restores the
raw linear sum for callers who need bit-exact offline renders.

With these defaults the limiter is transparent until it is needed: a four-note
piano chord measures 0.261 either way, while the 49-key square lead case is
pulled from 1.254 down to the 0.950 ceiling.

The voice cap addresses the root cause rather than the symptom. `max_voices`
defaults to 64. On `start`, if the number of *non-releasing* voices is at the
cap, the oldest is stolen by forcing a 5 ms release. Stealing by release rather
than by removal avoids the click that cutting a voice mid-waveform produces.
This requires `Envelope::set_release` and `Voice::is_releasing`.

### 2. Live MIDI decoding (`rameau_midi`)

```rust
impl MidiEvent {
    pub fn from_bytes(bytes: &[u8]) -> Result<MidiEvent, MidiError>;
}
```

Decodes one complete channel message with no running status, which is exactly
what midir delivers. Placed in `smf.rs` beside the existing byte-level code so it
can reuse `parse_channel` and keep `Reader` private. System messages
(`0xF0`–`0xFF`) fall through to the existing `MidiError::BadValue`; callers
ignore errors, so clock and active-sensing traffic is skipped silently.

### 3. Velocity curve (`rameau_synthesizer`)

`params_of` applies velocity as a linear `vel / 127`, where the SoundFont spec
calls for a dB-shaped response. Squaring the ratio approximates it. Soft notes
become genuinely soft, which is most of what makes a keyboard feel like an
instrument.

### 4. The demo (`examples/piano/`)

Three files, split along the seam introduced by having two input backends.

**`input.rs`** owns both backends and is the only place that knows about octave
and velocity state, keeping the audio thread free of UI state:

```rust
enum Command { NoteOn { key: u8, vel: u8 }, NoteOff { key: u8 },
               Sustain(bool), Program(u8), Panic, Quit }
```

midir connects to every available input port, or to one named by `--midi <name>`,
decodes bytes through `from_bytes`, and forwards note and CC64 messages. The
connection guard is returned and held for the process lifetime; dropping it
silently kills input.

The computer keyboard uses crossterm in raw mode with a chromatic layout:

```
lower octave:  z s x d c v g b h n j m ,     (z=C, s=C#, x=D, d=D#, c=E, …)
upper octave:  q 2 w 3 e r 5 t 6 y 7 u i
```

Two correctness details. A `HashMap<char, u8>` records which key number each
character actually triggered, so a note-off issued after an octave change
releases the note that is sounding rather than one that was never played. And a
repeat press on an already-held character is dropped, so auto-repeat does not
retrigger.

Key release reporting is the one portability question. The Windows console
reports `KeyEventKind::Release`, so the primary path is true note-on/note-off
with held sustain. Where releases are not reported, the fallback is a
repeat-timeout gate: the note sustains while auto-repeat continues and releases
~150 ms after it stops. The demo prints which mode it obtained. Space is a hold
pedal when releases work and a toggle when they do not.

Other controls: `←`/`→` velocity ∓8, `↓`/`↑` octave ∓1, `[`/`]` GM program ∓1
(printing `MidiProgram::get_name()`), `.` panic, `Esc` quit. Esc rather than `q`,
because `q` is now a note.

Raw mode needs a `Drop` guard and a panic hook that restores the terminal;
otherwise a panic in raw mode leaves the user's shell unusable.

**`audio.rs`** builds the render closure: drain commands, schedule at
`block_start`, render. The `pending` note-off list is deleted, since real
note-offs now arrive from input.

**`main.rs`** handles argument parsing, SoundFont loading (reusing the existing
candidate-path logic), program 0 on channel 0, stream setup, and help text.

### 5. Dependencies and cleanup

`midir` and `crossterm` become dev-dependencies of `rameau_synthesizer`. The
`[[example]]` block is repointed to `examples/piano/main.rs`. `interactive.rs` is
deleted and `README.md:31` updated.

## Testing

The library changes carry the behaviour, and all of it is testable:

- Output peak stays ≤ threshold under many loud voices.
- A quiet signal passes with no gain reduction.
- Active voices stay bounded at the cap.
- The oldest voice is the one released on overflow.
- `from_bytes` decodes each channel message and rejects truncated input and sysex.
- Low velocity is quieter than the linear curve would have produced.

Existing tests assert only `rms > 0.0`, `rms == 0.0`, or voice counts — no test
in the workspace asserts exact amplitudes — so making limiting the default does
not require rewriting expectations.

The example itself can only be verified by `cargo build --examples` plus manual
play: `cargo test` does not run tests inside example targets.

## Addendum: why nothing was audible at all

The first build of the piano produced no sound, and instrumenting each boundary
of the chain found a cause that predated this work entirely.

Per-block measurements showed the audio callback executing in at most 101 µs
while owing 5,333 µs of audio, and producing only 0.14 s of audio across 2.5 s
of wall time. The callback was not slow — it was 50× faster than required. The
device simply was not calling it.

On Windows `tinyaudio` uses DirectSound, which runs a double buffer and signals
its feed thread once per half. With `frames_per_buffer: 256` at 48 kHz that
thread must wake every 5.3 ms, well inside Windows' ~10-16 ms scheduling
granularity. It misses notifications, the buffer wraps over stale audio, and
output arrives at a fraction of real time without any error being reported.

A sweep confirmed the model, which says the half-buffer period must exceed
about 10 ms — 480 frames at 48 kHz:

| `frames_per_buffer` | period | audio produced per 2.5 s wall |
| --- | --- | --- |
| 256 | 5.3 ms | 0.14 s |
| 384 | 8.0 ms | 0.51 s |
| 512 | 10.7 ms | 2.53 s (real time) |
| 768 | 16.0 ms | 2.53 s (real time) |

The old `interactive` demo also used 256 frames, so it was starved in exactly
the same way. This, not the mix, is why it "sounded horrible".

Two changes follow. The demo moves to 512 frames. And `rameau_tinyaudio`
rejects any buffer below `min_frames_per_buffer(sample_rate)` rather than
opening a stream that silently starves — a loud failure is worth far more than
a device that plays a twentieth of its audio.

## Decisions deliberately not taken

- No lookahead limiter. It would need a delay buffer threaded through a render
  path that writes into the caller's buffer, for a transparency gain no one can
  hear in a demo.
- No backing score. The demo is an instrument you play, not a backing track.
