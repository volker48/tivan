use serde::{Deserialize, Serialize};

use super::avatar::{self, Avatar};
use super::centers::CenterIdx;
use super::classroom::{ClassroomCourseId, ClassroomUserId};
use super::grouping::{GroupCount, GroupIdx, Grouping, MoveError};
use super::label::{Label, LabelError};
use super::oracle::Oracle;
use super::rng::Rng;

/// Ids are never reused within a class, so a stale reference to a removed student can never
/// resolve to a newcomer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StudentId(pub(super) u32);

impl StudentId {
    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ClassId(pub(super) u32);

impl ClassId {
    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Student {
    pub id: StudentId,
    pub name: Label,
    pub avatar: Avatar,
    /// Set for a student a Classroom sync added. A student added by hand has none, so no sync
    /// ever touches them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classroom_user_id: Option<ClassroomUserId>,
}

/// What the oracle picks from: a student of the roster or a group of the grouping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Draw {
    Student,
    Group,
}

/// The oracle's answer, resolved to names and avatars so the UI never looks it up after the
/// fact. A group's members are in roster order, as in [`GroupView`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    Student(Student),
    Group {
        idx: GroupIdx,
        name: Label,
        members: Vec<Student>,
    },
}

impl Pick {
    pub fn kind(&self) -> Draw {
        match self {
            Pick::Student(_) => Draw::Student,
            Pick::Group { .. } => Draw::Group,
        }
    }
}

/// One group with its members resolved, in roster order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupView<'a> {
    pub idx: GroupIdx,
    pub name: &'a Label,
    pub members: Vec<&'a Student>,
}

/// One center with the group standing there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CenterView<'a> {
    pub idx: CenterIdx,
    pub name: &'a Label,
    pub group: GroupIdx,
}

/// Owns the roster, the grouping, and the student oracle round for one class. Roster edits go
/// through this type so the three stay consistent within a single call. The group round lives in
/// the grouping.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Class {
    id: ClassId,
    name: Label,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    classroom_course_id: Option<ClassroomCourseId>,
    students: Vec<Student>,
    next_student: u32,
    grouping: Option<Grouping>,
    oracle: Oracle<StudentId>,
}

impl Class {
    pub(super) fn new(id: ClassId, name: Label, students: Vec<Label>, rng: &mut impl Rng) -> Class {
        let mut class = Class {
            id,
            name,
            classroom_course_id: None,
            students: Vec::with_capacity(students.len()),
            next_student: 0,
            grouping: None,
            oracle: Oracle::default(),
        };
        for student in students {
            class.enroll(student, rng);
        }
        class
    }

    pub fn id(&self) -> ClassId {
        self.id
    }

    pub fn name(&self) -> &Label {
        &self.name
    }

    /// The Classroom course this class was made from, if it was.
    pub fn classroom_course_id(&self) -> Option<&ClassroomCourseId> {
        self.classroom_course_id.as_ref()
    }

    pub fn students(&self) -> &[Student] {
        &self.students
    }

    pub fn student(&self, id: StudentId) -> Option<&Student> {
        self.students.iter().find(|s| s.id == id)
    }

    pub fn grouping(&self) -> Option<&Grouping> {
        self.grouping.as_ref()
    }

    pub fn rename(&mut self, name: Label) {
        self.name = name;
    }

    pub(super) fn set_id(&mut self, id: ClassId) {
        self.id = id;
    }

    /// A student added after grouping joins the smallest group. Their avatar is chosen here, once,
    /// and never changes afterwards.
    pub fn add_student(&mut self, name: Label, rng: &mut impl Rng) -> StudentId {
        let id = self.enroll(name, rng);
        self.reconcile();
        id
    }

    /// Idempotent. Removes the student from the roster, the grouping, and the oracle round.
    pub fn remove_student(&mut self, id: StudentId) {
        self.students.retain(|s| s.id != id);
        self.reconcile();
    }

    /// Fresh random split with fresh names, replacing any existing grouping. A `count` made for
    /// an earlier, larger roster is clamped to the current one. Centers carry over with their
    /// names, fitted to the new group count, and start again at round 1.
    pub fn regroup(&mut self, count: GroupCount, rng: &mut impl Rng) {
        let mut centers = self.grouping.as_mut().and_then(Grouping::take_centers);
        if let Some(centers) = &mut centers {
            centers.restart();
        }
        self.grouping = GroupCount::nearest(count.get(), self.students.len())
            .ok()
            .map(|count| {
                let mut grouping = Grouping::split(&self.students, count, rng);
                grouping.set_centers(centers);
                grouping
            });
    }

    /// Re-deals the students across the existing groups, keeping the group names and the
    /// centers. Does nothing when there is no grouping yet.
    pub fn reshuffle(&mut self, rng: &mut impl Rng) {
        if let Some(grouping) = &mut self.grouping {
            grouping.reshuffle(&self.students, rng);
        }
    }

    pub fn move_student(&mut self, id: StudentId, to: GroupIdx) -> Result<(), MoveError> {
        self.grouping_mut()?.move_member(id, to)
    }

    pub fn rename_group(&mut self, idx: GroupIdx, name: Label) -> Result<(), MoveError> {
        self.grouping_mut()?.rename(idx, name)
    }

    pub fn reroll_group_name(
        &mut self,
        idx: GroupIdx,
        rng: &mut impl Rng,
    ) -> Result<(), MoveError> {
        self.grouping_mut()?.reroll_name(idx, rng)
    }

    /// Gives each group a center. Does nothing without groups, or when centers are in use already.
    pub fn add_centers(&mut self) {
        if let Some(grouping) = &mut self.grouping {
            grouping.add_centers();
        }
    }

    /// Drops the centers and their names.
    pub fn remove_centers(&mut self) {
        if let Some(grouping) = &mut self.grouping {
            grouping.remove_centers();
        }
    }

    /// Moves every group to the next center, and from the last center back to the first.
    pub fn rotate_centers(&mut self) {
        if let Some(grouping) = &mut self.grouping {
            grouping.rotate_centers();
        }
    }

    pub fn rename_center(&mut self, idx: CenterIdx, name: Label) -> Result<(), MoveError> {
        self.grouping
            .as_mut()
            .ok_or(MoveError::UnknownCenter)?
            .rename_center(idx, name)
    }

    /// Groups with resolved members, empty when there is no grouping.
    pub fn group_views(&self) -> Vec<GroupView<'_>> {
        let Some(grouping) = &self.grouping else {
            return Vec::new();
        };
        let mut views: Vec<GroupView<'_>> = grouping
            .names()
            .iter()
            .enumerate()
            .map(|(index, name)| GroupView {
                idx: GroupIdx(index),
                name,
                members: Vec::new(),
            })
            .collect();
        for student in &self.students {
            if let Some(group) = grouping.group_of(student.id) {
                views[group.get()].members.push(student);
            }
        }
        views
    }

    /// The centers in center order, each with the group standing there. `None` when the class
    /// has no groups or does not use centers.
    pub fn center_views(&self) -> Option<Vec<CenterView<'_>>> {
        let centers = self.grouping.as_ref()?.centers()?;
        let views = centers
            .names()
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let idx = CenterIdx(index);
                CenterView {
                    idx,
                    name,
                    group: centers.group_at(idx),
                }
            })
            .collect();
        Some(views)
    }

    /// `None` when the class has no students, or when `kind` is [`Draw::Group`] and it has no
    /// grouping.
    pub fn draw(&mut self, kind: Draw, rng: &mut impl Rng) -> Option<Pick> {
        match kind {
            Draw::Student => {
                let id = self.oracle.draw(&self.roster_ids(), rng)?;
                let student = self.student(id).expect("the oracle draws from the roster");
                Some(Pick::Student(student.clone()))
            }
            Draw::Group => {
                let idx = self.grouping.as_mut()?.draw(rng)?;
                let views = self.group_views();
                let view = &views[idx.get()];
                Some(Pick::Group {
                    idx,
                    name: view.name.clone(),
                    members: view.members.iter().map(|&s| s.clone()).collect(),
                })
            }
        }
    }

    /// Students or groups the oracle has not chosen yet this round. Zero for groups when the
    /// class has no grouping.
    pub fn oracle_remaining(&self, kind: Draw) -> usize {
        match kind {
            Draw::Student => self.oracle.remaining(&self.roster_ids()),
            Draw::Group => self.grouping.as_ref().map_or(0, Grouping::remaining),
        }
    }

    fn roster_ids(&self) -> Vec<StudentId> {
        self.students.iter().map(|s| s.id).collect()
    }

    /// The avatar is per student, not per name, so two children called Emma get different faces.
    fn enroll(&mut self, name: Label, rng: &mut impl Rng) -> StudentId {
        let id = StudentId(self.next_student);
        self.next_student += 1;
        let avatar = avatar::pick(self.students.iter().map(|s| s.avatar), rng);
        self.students.push(Student {
            id,
            name,
            avatar,
            classroom_user_id: None,
        });
        id
    }

    fn grouping_mut(&mut self) -> Result<&mut Grouping, MoveError> {
        self.grouping.as_mut().ok_or(MoveError::UnknownGroup)
    }

    /// Restores every cross-reference after a roster edit or a load. Idempotent.
    pub(super) fn reconcile(&mut self) {
        self.next_student = self
            .students
            .iter()
            .map(|s| s.id.0.saturating_add(1))
            .fold(self.next_student, u32::max);
        // A duplicated id would make two children share one group slot and one oracle turn, so the
        // later one gets a fresh id and is placed like a newcomer.
        let mut seen = std::collections::BTreeSet::new();
        for index in 0..self.students.len() {
            if !seen.insert(self.students[index].id) {
                self.students[index].id = StudentId(self.next_student);
                self.next_student += 1;
            }
        }
        self.oracle.retain(&self.roster_ids());
        if let Some(grouping) = &mut self.grouping {
            grouping.reconcile(&self.students);
        }
        if self.students.is_empty() || self.grouping.as_ref().is_some_and(Grouping::is_empty) {
            self.grouping = None;
        }
    }

    #[cfg(test)]
    pub(super) fn assert_invariants(&self) {
        let mut seen = std::collections::BTreeSet::new();
        for student in &self.students {
            assert!(seen.insert(student.id), "duplicate student id");
            assert!(
                student.id.0 < self.next_student,
                "counter not above every id"
            );
        }
        if let Some(grouping) = &self.grouping {
            assert!(!self.students.is_empty());
            grouping.assert_invariants(&self.students);
        }
        self.oracle.assert_invariants(&self.roster_ids());
    }
}

/// A pasted roster. `too_long` counts lines that were not added, so the UI can tell the teacher.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedRoster {
    pub names: Vec<Label>,
    pub too_long: usize,
}

/// One name per non-blank line. Tolerates CRLF, trims each line, and keeps only the text before a
/// tab so a two-column spreadsheet paste yields first names. Duplicates stay because two children
/// can share a name.
pub fn parse_roster(text: &str) -> ParsedRoster {
    let mut parsed = ParsedRoster::default();
    for line in text.lines() {
        match Label::parse(line.split('\t').next().unwrap_or_default()) {
            Ok(name) => parsed.names.push(name),
            Err(LabelError::TooLong) => parsed.too_long += 1,
            Err(LabelError::Empty) => {}
        }
    }
    parsed
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap, HashSet};
    use std::num::NonZeroUsize;

    use super::*;
    use crate::domain::label::MAX_LABEL_CHARS;
    use crate::domain::names::MAX_GROUPS;
    use crate::domain::rng::SplitMix64;
    use crate::domain::testing::{label, labels, numbered};

    fn class_of(n: u32) -> Class {
        Class::new(ClassId(0), label("Room 1"), numbered(n), &mut rng(0))
    }

    fn count(m: usize, class: &Class) -> GroupCount {
        GroupCount::new(m, class.students().len()).unwrap()
    }

    fn rng(seed: u64) -> SplitMix64 {
        SplitMix64::seeded(seed)
    }

    fn member_ids(class: &Class) -> Vec<Vec<StudentId>> {
        class
            .group_views()
            .iter()
            .map(|g| g.members.iter().map(|s| s.id).collect())
            .collect()
    }

    fn group_sizes(class: &Class) -> Vec<usize> {
        member_ids(class).iter().map(Vec::len).collect()
    }

    /// Removes students from the end of the roster until `keep` remain.
    fn keep_first(class: &mut Class, keep: usize) {
        let gone: Vec<StudentId> = class.students().iter().skip(keep).map(|s| s.id).collect();
        for id in gone {
            class.remove_student(id);
        }
    }

    fn names_of(class: &Class) -> Vec<Label> {
        class.grouping().unwrap().names().to_vec()
    }

    fn draw_student(class: &mut Class, rng: &mut SplitMix64) -> Student {
        match class.draw(Draw::Student, rng) {
            Some(Pick::Student(student)) => student,
            other => panic!("expected a student, got {other:?}"),
        }
    }

    fn draw_group(class: &mut Class, rng: &mut SplitMix64) -> GroupIdx {
        match class.draw(Draw::Group, rng) {
            Some(Pick::Group { idx, .. }) => idx,
            other => panic!("expected a group, got {other:?}"),
        }
    }

    #[test]
    fn a_new_class_numbers_students_in_order_with_no_grouping() {
        let class = Class::new(
            ClassId(7),
            label("Art"),
            labels(&["Ava", "Ben", "Ava"]),
            &mut rng(1),
        );
        let ids: Vec<u32> = class.students().iter().map(|s| s.id.get()).collect();
        assert_eq!(ids, [0, 1, 2]);
        let names: Vec<&str> = class.students().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Ava", "Ben", "Ava"]);
        assert_eq!(class.id().get(), 7);
        assert_eq!(class.name().as_str(), "Art");
        assert!(class.grouping().is_none());
        class.assert_invariants();
    }

    #[test]
    fn student_ids_are_never_reused_after_a_removal() {
        let mut class = class_of(3);
        let last = class.students()[2].id;
        class.remove_student(last);
        let newcomer = class.add_student(label("New"), &mut rng(1));
        assert_ne!(newcomer, last);
        assert!(class.student(last).is_none());
        assert_eq!(class.student(newcomer).unwrap().name.as_str(), "New");
        class.assert_invariants();
    }

    #[test]
    fn reconcile_moves_the_id_counter_above_every_existing_id() {
        let mut class = class_of(3);
        class.next_student = 0;
        class.reconcile();
        let newcomer = class.add_student(label("New"), &mut rng(1));
        assert!(class.students()[..3].iter().all(|s| s.id != newcomer));
        class.assert_invariants();
    }

    fn avatars(class: &Class) -> HashMap<StudentId, Avatar> {
        class.students().iter().map(|s| (s.id, s.avatar)).collect()
    }

    #[test]
    fn a_small_class_gives_every_student_a_different_avatar() {
        let class = class_of(Avatar::ALL.len() as u32);
        let distinct: HashSet<Avatar> = avatars(&class).into_values().collect();
        assert_eq!(distinct.len(), Avatar::ALL.len());
    }

    #[test]
    fn two_students_with_the_same_name_get_different_avatars() {
        for seed in 0..20 {
            let class = Class::new(
                ClassId(0),
                label("Art"),
                labels(&["Emma", "Emma"]),
                &mut rng(seed),
            );
            let [first, second] = class.students() else {
                panic!("two students were enrolled");
            };
            assert_ne!(first.avatar, second.avatar, "seed {seed}");
        }
    }

    #[test]
    fn a_student_added_later_gets_an_avatar_nobody_in_the_class_has() {
        for seed in 0..20 {
            let mut class = class_of(5);
            let before = avatars(&class);

            let id = class.add_student(label("Late"), &mut rng(seed));

            let added = class.student(id).unwrap().avatar;
            assert!(
                !before.values().any(|&avatar| avatar == added),
                "seed {seed}"
            );
        }
    }

    #[test]
    fn a_freed_avatar_is_handed_out_again_before_any_repeats() {
        let mut class = class_of(Avatar::ALL.len() as u32 - 1);
        let gone = class.students()[4].id;
        let freed = class.student(gone).unwrap().avatar;
        class.remove_student(gone);

        let id = class.add_student(label("Late"), &mut rng(4));
        let taken = class.add_student(label("Later"), &mut rng(5));

        assert_eq!(class.student(id).unwrap().avatar, freed);
        assert_ne!(class.student(taken).unwrap().avatar, freed);
    }

    #[test]
    fn avatars_stay_with_their_students_through_every_roster_and_group_edit() {
        let mut class = class_of(10);
        let mut before = avatars(&class);
        let mut rng = rng(6);

        class.regroup(count(3, &class), &mut rng);
        class.reshuffle(&mut rng);
        let mover = class.students()[0].id;
        let home = class.grouping().unwrap().group_of(mover).unwrap();
        class
            .move_student(mover, GroupIdx((home.get() + 1) % 3))
            .unwrap();
        class.draw(Draw::Student, &mut rng).unwrap();
        class.add_student(label("Late"), &mut rng);
        let gone = class.students()[5].id;
        class.remove_student(gone);
        before.remove(&gone);
        class.reconcile();

        let after = avatars(&class);
        assert_eq!(after.len(), 10);
        for (id, avatar) in before {
            assert_eq!(after[&id], avatar, "student {} changed avatar", id.get());
        }
    }

    #[test]
    fn rename_changes_the_class_name() {
        let mut class = class_of(1);
        class.rename(label("Period 2"));
        assert_eq!(class.name().as_str(), "Period 2");
    }

    #[test]
    fn regroup_partitions_the_roster_into_the_requested_number_of_groups() {
        let mut class = class_of(7);
        class.regroup(count(3, &class), &mut rng(1));
        assert_eq!(group_sizes(&class), [3, 2, 2]);
        class.assert_invariants();

        class.regroup(count(7, &class), &mut rng(2));
        assert_eq!(group_sizes(&class), [1; 7]);
    }

    #[test]
    fn regroup_clamps_a_count_made_for_a_larger_roster() {
        let mut class = class_of(5);
        let stale = count(5, &class);
        keep_first(&mut class, 3);
        class.regroup(stale, &mut rng(1));
        assert_eq!(group_sizes(&class), [1, 1, 1]);
        class.assert_invariants();
    }

    #[test]
    fn regroup_on_an_empty_class_leaves_no_grouping() {
        let mut class = class_of(3);
        let stale = count(2, &class);
        keep_first(&mut class, 0);
        class.regroup(stale, &mut rng(1));
        assert!(class.grouping().is_none());
        assert!(class.group_views().is_empty());
    }

    #[test]
    fn reshuffle_keeps_group_names_and_count_but_changes_members() {
        let mut class = class_of(12);
        class.regroup(count(4, &class), &mut rng(1));
        let before_names = names_of(&class);
        let before_members = member_ids(&class);

        class.reshuffle(&mut rng(2));

        assert_eq!(group_sizes(&class), [3, 3, 3, 3]);
        assert_eq!(names_of(&class), before_names);
        assert_ne!(member_ids(&class), before_members);
        class.assert_invariants();
    }

    #[test]
    fn reshuffle_clamps_to_a_roster_that_shrank_below_the_group_count() {
        let mut class = class_of(6);
        class.regroup(count(6, &class), &mut rng(1));
        let original_names = names_of(&class);
        keep_first(&mut class, 3);
        assert_eq!(class.group_views().len(), 6);

        class.reshuffle(&mut rng(2));

        assert_eq!(group_sizes(&class), [1, 1, 1]);
        assert_eq!(names_of(&class), original_names[..3]);
        class.assert_invariants();
    }

    #[test]
    fn reshuffle_keeps_a_group_name_the_teacher_typed() {
        let mut class = class_of(9);
        class.regroup(count(3, &class), &mut rng(1));
        class
            .rename_group(GroupIdx(1), label("Table Four"))
            .unwrap();
        let before_names = names_of(&class);

        for seed in 2..6 {
            class.reshuffle(&mut rng(seed));
            assert_eq!(names_of(&class), before_names);
        }
        assert_eq!(class.group_views()[1].name.as_str(), "Table Four");
        class.assert_invariants();
    }

    #[test]
    fn reshuffle_without_a_grouping_does_nothing() {
        let mut class = class_of(4);
        class.reshuffle(&mut rng(1));
        assert!(class.grouping().is_none());
    }

    #[test]
    fn a_student_added_after_grouping_joins_the_smallest_group() {
        let mut class = class_of(5);
        class.regroup(count(2, &class), &mut rng(1));
        assert_eq!(group_sizes(&class), [3, 2]);

        let id = class.add_student(label("Late"), &mut rng(2));

        assert_eq!(class.grouping().unwrap().group_of(id), Some(GroupIdx(1)));
        assert_eq!(group_sizes(&class), [3, 3]);
        class.assert_invariants();
    }

    #[test]
    fn removing_a_student_clears_them_from_roster_grouping_and_oracle() {
        let mut class = class_of(6);
        class.regroup(count(2, &class), &mut rng(1));
        let drawn = draw_student(&mut class, &mut rng(3));
        assert_eq!(class.oracle_remaining(Draw::Student), 5);

        class.remove_student(drawn.id);

        assert!(class.student(drawn.id).is_none());
        assert_eq!(class.grouping().unwrap().group_of(drawn.id), None);
        assert!(
            class
                .group_views()
                .iter()
                .all(|g| g.members.iter().all(|s| s.id != drawn.id))
        );
        assert_eq!(class.oracle_remaining(Draw::Student), 5);
        class.assert_invariants();
    }

    #[test]
    fn removing_is_idempotent_and_ignores_unknown_ids() {
        let mut class = class_of(3);
        let gone = class.students()[1].id;
        class.remove_student(gone);
        let once = class.clone();
        class.remove_student(gone);
        class.remove_student(StudentId(99));
        assert_eq!(class, once);
    }

    #[test]
    fn removing_the_last_student_drops_the_grouping() {
        let mut class = class_of(1);
        class.regroup(count(1, &class), &mut rng(1));
        class.remove_student(class.students()[0].id);
        assert!(class.grouping().is_none());
        class.assert_invariants();
    }

    #[test]
    fn move_student_places_them_in_the_chosen_group_only() {
        let mut class = class_of(6);
        class.regroup(count(3, &class), &mut rng(1));
        let mover = class.students()[0].id;
        let home = class.grouping().unwrap().group_of(mover).unwrap();
        let target = GroupIdx((home.get() + 1) % 3);
        let sizes_before = group_sizes(&class);

        class.move_student(mover, target).unwrap();

        let views = class.group_views();
        assert!(views[target.get()].members.iter().any(|s| s.id == mover));
        assert!(views[home.get()].members.iter().all(|s| s.id != mover));
        assert_eq!(
            views[target.get()].members.len(),
            sizes_before[target.get()] + 1
        );
        assert_eq!(
            views[home.get()].members.len(),
            sizes_before[home.get()] - 1
        );
    }

    #[test]
    fn group_edits_report_stale_references_instead_of_panicking() {
        let mut class = class_of(3);
        let first = class.students()[0].id;
        assert_eq!(
            class.move_student(first, GroupIdx(0)),
            Err(MoveError::UnknownGroup)
        );
        assert_eq!(
            class.rename_group(GroupIdx(0), label("x")),
            Err(MoveError::UnknownGroup)
        );
        assert_eq!(
            class.reroll_group_name(GroupIdx(0), &mut rng(1)),
            Err(MoveError::UnknownGroup)
        );

        class.regroup(count(2, &class), &mut rng(1));
        assert_eq!(
            class.move_student(StudentId(99), GroupIdx(0)),
            Err(MoveError::UnknownStudent)
        );
        assert_eq!(
            class.move_student(first, GroupIdx(2)),
            Err(MoveError::UnknownGroup)
        );
    }

    #[test]
    fn rename_and_reroll_change_only_the_chosen_group_name() {
        let mut class = class_of(6);
        class.regroup(count(3, &class), &mut rng(1));
        let before = names_of(&class);

        class.rename_group(GroupIdx(1), label("The Owls")).unwrap();
        assert_eq!(names_of(&class)[1].as_str(), "The Owls");
        assert_eq!(names_of(&class)[0], before[0]);

        class.reroll_group_name(GroupIdx(2), &mut rng(5)).unwrap();
        let after = names_of(&class);
        assert_ne!(after[2], before[2]);
        assert_eq!(after[0], before[0]);
        assert_eq!(after[1].as_str(), "The Owls");
    }

    #[test]
    fn group_views_list_members_in_roster_order_and_cover_everyone_once() {
        let mut class = class_of(10);
        assert!(class.group_views().is_empty());
        class.regroup(count(3, &class), &mut rng(9));

        let views = class.group_views();
        let names: Vec<&Label> = views.iter().map(|v| v.name).collect();
        assert_eq!(
            names,
            class.grouping().unwrap().names().iter().collect::<Vec<_>>()
        );
        for (i, view) in views.iter().enumerate() {
            assert_eq!(view.idx.get(), i);
            assert!(view.members.windows(2).all(|w| w[0].id < w[1].id));
        }
        let mut all: Vec<StudentId> = views
            .iter()
            .flat_map(|v| v.members.iter().map(|s| s.id))
            .collect();
        all.sort();
        let roster: Vec<StudentId> = class.students().iter().map(|s| s.id).collect();
        assert_eq!(all, roster);
    }

    #[test]
    fn the_oracle_names_each_student_once_per_round_and_counts_down() {
        let mut class = class_of(5);
        let mut rng = rng(4);
        let mut chosen = HashSet::new();
        for left in (0..5).rev() {
            let pick = draw_student(&mut class, &mut rng);
            assert_eq!(class.student(pick.id), Some(&pick));
            assert!(chosen.insert(pick.id));
            assert_eq!(class.oracle_remaining(Draw::Student), left);
        }
        let next = draw_student(&mut class, &mut rng);
        assert_eq!(class.oracle_remaining(Draw::Student), 4);
        assert!(class.student(next.id).is_some());
    }

    #[test]
    fn the_oracle_has_no_answer_for_an_empty_class() {
        let mut class = class_of(0);
        for kind in [Draw::Student, Draw::Group] {
            assert_eq!(class.draw(kind, &mut rng(1)), None);
            assert_eq!(class.oracle_remaining(kind), 0);
        }
    }

    #[test]
    fn picking_a_group_needs_groups_but_picking_a_student_does_not() {
        let mut class = class_of(3);
        let before = class.clone();

        assert_eq!(class.draw(Draw::Group, &mut rng(1)), None);
        assert_eq!(class.oracle_remaining(Draw::Group), 0);
        assert_eq!(class, before);

        let pick = draw_student(&mut class, &mut rng(1));
        assert!(class.student(pick.id).is_some());
        assert_eq!(class.oracle_remaining(Draw::Student), 2);
    }

    #[test]
    fn a_group_pick_resolves_the_name_and_members_the_wall_shows() {
        let mut class = class_of(10);
        class.regroup(count(3, &class), &mut rng(1));
        class
            .rename_group(GroupIdx(1), label("Table Four"))
            .unwrap();
        let mut rng = rng(5);
        let mut picked_names = Vec::new();
        for _ in 0..3 {
            let pick = class.draw(Draw::Group, &mut rng).unwrap();
            assert_eq!(pick.kind(), Draw::Group);
            let Pick::Group { idx, name, members } = pick else {
                panic!("expected a group");
            };
            let views = class.group_views();
            assert_eq!(name, *views[idx.get()].name);
            let shown: Vec<&Student> = members.iter().collect();
            assert_eq!(shown, views[idx.get()].members);
            picked_names.push(name);
        }
        assert!(picked_names.contains(&label("Table Four")));
    }

    #[test]
    fn the_oracle_names_each_group_once_per_round_and_counts_down() {
        for groups in [2, 3, 6] {
            for seed in 0..30 {
                let mut class = class_of(12);
                class.regroup(count(groups, &class), &mut rng(1));
                let mut rng = rng(seed);
                let mut previous = None;
                for _ in 0..3 {
                    let mut chosen = BTreeSet::new();
                    for left in (0..groups).rev() {
                        let idx = draw_group(&mut class, &mut rng);
                        assert!(chosen.insert(idx), "groups={groups} seed={seed}");
                        assert_ne!(Some(idx), previous, "groups={groups} seed={seed}");
                        previous = Some(idx);
                        assert_eq!(class.oracle_remaining(Draw::Group), left);
                    }
                }
            }
        }
    }

    #[test]
    fn the_group_and_student_rounds_are_separate() {
        let mut class = class_of(6);
        class.regroup(count(3, &class), &mut rng(1));
        let mut rng = rng(2);

        draw_group(&mut class, &mut rng);
        assert_eq!(class.oracle_remaining(Draw::Student), 6);
        draw_student(&mut class, &mut rng);
        draw_student(&mut class, &mut rng);

        assert_eq!(class.oracle_remaining(Draw::Group), 2);
        assert_eq!(class.oracle_remaining(Draw::Student), 4);
    }

    #[test]
    fn regrouping_starts_a_fresh_group_round_and_leaves_the_student_round_alone() {
        let mut class = class_of(8);
        class.regroup(count(4, &class), &mut rng(1));
        let mut rng = rng(2);
        draw_group(&mut class, &mut rng);
        draw_group(&mut class, &mut rng);
        draw_student(&mut class, &mut rng);

        class.regroup(count(4, &class), &mut rng);
        assert_eq!(class.oracle_remaining(Draw::Group), 4);
        assert_eq!(class.oracle_remaining(Draw::Student), 7);

        draw_group(&mut class, &mut rng);
        class.regroup(count(2, &class), &mut rng);
        assert_eq!(class.oracle_remaining(Draw::Group), 2);
        class.assert_invariants();
    }

    #[test]
    fn reshuffling_keeps_the_group_round_going() {
        for seed in 0..30 {
            let mut class = class_of(12);
            class.regroup(count(4, &class), &mut rng(1));
            let mut rng = rng(seed);
            let first = [
                draw_group(&mut class, &mut rng),
                draw_group(&mut class, &mut rng),
            ];

            class.reshuffle(&mut rng);

            assert_eq!(class.oracle_remaining(Draw::Group), 2, "seed {seed}");
            let rest = [
                draw_group(&mut class, &mut rng),
                draw_group(&mut class, &mut rng),
            ];
            let everyone: BTreeSet<GroupIdx> = first.into_iter().chain(rest).collect();
            assert_eq!(everyone.len(), 4, "seed {seed}");
            class.assert_invariants();
        }
    }

    #[test]
    fn a_group_round_shrinks_with_the_groups_and_never_yields_a_dropped_one() {
        for seed in 0..30 {
            let mut class = class_of(6);
            class.regroup(count(6, &class), &mut rng(1));
            let mut rng = rng(seed);
            let gone_before_shrinking = [
                draw_group(&mut class, &mut rng),
                draw_group(&mut class, &mut rng),
                draw_group(&mut class, &mut rng),
            ];
            keep_first(&mut class, 3);
            assert_eq!(class.oracle_remaining(Draw::Group), 3);

            class.reshuffle(&mut rng);

            class.assert_invariants();
            let survivors_gone = gone_before_shrinking
                .iter()
                .filter(|idx| idx.get() < 3)
                .count();
            assert_eq!(
                class.oracle_remaining(Draw::Group),
                3 - survivors_gone,
                "seed {seed}"
            );
            for _ in 0..9 {
                assert!(draw_group(&mut class, &mut rng).get() < 3, "seed {seed}");
            }
        }
    }

    #[test]
    fn removing_the_last_student_takes_the_group_round_with_the_grouping() {
        let mut class = class_of(1);
        class.regroup(count(1, &class), &mut rng(1));
        draw_group(&mut class, &mut rng(2));

        class.remove_student(class.students()[0].id);

        assert_eq!(class.draw(Draw::Group, &mut rng(3)), None);
        assert_eq!(class.oracle_remaining(Draw::Group), 0);
        class.assert_invariants();
    }

    /// Each center's name with the index of the group standing there, in center order.
    fn standing(class: &Class) -> Vec<(String, usize)> {
        class
            .center_views()
            .unwrap()
            .iter()
            .map(|center| (center.name.to_string(), center.group.get()))
            .collect()
    }

    fn center_names(class: &Class) -> Vec<String> {
        standing(class).into_iter().map(|(name, _)| name).collect()
    }

    fn round(class: &Class) -> usize {
        class.grouping().unwrap().centers().unwrap().round()
    }

    /// A grouped class with centers: `students` students in `groups` groups.
    fn class_with_centers(students: u32, groups: usize) -> Class {
        let mut class = class_of(students);
        class.regroup(count(groups, &class), &mut rng(1));
        class.add_centers();
        class
    }

    #[test]
    fn centers_start_one_per_group_in_group_order_at_round_one() {
        let mut class = class_of(6);
        class.regroup(count(3, &class), &mut rng(1));
        assert!(class.center_views().is_none());

        class.add_centers();

        let expected = [("Center 1", 0), ("Center 2", 1), ("Center 3", 2)];
        let expected: Vec<(String, usize)> = expected
            .into_iter()
            .map(|(name, group)| (name.to_owned(), group))
            .collect();
        assert_eq!(standing(&class), expected);
        assert_eq!(round(&class), 1);
        class.assert_invariants();
    }

    #[test]
    fn centers_need_groups_so_without_them_every_center_edit_does_nothing() {
        let mut class = class_of(3);
        let before = class.clone();

        class.add_centers();
        class.rotate_centers();
        class.remove_centers();

        assert_eq!(class, before);
        assert!(class.center_views().is_none());
        assert_eq!(
            class.rename_center(CenterIdx(0), label("x")),
            Err(MoveError::UnknownCenter)
        );
    }

    #[test]
    fn adding_centers_again_keeps_their_names_and_place_in_the_cycle() {
        let mut class = class_with_centers(6, 3);
        class
            .rename_center(CenterIdx(1), label("Reading Nook"))
            .unwrap();
        class.rotate_centers();
        let before = class.clone();

        class.add_centers();

        assert_eq!(class, before);
    }

    #[test]
    fn rotating_moves_every_group_one_center_on_and_a_full_cycle_comes_home() {
        let mut class = class_with_centers(9, 3);
        let start = standing(&class);

        class.rotate_centers();

        let groups: Vec<usize> = standing(&class)
            .into_iter()
            .map(|(_, group)| group)
            .collect();
        assert_eq!(
            groups,
            [2, 0, 1],
            "group g moved from center g to center g + 1"
        );
        assert_eq!(round(&class), 2);

        class.rotate_centers();
        class.rotate_centers();
        assert_eq!(standing(&class), start);
        assert_eq!(round(&class), 1);
        class.assert_invariants();
    }

    #[test]
    fn over_a_full_cycle_every_group_is_at_every_center_exactly_once() {
        let groups = 5;
        let mut class = class_with_centers(10, groups);
        let mut seen: Vec<Vec<String>> = vec![Vec::new(); groups];
        for _ in 0..groups {
            for (name, group) in standing(&class) {
                seen[group].push(name);
            }
            class.rotate_centers();
        }
        let every_center = center_names(&class);
        for (group, mut visited) in seen.into_iter().enumerate() {
            visited.sort();
            assert_eq!(visited, every_center, "group {group}");
        }
    }

    #[test]
    fn rotating_moves_the_groups_and_not_their_students_or_names() {
        let mut class = class_with_centers(9, 3);
        let members = member_ids(&class);
        let names = names_of(&class);

        class.rotate_centers();

        assert_eq!(member_ids(&class), members);
        assert_eq!(names_of(&class), names);
    }

    #[test]
    fn removing_centers_drops_their_names_and_turn() {
        let mut class = class_with_centers(6, 3);
        class
            .rename_center(CenterIdx(0), label("Computers"))
            .unwrap();
        class.rotate_centers();

        class.remove_centers();
        assert!(class.center_views().is_none());
        assert!(class.grouping().unwrap().centers().is_none());

        class.add_centers();
        assert_eq!(center_names(&class), ["Center 1", "Center 2", "Center 3"]);
        assert_eq!(round(&class), 1);
        class.assert_invariants();
    }

    #[test]
    fn rename_center_changes_only_that_center_and_rejects_a_stale_one() {
        let mut class = class_with_centers(6, 3);

        class
            .rename_center(CenterIdx(1), label("Art Table"))
            .unwrap();
        assert_eq!(center_names(&class), ["Center 1", "Art Table", "Center 3"]);

        let before = class.clone();
        assert_eq!(
            class.rename_center(CenterIdx(3), label("x")),
            Err(MoveError::UnknownCenter)
        );
        assert_eq!(class, before);
    }

    #[test]
    fn a_center_name_stays_at_its_place_while_the_groups_move_past() {
        let mut class = class_with_centers(6, 3);
        class
            .rename_center(CenterIdx(0), label("Computers"))
            .unwrap();
        let group_at_computers = standing(&class)[0].1;

        class.rotate_centers();

        let (name, group) = standing(&class).remove(0);
        assert_eq!(name, "Computers");
        assert_ne!(group, group_at_computers);
    }

    #[test]
    fn reshuffle_keeps_center_names_and_the_turn() {
        let mut class = class_with_centers(12, 4);
        class
            .rename_center(CenterIdx(2), label("Listening"))
            .unwrap();
        class.rotate_centers();
        class.rotate_centers();
        let standing_before = standing(&class);
        let members_before = member_ids(&class);

        class.reshuffle(&mut rng(2));

        assert_eq!(standing(&class), standing_before);
        assert_eq!(round(&class), 3);
        assert_ne!(member_ids(&class), members_before);
        class.assert_invariants();
    }

    #[test]
    fn regroup_keeps_center_names_and_restarts_at_round_one_even_at_the_same_count() {
        let mut class = class_with_centers(12, 3);
        class
            .rename_center(CenterIdx(1), label("Reading Nook"))
            .unwrap();
        class.rotate_centers();
        class.rotate_centers();
        assert_eq!(round(&class), 3);

        class.regroup(count(3, &class), &mut rng(2));

        assert_eq!(
            center_names(&class),
            ["Center 1", "Reading Nook", "Center 3"]
        );
        assert_eq!(round(&class), 1);
        let groups: Vec<usize> = standing(&class)
            .into_iter()
            .map(|(_, group)| group)
            .collect();
        assert_eq!(groups, [0, 1, 2]);
        class.assert_invariants();
    }

    #[test]
    fn regroup_fits_the_center_names_to_the_new_group_count() {
        let mut class = class_with_centers(12, 3);
        class
            .rename_center(CenterIdx(2), label("Listening"))
            .unwrap();
        class.rotate_centers();

        class.regroup(count(5, &class), &mut rng(2));
        assert_eq!(
            center_names(&class),
            ["Center 1", "Center 2", "Listening", "Center 4", "Center 5"]
        );
        assert_eq!(round(&class), 1);
        class.assert_invariants();

        class.regroup(count(2, &class), &mut rng(3));
        assert_eq!(center_names(&class), ["Center 1", "Center 2"]);
        class.assert_invariants();
    }

    #[test]
    fn regroup_does_not_invent_centers() {
        let mut class = class_of(6);
        class.regroup(count(3, &class), &mut rng(1));
        class.regroup(count(2, &class), &mut rng(2));
        assert!(class.center_views().is_none());
    }

    #[test]
    fn a_roster_shrinking_below_the_group_count_truncates_centers_in_step_with_groups() {
        let mut class = class_with_centers(6, 6);
        class
            .rename_center(CenterIdx(1), label("Art Table"))
            .unwrap();
        for _ in 0..4 {
            class.rotate_centers();
        }
        keep_first(&mut class, 3);
        assert_eq!(
            class.center_views().unwrap().len(),
            6,
            "removing students keeps the groups"
        );
        class.assert_invariants();

        class.reshuffle(&mut rng(2));

        assert_eq!(class.group_views().len(), 3);
        assert_eq!(center_names(&class), ["Center 1", "Art Table", "Center 3"]);
        let mut groups: Vec<usize> = standing(&class)
            .into_iter()
            .map(|(_, group)| group)
            .collect();
        groups.sort();
        assert_eq!(
            groups,
            [0, 1, 2],
            "each of the surviving groups still has a center"
        );
        class.assert_invariants();
    }

    #[test]
    fn removing_the_last_student_or_regrouping_an_empty_class_drops_the_centers() {
        let mut class = class_with_centers(1, 1);
        let stale = count(1, &class);
        class.remove_student(class.students()[0].id);
        assert!(class.grouping().is_none());
        assert!(class.center_views().is_none());

        class.regroup(stale, &mut rng(1));
        assert!(class.center_views().is_none());
    }

    fn roll(rng: &mut SplitMix64, n: usize) -> usize {
        rng.below(NonZeroUsize::new(n).unwrap())
    }

    #[test]
    fn invariants_survive_a_long_random_sequence_of_edits() {
        for seed in 0..30 {
            let mut rng = rng(seed);
            let mut class = class_of(8);
            for _ in 0..80 {
                let ids: Vec<StudentId> = class.students().iter().map(|s| s.id).collect();
                match roll(&mut rng, 12) {
                    0 => {
                        class.add_student(label("Newcomer"), &mut rng);
                    }
                    1 if !ids.is_empty() => {
                        class.remove_student(ids[roll(&mut rng, ids.len())]);
                    }
                    2 if !ids.is_empty() => {
                        let m = 1 + roll(&mut rng, GroupCount::max_for(ids.len()));
                        class.regroup(count(m, &class), &mut rng);
                    }
                    3 => class.reshuffle(&mut rng),
                    4 if !ids.is_empty() => {
                        let student = ids[roll(&mut rng, ids.len())];
                        let to = GroupIdx(roll(&mut rng, MAX_GROUPS));
                        class.move_student(student, to).ok();
                    }
                    5 => {
                        class.draw(Draw::Student, &mut rng);
                    }
                    6 => {
                        let idx = GroupIdx(roll(&mut rng, 4));
                        class.reroll_group_name(idx, &mut rng).ok();
                    }
                    7 => class.add_centers(),
                    8 => class.remove_centers(),
                    9 => class.rotate_centers(),
                    10 => {
                        class.draw(Draw::Group, &mut rng);
                    }
                    _ => {
                        let idx = CenterIdx(roll(&mut rng, 6));
                        class.rename_center(idx, label("Station")).ok();
                    }
                }
                class.assert_invariants();
            }
        }
    }

    #[test]
    fn parse_roster_reads_one_name_per_nonblank_line() {
        let parsed = parse_roster("Ava\nBen\n\n  Chloe  \n");
        assert_eq!(parsed.names, labels(&["Ava", "Ben", "Chloe"]));
    }

    #[test]
    fn parse_roster_tolerates_windows_line_endings() {
        let parsed = parse_roster("Ava\r\nBen\r\n\r\nChloe\r\n");
        assert_eq!(parsed.names, labels(&["Ava", "Ben", "Chloe"]));
    }

    #[test]
    fn parse_roster_keeps_only_the_first_spreadsheet_column() {
        let parsed = parse_roster("Ava\tLopez\nBen\t\tx\n\tNoFirstName\n");
        assert_eq!(parsed.names, labels(&["Ava", "Ben"]));
        assert_eq!(parsed.too_long, 0);
    }

    #[test]
    fn parse_roster_keeps_duplicate_names() {
        assert_eq!(parse_roster("Emma\nEmma").names, labels(&["Emma", "Emma"]));
    }

    #[test]
    fn parse_roster_counts_lines_too_long_to_be_names() {
        let long = "x".repeat(MAX_LABEL_CHARS + 1);
        let parsed = parse_roster(&format!("Ava\n{long}\nBen"));
        assert_eq!(parsed.names, labels(&["Ava", "Ben"]));
        assert_eq!(parsed.too_long, 1);
    }

    #[test]
    fn parse_roster_of_nothing_is_empty() {
        assert_eq!(parse_roster(""), ParsedRoster::default());
        assert_eq!(parse_roster(" \n\t\n"), ParsedRoster::default());
    }

    #[test]
    fn reconcile_gives_a_duplicated_student_id_a_fresh_id_and_a_group() {
        let mut class = class_of(4);
        class.regroup(count(2, &class), &mut rng(3));
        let first = class.students[0].id;
        class.students[3].id = first;
        class.reconcile();
        assert_eq!(class.students().len(), 4);
        assert_eq!(class.students()[0].id, first);
        assert_ne!(class.students()[3].id, first);
        class.assert_invariants();
    }
}
