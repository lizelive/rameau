//! Forms: the plan for each bar.
//!
//! A [`Form`] is a small state machine that, bar by bar, decides which idea
//! sounds in which voice under which mutation, in which key, over which
//! chords, and where the phrase cadences. It never writes free voices; that
//! is the annealer's job. Five forms are implemented:
//!
//! * [`Fugue`] — exposition, episodes, middle entries in related keys,
//!   stretto, final entry over a pedal;
//! * [`Rondeau`] — refrain and couplets;
//! * [`Chaconne`] — variations over a ground bass;
//! * [`Air`] — a tune, sung when there are words, with accompaniment;
//! * [`Contredanse`] — a strain-form dance.

use rameau_chords::{Cadence, Grammar, RomanNumeral, StockProgression};
use rameau_theory::{Meter, Midi, Mode, Scale};
use rameau_types::{Rng, SplitMix64};
use rameau_voix::Vowel;

use crate::harmonize::{self, ChordSlot, MelodyNote};
use crate::idea::{Idea, IdeaEvent, IdeaKind, IdeaLibrary};
use crate::instrument::Role;
use crate::mutate::{Mutation, Variant};
use crate::state::{FormKind, MusicState, Phase};

/// A note placed on the bar grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedNote {
    /// First slot.
    pub slot: usize,
    /// Length in slots.
    pub len: usize,
    /// MIDI key.
    pub midi: Midi,
    /// Whether the note began in an earlier bar (no new attack).
    pub tied: bool,
    /// Ornament sign carried from the source.
    pub ornament: Option<char>,
}

/// What a voice plays in a bar.
#[derive(Debug, Clone, PartialEq)]
pub enum Material {
    /// Fixed notes from an idea.
    Fixed(Vec<PlacedNote>),
    /// Written by the annealer.
    Free,
    /// Silent.
    Rest,
}

/// One voice's assignment for a bar.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceAssignment {
    /// The role.
    pub role: Role,
    /// Playing range for the annealer.
    pub range: (Midi, Midi),
    /// The material.
    pub material: Material,
    /// Density target as a fraction of the voice's slots (free voices).
    pub density: f64,
    /// The variant label, when fixed.
    pub label: Option<String>,
}

/// The plan for one bar.
#[derive(Debug, Clone, PartialEq)]
pub struct BarPlan {
    /// Key.
    pub key: Scale,
    /// Meter.
    pub meter: Meter,
    /// Chords.
    pub chords: Vec<ChordSlot>,
    /// Voices, highest first.
    pub voices: Vec<VoiceAssignment>,
    /// Section label for display.
    pub section: String,
    /// The cadence this bar closes with, if any.
    pub cadence: Option<Cadence>,
    /// Whether this bar begins a phrase (instruments may change).
    pub phrase_start: bool,
    /// Voice index carrying the tune to be sung, if any.
    pub sung_voice: Option<usize>,
    /// Vowels for the sung notes this bar, one per attack, cycled.
    pub vowels: Vec<Vowel>,
    /// Whether the form has finished and the composer should pick another.
    pub finished: bool,
    /// Bass pedal point pitch class, when the form wants one.
    pub pedal: Option<Midi>,
}

/// What forms are given to plan a bar.
pub struct FormContext<'a> {
    /// The library.
    pub lib: &'a IdeaLibrary,
    /// The sliders.
    pub state: &'a MusicState,
    /// The grammar.
    pub grammar: &'a Grammar,
    /// Random source.
    pub rng: &'a mut SplitMix64,
    /// The number of melodic voices to write.
    pub voices: usize,
    /// Ranges of the ensemble's voices, highest first.
    pub ranges: &'a [(Midi, Midi)],
    /// An idea the player asked for, to be taken up as soon as musically
    /// possible.
    pub requested_idea: Option<String>,
    /// Whether a cadence was asked for.
    pub requested_cadence: bool,
    /// Text the player asked to have sung.
    pub requested_lyric: Option<Vec<Vowel>>,
}

/// A form.
pub trait Form: Send {
    /// Which form this is.
    fn kind(&self) -> FormKind;
    /// Plans the next bar.
    fn next_bar(&mut self, ctx: &mut FormContext<'_>) -> BarPlan;
    /// The current key.
    fn key(&self) -> Scale;
    /// A short description of where the form is.
    fn describe(&self) -> String;
}

/// A realised idea line: events converted to MIDI in a key, ready to be
/// sliced into bars.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// `(onset in crotchets, duration, midi, ornament)`.
    pub notes: Vec<(f64, f64, Midi, Option<char>)>,
    /// Total length in crotchets (rests included).
    pub length: f64,
    /// Label of the variant.
    pub label: String,
}

impl Line {
    /// Realises `events` in `scale`, placing the line so its median pitch
    /// sits nearest `centre`.
    pub fn realise(events: &[IdeaEvent], scale: &Scale, centre: Midi, label: String) -> Self {
        let mut raw: Vec<(f64, f64, Midi, Option<char>)> = Vec::new();
        let mut t = 0.0;
        for e in events {
            if let Some(n) = e.note {
                raw.push((t, e.beats, scale.midi(n, 60), e.ornament));
            }
            t += e.beats;
        }
        let mut pitches: Vec<Midi> = raw.iter().map(|n| n.2).collect();
        pitches.sort_unstable();
        let median = pitches.get(pitches.len() / 2).copied().unwrap_or(centre);
        let shift = ((f64::from(centre - median)) / 12.0).round() as i32 * 12;
        let notes = raw
            .into_iter()
            .map(|(o, d, m, orn)| (o, d, m + shift, orn))
            .collect();
        Self {
            notes,
            length: t,
            label,
        }
    }

    /// The notes of the line falling in `[start, start + bar_len)` placed on
    /// a grid of `slot_beats`, notes that began earlier marked tied.
    pub fn slice(&self, start: f64, bar_len: f64, slot_beats: f64) -> Vec<PlacedNote> {
        let end = start + bar_len;
        let slots = (bar_len / slot_beats).round() as usize;
        let mut out = Vec::new();
        for &(onset, dur, midi, orn) in &self.notes {
            let n_end = onset + dur;
            if n_end <= start + 1e-9 || onset >= end - 1e-9 {
                continue;
            }
            let tied = onset < start - 1e-9;
            let s0 = ((onset.max(start) - start) / slot_beats).round() as usize;
            let s1 = ((n_end.min(end) - start) / slot_beats).round() as usize;
            let s1 = s1.min(slots);
            if s1 <= s0 {
                // Shorter than a slot: give it one slot if it starts here.
                if !tied && s0 < slots {
                    out.push(PlacedNote { slot: s0, len: 1, midi, tied: false, ornament: orn });
                }
                continue;
            }
            out.push(PlacedNote { slot: s0, len: s1 - s0, midi, tied, ornament: orn });
        }
        // Resolve collisions from rounding: later notes win their start slot.
        out.sort_by_key(|n| n.slot);
        let mut cleaned: Vec<PlacedNote> = Vec::with_capacity(out.len());
        for n in out {
            if let Some(prev) = cleaned.last_mut()
                && prev.slot + prev.len > n.slot
            {
                prev.len = n.slot.saturating_sub(prev.slot);
                if prev.len == 0 {
                    cleaned.pop();
                }
            }
            cleaned.push(n);
        }
        cleaned
    }

    /// The melody notes for harmonisation, per bar.
    pub fn melody_bars(&self, bar_len: f64) -> Vec<Vec<MelodyNote>> {
        let n_bars = (self.length / bar_len - 1e-9).ceil().max(1.0) as usize;
        let mut bars: Vec<Vec<MelodyNote>> = vec![Vec::new(); n_bars];
        for &(onset, dur, midi, _) in &self.notes {
            let mut t = onset;
            let end = onset + dur;
            while t < end - 1e-9 {
                let bar = (t / bar_len + 1e-9).floor() as usize;
                let bar_end = (bar as f64 + 1.0) * bar_len;
                let seg_end = end.min(bar_end);
                if let Some(b) = bars.get_mut(bar) {
                    b.push(MelodyNote {
                        onset: t - bar as f64 * bar_len,
                        duration: seg_end - t,
                        midi,
                    });
                }
                t = seg_end;
            }
        }
        bars
    }

    /// Number of bars the line spans.
    pub fn bars(&self, bar_len: f64) -> usize {
        (self.length / bar_len - 1e-9).ceil().max(1.0) as usize
    }
}

/// Picks the mode for a key from the darkness slider.
pub fn mode_for(state: &MusicState, rng: &mut SplitMix64) -> Mode {
    if state.darkness > 0.9 && rng.chance(0.3) {
        Mode::Phrygian
    } else if state.darkness > 0.6 && rng.chance(0.3) {
        Mode::Dorian
    } else if rng.chance(state.darkness) {
        Mode::Minor
    } else {
        Mode::Major
    }
}

/// The tonics comfortable for period winds and strings.
const TONICS: [i32; 7] = [2, 7, 0, 5, 9, 10, 4];

/// Picks a key near `previous` (a related key when there is one).
pub fn pick_key(state: &MusicState, rng: &mut SplitMix64, previous: Option<Scale>) -> Scale {
    let mode = mode_for(state, rng);
    let tonic = match previous {
        Some(p) => {
            let options = [p.tonic, p.dominant().tonic, p.subdominant().tonic, p.relative().tonic];
            rng.pick(&options).copied().unwrap_or(p.tonic)
        }
        None => rameau_theory::PitchClass::new(rng.pick(&TONICS).copied().unwrap_or(2)),
    };
    Scale::new(tonic, mode)
}

fn density_for(state: &MusicState, role: Role) -> f64 {
    let base = state.density;
    match role {
        Role::Lead => 0.15 + 0.75 * base,
        Role::Inner => 0.1 + 0.55 * base * (0.5 + 0.5 * state.polyphony),
        Role::Bass => 0.08 + 0.5 * base,
        _ => base,
    }
}

fn free(role: Role, range: (Midi, Midi), state: &MusicState) -> VoiceAssignment {
    VoiceAssignment {
        role,
        range,
        material: Material::Free,
        density: density_for(state, role),
        label: None,
    }
}

fn fixed(role: Role, range: (Midi, Midi), notes: Vec<PlacedNote>, label: &str) -> VoiceAssignment {
    VoiceAssignment {
        role,
        range,
        material: Material::Fixed(notes),
        density: 0.0,
        label: Some(label.to_owned()),
    }
}

fn role_of(index: usize, n: usize) -> Role {
    if n > 1 && index + 1 == n {
        Role::Bass
    } else if index == 0 {
        Role::Lead
    } else {
        Role::Inner
    }
}

fn range_of(ctx: &FormContext<'_>, index: usize, n: usize) -> (Midi, Midi) {
    ctx.ranges.get(index).copied().unwrap_or(match role_of(index, n) {
        Role::Bass => (36, 60),
        Role::Lead => (60, 84),
        _ => (50, 74),
    })
}

/// Chooses an idea of `kind` (or with role `role`) that suits the state,
/// avoiding `avoid`.
pub fn choose_idea<'a>(
    lib: &'a IdeaLibrary,
    state: &MusicState,
    rng: &mut SplitMix64,
    kind: IdeaKind,
    role: Option<&str>,
    avoid: &[String],
) -> Option<&'a Idea> {
    let pool: Vec<&Idea> = lib
        .all()
        .iter()
        .filter(|i| i.kind == kind || role.is_some_and(|r| i.has_role(r)))
        .filter(|i| !avoid.contains(&i.id))
        .collect();
    if pool.is_empty() {
        return None;
    }
    let weights: Vec<f64> = pool
        .iter()
        .map(|i| {
            let phase = if i.fits_phase(state.phase) { 1.0 } else { 0.25 };
            let order = 0.2 + 0.8 * f64::from(i.order_affinity(state.refinement as f32));
            let mode = match (i.mode.is_minor(), state.darkness > 0.5) {
                (true, true) | (false, false) => 1.0,
                _ => 0.6,
            };
            phase * order * mode
        })
        .collect();
    rng.weighted(&weights).and_then(|k| pool.get(k).copied())
}

/// A rotation of mutations that keeps an idea fresh across its tenure.
fn variation(step: usize, state: &MusicState, rng: &mut SplitMix64) -> Vec<Mutation> {
    let mut m = Vec::new();
    match step % 7 {
        0 => {}
        1 => m.push(Mutation::Transposition { steps: if rng.chance(0.5) { 4 } else { -3 } }),
        2 => m.push(Mutation::Inversion),
        3 => m.push(Mutation::Sequence { times: 1, steps: -1 }),
        4 => {
            if state.density > 0.5 {
                m.push(Mutation::Diminution { factor: 2.0 });
                m.push(Mutation::Sequence { times: 1, steps: 1 });
            } else {
                m.push(Mutation::Augmentation { factor: 2.0 });
            }
        }
        5 => m.push(Mutation::Retrograde),
        _ => {
            m.push(Mutation::Inversion);
            m.push(Mutation::Transposition { steps: 2 });
        }
    }
    if state.ornament > 0.55 && rng.chance(state.ornament - 0.3) {
        m.push(Mutation::Ornamentation);
    }
    if state.density < 0.25 && rng.chance(0.5) {
        m.push(Mutation::Simplification { min_beats: 0.5 });
    }
    m
}

/// Shared machinery: a fixed line playing through a voice over several bars.
#[derive(Debug, Clone)]
struct Track {
    line: Line,
    voice: usize,
    /// Crotchets into the line at the next bar start.
    cursor: f64,
    chords: Vec<Vec<ChordSlot>>,
    bar: usize,
}

impl Track {
    fn new(line: Line, voice: usize, chords: Vec<Vec<ChordSlot>>) -> Self {
        Self {
            line,
            voice,
            cursor: 0.0,
            chords,
            bar: 0,
        }
    }

    fn finished(&self, bar_len: f64) -> bool {
        self.cursor >= self.line.length - bar_len * 0.25
    }

    fn take_bar(&mut self, bar_len: f64, slot_beats: f64) -> (Vec<PlacedNote>, Vec<ChordSlot>) {
        let notes = self.line.slice(self.cursor, bar_len, slot_beats);
        let chords = self.chords.get(self.bar).cloned().unwrap_or_default();
        self.cursor += bar_len;
        self.bar += 1;
        (notes, chords)
    }
}

fn harmonise_line(
    ctx: &mut FormContext<'_>,
    key: &Scale,
    meter: &Meter,
    line: &Line,
    cadence: Cadence,
    start: Option<RomanNumeral>,
) -> Vec<Vec<ChordSlot>> {
    let bars = line.melody_bars(meter.bar_quarters());
    let slots = if ctx.state.tempo_bpm < 96.0 || meter.bar_quarters() >= 4.0 { 2 } else { 1 };
    harmonize::harmonize(ctx.rng, ctx.grammar, key, meter, &bars, slots, Some(cadence), start, 0.3)
}

fn vowels_for(idea: &Idea, ctx: &mut FormContext<'_>) -> Vec<Vowel> {
    if let Some(v) = ctx.requested_lyric.take() {
        return v;
    }
    idea.lyrics.as_deref().map(rameau_voix::vowels).unwrap_or_default()
}

// ---------------------------------------------------------------- Air

/// A tune with accompaniment, sung when it has words.
pub struct Air {
    key: Scale,
    meter: Meter,
    idea: String,
    tenure: usize,
    step: usize,
    track: Option<Track>,
    vowels: Vec<Vowel>,
    bars_done: usize,
    finished: bool,
    dance: bool,
}

impl Air {
    /// A new air (or, with `dance`, a contredanse) on `idea` in `key`.
    pub fn new(idea: &Idea, key: Scale, dance: bool) -> Self {
        Self {
            key,
            meter: idea.metre,
            idea: idea.id.clone(),
            tenure: if dance { 4 } else { 3 },
            step: 0,
            track: None,
            vowels: Vec::new(),
            bars_done: 0,
            finished: false,
            dance,
        }
    }

    fn start_strain(&mut self, ctx: &mut FormContext<'_>) {
        let Some(idea) = ctx.lib.get(&self.idea) else {
            self.finished = true;
            return;
        };
        // Dances repeat each strain plainly before varying it.
        let muts = if self.dance && self.step.is_multiple_of(2) {
            Vec::new()
        } else {
            variation(if self.dance { self.step / 2 } else { self.step }, ctx.state, ctx.rng)
        };
        let variant = Variant {
            idea: idea.id.clone(),
            mutations: muts,
        };
        let events = idea.with_pickup(&variant.apply_to(&idea.events));
        let n = ctx.voices;
        let centre = {
            let (lo, hi) = range_of(ctx, 0, n);
            (lo + hi) / 2 + ((ctx.state.register - 0.5) * 7.0) as i32
        };
        let line = Line::realise(&events, &self.key, centre, variant.label());
        let cadence = if self.step + 1 >= self.tenure {
            Cadence::Authentic
        } else if ctx.rng.chance(0.4) {
            Cadence::Half
        } else {
            Cadence::Imperfect
        };
        let chords = harmonise_line(ctx, &self.key, &self.meter, &line, cadence, None);
        self.vowels = vowels_for(idea, ctx);
        self.track = Some(Track::new(line, 0, chords));
    }
}

impl Form for Air {
    fn kind(&self) -> FormKind {
        if self.dance { FormKind::Contredanse } else { FormKind::Air }
    }

    fn key(&self) -> Scale {
        self.key
    }

    fn describe(&self) -> String {
        format!("{} · strain {}/{}", self.idea, self.step + 1, self.tenure)
    }

    fn next_bar(&mut self, ctx: &mut FormContext<'_>) -> BarPlan {
        let bar_len = self.meter.bar_quarters();
        let slot = self.meter.grid_quarters();
        let mut phrase_start = false;
        if self.track.as_ref().is_none_or(|t| t.finished(bar_len)) {
            if self.track.is_some() {
                self.step += 1;
            }
            if self.step >= self.tenure || ctx.requested_idea.is_some() || ctx.requested_cadence {
                self.finished = true;
            }
            self.start_strain(ctx);
            phrase_start = true;
        }
        let n = ctx.voices.max(1);
        let mut voices: Vec<VoiceAssignment> = (0..n)
            .map(|i| free(role_of(i, n), range_of(ctx, i, n), ctx.state))
            .collect();
        let mut chords = Vec::new();
        let mut section = self.describe();
        let mut cadence = None;
        if let Some(track) = &mut self.track {
            let last_bar = track.bar + 1 >= track.line.bars(bar_len);
            let (notes, ch) = track.take_bar(bar_len, slot);
            chords = ch;
            if let Some(v) = voices.first_mut() {
                *v = fixed(Role::Lead, v.range, notes, &track.line.label);
            }
            section = format!("{} · {}", if self.dance { "contredanse" } else { "air" }, track.line.label);
            if last_bar {
                cadence = Some(if self.step + 1 >= self.tenure { Cadence::Authentic } else { Cadence::Half });
            }
        }
        if chords.is_empty() {
            chords = vec![ChordSlot { onset: 0.0, duration: bar_len, chord: RomanNumeral::diatonic(&self.key, 0) }];
        }
        self.bars_done += 1;
        let sing = ctx.state.singing > 0.15 && !self.vowels.is_empty();
        BarPlan {
            key: self.key,
            meter: self.meter,
            chords,
            voices,
            section,
            cadence,
            phrase_start,
            sung_voice: sing.then_some(0),
            vowels: self.vowels.clone(),
            finished: self.finished && self.track.as_ref().is_none_or(|t| t.finished(bar_len)),
            pedal: None,
        }
    }
}

// ---------------------------------------------------------------- Rondeau

/// Refrain and couplets: A B A C A.
pub struct Rondeau {
    home: Scale,
    meter: Meter,
    refrain: String,
    couplets: Vec<String>,
    /// Section index into the A B A C A pattern.
    section: usize,
    track: Option<Track>,
    key: Scale,
    vowels: Vec<Vowel>,
    finished: bool,
}

impl Rondeau {
    /// A rondeau on `refrain` with up to two couplet ideas.
    pub fn new(refrain: &Idea, couplets: Vec<String>, key: Scale) -> Self {
        Self {
            home: key,
            meter: refrain.metre,
            refrain: refrain.id.clone(),
            couplets,
            section: 0,
            track: None,
            key,
            vowels: Vec::new(),
            finished: false,
        }
    }

    fn start_section(&mut self, ctx: &mut FormContext<'_>) {
        let is_refrain = self.section.is_multiple_of(2);
        let couplet_index = self.section / 2;
        let (idea_id, key, muts): (String, Scale, Vec<Mutation>) = if is_refrain {
            let muts = if self.section == 0 || ctx.state.ornament < 0.5 {
                Vec::new()
            } else {
                vec![Mutation::Ornamentation]
            };
            (self.refrain.clone(), self.home, muts)
        } else {
            let key = if couplet_index == 1 { self.home.relative() } else { self.home.dominant() };
            match self.couplets.get(couplet_index.saturating_sub(1)) {
                Some(id) => (id.clone(), key, Vec::new()),
                None => (self.refrain.clone(), key, variation(2 + couplet_index, ctx.state, ctx.rng)),
            }
        };
        let Some(idea) = ctx.lib.get(&idea_id) else {
            self.finished = true;
            return;
        };
        let variant = Variant { idea: idea.id.clone(), mutations: muts };
        let events = idea.with_pickup(&variant.apply_to(&idea.events));
        let n = ctx.voices;
        let (lo, hi) = range_of(ctx, 0, n);
        let line = Line::realise(&events, &key, (lo + hi) / 2, variant.label());
        let cadence = if is_refrain { Cadence::Authentic } else { Cadence::Half };
        let chords = harmonise_line(ctx, &key, &self.meter, &line, cadence, None);
        self.vowels = if is_refrain { vowels_for(idea, ctx) } else { Vec::new() };
        self.key = key;
        self.track = Some(Track::new(line, 0, chords));
    }
}

impl Form for Rondeau {
    fn kind(&self) -> FormKind {
        FormKind::Rondeau
    }

    fn key(&self) -> Scale {
        self.key
    }

    fn describe(&self) -> String {
        let name = match self.section {
            0 => "refrain",
            1 => "couplet I",
            2 => "refrain (bis)",
            3 => "couplet II",
            _ => "refrain (ter)",
        };
        format!("rondeau · {name}")
    }

    fn next_bar(&mut self, ctx: &mut FormContext<'_>) -> BarPlan {
        let bar_len = self.meter.bar_quarters();
        let slot = self.meter.grid_quarters();
        let mut phrase_start = false;
        if self.track.as_ref().is_none_or(|t| t.finished(bar_len)) {
            if self.track.is_some() {
                self.section += 1;
            }
            if self.section >= 5
                || ((ctx.requested_idea.is_some() || ctx.requested_cadence) && self.section.is_multiple_of(2))
            {
                self.finished = true;
            }
            if !self.finished {
                self.start_section(ctx);
            }
            phrase_start = true;
        }
        let n = ctx.voices.max(1);
        let mut voices: Vec<VoiceAssignment> = (0..n)
            .map(|i| free(role_of(i, n), range_of(ctx, i, n), ctx.state))
            .collect();
        let mut chords = Vec::new();
        let mut cadence = None;
        if let Some(track) = &mut self.track {
            let last_bar = track.bar + 1 >= track.line.bars(bar_len);
            let (notes, ch) = track.take_bar(bar_len, slot);
            chords = ch;
            if let Some(v) = voices.first_mut() {
                *v = fixed(Role::Lead, v.range, notes, &track.line.label);
            }
            if last_bar {
                cadence = Some(if self.section.is_multiple_of(2) { Cadence::Authentic } else { Cadence::Half });
            }
        }
        if chords.is_empty() {
            chords = vec![ChordSlot { onset: 0.0, duration: bar_len, chord: RomanNumeral::diatonic(&self.key, 0) }];
        }
        let sing = ctx.state.singing > 0.15 && !self.vowels.is_empty();
        let done = self.finished;
        BarPlan {
            key: self.key,
            meter: self.meter,
            chords,
            voices,
            section: self.describe(),
            cadence,
            phrase_start,
            sung_voice: sing.then_some(0),
            vowels: self.vowels.clone(),
            finished: done,
            pedal: None,
        }
    }
}

// ---------------------------------------------------------------- Chaconne

/// Variations over a ground bass.
pub struct Chaconne {
    key: Scale,
    meter: Meter,
    ground: String,
    ground_line: Option<Line>,
    ground_chords: Vec<Vec<ChordSlot>>,
    cursor: f64,
    variation: usize,
    max_variations: usize,
    tune: Option<Track>,
    tune_ideas: Vec<String>,
    finished: bool,
}

impl Chaconne {
    /// A chaconne on `ground` in `key`, using `tunes` for the upper voices.
    pub fn new(ground: &Idea, tunes: Vec<String>, key: Scale, max_variations: usize) -> Self {
        Self {
            key,
            meter: ground.metre,
            ground: ground.id.clone(),
            ground_line: None,
            ground_chords: Vec::new(),
            cursor: 0.0,
            variation: 0,
            max_variations: max_variations.max(2),
            tune: None,
            tune_ideas: tunes,
            finished: false,
        }
    }

    fn start_variation(&mut self, ctx: &mut FormContext<'_>) {
        let Some(ground) = ctx.lib.get(&self.ground) else {
            self.finished = true;
            return;
        };
        let n = ctx.voices;
        let bass_range = range_of(ctx, n.saturating_sub(1), n);
        let events = if self.variation % 4 == 3 && ctx.state.density > 0.5 {
            Mutation::Ornamentation.apply(&ground.events)
        } else {
            ground.events.clone()
        };
        let line = Line::realise(&events, &self.key, (bass_range.0 + bass_range.1) / 2, ground.id.clone());
        self.ground_chords = harmonize::ground_chords(&self.key, &events, self.meter.bar_quarters());
        self.ground_line = Some(line);
        self.cursor = 0.0;
        // Upper voices: alternate free variations with tune fragments.
        self.tune = None;
        if self.variation > 0 && !self.tune_ideas.is_empty() && self.variation % 2 == 1 {
            let idx = (self.variation / 2) % self.tune_ideas.len();
            if let Some(idea) = self.tune_ideas.get(idx).and_then(|id| ctx.lib.get(id)) {
                let muts = variation(self.variation / 2, ctx.state, ctx.rng);
                let variant = Variant { idea: idea.id.clone(), mutations: muts };
                let ev = idea.with_pickup(&variant.apply_to(&idea.events));
                let (lo, hi) = range_of(ctx, 0, n);
                let tl = Line::realise(&ev, &self.key, (lo + hi) / 2, variant.label());
                self.tune = Some(Track::new(tl, 0, Vec::new()));
            }
        }
    }
}

impl Form for Chaconne {
    fn kind(&self) -> FormKind {
        FormKind::Chaconne
    }

    fn key(&self) -> Scale {
        self.key
    }

    fn describe(&self) -> String {
        format!("chaconne · {} · variation {}", self.ground, self.variation + 1)
    }

    fn next_bar(&mut self, ctx: &mut FormContext<'_>) -> BarPlan {
        let bar_len = self.meter.bar_quarters();
        let slot = self.meter.grid_quarters();
        let mut phrase_start = false;
        let ground_done = self
            .ground_line
            .as_ref()
            .is_none_or(|l| self.cursor >= l.length - bar_len * 0.25);
        if ground_done {
            if self.ground_line.is_some() {
                self.variation += 1;
            }
            if self.variation >= self.max_variations
                || ((ctx.requested_idea.is_some() || ctx.requested_cadence) && self.variation >= 2)
            {
                self.finished = true;
            }
            self.start_variation(ctx);
            phrase_start = true;
        }
        let n = ctx.voices.max(1);
        let mut voices: Vec<VoiceAssignment> = (0..n)
            .map(|i| {
                let mut v = free(role_of(i, n), range_of(ctx, i, n), ctx.state);
                // Variations intensify then relax: an arch of density.
                let arch = 1.0 - ((self.variation as f64 / self.max_variations as f64) * 2.0 - 1.0).abs();
                v.density = (v.density * (0.6 + 0.8 * arch)).min(1.0);
                v
            })
            .collect();
        let bar_index = (self.cursor / bar_len + 1e-9).floor() as usize;
        let mut chords = self.ground_chords.get(bar_index).cloned().unwrap_or_default();
        if let Some(gl) = &self.ground_line {
            let notes = gl.slice(self.cursor, bar_len, slot);
            if let Some(v) = voices.last_mut() {
                *v = fixed(Role::Bass, v.range, notes, &gl.label);
                if n == 1 {
                    v.role = Role::Lead;
                }
            }
        }
        if let Some(t) = &mut self.tune
            && !t.finished(bar_len)
            && n > 1
        {
            let (notes, _) = t.take_bar(bar_len, slot);
            if let Some(v) = voices.first_mut() {
                *v = fixed(Role::Lead, v.range, notes, &t.line.label);
            }
        }
        if chords.is_empty() {
            chords = vec![ChordSlot { onset: 0.0, duration: bar_len, chord: RomanNumeral::diatonic(&self.key, 0) }];
        }
        let last_bar = self
            .ground_line
            .as_ref()
            .is_some_and(|l| self.cursor + bar_len >= l.length - bar_len * 0.25);
        self.cursor += bar_len;
        BarPlan {
            key: self.key,
            meter: self.meter,
            chords,
            voices,
            section: self.describe(),
            cadence: last_bar.then_some(Cadence::Authentic),
            phrase_start,
            sung_voice: None,
            vowels: Vec::new(),
            finished: self.finished,
            pedal: None,
        }
    }
}

// ---------------------------------------------------------------- Fugue

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FugueStage {
    Exposition,
    Episode,
    MiddleEntry,
    Stretto,
    Final,
}

/// Subject, answer, episodes, middle entries, stretto and a final entry over
/// a pedal.
pub struct Fugue {
    key: Scale,
    home: Scale,
    meter: Meter,
    subject: String,
    countersubject: Option<String>,
    stage: FugueStage,
    /// Entries still to make in the current stage: `(voice, is_answer)`.
    entries: Vec<(usize, bool)>,
    /// Active tracks: subject/answer/countersubject lines in voices.
    tracks: Vec<Track>,
    /// Bars left in the current episode.
    episode_bars: usize,
    episode_track: Option<Track>,
    chords: Vec<Vec<ChordSlot>>,
    chord_bar: usize,
    middle_entries_done: usize,
    stretto_done: bool,
    voices_seen: usize,
    finished: bool,
    subject_bars: usize,
}

impl Fugue {
    /// A fugue on `subject` in `key`.
    pub fn new(subject: &Idea, key: Scale, voices: usize) -> Self {
        let countersubject = subject
            .pairs_with
            .first()
            .cloned();
        let mut f = Self {
            key,
            home: key,
            meter: subject.metre,
            subject: subject.id.clone(),
            countersubject,
            stage: FugueStage::Exposition,
            entries: Vec::new(),
            tracks: Vec::new(),
            episode_bars: 0,
            episode_track: None,
            chords: Vec::new(),
            chord_bar: 0,
            middle_entries_done: 0,
            stretto_done: false,
            voices_seen: voices,
            finished: false,
            subject_bars: if subject.kind == IdeaKind::Subject { subject.bars().max(1) } else { subject.bars().clamp(1, 4) },
        };
        f.entries = entry_order(voices);
        f
    }

    fn subject_line(&self, ctx: &mut FormContext<'_>, voice: usize, answer: bool, key: &Scale, muts: &[Mutation]) -> Option<Line> {
        let idea = ctx.lib.get(&self.subject)?;
        let mut variant = Variant::plain(&idea.id);
        if idea.kind != IdeaKind::Subject {
            variant = variant.with(Mutation::Head { beats: idea.metre.bar_quarters() * self.subject_bars as f64 });
        }
        if answer {
            variant = variant.with(Mutation::Transposition { steps: 4 });
        }
        for m in muts {
            variant = variant.with(*m);
        }
        let events = idea.with_pickup(&variant.apply_to(&idea.events));
        let n = ctx.voices;
        let (lo, hi) = range_of(ctx, voice, n);
        Some(Line::realise(&events, key, (lo + hi) / 2, format!("{} ({})", variant.label(), if answer { "answer" } else { "subject" })))
    }

    fn countersubject_line(&self, ctx: &mut FormContext<'_>, voice: usize, key: &Scale) -> Option<Line> {
        let id = self.countersubject.as_ref()?;
        let idea = ctx.lib.get(id)?;
        let n = ctx.voices;
        let (lo, hi) = range_of(ctx, voice, n);
        Some(Line::realise(&idea.events, key, (lo + hi) / 2, format!("{} (countersubject)", idea.id)))
    }

    /// Starts the next entry of the current stage in `voice`.
    fn start_entry(&mut self, ctx: &mut FormContext<'_>, voice: usize, answer: bool, offset_bars: usize) {
        let key = self.key;
        let muts: Vec<Mutation> = match self.stage {
            FugueStage::MiddleEntry if self.middle_entries_done % 3 == 1 => vec![Mutation::Inversion],
            FugueStage::Final if ctx.state.density < 0.4 => vec![Mutation::Augmentation { factor: 2.0 }],
            _ => Vec::new(),
        };
        let Some(line) = self.subject_line(ctx, voice, answer, &key, &muts) else {
            self.finished = true;
            return;
        };
        let cadence = if self.stage == FugueStage::Final { Cadence::Authentic } else { Cadence::Imperfect };
        let start = self.chords.last().and_then(|b| b.last()).map(|s| s.chord);
        let chords = harmonise_line(ctx, &key, &self.meter, &line, cadence, start);
        // Chords for the bars this entry spans, offset by where it starts.
        for (i, bar) in chords.into_iter().enumerate() {
            let idx = self.chord_bar + offset_bars + i;
            while self.chords.len() <= idx {
                self.chords.push(Vec::new());
            }
            if let Some(slot) = self.chords.get_mut(idx)
                && slot.is_empty()
            {
                *slot = bar;
            }
        }
        let mut track = Track::new(line, voice, Vec::new());
        track.cursor = -(offset_bars as f64) * self.meter.bar_quarters();
        self.tracks.push(track);
        // The voice that just finished its subject takes the countersubject.
        if let Some(prev_voice) = self.previous_entry_voice(voice)
            && let Some(cs) = self.countersubject_line(ctx, prev_voice, &key)
        {
            let mut t = Track::new(cs, prev_voice, Vec::new());
            t.cursor = -(offset_bars as f64) * self.meter.bar_quarters();
            self.tracks.push(t);
        }
    }

    fn previous_entry_voice(&self, current: usize) -> Option<usize> {
        self.tracks
            .iter()
            .rev()
            .find(|t| t.voice != current && t.line.label.contains("subject") && !t.line.label.contains("counter"))
            .map(|t| t.voice)
    }

    fn advance_stage(&mut self, ctx: &mut FormContext<'_>) {
        let n = ctx.voices.max(1);
        // A bigger horde: the newcomers enter with the subject.
        if n > self.voices_seen {
            for v in self.voices_seen..n {
                self.entries.push((v, v % 2 == 1));
            }
            self.voices_seen = n;
            self.stage = FugueStage::Exposition;
            return;
        }
        self.voices_seen = n;
        if ctx.requested_cadence && self.stage != FugueStage::Final {
            self.stage = FugueStage::Final;
            self.key = self.home;
            self.entries = vec![(n.saturating_sub(1), false)];
            return;
        }
        self.stage = match self.stage {
            FugueStage::Exposition | FugueStage::MiddleEntry | FugueStage::Stretto => FugueStage::Episode,
            FugueStage::Episode => {
                if self.middle_entries_done >= 2 && !self.stretto_done && ctx.state.dissonance + ctx.state.density > 0.9 {
                    FugueStage::Stretto
                } else if self.middle_entries_done >= 3 || (self.middle_entries_done >= 2 && self.stretto_done) {
                    FugueStage::Final
                } else {
                    FugueStage::MiddleEntry
                }
            }
            FugueStage::Final => FugueStage::Final,
        };
        match self.stage {
            FugueStage::Episode => {
                self.episode_bars = if ctx.state.density > 0.5 { 2 } else { 3 };
                self.key = if self.middle_entries_done == 0 { self.home } else { self.key };
                // Episode: the head of the subject in sequence.
                if let Some(idea) = ctx.lib.get(&self.subject) {
                    let head = idea.events.iter().take_while(|e| !e.is_rest()).count().clamp(2, 5);
                    let variant = Variant::plain(&idea.id)
                        .with(Mutation::Fragment { start: 0, count: head })
                        .with(Mutation::Sequence { times: 3, steps: -1 });
                    let ev = variant.apply_to(&idea.events);
                    let voice = self.middle_entries_done % n;
                    let (lo, hi) = range_of(ctx, voice, n);
                    let line = Line::realise(&ev, &self.key, (lo + hi) / 2, format!("{} (episode)", variant.label()));
                    // Circle-of-fifths harmony under the sequence.
                    let prog = StockProgression::CircleOfFifths.realise(&self.key);
                    let bar_len = self.meter.bar_quarters();
                    for b in 0..self.episode_bars {
                        let idx = self.chord_bar + b;
                        while self.chords.len() <= idx {
                            self.chords.push(Vec::new());
                        }
                        let c1 = prog.chords.get((2 * b) % prog.chords.len()).copied().unwrap_or_default();
                        let c2 = prog.chords.get((2 * b + 1) % prog.chords.len()).copied().unwrap_or_default();
                        if let Some(slot) = self.chords.get_mut(idx) {
                            *slot = vec![
                                ChordSlot { onset: 0.0, duration: bar_len / 2.0, chord: c1 },
                                ChordSlot { onset: bar_len / 2.0, duration: bar_len / 2.0, chord: c2 },
                            ];
                        }
                    }
                    let mut t = Track::new(line, voice, Vec::new());
                    t.cursor = 0.0;
                    self.episode_track = Some(t);
                }
            }
            FugueStage::MiddleEntry => {
                self.key = match self.middle_entries_done % 3 {
                    0 => self.home.relative(),
                    1 => self.home.dominant(),
                    _ => self.home.subdominant(),
                };
                let voice = (self.middle_entries_done + 1) % n;
                self.entries = vec![(voice, false)];
                self.middle_entries_done += 1;
            }
            FugueStage::Stretto => {
                self.key = self.home;
                self.entries = (0..n).map(|v| (v, v % 2 == 1)).collect();
                self.stretto_done = true;
            }
            FugueStage::Final => {
                self.key = self.home;
                self.entries = vec![(n.saturating_sub(1), false)];
            }
            FugueStage::Exposition => {}
        }
    }
}

/// The order voices enter: alto, soprano, bass, tenor …
fn entry_order(n: usize) -> Vec<(usize, bool)> {
    let n = n.max(1);
    let order: Vec<usize> = match n {
        1 => vec![0],
        2 => vec![0, 1],
        3 => vec![1, 0, 2],
        4 => vec![1, 0, 3, 2],
        _ => {
            let mut v: Vec<usize> = (0..n).collect();
            v.swap(0, 1);
            v
        }
    };
    order.into_iter().enumerate().map(|(i, v)| (v, i % 2 == 1)).collect()
}

impl Form for Fugue {
    fn kind(&self) -> FormKind {
        FormKind::Fugue
    }

    fn key(&self) -> Scale {
        self.key
    }

    fn describe(&self) -> String {
        let stage = match self.stage {
            FugueStage::Exposition => "exposition",
            FugueStage::Episode => "episode",
            FugueStage::MiddleEntry => "middle entry",
            FugueStage::Stretto => "stretto",
            FugueStage::Final => "final entry",
        };
        format!("fugue · {} · {stage}", self.subject)
    }

    fn next_bar(&mut self, ctx: &mut FormContext<'_>) -> BarPlan {
        let bar_len = self.meter.bar_quarters();
        let slot = self.meter.grid_quarters();
        let n = ctx.voices.max(1);
        let mut phrase_start = false;

        // Drop tracks that have played out.
        self.tracks.retain(|t| !t.finished(bar_len));
        if let Some(t) = &self.episode_track
            && t.finished(bar_len)
        {
            self.episode_track = None;
        }

        // Start the next entry when nothing is entering, or (in stretto)
        // half a subject after the last one.
        let entering = self.tracks.iter().any(|t| !t.line.label.contains("counter") && t.cursor < t.line.length - bar_len * 0.5);
        let stretto_gap = self.subject_bars.div_ceil(2).max(1);
        let stretto_due = self.stage == FugueStage::Stretto
            && self.tracks.iter().filter(|t| !t.line.label.contains("counter")).all(|t| t.cursor >= (stretto_gap as f64) * bar_len - 1e-9);
        if !self.finished {
            if self.stage == FugueStage::Episode {
                if self.episode_track.is_none() && self.episode_bars == 0 {
                    self.advance_stage(ctx);
                    phrase_start = true;
                }
            } else if (!entering || stretto_due) && !self.entries.is_empty() {
                let (voice, answer) = self.entries.remove(0);
                let voice = voice.min(n - 1);
                self.start_entry(ctx, voice, answer, 0);
                phrase_start = true;
            } else if !entering && self.entries.is_empty() {
                if self.stage == FugueStage::Final {
                    self.finished = true;
                } else {
                    self.advance_stage(ctx);
                    phrase_start = true;
                    if self.stage != FugueStage::Episode && !self.entries.is_empty() {
                        let (voice, answer) = self.entries.remove(0);
                        self.start_entry(ctx, voice.min(n - 1), answer, 0);
                    }
                }
            }
        }

        let mut voices: Vec<VoiceAssignment> = (0..n)
            .map(|i| {
                let mut v = free(role_of(i, n), range_of(ctx, i, n), ctx.state);
                v.density = (v.density * (0.7 + 0.6 * ctx.state.polyphony)).min(1.0);
                v
            })
            .collect();
        // Voices that have not yet entered in the exposition stay silent.
        if self.stage == FugueStage::Exposition {
            let entered: Vec<usize> = self.tracks.iter().map(|t| t.voice).collect();
            for (i, v) in voices.iter_mut().enumerate() {
                if !entered.contains(&i) && self.entries.iter().any(|(e, _)| *e == i) {
                    v.material = Material::Rest;
                }
            }
        }
        let mut labels = Vec::new();
        for t in &mut self.tracks {
            let (notes, _) = t.take_bar(bar_len, slot);
            if notes.is_empty() {
                continue;
            }
            if let Some(v) = voices.get_mut(t.voice.min(n - 1)) {
                let role = v.role;
                *v = fixed(role, v.range, notes, &t.line.label);
                labels.push(t.line.label.clone());
            }
        }
        if let Some(t) = &mut self.episode_track {
            let (notes, _) = t.take_bar(bar_len, slot);
            if let Some(v) = voices.get_mut(t.voice.min(n - 1))
                && !notes.is_empty()
            {
                let role = v.role;
                *v = fixed(role, v.range, notes, &t.line.label);
                labels.push(t.line.label.clone());
            }
            self.episode_bars = self.episode_bars.saturating_sub(1);
        }
        let mut chords = self.chords.get(self.chord_bar).cloned().unwrap_or_default();
        if chords.is_empty() {
            let prog = ctx.grammar.phrase(ctx.rng, &self.key, 2, Cadence::Half, None);
            chords = prog
                .chords
                .iter()
                .enumerate()
                .map(|(i, c)| ChordSlot { onset: i as f64 * bar_len / 2.0, duration: bar_len / 2.0, chord: *c })
                .collect();
        }
        self.chord_bar += 1;
        let pedal = (self.stage == FugueStage::Final).then(|| self.key.tonic_midi(36));
        let cadence = if self.finished { Some(Cadence::Authentic) } else if phrase_start && self.stage == FugueStage::Episode { Some(Cadence::Half) } else { None };
        let mut section = self.describe();
        if !labels.is_empty() {
            section.push_str(" · ");
            section.push_str(&labels.join(" + "));
        }
        BarPlan {
            key: self.key,
            meter: self.meter,
            chords,
            voices,
            section,
            cadence,
            phrase_start,
            sung_voice: None,
            vowels: Vec::new(),
            finished: self.finished && self.tracks.is_empty(),
            pedal,
        }
    }
}

/// Picks a form for the state, honouring a request.
pub fn choose_form_kind(state: &MusicState, rng: &mut SplitMix64, requested: Option<FormKind>) -> FormKind {
    if let Some(f) = requested {
        return f;
    }
    let weights: Vec<(FormKind, f64)> = match state.phase {
        Phase::Recruit => vec![
            (FormKind::Air, 3.0 + 2.0 * state.singing),
            (FormKind::Rondeau, 2.5),
            (FormKind::Fugue, 1.0 + 3.0 * state.polyphony * state.refinement),
            (FormKind::Contredanse, 1.0 + 2.0 * state.density),
            (FormKind::Chaconne, 0.5),
        ],
        Phase::Riot => vec![
            (FormKind::Contredanse, 3.0 + 3.0 * state.density),
            (FormKind::Fugue, 1.5 + 3.0 * state.polyphony + 2.0 * (state.voices as f64 / 6.0)),
            (FormKind::Rondeau, 1.5),
            (FormKind::Air, 0.6 + state.singing),
            (FormKind::Chaconne, 1.0 + 2.0 * state.license),
        ],
        Phase::Retreat => vec![
            (FormKind::Chaconne, 3.0 + 2.0 * state.darkness),
            (FormKind::Air, 2.0 + state.singing),
            (FormKind::Fugue, 0.8 + 2.0 * state.polyphony),
            (FormKind::Rondeau, 0.8),
            (FormKind::Contredanse, 0.3),
        ],
    };
    let ws: Vec<f64> = weights.iter().map(|w| w.1).collect();
    weights
        .get(rng.weighted(&ws).unwrap_or(0))
        .map_or(FormKind::Air, |w| w.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rameau_theory::PitchClass;

    #[test]
    fn line_slices_across_barlines() {
        let events = vec![
            IdeaEvent::note(0, 0, 0, 3.0),
            IdeaEvent::note(1, 0, 0, 2.0),
            IdeaEvent::rest(1.0),
            IdeaEvent::note(2, 0, 0, 2.0),
        ];
        let c = Scale::new(PitchClass::C, Mode::Major);
        let line = Line::realise(&events, &c, 72, "t".into());
        assert_eq!(line.length, 8.0);
        assert_eq!(line.bars(4.0), 2);
        let b1 = line.slice(0.0, 4.0, 0.25);
        assert_eq!(b1.len(), 2);
        assert_eq!(b1[0].len, 12);
        assert_eq!(b1[1].len, 4);
        let b2 = line.slice(4.0, 4.0, 0.25);
        assert_eq!(b2.len(), 2);
        assert!(b2[0].tied);
        assert_eq!(b2[0].len, 4);
        assert_eq!(b2[1].slot, 8);
        let mb = line.melody_bars(4.0);
        assert_eq!(mb[1][0].onset, 0.0);
        assert_eq!(mb[1][0].duration, 1.0);
        // Median placed near the centre.
        assert!(line.notes.iter().all(|n| (60..=84).contains(&n.2)));
    }

    #[test]
    fn entry_orders() {
        assert_eq!(entry_order(3), vec![(1, false), (0, true), (2, false)]);
        assert_eq!(entry_order(4).len(), 4);
    }
}
