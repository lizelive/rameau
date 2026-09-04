//! The idea library: motifs, grounds, subjects and countersubjects with
//! their provenance.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use rameau_theory::{DegreeNote, Meter, Mode};

use crate::state::Phase;

/// What kind of material an idea is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdeaKind {
    /// A melodic motif or tune strain.
    Motif,
    /// A ground bass (chaconne, passacaille, folia, lament).
    Ground,
    /// A fugue subject.
    Subject,
    /// A countersubject written to go with a subject.
    Countersubject,
    /// A rhythmic cell (a dance rhythm) with no fixed pitches.
    Rhythm,
}

impl IdeaKind {
    /// The kinds, in a stable order.
    pub const ALL: [IdeaKind; 5] = [
        IdeaKind::Motif,
        IdeaKind::Ground,
        IdeaKind::Subject,
        IdeaKind::Countersubject,
        IdeaKind::Rhythm,
    ];

    /// The directory name ideas of this kind live in.
    pub const fn dir(self) -> &'static str {
        match self {
            IdeaKind::Motif => "motifs",
            IdeaKind::Ground => "grounds",
            IdeaKind::Subject => "subjects",
            IdeaKind::Countersubject => "countersubjects",
            IdeaKind::Rhythm => "rhythms",
        }
    }
}

/// Where an idea came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provenance {
    /// Extracted from a cited, machine-readable source file.
    Sourced,
    /// A documented common practice of the period, with no single source.
    PeriodConvention,
    /// Written down from memory; unverified against a source.
    TranscribedFromMemory,
    /// Newly written to fit the sourced material (countersubjects, mostly).
    ComposedForThisSystem,
}

/// Provenance and citation for an idea.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Citation {
    /// How trustworthy the transcription is.
    pub provenance: Option<Provenance>,
    /// The work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<String>,
    /// The composer, as best known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composer: Option<String>,
    /// When it was composed or first printed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composed: Option<String>,
    /// Political colour, where the song had one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faction: Option<String>,
    /// The piece id in the peasanttide corpus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corpus_piece_id: Option<String>,
    /// The file in the corpus the notes were read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corpus_file: Option<String>,
    /// Bars of the source the idea was cut from, inclusive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bars: Option<[u32; 2]>,
    /// The voice/spine of the source the idea was cut from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    /// The key of the source, as printed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_key: Option<String>,
    /// The encoding's origin (site or project).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// URL of the encoding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    /// Who encoded it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder: Option<String>,
    /// The encoding's licence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Whether the music itself is public domain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub music_public_domain: Option<bool>,
    /// How the notes were extracted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction: Option<String>,
    /// Free-form note about this particular cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One event of an idea: a note as a scale degree, or a rest.
///
/// Serialised as `{"d": 0, "a": 0, "o": 1, "t": 0.5}` for a note and
/// `{"r": true, "t": 1.0}` for a rest. `t` is in crotchets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IdeaEvent {
    /// The note, or `None` for a rest.
    pub note: Option<DegreeNote>,
    /// Duration in crotchets.
    pub beats: f64,
    /// Ornament sign from the source, if any (`T` trill, `M` mordent …).
    pub ornament: Option<char>,
}

impl IdeaEvent {
    /// A sounding note.
    pub const fn note(degree: i32, alteration: i8, octave: i32, beats: f64) -> Self {
        Self {
            note: Some(DegreeNote::new(degree, alteration, octave)),
            beats,
            ornament: None,
        }
    }

    /// A rest.
    pub const fn rest(beats: f64) -> Self {
        Self {
            note: None,
            beats,
            ornament: None,
        }
    }

    /// Whether this is a rest.
    pub const fn is_rest(&self) -> bool {
        self.note.is_none()
    }
}

#[derive(Serialize, Deserialize)]
struct RawEvent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    d: Option<i32>,
    #[serde(default, skip_serializing_if = "is_zero_i8")]
    a: i8,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    o: i32,
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    r: bool,
    t: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    orn: Option<char>,
}

fn is_zero_i8(v: &i8) -> bool {
    *v == 0
}
fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}

impl Serialize for IdeaEvent {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let raw = match self.note {
            Some(n) => RawEvent {
                d: Some(n.degree as i32),
                a: n.alteration,
                o: n.octave as i32,
                r: false,
                t: self.beats,
                orn: self.ornament,
            },
            None => RawEvent {
                d: None,
                a: 0,
                o: 0,
                r: true,
                t: self.beats,
                orn: None,
            },
        };
        raw.serialize(s)
    }
}

impl<'de> Deserialize<'de> for IdeaEvent {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawEvent::deserialize(d)?;
        let note = match (raw.r, raw.d) {
            (false, Some(deg)) => Some(DegreeNote::new(deg, raw.a, raw.o)),
            _ => None,
        };
        Ok(Self {
            note,
            beats: raw.t,
            ornament: raw.orn,
        })
    }
}

/// A musical idea, stored in scale degrees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Idea {
    /// Schema tag, `"peasantide-idea/2"`.
    #[serde(default = "default_schema")]
    pub schema: String,
    /// Stable identifier, e.g. `caira.A`.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// What kind of material this is.
    pub kind: IdeaKind,
    /// The metre it was written in.
    #[serde(with = "meter_serde")]
    pub metre: Meter,
    /// The mode it was written in.
    pub mode: Mode,
    /// Tempo of the source in crotchets per minute, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tempo_hint: Option<f64>,
    /// The events.
    pub events: Vec<IdeaEvent>,
    /// Crotchets of pickup before the first downbeat (0 when the idea
    /// starts on the beat). The forms pad the idea so its downbeat lands on
    /// a barline.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub anacrusis: f64,
    /// The text sung to it, if it is a song.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lyrics: Option<String>,
    /// Descriptive tags.
    #[serde(default)]
    pub affect: Vec<String>,
    /// Roles it plays well: `hook`, `subject`, `episode`, `refrain`, `dance`.
    #[serde(default)]
    pub roles: Vec<String>,
    /// Which game phases it suits.
    #[serde(default)]
    pub phase_fit: Vec<Phase>,
    /// The range of the *order* slider (0 tavern … 1 court) it suits.
    #[serde(default = "full_range")]
    pub order_fit: [f32; 2],
    /// Ideas that go with this one (countersubjects for a subject, the
    /// ground for a chaconne tune).
    #[serde(default)]
    pub pairs_with: Vec<String>,
    /// Why it is in the library and how the engine should treat it.
    #[serde(default)]
    pub design_note: String,
    /// Provenance.
    #[serde(default)]
    pub citation: Citation,
}

fn default_schema() -> String {
    "peasantide-idea/2".to_owned()
}

fn full_range() -> [f32; 2] {
    [0.0, 1.0]
}

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

mod meter_serde {
    use rameau_theory::Meter;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(m: &Meter, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&m.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Meter, D::Error> {
        let text = String::deserialize(d)?;
        Meter::parse(&text).ok_or_else(|| serde::de::Error::custom(format!("bad metre {text:?}")))
    }
}

impl Idea {
    /// Total length in crotchets.
    pub fn beats(&self) -> f64 {
        self.events.iter().map(|e| e.beats).sum()
    }

    /// Length in bars of its own metre (rounded up).
    pub fn bars(&self) -> usize {
        (self.beats() / self.metre.bar_quarters() - 1e-9).ceil().max(1.0) as usize
    }

    /// Number of sounding notes.
    pub fn note_count(&self) -> usize {
        self.events.iter().filter(|e| !e.is_rest()).count()
    }

    /// Whether the idea suits `phase` (an empty list suits everything).
    pub fn fits_phase(&self, phase: Phase) -> bool {
        self.phase_fit.is_empty() || self.phase_fit.contains(&phase)
    }

    /// How well the idea suits an *order* value: 1 inside its range, falling
    /// off linearly outside it.
    pub fn order_affinity(&self, order: f32) -> f32 {
        let [lo, hi] = self.order_fit;
        if order >= lo && order <= hi {
            1.0
        } else if order < lo {
            (1.0 - (lo - order) * 2.0).max(0.0)
        } else {
            (1.0 - (order - hi) * 2.0).max(0.0)
        }
    }

    /// The events padded with an initial rest so that the first downbeat
    /// lands on a barline of the idea's metre (a pickup is placed at the
    /// end of the padding bar). Mutated `events` may be passed in.
    pub fn with_pickup(&self, events: &[IdeaEvent]) -> Vec<IdeaEvent> {
        if self.anacrusis <= 1e-9 {
            return events.to_vec();
        }
        let bar = self.metre.bar_quarters();
        let pad = bar - self.anacrusis.rem_euclid(bar);
        if pad <= 1e-9 || pad >= bar - 1e-9 {
            return events.to_vec();
        }
        let mut out = Vec::with_capacity(events.len() + 1);
        out.push(IdeaEvent::rest(pad));
        out.extend_from_slice(events);
        out
    }

    /// Whether the idea carries a role tag.
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// The provenance, defaulting to "sourced" when unstated.
    pub fn provenance(&self) -> Provenance {
        self.citation.provenance.unwrap_or(Provenance::Sourced)
    }
}

/// A set of ideas, indexed by id.
#[derive(Debug, Clone, Default)]
pub struct IdeaLibrary {
    ideas: Vec<Idea>,
    index: HashMap<String, usize>,
}

/// Why a library could not be loaded.
#[derive(Debug)]
pub enum LibraryError {
    /// A file could not be read.
    Io(std::io::Error),
    /// A file was not a valid idea.
    Json {
        /// Which file (or bundle entry).
        file: String,
        /// The parse error.
        error: serde_json::Error,
    },
}

impl core::fmt::Display for LibraryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LibraryError::Io(e) => write!(f, "io error: {e}"),
            LibraryError::Json { file, error } => write!(f, "{file}: {error}"),
        }
    }
}

impl core::error::Error for LibraryError {}

impl From<std::io::Error> for LibraryError {
    fn from(e: std::io::Error) -> Self {
        LibraryError::Io(e)
    }
}

impl IdeaLibrary {
    /// An empty library.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an idea, replacing any with the same id.
    pub fn insert(&mut self, idea: Idea) {
        if let Some(&i) = self.index.get(&idea.id) {
            if let Some(slot) = self.ideas.get_mut(i) {
                *slot = idea;
            }
            return;
        }
        self.index.insert(idea.id.clone(), self.ideas.len());
        self.ideas.push(idea);
    }

    /// Loads every `*.json` idea under `dir` (recursively).
    ///
    /// # Errors
    ///
    /// Returns [`LibraryError`] on an unreadable directory or an invalid file.
    pub fn load_dir(dir: impl AsRef<Path>) -> Result<Self, LibraryError> {
        let mut lib = Self::new();
        lib.load_dir_into(dir.as_ref())?;
        Ok(lib)
    }

    fn load_dir_into(&mut self, dir: &Path) -> Result<(), LibraryError> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)?.flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                self.load_dir_into(&path)?;
            } else if path.extension().is_some_and(|x| x == "json")
                && path.file_name().is_none_or(|n| n != "index.json")
            {
                let text = std::fs::read_to_string(&path)?;
                let idea: Idea = serde_json::from_str(&text).map_err(|error| LibraryError::Json {
                    file: path.display().to_string(),
                    error,
                })?;
                self.insert(idea);
            }
        }
        Ok(())
    }

    /// Loads a bundle: a JSON array of ideas.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryError::Json`] if the text is not an array of ideas.
    pub fn from_bundle(json: &str) -> Result<Self, LibraryError> {
        let ideas: Vec<Idea> = serde_json::from_str(json).map_err(|error| LibraryError::Json {
            file: "bundle".to_owned(),
            error,
        })?;
        let mut lib = Self::new();
        for idea in ideas {
            lib.insert(idea);
        }
        Ok(lib)
    }

    /// Serialises the whole library as a JSON array.
    ///
    /// # Errors
    ///
    /// Returns the serialisation error, which cannot happen for valid ideas.
    pub fn to_bundle(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.ideas)
    }

    /// Looks an idea up by id.
    pub fn get(&self, id: &str) -> Option<&Idea> {
        self.index.get(id).and_then(|&i| self.ideas.get(i))
    }

    /// Every idea.
    pub fn all(&self) -> &[Idea] {
        &self.ideas
    }

    /// Ideas of one kind.
    pub fn of_kind(&self, kind: IdeaKind) -> impl Iterator<Item = &Idea> {
        self.ideas.iter().filter(move |i| i.kind == kind)
    }

    /// Number of ideas.
    pub fn len(&self) -> usize {
        self.ideas.len()
    }

    /// Whether the library is empty.
    pub fn is_empty(&self) -> bool {
        self.ideas.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_round_trip_in_the_compact_form() {
        let e = IdeaEvent::note(3, 1, 1, 0.5);
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(json, r#"{"d":3,"a":1,"o":1,"t":0.5}"#);
        let back: IdeaEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
        let r: IdeaEvent = serde_json::from_str(r#"{"r":true,"t":1.0}"#).unwrap();
        assert!(r.is_rest());
        assert_eq!(serde_json::to_string(&r).unwrap(), r#"{"r":true,"t":1.0}"#);
    }

    #[test]
    fn idea_round_trips() {
        let idea = Idea {
            schema: default_schema(),
            id: "test.A".into(),
            name: "Test".into(),
            kind: IdeaKind::Motif,
            metre: Meter::new(2, 4),
            mode: Mode::Major,
            tempo_hint: Some(112.0),
            events: vec![IdeaEvent::note(0, 0, 0, 1.0), IdeaEvent::rest(1.0)],
            anacrusis: 0.0,
            lyrics: Some("Ah".into()),
            affect: vec!["defiant".into()],
            roles: vec!["hook".into()],
            phase_fit: vec![Phase::Riot],
            order_fit: [0.0, 0.6],
            pairs_with: vec![],
            design_note: String::new(),
            citation: Citation {
                provenance: Some(Provenance::Sourced),
                ..Default::default()
            },
        };
        let json = serde_json::to_string_pretty(&idea).unwrap();
        assert!(json.contains("\"metre\": \"2/4\""));
        let back: Idea = serde_json::from_str(&json).unwrap();
        assert_eq!(back, idea);
        assert_eq!(idea.bars(), 1);
        assert!(idea.fits_phase(Phase::Riot));
        assert!(!idea.fits_phase(Phase::Retreat));
        assert_eq!(idea.order_affinity(0.3), 1.0);
        assert!(idea.order_affinity(0.9) < 0.5);
        let mut lib = IdeaLibrary::new();
        lib.insert(idea.clone());
        lib.insert(idea);
        assert_eq!(lib.len(), 1);
        let bundle = lib.to_bundle().unwrap();
        assert_eq!(IdeaLibrary::from_bundle(&bundle).unwrap().len(), 1);
    }
}
