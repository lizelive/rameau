//! The instruments of France around 1789, and how the sliders pick among
//! them.
//!
//! Each [`Instrument`] is a period instrument mapped onto the nearest General
//! MIDI program, with the roles it can take, the range it plays in, and where
//! on the tavern-to-court axis it belongs. An [`Ensemble`] is one instrument
//! per voice role, chosen by scoring every candidate against the sliders and
//! changed only at phrase boundaries, with hysteresis, so the band does not
//! flicker.

use serde::{Deserialize, Serialize};

use rameau_theory::Midi;
use rameau_types::Rng;

use crate::state::{MusicState, Phase};

/// What a voice does in the texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Carries the tune or the subject entry.
    Lead,
    /// An inner counterpoint or filler voice.
    Inner,
    /// The bass line.
    Bass,
    /// A chord-realising keyboard or plucked instrument.
    Continuo,
    /// Unpitched percussion.
    Percussion,
    /// The singer or crowd.
    Singer,
    /// A bell, cannon or other one-shot stinger.
    Stinger,
}

/// Instrument family, for doubling decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    /// Bowed strings.
    Strings,
    /// Woodwind.
    Winds,
    /// Brass.
    Brass,
    /// Keyboards and plucked continuo.
    Keyboard,
    /// Folk instruments of the street and the tavern.
    Folk,
    /// Drums and bells.
    Percussion,
    /// Human voices.
    Voice,
}

/// A period instrument.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Instrument {
    /// Stable identifier.
    pub id: &'static str,
    /// French name.
    pub name: &'static str,
    /// Family.
    pub family: Family,
    /// MIDI bank (0 for General MIDI melodic).
    pub bank: u16,
    /// General MIDI program (0-based), or the drum key for percussion.
    pub program: u8,
    /// Playing range, MIDI keys.
    pub range: (Midi, Midi),
    /// Roles it can take.
    pub roles: &'static [Role],
    /// Where on the tavern (0) – court (1) axis it lives, as a centre and a
    /// half-width.
    pub order: (f64, f64),
    /// Minimum wealth to afford it.
    pub wealth_min: f64,
    /// How much destruction/percussion calls for it (bells, drums, brass).
    pub violence: f64,
    /// A note about the historical instrument and the substitution.
    pub note: &'static str,
}

/// The violin, which every ensemble can fall back on.
pub const VIOLON: Instrument = Instrument { id: "violon", name: "violon", family: Family::Strings, bank: 0, program: 40, range: (55, 96), roles: &[Role::Lead, Role::Inner], order: (0.55, 0.45), wealth_min: 0.1, violence: 0.1, note: "the violin: from the dancing master's pochette to the Vingt-quatre Violons du Roi" };

/// The catalogue.
pub const CATALOGUE: &[Instrument] = &[
    Instrument { id: "vielle", name: "vielle à roue", family: Family::Folk, bank: 0, program: 21, range: (55, 84), roles: &[Role::Lead, Role::Inner], order: (0.1, 0.25), wealth_min: 0.0, violence: 0.2, note: "hurdy-gurdy of the street singer; GM accordion stands in for its reedy drone" },
    Instrument { id: "musette", name: "musette de cour", family: Family::Folk, bank: 0, program: 109, range: (60, 84), roles: &[Role::Lead], order: (0.25, 0.3), wealth_min: 0.0, violence: 0.1, note: "the bellows bagpipe, played in taverns and, as pastoral affectation, at court" },
    Instrument { id: "fifre", name: "fifre", family: Family::Winds, bank: 0, program: 72, range: (72, 96), roles: &[Role::Lead], order: (0.2, 0.3), wealth_min: 0.0, violence: 0.6, note: "the fife of the militia and the National Guard; GM piccolo" },
    Instrument { id: "flute", name: "flûte traversière", family: Family::Winds, bank: 0, program: 73, range: (60, 93), roles: &[Role::Lead, Role::Inner], order: (0.65, 0.3), wealth_min: 0.2, violence: 0.0, note: "one-keyed traverso" },
    Instrument { id: "hautbois", name: "hautbois", family: Family::Winds, bank: 0, program: 68, range: (58, 88), roles: &[Role::Lead, Role::Inner], order: (0.6, 0.35), wealth_min: 0.2, violence: 0.2, note: "two-keyed oboe, the backbone of Lully's band and the military wind band" },
    Instrument { id: "clarinette", name: "clarinette", family: Family::Winds, bank: 0, program: 71, range: (50, 86), roles: &[Role::Lead, Role::Inner], order: (0.4, 0.3), wealth_min: 0.2, violence: 0.4, note: "five-keyed clarinet; new to the fête music of Gossec and the Garde nationale bands" },
    Instrument { id: "basson", name: "basson", family: Family::Winds, bank: 0, program: 70, range: (34, 67), roles: &[Role::Bass, Role::Inner], order: (0.5, 0.4), wealth_min: 0.15, violence: 0.2, note: "baroque bassoon, the wind bass" },
    Instrument { id: "serpent", name: "serpent", family: Family::Brass, bank: 0, program: 58, range: (36, 60), roles: &[Role::Bass], order: (0.3, 0.3), wealth_min: 0.1, violence: 0.5, note: "the church and military bass horn; GM tuba" },
    Instrument { id: "cor", name: "cor de chasse", family: Family::Brass, bank: 0, program: 60, range: (41, 77), roles: &[Role::Inner, Role::Lead], order: (0.6, 0.3), wealth_min: 0.3, violence: 0.5, note: "natural horn" },
    Instrument { id: "trompette", name: "trompette", family: Family::Brass, bank: 0, program: 56, range: (55, 84), roles: &[Role::Lead, Role::Inner], order: (0.5, 0.35), wealth_min: 0.3, violence: 0.8, note: "natural trumpet, fanfares and fêtes" },
    VIOLON,
    Instrument { id: "alto", name: "alto", family: Family::Strings, bank: 0, program: 41, range: (48, 84), roles: &[Role::Inner], order: (0.65, 0.35), wealth_min: 0.25, violence: 0.1, note: "the viola, the haute-contre and taille of the French five-part string band" },
    Instrument { id: "violoncelle", name: "violoncelle", family: Family::Strings, bank: 0, program: 42, range: (36, 72), roles: &[Role::Bass, Role::Inner], order: (0.65, 0.35), wealth_min: 0.25, violence: 0.1, note: "basse de violon / cello" },
    Instrument { id: "contrebasse", name: "contrebasse", family: Family::Strings, bank: 0, program: 43, range: (28, 55), roles: &[Role::Bass], order: (0.7, 0.3), wealth_min: 0.4, violence: 0.3, note: "doubling the bass an octave down" },
    Instrument { id: "cordes", name: "cordes", family: Family::Strings, bank: 0, program: 48, range: (40, 96), roles: &[Role::Inner, Role::Lead], order: (0.85, 0.2), wealth_min: 0.6, violence: 0.0, note: "the full string band, opéra-ballet texture" },
    Instrument { id: "pizzicato", name: "cordes pincées", family: Family::Strings, bank: 0, program: 45, range: (36, 84), roles: &[Role::Bass, Role::Inner], order: (0.6, 0.4), wealth_min: 0.3, violence: 0.0, note: "pizzicato strings for the light bass of an air" },
    Instrument { id: "clavecin", name: "clavecin", family: Family::Keyboard, bank: 0, program: 6, range: (29, 89), roles: &[Role::Continuo, Role::Lead, Role::Inner], order: (0.8, 0.3), wealth_min: 0.45, violence: 0.0, note: "harpsichord: the continuo of the salon and the opera pit" },
    Instrument { id: "pianoforte", name: "pianoforte", family: Family::Keyboard, bank: 0, program: 0, range: (29, 96), roles: &[Role::Continuo, Role::Lead], order: (0.7, 0.25), wealth_min: 0.5, violence: 0.0, note: "the new instrument of the 1780s salon" },
    Instrument { id: "orgue", name: "orgue", family: Family::Keyboard, bank: 0, program: 19, range: (29, 96), roles: &[Role::Continuo, Role::Inner, Role::Bass], order: (0.75, 0.3), wealth_min: 0.3, violence: 0.3, note: "church organ, for the fugue and the funeral" },
    Instrument { id: "luth", name: "guitare", family: Family::Keyboard, bank: 0, program: 24, range: (40, 84), roles: &[Role::Continuo, Role::Inner], order: (0.35, 0.35), wealth_min: 0.05, violence: 0.0, note: "guitar or lute, the street and salon continuo" },
    Instrument { id: "harpe", name: "harpe", family: Family::Keyboard, bank: 0, program: 46, range: (36, 96), roles: &[Role::Continuo, Role::Inner], order: (0.9, 0.15), wealth_min: 0.6, violence: 0.0, note: "the queen's own instrument" },
    Instrument { id: "timbales", name: "timbales", family: Family::Percussion, bank: 0, program: 47, range: (40, 55), roles: &[Role::Percussion, Role::Bass], order: (0.6, 0.5), wealth_min: 0.3, violence: 0.7, note: "timpani on tonic and dominant" },
    Instrument { id: "tambour", name: "tambour", family: Family::Percussion, bank: 128, program: 38, range: (38, 40), roles: &[Role::Percussion], order: (0.3, 0.4), wealth_min: 0.0, violence: 0.8, note: "the side drum of the Garde; GM snare" },
    Instrument { id: "tambourin", name: "tambourin", family: Family::Percussion, bank: 128, program: 45, range: (41, 47), roles: &[Role::Percussion], order: (0.15, 0.25), wealth_min: 0.0, violence: 0.5, note: "the Provençal long drum under the galoubet; GM low tom" },
    Instrument { id: "tocsin", name: "tocsin", family: Family::Percussion, bank: 0, program: 14, range: (55, 79), roles: &[Role::Stinger], order: (0.5, 0.6), wealth_min: 0.0, violence: 1.0, note: "the alarm bell; GM tubular bells" },
    Instrument { id: "canon", name: "canon", family: Family::Percussion, bank: 128, program: 49, range: (49, 57), roles: &[Role::Stinger], order: (0.5, 0.6), wealth_min: 0.0, violence: 1.0, note: "the cannon of the Carmagnole; GM crash" },
    Instrument { id: "voix", name: "voix", family: Family::Voice, bank: rameau_voix::SOLO_BANK, program: 0, range: (48, 79), roles: &[Role::Singer], order: (0.5, 0.6), wealth_min: 0.0, violence: 0.0, note: "one singer (formant synthesis)" },
    Instrument { id: "choeur", name: "chœur", family: Family::Voice, bank: rameau_voix::CHOIR_BANK, program: 0, range: (43, 79), roles: &[Role::Singer], order: (0.4, 0.5), wealth_min: 0.0, violence: 0.3, note: "the crowd singing (formant synthesis)" },
];

impl Instrument {
    /// Looks up an instrument by id.
    pub fn by_id(id: &str) -> Option<&'static Instrument> {
        CATALOGUE.iter().find(|i| i.id == id)
    }

    /// Whether the instrument can take `role`.
    pub fn plays(&self, role: Role) -> bool {
        self.roles.contains(&role)
    }

    /// Whether it is unpitched (plays on the drum channel).
    pub const fn is_drum(&self) -> bool {
        self.bank == 128
    }

    /// The centre of its range.
    pub const fn centre(&self) -> Midi {
        (self.range.0 + self.range.1) / 2
    }

    /// How well it fits the sliders for `role`, `0..`.
    pub fn fit(&self, state: &MusicState, role: Role) -> f64 {
        if !self.plays(role) {
            return 0.0;
        }
        let (centre, half) = self.order;
        let d = (state.refinement - centre).abs() / half.max(0.05);
        let order_fit = (-(d * d) * 0.7).exp();
        let afford = if state.wealth + 0.05 >= self.wealth_min { 1.0 } else { 0.08 };
        let violence = 1.0 + self.violence * (state.percussion * 1.5 + state.license) - 0.4 * self.violence * (1.0 - state.percussion);
        let phase = match (state.phase, self.family) {
            (Phase::Retreat, Family::Brass) => 0.5,
            (Phase::Retreat, Family::Keyboard) if self.id == "orgue" => 1.6,
            (Phase::Riot, Family::Folk | Family::Percussion) => 1.3,
            (Phase::Recruit, Family::Winds) if self.id == "fifre" || self.id == "clarinette" => 1.3,
            _ => 1.0,
        };
        let register = match role {
            Role::Lead => 1.0 + 0.5 * (state.register - 0.5) * ((self.centre() - 72) as f64 / 12.0),
            _ => 1.0,
        };
        (order_fit * afford * violence.max(0.05) * phase * register.max(0.2)).max(0.0)
    }
}

/// One instrument per voice, plus the optional continuo, percussion and
/// singer.
#[derive(Debug, Clone, PartialEq)]
pub struct Ensemble {
    /// Melodic voices, highest first.
    pub voices: Vec<&'static Instrument>,
    /// Chord-realising instrument, if the sliders can afford one.
    pub continuo: Option<&'static Instrument>,
    /// Drums, if any.
    pub percussion: Vec<&'static Instrument>,
    /// Singer, if any.
    pub singer: Option<&'static Instrument>,
}

impl Ensemble {
    /// Chooses an ensemble for `state` with `n` melodic voices.
    ///
    /// `previous` biases the choice toward what was already playing.
    pub fn choose(rng: &mut impl Rng, state: &MusicState, n: usize, previous: Option<&Ensemble>) -> Self {
        let n = n.clamp(1, 6);
        let mut voices = Vec::with_capacity(n);
        for v in 0..n {
            let role = if v + 1 == n && n > 1 { Role::Bass } else if v == 0 { Role::Lead } else { Role::Inner };
            let prev = previous.and_then(|p| p.voices.get(v).copied());
            let pick = pick(rng, state, role, prev, &voices);
            voices.push(pick);
        }
        let continuo = if state.wealth > 0.3 && state.polyphony < 0.85 {
            let prev = previous.and_then(|p| p.continuo);
            Some(pick(rng, state, Role::Continuo, prev, &[]))
        } else {
            None
        };
        let mut percussion = Vec::new();
        if state.percussion > 0.25 {
            let prev = previous.and_then(|p| p.percussion.first().copied());
            percussion.push(pick(rng, state, Role::Percussion, prev, &[]));
            if state.percussion > 0.65
                && state.wealth > 0.3
                && let Some(t) = Instrument::by_id("timbales")
            {
                percussion.push(t);
            }
        }
        let singer = if state.singing > 0.15 {
            let crowd = state.singing > 0.55 || state.refinement < 0.3;
            Instrument::by_id(if crowd { "choeur" } else { "voix" })
        } else {
            None
        };
        Self {
            voices,
            continuo,
            percussion,
            singer,
        }
    }

    /// A one-line description.
    pub fn describe(&self) -> String {
        let mut parts: Vec<&str> = self.voices.iter().map(|i| i.name).collect();
        if let Some(c) = self.continuo {
            parts.push(c.name);
        }
        parts.extend(self.percussion.iter().map(|i| i.name));
        if let Some(s) = self.singer {
            parts.push(s.name);
        }
        parts.join(", ")
    }
}

/// Picks an instrument for `role`, favouring `previous` (hysteresis) and
/// avoiding instruments already used unless the palette is thin.
fn pick(
    rng: &mut impl Rng,
    state: &MusicState,
    role: Role,
    previous: Option<&'static Instrument>,
    used: &[&'static Instrument],
) -> &'static Instrument {
    let candidates: Vec<&'static Instrument> = CATALOGUE.iter().filter(|i| i.plays(role)).collect();
    let weights: Vec<f64> = candidates
        .iter()
        .map(|i| {
            let mut w = i.fit(state, role);
            if previous == Some(*i) {
                w *= 9.0;
            }
            if used.contains(i) {
                w *= 0.35;
            }
            w
        })
        .collect();
    let idx = rng.weighted(&weights).unwrap_or(0);
    candidates
        .get(idx)
        .or(candidates.first())
        .copied()
        .unwrap_or(&VIOLON)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rameau_types::SplitMix64;

    #[test]
    fn taverns_get_folk_and_courts_get_strings() {
        let mut rng = SplitMix64::new(1);
        let tavern = MusicState {
            refinement: 0.05,
            wealth: 0.1,
            percussion: 0.5,
            ..MusicState::default()
        };
        let mut folk = 0;
        for _ in 0..40 {
            let e = Ensemble::choose(&mut rng, &tavern, 2, None);
            if matches!(e.voices[0].family, Family::Folk | Family::Winds) {
                folk += 1;
            }
            assert!(e.continuo.is_none(), "no harpsichord in a tavern");
            assert!(!e.percussion.is_empty());
        }
        assert!(folk > 25, "tavern leads were folk/wind {folk}/40");

        let court = MusicState {
            refinement: 0.95,
            wealth: 0.9,
            percussion: 0.0,
            ..MusicState::default()
        };
        let mut strings = 0;
        for _ in 0..40 {
            let e = Ensemble::choose(&mut rng, &court, 4, None);
            if e.voices.iter().any(|i| i.family == Family::Strings) {
                strings += 1;
            }
            assert!(e.continuo.is_some());
        }
        assert!(strings > 30);
    }

    #[test]
    fn hysteresis_keeps_the_band_together() {
        let mut rng = SplitMix64::new(2);
        let s = MusicState::default();
        let first = Ensemble::choose(&mut rng, &s, 3, None);
        let mut same = 0;
        for _ in 0..30 {
            let next = Ensemble::choose(&mut rng, &s, 3, Some(&first));
            if next.voices[0] == first.voices[0] {
                same += 1;
            }
        }
        assert!(same > 20, "lead kept {same}/30");
        assert!(Instrument::by_id("tocsin").unwrap().plays(Role::Stinger));
    }
}
