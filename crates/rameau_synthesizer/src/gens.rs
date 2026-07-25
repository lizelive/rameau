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

/// The generators whose SoundFont default is not zero (spec table, "Default
/// Value"). Everything not listed here starts at `0`.
const NON_ZERO_DEFAULTS: &[(G, i32)] = &[
    (G::INITIAL_FILTER_CUTOFF_FREQUENCY, 13_500),
    (G::DELAY_MODULATION_LFO, -12_000),
    (G::DELAY_VIBRATO_LFO, -12_000),
    (G::DELAY_MODULATION_ENVELOPE, -12_000),
    (G::ATTACK_MODULATION_ENVELOPE, -12_000),
    (G::HOLD_MODULATION_ENVELOPE, -12_000),
    (G::DECAY_MODULATION_ENVELOPE, -12_000),
    (G::RELEASE_MODULATION_ENVELOPE, -12_000),
    (G::DELAY_VOLUME_ENVELOPE, -12_000),
    (G::ATTACK_VOLUME_ENVELOPE, -12_000),
    (G::HOLD_VOLUME_ENVELOPE, -12_000),
    (G::DECAY_VOLUME_ENVELOPE, -12_000),
    (G::RELEASE_VOLUME_ENVELOPE, -12_000),
    (G::SCALE_TUNING, 100),
    (G::OVERRIDING_ROOT_KEY, -1),
    (G::KEY_NUMBER, -1),
    (G::VELOCITY, -1),
];

impl Gens {
    /// The SoundFont default generator values (spec table, "Default Value").
    pub fn defaults() -> Self {
        let mut gens = Self { v: [0i32; N] };
        for &(g, value) in NON_ZERO_DEFAULTS {
            if let Some(slot) = gens.slot_mut(g) {
                *slot = value;
            }
        }
        gens
    }

    /// A mutable handle on one generator's slot.
    ///
    /// Every [`GeneratorType`] discriminant is below `COUNT`, which is the
    /// array's length, so this always resolves; going through `get_mut` keeps
    /// that guarantee checked rather than assumed.
    #[inline]
    fn slot_mut(&mut self, g: G) -> Option<&mut i32> {
        self.v.get_mut(g as usize)
    }

    /// Reads a generator value.
    #[inline]
    pub fn get(&self, g: G) -> i32 {
        self.v.get(g as usize).copied().unwrap_or(0)
    }

    /// Reads a generator value as `f32` (for timecent / centibel maths).
    #[inline]
    pub fn getf(&self, g: G) -> f32 {
        self.get(g) as f32
    }

    /// Absolute merge: each scalar generator in `zone` *replaces* the slot.
    pub fn apply_set(&mut self, zone: &Zone) {
        for g in &zone.generators {
            if let Some(value) = scalar(g.amount)
                && let Some(slot) = self.slot_mut(g.kind)
            {
                *slot = value;
            }
        }
    }

    /// Relative merge: each scalar generator in `zone` is *added* to the slot.
    pub fn apply_add(&mut self, zone: &Zone) {
        for g in &zone.generators {
            if let Some(value) = scalar(g.amount)
                && let Some(slot) = self.slot_mut(g.kind)
            {
                *slot += value;
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
