//! Pure domain. No Dioxus, no web-sys, no clock, no global randomness.
//! Every random choice takes `&mut impl Rng` and every time-dependent choice takes `Millis`.
//! Fields are private, so each invariant has exactly one writer.

pub mod label;
pub mod names;
pub mod rng;
pub mod timer;

pub use label::{Label, LabelError, MAX_LABEL_CHARS};
pub use names::MAX_GROUPS;
pub use rng::{Rng, SplitMix64};
pub use timer::{Millis, Preset, Timer, format_mm_ss};
