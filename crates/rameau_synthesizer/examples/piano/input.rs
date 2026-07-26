//! Input backends: a MIDI controller via `midir`, and the computer keyboard via
//! `crossterm` raw mode.
//!
//! Both funnel into the same [`Command`] channel. This module is the only place
//! that knows about octave and velocity state, so the audio thread receives
//! final MIDI key numbers and never has to reason about UI state.

use std::collections::HashMap;
use std::sync::mpsc::Sender;
use core::time::Duration;
use std::time::Instant;

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement};
use crossterm::{execute, queue};
use midir::{MidiInput, MidiInputConnection};
use rameau_midi::event::MidiEvent;
use rameau_midi::program::MidiProgram;

/// A message from an input backend to the audio callback.
#[derive(Debug, Clone, Copy)]
pub enum Command {
    NoteOn { key: u8, vel: u8 },
    NoteOff { key: u8 },
    /// Sustain pedal (CC64) down or up.
    Sustain(bool),
    /// Select the General MIDI program.
    Program(u8),
    /// All notes off, everywhere.
    Panic,
}

/// How long a held computer key sustains after its last auto-repeat, when the
/// terminal does not report key releases.
const REPEAT_GATE: Duration = Duration::from_millis(150);

/// Shortest time a typed note is allowed to sound.
///
/// A terminal keystroke does not always carry a duration. Under ConPTY — the
/// VS Code integrated terminal — a press and its release arrive together, so
/// the note would start and stop on the same audio frame: measured through a
/// GM bank, a same-frame note-off renders an RMS of exactly 0.0, where the
/// same note held 100 ms renders 0.020. That is silence, and it is why typing
/// produced nothing there while a MIDI controller worked.
///
/// This floor applies only to the computer keyboard. A MIDI controller reports
/// real note lengths, so its note-offs pass through untouched.
const MIN_NOTE: Duration = Duration::from_millis(120);

/// Lowest octave the computer keyboard can be shifted to (MIDI key of its C).
const MIN_OCTAVE_KEY: i32 = 0;
/// Highest, leaving room for the upper row plus its top C.
const MAX_OCTAVE_KEY: i32 = 96;

/// Semitone offsets of the computer keyboard's two chromatic rows.
///
/// Laid out like a piano: the lower row's letters are the white keys and the
/// row above them the black keys, so accidentals are actually playable — the
/// previous diatonic mapping had no way to reach them at all.
const LOWER_ROW: &[(char, i32)] = &[
    ('z', 0),  // C
    ('s', 1),  // C#
    ('x', 2),  // D
    ('d', 3),  // D#
    ('c', 4),  // E
    ('v', 5),  // F
    ('g', 6),  // F#
    ('b', 7),  // G
    ('h', 8),  // G#
    ('n', 9),  // A
    ('j', 10), // A#
    ('m', 11), // B
    (',', 12), // C
];

const UPPER_ROW: &[(char, i32)] = &[
    ('q', 12), // C, one octave above the lower row
    ('2', 13),
    ('w', 14),
    ('3', 15),
    ('e', 16),
    ('r', 17),
    ('5', 18),
    ('t', 19),
    ('6', 20),
    ('y', 21),
    ('7', 22),
    ('u', 23),
    ('i', 24),
];

/// The semitone offset a character plays, if it is a note key.
fn semitone_for(ch: char) -> Option<i32> {
    LOWER_ROW
        .iter()
        .chain(UPPER_ROW)
        .find(|&&(c, _)| c == ch)
        .map(|&(_, s)| s)
}

// ---------------------------------------------------------------------------
// MIDI controller
// ---------------------------------------------------------------------------

/// Live MIDI connections. Dropping this silently stops input, so the caller
/// must hold it for as long as it wants to play.
pub struct MidiInputs {
    _connections: Vec<MidiInputConnection<()>>,
    /// Names of the ports that were opened.
    pub ports: Vec<String>,
}

/// Connects to MIDI input ports and forwards note and pedal messages to `tx`.
///
/// With no `filter`, every available port is opened, so whichever controller is
/// plugged in just works. With one, only ports whose name contains it (case
/// insensitively) are opened.
pub fn connect_midi(tx: &Sender<Command>, filter: Option<&str>) -> Result<MidiInputs, String> {
    let input = MidiInput::new("rameau piano").map_err(|e| e.to_string())?;
    let mut connections = Vec::new();
    let mut names = Vec::new();

    for port in input.ports() {
        // Each connection consumes its own MidiInput, so build one per port.
        let scan = MidiInput::new("rameau piano").map_err(|e| e.to_string())?;
        let Ok(name) = scan.port_name(&port) else {
            continue;
        };
        if let Some(f) = filter
            && !name.to_lowercase().contains(&f.to_lowercase())
        {
            continue;
        }

        let tx = tx.clone();
        match scan.connect(
            &port,
            "rameau-piano",
            move |_stamp, bytes, _| {
                // Anything that is not a channel message — clock, sysex, active
                // sensing — fails to decode and is simply skipped.
                if let Ok(ev) = MidiEvent::from_bytes(bytes) {
                    forward(&tx, ev);
                }
            },
            (),
        ) {
            Ok(conn) => {
                connections.push(conn);
                names.push(name);
            }
            // A port already held open by another application is normal; the
            // remaining ports are still worth trying.
            Err(_) => continue,
        }
    }

    Ok(MidiInputs {
        _connections: connections,
        ports: names,
    })
}

/// Translates an incoming MIDI event into a [`Command`], dropping the rest.
fn forward(tx: &Sender<Command>, ev: MidiEvent) {
    let cmd = match ev {
        // A note-on with zero velocity is the conventional note-off.
        MidiEvent::NoteOn { key, vel: 0, .. } | MidiEvent::NoteOff { key, .. } => {
            Command::NoteOff { key }
        }
        MidiEvent::NoteOn { key, vel, .. } => Command::NoteOn { key, vel },
        MidiEvent::ControlChange { ctrl: 64, value, .. } => Command::Sustain(value >= 64),
        MidiEvent::ProgramChange { program, .. } => Command::Program(program.into()),
        MidiEvent::AllNotesOff { .. } | MidiEvent::AllSoundOff { .. } => Command::Panic,
        _ => return,
    };
    let _ = tx.send(cmd);
}

// ---------------------------------------------------------------------------
// Computer keyboard
// ---------------------------------------------------------------------------

/// Restores the terminal when the keyboard loop ends, however it ends.
struct RawMode {
    enhanced: bool,
}

impl RawMode {
    fn enter() -> std::io::Result<Self> {
        enable_raw_mode()?;
        // Ask for key-release reporting. Windows consoles report releases
        // natively; on other platforms this is the Kitty protocol extension,
        // and terminals that do not implement it simply ignore the request.
        let enhanced = execute!(
            std::io::stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::REPORT_EVENT_TYPES)
        )
        .is_ok()
            && supports_keyboard_enhancement().unwrap_or(false);
        Ok(Self { enhanced })
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        if self.enhanced {
            let _ = queue!(std::io::stdout(), PopKeyboardEnhancementFlags);
        }
        let _ = disable_raw_mode();
    }
}

/// Tracks whether this terminal actually reports key releases.
///
/// This cannot be decided up front. A classic Windows console delivers release
/// records, but the same binary under ConPTY — the VS Code integrated terminal,
/// Windows Terminal — sees input translated to escape sequences and may get no
/// releases at all, and on Unix it depends on the Kitty protocol. Assuming
/// releases and being wrong is the worst case: the auto-repeat fallback stays
/// disabled and notes never stop.
///
/// So assume nothing and watch. Until a release is actually observed the
/// repeat-gate fallback runs, which stops notes in every terminal. The first
/// genuine release switches to precise release-driven note-offs.
#[derive(Debug, Default)]
struct ReleaseSupport {
    seen: bool,
}

impl ReleaseSupport {
    /// Records that a real release event arrived.
    fn observed(&mut self) -> bool {
        let first = !self.seen;
        self.seen = true;
        first
    }

    /// Whether note-offs can be left to release events alone.
    fn trusted(&self) -> bool {
        self.seen
    }
}

/// Mutable state for the keyboard: which characters are sounding, and the
/// octave and velocity the next note will use.
struct Keyboard {
    /// Character -> the MIDI key it actually triggered. Recording the key that
    /// was sent (rather than recomputing it) means a note-off issued after an
    /// octave shift releases the note that is sounding, not one never played.
    held: HashMap<char, HeldNote>,
    /// When each held character last produced a press, for the repeat-gate
    /// fallback on terminals without release events.
    last_press: HashMap<char, Instant>,
    /// Note-offs held back until their note has sounded for [`MIN_NOTE`].
    deferred_off: Vec<(Instant, u8)>,
    /// MIDI key of the lower row's C.
    octave_key: i32,
    vel: u8,
    program: u8,
    /// Sustain pedal state, for the toggle fallback.
    sustain: bool,
}

/// A note currently sounding from a held character.
#[derive(Clone, Copy)]
struct HeldNote {
    key: u8,
    struck: Instant,
}

impl Keyboard {
    fn new() -> Self {
        Self {
            held: HashMap::new(),
            last_press: HashMap::new(),
            deferred_off: Vec::new(),
            octave_key: 60, // middle C
            vel: 96,
            program: 0,
            sustain: false,
        }
    }

    fn note_on(&mut self, tx: &Sender<Command>, ch: char, semitone: i32) {
        self.last_press.insert(ch, Instant::now());
        // Auto-repeat re-sends a press for a key that is already down; the note
        // must not retrigger.
        if self.held.contains_key(&ch) {
            return;
        }
        let key = (self.octave_key + semitone).clamp(0, 127) as u8;
        // If this key is struck again while its previous release is still
        // pending, that release now belongs to a note that is gone; letting it
        // fire would cut the new one short.
        self.deferred_off.retain(|&(_, pending)| pending != key);
        self.held.insert(
            ch,
            HeldNote {
                key,
                struck: Instant::now(),
            },
        );
        // Echo the note. Besides being pleasant to watch, this is the fastest
        // way to tell a dead keyboard from dead audio: if names appear and
        // nothing sounds, the problem is downstream of input.
        status(&format!("{} ({key})", note_name(key)));
        let _ = tx.send(Command::NoteOn { key, vel: self.vel });
    }

    fn note_off(&mut self, tx: &Sender<Command>, ch: char) {
        self.last_press.remove(&ch);
        let Some(note) = self.held.remove(&ch) else {
            return;
        };
        // A terminal keystroke does not always carry a duration: under ConPTY
        // the press and release arrive together, which would stop the note on
        // the same frame it started and produce silence. Give it a floor.
        let elapsed = note.struck.elapsed();
        if elapsed >= MIN_NOTE {
            let _ = tx.send(Command::NoteOff { key: note.key });
        } else {
            self.deferred_off.push((note.struck + MIN_NOTE, note.key));
        }
    }

    /// Sends any deferred note-offs that have now come due.
    fn flush_deferred(&mut self, tx: &Sender<Command>) {
        let now = Instant::now();
        self.deferred_off.retain(|&(due, key)| {
            if due <= now {
                let _ = tx.send(Command::NoteOff { key });
                false
            } else {
                true
            }
        });
    }

    /// Releases held notes whose auto-repeat has stopped. Only used where the
    /// terminal cannot tell us about releases directly.
    fn expire_held(&mut self, tx: &Sender<Command>) {
        let now = Instant::now();
        let stale: Vec<char> = self
            .last_press
            .iter()
            .filter(|&(_, &t)| now.duration_since(t) > REPEAT_GATE)
            .map(|(&c, _)| c)
            .collect();
        for ch in stale {
            self.note_off(tx, ch);
        }
    }

    fn all_off(&mut self, tx: &Sender<Command>) {
        self.held.clear();
        self.last_press.clear();
        // Drop pending releases too: Panic silences those notes anyway, and a
        // stale note-off arriving later would cut a note struck after it.
        self.deferred_off.clear();
        let _ = tx.send(Command::Panic);
    }

    fn shift_octave(&mut self, delta: i32) {
        self.octave_key = (self.octave_key + delta * 12).clamp(MIN_OCTAVE_KEY, MAX_OCTAVE_KEY);
        status(&format!(
            "octave: lower row starts at {}",
            note_name(self.octave_key as u8)
        ));
    }

    fn shift_velocity(&mut self, delta: i32) {
        self.vel = (self.vel as i32 + delta).clamp(1, 127) as u8;
        status(&format!("velocity: {}", self.vel));
    }

    fn shift_program(&mut self, tx: &Sender<Command>, delta: i32) {
        self.program = (self.program as i32 + delta).clamp(0, 127) as u8;
        let _ = tx.send(Command::Program(self.program));
        status(&format!(
            "program {}: {}",
            self.program,
            MidiProgram::from(self.program).get_name()
        ));
    }
}

/// Scientific pitch name for a MIDI key, e.g. 60 -> "C4".
fn note_name(key: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let name = NAMES.get(key as usize % 12).copied().unwrap_or("?");
    format!("{}{}", name, key as i32 / 12 - 1)
}

/// Prints a line in raw mode, where a bare newline does not return the cursor.
fn status(msg: &str) {
    print!("\r  {msg}\r\n");
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

/// Reports what this terminal actually delivers, and echoes every event.
///
/// When keys do nothing, the useful question is whether the events arrive at
/// all. This answers it without any of the synth in the way.
pub fn debug_input() -> std::io::Result<()> {
    println!("terminal diagnostics:");
    println!(
        "  stdin is a terminal:      {}",
        std::io::IsTerminal::is_terminal(&std::io::stdin())
    );
    println!(
        "  keyboard enhancement:     {:?}",
        supports_keyboard_enhancement()
    );

    let raw = match RawMode::enter() {
        Ok(r) => r,
        Err(e) => {
            println!("  raw mode:                 FAILED ({e})");
            println!();
            println!("Raw mode is required to read keys. This usually means the program");
            println!("is not attached to a real terminal - check that it is not running");
            println!("through a pipe, a task runner, or an IDE output pane.");
            return Ok(());
        }
    };
    println!("  raw mode:                 ok");
    println!("  release events requested: {}", raw.enhanced);
    println!();
    status("press keys - every event is echoed. Esc to finish.");

    loop {
        if !event::poll(Duration::from_millis(500))? {
            status("(no event in 500ms)");
            continue;
        }
        let ev = event::read()?;
        status(&format!("{ev:?}"));
        if let Event::Key(KeyEvent {
            code: KeyCode::Esc, ..
        }) = ev
        {
            break;
        }
    }
    Ok(())
}

/// Runs the computer keyboard until Esc (or Ctrl-C).
///
/// Blocks, so the caller should treat this as the demo's main loop.
pub fn run_keyboard(tx: &Sender<Command>) -> std::io::Result<()> {
    let _raw = RawMode::enter()?;
    let mut releases = ReleaseSupport::default();
    status("notes sustain while held; the pedal is space");

    let mut kb = Keyboard::new();
    loop {
        // Until releases are known to work the gate has to be re-checked on a
        // timer, so poll rather than block; afterwards this just costs a wakeup.
        // Polling rather than blocking so deferred note-offs (and the repeat
        // gate) come due on time even while no keys are being pressed.
        if !event::poll(Duration::from_millis(20))? {
            kb.flush_deferred(tx);
            if !releases.trusted() {
                kb.expire_held(tx);
            }
            continue;
        }

        let Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            ..
        }) = event::read()?
        else {
            continue;
        };

        if kind == KeyEventKind::Release {
            if releases.observed() {
                // Now that releases are known to arrive, the timer fallback can
                // stand down and held notes sustain for exactly as long as held.
                status("held keys sustain (this terminal reports key releases)");
            }
            if let KeyCode::Char(ch) = code {
                let ch = ch.to_ascii_lowercase();
                if semitone_for(ch).is_some() {
                    kb.note_off(tx, ch);
                }
            }
            if code == KeyCode::Char(' ') {
                let _ = tx.send(Command::Sustain(false));
            }
            continue;
        }

        // Ctrl-C would otherwise be swallowed by raw mode.
        if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
            break;
        }

        match code {
            KeyCode::Esc => break,
            KeyCode::Char(' ') => {
                if releases.trusted() {
                    let _ = tx.send(Command::Sustain(true));
                } else {
                    kb.sustain = !kb.sustain;
                    let _ = tx.send(Command::Sustain(kb.sustain));
                    status(if kb.sustain { "pedal down" } else { "pedal up" });
                }
            }
            KeyCode::Char('.') => {
                kb.all_off(tx);
                status("panic");
            }
            KeyCode::Up => kb.shift_octave(1),
            KeyCode::Down => kb.shift_octave(-1),
            KeyCode::Right => kb.shift_velocity(8),
            KeyCode::Left => kb.shift_velocity(-8),
            KeyCode::Char(']') => kb.shift_program(tx, 1),
            KeyCode::Char('[') => kb.shift_program(tx, -1),
            KeyCode::Char(ch) => {
                let ch = ch.to_ascii_lowercase();
                if let Some(semitone) = semitone_for(ch) {
                    kb.note_on(tx, ch, semitone);
                }
            }
            _ => {}
        }

        kb.flush_deferred(tx);
        if !releases.trusted() {
            kb.expire_held(tx);
        }
    }

    kb.all_off(tx);
    Ok(())
}
