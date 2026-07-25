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

/// Ogg/Vorbis codes audio in blocks, so decoding the final packet yields more
/// frames than the stream actually contains. The true length is carried in the
/// last page's granule position, and the decoded data must be truncated to it;
/// simply concatenating packets leaves up to a block of spurious audio past
/// where the sample should stop.
///
/// The expected lengths below are the granule positions of those samples in
/// `FluidR3Mono_GM.sf3`, cross-checked against libvorbisfile — which agrees
/// with the granule position on all 1037 compressed samples in that bank. Each
/// of these samples is one that naive packet concatenation gets wrong, so this
/// test fails if the truncation is ever dropped.
#[test]
fn sf3_samples_decode_to_their_true_length() {
    // (sample name, true frame count, length naive concatenation would give)
    const EXPECTED: &[(&str, usize, usize)] = &[
        ("whistle", 16_449, 17_088),
        ("harmnc_e5(L)", 16_760, 16_768),
        ("harmon_c5(L)", 16_903, 17_024),
        ("harmnc_g4(L)", 15_752, 15_872),
        ("harmon_c2(L)", 16_908, 17_024),
    ];

    let sf = SoundFont::load_file(asset("FluidR3Mono_GM.sf3")).expect("load sf3");

    for &(name, expected, over_read) in EXPECTED {
        let sample = sf
            .samples
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("no sample named '{name}' in the bank"));

        assert_eq!(
            sample.clip.data.len(),
            expected,
            "sample '{name}' decoded to {} frames, expected {expected} \
             (naive packet concatenation would give {over_read}); \
             decoded audio is not truncated to the stream's granule position",
            sample.clip.data.len(),
        );
    }
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
