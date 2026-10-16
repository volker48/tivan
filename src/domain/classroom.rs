//! Google Classroom sync, the pure half. The Worker hands the client the teacher's courses with
//! their rosters, [`parse_courses`] turns that into [`ClassroomCourse`]s, and a [`SyncPlan`] says
//! what a sync would do before it does it.
//!
//! A class links to the course it was made from and a student to the Classroom user they came
//! from, so a second sync finds them again instead of adding them twice. Syncing makes each
//! linked class's roster follow Classroom and touches nothing else: students added by hand, a
//! student's name once linked, groups, centers and oracle rounds all survive. A course with no
//! class yet is only created when the teacher chose it, so a class they deleted does not quietly
//! come back.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::doc::Doc;
use super::label::{Label, MAX_LABEL_CHARS};
use super::rng::Rng;

/// What a classroom name becomes when Classroom sent none.
const STUDENT_FALLBACK: &str = "Student";
const COURSE_FALLBACK: &str = "Class";

/// A Classroom id was the empty string. Google never issues one, so it means a broken payload or a
/// hand-edited blob.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmptyIdError;

impl fmt::Display for EmptyIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a Classroom id is empty")
    }
}

impl std::error::Error for EmptyIdError {}

/// Ids are compared exactly as Google sent them, so unlike a `Label` they are not trimmed.
/// Deserialization goes through `parse`, so a stored blob cannot hold an id the sync could
/// never have produced.
macro_rules! classroom_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn parse(raw: &str) -> Result<$name, EmptyIdError> {
                $name::try_from(raw.to_owned())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = EmptyIdError;

            fn try_from(raw: String) -> Result<Self, Self::Error> {
                if raw.is_empty() {
                    Err(EmptyIdError)
                } else {
                    Ok($name(raw))
                }
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> String {
                id.0
            }
        }
    };
}

classroom_id! {
    /// A Classroom course, which a class is linked to.
    ClassroomCourseId
}

classroom_id! {
    /// A Classroom user, which a student is linked to.
    ClassroomUserId
}

/// A name from Classroom made to fit a [`Label`]: trimmed, cut to [`MAX_LABEL_CHARS`] characters
/// (never inside one), and `fallback` when nothing is left.
fn display_label(raw: &str, fallback: &str) -> Label {
    let cut: String = raw.trim().chars().take(MAX_LABEL_CHARS).collect();
    Label::parse(&cut)
        .unwrap_or_else(|_| Label::parse(fallback).expect("the fallback names are valid labels"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassroomStudent {
    id: ClassroomUserId,
    name: Label,
}

impl ClassroomStudent {
    pub fn new(id: ClassroomUserId, name: &str) -> ClassroomStudent {
        ClassroomStudent {
            id,
            name: display_label(name, STUDENT_FALLBACK),
        }
    }

    pub fn id(&self) -> &ClassroomUserId {
        &self.id
    }

    pub fn name(&self) -> &Label {
        &self.name
    }
}

/// One course the teacher teaches, with its roster. A roster never lists the same Classroom user
/// twice, because the first entry is the only one kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassroomCourse {
    id: ClassroomCourseId,
    name: Label,
    students: Vec<ClassroomStudent>,
}

impl ClassroomCourse {
    pub fn new(
        id: ClassroomCourseId,
        name: &str,
        mut students: Vec<ClassroomStudent>,
    ) -> ClassroomCourse {
        let mut seen = BTreeSet::new();
        students.retain(|student| seen.insert(student.id.clone()));
        ClassroomCourse {
            id,
            name: display_label(name, COURSE_FALLBACK),
            students,
        }
    }

    pub fn id(&self) -> &ClassroomCourseId {
        &self.id
    }

    pub fn name(&self) -> &Label {
        &self.name
    }

    pub fn students(&self) -> &[ClassroomStudent] {
        &self.students
    }
}

/// The Worker's answer was not the shape the contract promises.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayloadError(String);

impl fmt::Display for PayloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PayloadError {}

#[derive(Deserialize)]
struct WireCourses {
    courses: Vec<WireCourse>,
}

#[derive(Deserialize)]
struct WireCourse {
    id: ClassroomCourseId,
    name: String,
    /// The Worker always sends the roster, empty for a course nobody has joined. A missing one is
    /// a broken payload, and reading it as empty would make the sync remove every linked student.
    students: Vec<WireStudent>,
}

#[derive(Deserialize)]
struct WireStudent {
    id: ClassroomUserId,
    name: String,
}

/// Reads `{"courses":[{"id","name","students":[{"id","name"}]}]}`. Names are made to fit and
/// repeated student ids collapse, but a missing or empty id fails the whole payload, because a
/// student without an identity could not be matched on the next sync.
pub fn parse_courses(body: &str) -> Result<Vec<ClassroomCourse>, PayloadError> {
    let wire: WireCourses =
        serde_json::from_str(body).map_err(|error| PayloadError(error.to_string()))?;
    let courses = wire
        .courses
        .into_iter()
        .map(|course| {
            let students = course
                .students
                .into_iter()
                .map(|student| ClassroomStudent::new(student.id, &student.name))
                .collect();
            ClassroomCourse::new(course.id, &course.name, students)
        })
        .collect();
    Ok(courses)
}

/// What a sync does to one class's roster.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct RosterChange {
    pub(super) added: usize,
    pub(super) removed: usize,
}

/// What a sync did, for the line the teacher reads afterwards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    classes_added: usize,
    students_added: usize,
    students_removed: usize,
}

impl SyncReport {
    pub fn classes_added(&self) -> usize {
        self.classes_added
    }

    /// Counts students added to classes that already existed as well as to new ones.
    pub fn students_added(&self) -> usize {
        self.students_added
    }

    pub fn students_removed(&self) -> usize {
        self.students_removed
    }

    /// Nothing changed, which is what a sync right after a sync reports.
    pub fn is_empty(&self) -> bool {
        *self == SyncReport::default()
    }

    fn absorb(&mut self, change: RosterChange) {
        self.students_added += change.added;
        self.students_removed += change.removed;
    }
}

/// Where a course stands against the doc a plan was made from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CourseStatus {
    /// No class is linked to it yet. `chosen` is whether syncing will make one.
    New { chosen: bool },
    /// A class is linked to it, and syncing refreshes that class's roster.
    Linked,
}

/// One course as the picker shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanEntry {
    pub id: ClassroomCourseId,
    pub name: Label,
    pub students: usize,
    pub status: CourseStatus,
}

/// What syncing `courses` into a doc would do, with the teacher's choice about each new course.
/// Every new course starts chosen. What applying does is decided against the doc it is applied
/// to, so a class deleted or edited while the picker was open is neither revived nor missed, and
/// applying the same plan twice changes nothing the second time.
#[derive(Clone, Debug)]
pub struct SyncPlan {
    courses: Vec<ClassroomCourse>,
    linked: BTreeSet<ClassroomCourseId>,
    declined: BTreeSet<ClassroomCourseId>,
}

impl SyncPlan {
    /// A course repeated in `courses` counts once, as its first entry.
    pub fn new(doc: &Doc, mut courses: Vec<ClassroomCourse>) -> SyncPlan {
        let mut seen = BTreeSet::new();
        courses.retain(|course| seen.insert(course.id.clone()));
        let linked = doc
            .classes()
            .iter()
            .filter_map(|class| class.classroom_course_id().cloned())
            .collect();
        SyncPlan {
            courses,
            linked,
            declined: BTreeSet::new(),
        }
    }

    pub fn entries(&self) -> Vec<PlanEntry> {
        self.courses
            .iter()
            .map(|course| PlanEntry {
                id: course.id.clone(),
                name: course.name.clone(),
                students: course.students.len(),
                status: self.status_of(&course.id),
            })
            .collect()
    }

    /// Checks or unchecks a new course. A linked course is always refreshed, so it has no choice
    /// to make.
    pub fn set_chosen(&mut self, course: &ClassroomCourseId, chosen: bool) {
        if self.linked.contains(course) {
            return;
        }
        if chosen {
            self.declined.remove(course);
        } else {
            self.declined.insert(course.clone());
        }
    }

    /// Refreshes the class linked to each course and makes a class for each chosen new one. The
    /// teacher's selected class stays selected, unless there was none.
    pub fn apply(&self, doc: &mut Doc, rng: &mut impl Rng) -> SyncReport {
        let mut report = SyncReport::default();
        for course in &self.courses {
            if let Some(class) = doc.class_linked_to_mut(&course.id) {
                report.absorb(class.sync_roster(&course.students, rng));
                continue;
            }
            if self.status_of(&course.id) != (CourseStatus::New { chosen: true }) {
                continue;
            }
            let class = doc.add_class_linked_to(course.name.clone(), course.id.clone(), rng);
            report.classes_added += 1;
            report.absorb(class.sync_roster(&course.students, rng));
        }
        report
    }

    fn status_of(&self, course: &ClassroomCourseId) -> CourseStatus {
        if self.linked.contains(course) {
            CourseStatus::Linked
        } else {
            CourseStatus::New {
                chosen: !self.declined.contains(course),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use serde_json::json;

    use super::*;
    use crate::domain::class::{Class, Draw, StudentId};
    use crate::domain::grouping::{GroupCount, GroupIdx};
    use crate::domain::rng::SplitMix64;
    use crate::domain::seed::seed_doc;
    use crate::domain::testing::label;

    fn rng() -> SplitMix64 {
        SplitMix64::seeded(7)
    }

    fn course_id(raw: &str) -> ClassroomCourseId {
        ClassroomCourseId::parse(raw).unwrap()
    }

    fn user_id(raw: &str) -> ClassroomUserId {
        ClassroomUserId::parse(raw).unwrap()
    }

    fn course(id: &str, name: &str, students: &[(&str, &str)]) -> ClassroomCourse {
        let students = students
            .iter()
            .map(|(id, name)| ClassroomStudent::new(user_id(id), name))
            .collect();
        ClassroomCourse::new(course_id(id), name, students)
    }

    /// Plans and applies in one go, checking the doc's invariants afterwards.
    fn sync(doc: &mut Doc, courses: Vec<ClassroomCourse>) -> SyncReport {
        let report = SyncPlan::new(doc, courses).apply(doc, &mut rng());
        doc.assert_invariants();
        report
    }

    fn class_for<'a>(doc: &'a Doc, course: &str) -> &'a Class {
        doc.classes()
            .iter()
            .find(|class| class.classroom_course_id() == Some(&course_id(course)))
            .unwrap_or_else(|| panic!("no class is linked to {course}"))
    }

    fn roster(class: &Class) -> Vec<&str> {
        class.students().iter().map(|s| s.name.as_str()).collect()
    }

    fn group_of(class: &Class, name: &str) -> GroupIdx {
        let student = class.students().iter().find(|s| s.name.as_str() == name);
        class
            .grouping()
            .unwrap()
            .group_of(student.unwrap().id)
            .unwrap()
    }

    fn group_sizes(class: &Class) -> Vec<usize> {
        class
            .group_views()
            .iter()
            .map(|g| g.members.len())
            .collect()
    }

    fn counts(report: SyncReport) -> (usize, usize, usize) {
        (
            report.classes_added(),
            report.students_added(),
            report.students_removed(),
        )
    }

    fn math() -> ClassroomCourse {
        course(
            "c1",
            "Math",
            &[
                ("u1", "Ava"),
                ("u2", "Ben"),
                ("u3", "Chloe"),
                ("u4", "Diego"),
            ],
        )
    }

    #[test]
    fn an_id_keeps_exactly_the_text_it_was_given() {
        assert_eq!(course_id(" 12 34 ").as_str(), " 12 34 ");
    }

    #[test]
    fn an_empty_id_is_refused() {
        assert_eq!(ClassroomCourseId::parse(""), Err(EmptyIdError));
        assert_eq!(ClassroomUserId::parse(""), Err(EmptyIdError));
    }

    #[test]
    fn an_id_is_stored_as_a_plain_string_and_an_empty_one_is_unreadable() {
        let id = user_id("1089");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"1089\"");
        assert_eq!(serde_json::from_str::<ClassroomUserId>(&json).unwrap(), id);
        assert!(serde_json::from_str::<ClassroomUserId>("\"\"").is_err());
    }

    #[test]
    fn a_first_sync_makes_a_linked_class_for_each_course_with_its_roster() {
        let mut doc = Doc::default();
        let report = sync(
            &mut doc,
            vec![math(), course("c2", "Art", &[("u1", "Ava"), ("u9", "Zed")])],
        );

        assert_eq!(counts(report), (2, 6, 0));
        assert_eq!(doc.classes().len(), 2);
        let class = class_for(&doc, "c1");
        assert_eq!(class.name().as_str(), "Math");
        assert_eq!(roster(class), ["Ava", "Ben", "Chloe", "Diego"]);
        assert!(class.grouping().is_none());
        assert_eq!(roster(class_for(&doc, "c2")), ["Ava", "Zed"]);
        let linked: Vec<_> = class
            .students()
            .iter()
            .map(|s| s.classroom_user_id.as_ref().map(ClassroomUserId::as_str))
            .collect();
        assert_eq!(linked, [Some("u1"), Some("u2"), Some("u3"), Some("u4")]);
    }

    #[test]
    fn syncing_twice_changes_nothing_the_second_time() {
        let mut doc = Doc::default();
        sync(
            &mut doc,
            vec![math(), course("c2", "Art", &[("u1", "Ava")])],
        );
        let once = doc.clone();

        let report = sync(
            &mut doc,
            vec![math(), course("c2", "Art", &[("u1", "Ava")])],
        );

        assert!(report.is_empty());
        assert_eq!(doc, once);
    }

    #[test]
    fn applying_the_same_plan_twice_changes_nothing_the_second_time() {
        let mut doc = Doc::default();
        let plan = SyncPlan::new(&doc, vec![math()]);
        plan.apply(&mut doc, &mut rng());
        let once = doc.clone();

        let report = plan.apply(&mut doc, &mut rng());

        assert!(report.is_empty());
        assert_eq!(doc, once);
    }

    #[test]
    fn syncing_leaves_the_selected_class_selected() {
        let mut doc = seed_doc();
        let selected = doc.selected().unwrap().id();

        sync(&mut doc, vec![math()]);

        assert_eq!(doc.classes().len(), 4);
        assert_eq!(doc.selected().unwrap().id(), selected);
    }

    #[test]
    fn syncing_into_an_empty_doc_selects_the_first_class_it_makes() {
        let mut doc = Doc::default();

        sync(&mut doc, vec![math(), course("c2", "Art", &[])]);

        assert_eq!(doc.selected().unwrap().name().as_str(), "Math");
    }

    #[test]
    fn a_sync_keeps_groups_names_centers_and_everyone_where_they_were() {
        let mut doc = Doc::default();
        sync(&mut doc, vec![math()]);
        let class = doc.selected_mut().unwrap();
        class.regroup(GroupCount::new(2, 4).unwrap(), &mut rng());
        class.rename_group(GroupIdx(0), label("Table One")).unwrap();
        class.add_centers();
        class.rotate_centers();
        let before = doc.clone();

        let report = sync(&mut doc, vec![math()]);
        assert!(report.is_empty());
        assert_eq!(doc, before);

        let changed = course(
            "c1",
            "Math",
            &[("u1", "Ava"), ("u2", "Ben"), ("u4", "Diego"), ("u5", "Eli")],
        );
        let report = sync(&mut doc, vec![changed]);

        assert_eq!(counts(report), (0, 1, 1));
        let was = class_for(&before, "c1");
        let now = class_for(&doc, "c1");
        for name in ["Ava", "Ben", "Diego"] {
            assert_eq!(group_of(now, name), group_of(was, name), "{name}");
        }
        assert_eq!(
            now.grouping().unwrap().names(),
            was.grouping().unwrap().names()
        );
        assert_eq!(now.group_views()[0].name.as_str(), "Table One");
        assert_eq!(
            now.center_views().unwrap().len(),
            2,
            "the centers carried over"
        );
        assert_eq!(
            now.grouping().unwrap().centers().unwrap().round(),
            was.grouping().unwrap().centers().unwrap().round()
        );
    }

    #[test]
    fn a_new_student_joins_the_smallest_group_and_the_rest_balance_out() {
        let mut doc = Doc::default();
        sync(
            &mut doc,
            vec![course(
                "c1",
                "Math",
                &[
                    ("u1", "A"),
                    ("u2", "B"),
                    ("u3", "C"),
                    ("u4", "D"),
                    ("u5", "E"),
                ],
            )],
        );
        let class = doc.selected_mut().unwrap();
        class.regroup(GroupCount::new(2, 5).unwrap(), &mut rng());
        assert_eq!(group_sizes(class), [3, 2]);

        sync(
            &mut doc,
            vec![course(
                "c1",
                "Math",
                &[
                    ("u1", "A"),
                    ("u2", "B"),
                    ("u3", "C"),
                    ("u4", "D"),
                    ("u5", "E"),
                    ("u6", "F"),
                ],
            )],
        );

        let class = class_for(&doc, "c1");
        assert_eq!(group_sizes(class), [3, 3]);
        assert_eq!(group_of(class, "F"), GroupIdx(1));
    }

    #[test]
    fn a_student_who_left_classroom_leaves_the_roster_the_groups_and_the_oracle_round() {
        let mut doc = Doc::default();
        sync(&mut doc, vec![math()]);
        let class = doc.selected_mut().unwrap();
        class.regroup(GroupCount::new(2, 4).unwrap(), &mut rng());
        let mut rng = rng();
        for _ in 0..4 {
            class.draw(Draw::Student, &mut rng).unwrap();
        }
        let ben = class.students()[1].id;
        assert_eq!(class.oracle_remaining(Draw::Student), 0);

        let report = sync(
            &mut doc,
            vec![course(
                "c1",
                "Math",
                &[("u1", "Ava"), ("u3", "Chloe"), ("u4", "Diego")],
            )],
        );

        assert_eq!(counts(report), (0, 0, 1));
        let class = class_for(&doc, "c1");
        assert_eq!(roster(class), ["Ava", "Chloe", "Diego"]);
        assert_eq!(class.grouping().unwrap().group_of(ben), None);
        let stored = serde_json::to_value(class).unwrap();
        assert_eq!(stored["grouping"]["of"].as_object().unwrap().len(), 3);
        let drawn: Vec<u64> = stored["oracle"]["drawn"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap())
            .collect();
        assert!(!drawn.contains(&u64::from(ben.get())));
        assert_eq!(drawn.len(), 3);
    }

    #[test]
    fn students_added_by_hand_are_never_touched() {
        let mut doc = Doc::default();
        sync(&mut doc, vec![math()]);
        let hand = doc
            .selected_mut()
            .unwrap()
            .add_student(label("Ava"), &mut rng());

        let report = sync(&mut doc, vec![course("c1", "Math", &[("u9", "Ava")])]);

        assert_eq!(counts(report), (0, 1, 4));
        let class = class_for(&doc, "c1");
        assert_eq!(roster(class), ["Ava", "Ava"], "same name, two people");
        assert_eq!(class.students()[0].id, hand);
        assert_eq!(class.students()[0].classroom_user_id, None);
        assert_eq!(class.students()[1].classroom_user_id, Some(user_id("u9")));
    }

    #[test]
    fn a_linked_student_keeps_the_name_they_have_whatever_classroom_calls_them_now() {
        let mut doc = Doc::default();
        sync(&mut doc, vec![math()]);

        let report = sync(
            &mut doc,
            vec![course(
                "c1",
                "Maths",
                &[
                    ("u1", "Ava Lopez"),
                    ("u2", ""),
                    ("u3", "Chloe"),
                    ("u4", "Diego"),
                ],
            )],
        );

        assert!(report.is_empty());
        let class = class_for(&doc, "c1");
        assert_eq!(roster(class), ["Ava", "Ben", "Chloe", "Diego"]);
        assert_eq!(
            class.name().as_str(),
            "Math",
            "the class name is the teacher's now"
        );
    }

    #[test]
    fn a_course_that_lost_every_student_keeps_its_class_but_not_its_groups() {
        let mut doc = Doc::default();
        sync(&mut doc, vec![math()]);
        doc.selected_mut()
            .unwrap()
            .regroup(GroupCount::new(2, 4).unwrap(), &mut rng());

        let report = sync(&mut doc, vec![course("c1", "Math", &[])]);

        assert_eq!(counts(report), (0, 0, 4));
        let class = class_for(&doc, "c1");
        assert!(class.students().is_empty());
        assert!(class.grouping().is_none());
    }

    #[test]
    fn a_course_nobody_has_joined_makes_an_empty_class() {
        let mut doc = Doc::default();

        let report = sync(&mut doc, vec![course("c1", "Seminar", &[])]);

        assert_eq!(counts(report), (1, 0, 0));
        assert!(class_for(&doc, "c1").students().is_empty());
    }

    #[test]
    fn an_unchecked_new_course_is_not_made() {
        let mut doc = Doc::default();
        let mut plan = SyncPlan::new(&doc, vec![math(), course("c2", "Art", &[("u1", "Ava")])]);
        plan.set_chosen(&course_id("c2"), false);

        let report = plan.apply(&mut doc, &mut rng());
        doc.assert_invariants();

        assert_eq!(counts(report), (1, 4, 0));
        assert_eq!(doc.classes().len(), 1);
        assert_eq!(doc.classes()[0].name().as_str(), "Math");
    }

    #[test]
    fn a_deleted_class_is_offered_again_and_comes_back_only_when_chosen() {
        let mut doc = Doc::default();
        sync(&mut doc, vec![math()]);
        let id = doc.classes()[0].id();
        doc.delete_class(id);

        let mut plan = SyncPlan::new(&doc, vec![math()]);
        assert_eq!(plan.entries()[0].status, CourseStatus::New { chosen: true });

        plan.set_chosen(&course_id("c1"), false);
        assert_eq!(
            plan.entries()[0].status,
            CourseStatus::New { chosen: false }
        );
        assert!(plan.apply(&mut doc, &mut rng()).is_empty());
        assert!(doc.classes().is_empty());

        plan.set_chosen(&course_id("c1"), true);
        let report = plan.apply(&mut doc, &mut rng());
        doc.assert_invariants();
        assert_eq!(counts(report), (1, 4, 0));
        assert_eq!(roster(class_for(&doc, "c1")).len(), 4);
    }

    #[test]
    fn a_class_deleted_while_the_picker_was_open_is_not_brought_back() {
        let mut doc = Doc::default();
        sync(&mut doc, vec![math()]);
        let plan = SyncPlan::new(&doc, vec![math()]);
        let id = doc.classes()[0].id();
        doc.delete_class(id);

        let report = plan.apply(&mut doc, &mut rng());

        assert!(report.is_empty());
        assert!(doc.classes().is_empty());
    }

    #[test]
    fn the_plan_tells_new_courses_from_linked_ones_and_a_linked_one_has_no_choice() {
        let mut doc = Doc::default();
        sync(&mut doc, vec![math()]);
        let mut plan = SyncPlan::new(
            &doc,
            vec![math(), course("c2", "Art", &[("u1", "Ava"), ("u2", "Ben")])],
        );

        plan.set_chosen(&course_id("c1"), false);

        let entries = plan.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            (
                entries[0].name.as_str(),
                entries[0].students,
                entries[0].status
            ),
            ("Math", 4, CourseStatus::Linked)
        );
        assert_eq!(
            (
                entries[1].name.as_str(),
                entries[1].students,
                entries[1].status
            ),
            ("Art", 2, CourseStatus::New { chosen: true })
        );
    }

    #[test]
    fn two_courses_with_the_same_name_are_two_classes_and_stay_two_on_a_resync() {
        let mut doc = Doc::default();
        let courses = || {
            vec![
                course("c1", "Math", &[("u1", "Ava")]),
                course("c2", "Math", &[("u2", "Ben")]),
            ]
        };

        sync(&mut doc, courses());
        let once = doc.clone();
        let report = sync(&mut doc, courses());

        assert!(report.is_empty());
        assert_eq!(doc, once);
        assert_eq!(roster(class_for(&doc, "c1")), ["Ava"]);
        assert_eq!(roster(class_for(&doc, "c2")), ["Ben"]);
    }

    #[test]
    fn the_same_person_in_two_courses_is_a_student_of_each() {
        let mut doc = Doc::default();

        let report = sync(
            &mut doc,
            vec![
                course("c1", "Math", &[("u1", "Ava")]),
                course("c2", "Art", &[("u1", "Ava")]),
            ],
        );

        assert_eq!(counts(report), (2, 2, 0));
        sync(&mut doc, vec![course("c1", "Math", &[])]);
        assert!(class_for(&doc, "c1").students().is_empty());
        assert_eq!(roster(class_for(&doc, "c2")), ["Ava"]);
    }

    #[test]
    fn a_student_listed_twice_in_a_course_is_added_once_under_the_first_name() {
        let mut doc = Doc::default();

        let report = sync(
            &mut doc,
            vec![course(
                "c1",
                "Math",
                &[("u1", "Ava"), ("u2", "Ben"), ("u1", "Impostor")],
            )],
        );

        assert_eq!(counts(report), (1, 2, 0));
        assert_eq!(roster(class_for(&doc, "c1")), ["Ava", "Ben"]);
    }

    #[test]
    fn a_course_listed_twice_is_made_once_from_its_first_entry() {
        let mut doc = Doc::default();

        let report = sync(
            &mut doc,
            vec![
                course("c1", "Math", &[("u1", "Ava")]),
                course("c1", "Other", &[("u2", "Ben")]),
            ],
        );

        assert_eq!(counts(report), (1, 1, 0));
        assert_eq!(doc.classes().len(), 1);
        assert_eq!(doc.classes()[0].name().as_str(), "Math");
        assert_eq!(roster(&doc.classes()[0]), ["Ava"]);
    }

    #[test]
    fn long_names_are_cut_on_a_character_boundary_and_empty_ones_get_a_stand_in() {
        let at_limit = "é".repeat(MAX_LABEL_CHARS);
        let over = "é".repeat(MAX_LABEL_CHARS + 9);
        let cut_at_a_space = format!("{} y", "x".repeat(MAX_LABEL_CHARS - 1));
        let mut doc = Doc::default();

        sync(
            &mut doc,
            vec![
                course(
                    "c1",
                    &over,
                    &[
                        ("u1", at_limit.as_str()),
                        ("u2", over.as_str()),
                        ("u3", ""),
                        ("u4", " \t\n"),
                        ("u5", "  Ava  "),
                        ("u6", cut_at_a_space.as_str()),
                    ],
                ),
                course("c2", "   ", &[]),
            ],
        );

        let class = class_for(&doc, "c1");
        assert_eq!(class.name().as_str(), at_limit);
        assert_eq!(
            roster(class),
            [
                at_limit.as_str(),
                at_limit.as_str(),
                "Student",
                "Student",
                "Ava",
                "x".repeat(MAX_LABEL_CHARS - 1).as_str(),
            ],
            "the cut that ends in a space is trimmed"
        );
        assert_eq!(class_for(&doc, "c2").name().as_str(), "Class");
    }

    #[test]
    fn courses_are_read_from_the_workers_json() {
        let body = json!({"courses": [
            {"id": "c1", "name": "Math", "teacher": "ignored", "students": [
                {"id": "u1", "name": "Ava Lopez", "photo": "ignored"},
                {"id": "u1", "name": "Again"},
            ]},
            {"id": "c2", "name": " ", "students": []},
            {"id": "c3", "name": "Seminar", "students": []},
        ]})
        .to_string();

        let courses = parse_courses(&body).unwrap();

        assert_eq!(courses.len(), 3);
        assert_eq!(courses[0], course("c1", "Math", &[("u1", "Ava Lopez")]));
        assert_eq!(courses[0].students().len(), 1, "the repeated id collapsed");
        assert_eq!(courses[1].name().as_str(), "Class");
        assert!(courses[2].students().is_empty());
    }

    #[test]
    fn a_payload_that_breaks_the_contract_is_refused_with_its_cause() {
        let bodies = [
            "",
            "[]",
            r#"{"courses":"none"}"#,
            r#"{"courses":[{"name":"Math","students":[]}]}"#,
            r#"{"courses":[{"id":"","name":"Math","students":[]}]}"#,
            r#"{"courses":[{"id":"c1","name":"Math","students":[{"id":"","name":"Ava"}]}]}"#,
            r#"{"courses":[{"id":"c1","name":"Math","students":[{"name":"Ava"}]}]}"#,
            r#"{"courses":[{"id":7,"name":"Math","students":[]}]}"#,
            r#"{"courses":[{"id":"c1","name":"Math"}]}"#,
        ];
        for body in bodies {
            let error = parse_courses(body).unwrap_err();
            assert!(!error.to_string().is_empty(), "{body}");
        }
    }

    fn roll(rng: &mut SplitMix64, n: usize) -> usize {
        rng.below(NonZeroUsize::new(n).unwrap())
    }

    fn random_courses(rng: &mut SplitMix64) -> Vec<ClassroomCourse> {
        let long = "w".repeat(60);
        let names = ["Ava", "Ben", "", "  ", "Chloe", long.as_str()];
        (0..roll(rng, 4))
            .map(|_| {
                let students: Vec<(String, &str)> = (0..roll(rng, 9))
                    .map(|_| (format!("u{}", roll(rng, 12)), names[roll(rng, names.len())]))
                    .collect();
                let students: Vec<(&str, &str)> = students
                    .iter()
                    .map(|(id, name)| (id.as_str(), *name))
                    .collect();
                course(
                    &format!("c{}", roll(rng, 3)),
                    names[roll(rng, names.len())],
                    &students,
                )
            })
            .collect()
    }

    fn assert_links_are_unique(doc: &Doc) {
        let courses: Vec<&ClassroomCourseId> = doc
            .classes()
            .iter()
            .filter_map(Class::classroom_course_id)
            .collect();
        let distinct: BTreeSet<_> = courses.iter().collect();
        assert_eq!(distinct.len(), courses.len(), "two classes share a course");
        for class in doc.classes() {
            let users: Vec<&ClassroomUserId> = class
                .students()
                .iter()
                .filter_map(|s| s.classroom_user_id.as_ref())
                .collect();
            let distinct: BTreeSet<_> = users.iter().collect();
            assert_eq!(distinct.len(), users.len(), "a person is on a roster twice");
        }
    }

    #[test]
    fn invariants_and_idempotence_survive_a_long_random_mix_of_edits_and_syncs() {
        for seed in 0..40 {
            let mut rng = SplitMix64::seeded(seed);
            let mut doc = Doc::default();
            for _ in 0..60 {
                match roll(&mut rng, 12) {
                    0..=5 => {
                        let mut plan = SyncPlan::new(&doc, random_courses(&mut rng));
                        for entry in plan.entries() {
                            if roll(&mut rng, 3) == 0 {
                                plan.set_chosen(&entry.id, false);
                            }
                        }
                        plan.apply(&mut doc, &mut rng);
                        doc.assert_invariants();
                        assert_links_are_unique(&doc);

                        let once = doc.clone();
                        let report = plan.apply(&mut doc, &mut rng);
                        assert!(report.is_empty(), "seed {seed}");
                        assert_eq!(doc, once, "seed {seed}");
                    }
                    6 if !doc.classes().is_empty() => {
                        let id = doc.classes()[roll(&mut rng, doc.classes().len())].id();
                        doc.delete_class(id);
                    }
                    7 => {
                        if let Some(class) = doc.selected_mut() {
                            class.add_student(label("By hand"), &mut rng);
                        }
                    }
                    8 => {
                        if let Some(class) = doc.selected_mut() {
                            let n = class.students().len();
                            if let Ok(count) = GroupCount::new(1 + roll(&mut rng, 3), n) {
                                class.regroup(count, &mut rng);
                            }
                        }
                    }
                    9 => {
                        if let Some(class) = doc.selected_mut() {
                            class.draw(Draw::Student, &mut rng);
                            class.draw(Draw::Group, &mut rng);
                        }
                    }
                    10 if !doc.classes().is_empty() => {
                        let id = doc.classes()[roll(&mut rng, doc.classes().len())].id();
                        doc.select(id);
                    }
                    _ => {
                        if let Some(class) = doc.selected_mut() {
                            let ids: Vec<StudentId> =
                                class.students().iter().map(|s| s.id).collect();
                            if !ids.is_empty() {
                                class.remove_student(ids[roll(&mut rng, ids.len())]);
                            }
                        }
                    }
                }
                doc.assert_invariants();
            }
        }
    }
}
