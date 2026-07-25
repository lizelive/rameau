# Keyboard piano demo, and fixing the distortion behind it

## Problem

The `interactive` example sounds bad, for two independent reasons.

**The signal path distorts.** `Software::render` sums voices straight into the
output buffer with no headroom and no limit, and the example copies that buffer
to the device unchanged. A three-note chord plus a bass line, across presets
whose zones each contribute several voices, exceeds ±1.0 and hard-clips at the
device. Nothing bounds voice count either: `resolve_voices` returns an unbounded
`Vec`, so held notes accumulate without limit. `render_score.rs` already
peak-normalises its output, working around the same defect downstream.

**The demo is not an instrument.** Input is line-buffered stdin, so notes fire in
typed bursts at a fixed 0.4 s gate with no sustain and no dynamics. The mapping
is diatonic only, so black keys cannot be played at all. A square-wave lead
(program 80) sits over a looping I-V-vi-IV backing score, whose bass plays on
channel 2 — a channel that never receives a program change.

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

Defaults: `master_gain` 0.7 (−3 dB of headroom, so the limiter is not engaged
constantly — constant engagement is what makes a limiter pump), `threshold`
0.95, release ~100 ms. Configurable through `with_master_gain` and
`with_limiter`; `with_limiter(None)` restores the raw linear sum for callers who
need bit-exact offline renders.

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

## Decisions deliberately not taken

- No lookahead limiter. It would need a delay buffer threaded through a render
  path that writes into the caller's buffer, for a transparency gain no one can
  hear in a demo.
- No backing score. The demo is an instrument you play, not a backing track.
