//! The SoundFont generator enumeration.
//!
//! Generators are the synthesis parameters a zone sets. The names and
//! discriminants follow `SFGenerator` in the SoundFont 2.04 specification,
//! §8.1.2, and the one-line descriptions summarise the meanings given there.
//!
//! Units are the specification's own: absolute cents for pitch, centibels for
//! attenuation, and timecents for envelope durations.

/// The SoundFont generator enumeration (`SFGenerator`).
///
/// Discriminants match the on-disk operator values and are contiguous from
/// `0` up to (but not including) [`GeneratorType::COUNT`].
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[expect(
    non_camel_case_types,
    reason = "the variant names mirror the SoundFont specification's own \
              SFGenerator enumeration, which is easier to cross-reference \
              when the spelling matches"
)]
pub enum GeneratorType {
    /// Offset, in sample points, added to the sample's start.
    START_ADDRESS_OFFSET = 0,
    /// Offset, in sample points, added to the sample's end.
    END_ADDRESS_OFFSET = 1,
    /// Offset, in sample points, added to the loop's start.
    START_LOOP_ADDRESS_OFFSET = 2,
    /// Offset, in sample points, added to the loop's end.
    END_LOOP_ADDRESS_OFFSET = 3,
    /// Coarse (32768-sample-point) offset added to the sample's start.
    START_ADDRESS_COARSE_OFFSET = 4,
    /// Modulation LFO to pitch, in cents at full excursion.
    MODULATION_LFO_TO_PITCH = 5,
    /// Vibrato LFO to pitch, in cents at full excursion.
    VIBRATO_LFO_TO_PITCH = 6,
    /// Modulation envelope to pitch, in cents at full excursion.
    MODULATION_ENVELOPE_TO_PITCH = 7,
    /// Low-pass filter cutoff, in absolute cents.
    INITIAL_FILTER_CUTOFF_FREQUENCY = 8,
    /// Low-pass filter resonance at cutoff, in centibels.
    INITIAL_FILTER_Q = 9,
    /// Modulation LFO to filter cutoff, in cents at full excursion.
    MODULATION_LFO_TO_FILTER_CUTOFF_FREQUENCY = 10,
    /// Modulation envelope to filter cutoff, in cents at full excursion.
    MODULATION_ENVELOPE_TO_FILTER_CUTOFF_FREQUENCY = 11,
    /// Coarse (32768-sample-point) offset added to the sample's end.
    END_ADDRESS_COARSE_OFFSET = 12,
    /// Modulation LFO to volume, in centibels at full excursion.
    MODULATION_LFO_TO_VOLUME = 13,
    /// Unused; present only to keep the enumeration contiguous.
    UNUSED_1 = 14,
    /// Send level to the chorus effect, in 0.1% units.
    CHORUS_EFFECTS_SEND = 15,
    /// Send level to the reverb effect, in 0.1% units.
    REVERB_EFFECTS_SEND = 16,
    /// Stereo pan, in 0.1% units; negative is left.
    PAN = 17,
    /// Unused; present only to keep the enumeration contiguous.
    UNUSED_2 = 18,
    /// Unused; present only to keep the enumeration contiguous.
    UNUSED_3 = 19,
    /// Unused; present only to keep the enumeration contiguous.
    UNUSED_4 = 20,
    /// Delay before the modulation LFO starts, in timecents.
    DELAY_MODULATION_LFO = 21,
    /// Modulation LFO frequency, in absolute cents.
    FREQUENCY_MODULATION_LFO = 22,
    /// Delay before the vibrato LFO starts, in timecents.
    DELAY_VIBRATO_LFO = 23,
    /// Vibrato LFO frequency, in absolute cents.
    FREQUENCY_VIBRATO_LFO = 24,
    /// Modulation envelope delay phase, in timecents.
    DELAY_MODULATION_ENVELOPE = 25,
    /// Modulation envelope attack phase, in timecents.
    ATTACK_MODULATION_ENVELOPE = 26,
    /// Modulation envelope hold phase, in timecents.
    HOLD_MODULATION_ENVELOPE = 27,
    /// Modulation envelope decay phase, in timecents.
    DECAY_MODULATION_ENVELOPE = 28,
    /// Modulation envelope sustain level, in 0.1% units below peak.
    SUSTAIN_MODULATION_ENVELOPE = 29,
    /// Modulation envelope release phase, in timecents.
    RELEASE_MODULATION_ENVELOPE = 30,
    /// Key-number dependence of the modulation envelope's hold phase.
    KEY_NUMBER_TO_MODULATION_ENVELOPE_HOLD = 31,
    /// Key-number dependence of the modulation envelope's decay phase.
    KEY_NUMBER_TO_MODULATION_ENVELOPE_DECAY = 32,
    /// Volume envelope delay phase, in timecents.
    DELAY_VOLUME_ENVELOPE = 33,
    /// Volume envelope attack phase, in timecents.
    ATTACK_VOLUME_ENVELOPE = 34,
    /// Volume envelope hold phase, in timecents.
    HOLD_VOLUME_ENVELOPE = 35,
    /// Volume envelope decay phase, in timecents.
    DECAY_VOLUME_ENVELOPE = 36,
    /// Volume envelope sustain level, in centibels of attenuation below peak.
    SUSTAIN_VOLUME_ENVELOPE = 37,
    /// Volume envelope release phase, in timecents.
    RELEASE_VOLUME_ENVELOPE = 38,
    /// Key-number dependence of the volume envelope's hold phase.
    KEY_NUMBER_TO_VOLUME_ENVELOPE_HOLD = 39,
    /// Key-number dependence of the volume envelope's decay phase.
    KEY_NUMBER_TO_VOLUME_ENVELOPE_DECAY = 40,
    /// Index of the instrument this preset zone plays. Preset zones only.
    INSTRUMENT = 41,
    /// Reserved by the specification.
    RESERVED_1 = 42,
    /// Inclusive MIDI key range this zone responds to.
    KEY_RANGE = 43,
    /// Inclusive MIDI velocity range this zone responds to.
    VELOCITY_RANGE = 44,
    /// Coarse (32768-sample-point) offset added to the loop's start.
    START_LOOP_ADDRESS_COARSE_OFFSET = 45,
    /// Fixed MIDI key number to use instead of the played one.
    KEY_NUMBER = 46,
    /// Fixed MIDI velocity to use instead of the played one.
    VELOCITY = 47,
    /// Attenuation applied to the zone, in centibels.
    INITIAL_ATTENUATION = 48,
    /// Reserved by the specification.
    RESERVED_2 = 49,
    /// Coarse (32768-sample-point) offset added to the loop's end.
    END_LOOP_ADDRESS_COARSE_OFFSET = 50,
    /// Pitch offset in semitones.
    COARSE_TUNE = 51,
    /// Pitch offset in cents.
    FINE_TUNE = 52,
    /// Index of the sample this instrument zone plays. Instrument zones only.
    SAMPLE_ID = 53,
    /// Loop mode: no loop, continuous loop, or loop until release.
    SAMPLE_MODES = 54,
    /// Reserved by the specification.
    RESERVED_3 = 55,
    /// Cents of pitch change per MIDI key; 100 is equal temperament.
    SCALE_TUNING = 56,
    /// Exclusive class: a new note in this class stops others in it.
    EXCLUSIVE_CLASS = 57,
    /// MIDI key number the sample is treated as having been recorded at.
    OVERRIDING_ROOT_KEY = 58,
    /// Unused; present only to keep the enumeration contiguous.
    UNUSED_5 = 59,
    /// The terminal generator marking the end of a zone's list.
    UNUSED_END = 60,
    /// Not a generator: one past the highest valid operator value.
    COUNT = 61,
}

impl GeneratorType {
    /// Every generator in operator order, indexed by its discriminant.
    ///
    /// This is what makes [`from_u16`](Self::from_u16) a safe lookup rather
    /// than a transmute over the enum's representation.
    const ALL: [Self; Self::COUNT as usize] = [
        Self::START_ADDRESS_OFFSET,
        Self::END_ADDRESS_OFFSET,
        Self::START_LOOP_ADDRESS_OFFSET,
        Self::END_LOOP_ADDRESS_OFFSET,
        Self::START_ADDRESS_COARSE_OFFSET,
        Self::MODULATION_LFO_TO_PITCH,
        Self::VIBRATO_LFO_TO_PITCH,
        Self::MODULATION_ENVELOPE_TO_PITCH,
        Self::INITIAL_FILTER_CUTOFF_FREQUENCY,
        Self::INITIAL_FILTER_Q,
        Self::MODULATION_LFO_TO_FILTER_CUTOFF_FREQUENCY,
        Self::MODULATION_ENVELOPE_TO_FILTER_CUTOFF_FREQUENCY,
        Self::END_ADDRESS_COARSE_OFFSET,
        Self::MODULATION_LFO_TO_VOLUME,
        Self::UNUSED_1,
        Self::CHORUS_EFFECTS_SEND,
        Self::REVERB_EFFECTS_SEND,
        Self::PAN,
        Self::UNUSED_2,
        Self::UNUSED_3,
        Self::UNUSED_4,
        Self::DELAY_MODULATION_LFO,
        Self::FREQUENCY_MODULATION_LFO,
        Self::DELAY_VIBRATO_LFO,
        Self::FREQUENCY_VIBRATO_LFO,
        Self::DELAY_MODULATION_ENVELOPE,
        Self::ATTACK_MODULATION_ENVELOPE,
        Self::HOLD_MODULATION_ENVELOPE,
        Self::DECAY_MODULATION_ENVELOPE,
        Self::SUSTAIN_MODULATION_ENVELOPE,
        Self::RELEASE_MODULATION_ENVELOPE,
        Self::KEY_NUMBER_TO_MODULATION_ENVELOPE_HOLD,
        Self::KEY_NUMBER_TO_MODULATION_ENVELOPE_DECAY,
        Self::DELAY_VOLUME_ENVELOPE,
        Self::ATTACK_VOLUME_ENVELOPE,
        Self::HOLD_VOLUME_ENVELOPE,
        Self::DECAY_VOLUME_ENVELOPE,
        Self::SUSTAIN_VOLUME_ENVELOPE,
        Self::RELEASE_VOLUME_ENVELOPE,
        Self::KEY_NUMBER_TO_VOLUME_ENVELOPE_HOLD,
        Self::KEY_NUMBER_TO_VOLUME_ENVELOPE_DECAY,
        Self::INSTRUMENT,
        Self::RESERVED_1,
        Self::KEY_RANGE,
        Self::VELOCITY_RANGE,
        Self::START_LOOP_ADDRESS_COARSE_OFFSET,
        Self::KEY_NUMBER,
        Self::VELOCITY,
        Self::INITIAL_ATTENUATION,
        Self::RESERVED_2,
        Self::END_LOOP_ADDRESS_COARSE_OFFSET,
        Self::COARSE_TUNE,
        Self::FINE_TUNE,
        Self::SAMPLE_ID,
        Self::SAMPLE_MODES,
        Self::RESERVED_3,
        Self::SCALE_TUNING,
        Self::EXCLUSIVE_CLASS,
        Self::OVERRIDING_ROOT_KEY,
        Self::UNUSED_5,
        Self::UNUSED_END,
    ];

    /// Converts a raw operator value into a [`GeneratorType`], returning
    /// `None` for values that are not valid generators (i.e. `>= COUNT`).
    pub fn from_u16(value: u16) -> Option<Self> {
        Self::ALL.get(value as usize).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::GeneratorType;

    /// The lookup table replaced a transmute, so its entries must stay in
    /// discriminant order — an entry out of place would silently mis-decode
    /// every bank that uses that generator.
    #[test]
    fn lookup_table_is_in_discriminant_order() {
        for (i, kind) in GeneratorType::ALL.iter().enumerate() {
            assert_eq!(*kind as usize, i, "entry {i} is out of order");
        }
    }

    #[test]
    fn from_u16_round_trips_every_generator() {
        for i in 0..GeneratorType::COUNT as u16 {
            assert_eq!(GeneratorType::from_u16(i).map(|k| k as u16), Some(i));
        }
    }

    #[test]
    fn from_u16_rejects_out_of_range_operators() {
        assert_eq!(GeneratorType::from_u16(GeneratorType::COUNT as u16), None);
        assert_eq!(GeneratorType::from_u16(u16::MAX), None);
    }
}
