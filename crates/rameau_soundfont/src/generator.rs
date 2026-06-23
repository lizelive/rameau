/// The SoundFont generator enumeration (`SFGenerator`).
///
/// Discriminants match the on-disk operator values and are contiguous from
/// `0` up to (but not including) [`GeneratorType::COUNT`].
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[expect(non_camel_case_types)]
pub enum GeneratorType {
    START_ADDRESS_OFFSET = 0,
    END_ADDRESS_OFFSET = 1,
    START_LOOP_ADDRESS_OFFSET = 2,
    END_LOOP_ADDRESS_OFFSET = 3,
    START_ADDRESS_COARSE_OFFSET = 4,
    MODULATION_LFO_TO_PITCH = 5,
    VIBRATO_LFO_TO_PITCH = 6,
    MODULATION_ENVELOPE_TO_PITCH = 7,
    INITIAL_FILTER_CUTOFF_FREQUENCY = 8,
    INITIAL_FILTER_Q = 9,
    MODULATION_LFO_TO_FILTER_CUTOFF_FREQUENCY = 10,
    MODULATION_ENVELOPE_TO_FILTER_CUTOFF_FREQUENCY = 11,
    END_ADDRESS_COARSE_OFFSET = 12,
    MODULATION_LFO_TO_VOLUME = 13,
    UNUSED_1 = 14,
    CHORUS_EFFECTS_SEND = 15,
    REVERB_EFFECTS_SEND = 16,
    PAN = 17,
    UNUSED_2 = 18,
    UNUSED_3 = 19,
    UNUSED_4 = 20,
    DELAY_MODULATION_LFO = 21,
    FREQUENCY_MODULATION_LFO = 22,
    DELAY_VIBRATO_LFO = 23,
    FREQUENCY_VIBRATO_LFO = 24,
    DELAY_MODULATION_ENVELOPE = 25,
    ATTACK_MODULATION_ENVELOPE = 26,
    HOLD_MODULATION_ENVELOPE = 27,
    DECAY_MODULATION_ENVELOPE = 28,
    SUSTAIN_MODULATION_ENVELOPE = 29,
    RELEASE_MODULATION_ENVELOPE = 30,
    KEY_NUMBER_TO_MODULATION_ENVELOPE_HOLD = 31,
    KEY_NUMBER_TO_MODULATION_ENVELOPE_DECAY = 32,
    DELAY_VOLUME_ENVELOPE = 33,
    ATTACK_VOLUME_ENVELOPE = 34,
    HOLD_VOLUME_ENVELOPE = 35,
    DECAY_VOLUME_ENVELOPE = 36,
    SUSTAIN_VOLUME_ENVELOPE = 37,
    RELEASE_VOLUME_ENVELOPE = 38,
    KEY_NUMBER_TO_VOLUME_ENVELOPE_HOLD = 39,
    KEY_NUMBER_TO_VOLUME_ENVELOPE_DECAY = 40,
    INSTRUMENT = 41,
    RESERVED_1 = 42,
    KEY_RANGE = 43,
    VELOCITY_RANGE = 44,
    START_LOOP_ADDRESS_COARSE_OFFSET = 45,
    KEY_NUMBER = 46,
    VELOCITY = 47,
    INITIAL_ATTENUATION = 48,
    RESERVED_2 = 49,
    END_LOOP_ADDRESS_COARSE_OFFSET = 50,
    COARSE_TUNE = 51,
    FINE_TUNE = 52,
    SAMPLE_ID = 53,
    SAMPLE_MODES = 54,
    RESERVED_3 = 55,
    SCALE_TUNING = 56,
    EXCLUSIVE_CLASS = 57,
    OVERRIDING_ROOT_KEY = 58,
    UNUSED_5 = 59,
    UNUSED_END = 60,
    COUNT = 61,
}

impl GeneratorType {
    /// Converts a raw operator value into a [`GeneratorType`], returning
    /// `None` for values that are not valid generators (i.e. `>= COUNT`).
    pub fn from_u16(value: u16) -> Option<Self> {
        if value < Self::COUNT as u16 {
            // SAFETY: the discriminants are `#[repr(u8)]` and contiguous over
            // `0..COUNT`, so every such value is a valid variant.
            Some(unsafe { core::mem::transmute::<u8, GeneratorType>(value as u8) })
        } else {
            None
        }
    }
}
