//! Pure domain. No Dioxus, no web-sys, no clock, no global randomness.
//! Every random choice takes `&mut impl Rng` and every time-dependent choice takes `Millis`.
//! Fields are private, so each invariant has exactly one writer.

pub mod avatar;
pub mod centers;
pub mod class;
pub mod doc;
pub mod grouping;
pub mod label;
pub mod names;
pub mod oracle;
pub mod rng;
pub mod seed;
pub mod spin;
pub mod timer;

pub use avatar::Avatar;
pub use centers::{CenterIdx, Centers};
pub use class::{
    CenterView, Class, ClassId, Draw, GroupView, ParsedRoster, Pick, Student, StudentId,
    parse_roster,
};
pub use doc::Doc;
pub use grouping::{GroupCount, GroupCountError, GroupIdx, Grouping, MoveError, group_sizes};
pub use label::{Label, LabelError, MAX_LABEL_CHARS};
pub use names::MAX_GROUPS;
pub use oracle::Oracle;
pub use rng::{Rng, SplitMix64};
pub use seed::seed_doc;
pub use spin::{Frame, Reel};
pub use timer::{Millis, Preset, Timer, format_mm_ss};

#[cfg(test)]
pub(crate) mod testing {
    use super::{Avatar, Label, Student, StudentId};

    pub fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    pub fn labels(raw: &[&str]) -> Vec<Label> {
        raw.iter().map(|name| label(name)).collect()
    }

    /// "Student 0", "Student 1", ...
    pub fn numbered(n: u32) -> Vec<Label> {
        (0..n).map(|i| label(&format!("Student {i}"))).collect()
    }

    /// A roster with ids `0..n`, the way `Class::new` would number it.
    pub fn roster(n: u32) -> Vec<Student> {
        (0..)
            .map(StudentId)
            .zip(numbered(n))
            .zip(Avatar::ALL.into_iter().cycle())
            .map(|((id, name), avatar)| Student { id, name, avatar })
            .collect()
    }
}
