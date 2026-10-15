//! Splitting a roster into named groups. Membership is a map from student to group index, so a
//! student cannot be in two groups by construction. The roster lives on `Class`; `Class` calls
//! [`Grouping::reconcile`] after every roster edit so the map covers exactly the roster.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;

use serde::{Deserialize, Serialize};

use super::class::{Student, StudentId};
use super::label::Label;
use super::names::{MAX_GROUPS, draw_name, draw_names};
use super::rng::{Rng, shuffle};

/// A requested number of groups, `1..=min(student_count, MAX_GROUPS)`. The UI stepper is bounded
/// by [`GroupCount::max_for`], so `new` failing means a stale control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupCount(NonZeroUsize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupCountError {
    NoStudents,
    OutOfRange { max: usize },
}

impl GroupCount {
    pub fn new(m: usize, student_count: usize) -> Result<GroupCount, GroupCountError> {
        if student_count == 0 {
            return Err(GroupCountError::NoStudents);
        }
        let max = GroupCount::max_for(student_count);
        NonZeroUsize::new(m)
            .filter(|m| m.get() <= max)
            .map(GroupCount)
            .ok_or(GroupCountError::OutOfRange { max })
    }

    pub fn max_for(student_count: usize) -> usize {
        student_count.min(MAX_GROUPS)
    }

    /// The valid count closest to `wanted`, for a control whose value outlived a roster edit.
    pub fn nearest(wanted: usize, student_count: usize) -> Result<GroupCount, GroupCountError> {
        let max = GroupCount::max_for(student_count);
        GroupCount::new(wanted.clamp(1, max.max(1)), student_count)
    }

    pub fn get(self) -> usize {
        self.0.get()
    }
}

/// Position of a group in its `Grouping`. Groups are never inserted or removed individually, so
/// an index stays valid until the next split.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GroupIdx(pub(super) usize);

impl GroupIdx {
    pub fn get(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveError {
    UnknownStudent,
    UnknownGroup,
}

/// `names` is never empty and has at most `MAX_GROUPS` entries (the owning `Class` drops an empty
/// grouping), and `of` maps every roster student to an index below `names.len()`.
/// Balance is a property of [`Grouping::split`], not of this type, because manual moves break it.
/// Group names are distinct when generated; a teacher may rename two groups to the same text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grouping {
    names: Vec<Label>,
    of: BTreeMap<StudentId, GroupIdx>,
}

/// Sizes of `m` groups over `n` students: the first `n % m` groups get one extra member.
pub fn group_sizes(n: usize, m: GroupCount) -> Vec<usize> {
    let m = m.get();
    let (base, extra) = (n / m, n % m);
    (0..m).map(|i| base + usize::from(i < extra)).collect()
}

impl Grouping {
    /// Shuffles `students`, cuts them by [`group_sizes`], and draws fresh unique names.
    pub fn split(students: &[Student], count: GroupCount, rng: &mut impl Rng) -> Grouping {
        let mut ids: Vec<StudentId> = students.iter().map(|s| s.id).collect();
        shuffle(&mut ids, rng);
        let names = draw_names(count.get(), rng);

        let mut shuffled = ids.into_iter();
        let mut of = BTreeMap::new();
        for (group, size) in group_sizes(students.len(), count).into_iter().enumerate() {
            for id in shuffled.by_ref().take(size) {
                of.insert(id, GroupIdx(group));
            }
        }
        Grouping { names, of }
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn names(&self) -> &[Label] {
        &self.names
    }

    pub fn group_of(&self, id: StudentId) -> Option<GroupIdx> {
        self.of.get(&id).copied()
    }

    /// Moving a student into the group they are already in changes nothing.
    pub fn move_member(&mut self, id: StudentId, to: GroupIdx) -> Result<(), MoveError> {
        self.check(to)?;
        let group = self.of.get_mut(&id).ok_or(MoveError::UnknownStudent)?;
        *group = to;
        Ok(())
    }

    pub fn rename(&mut self, idx: GroupIdx, name: Label) -> Result<(), MoveError> {
        self.check(idx)?;
        self.names[idx.0] = name;
        Ok(())
    }

    /// Draws a name that differs from every current name, the old one included.
    pub fn reroll_name(&mut self, idx: GroupIdx, rng: &mut impl Rng) -> Result<(), MoveError> {
        self.check(idx)?;
        let used: Vec<&Label> = self.names.iter().collect();
        let fresh = draw_name(&used, rng);
        self.names[idx.0] = fresh;
        Ok(())
    }

    fn check(&self, idx: GroupIdx) -> Result<(), MoveError> {
        if idx.0 < self.names.len() {
            Ok(())
        } else {
            Err(MoveError::UnknownGroup)
        }
    }

    /// Makes the map cover exactly `roster`. Drops ids not on the roster and indexes with no
    /// group, then places each unassigned student, in roster order, into the currently smallest
    /// group (lowest index on ties). Idempotent.
    pub(super) fn reconcile(&mut self, roster: &[Student]) {
        // Only a hand-edited blob can exceed the cap, but name generation relies on it.
        self.names.truncate(MAX_GROUPS);
        let group_count = self.names.len();
        let on_roster: BTreeSet<StudentId> = roster.iter().map(|s| s.id).collect();
        self.of
            .retain(|id, group| on_roster.contains(id) && group.0 < group_count);

        let mut sizes = vec![0usize; group_count];
        for group in self.of.values() {
            sizes[group.0] += 1;
        }
        for student in roster {
            if self.of.contains_key(&student.id) {
                continue;
            }
            let Some((smallest, _)) = sizes.iter().enumerate().min_by_key(|&(_, &size)| size)
            else {
                return;
            };
            sizes[smallest] += 1;
            self.of.insert(student.id, GroupIdx(smallest));
        }
    }

    #[cfg(test)]
    pub(super) fn assert_invariants(&self, roster: &[Student]) {
        assert!(!self.names.is_empty() && self.names.len() <= MAX_GROUPS);
        let on_roster: BTreeSet<StudentId> = roster.iter().map(|s| s.id).collect();
        let mapped: BTreeSet<StudentId> = self.of.keys().copied().collect();
        assert_eq!(
            mapped, on_roster,
            "membership must cover exactly the roster"
        );
        assert!(self.of.values().all(|g| g.0 < self.names.len()));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::domain::rng::SplitMix64;
    use crate::domain::testing::{label, roster};

    fn count(m: usize, n: usize) -> GroupCount {
        GroupCount::new(m, n).unwrap()
    }

    fn sizes(grouping: &Grouping) -> Vec<usize> {
        (0..grouping.len())
            .map(|g| grouping.of.values().filter(|idx| idx.0 == g).count())
            .collect()
    }

    #[test]
    fn group_sizes_gives_the_first_remainder_groups_one_extra() {
        let cases: [(usize, usize, &[usize]); 7] = [
            (7, 3, &[3, 2, 2]),
            (6, 3, &[2, 2, 2]),
            (5, 5, &[1, 1, 1, 1, 1]),
            (5, 1, &[5]),
            (22, 4, &[6, 6, 5, 5]),
            (1, 1, &[1]),
            (3, 2, &[2, 1]),
        ];
        for (n, m, expected) in cases {
            assert_eq!(group_sizes(n, count(m, n)), expected, "n={n} m={m}");
        }
    }

    #[test]
    fn group_sizes_always_sum_to_n_and_differ_by_at_most_one() {
        for n in 1..=50 {
            for m in 1..=GroupCount::max_for(n) {
                let sizes = group_sizes(n, count(m, n));
                assert_eq!(sizes.len(), m);
                assert_eq!(sizes.iter().sum::<usize>(), n);
                assert!(sizes.windows(2).all(|w| w[0] >= w[1] && w[0] - w[1] <= 1));
            }
        }
    }

    #[test]
    fn group_count_accepts_one_through_the_cap() {
        assert_eq!(count(1, 10).get(), 1);
        assert_eq!(count(10, 10).get(), 10);
        assert_eq!(count(MAX_GROUPS, 100).get(), MAX_GROUPS);
    }

    #[test]
    fn group_count_rejects_zero_too_many_and_empty_rosters() {
        assert_eq!(
            GroupCount::new(0, 10),
            Err(GroupCountError::OutOfRange { max: 10 })
        );
        assert_eq!(
            GroupCount::new(11, 10),
            Err(GroupCountError::OutOfRange { max: 10 })
        );
        assert_eq!(
            GroupCount::new(MAX_GROUPS + 1, 100),
            Err(GroupCountError::OutOfRange { max: MAX_GROUPS })
        );
        assert_eq!(GroupCount::new(1, 0), Err(GroupCountError::NoStudents));
        assert_eq!(GroupCount::max_for(0), 0);
        assert_eq!(GroupCount::max_for(25), 25);
        assert_eq!(GroupCount::max_for(500), MAX_GROUPS);
    }

    #[test]
    fn nearest_pulls_a_stale_count_into_range() {
        assert_eq!(GroupCount::nearest(0, 10).unwrap().get(), 1);
        assert_eq!(GroupCount::nearest(6, 10).unwrap().get(), 6);
        assert_eq!(GroupCount::nearest(12, 10).unwrap().get(), 10);
        assert_eq!(GroupCount::nearest(99, 500).unwrap().get(), MAX_GROUPS);
        assert_eq!(GroupCount::nearest(3, 0), Err(GroupCountError::NoStudents));
    }

    #[test]
    fn split_places_every_student_in_exactly_one_balanced_group() {
        for seed in 0..40 {
            for n in [1, 2, 7, 22, 30] {
                for m in [1, 2, 3, n.min(MAX_GROUPS)].into_iter().filter(|&m| m <= n) {
                    let students = roster(n as u32);
                    let grouping =
                        Grouping::split(&students, count(m, n), &mut SplitMix64::seeded(seed));
                    grouping.assert_invariants(&students);
                    assert_eq!(grouping.len(), m);
                    assert_eq!(sizes(&grouping), group_sizes(n, count(m, n)));
                    let unique: HashSet<_> = grouping.names().iter().collect();
                    assert_eq!(unique.len(), m, "names must be unique");
                }
            }
        }
    }

    #[test]
    fn split_is_deterministic_for_a_seed_and_varies_across_seeds() {
        let students = roster(12);
        let split = |seed| Grouping::split(&students, count(3, 12), &mut SplitMix64::seeded(seed));
        assert_eq!(split(7), split(7));
        let distinct: HashSet<Vec<Option<usize>>> = (0..20)
            .map(|seed| {
                let g = split(seed);
                students
                    .iter()
                    .map(|s| g.group_of(s.id).map(GroupIdx::get))
                    .collect()
            })
            .collect();
        assert!(
            distinct.len() > 10,
            "seeds should produce different assignments"
        );
    }

    #[test]
    fn split_gives_each_student_a_chance_at_every_group() {
        let students = roster(9);
        let first = students[0].id;
        let landed: HashSet<usize> = (0..60)
            .map(|seed| {
                Grouping::split(&students, count(3, 9), &mut SplitMix64::seeded(seed))
                    .group_of(first)
                    .unwrap()
                    .get()
            })
            .collect();
        assert_eq!(landed, HashSet::from([0, 1, 2]));
    }

    fn two_groups() -> (Vec<Student>, Grouping) {
        let students = roster(5);
        let grouping = Grouping::split(&students, count(2, 5), &mut SplitMix64::seeded(3));
        (students, grouping)
    }

    #[test]
    fn move_member_changes_only_that_students_group() {
        let (students, mut grouping) = two_groups();
        let mover = students[0].id;
        let target = GroupIdx(1 - grouping.group_of(mover).unwrap().get());
        let before = grouping.clone();

        grouping.move_member(mover, target).unwrap();

        assert_eq!(grouping.group_of(mover), Some(target));
        for other in students.iter().filter(|s| s.id != mover) {
            assert_eq!(grouping.group_of(other.id), before.group_of(other.id));
        }
        grouping.assert_invariants(&students);
    }

    #[test]
    fn move_member_is_idempotent() {
        let (students, mut grouping) = two_groups();
        let mover = students[1].id;
        let home = grouping.group_of(mover).unwrap();
        let before = grouping.clone();
        grouping.move_member(mover, home).unwrap();
        assert_eq!(grouping, before);

        grouping
            .move_member(mover, GroupIdx(1 - home.get()))
            .unwrap();
        let moved = grouping.clone();
        grouping
            .move_member(mover, GroupIdx(1 - home.get()))
            .unwrap();
        assert_eq!(grouping, moved);
    }

    #[test]
    fn move_member_rejects_unknown_students_and_groups_without_changing_anything() {
        let (students, mut grouping) = two_groups();
        let before = grouping.clone();
        assert_eq!(
            grouping.move_member(students[0].id, GroupIdx(2)),
            Err(MoveError::UnknownGroup)
        );
        assert_eq!(
            grouping.move_member(StudentId(99), GroupIdx(0)),
            Err(MoveError::UnknownStudent)
        );
        assert_eq!(grouping, before);
    }

    #[test]
    fn rename_replaces_one_name_and_allows_a_duplicate() {
        let (_, mut grouping) = two_groups();
        let other = grouping.names()[1].clone();
        grouping.rename(GroupIdx(0), label("Table Four")).unwrap();
        assert_eq!(grouping.names()[0].as_str(), "Table Four");
        assert_eq!(grouping.names()[1], other);

        grouping.rename(GroupIdx(0), other.clone()).unwrap();
        assert_eq!(grouping.names(), [other.clone(), other]);
        assert_eq!(
            grouping.rename(GroupIdx(2), label("x")),
            Err(MoveError::UnknownGroup)
        );
    }

    #[test]
    fn reroll_name_always_picks_a_name_no_group_has() {
        let students = roster(30);
        for seed in 0..40 {
            let mut rng = SplitMix64::seeded(seed);
            let mut grouping = Grouping::split(&students, count(30, 30), &mut rng);
            let before = grouping.clone();
            grouping.reroll_name(GroupIdx(4), &mut rng).unwrap();

            let fresh = &grouping.names()[4];
            assert!(
                !before.names().contains(fresh),
                "seed {seed} reused {fresh}"
            );
            for i in (0..30).filter(|&i| i != 4) {
                assert_eq!(grouping.names()[i], before.names()[i]);
            }
            assert_eq!(grouping.of, before.of);
        }
    }

    #[test]
    fn reroll_name_rejects_an_unknown_group() {
        let (_, mut grouping) = two_groups();
        assert_eq!(
            grouping.reroll_name(GroupIdx(2), &mut SplitMix64::seeded(0)),
            Err(MoveError::UnknownGroup)
        );
    }

    fn doctored(names: usize, of: &[(u32, usize)]) -> Grouping {
        Grouping {
            names: draw_names(names, &mut SplitMix64::seeded(0)),
            of: of
                .iter()
                .map(|&(id, g)| (StudentId(id), GroupIdx(g)))
                .collect(),
        }
    }

    #[test]
    fn reconcile_drops_unknown_students_and_dangling_group_indexes() {
        let students = roster(3);
        let mut grouping = doctored(2, &[(0, 0), (1, 1), (2, 5), (77, 0)]);
        grouping.reconcile(&students);
        grouping.assert_invariants(&students);
        assert_eq!(grouping.group_of(StudentId(0)), Some(GroupIdx(0)));
        assert_eq!(grouping.group_of(StudentId(1)), Some(GroupIdx(1)));
        assert_eq!(grouping.group_of(StudentId(77)), None);
        assert!(grouping.group_of(StudentId(2)).is_some_and(|g| g.get() < 2));
    }

    #[test]
    fn reconcile_places_newcomers_in_the_smallest_group_lowest_index_first() {
        let students = roster(6);
        let mut grouping = doctored(3, &[(0, 0), (1, 0), (2, 1)]);
        grouping.reconcile(&students);
        assert_eq!(sizes(&grouping), [2, 2, 2]);
        // Roster order decides who lands where: 3 joins group 2 (size 0), 4 then ties at 1 and
        // takes the lowest index, 5 joins the remaining smallest group.
        assert_eq!(grouping.group_of(StudentId(3)), Some(GroupIdx(2)));
        assert_eq!(grouping.group_of(StudentId(4)), Some(GroupIdx(1)));
        assert_eq!(grouping.group_of(StudentId(5)), Some(GroupIdx(2)));
    }

    #[test]
    fn reconcile_is_idempotent() {
        let students = roster(8);
        let mut grouping = doctored(3, &[(0, 2), (1, 9), (50, 0)]);
        grouping.reconcile(&students);
        let once = grouping.clone();
        grouping.reconcile(&students);
        assert_eq!(grouping, once);
    }

    #[test]
    fn reconcile_leaves_a_consistent_grouping_untouched() {
        let (students, mut grouping) = two_groups();
        let before = grouping.clone();
        grouping.reconcile(&students);
        assert_eq!(grouping, before);
    }

    #[test]
    fn reconcile_truncates_names_beyond_the_cap() {
        let students = roster(5);
        let mut grouping = doctored(MAX_GROUPS + 10, &[(0, MAX_GROUPS + 5)]);
        grouping.reconcile(&students);
        grouping.assert_invariants(&students);
        assert_eq!(grouping.len(), MAX_GROUPS);
    }

    #[test]
    fn reconcile_cannot_place_anyone_without_groups_and_does_not_panic() {
        let students = roster(2);
        let mut grouping = doctored(0, &[]);
        grouping.reconcile(&students);
        assert!(grouping.is_empty());
        assert_eq!(grouping.group_of(StudentId(0)), None);
    }

    #[test]
    fn json_stores_membership_as_a_string_keyed_object() {
        let students = roster(3);
        let grouping = Grouping::split(&students, count(2, 3), &mut SplitMix64::seeded(1));
        let json = serde_json::to_value(&grouping).unwrap();
        let of = json["of"].as_object().unwrap();
        assert_eq!(of.keys().collect::<Vec<_>>(), ["0", "1", "2"]);
        assert!(of.values().all(|g| g.is_u64()));
        assert_eq!(serde_json::from_value::<Grouping>(json).unwrap(), grouping);
    }
}
