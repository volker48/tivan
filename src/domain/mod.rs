//! Pure domain. No Dioxus, no web-sys, no clock, no global randomness.
//! Every random choice takes `&mut impl Rng` and every time-dependent choice takes `Millis`.
//! Fields are private, so each invariant has exactly one writer.

pub mod avatar;
pub mod centers;
pub mod class;
pub mod classroom;
pub mod doc;
pub mod grouping;
pub mod label;
pub mod names;
pub mod oracle;
pub mod rng;
pub mod seed;
pub mod skin;
pub mod spin;
pub mod timer;

pub use avatar::Avatar;
pub use centers::{CenterIdx, Centers};
pub use class::{
    CenterView, Class, ClassId, Draw, GroupView, ParsedRoster, Pick, Student, StudentId,
    parse_roster,
};
pub use classroom::{
    ClassroomCourse, ClassroomCourseId, ClassroomStudent, ClassroomUserId, CourseStatus, PlanEntry,
    SyncPlan, SyncReport, parse_courses,
};
pub use doc::Doc;
pub use grouping::{GroupCount, GroupCountError, GroupIdx, Grouping, MoveError, group_sizes};
pub use label::{Label, LabelError, MAX_LABEL_CHARS};
pub use names::MAX_GROUPS;
pub use oracle::Oracle;
pub use rng::{Rng, SplitMix64};
pub use seed::seed_doc;
pub use skin::{CalendarDay, Costume, Season, Settings, Skin, SkinChoice};
pub use spin::{Frame, Reel};
pub use timer::{Millis, Preset, Timer, format_mm_ss};

#[cfg(test)]
pub(crate) mod testing {
    use super::{
        Avatar, ClassroomCourse, ClassroomCourseId, ClassroomStudent, ClassroomUserId, Label,
        Student, StudentId,
    };

    pub fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    pub fn course_id(raw: &str) -> ClassroomCourseId {
        ClassroomCourseId::parse(raw).unwrap()
    }

    pub fn user_id(raw: &str) -> ClassroomUserId {
        ClassroomUserId::parse(raw).unwrap()
    }

    /// A Classroom course whose id is `id`, with `(id, name)` students.
    pub fn course(id: &str, name: &str, students: &[(&str, &str)]) -> ClassroomCourse {
        let students = students
            .iter()
            .map(|(id, name)| ClassroomStudent::new(user_id(id), name))
            .collect();
        ClassroomCourse::new(course_id(id), name, students)
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
            .map(|((id, name), avatar)| Student {
                id,
                name,
                avatar,
                classroom_user_id: None,
            })
            .collect()
    }
}
