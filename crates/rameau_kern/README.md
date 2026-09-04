# rameau_kern

A small, forgiving parser for Humdrum `**kern` files.

It reads the spines of a `**kern` file — following `*^` splits and `*v`
merges — and produces one [`Voice`] per spine with every note and rest as a
timed event: onset in crotchets from the start, duration, MIDI key, bar
number and position in the bar, tie state and ornament signs. Chords come
out as several notes with the same onset.

It also picks up the header the composer cares about: key (`*G:`), key
signature (`*k[f#]`), meter (`*M6/8`), tempo (`*MM100`), title and composer
reference records.

```rust,no_run
let text = std::fs::read_to_string("ca-ira.krn")?;
let score = rameau_kern::parse(&text)?;
for voice in &score.voices {
    for note in voice.melody() {
        println!("bar {} beat {} midi {:?} for {} crotchets",
            note.measure, note.beat, note.midi, note.duration);
    }
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

Things it deliberately does not do: layout, lyrics, dynamics spines,
`**recip` or `**mens`. Unknown tokens are skipped rather than rejected, so
a slightly odd file still yields its notes.
