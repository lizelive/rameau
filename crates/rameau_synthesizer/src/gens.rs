//! Resolution of SoundFont generators into a flat parameter set.
//!
//! A SoundFont describes a voice indirectly: a preset zone points at an
//! instrument, an instrument zone points at a sample, and each zone carries a
//! list of *generators* (synthesis parameters). To articulate a note the
//! synthesizer must merge those layers into a single value per generator.
//!
//! The merge rules from the SoundFont 2 specification are:
//!
//! * Instrument-level generators are **absolute**: a later zone *replaces* the
//!   value of an earlier one (global zone, then the matching local zone).
//! * Preset-level generators are **relative**: they are *added* on top of the
//!   resolved instrument value.
//!
//! [`Gens`] holds one [`i32`] per generator and implements exactly that.

use rameau_soundfont::{GeneratorAmount, GeneratorType as G, Range, Zone};

/// Number of distinct generators (`GeneratorType::COUNT`).
const N: usize = G::COUNT as usize;

/// A resolved set of generator values, one slot per [`GeneratorType`].
#[derive(Debug, Clone)]
pub struct Gens {
    v: [i32; N],
}

impl Gens {
    /// The SoundFont default generator values (spec table, "Default Value").
    pub fn defaults() -> Self {
        let mut v = [0i32; N];
        v[G::INITIAL_FILTER_CUTOFF_FREQUENCY as usize] = 13_500;
        v[G::DELAY_MODULATION_LFO as usize] = -12_000;
        v[G::DELAY_VIBRATO_LFO as usize] = -12_000;
        v[G::DELAY_MODULATION_ENVELOPE as usize] = -12_000;
        v[G::ATTACK_MODULATION_ENVELOPE as usize] = -12_000;
        v[G::HOLD_MODULATION_ENVELOPE as usize] = -12_000;
        v[G::DECAY_MODULATION_ENVELOPE as usize] = -12_000;
        v[G::RELEASE_MODULATION_ENVELOPE as usize] = -12_000;
        v[G::DELAY_VOLUME_ENVELOPE as usize] = -12_000;
        v[G::ATTACK_VOLUME_ENVELOPE as usize] = -12_000;
        v[G::HOLD_VOLUME_ENVELOPE as usize] = -12_000;
        v[G::DECAY_VOLUME_ENVELOPE as usize] = -12_000;
        v[G::RELEASE_VOLUME_ENVELOPE as usize] = -12_000;
        v[G::SCALE_TUNING as usize] = 100;
        v[G::OVERRIDING_ROOT_KEY as usize] = -1;
        v[G::KEY_NUMBER as usize] = -1;
        v[G::VELOCITY as usize] = -1;
        Self { v }
    }

    /// Reads a generator value.
    #[inline]
    pub fn get(&self, g: G) -> i32 {
        self.v[g as usize]
    }

    /// Reads a generator value as `f32` (for timecent / centibel maths).
    #[inline]
    pub fn getf(&self, g: G) -> f32 {
        self.v[g as usize] as f32
    }

    /// Absolute merge: each scalar generator in `zone` *replaces* the slot.
    pub fn apply_set(&mut self, zone: &Zone) {
        for g in &zone.generators {
            if let Some(value) = scalar(g.amount) {
                self.v[g.kind as usize] = value;
            }
        }
    }

    /// Relative merge: each scalar generator in `zone` is *added* to the slot.
    pub fn apply_add(&mut self, zone: &Zone) {
        for g in &zone.generators {
            if let Some(value) = scalar(g.amount) {
                self.v[g.kind as usize] += value;
            }
        }
    }
}

/// Extracts a generator's scalar value, or `None` for range generators.
fn scalar(amount: GeneratorAmount) -> Option<i32> {
    match amount {
        GeneratorAmount::Short(s) => Some(s as i32),
        GeneratorAmount::Word(w) => Some(w as i32),
        GeneratorAmount::Range(_) => None,
    }
}

/// Returns the range carried by generator `kind` in `zone`, if any.
pub fn range_of(zone: &Zone, kind: G) -> Option<Range> {
    zone.generators
        .iter()
        .find_map(|g| match (g.kind, g.amount) {
            (k, GeneratorAmount::Range(r)) if k == kind => Some(r),
            _ => None,
        })
}

/// Returns the unsigned index carried by generator `kind` (e.g. `Instrument`,
/// `SampleID`), if present.
pub fn index_of(zone: &Zone, kind: G) -> Option<u16> {
    zone.generators.iter().find_map(|g| {
        if g.kind != kind {
            return None;
        }
        match g.amount {
            GeneratorAmount::Word(w) => Some(w),
            GeneratorAmount::Short(s) => Some(s as u16),
            GeneratorAmount::Range(_) => None,
        }
    })
}

/// Whether `zone` admits `key`/`vel` given its `KeyRange`/`VelocityRange`.
///
/// A missing range generator means "matches everything" for that axis.
pub fn zone_matches(zone: &Zone, key: u8, vel: u8) -> bool {
    let key_ok = match range_of(zone, G::KEY_RANGE) {
        Some(r) => key >= r.low && key <= r.high,
        None => true,
    };
    let vel_ok = match range_of(zone, G::VELOCITY_RANGE) {
        Some(r) => vel >= r.low && vel <= r.high,
        None => true,
    };
    key_ok && vel_ok
}
