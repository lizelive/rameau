//! Integration tests that load the real SoundFonts shipped in `assets/`.

use std::path::PathBuf;

use rameau_soundfont::{SampleType, SoundFont};

fn asset(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .join(name)
}

/// Shared structural checks that should hold for any well-formed bank,
/// regardless of the container it was loaded from.
fn assert_well_formed(sf: &SoundFont) {
    assert!(sf.info.version.major >= 2, "expected SF2-family version");
    assert!(!sf.presets.is_empty(), "expected at least one preset");
    assert!(
        !sf.instruments.is_empty(),
        "expected at least one instrument"
    );
    assert!(!sf.samples.is_empty(), "expected at least one sample");

    for sample in &sf.samples {
        assert!(
            !sample.clip.data.is_empty(),
            "sample '{}' has no audio",
            sample.name
        );
        assert!(
            sample.clip.sample_rate > 0,
            "sample '{}' has no rate",
            sample.name
        );
        // Loop points must lie inside the decoded audio.
        assert!(
            sample.loop_end as usize <= sample.clip.data.len(),
            "sample '{}' loop_end {} exceeds len {}",
            sample.name,
            sample.loop_end,
            sample.clip.data.len()
        );
        assert!(
            sample.loop_start <= sample.loop_end,
            "sample '{}' has inverted loop points",
            sample.name
        );
    }

    // The abstract model should not leak the on-disk sample pool: at least one
    // mono sample is expected in a general-MIDI bank.
    assert!(
        sf.samples
            .iter()
            .any(|s| matches!(s.kind, SampleType::Mono)),
        "expected at least one mono sample"
    );
}

#[test]
fn loads_sf2() {
    let sf = SoundFont::load_file(asset("Unison.SF2")).expect("load Unison.SF2");
    assert_well_formed(&sf);
}

#[test]
fn loads_sf3() {
    let sf = SoundFont::load_file(asset("FluidR3Mono_GM.sf3")).expect("load FluidR3Mono_GM.sf3");
    assert_well_formed(&sf);
}

/// The two example banks load into the same shape of model despite using
/// different sample storage (raw PCM vs. Ogg/Vorbis).
#[test]
fn both_formats_yield_decoded_pcm() {
    let sf2 = SoundFont::load_file(asset("Unison.SF2")).expect("load sf2");
    let sf3 = SoundFont::load_file(asset("FluidR3Mono_GM.sf3")).expect("load sf3");

    // Both expose plain PCM samples; nothing distinguishes them at the type
    // level once loaded.
    let total_pcm = |sf: &SoundFont| sf.samples.iter().map(|s| s.clip.data.len()).sum::<usize>();
    assert!(total_pcm(&sf2) > 0);
    assert!(total_pcm(&sf3) > 0);
}
