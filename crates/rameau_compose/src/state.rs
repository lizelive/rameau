//! The musical sliders that drive composition.

use serde::{Deserialize, Serialize};

/// The dramatic phase the music is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Gathering: marching songs, refrains, recruitment.
    #[default]
    Recruit,
    /// The riot itself: dances, hammering rhythms, stretto.
    Riot,
    /// The withdrawal: laments, grounds, thinning textures.
    Retreat,
}

impl Phase {
    /// All phases.
    pub const ALL: [Phase; 3] = [Phase::Recruit, Phase::Riot, Phase::Retreat];

    /// Lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            Phase::Recruit => "recruit",
            Phase::Riot => "riot",
            Phase::Retreat => "retreat",
        }
    }

    /// Parses a phase name.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "recruit" => Some(Phase::Recruit),
            "riot" => Some(Phase::Riot),
            "retreat" => Some(Phase::Retreat),
            _ => None,
        }
    }
}

/// The forms the engine can write in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormKind {
    /// Subject, answer, episodes, stretto.
    Fugue,
    /// Refrain and couplets.
    Rondeau,
    /// Variations over a ground bass.
    Chaconne,
    /// A tune with accompaniment, possibly sung.
    Air,
    /// A lively strain-form dance with drums.
    Contredanse,
}

impl FormKind {
    /// All forms.
    pub const ALL: [FormKind; 5] = [
        FormKind::Fugue,
        FormKind::Rondeau,
        FormKind::Chaconne,
        FormKind::Air,
        FormKind::Contredanse,
    ];

    /// Lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            FormKind::Fugue => "fugue",
            FormKind::Rondeau => "rondeau",
            FormKind::Chaconne => "chaconne",
            FormKind::Air => "air",
            FormKind::Contredanse => "contredanse",
        }
    }

    /// Parses a form name.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|f| f.name() == s.trim().to_ascii_lowercase())
    }
}

/// The musical sliders. Every value other than `tempo_bpm`, `voices` and
/// `phase` lies in `0.0..=1.0`.
///
/// A game maps its own state onto these; nothing in the engine knows what a
/// *horde* is, only how many voices to write and how hard to hit them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MusicState {
    /// Crotchets per minute.
    pub tempo_bpm: f64,
    /// Rhythmic density target: 0 whole notes, 1 running semiquavers.
    pub density: f64,
    /// Number of melodic voices to write, `1..=6`.
    pub voices: usize,
    /// How much dissonance is welcome on strong beats.
    pub dissonance: f64,
    /// Licence to break the rules: lowers the weight of parallels and
    /// foreign notes. 0 strict counterpoint, 1 anything goes.
    pub license: f64,
    /// Pull toward the minor mode (and, above 0.8, the Phrygian).
    pub darkness: f64,
    /// Chromatic alterations allowed in free voices.
    pub chromaticism: f64,
    /// Register: 0 low and heavy, 1 high and bright.
    pub register: f64,
    /// Loudness.
    pub dynamics: f64,
    /// Articulation: 0 legato, 1 detached.
    pub articulation: f64,
    /// Ornamentation: trills, mordents, coulés.
    pub ornament: f64,
    /// Texture: 0 homophonic (tune and chords), 1 independent counterpoint.
    pub polyphony: f64,
    /// Refinement of the ensemble: 0 tavern, 1 court.
    pub refinement: f64,
    /// Richness of the ensemble: 0 a fiddle and a drum, 1 the full band.
    pub wealth: f64,
    /// Percussion presence.
    pub percussion: f64,
    /// Singing presence (0 instrumental, 1 the crowd sings every tune).
    pub singing: f64,
    /// Dramatic phase.
    pub phase: Phase,
}

impl Default for MusicState {
    fn default() -> Self {
        Self {
            tempo_bpm: 108.0,
            density: 0.45,
            voices: 3,
            dissonance: 0.2,
            license: 0.1,
            darkness: 0.3,
            chromaticism: 0.15,
            register: 0.5,
            dynamics: 0.6,
            articulation: 0.4,
            ornament: 0.3,
            polyphony: 0.5,
            refinement: 0.4,
            wealth: 0.4,
            percussion: 0.2,
            singing: 0.2,
            phase: Phase::Recruit,
        }
    }
}

impl MusicState {
    /// Clamps every slider into its range.
    pub fn clamped(mut self) -> Self {
        let c = |v: f64| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        self.tempo_bpm = if self.tempo_bpm.is_finite() { self.tempo_bpm.clamp(36.0, 200.0) } else { 100.0 };
        self.density = c(self.density);
        self.voices = self.voices.clamp(1, 6);
        self.dissonance = c(self.dissonance);
        self.license = c(self.license);
        self.darkness = c(self.darkness);
        self.chromaticism = c(self.chromaticism);
        self.register = c(self.register);
        self.dynamics = c(self.dynamics);
        self.articulation = c(self.articulation);
        self.ornament = c(self.ornament);
        self.polyphony = c(self.polyphony);
        self.refinement = c(self.refinement);
        self.wealth = c(self.wealth);
        self.percussion = c(self.percussion);
        self.singing = c(self.singing);
        self
    }

    /// The largest change in any unit slider between two states (tempo is
    /// scaled to the same range), used to decide whether a pending bar
    /// should be recomposed.
    pub fn distance(&self, other: &Self) -> f64 {
        let pairs = [
            (self.density, other.density),
            (self.dissonance, other.dissonance),
            (self.license, other.license),
            (self.darkness, other.darkness),
            (self.register, other.register),
            (self.dynamics, other.dynamics),
            (self.polyphony, other.polyphony),
            (self.refinement, other.refinement),
            (self.wealth, other.wealth),
            (self.percussion, other.percussion),
            (self.singing, other.singing),
            ((self.tempo_bpm - 36.0) / 164.0, (other.tempo_bpm - 36.0) / 164.0),
            (self.voices as f64 / 6.0, other.voices as f64 / 6.0),
        ];
        let mut d = pairs
            .iter()
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        if self.phase != other.phase {
            d = d.max(1.0);
        }
        d
    }

    /// Duration of `beats` crotchets at this tempo, in seconds.
    pub fn seconds_per_beat(&self) -> f64 {
        60.0 / self.tempo_bpm.max(1.0)
    }

    /// The MIDI velocity for a note at the current dynamics, with `accent`
    /// (0..1) added for strong beats.
    pub fn velocity(&self, accent: f64) -> u8 {
        let v = 40.0 + 75.0 * self.dynamics + 12.0 * accent;
        v.clamp(1.0, 127.0) as u8
    }
}

/// The weight of each rule in the annealer's cost, derived from the sliders.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RuleWeights {
    /// Parallel fifths and octaves.
    pub parallels: f64,
    /// Direct (hidden) fifths and octaves.
    pub direct: f64,
    /// Voice crossing and overlap.
    pub crossing: f64,
    /// Wide spacing between upper voices.
    pub spacing: f64,
    /// Awkward melodic leaps.
    pub leaps: f64,
    /// Unresolved leading tone.
    pub leading_tone: f64,
    /// Notes foreign to the key.
    pub out_of_key: f64,
    /// Dissonance against the bass on a strong beat.
    pub strong_dissonance: f64,
    /// Dissonance on a weak beat not approached and left by step.
    pub weak_dissonance: f64,
    /// Non-chord tone on a chord onset.
    pub non_chord_tone: f64,
    /// Bass not on the chord's bass at a chord change.
    pub bass_mismatch: f64,
    /// Density away from the target.
    pub density: f64,
    /// A required voice resting most of the bar.
    pub silence: f64,
    /// Similarity to recent bars.
    pub repetition: f64,
    /// Notes outside the voice's range.
    pub range: f64,
    /// A run of repeated pitches.
    pub monotony: f64,
}

impl RuleWeights {
    /// Weights for a state: destruction (`license`) lowers the grammar rules,
    /// anger (`dissonance`) lowers the dissonance rules.
    pub fn from_state(s: &MusicState) -> Self {
        let strict = 1.0 - s.license;
        let consonant = 1.0 - s.dissonance;
        Self {
            parallels: 6.0 * strict,
            direct: 1.5 * strict,
            crossing: 2.5,
            spacing: 0.8,
            leaps: 1.5,
            leading_tone: 1.2 * strict,
            out_of_key: 4.0 * strict * (1.0 - 0.6 * s.chromaticism),
            strong_dissonance: 3.0 * consonant,
            weak_dissonance: 1.0 * consonant,
            non_chord_tone: 1.5 * (0.4 + 0.6 * consonant),
            bass_mismatch: 2.0,
            density: 3.0,
            silence: 2.0,
            repetition: 2.0,
            range: 3.0,
            monotony: 0.6,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_and_clamp() {
        let a = MusicState::default();
        let mut b = a;
        b.density = 1.5;
        b.voices = 9;
        let b = b.clamped();
        assert_eq!(b.density, 1.0);
        assert_eq!(b.voices, 6);
        assert!((a.distance(&b) - 0.55).abs() < 1e-9);
        let mut c = a;
        c.phase = Phase::Riot;
        assert_eq!(a.distance(&c), 1.0);
        assert_eq!(FormKind::parse("Fugue"), Some(FormKind::Fugue));
        assert_eq!(Phase::parse("riot"), Some(Phase::Riot));
        assert_eq!(a.velocity(0.0), 85);
    }
}
