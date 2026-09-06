//! Driving a synthesizer from the composer: in real time, or offline.
//!
//! [`Conductor`] runs a beat clock. It composes each bar a beat before it
//! is due, turns it into MIDI events on an absolute beat timeline, and
//! dispatches them to the synthesizer as their time comes. Tempo changes
//! move the clock at once; dynamics changes go out as expression
//! controllers at once; everything structural lands on the next bar.
//!
//! [`OfflineRenderer`] does the same with a simulated clock and returns the
//! events with wall-clock times, for rendering to a file.

use core::cmp::Ordering;
extern crate alloc;
use alloc::collections::BinaryHeap;
use std::sync::mpsc::Receiver;
use core::time::Duration;
use std::time::Instant;

use rameau_midi::event::MidiEvent;
use rameau_playback::{AudioPlayback, PlaybackError, Timestamp};
use rameau_synthesizer::Synthesizer;

use crate::composer::{ComposedBar, Composer, DRUM_CHANNEL, STINGER_CHANNEL, Trigger};
use crate::instrument::Instrument;
use crate::state::MusicState;

/// How long an immediately struck bell or cannon is held, in beats, before
/// its scheduled release.
const STINGER_BEATS: f64 = 4.0;

/// An event with a time in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimedEvent {
    /// Seconds from the start.
    pub time: f64,
    /// The event.
    pub event: MidiEvent,
}

/// A command to a running conductor.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// New sliders.
    SetState(MusicState),
    /// A trigger.
    Trigger(Trigger),
    /// Stop playing and return.
    Stop,
}

/// An event on the beat timeline.
#[derive(Debug, Clone, PartialEq)]
struct Scheduled {
    beat: f64,
    /// Ordering at equal beats: note-offs, then programs, then note-ons.
    order: u8,
    seq: u64,
    event: MidiEvent,
}

impl Eq for Scheduled {}

impl PartialOrd for Scheduled {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Scheduled {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed, so the BinaryHeap pops the earliest.
        other
            .beat
            .total_cmp(&self.beat)
            .then(other.order.cmp(&self.order))
            .then(other.seq.cmp(&self.seq))
    }
}

/// Converts a composed bar into scheduled events starting at `start_beat`.
fn schedule_bar(bar: &ComposedBar, start_beat: f64, seq: &mut u64, out: &mut BinaryHeap<Scheduled>) {
    let mut push = |beat: f64, order: u8, event: MidiEvent| {
        *seq += 1;
        out.push(Scheduled { beat, order, seq: *seq, event });
    };
    for &(channel, bank, program) in &bar.programs {
        push(start_beat, 1, MidiEvent::ControlChange { channel, ctrl: 0, value: (bank & 0x7F) as u8 });
        push(start_beat, 1, MidiEvent::ProgramChange { channel, program: program.into() });
    }
    for n in &bar.notes {
        let on = start_beat + n.onset;
        if let Some((bank, program)) = n.program {
            push(on, 1, MidiEvent::ControlChange { channel: n.channel, ctrl: 0, value: (bank & 0x7F) as u8 });
            push(on, 1, MidiEvent::ProgramChange { channel: n.channel, program: program.into() });
        }
        push(on, 2, MidiEvent::NoteOn { channel: n.channel, key: n.key, vel: n.velocity.max(1) });
        push(on + n.duration.max(0.05), 0, MidiEvent::NoteOff { channel: n.channel, key: n.key, vel: 0 });
    }
}

/// Expression controller value for a dynamics slider.
fn expression_for(dynamics: f64) -> u8 {
    (70.0 + 57.0 * dynamics.clamp(0.0, 1.0)) as u8
}

/// A real-time conductor.
pub struct Conductor<P: AudioPlayback> {
    synth: Synthesizer<P>,
    composer: Composer,
    /// Beats of lookahead before a bar is composed.
    pub lookahead_beats: f64,
}

impl<P: AudioPlayback> Conductor<P> {
    /// A conductor driving `synth` from `composer`.
    pub fn new(synth: Synthesizer<P>, composer: Composer) -> Self {
        Self {
            synth,
            composer,
            lookahead_beats: 1.0,
        }
    }

    /// The composer.
    pub fn composer(&mut self) -> &mut Composer {
        &mut self.composer
    }

    /// Runs until a [`Command::Stop`] arrives or the sender hangs up,
    /// calling `on_bar` as each bar begins.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackError`] if the backend rejects an event.
    pub fn run(
        mut self,
        rx: &Receiver<Command>,
        mut on_bar: impl FnMut(&ComposedBar),
    ) -> Result<(), PlaybackError> {
        let mut heap: BinaryHeap<Scheduled> = BinaryHeap::new();
        let mut seq = 0u64;
        let mut beat = 0.0f64;
        let mut next_bar_start = 0.0f64;
        let mut last = Instant::now();
        let mut expression = expression_for(self.composer.state().dynamics);
        for ch in 0..16u8 {
            self.synth.handle(Timestamp::Now, MidiEvent::ControlChange { channel: ch, ctrl: 11, value: expression })?;
        }
        loop {
            // Commands.
            loop {
                match rx.try_recv() {
                    Ok(Command::SetState(s)) => {
                        self.composer.set_state(s);
                        let e = expression_for(s.dynamics);
                        if e != expression {
                            expression = e;
                            for ch in 0..16u8 {
                                self.synth.handle(Timestamp::Now, MidiEvent::ControlChange { channel: ch, ctrl: 11, value: e })?;
                            }
                        }
                    }
                    Ok(Command::Trigger(t)) => {
                        // A bell or a cannon sounds at once rather than
                        // waiting for the next bar; its release is scheduled
                        // on the beat clock so the voice is not left gated.
                        for (channel, key) in self.strike(&t)? {
                            seq += 1;
                            heap.push(Scheduled {
                                beat: beat + STINGER_BEATS,
                                order: 0,
                                seq,
                                event: MidiEvent::NoteOff { channel, key, vel: 0 },
                            });
                        }
                        self.composer.trigger(t);
                    }
                    Ok(Command::Stop) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        for ch in 0..16u8 {
                            self.synth.handle(Timestamp::Now, MidiEvent::AllNotesOff { channel: ch })?;
                        }
                        return Ok(());
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                }
            }
            // Clock.
            let now = Instant::now();
            let dt = now.duration_since(last).as_secs_f64();
            last = now;
            beat += dt * self.composer.state().tempo_bpm / 60.0;

            // Compose ahead.
            if beat + self.lookahead_beats >= next_bar_start {
                let bar = self.composer.next_bar();
                schedule_bar(&bar, next_bar_start, &mut seq, &mut heap);
                next_bar_start += bar.beats;
                on_bar(&bar);
            }
            // Dispatch.
            while let Some(top) = heap.peek() {
                if top.beat > beat {
                    break;
                }
                if let Some(s) = heap.pop() {
                    self.synth.handle(Timestamp::Now, s.event)?;
                }
            }
            std::thread::sleep(Duration::from_micros(800));
        }
    }

    /// Sounds a bell or a cannon at once, and returns the
    /// `(channel, key)` pairs the caller must release: nothing else will
    /// send a note-off for them, so a gated voice would otherwise be left
    /// sounding until the transport stopped or it was stolen.
    fn strike(&mut self, t: &Trigger) -> Result<Vec<(u8, u8)>, PlaybackError> {
        match t {
            Trigger::Tocsin => {
                if let Some(b) = Instrument::by_id("tocsin") {
                    self.synth.handle(Timestamp::Now, MidiEvent::ControlChange { channel: STINGER_CHANNEL, ctrl: 0, value: 0 })?;
                    self.synth.handle(Timestamp::Now, MidiEvent::ProgramChange { channel: STINGER_CHANNEL, program: b.program.into() })?;
                }
                let key = self.composer.key().tonic_midi(72).clamp(0, 127) as u8;
                self.synth.handle(Timestamp::Now, MidiEvent::NoteOn { channel: STINGER_CHANNEL, key, vel: 115 })?;
                Ok(vec![(STINGER_CHANNEL, key)])
            }
            Trigger::Cannon => {
                self.synth.handle(Timestamp::Now, MidiEvent::NoteOn { channel: DRUM_CHANNEL, key: 49, vel: 127 })?;
                self.synth.handle(Timestamp::Now, MidiEvent::NoteOn { channel: DRUM_CHANNEL, key: 35, vel: 127 })?;
                Ok(vec![(DRUM_CHANNEL, 49), (DRUM_CHANNEL, 35)])
            }
            _ => Ok(Vec::new()),
        }
    }
}

/// An offline renderer.
pub struct OfflineRenderer;

impl OfflineRenderer {
    /// Composes `seconds` of music, applying `timeline` commands (sorted by
    /// time) at the bar boundaries after their time, and returns the events
    /// with wall-clock times. `on_bar` sees each bar with its start time.
    pub fn render(
        composer: &mut Composer,
        timeline: &[(f64, Command)],
        seconds: f64,
        mut on_bar: impl FnMut(f64, &ComposedBar),
    ) -> Vec<TimedEvent> {
        let mut heap: BinaryHeap<Scheduled> = BinaryHeap::new();
        let mut seq = 0u64;
        let mut events: Vec<TimedEvent> = Vec::new();
        let mut time = 0.0f64;
        let mut beat = 0.0f64;
        let mut next_cmd = 0usize;
        // Beat -> seconds map, piecewise by bar.
        let mut segments: Vec<(f64, f64, f64)> = Vec::new(); // (start beat, start time, secs per beat)
        while time < seconds {
            while let Some((t, cmd)) = timeline.get(next_cmd)
                && *t <= time + 1e-9
            {
                match cmd {
                    Command::SetState(s) => composer.set_state(*s),
                    Command::Trigger(tr) => composer.trigger(tr.clone()),
                    Command::Stop => {}
                }
                next_cmd += 1;
            }
            let bar = composer.next_bar();
            let spb = 60.0 / bar.tempo_bpm.max(1.0);
            segments.push((beat, time, spb));
            schedule_bar(&bar, beat, &mut seq, &mut heap);
            on_bar(time, &bar);
            beat += bar.beats;
            time += bar.beats * spb;
        }
        // Convert.
        let to_time = |b: f64| -> f64 {
            let seg = segments
                .iter()
                .rev()
                .find(|s| s.0 <= b + 1e-9)
                .or(segments.first())
                .copied()
                .unwrap_or((0.0, 0.0, 0.5));
            seg.1 + (b - seg.0) * seg.2
        };
        while let Some(s) = heap.pop() {
            events.push(TimedEvent { time: to_time(s.beat), event: s.event });
        }
        events
    }

    /// Hands every event to `synth` at its scheduled time (for a backend that
    /// honours timestamps, such as the software mixer).
    ///
    /// # Errors
    ///
    /// Returns the first [`PlaybackError`] the backend produced.
    pub fn schedule<P: AudioPlayback>(synth: &mut Synthesizer<P>, events: &[TimedEvent]) -> Result<(), PlaybackError> {
        for e in events {
            synth.handle(Timestamp::AtSeconds(e.time), e.event)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_struck_stinger_names_a_note_to_release() {
        // The immediate strike is outside the bar scheduler, so nothing else
        // will release it; `strike` must hand back what it gated.
        use crate::idea::IdeaLibrary;
        let mut backend = rameau_software::Software::new(8_000);
        let sf = rameau_soundfont::SoundFont::<alloc::sync::Arc<rameau_clip::Clip<i16>>>::default();
        let _ = &mut backend;
        let synth = rameau_synthesizer::Synthesizer::new(sf, backend, 8_000);
        let mut conductor = Conductor::new(synth, Composer::new(IdeaLibrary::new(), 1));
        let bell = conductor.strike(&Trigger::Tocsin).unwrap();
        assert_eq!(bell.len(), 1);
        assert_eq!(bell[0].0, STINGER_CHANNEL);
        let cannon = conductor.strike(&Trigger::Cannon).unwrap();
        assert_eq!(cannon.len(), 2);
        assert!(cannon.iter().all(|(c, _)| *c == DRUM_CHANNEL));
        assert!(conductor.strike(&Trigger::Cadence).unwrap().is_empty());
    }

    #[test]
    fn heap_orders_offs_before_ons_at_equal_beats() {
        let mut heap = BinaryHeap::new();
        heap.push(Scheduled { beat: 1.0, order: 2, seq: 1, event: MidiEvent::NoteOn { channel: 0, key: 60, vel: 1 } });
        heap.push(Scheduled { beat: 1.0, order: 0, seq: 2, event: MidiEvent::NoteOff { channel: 0, key: 60, vel: 0 } });
        heap.push(Scheduled { beat: 0.5, order: 2, seq: 3, event: MidiEvent::NoteOn { channel: 0, key: 62, vel: 1 } });
        let first = heap.pop().unwrap();
        assert_eq!(first.beat, 0.5);
        let second = heap.pop().unwrap();
        assert!(matches!(second.event, MidiEvent::NoteOff { .. }));
    }
}
