use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::attendance::{Absences, Day};
use super::avatar::{self, Avatar};
use super::centers::CenterIdx;
use super::classroom::{ClassroomCourseId, ClassroomStudent, ClassroomUserId, SyncReport};
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

    fn of_group(view: &GroupView<'_>) -> Pick {
        Pick::Group {
            idx: view.idx,
            name: view.name.clone(),
            members: view.members.iter().map(|&s| s.clone()).collect(),
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

/// Owns the roster, the grouping, the student oracle round and today's absences for one class.
/// Roster edits go through this type so the four stay consistent within a single call. The group
/// round lives in the grouping.
///
/// An absent student keeps their place in the grouping, so they are back in their group once the
/// absence lapses, but group views, splits and student draws see only the students present.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    absent: Option<Absences>,
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
            absent: None,
        };
        for student in students {
            class.enroll(student, None, rng);
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

    pub fn is_absent(&self, id: StudentId) -> bool {
        self.absent
            .as_ref()
            .is_some_and(|absences| absences.contains(id))
    }

    /// Students not marked absent, in roster order.
    pub fn present(&self) -> impl Iterator<Item = &Student> {
        self.students.iter().filter(|s| !self.is_absent(s.id))
    }

    /// Students marked absent, in roster order.
    pub fn absent(&self) -> impl Iterator<Item = &Student> {
        self.students.iter().filter(|s| self.is_absent(s.id))
    }

    /// Marks the student away for `today`. Absences left over from an earlier day lapse first, so
    /// they never come back alongside today's. The student keeps their group, and leaves the
    /// oracle round so a turn they missed is still theirs when they are back. Does nothing for an
    /// id not on the roster.
    pub fn mark_absent(&mut self, id: StudentId, today: Day) {
        if self.student(id).is_none() {
            return;
        }
        self.start_day(today);
        match &mut self.absent {
            Some(absences) => absences.insert(id),
            None => self.absent = Some(Absences::new(today, id)),
        }
        self.oracle.forget(id);
    }

    /// Idempotent.
    pub fn mark_present(&mut self, id: StudentId) {
        if let Some(absences) = &mut self.absent {
            absences.remove(id);
            if absences.is_empty() {
                self.absent = None;
            }
        }
    }

    /// Whether some absence was marked on a day before `today`.
    pub(super) fn absences_lapsed(&self, today: Day) -> bool {
        self.absent
            .as_ref()
            .is_some_and(|absences| absences.lapsed(today))
    }

    /// Drops absences marked on a day before `today`, so everyone is back.
    pub(super) fn start_day(&mut self, today: Day) {
        if self.absences_lapsed(today) {
            self.absent = None;
        }
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
        let id = self.enroll(name, None, rng);
        self.reconcile();
        id
    }

    /// Only a sync that makes the class links it, so a class never moves to another course.
    pub(super) fn link_course(&mut self, course: ClassroomCourseId) {
        self.classroom_course_id = Some(course);
    }

    /// Makes the linked part of the roster match `incoming`. A linked student who is not in it is
    /// removed, and a Classroom student with no counterpart is added after everyone else, in the
    /// order given, and joins the smallest group like any student added later. Students added by
    /// hand are left alone, and so is the name of a student already linked. Uses no randomness
    /// when nothing changes. Adds the students it added and removed to `report`.
    pub(super) fn sync_roster(
        &mut self,
        incoming: &[ClassroomStudent],
        report: &mut SyncReport,
        rng: &mut impl Rng,
    ) {
        let wanted: BTreeSet<&ClassroomUserId> =
            incoming.iter().map(ClassroomStudent::id).collect();
        let before = self.students.len();
        self.students.retain(|s| {
            s.classroom_user_id
                .as_ref()
                .is_none_or(|id| wanted.contains(id))
        });
        report.students_removed += before - self.students.len();

        let known: BTreeSet<ClassroomUserId> = self
            .students
            .iter()
            .filter_map(|s| s.classroom_user_id.clone())
            .collect();
        for student in incoming.iter().filter(|s| !known.contains(s.id())) {
            self.enroll(student.name().clone(), Some(student.id().clone()), rng);
            report.students_added += 1;
        }
        self.reconcile();
    }

    /// Idempotent. Removes the student from the roster, the grouping, and the oracle round.
    pub fn remove_student(&mut self, id: StudentId) {
        self.students.retain(|s| s.id != id);
        self.reconcile();
    }

    /// Fresh random split of the students present, with fresh names, replacing any existing
    /// grouping. A `count` made for an earlier, larger roster is clamped to the students present.
    /// Absent students then join the smallest groups, so the groups stay even when they are back.
    /// Centers carry over with their names, fitted to the new group count, and start again at
    /// round 1. Does nothing when no one is present.
    pub fn regroup(&mut self, count: GroupCount, rng: &mut impl Rng) {
        let present: Vec<Student> = self.present().cloned().collect();
        let Ok(count) = GroupCount::nearest(count.get(), present.len()) else {
            return;
        };
        let mut centers = self.grouping.as_mut().and_then(Grouping::take_centers);
        if let Some(centers) = &mut centers {
            centers.restart();
        }
        let mut grouping = Grouping::split(&present, count, rng);
        grouping.set_centers(centers);
        grouping.reconcile(&self.students);
        self.grouping = Some(grouping);
    }

    /// Re-deals the students present across the existing groups, keeping the group names and the
    /// centers. Absent students then join the smallest groups, as in [`Class::regroup`]. Does
    /// nothing when there is no grouping yet or no one is present.
    pub fn reshuffle(&mut self, rng: &mut impl Rng) {
        let present: Vec<Student> = self.present().cloned().collect();
        if let Some(grouping) = &mut self.grouping {
            grouping.reshuffle(&present, rng);
            grouping.reconcile(&self.students);
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

    /// Groups with their present members resolved, empty when there is no grouping.
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
        for student in self.present() {
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

    /// `None` when no student is present, or when `kind` is [`Draw::Group`] and the class has no
    /// grouping. A student draw picks only students present.
    pub fn draw(&mut self, kind: Draw, rng: &mut impl Rng) -> Option<Pick> {
        match kind {
            Draw::Student => {
                let id = self.oracle.draw(&self.present_ids(), rng)?;
                let student = self.student(id).expect("the oracle draws from the roster");
                Some(Pick::Student(student.clone()))
            }
            Draw::Group => {
                let idx = self.grouping.as_mut()?.draw(rng)?;
                Some(Pick::of_group(&self.group_views()[idx.get()]))
            }
        }
    }

    /// Everything [`Class::draw`] could answer for `kind`, in roster or group order. It records
    /// nothing, so the UI can flash these before the real pick lands without spending a draw.
    pub fn candidates(&self, kind: Draw) -> Vec<Pick> {
        match kind {
            Draw::Student => self.present().cloned().map(Pick::Student).collect(),
            Draw::Group => self.group_views().iter().map(Pick::of_group).collect(),
        }
    }

    /// Students present or groups the oracle has not chosen yet this round. Zero for groups when
    /// the class has no grouping.
    pub fn oracle_remaining(&self, kind: Draw) -> usize {
        match kind {
            Draw::Student => self.oracle.remaining(&self.present_ids()),
            Draw::Group => self.grouping.as_ref().map_or(0, Grouping::remaining),
        }
    }

    fn roster_ids(&self) -> Vec<StudentId> {
        self.students.iter().map(|s| s.id).collect()
    }

    fn present_ids(&self) -> Vec<StudentId> {
        self.present().map(|s| s.id).collect()
    }

    /// The avatar is per student, not per name, so two children called Emma get different faces.
    fn enroll(
        &mut self,
        name: Label,
        classroom_user_id: Option<ClassroomUserId>,
        rng: &mut impl Rng,
    ) -> StudentId {
        let id = StudentId(self.next_student);
        self.next_student += 1;
        let avatar = avatar::pick(self.students.iter().map(|s| s.avatar), rng);
        self.students.push(Student {
            id,
            name,
            avatar,
            classroom_user_id,
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
        if let Some(absences) = &mut self.absent {
            let students = &self.students;
            absences.retain(|id| students.iter().any(|s| s.id == id));
            if absences.is_empty() {
                self.absent = None;
            }
        }
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
        if let Some(absences) = &self.absent {
            assert!(!absences.is_empty(), "an empty absence list is kept");
            assert_eq!(
                self.absent().count(),
                absences.len(),
                "an absent id is not on the roster"
            );
        }
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
    fn candidates_list_every_student_in_roster_order_and_every_group_in_group_order() {
        let mut class = class_of(7);
        let students: Vec<Pick> = class
            .students()
            .iter()
            .cloned()
            .map(Pick::Student)
            .collect();
        assert_eq!(class.candidates(Draw::Student), students);
        assert_eq!(class.candidates(Draw::Group), []);

        class.regroup(count(3, &class), &mut rng(1));
        let groups = class.candidates(Draw::Group);
        let views = class.group_views();
        assert_eq!(groups.len(), views.len());
        for (pick, view) in groups.iter().zip(&views) {
            let Pick::Group { idx, name, members } = pick else {
                panic!("expected a group, got {pick:?}");
            };
            assert_eq!((*idx, name), (view.idx, view.name));
            assert_eq!(members.iter().collect::<Vec<_>>(), view.members);
        }
    }

    #[test]
    fn an_empty_class_has_no_candidates() {
        let class = class_of(0);
        for kind in [Draw::Student, Draw::Group] {
            assert_eq!(class.candidates(kind), []);
        }
    }

    #[test]
    fn every_draw_is_one_of_the_candidates() {
        for seed in 0..20 {
            let mut rng = rng(seed);
            let mut class = class_of(9);
            class.regroup(count(4, &class), &mut rng);
            for kind in [Draw::Student, Draw::Group] {
                for _ in 0..12 {
                    let pick = class.draw(kind, &mut rng).unwrap();
                    assert!(class.candidates(kind).contains(&pick), "seed {seed}");
                }
            }
        }
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

    fn day(day: u8) -> Day {
        Day {
            year: 2026,
            month: 10,
            day,
        }
    }

    fn present_ids(class: &Class) -> Vec<StudentId> {
        class.present().map(|s| s.id).collect()
    }

    fn absent_ids(class: &Class) -> Vec<StudentId> {
        class.absent().map(|s| s.id).collect()
    }

    #[test]
    fn an_absent_student_leaves_the_group_views_but_keeps_their_group() {
        let mut class = class_of(9);
        class.regroup(count(3, &class), &mut rng(1));
        let away = class.students()[4].id;
        let home = class.grouping().unwrap().group_of(away).unwrap();

        class.mark_absent(away, day(24));

        assert!(class.is_absent(away));
        assert_eq!(absent_ids(&class), [away]);
        assert!(!member_ids(&class).concat().contains(&away));
        assert_eq!(class.group_views()[home.get()].members.len(), 2);
        assert_eq!(class.grouping().unwrap().group_of(away), Some(home));
        class.assert_invariants();

        class.mark_present(away);

        assert!(!class.is_absent(away));
        assert!(member_ids(&class)[home.get()].contains(&away));
        assert_eq!(class.absent, None);
        class.assert_invariants();
    }

    #[test]
    fn the_oracle_never_picks_an_absent_student_or_counts_them_as_waiting() {
        let mut class = class_of(4);
        let away = class.students()[1].id;
        class.mark_absent(away, day(24));
        assert_eq!(class.oracle_remaining(Draw::Student), 3);
        assert_eq!(class.candidates(Draw::Student).len(), 3);

        let mut rng = rng(2);
        for _ in 0..30 {
            assert_ne!(draw_student(&mut class, &mut rng).id, away);
        }
        class.assert_invariants();
    }

    #[test]
    fn a_student_marked_absent_after_their_pick_gets_their_turn_back() {
        let mut class = class_of(5);
        let mut rng = rng(3);
        let picked = draw_student(&mut class, &mut rng).id;
        assert_eq!(class.oracle_remaining(Draw::Student), 4);

        class.mark_absent(picked, day(24));
        assert_eq!(class.oracle_remaining(Draw::Student), 4);
        class.mark_present(picked);

        assert_eq!(class.oracle_remaining(Draw::Student), 5);
        class.assert_invariants();
    }

    #[test]
    fn with_everyone_absent_there_is_no_one_to_pick() {
        let mut class = class_of(2);
        for id in class.roster_ids() {
            class.mark_absent(id, day(24));
        }
        assert_eq!(class.draw(Draw::Student, &mut rng(1)), None);
        assert_eq!(class.oracle_remaining(Draw::Student), 0);
        assert!(class.candidates(Draw::Student).is_empty());
    }

    #[test]
    fn making_groups_splits_the_students_present_evenly_and_files_the_absent_with_the_smallest() {
        for seed in 0..20 {
            let mut class = class_of(10);
            let away = [class.students()[0].id, class.students()[1].id];
            for id in away {
                class.mark_absent(id, day(24));
            }

            class.regroup(count(3, &class), &mut rng(seed));

            assert_eq!(group_sizes(&class), [3, 3, 2], "seed {seed}");
            class.assert_invariants();
            class.mark_present(away[0]);
            class.mark_present(away[1]);
            let mut sizes = group_sizes(&class);
            sizes.sort_unstable();
            assert_eq!(sizes, [3, 3, 4], "seed {seed}");
        }
    }

    #[test]
    fn making_groups_clamps_the_count_to_the_students_present() {
        let mut class = class_of(4);
        let wanted = count(4, &class);
        class.mark_absent(class.students()[0].id, day(24));
        class.regroup(wanted, &mut rng(1));
        assert_eq!(group_sizes(&class), [1, 1, 1]);
        class.assert_invariants();
    }

    #[test]
    fn making_groups_with_everyone_absent_changes_nothing() {
        let mut class = class_of(3);
        class.regroup(count(2, &class), &mut rng(1));
        for id in class.roster_ids() {
            class.mark_absent(id, day(24));
        }
        let before = class.clone();

        class.regroup(count(2, &class), &mut rng(2));
        class.reshuffle(&mut rng(3));

        assert_eq!(class, before);
    }

    #[test]
    fn shuffling_deals_the_students_present_evenly_and_keeps_the_absent_in_a_group() {
        for seed in 0..20 {
            let mut class = class_of(12);
            class.regroup(count(4, &class), &mut rng(1));
            let away = class.students()[5].id;
            class.mark_absent(away, day(24));

            class.reshuffle(&mut rng(seed));

            let mut sizes = group_sizes(&class);
            sizes.sort_unstable();
            assert_eq!(sizes, [2, 3, 3, 3], "seed {seed}");
            assert!(class.grouping().unwrap().group_of(away).is_some());
            class.assert_invariants();
        }
    }

    #[test]
    fn absences_lapse_on_a_later_day_and_hold_on_their_own() {
        let mut class = class_of(3);
        let away = class.students()[0].id;
        class.mark_absent(away, day(24));

        class.start_day(day(24));
        assert!(class.is_absent(away));
        assert!(!class.absences_lapsed(day(24)));

        assert!(class.absences_lapsed(day(25)));
        class.start_day(day(25));
        assert!(!class.is_absent(away));
        assert_eq!(present_ids(&class).len(), 3);
        class.assert_invariants();
    }

    #[test]
    fn marking_someone_absent_on_a_new_day_brings_back_yesterdays_absences() {
        let mut class = class_of(3);
        let [monday, tuesday] = [class.students()[0].id, class.students()[1].id];
        class.mark_absent(monday, day(26));

        class.mark_absent(tuesday, day(27));

        assert_eq!(absent_ids(&class), [tuesday]);
        assert!(!class.absences_lapsed(day(27)));
    }

    #[test]
    fn marking_absent_ignores_unknown_ids_and_marking_present_is_idempotent() {
        let mut class = class_of(2);
        class.mark_absent(StudentId(99), day(24));
        assert_eq!(class.absent, None);

        let away = class.students()[0].id;
        class.mark_absent(away, day(24));
        class.mark_absent(away, day(24));
        assert_eq!(absent_ids(&class), [away]);
        class.mark_present(away);
        class.mark_present(away);
        assert_eq!(class.absent, None);
        class.assert_invariants();
    }

    #[test]
    fn removing_an_absent_student_drops_their_absence() {
        let mut class = class_of(3);
        let [away, also] = [class.students()[0].id, class.students()[1].id];
        class.mark_absent(away, day(24));
        class.mark_absent(also, day(24));

        class.remove_student(away);
        assert_eq!(absent_ids(&class), [also]);
        class.remove_student(also);
        assert_eq!(class.absent, None);
        class.assert_invariants();
    }
}
