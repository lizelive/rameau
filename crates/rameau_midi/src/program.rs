/// A General MIDI program number.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct MidiProgram(u8);

impl MidiProgram {
    pub fn display_number(&self) -> u8 {
        self.0 + 1
    }

    /// The raw 0-based General MIDI program number (`0..=127`).
    #[inline]
    pub const fn index(&self) -> u8 {
        self.0
    }
}

impl From<MidiProgram> for u8 {
    #[inline]
    fn from(p: MidiProgram) -> u8 {
        p.0
    }
}
impl From<u8> for MidiProgram {
    #[inline]
    fn from(v: u8) -> Self {
        MidiProgram(v)
    }
}

#[macro_export]
macro_rules! midi_program {
    (
        $(
            $(#[$group_meta:meta])*
            $group:ident = [
                $(
                    $(#[$inst_meta:meta])*
                    $ident:ident ($name:expr, $num:expr)
                ),* $(,)?
            ];
        )*
    ) => {
        // --- Instrument constants ---
        $(
            $(
                $(#[$inst_meta])*
                pub const $ident: MidiProgram = MidiProgram($num - 1);
            )*
        )*

        // --- Group arrays ---
        $(
            $(#[$group_meta])*
            pub const $group: &[MidiProgram] = &[
                $(
                    $ident,
                )*
            ];
        )*

        // --- Name lookup ---
        impl MidiProgram {
            #[inline]
            pub fn get_name(&self) -> &'static str {
                match self {
                    $(
                        $(
                            &$ident => $name,
                        )*
                    )*
                    _ => "Unknown",
                }
            }
        }
    };
}

midi_program!(
    /// Piano family
    PIANO = [
        ACOUSTIC_GRAND_PIANO("Acoustic Grand Piano", 1),
        BRIGHT_ACOUSTIC_PIANO("Bright Acoustic Piano", 2),
        ELECTRIC_GRAND_PIANO("Electric Grand Piano", 3),
        HONKY_TONK_PIANO("Honky-tonk Piano", 4),
        ELECTRIC_PIANO_1("Electric Piano 1", 5),
        ELECTRIC_PIANO_2("Electric Piano 2", 6),
        HARPSICHORD("Harpsichord", 7),
        CLAVINET("Clavinet", 8),
    ];

    /// Chromatic percussion instruments
    CHROMATIC_PERCUSSION = [
        CELESTA("Celesta", 9),
        GLOCKENSPIEL("Glockenspiel", 10),
        MUSIC_BOX("Music Box", 11),
        VIBRAPHONE("Vibraphone", 12),
        MARIMBA("Marimba", 13),
        XYLOPHONE("Xylophone", 14),
        TUBULAR_BELLS("Tubular Bells", 15),
        DULCIMER("Dulcimer", 16),
    ];

    /// Organs
    ORGAN = [
        DRAWBAR_ORGAN("Drawbar Organ", 17),
        PERCUSSIVE_ORGAN("Percussive Organ", 18),
        ROCK_ORGAN("Rock Organ", 19),
        CHURCH_ORGAN("Church Organ", 20),
        REED_ORGAN("Reed Organ", 21),
        ACCORDION("Accordion", 22),
        HARMONICA("Harmonica", 23),
        BANDONEON("Bandoneon", 24),
    ];

    /// Guitars
    GUITAR = [
        ACOUSTIC_GUITAR_NYLON("Acoustic Guitar (Nylon)", 25),
        ACOUSTIC_GUITAR_STEEL("Acoustic Guitar (Steel)", 26),
        ELECTRIC_GUITAR_JAZZ("Electric Guitar (Jazz)", 27),
        ELECTRIC_GUITAR_CLEAN("Electric Guitar (Clean)", 28),
        ELECTRIC_GUITAR_MUTED("Electric Guitar (Muted)", 29),
        OVERDRIVEN_GUITAR("Overdriven Guitar", 30),
        DISTORTION_GUITAR("Distortion Guitar", 31),
        GUITAR_HARMONICS("Guitar Harmonics", 32),
    ];

    /// Bass instruments
    BASS = [
        ACOUSTIC_BASS("Acoustic Bass", 33),
        ELECTRIC_BASS_FINGER("Electric Bass (Finger)", 34),
        ELECTRIC_BASS_PICK("Electric Bass (Pick)", 35),
        FRETLESS_BASS("Fretless Bass", 36),
        SLAP_BASS_1("Slap Bass 1", 37),
        SLAP_BASS_2("Slap Bass 2", 38),
        SYNTH_BASS_1("Synth Bass 1", 39),
        SYNTH_BASS_2("Synth Bass 2", 40),
    ];

    /// Bowed strings
    STRINGS = [
        VIOLIN("Violin", 41),
        VIOLA("Viola", 42),
        CELLO("Cello", 43),
        CONTRABASS("Contrabass", 44),
        TREMOLO_STRINGS("Tremolo Strings", 45),
        PIZZICATO_STRINGS("Pizzicato Strings", 46),
        ORCHESTRAL_HARP("Orchestral Harp", 47),
        TIMPANI("Timpani", 48),
    ];

    /// Ensemble instruments
    ENSEMBLE = [
        STRING_ENSEMBLE_1("String Ensemble 1", 49),
        STRING_ENSEMBLE_2("String Ensemble 2", 50),
        SYNTH_STRINGS_1("Synth Strings 1", 51),
        SYNTH_STRINGS_2("Synth Strings 2", 52),
        CHOIR_AAHS("Choir Aahs", 53),
        VOICE_OOHS("Voice Oohs", 54),
        SYNTH_VOICE("Synth Voice", 55),
        ORCHESTRA_HIT("Orchestra Hit", 56),
    ];

    /// Brass instruments
    BRASS = [
        TRUMPET("Trumpet", 57),
        TROMBONE("Trombone", 58),
        TUBA("Tuba", 59),
        MUTED_TRUMPET("Muted Trumpet", 60),
        FRENCH_HORN("French Horn", 61),
        BRASS_SECTION("Brass Section", 62),
        SYNTH_BRASS_1("Synth Brass 1", 63),
        SYNTH_BRASS_2("Synth Brass 2", 64),
    ];

    /// Reed instruments
    REED = [
        SOPRANO_SAX("Soprano Sax", 65),
        ALTO_SAX("Alto Sax", 66),
        TENOR_SAX("Tenor Sax", 67),
        BARITONE_SAX("Baritone Sax", 68),
        OBOE("Oboe", 69),
        ENGLISH_HORN("English Horn", 70),
        BASSOON("Bassoon", 71),
        CLARINET("Clarinet", 72),
    ];

    /// Pipe, or [Aerophone](https://en.wikipedia.org/wiki/Aerophone)
    PIPE = [
        /// Highest orchestral woodwind
        PICCOLO("Piccolo", 73),
        FLUTE("Flute", 74),
        RECORDER("Recorder", 75),
        PAN_FLUTE("Pan Flute", 76),
        BLOWN_BOTTLE("Blown Bottle", 77),
        SHAKUHACHI("Shakuhachi", 78),
        WHISTLE("Whistle", 79),
        OCARINA("Ocarina", 80),
    ];

    /// Synth leads
    LEAD = [
        LEAD_1_SQUARE("Lead 1 (Square)", 81),
        LEAD_2_SAWTOOTH("Lead 2 (Sawtooth)", 82),
        LEAD_3_CALLIOPE("Lead 3 (Calliope)", 83),
        LEAD_4_CHIFF("Lead 4 (Chiff)", 84),
        LEAD_5_CHARANG("Lead 5 (Charang)", 85),
        LEAD_6_VOICE("Lead 6 (Voice)", 86),
        LEAD_7_FIFTHS("Lead 7 (Fifths)", 87),
        LEAD_8_BASS_LEAD("Lead 8 (Bass + Lead)", 88),
    ];

    /// Synth pads
    PAD = [
        PAD_1_NEW_AGE("Pad 1 (New Age)", 89),
        PAD_2_WARM("Pad 2 (Warm)", 90),
        PAD_3_POLYSYNTH("Pad 3 (Polysynth)", 91),
        PAD_4_CHOIR("Pad 4 (Choir)", 92),
        PAD_5_BOWED("Pad 5 (Bowed)", 93),
        PAD_6_METALLIC("Pad 6 (Metallic)", 94),
        PAD_7_HALO("Pad 7 (Halo)", 95),
        PAD_8_SWEEP("Pad 8 (Sweep)", 96),
    ];

    /// Synth effects
    FX = [
        FX_1_RAIN("FX 1 (Rain)", 97),
        FX_2_SOUNDTRACK("FX 2 (Soundtrack)", 98),
        FX_3_CRYSTAL("FX 3 (Crystal)", 99),
        FX_4_ATMOSPHERE("FX 4 (Atmosphere)", 100),
        FX_5_BRIGHTNESS("FX 5 (Brightness)", 101),
        FX_6_GOBLINS("FX 6 (Goblins)", 102),
        FX_7_ECHOES("FX 7 (Echoes)", 103),
        FX_8_SCI_FI("FX 8 (Sci-Fi)", 104),
    ];

    /// Ethnic/world instruments
    ETHNIC = [
        SITAR("Sitar", 105),
        BANJO("Banjo", 106),
        SHAMISEN("Shamisen", 107),
        KOTO("Koto", 108),
        KALIMBA("Kalimba", 109),
        BAG_PIPE("Bag Pipe", 110),
        FIDDLE("Fiddle", 111),
        SHANAI("Shanai", 112),
    ];

    /// Percussive pitched instruments
    PERCUSSIVE = [
        TINKLE_BELL("Tinkle Bell", 113),
        AGOGO("Agogo", 114),
        STEEL_DRUMS("Steel Drums", 115),
        WOODBLOCK("Woodblock", 116),
        TAIKO_DRUM("Taiko Drum", 117),
        MELODIC_TOM("Melodic Tom", 118),
        SYNTH_DRUM("Synth Drum", 119),
        REVERSE_CYMBAL("Reverse Cymbal", 120),
    ];

    /// Sound effects
    SOUND_EFFECTS = [
        GUITAR_FRET_NOISE("Guitar Fret Noise", 121),
        BREATH_NOISE("Breath Noise", 122),
        SEASHORE("Seashore", 123),
        BIRD_TWEET("Bird Tweet", 124),
        TELEPHONE_RING("Telephone Ring", 125),
        HELICOPTER("Helicopter", 126),
        APPLAUSE("Applause", 127),
        GUNSHOT("Gunshot", 128),
    ];
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_individual_constants() {
        assert_eq!(ACOUSTIC_GRAND_PIANO.0, 0);
        assert_eq!(FLUTE.0, 73);
        assert_eq!(GUNSHOT.0, 127);
    }

    #[test]
    fn test_group_arrays() {
        // Guitar group should contain exactly 8 instruments
        assert_eq!(GUITAR.len(), 8);
        assert!(GUITAR.contains(&ACOUSTIC_GUITAR_NYLON));
        assert!(GUITAR.contains(&DISTORTION_GUITAR));

        // Pipe group should contain 8 instruments
        assert_eq!(PIPE.len(), 8);
        assert!(PIPE.contains(&PICCOLO));
        assert!(PIPE.contains(&OCARINA));
    }

    #[test]
    fn test_get_name() {
        assert_eq!(ACOUSTIC_GRAND_PIANO.get_name(), "Acoustic Grand Piano");
        assert_eq!(FLUTE.get_name(), "Flute");
        assert_eq!(GUNSHOT.get_name(), "Gunshot");

        // Unknown values
        assert_eq!(MidiProgram(200).get_name(), "Unknown");
    }

    #[test]
    fn test_from_u8() {
        let p: MidiProgram = 24u8.into();
        assert_eq!(p, ACOUSTIC_GUITAR_NYLON);

        let p: MidiProgram = 72u8.into();
        assert_eq!(p, PICCOLO);
    }

    #[test]
    fn test_all_programs_are_unique() {
        // Collect all program numbers from the full GM1 set
        let mut seen = [false; 128]; // index 1..128 used

        let all_groups: &[&[MidiProgram]] = &[
            PIANO,
            CHROMATIC_PERCUSSION,
            ORGAN,
            GUITAR,
            BASS,
            STRINGS,
            ENSEMBLE,
            BRASS,
            REED,
            PIPE,
            LEAD,
            PAD,
            FX,
            ETHNIC,
            PERCUSSIVE,
            SOUND_EFFECTS,
        ];

        for group in all_groups {
            for inst in *group {
                let n = inst.0 as usize;
                assert!(n < 128, "Program out of range: {}", n);
                assert!(!seen[n], "Duplicate program number: {}", n);
                seen[n] = true;
            }
        }

        // Ensure all 128 programs are present
        assert!(seen[0..128].iter().all(|&x| x));
    }
}
