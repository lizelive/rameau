//! A living, reactive composition engine built on period material.
//!
//! The engine composes **one bar at a time**, just ahead of playback, from a
//! library of [`Idea`]s — motifs, ground basses, fugue subjects and
//! countersubjects transcribed from real music of the long eighteenth
//! century and stored as scale degrees. Each bar is:
//!
//! 1. **planned** by a [`form`] (fugue, rondeau, chaconne, air, contredanse):
//!    which idea, in which voice, under which mutation, in which key;
//! 2. **harmonised** by a dynamic programme over the period
//!    [`Grammar`](rameau_chords::Grammar) so the chords fit the fixed melody;
//! 3. **filled in** by simulated annealing ([`anneal`]) that writes the free
//!    voices against a cost made of counterpoint rules, harmonic fit, the
//!    musical sliders' targets and a repetition penalty;
//! 4. **orchestrated** with instruments of the period ([`instrument`]) that
//!    follow the sliders, and sung by the formant voice of `rameau_voix`
//!    when there are lyrics.
//!
//! The sliders are a [`MusicState`]; a game maps its own variables onto
//! them. Triggers ([`Trigger`]) land on the next bar: a named idea, a form,
//! a cadence, a line of text to sing, a bell.

#![forbid(unsafe_code)]

pub mod anneal;
pub mod composer;
pub mod conductor;
pub mod form;
pub mod harmonize;
pub mod idea;
pub mod instrument;
pub mod mutate;
pub mod state;

pub use composer::{ComposedBar, Composer, ScoredNote, Trigger};
pub use conductor::{Conductor, OfflineRenderer, TimedEvent};
pub use idea::{Citation, Idea, IdeaEvent, IdeaKind, IdeaLibrary, Provenance};
pub use instrument::{Ensemble, Instrument, Role};
pub use mutate::{Mutation, Variant};
pub use state::{FormKind, MusicState, Phase};
