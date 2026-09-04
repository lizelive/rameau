//! The vowels of French and their formant frequencies.

/// A French vowel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vowel {
    /// /a/ as in *patte*, *ça*.
    A,
    /// /e/ as in *été*.
    E,
    /// /ɛ/ as in *père*, *ai*.
    Eh,
    /// /i/ as in *ira*.
    I,
    /// /o/ as in *eau*, *canon*'s first syllable.
    O,
    /// /ɔ/ as in *note*.
    Oh,
    /// /u/ as in *nous*.
    U,
    /// /y/ as in *tu*.
    Y,
    /// /ø/ as in *feu*.
    Eu,
    /// /ə/ the mute e, as in *le*.
    Schwa,
    /// /ɑ̃/ as in *dansons*, *chant*.
    An,
    /// /ɔ̃/ as in *canon*, *son*.
    On,
    /// /ɛ̃/ as in *pain*, *citoyens*.
    In,
}

/// One formant: centre frequency in Hz, bandwidth in Hz, relative gain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Formant {
    /// Centre frequency in Hz (for an adult male voice).
    pub hz: f32,
    /// Bandwidth in Hz.
    pub bw: f32,
}

impl Vowel {
    /// Every vowel, in preset order.
    pub const ALL: [Vowel; 13] = [
        Vowel::A,
        Vowel::E,
        Vowel::Eh,
        Vowel::I,
        Vowel::O,
        Vowel::Oh,
        Vowel::U,
        Vowel::Y,
        Vowel::Eu,
        Vowel::Schwa,
        Vowel::An,
        Vowel::On,
        Vowel::In,
    ];

    /// The preset (program) number of this vowel in the voice banks.
    pub const fn program(self) -> u8 {
        match self {
            Vowel::A => 0,
            Vowel::E => 1,
            Vowel::Eh => 2,
            Vowel::I => 3,
            Vowel::O => 4,
            Vowel::Oh => 5,
            Vowel::U => 6,
            Vowel::Y => 7,
            Vowel::Eu => 8,
            Vowel::Schwa => 9,
            Vowel::An => 10,
            Vowel::On => 11,
            Vowel::In => 12,
        }
    }

    /// The vowel with a given preset number.
    pub fn from_program(program: u8) -> Option<Self> {
        Self::ALL.get(program as usize).copied()
    }

    /// IPA symbol.
    pub const fn ipa(self) -> &'static str {
        match self {
            Vowel::A => "a",
            Vowel::E => "e",
            Vowel::Eh => "ɛ",
            Vowel::I => "i",
            Vowel::O => "o",
            Vowel::Oh => "ɔ",
            Vowel::U => "u",
            Vowel::Y => "y",
            Vowel::Eu => "ø",
            Vowel::Schwa => "ə",
            Vowel::An => "ɑ̃",
            Vowel::On => "ɔ̃",
            Vowel::In => "ɛ̃",
        }
    }

    /// Whether the vowel is nasal.
    pub const fn is_nasal(self) -> bool {
        matches!(self, Vowel::An | Vowel::On | Vowel::In)
    }

    /// The first four formants for an adult male voice.
    ///
    /// Values are the usual textbook averages (Peterson–Barney style,
    /// adjusted for French), rounded; they are a starting point the
    /// synthesizer scales per register.
    pub const fn formants(self) -> [Formant; 4] {
        const fn f(hz: f32, bw: f32) -> Formant {
            Formant { hz, bw }
        }
        match self {
            Vowel::A => [f(700.0, 80.0), f(1220.0, 90.0), f(2600.0, 120.0), f(3300.0, 150.0)],
            Vowel::E => [f(390.0, 60.0), f(2300.0, 100.0), f(3000.0, 130.0), f(3600.0, 160.0)],
            Vowel::Eh => [f(530.0, 70.0), f(1840.0, 100.0), f(2480.0, 120.0), f(3400.0, 160.0)],
            Vowel::I => [f(270.0, 50.0), f(2290.0, 100.0), f(3010.0, 140.0), f(3700.0, 170.0)],
            Vowel::O => [f(450.0, 60.0), f(800.0, 80.0), f(2830.0, 130.0), f(3400.0, 160.0)],
            Vowel::Oh => [f(570.0, 70.0), f(840.0, 80.0), f(2410.0, 120.0), f(3300.0, 150.0)],
            Vowel::U => [f(300.0, 50.0), f(870.0, 80.0), f(2240.0, 120.0), f(3200.0, 150.0)],
            Vowel::Y => [f(250.0, 50.0), f(1750.0, 90.0), f(2200.0, 120.0), f(3300.0, 150.0)],
            Vowel::Eu => [f(370.0, 60.0), f(1600.0, 90.0), f(2200.0, 120.0), f(3300.0, 150.0)],
            Vowel::Schwa => [f(500.0, 70.0), f(1500.0, 90.0), f(2500.0, 120.0), f(3300.0, 150.0)],
            Vowel::An => [f(650.0, 110.0), f(1200.0, 120.0), f(2500.0, 140.0), f(3300.0, 160.0)],
            Vowel::On => [f(450.0, 100.0), f(850.0, 110.0), f(2400.0, 140.0), f(3300.0, 160.0)],
            Vowel::In => [f(500.0, 100.0), f(1800.0, 120.0), f(2400.0, 140.0), f(3300.0, 160.0)],
        }
    }
}
