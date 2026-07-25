//! The real-time audio callback.
//!
//! The callback owns the clock: each block it drains whatever input has arrived,
//! schedules it on the synth at a sample-accurate [`Timestamp::AtSeconds`], and
//! renders the backend into the output buffer. Because the software backend's
//! clock advances in lockstep with the callback, timing never drifts with buffer
//! size.

use std::sync::mpsc::Receiver;

use rameau_clip::Clip;
use rameau_midi::event::MidiEvent;
use rameau_playback::Timestamp;
use rameau_software::Software;
use rameau_soundfont::SoundFont;
use rameau_synthesizer::Synthesizer;

use crate::input::Command;

/// The MIDI channel the piano plays on.
pub const CHANNEL: u8 = 0;

type Backend = Software;
type Bank = SoundFont<<Software as rameau_playback::AudioPlayback>::Clip>;

/// Builds the render closure driving `synth` from `rx`.
pub fn render_callback(
    bank: Bank,
    backend: Backend,
    sample_rate: u32,
    buffer_len: usize,
    rx: Receiver<Command>,
    program: u8,
) -> impl FnMut(&mut [f32]) {
    let mut synth = Synthesizer::new(bank, backend, sample_rate);
    let mut scratch = Clip::new(vec![0.0f32; buffer_len], sample_rate);
    let mut clock: u64 = 0;
    // Applied on the first block, once the synth is live on the audio thread.
    let mut bootstrap = Some(MidiEvent::ProgramChange {
        channel: CHANNEL,
        program: program.into(),
    });

    move |buf: &mut [f32]| {
        let block_start = clock;
        let when = Timestamp::AtSeconds(block_start as f64 / sample_rate as f64);

        if let Some(ev) = bootstrap.take() {
            let _ = synth.handle(when, ev);
        }

        // Everything that arrived since the last block lands at its start. The
        // resulting jitter is bounded by one buffer — about 5 ms — which is
        // below the threshold where playing feels laggy.
        while let Ok(cmd) = rx.try_recv() {
            let (first, second) = events_for(cmd);
            let _ = synth.handle(when, first);
            if let Some(ev) = second {
                let _ = synth.handle(when, ev);
            }
        }

        scratch.data.resize(buf.len(), 0.0);
        let _ = synth.render(&mut scratch);
        buf.copy_from_slice(&scratch.data);

        clock = block_start + (buf.len() / 2) as u64;
    }
}

/// The MIDI events a command expands to.
///
/// Returned as a pair rather than a `Vec` because this runs on the audio
/// thread, where an allocation can block long enough to drop out.
fn events_for(cmd: Command) -> (MidiEvent, Option<MidiEvent>) {
    let pedal = |value| MidiEvent::ControlChange {
        channel: CHANNEL,
        ctrl: 64,
        value,
    };
    match cmd {
        Command::NoteOn { key, vel } => (
            MidiEvent::NoteOn {
                channel: CHANNEL,
                key,
                vel,
            },
            None,
        ),
        Command::NoteOff { key } => (
            MidiEvent::NoteOff {
                channel: CHANNEL,
                key,
                vel: 0,
            },
            None,
        ),
        // The synth already implements CC64: it defers note-offs while the
        // pedal is down and releases them together when it comes up.
        Command::Sustain(down) => (pedal(if down { 127 } else { 0 }), None),
        Command::Program(p) => (
            MidiEvent::ProgramChange {
                channel: CHANNEL,
                program: p.into(),
            },
            None,
        ),
        // Lift the pedal too, or notes it is holding survive the panic.
        Command::Panic => (pedal(0), Some(MidiEvent::AllNotesOff { channel: CHANNEL })),
    }
}
