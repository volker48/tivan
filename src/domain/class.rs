use serde::{Deserialize, Serialize};

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
}

/// The oracle's answer, resolved to a name so the UI never looks it up after the fact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    pub id: StudentId,
    pub name: Label,
}

/// One group with its members resolved, in roster order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupView<'a> {
    pub idx: GroupIdx,
    pub name: &'a Label,
    pub members: Vec<&'a Student>,
}

/// Owns the roster, the grouping, and the oracle round for one class. Roster edits go through this
/// type so the three stay consistent within a single call.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Class {
    id: ClassId,
    name: Label,
    students: Vec<Student>,
    next_student: u32,
    grouping: Option<Grouping>,
    oracle: Oracle,
}

impl Class {
    pub(super) fn new(id: ClassId, name: Label, students: Vec<Label>) -> Class {
        let mut class = Class {
            id,
            name,
            students: Vec::with_capacity(students.len()),
            next_student: 0,
            grouping: None,
            oracle: Oracle::default(),
        };
        for student in students {
            class.enroll(student);
        }
        class
    }

    pub fn id(&self) -> ClassId {
        self.id
    }

    pub fn name(&self) -> &Label {
        &self.name
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

    /// A student added after grouping joins the smallest group.
    pub fn add_student(&mut self, name: Label) -> StudentId {
        let id = self.enroll(name);
        self.reconcile();
        id
    }

    /// Idempotent. Removes the student from the roster, the grouping, and the oracle round.
    pub fn remove_student(&mut self, id: StudentId) {
        self.students.retain(|s| s.id != id);
        self.reconcile();
    }

    /// Fresh random split with fresh names, replacing any existing grouping. A `count` made for
    /// an earlier, larger roster is clamped to the current one.
    pub fn regroup(&mut self, count: GroupCount, rng: &mut impl Rng) {
        self.rebuild(count.get(), rng);
    }

    /// Keeps the group count (clamped to the roster), rerolls members and names. Does nothing
    /// when there is no grouping yet.
    pub fn reshuffle(&mut self, rng: &mut impl Rng) {
        if let Some(grouping) = &self.grouping {
            self.rebuild(grouping.len(), rng);
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

    /// `None` only when the class has no students.
    pub fn draw(&mut self, rng: &mut impl Rng) -> Option<Pick> {
        let id = self.oracle.draw(&self.students, rng)?;
        let student = self.student(id).expect("the oracle draws from the roster");
        Some(Pick {
            id,
            name: student.name.clone(),
        })
    }

    /// Students the oracle has not chosen yet this round.
    pub fn oracle_remaining(&self) -> usize {
        self.oracle.remaining(&self.students)
    }

    fn enroll(&mut self, name: Label) -> StudentId {
        let id = StudentId(self.next_student);
        self.next_student += 1;
        self.students.push(Student { id, name });
        id
    }

    fn grouping_mut(&mut self) -> Result<&mut Grouping, MoveError> {
        self.grouping.as_mut().ok_or(MoveError::UnknownGroup)
    }

    fn rebuild(&mut self, wanted: usize, rng: &mut impl Rng) {
        let n = self.students.len();
        self.grouping = (n > 0).then(|| {
            let m = wanted.clamp(1, GroupCount::max_for(n));
            let count = GroupCount::new(m, n).expect("m is clamped into 1..=max_for(n)");
            Grouping::split(&self.students, count, rng)
        });
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
        self.oracle.retain_roster(&self.students);
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
        self.oracle.assert_invariants(&self.students);
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
    use std::collections::HashSet;
    use std::num::NonZeroUsize;

    use super::*;
    use crate::domain::label::MAX_LABEL_CHARS;
    use crate::domain::names::MAX_GROUPS;
    use crate::domain::rng::SplitMix64;
    use crate::domain::testing::{label, labels, numbered};

    fn class_of(n: u32) -> Class {
        Class::new(ClassId(0), label("Room 1"), numbered(n))
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

    #[test]
    fn a_new_class_numbers_students_in_order_with_no_grouping() {
        let class = Class::new(ClassId(7), label("Art"), labels(&["Ava", "Ben", "Ava"]));
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
        let newcomer = class.add_student(label("New"));
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
        let newcomer = class.add_student(label("New"));
        assert!(class.students()[..3].iter().all(|s| s.id != newcomer));
        class.assert_invariants();
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
    fn reshuffle_keeps_the_group_count_but_changes_members_and_names() {
        let mut class = class_of(12);
        class.regroup(count(4, &class), &mut rng(1));
        let before_names = names_of(&class);
        let before_members = member_ids(&class);

        class.reshuffle(&mut rng(2));

        assert_eq!(group_sizes(&class), [3, 3, 3, 3]);
        assert_ne!(names_of(&class), before_names);
        assert_ne!(member_ids(&class), before_members);
        class.assert_invariants();
    }

    #[test]
    fn reshuffle_clamps_to_a_roster_that_shrank_below_the_group_count() {
        let mut class = class_of(6);
        class.regroup(count(6, &class), &mut rng(1));
        keep_first(&mut class, 3);
        assert_eq!(class.group_views().len(), 6);

        class.reshuffle(&mut rng(2));

        assert_eq!(group_sizes(&class), [1, 1, 1]);
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

        let id = class.add_student(label("Late"));

        assert_eq!(class.grouping().unwrap().group_of(id), Some(GroupIdx(1)));
        assert_eq!(group_sizes(&class), [3, 3]);
        class.assert_invariants();
    }

    #[test]
    fn removing_a_student_clears_them_from_roster_grouping_and_oracle() {
        let mut class = class_of(6);
        class.regroup(count(2, &class), &mut rng(1));
        let drawn = class.draw(&mut rng(3)).unwrap();
        assert_eq!(class.oracle_remaining(), 5);

        class.remove_student(drawn.id);

        assert!(class.student(drawn.id).is_none());
        assert_eq!(class.grouping().unwrap().group_of(drawn.id), None);
        assert!(
            class
                .group_views()
                .iter()
                .all(|g| g.members.iter().all(|s| s.id != drawn.id))
        );
        assert_eq!(class.oracle_remaining(), 5);
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
            let pick = class.draw(&mut rng).unwrap();
            assert_eq!(class.student(pick.id).unwrap().name, pick.name);
            assert!(chosen.insert(pick.id));
            assert_eq!(class.oracle_remaining(), left);
        }
        let next = class.draw(&mut rng).unwrap();
        assert_eq!(class.oracle_remaining(), 4);
        assert!(class.student(next.id).is_some());
    }

    #[test]
    fn the_oracle_has_no_answer_for_an_empty_class() {
        let mut class = class_of(0);
        assert_eq!(class.draw(&mut rng(1)), None);
        assert_eq!(class.oracle_remaining(), 0);
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
                match roll(&mut rng, 7) {
                    0 => {
                        class.add_student(label("Newcomer"));
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
                        class.draw(&mut rng);
                    }
                    _ => {
                        let idx = GroupIdx(roll(&mut rng, 4));
                        class.reroll_group_name(idx, &mut rng).ok();
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
