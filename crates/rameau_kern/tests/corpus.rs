//! Parses real files from the peasanttide music corpus when it is checked out
//! next to this workspace (or at `PEASANTIDE_MUSIC_DIR`).

use std::path::PathBuf;

fn corpus_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("PEASANTIDE_MUSIC_DIR") {
        return Some(PathBuf::from(d));
    }
    let sibling = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../music");
    sibling.join("humdrum").is_dir().then_some(sibling)
}

fn load(rel: &str) -> Option<rameau_kern::KernScore> {
    let path = corpus_dir()?.join("humdrum").join(rel);
    let text = std::fs::read_to_string(path).ok()?;
    rameau_kern::parse(&text).ok()
}

#[test]
fn ca_ira_is_a_single_line_in_g() {
    let Some(score) = load("83/ee/becourt-fl-1790-violinis-ah-ca-ira.krn") else {
        eprintln!("corpus not available; skipping");
        return;
    };
    assert_eq!(score.key.map(|k| k.tonic), Some(rameau_theory::PitchClass::G));
    assert_eq!(score.meter.map(|m| (m.beats, m.unit)), Some((2, 4)));
    let melody = score.voices[0].melody();
    // "Ah! ça ira": G G A G G A G ...
    let head: Vec<i32> = melody.iter().filter_map(|n| n.midi).take(6).collect();
    assert_eq!(head, vec![67, 67, 69, 67, 67, 69]);
    assert!(score.bar_count() >= 16);
    // Every bar of 2/4 in the first strain sums to two crotchets.
    for bar in 1..=8 {
        let total: f64 = score.voices[0].bars(bar, bar).iter().map(|n| n.duration).sum();
        assert!((total - 2.0).abs() < 1e-9, "bar {bar} sums to {total}");
    }
}

#[test]
fn marcello_ciaccona_has_a_bass_and_a_treble() {
    let Some(score) = load("14/4f/benedetto-marcello-ciaccona.krn") else {
        return;
    };
    assert_eq!(score.voices.len(), 2);
    assert_eq!(score.tempo, Some(100.0));
    let bass = score.bottom_voice().unwrap();
    let treble = score.top_voice().unwrap();
    assert!(bass.range().unwrap().0 < treble.range().unwrap().0);
    assert_eq!(score.meter.map(|m| m.beats), Some(3));
}

#[test]
fn split_spines_in_bach_do_not_lose_time() {
    let Some(dir) = corpus_dir() else { return };
    let mut checked = 0;
    for entry in walk(&dir.join("humdrum")).into_iter().take(400) {
        let Ok(text) = std::fs::read_to_string(&entry) else { continue };
        if !text.contains("*^") {
            continue;
        }
        let Ok(score) = rameau_kern::parse(&text) else { continue };
        for v in &score.voices {
            for w in v.notes.windows(2) {
                assert!(w[1].onset + 1e-9 >= w[0].onset, "{}: onsets go backwards", entry.display());
            }
        }
        checked += 1;
    }
    eprintln!("checked {checked} files with splits");
}

fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else if p.extension().is_some_and(|x| x == "krn") {
            out.push(p);
        }
    }
    out.sort();
    out
}
