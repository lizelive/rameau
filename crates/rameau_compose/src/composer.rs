//! The composer: plan a bar, anneal it, orchestrate it.

extern crate alloc;
use alloc::collections::VecDeque;

use rameau_chords::Grammar;
use rameau_theory::{BeatStrength, Meter, Midi, Scale};
use rameau_types::{Rng, SplitMix64};
use rameau_voix::Vowel;

use crate::anneal::{self, BarGrid, Breakdown, Context, Fingerprint};
use crate::form::{self, Air, BarPlan, Chaconne, Form, FormContext, Fugue, Material, Rondeau};
use crate::idea::{IdeaKind, IdeaLibrary};
use crate::instrument::{Ensemble, Instrument, Role};
use crate::state::{FormKind, MusicState, RuleWeights};

/// MIDI channel of the continuo.
pub const CONTINUO_CHANNEL: u8 = 8;
/// MIDI channel of unpitched percussion.
pub const DRUM_CHANNEL: u8 = 9;
/// MIDI channel of the singer.
pub const SINGER_CHANNEL: u8 = 10;
/// MIDI channel of stingers (bells, cannon).
pub const STINGER_CHANNEL: u8 = 11;
/// MIDI channel of the timpani.
pub const TIMPANI_CHANNEL: u8 = 12;

/// A request that lands on the next bar.
#[derive(Debug, Clone, PartialEq)]
pub enum Trigger {
    /// Take up a named idea as soon as the form allows.
    Idea(String),
    /// Change to a form.
    Form(FormKind),
    /// Cadence and start a new phrase.
    Cadence,
    /// Sing this text on the next tune.
    Lyric(String),
    /// The alarm bell.
    Tocsin,
    /// A cannon shot.
    Cannon,
    /// Change key.
    Key(Scale),
}

/// One note of a composed bar.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredNote {
    /// MIDI channel.
    pub channel: u8,
    /// MIDI key.
    pub key: u8,
    /// Velocity.
    pub velocity: u8,
    /// Onset in crotchets from the bar start.
    pub onset: f64,
    /// Duration in crotchets.
    pub duration: f64,
    /// Voice index in the plan (melodic voices), or `None`.
    pub voice: Option<usize>,
    /// Bank and program to select on the channel *before* this note (used
    /// for vowels, which change per note).
    pub program: Option<(u16, u8)>,
}

/// What was composed for one bar, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct ComposedBar {
    /// Running bar number.
    pub index: u64,
    /// Tempo in crotchets per minute.
    pub tempo_bpm: f64,
    /// Meter.
    pub meter: Meter,
    /// Length in crotchets.
    pub beats: f64,
    /// Program changes to apply at the start of the bar: `(channel, bank, program)`.
    pub programs: Vec<(u8, u16, u8)>,
    /// Notes.
    pub notes: Vec<ScoredNote>,
    /// Form.
    pub form: FormKind,
    /// Section label.
    pub section: String,
    /// Key.
    pub key: Scale,
    /// Chord labels in order.
    pub chords: Vec<String>,
    /// Ensemble description.
    pub ensemble: String,
    /// Instrument per melodic voice.
    pub instruments: Vec<&'static str>,
    /// Cost breakdown of the annealed bar.
    pub breakdown: Breakdown,
    /// Cost before annealing.
    pub initial_cost: f64,
    /// Whether a phrase began here.
    pub phrase_start: bool,
    /// The sliders in force.
    pub state: MusicState,
}

/// The composer.
pub struct Composer {
    lib: IdeaLibrary,
    state: MusicState,
    rng: SplitMix64,
    grammar: Grammar,
    form: Option<Box<dyn Form>>,
    form_kind: FormKind,
    form_bars: usize,
    key: Scale,
    ensemble: Option<Ensemble>,
    history: VecDeque<Vec<Fingerprint>>,
    last_sim: Vec<Option<Midi>>,
    bar_index: u64,
    requested_idea: Option<String>,
    requested_form: Option<FormKind>,
    requested_cadence: bool,
    requested_lyric: Option<Vec<Vowel>>,
    stingers: Vec<Trigger>,
    vowel_cursor: usize,
    recent_ideas: VecDeque<String>,
    /// Annealing proposals per bar.
    pub iterations: usize,
}

impl Composer {
    /// A composer over `lib`, seeded.
    pub fn new(lib: IdeaLibrary, seed: u64) -> Self {
        let mut rng = SplitMix64::new(seed);
        let state = MusicState::default();
        let key = form::pick_key(&state, &mut rng, None);
        Self {
            lib,
            state,
            rng,
            grammar: Grammar::period(),
            form: None,
            form_kind: FormKind::Air,
            form_bars: 0,
            key,
            ensemble: None,
            history: VecDeque::new(),
            last_sim: Vec::new(),
            bar_index: 0,
            requested_idea: None,
            requested_form: None,
            requested_cadence: false,
            requested_lyric: None,
            stingers: Vec::new(),
            vowel_cursor: 0,
            recent_ideas: VecDeque::new(),
            iterations: 700,
        }
    }

    /// The library.
    pub fn library(&self) -> &IdeaLibrary {
        &self.lib
    }

    /// The sliders in force.
    pub fn state(&self) -> &MusicState {
        &self.state
    }

    /// Replaces the sliders; takes effect on the next composed bar.
    pub fn set_state(&mut self, state: MusicState) {
        self.state = state.clamped();
    }

    /// Queues a trigger for the next bar.
    pub fn trigger(&mut self, t: Trigger) {
        match t {
            Trigger::Idea(id) => self.requested_idea = Some(id),
            Trigger::Form(f) => self.requested_form = Some(f),
            Trigger::Cadence => self.requested_cadence = true,
            Trigger::Lyric(text) => {
                self.requested_lyric = Some(rameau_voix::vowels(&text));
                self.vowel_cursor = 0;
            }
            Trigger::Key(k) => {
                self.key = k;
                self.form = None;
            }
            Trigger::Tocsin | Trigger::Cannon => self.stingers.push(t),
        }
    }

    /// The current key.
    pub fn key(&self) -> Scale {
        self.form.as_ref().map_or(self.key, |f| f.key())
    }

    /// The current form.
    pub fn form_kind(&self) -> FormKind {
        self.form_kind
    }

    /// Starts a new form for the current state.
    fn start_form(&mut self) {
        let requested = self.requested_form.take();
        let kind = form::choose_form_kind(&self.state, &mut self.rng, requested);
        let key = if self.form_bars > 0 {
            form::pick_key(&self.state, &mut self.rng, Some(self.key))
        } else {
            self.key
        };
        let avoid: Vec<String> = self.recent_ideas.iter().cloned().collect();
        let requested_idea = self.requested_idea.take().and_then(|id| self.lib.get(&id).cloned());
        let n = self.state.voices;
        let form: Option<Box<dyn Form>> = match kind {
            FormKind::Fugue => {
                let subject = requested_idea
                    .clone()
                    .filter(|i| matches!(i.kind, IdeaKind::Subject | IdeaKind::Motif))
                    .or_else(|| form::choose_idea(&self.lib, &self.state, &mut self.rng, IdeaKind::Subject, Some("subject"), &avoid).cloned());
                subject.map(|s| {
                    self.remember(&s.id);
                    Box::new(Fugue::new(&s, key, n)) as Box<dyn Form>
                })
            }
            FormKind::Chaconne => {
                let ground = requested_idea
                    .clone()
                    .filter(|i| i.kind == IdeaKind::Ground)
                    .or_else(|| form::choose_idea(&self.lib, &self.state, &mut self.rng, IdeaKind::Ground, None, &avoid).cloned());
                ground.map(|g| {
                    self.remember(&g.id);
                    let mut tunes: Vec<String> = g.pairs_with.clone();
                    if let Some(req) = &requested_idea
                        && req.kind == IdeaKind::Motif
                    {
                        tunes.insert(0, req.id.clone());
                    }
                    if tunes.is_empty()
                        && let Some(t) = form::choose_idea(&self.lib, &self.state, &mut self.rng, IdeaKind::Motif, None, &avoid)
                    {
                        tunes.push(t.id.clone());
                    }
                    let key = if g.mode.is_minor() && !key.mode.is_minor() { key.parallel() } else { key };
                    Box::new(Chaconne::new(&g, tunes, key, 6 + (self.state.density * 4.0) as usize)) as Box<dyn Form>
                })
            }
            FormKind::Rondeau => {
                let refrain = requested_idea
                    .clone()
                    .filter(|i| i.kind == IdeaKind::Motif)
                    .or_else(|| form::choose_idea(&self.lib, &self.state, &mut self.rng, IdeaKind::Motif, Some("refrain"), &avoid).cloned());
                refrain.map(|r| {
                    self.remember(&r.id);
                    let mut avoid2 = avoid.clone();
                    avoid2.push(r.id.clone());
                    let couplets: Vec<String> = (0..2)
                        .filter_map(|_| {
                            let c = form::choose_idea(&self.lib, &self.state, &mut self.rng, IdeaKind::Motif, Some("episode"), &avoid2)?;
                            avoid2.push(c.id.clone());
                            Some(c.id.clone())
                        })
                        .collect();
                    Box::new(Rondeau::new(&r, couplets, key)) as Box<dyn Form>
                })
            }
            FormKind::Air | FormKind::Contredanse => {
                let dance = kind == FormKind::Contredanse;
                let role = if dance { Some("dance") } else { Some("hook") };
                let tune = requested_idea
                    .clone()
                    .filter(|i| i.kind == IdeaKind::Motif)
                    .or_else(|| form::choose_idea(&self.lib, &self.state, &mut self.rng, IdeaKind::Motif, role, &avoid).cloned());
                tune.map(|t| {
                    self.remember(&t.id);
                    let key = if t.mode.is_minor() != key.mode.is_minor() && self.rng.chance(0.6) { key.parallel() } else { key };
                    Box::new(Air::new(&t, key, dance)) as Box<dyn Form>
                })
            }
        };
        self.form_kind = kind;
        self.form_bars = 0;
        self.key = key;
        self.form = form;
        if self.form.is_none()
            && let Some(t) = self.lib.of_kind(IdeaKind::Motif).next().cloned()
        {
            self.form_kind = FormKind::Air;
            self.form = Some(Box::new(Air::new(&t, key, false)));
        }
    }

    fn remember(&mut self, id: &str) {
        self.recent_ideas.push_back(id.to_owned());
        while self.recent_ideas.len() > 4 {
            self.recent_ideas.pop_front();
        }
    }

    /// Composes the next bar.
    pub fn next_bar(&mut self) -> ComposedBar {
        if self.form.is_none() || self.requested_form.is_some() || (self.form_bars > 96 && self.requested_cadence) {
            self.start_form();
        }
        let state = self.state;
        let n = state.voices.clamp(1, 6);

        // Ensemble: re-chosen at phrase starts only.
        if self.ensemble.as_ref().is_none_or(|e| e.voices.len() != n) {
            let prev = self.ensemble.take();
            self.ensemble = Some(Ensemble::choose(&mut self.rng, &state, n, prev.as_ref()));
        }
        let ranges: Vec<(Midi, Midi)> = self
            .ensemble
            .as_ref()
            .map(|e| e.voices.iter().map(|i| i.range).collect())
            .unwrap_or_default();

        let plan = {
            let mut ctx = FormContext {
                lib: &self.lib,
                state: &state,
                grammar: &self.grammar,
                rng: &mut self.rng,
                voices: n,
                ranges: &ranges,
                requested_idea: self.requested_idea.clone(),
                requested_cadence: self.requested_cadence,
                requested_lyric: self.requested_lyric.take(),
            };
            let plan = match &mut self.form {
                Some(f) => f.next_bar(&mut ctx),
                None => return self.silent_bar(),
            };
            if ctx.requested_lyric.is_some() {
                self.requested_lyric = ctx.requested_lyric.take();
            }
            plan
        };
        if plan.phrase_start {
            self.requested_cadence = false;
            let prev = self.ensemble.take();
            self.ensemble = Some(Ensemble::choose(&mut self.rng, &state, n, prev.as_ref()));
        }
        if plan.finished {
            self.form = None;
        }
        self.form_bars += 1;
        if plan.phrase_start && self.requested_idea.is_some() && self.form_bars > 2 {
            // The form did not take the request: change form at the next
            // phrase.
            self.form = None;
        }

        let bar = self.realise(&plan, &state);
        self.bar_index += 1;
        bar
    }

    fn silent_bar(&mut self) -> ComposedBar {
        let meter = Meter::COMMON;
        self.bar_index += 1;
        ComposedBar {
            index: self.bar_index,
            tempo_bpm: self.state.tempo_bpm,
            meter,
            beats: meter.bar_quarters(),
            programs: Vec::new(),
            notes: Vec::new(),
            form: self.form_kind,
            section: "silence (no ideas loaded)".to_owned(),
            key: self.key,
            chords: Vec::new(),
            ensemble: String::new(),
            instruments: Vec::new(),
            breakdown: Breakdown::default(),
            initial_cost: 0.0,
            phrase_start: true,
            state: self.state,
        }
    }

    /// Builds the grid from the plan, anneals the free voices, and turns the
    /// result into notes.
    fn realise(&mut self, plan: &BarPlan, state: &MusicState) -> ComposedBar {
        let meter = plan.meter;
        let mut grid = BarGrid::new(&meter);
        let slots = grid.slots;
        for (i, v) in plan.voices.iter().enumerate() {
            let vi = grid.add_voice(v.role, v.range);
            let Some(g) = grid.voices.get_mut(vi) else { continue };
            g.prev_pitch = self.last_sim.get(i).copied().flatten();
            g.target_onsets = (v.density * slots as f64).round().max(if v.density > 0.0 { 1.0 } else { 0.0 });
            match &v.material {
                Material::Fixed(notes) => {
                    g.free = false;
                    for n in notes {
                        if n.tied {
                            g.tie_in(n.len, n.midi, true);
                        } else {
                            g.write(n.slot, n.len, Some(n.midi), true);
                        }
                    }
                    // Slots not covered by the idea stay fixed rests.
                    g.fixed.fill(true);
                }
                Material::Rest => {
                    g.free = false;
                    g.fixed.fill(true);
                }
                Material::Free => {
                    g.free = true;
                }
            }
        }
        // A pedal: the bass holds the tonic.
        if let Some(p) = plan.pedal
            && let Some(b) = grid.voices.last_mut()
            && b.free
        {
            let pitch = p.clamp(b.range.0, b.range.1);
            b.write(0, slots, Some(pitch), true);
            b.free = false;
        }
        self.seed_free_voices(&mut grid, plan);

        let weights = RuleWeights::from_state(state);
        let history: Vec<Vec<Fingerprint>> = self.history.iter().cloned().collect();
        let ctx = Context {
            scale: plan.key,
            meter,
            chords: &plan.chords,
            weights,
            previous: &self.last_sim,
            history: &history,
            chromaticism: state.chromaticism,
        };
        let iterations = if grid.voices.iter().any(|v| v.free) { self.iterations } else { 0 };
        let out = anneal::anneal(&mut self.rng, grid, &ctx, iterations);
        let grid = out.grid;

        self.last_sim = grid.final_simultaneity();
        self.history.push_back(grid.fingerprint());
        while self.history.len() > 12 {
            self.history.pop_front();
        }

        // Notes.
        let mut notes = Vec::new();
        let mut programs = Vec::new();
        let ensemble = self.ensemble.clone().unwrap_or_else(|| Ensemble::choose(&mut self.rng, state, grid.voices.len(), None));
        let slot_beats = grid.slot_beats;
        let gap = 0.03 + 0.5 * state.articulation;
        for (i, v) in grid.voices.iter().enumerate() {
            let inst = ensemble.voices.get(i).copied().unwrap_or(&crate::instrument::VIOLON);
            let channel = i as u8;
            programs.push((channel, inst.bank, inst.program));
            let accent_role = match v.role {
                Role::Lead => 0.4,
                Role::Bass => 0.15,
                _ => -0.1,
            };
            for (start, len, pitch) in v.notes() {
                let attacked = v.onset.get(start).copied().unwrap_or(true);
                if !attacked && start == 0 {
                    // Tied from the previous bar: the note is already
                    // sounding; re-attack softly so nothing is lost if the
                    // previous note-off already happened.
                    continue;
                }
                let onset = start as f64 * slot_beats;
                let full = len as f64 * slot_beats;
                let short = full <= 0.5;
                let duration = if short { full * (1.0 - 0.35 * state.articulation) } else { (full - gap.min(full * 0.4)).max(0.1) };
                let accent: f64 = match meter.strength_at(onset) {
                    BeatStrength::Downbeat => 1.0,
                    BeatStrength::Strong => 0.6,
                    BeatStrength::Weak => 0.25,
                    BeatStrength::Off => 0.0,
                } + accent_role;
                let velocity = state.velocity(accent.clamp(0.0, 1.4));
                let key = pitch.clamp(0, 127) as u8;
                let ornament_here = state.ornament > 0.45
                    && full >= 1.0
                    && v.role == Role::Lead
                    && self.rng.chance((state.ornament - 0.3) * 0.6);
                if ornament_here {
                    notes.extend(trill(channel, key, plan.key, onset, duration, velocity, Some(i)));
                } else {
                    notes.push(ScoredNote { channel, key, velocity, onset, duration, voice: Some(i), program: None });
                }
            }
        }

        // Continuo.
        if let Some(c) = ensemble.continuo {
            programs.push((CONTINUO_CHANNEL, c.bank, c.program));
            notes.extend(continuo_notes(&mut self.rng, plan, state, c));
        }
        // Percussion.
        if !ensemble.percussion.is_empty() {
            notes.extend(percussion_notes(&mut self.rng, plan, state, &ensemble.percussion));
            if ensemble.percussion.iter().any(|p| p.id == "timbales")
                && let Some(t) = Instrument::by_id("timbales")
            {
                programs.push((TIMPANI_CHANNEL, t.bank, t.program));
            }
        }
        // Singing: double the tune with vowels.
        if let (Some(vi), Some(singer)) = (plan.sung_voice, ensemble.singer)
            && !plan.vowels.is_empty()
        {
            let choir = singer.id == "choeur";
            let sung: Vec<ScoredNote> = notes
                .iter()
                .filter(|n| n.voice == Some(vi) && n.channel == vi as u8)
                .cloned()
                .collect();
            for n in sung {
                let vowel = plan.vowels.get(self.vowel_cursor % plan.vowels.len()).copied().unwrap_or(Vowel::A);
                self.vowel_cursor += 1;
                let bank = if choir { rameau_voix::CHOIR_BANK } else { rameau_voix::SOLO_BANK };
                // Sing in a comfortable octave.
                let mut key = i32::from(n.key);
                while key > singer.range.1 {
                    key -= 12;
                }
                while key < singer.range.0 {
                    key += 12;
                }
                notes.push(ScoredNote {
                    channel: SINGER_CHANNEL,
                    key: key as u8,
                    velocity: (n.velocity / 5 * 4).max(30),
                    onset: n.onset,
                    duration: n.duration.max(0.2),
                    voice: None,
                    program: Some((bank, vowel.program())),
                });
            }
        }
        // Stingers.
        for s in core::mem::take(&mut self.stingers) {
            match s {
                Trigger::Tocsin => {
                    if let Some(b) = Instrument::by_id("tocsin") {
                        programs.push((STINGER_CHANNEL, b.bank, b.program));
                        let tonic = plan.key.tonic_midi(60);
                        for k in 0..(meter.pulses() as usize * 2) {
                            let onset = k as f64 * meter.pulse_quarters() / 2.0;
                            let key = if k % 3 == 2 { tonic + 7 } else { tonic };
                            notes.push(ScoredNote { channel: STINGER_CHANNEL, key: key as u8, velocity: 110, onset, duration: 1.5, voice: None, program: None });
                        }
                    }
                }
                Trigger::Cannon => {
                    notes.push(ScoredNote { channel: DRUM_CHANNEL, key: 49, velocity: 127, onset: 0.0, duration: 1.0, voice: None, program: None });
                    notes.push(ScoredNote { channel: DRUM_CHANNEL, key: 35, velocity: 127, onset: 0.0, duration: 0.5, voice: None, program: None });
                }
                _ => {}
            }
        }
        notes.sort_by(|a, b| a.onset.total_cmp(&b.onset).then(a.channel.cmp(&b.channel)));

        ComposedBar {
            index: self.bar_index + 1,
            tempo_bpm: state.tempo_bpm,
            meter,
            beats: meter.bar_quarters(),
            programs,
            notes,
            form: self.form_kind,
            section: plan.section.clone(),
            key: plan.key,
            chords: plan.chords.iter().map(|c| c.chord.label()).collect(),
            ensemble: ensemble.describe(),
            instruments: ensemble.voices.iter().map(|i| i.name).collect(),
            breakdown: out.breakdown,
            initial_cost: out.initial_cost,
            phrase_start: plan.phrase_start,
            state: *state,
        }
    }

    /// Gives free voices a sensible starting point: chord tones in a rhythm
    /// near the density target, so the annealer refines rather than invents.
    fn seed_free_voices(&mut self, grid: &mut BarGrid, plan: &BarPlan) {
        let slots = grid.slots;
        let slot_beats = grid.slot_beats;
        for v in grid.voices.iter_mut().filter(|v| v.free) {
            let target = v.target_onsets.max(1.0) as usize;
            // Note length in slots, from the target count.
            let len = (slots / target.max(1)).clamp(1, slots);
            let len = if len >= 4 { len / 4 * 4 } else if len >= 2 { 2 } else { 1 };
            let mut prev = v.prev_pitch.unwrap_or((v.range.0 + v.range.1) / 2);
            let mut k = 0;
            while k < slots {
                let beat = k as f64 * slot_beats;
                let chord = crate::harmonize::chord_at(&plan.chords, beat);
                let pitch = match chord {
                    Some(c) if v.role == Role::Bass && meter_slot_is_chord_start(&plan.chords, beat) => {
                        // Bass takes the chord bass nearest the previous pitch.
                        let bass_pc = c.bass(&plan.key);
                        (v.range.0..=v.range.1)
                            .filter(|m| rameau_theory::PitchClass::of_midi(*m) == bass_pc)
                            .min_by_key(|m| (m - prev).abs())
                            .unwrap_or(prev)
                    }
                    Some(c) => {
                        let lo = (prev - 5).max(v.range.0);
                        let hi = (prev + 5).min(v.range.1);
                        let pool = c.tones_in_range(&plan.key, lo, hi);
                        self.rng.pick(&pool).copied().unwrap_or_else(|| c.nearest_tone(&plan.key, prev))
                    }
                    None => plan.key.snap(prev),
                };
                let this_len = len.min(slots - k);
                v.write(k, this_len, Some(pitch), false);
                prev = pitch;
                k += this_len;
            }
        }
    }
}

fn meter_slot_is_chord_start(chords: &[crate::harmonize::ChordSlot], beat: f64) -> bool {
    chords.iter().any(|c| (c.onset - beat).abs() < 1e-6)
}

/// A cadential trill: alternation with the upper neighbour, ending on the
/// main note.
fn trill(channel: u8, key: u8, scale: Scale, onset: f64, duration: f64, velocity: u8, voice: Option<usize>) -> Vec<ScoredNote> {
    let upper = scale.step_from(i32::from(key), 1).clamp(0, 127) as u8;
    let step = 0.125;
    let n = ((duration - 0.25) / step).floor().max(0.0) as usize;
    let n = n.min(12);
    let mut out = Vec::with_capacity(n + 1);
    for i in 0..n {
        let k = if i % 2 == 0 { upper } else { key };
        out.push(ScoredNote { channel, key: k, velocity: velocity.saturating_sub(8), onset: onset + i as f64 * step, duration: step * 0.9, voice, program: None });
    }
    let tail_onset = onset + n as f64 * step;
    out.push(ScoredNote { channel, key, velocity, onset: tail_onset, duration: (onset + duration - tail_onset).max(0.1), voice, program: None });
    out
}

/// Chords for the continuo: block chords, or a broken pattern when the
/// texture is light.
fn continuo_notes(rng: &mut SplitMix64, plan: &BarPlan, state: &MusicState, inst: &Instrument) -> Vec<ScoredNote> {
    let mut out = Vec::new();
    let vel = state.velocity(0.0).saturating_sub(18);
    let broken = matches!(inst.id, "harpe" | "luth") || (state.density < 0.35 && rng.chance(0.5));
    for slot in &plan.chords {
        let bass_pc = slot.chord.bass(&plan.key);
        let bass = (40..=52).find(|m| rameau_theory::PitchClass::of_midi(*m) == bass_pc).unwrap_or(48);
        let upper = slot.chord.tones_in_range(&plan.key, 55, 67);
        let mut chord: Vec<Midi> = vec![bass];
        chord.extend(upper.iter().take(3));
        if broken {
            let step = (slot.duration / chord.len() as f64).max(0.25);
            for (i, m) in chord.iter().enumerate() {
                let onset = slot.onset + i as f64 * step;
                if onset >= slot.onset + slot.duration {
                    break;
                }
                out.push(ScoredNote { channel: CONTINUO_CHANNEL, key: *m as u8, velocity: vel, onset, duration: (slot.duration - i as f64 * step).max(0.2), voice: None, program: None });
            }
        } else {
            // Oom-pah in a dance, sustained otherwise.
            let dance = state.density > 0.55 && state.articulation > 0.4;
            if dance {
                let pulse = plan.meter.pulse_quarters();
                let mut t = slot.onset;
                let mut k = 0;
                while t < slot.onset + slot.duration - 1e-6 {
                    if k % 2 == 0 {
                        out.push(ScoredNote { channel: CONTINUO_CHANNEL, key: bass as u8, velocity: vel, onset: t, duration: pulse * 0.5, voice: None, program: None });
                    } else {
                        for m in chord.iter().skip(1) {
                            out.push(ScoredNote { channel: CONTINUO_CHANNEL, key: *m as u8, velocity: vel.saturating_sub(6), onset: t, duration: pulse * 0.4, voice: None, program: None });
                        }
                    }
                    t += pulse / 2.0;
                    k += 1;
                }
            } else {
                for m in &chord {
                    out.push(ScoredNote { channel: CONTINUO_CHANNEL, key: *m as u8, velocity: vel, onset: slot.onset + 0.01, duration: slot.duration * 0.95, voice: None, program: None });
                }
            }
        }
    }
    out
}

/// Drums for the bar.
fn percussion_notes(rng: &mut SplitMix64, plan: &BarPlan, state: &MusicState, drums: &[&'static Instrument]) -> Vec<ScoredNote> {
    let mut out = Vec::new();
    let meter = plan.meter;
    let pulse = meter.pulse_quarters();
    let pulses = meter.pulses() as usize;
    let vel = state.velocity(0.3);
    for d in drums {
        match d.id {
            "tambour" => {
                // Side drum: beats, with subdivisions and rolls as things heat up.
                for p in 0..pulses {
                    let t = p as f64 * pulse;
                    let v = if p == 0 { vel } else { vel.saturating_sub(15) };
                    out.push(ScoredNote { channel: DRUM_CHANNEL, key: 38, velocity: v, onset: t, duration: 0.2, voice: None, program: None });
                    if state.percussion > 0.45 {
                        out.push(ScoredNote { channel: DRUM_CHANNEL, key: 38, velocity: vel.saturating_sub(30), onset: t + pulse / 2.0, duration: 0.2, voice: None, program: None });
                    }
                    if state.percussion > 0.7 && p + 1 == pulses && rng.chance(0.5) {
                        // A roll into the downbeat.
                        for r in 0..4 {
                            out.push(ScoredNote { channel: DRUM_CHANNEL, key: 38, velocity: vel.saturating_sub(35 - r * 6), onset: t + pulse * (0.5 + 0.125 * r as f64), duration: 0.1, voice: None, program: None });
                        }
                    }
                }
            }
            "tambourin" => {
                for p in 0..pulses {
                    let t = p as f64 * pulse;
                    out.push(ScoredNote { channel: DRUM_CHANNEL, key: 45, velocity: if p == 0 { vel } else { vel.saturating_sub(20) }, onset: t, duration: 0.3, voice: None, program: None });
                    if meter.is_compound() {
                        out.push(ScoredNote { channel: DRUM_CHANNEL, key: 47, velocity: vel.saturating_sub(30), onset: t + pulse * 2.0 / 3.0, duration: 0.2, voice: None, program: None });
                    }
                }
            }
            "timbales" => {
                // Tonic and dominant on the strong beats.
                let tonic = plan.key.tonic_midi(41);
                let dom = tonic + 7;
                let dom = if dom > 55 { dom - 12 } else { dom };
                for slot in &plan.chords {
                    let key = if slot.chord.is_dominant() { dom } else { tonic };
                    if meter.strength_at(slot.onset) >= BeatStrength::Strong || state.percussion > 0.8 {
                        out.push(ScoredNote { channel: TIMPANI_CHANNEL, key: key as u8, velocity: vel, onset: slot.onset, duration: slot.duration.min(1.0), voice: None, program: None });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::idea::{Citation, Idea, IdeaEvent};
    use rameau_theory::Mode;

    fn library() -> IdeaLibrary {
        let mut lib = IdeaLibrary::new();
        let mk = |id: &str, kind: IdeaKind, events: Vec<IdeaEvent>, roles: &[&str], metre: Meter, pairs: Vec<&str>| Idea {
            schema: "peasantide-idea/2".into(),
            id: id.into(),
            name: id.into(),
            kind,
            metre,
            mode: Mode::Major,
            tempo_hint: None,
            events,
            anacrusis: 0.0,
            lyrics: Some("Ah ça ira ça ira".into()),
            affect: vec![],
            roles: roles.iter().map(|s| (*s).to_owned()).collect(),
            phase_fit: vec![],
            order_fit: [0.0, 1.0],
            pairs_with: pairs.into_iter().map(str::to_owned).collect(),
            design_note: String::new(),
            citation: Citation::default(),
        };
        lib.insert(mk(
            "tune",
            IdeaKind::Motif,
            vec![
                IdeaEvent::note(0, 0, 0, 0.5), IdeaEvent::note(0, 0, 0, 0.25), IdeaEvent::note(1, 0, 0, 0.25),
                IdeaEvent::note(0, 0, 0, 0.5), IdeaEvent::note(0, 0, 0, 0.25), IdeaEvent::note(1, 0, 0, 0.25),
                IdeaEvent::note(0, 0, 0, 0.5), IdeaEvent::note(1, 0, 0, 0.5), IdeaEvent::note(2, 0, 0, 1.0),
                IdeaEvent::note(4, 0, 0, 1.0), IdeaEvent::note(3, 0, 0, 0.5), IdeaEvent::note(2, 0, 0, 0.5),
                IdeaEvent::note(1, 0, 0, 1.0), IdeaEvent::note(0, 0, 0, 1.0),
            ],
            &["hook", "refrain", "dance", "subject"],
            Meter::new(2, 4),
            vec!["cs"],
        ));
        lib.insert(mk(
            "cs",
            IdeaKind::Countersubject,
            vec![IdeaEvent::note(4, 0, -1, 1.0), IdeaEvent::note(3, 0, -1, 1.0), IdeaEvent::note(2, 0, -1, 1.0), IdeaEvent::note(4, 0, -1, 1.0)],
            &[],
            Meter::new(2, 4),
            vec![],
        ));
        lib.insert(mk(
            "ground",
            IdeaKind::Ground,
            vec![IdeaEvent::note(0, 0, 0, 3.0), IdeaEvent::note(6, 0, -1, 3.0), IdeaEvent::note(5, 0, -1, 3.0), IdeaEvent::note(4, 0, -1, 3.0)],
            &[],
            Meter::new(3, 4),
            vec!["tune"],
        ));
        lib
    }

    #[test]
    fn composes_bars_in_every_form() {
        for form in FormKind::ALL {
            let mut c = Composer::new(library(), 5);
            c.iterations = 150;
            c.set_state(MusicState { voices: 3, singing: 0.7, percussion: 0.6, wealth: 0.6, ..MusicState::default() });
            c.trigger(Trigger::Form(form));
            let mut total_notes = 0;
            let mut forms_seen = std::collections::HashSet::new();
            for _ in 0..24 {
                let bar = c.next_bar();
                forms_seen.insert(bar.form);
                total_notes += bar.notes.len();
                assert!(bar.beats > 0.0);
                for n in &bar.notes {
                    assert!(n.onset >= 0.0 && n.onset < bar.beats + 1e-9, "{form:?}: onset {} in {} beats", n.onset, bar.beats);
                    assert!(n.duration > 0.0);
                }
            }
            assert!(forms_seen.contains(&form), "{form:?} never ran: {forms_seen:?}");
            assert!(total_notes > 24 * 3, "{form:?} produced {total_notes} notes");
        }
    }

    #[test]
    fn triggers_and_state_changes_are_honoured() {
        let mut c = Composer::new(library(), 9);
        c.iterations = 100;
        c.trigger(Trigger::Form(FormKind::Air));
        let first = c.next_bar();
        assert_eq!(first.form, FormKind::Air);
        c.trigger(Trigger::Tocsin);
        let b = c.next_bar();
        assert!(b.notes.iter().any(|n| n.channel == STINGER_CHANNEL));
        let mut s = *c.state();
        s.voices = 5;
        s.tempo_bpm = 160.0;
        c.set_state(s);
        let b = c.next_bar();
        assert_eq!(b.tempo_bpm, 160.0);
        assert_eq!(b.instruments.len(), 5);
        c.trigger(Trigger::Lyric("Dansons la carmagnole".into()));
        c.trigger(Trigger::Form(FormKind::Air));
        let mut sung = 0;
        for _ in 0..8 {
            let b = c.next_bar();
            sung += b.notes.iter().filter(|n| n.channel == SINGER_CHANNEL).count();
        }
        assert!(sung > 0, "the lyric should be sung");
    }
}
