# rameau_compose

A real-time composition engine that writes music one bar ahead of playback
from a library of period *ideas* — motifs, ground basses, fugue subjects
and countersubjects stored as scale degrees — and bends everything it
writes to a set of musical sliders.

```
MusicState  ─┐                        ┌─ ComposedBar (notes, programs, why)
Triggers    ─┼─►  Composer::next_bar ─┤
IdeaLibrary ─┘         │              └─ Conductor / OfflineRenderer ─► Synthesizer
                       │
        form ─► harmonise ─► anneal ─► orchestrate
```

* **Ideas** (`idea`): JSON, degree-encoded (`{"d","a","o","t"}`), with a
  citation and a provenance tag. `IdeaLibrary::load_dir` or `from_bundle`.
* **Mutations** (`mutate`): transposition, chromatic shift, inversion,
  retrograde, augmentation, diminution, head, fragment, sequence, octave
  shift, ornamentation, simplification — all diatonic, all composable.
* **Sliders** (`state::MusicState`): tempo, density, voices, dissonance,
  licence, darkness, chromaticism, register, dynamics, articulation,
  ornament, polyphony, refinement, wealth, percussion, singing, phase. A
  game maps its own variables onto these. `RuleWeights::from_state` turns
  them into the annealer's weights: licence lowers the grammar rules,
  dissonance lowers the dissonance rules.
* **Harmonisation** (`harmonize`): a Viterbi pass over the period
  [`Grammar`](../rameau_chords) that fits chords to a fixed tune, with
  cadence targets; and the chords a ground bass implies.
* **Forms** (`form`): fugue (exposition, episodes on the subject's head in
  sequence, middle entries in related keys, stretto when the sliders are
  hot, final entry over a pedal; a bigger horde adds an entry), rondeau,
  chaconne (variations over a ground with an arch of density), air (sung
  when there are words) and contredanse.
* **Annealing** (`anneal`): a semiquaver grid per voice; fixed slots for
  idea material, free slots rewritten by Metropolis moves against a cost of
  counterpoint faults, chord fit, slider targets, range, monotony and
  similarity to recent bars. Hundreds of proposals per bar in well under a
  millisecond in release.
* **Instruments** (`instrument`): a catalogue of the instruments of France
  around 1789 mapped to General MIDI, each with roles, range and a place
  on the tavern-to-court axis; ensembles chosen from the sliders with
  hysteresis.
* **Conducting** (`conductor`): `Conductor` runs a beat clock over a
  `Synthesizer` — tempo and dynamics react at once, structure on the next
  bar; `OfflineRenderer` does the same on a simulated clock and returns
  timed MIDI events.

Singing goes through [`rameau_voix`](../rameau_voix): the composer selects
a vowel preset before each sung note.

## Minimal use

```rust,no_run
use rameau_compose::{Composer, IdeaLibrary, MusicState, Trigger, FormKind};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let lib = IdeaLibrary::load_dir("ideas")?;
let mut composer = Composer::new(lib, 1789);
composer.set_state(MusicState { tempo_bpm: 132.0, voices: 4, ..MusicState::default() });
composer.trigger(Trigger::Form(FormKind::Fugue));
let bar = composer.next_bar();
println!("{} · {} · {}", bar.form.name(), bar.section, bar.chords.join(" "));
# Ok(()) }
```

The library format and a curated set of ideas cut from real sources live
in the [peasanttide/music](https://github.com/peasanttide/music) repository.
