//! End-to-end test: parse the real `assets/heist.midi` file.

use rameau_midi::event::MidiEvent;
use rameau_midi::smf::{Division, Format, MetaEvent, Smf, TrackEventKind};

/// Load the workspace's `assets/heist.midi`.
fn heist_bytes() -> Vec<u8> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/heist.midi");
    std::fs::read(path).expect("read assets/heist.midi")
}

#[test]
fn parses_heist_header_and_tracks() {
    let smf = Smf::parse(&heist_bytes()).expect("parse heist.midi");

    // Header: format 1, 3 tracks, 480 ticks per quarter note.
    assert_eq!(smf.format, Format::Parallel);
    assert_eq!(smf.division, Division::TicksPerQuarter(480));
    assert_eq!(smf.tracks.len(), 3);

    // Every track must terminate with an explicit end-of-track meta event.
    for track in &smf.tracks {
        assert!(
            matches!(
                track.events.last().map(|e| &e.kind),
                Some(TrackEventKind::Meta(MetaEvent::EndOfTrack))
            ),
            "track does not end with EndOfTrack",
        );
    }
}

#[test]
fn heist_conductor_track_has_tempo_and_title() {
    let smf = Smf::parse(&heist_bytes()).expect("parse heist.midi");
    let conductor = &smf.tracks[0];

    let metas: Vec<&MetaEvent> = conductor
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            TrackEventKind::Meta(m) => Some(m),
            _ => None,
        })
        .collect();

    // 375000 us/quarter == 160 BPM.
    assert!(
        metas.contains(&&MetaEvent::Tempo(375_000)),
        "expected tempo of 375000 us/quarter, got {metas:?}",
    );

    // 4/4 time.
    assert!(
        metas.iter().any(|m| matches!(
            m,
            MetaEvent::TimeSignature {
                numerator: 4,
                denominator: 4,
                ..
            }
        )),
        "expected 4/4 time signature, got {metas:?}",
    );

    // The embedded title.
    assert!(
        metas.iter().any(|m| matches!(
            m,
            MetaEvent::Text(t) if t == "The Vault (Heist Theme)"
        )),
        "expected title text event, got {metas:?}",
    );
}

#[test]
fn heist_notes_are_balanced() {
    let smf = Smf::parse(&heist_bytes()).expect("parse heist.midi");

    let mut note_on = 0usize;
    let mut note_off = 0usize;
    let mut last_tick = 0u64;

    for track in &smf.tracks {
        for (tick, event) in track.midi_events() {
            // Per track, absolute ticks are non-decreasing.
            match event {
                MidiEvent::NoteOn { vel, .. } if vel > 0 => note_on += 1,
                MidiEvent::NoteOn { .. } | MidiEvent::NoteOff { .. } => note_off += 1,
                _ => {}
            }
            last_tick = last_tick.max(tick);
        }
    }

    assert!(note_on > 0, "expected at least one note-on");
    assert_eq!(
        note_on, note_off,
        "every note-on must be matched by a note-off (or zero-velocity note-on)",
    );
    assert!(last_tick > 0, "expected a non-zero total duration in ticks");
}
