//! End-to-end conversion of the real `.sf2` bank shipped in `assets/`.

use std::path::PathBuf;

use rameau_soundfont::SoundFont;
use rameau_soundfont_convert::{Quality, save_sf3};

fn asset(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .join(name)
}

/// Root-mean-square level of a signal, normalised to full scale.
fn rms(v: &[i16]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let sum: f64 = v
        .iter()
        .map(|&x| {
            let d = x as f64 / 32768.0;
            d * d
        })
        .sum();
    (sum / v.len() as f64).sqrt()
}

/// Root-mean-square difference between two signals, normalised to full scale.
fn rms_error(a: &[i16], b: &[i16]) -> f64 {
    if a.is_empty() {
        return 0.0;
    }
    let sum: f64 = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| {
            let d = (x as f64 - y as f64) / 32768.0;
            d * d
        })
        .sum();
    (sum / a.len() as f64).sqrt()
}

/// Converts the real bank and reloads it, asserting that everything the model
/// carries survives the trip and that only the audio changed — lossily, but
/// within tolerance, and without any change in length.
#[test]
fn converts_unison_sf2_to_sf3() {
    let original = SoundFont::load_file(asset("Unison.SF2")).expect("load Unison.SF2");

    let out = std::env::temp_dir().join("rameau_unison_convert_test.sf3");
    save_sf3(&original, Quality::default(), &out).expect("write sf3");

    let converted = SoundFont::load_file(&out).expect("reload the converted sf3");

    assert_eq!(converted.info.version.major, 3, "expected an sf3 ifil");
    assert_eq!(converted.presets.len(), original.presets.len());
    assert_eq!(converted.instruments.len(), original.instruments.len());
    assert_eq!(converted.samples.len(), original.samples.len());

    for (a, b) in original.presets.iter().zip(&converted.presets) {
        assert_eq!(b.name, a.name);
        assert_eq!(b.bank, a.bank);
        assert_eq!(b.program, a.program);
        assert_eq!(b.zones.len(), a.zones.len(), "preset '{}'", a.name);
        // Zone contents carry the synthesis parameters; a mismatch here would
        // mean the bag index chains were rebuilt wrongly.
        for (x, y) in a.zones.iter().zip(&b.zones) {
            assert_eq!(y.generators, x.generators, "preset '{}'", a.name);
            assert_eq!(y.modulators, x.modulators, "preset '{}'", a.name);
        }
    }

    for (a, b) in original.instruments.iter().zip(&converted.instruments) {
        assert_eq!(b.name, a.name);
        assert_eq!(b.zones.len(), a.zones.len(), "instrument '{}'", a.name);
        for (x, y) in a.zones.iter().zip(&b.zones) {
            assert_eq!(y.generators, x.generators, "instrument '{}'", a.name);
            assert_eq!(y.modulators, x.modulators, "instrument '{}'", a.name);
        }
    }

    let mut worst_rms = 0.0f64;
    let mut worst_name = String::new();
    let mut worst_level = 0.0f64;
    let mut worst_level_name = String::new();
    for (a, b) in original.samples.iter().zip(&converted.samples) {
        assert_eq!(b.name, a.name);
        assert_eq!(b.sample_rate, a.sample_rate, "sample '{}'", a.name);
        assert_eq!(b.original_key, a.original_key, "sample '{}'", a.name);
        assert_eq!(b.correction, a.correction, "sample '{}'", a.name);
        assert_eq!(b.link, a.link, "sample '{}'", a.name);
        assert_eq!(b.kind, a.kind, "sample '{}'", a.name);
        assert_eq!(b.loop_start, a.loop_start, "sample '{}'", a.name);
        assert_eq!(b.loop_end, a.loop_end, "sample '{}'", a.name);

        // Loop points are frame offsets, so a length change would move them.
        assert_eq!(
            b.clip.data.len(),
            a.clip.data.len(),
            "sample '{}' changed length",
            a.name
        );

        let error = rms_error(&a.clip.data, &b.clip.data);
        if error > worst_rms {
            worst_rms = error;
            worst_name = a.name.clone();
        }

        // Level is phase-insensitive, so unlike the waveform difference it stays
        // small even for content the codec reproduces by character rather than
        // sample-for-sample. A swapped or mis-sliced sample would show a wildly
        // different level.
        let (level_a, level_b) = (rms(&a.clip.data), rms(&b.clip.data));
        if level_a > 1e-4 {
            let deviation = (level_b / level_a - 1.0).abs();
            if deviation > worst_level {
                worst_level = deviation;
                worst_level_name = a.name.clone();
            }
        }
    }

    // Vorbis is lossy and does not preserve phase, so exact PCM equality is the
    // wrong assertion — and so is a tight bound on waveform difference. Across
    // this bank's 655 samples only one exceeds a 0.1 waveform difference
    // ('Telephone1', 0.20): a 1270-frame burst of dual-tone noise, the hardest
    // possible case for a perceptual codec. The 808 drum samples and assorted
    // effects follow at 0.04-0.09. These bounds sit above the measured worst
    // case while still catching gross corruption.
    //
    // The real structural proof is elsewhere: every sample's frame count,
    // metadata and loop points match exactly, which mis-sliced streams could
    // not produce.
    assert!(
        worst_rms < 0.3,
        "sample '{worst_name}' differs by rms {worst_rms:.4}; \
         expected lossy but recognisable audio"
    );
    assert!(
        worst_level < 0.5,
        "sample '{worst_level_name}' changed level by {:.0}%; \
         a sample may be mis-sliced or swapped",
        worst_level * 100.0
    );

    let before = std::fs::metadata(asset("Unison.SF2")).unwrap().len();
    let after = std::fs::metadata(&out).unwrap().len();
    assert!(
        after < before / 2,
        "expected substantial compression, got {before} -> {after} bytes"
    );

    println!(
        "converted {} presets / {} instruments / {} samples; \
         {:.1} MiB -> {:.1} MiB ({:.0}% smaller)\n\
         worst waveform difference {worst_rms:.4} ('{worst_name}'); \
         worst level change {:.1}% ('{worst_level_name}')",
        converted.presets.len(),
        converted.instruments.len(),
        converted.samples.len(),
        before as f64 / (1024.0 * 1024.0),
        after as f64 / (1024.0 * 1024.0),
        100.0 - (after as f64 / before as f64 * 100.0),
        worst_level * 100.0,
    );

    let _ = std::fs::remove_file(&out);
}
