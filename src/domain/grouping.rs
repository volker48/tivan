//! Splitting a roster into named groups. Membership is a map from student to group index, so a
//! student cannot be in two groups by construction. The roster lives on `Class`; `Class` calls
//! [`Grouping::reconcile`] after every roster edit so the map covers exactly the roster.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;

use serde::{Deserialize, Serialize};

use super::centers::{CenterIdx, Centers};
use super::class::{Student, StudentId};
use super::label::Label;
use super::names::{MAX_GROUPS, draw_name, draw_names};
use super::oracle::Oracle;
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
    UnknownCenter,
}

/// `names` is never empty and has at most `MAX_GROUPS` entries (the owning `Class` drops an empty
/// grouping), and `of` maps every roster student to an index below `names.len()`.
/// Balance is a property of [`Grouping::split`], not of this type, because manual moves break it.
/// Group names are distinct when generated; a teacher may rename two groups to the same text.
/// When `centers` is present it has one center per group, which is why every edit that changes
/// `names` also refits it. The same goes for `oracle`, the round of groups picked so far: it
/// lives here because a group round cannot outlive its groups, so a fresh split starts a fresh
/// round.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grouping {
    names: Vec<Label>,
    of: BTreeMap<StudentId, GroupIdx>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    centers: Option<Centers>,
    #[serde(default, skip_serializing_if = "Oracle::is_empty")]
    oracle: Oracle<GroupIdx>,
}

/// Sizes of `m` groups over `n` students: the first `n % m` groups get one extra member.
pub fn group_sizes(n: usize, m: GroupCount) -> Vec<usize> {
    let m = m.get();
    let (base, extra) = (n / m, n % m);
    (0..m).map(|i| base + usize::from(i < extra)).collect()
}

/// Shuffles `students` and cuts them into `count` groups by [`group_sizes`].
fn deal(
    students: &[Student],
    count: GroupCount,
    rng: &mut impl Rng,
) -> BTreeMap<StudentId, GroupIdx> {
    let mut ids: Vec<StudentId> = students.iter().map(|s| s.id).collect();
    shuffle(&mut ids, rng);

    let mut shuffled = ids.into_iter();
    let mut of = BTreeMap::new();
    for (group, size) in group_sizes(students.len(), count).into_iter().enumerate() {
        for id in shuffled.by_ref().take(size) {
            of.insert(id, GroupIdx(group));
        }
    }
    of
}

impl Grouping {
    /// Shuffles `students`, cuts them by [`group_sizes`], and draws fresh unique names.
    pub fn split(students: &[Student], count: GroupCount, rng: &mut impl Rng) -> Grouping {
        let of = deal(students, count, rng);
        let names = draw_names(count.get(), rng);
        Grouping {
            names,
            of,
            centers: None,
            oracle: Oracle::default(),
        }
    }

    /// Re-deals `students` evenly across the existing groups and keeps every name, hand-made
    /// ones included. When the roster has shrunk below the group count, the first groups survive
    /// with their names and the rest are dropped, and the centers and the group round shrink with
    /// them. Otherwise the centers, their turn and the round are untouched: only members change,
    /// groups stay where they stand. Does nothing for an empty roster.
    pub fn reshuffle(&mut self, students: &[Student], rng: &mut impl Rng) {
        let Ok(count) = GroupCount::nearest(self.names.len(), students.len()) else {
            return;
        };
        self.names.truncate(count.get());
        self.fit_centers();
        self.retain_oracle();
        self.of = deal(students, count, rng);
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

    /// Uniform over groups not yet drawn this round, empty ones included. When every group has
    /// gone a new round starts, and its first pick is not the last group of the old one. `None`
    /// only when there are no groups.
    pub fn draw(&mut self, rng: &mut impl Rng) -> Option<GroupIdx> {
        let pool = self.pool();
        self.oracle.draw(&pool, rng)
    }

    /// Groups not yet drawn this round.
    pub fn remaining(&self) -> usize {
        self.oracle.remaining(&self.pool())
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

    pub fn centers(&self) -> Option<&Centers> {
        self.centers.as_ref()
    }

    /// Gives each group a center, "Center 1" onward, at round 1. Does nothing when centers are
    /// already in use, so a repeated call keeps the teacher's names and place in the cycle.
    pub fn add_centers(&mut self) {
        let count = self.names.len();
        self.centers.get_or_insert_with(|| Centers::new(count));
    }

    pub fn remove_centers(&mut self) {
        self.centers = None;
    }

    /// Moves every group to the next center. Does nothing without centers.
    pub fn rotate_centers(&mut self) {
        if let Some(centers) = &mut self.centers {
            centers.rotate();
        }
    }

    pub fn rename_center(&mut self, idx: CenterIdx, name: Label) -> Result<(), MoveError> {
        self.centers
            .as_mut()
            .ok_or(MoveError::UnknownCenter)?
            .rename(idx, name)
    }

    /// Hands the centers to the caller so they can outlive this grouping.
    pub(super) fn take_centers(&mut self) -> Option<Centers> {
        self.centers.take()
    }

    /// Adopts `centers`, refitted to this grouping's group count.
    pub(super) fn set_centers(&mut self, centers: Option<Centers>) {
        self.centers = centers;
        self.fit_centers();
    }

    fn fit_centers(&mut self) {
        let count = self.names.len();
        if let Some(centers) = &mut self.centers {
            centers.fit(count);
        }
    }

    fn retain_oracle(&mut self) {
        let pool = self.pool();
        self.oracle.retain(&pool);
    }

    fn pool(&self) -> Vec<GroupIdx> {
        (0..self.names.len()).map(GroupIdx).collect()
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
        self.fit_centers();
        self.retain_oracle();
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
        if let Some(centers) = &self.centers {
            centers.assert_invariants(self.names.len());
        }
        self.oracle.assert_invariants(&self.pool());
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

    #[test]
    fn reshuffle_keeps_names_covers_the_roster_and_rebalances() {
        let students = roster(11);
        for seed in 0..40 {
            let mut rng = SplitMix64::seeded(seed);
            let mut grouping = Grouping::split(&students, count(4, 11), &mut rng);
            grouping.rename(GroupIdx(2), label("Table Four")).unwrap();
            grouping.move_member(students[0].id, GroupIdx(3)).unwrap();
            grouping.move_member(students[1].id, GroupIdx(3)).unwrap();
            let names = grouping.names().to_vec();

            grouping.reshuffle(&students, &mut rng);

            assert_eq!(grouping.names(), names);
            grouping.assert_invariants(&students);
            assert_eq!(sizes(&grouping), group_sizes(11, count(4, 11)));
        }
    }

    #[test]
    fn reshuffle_is_deterministic_for_a_seed_and_moves_students() {
        let students = roster(12);
        let start = Grouping::split(&students, count(3, 12), &mut SplitMix64::seeded(1));
        let reshuffled = |seed| {
            let mut grouping = start.clone();
            grouping.reshuffle(&students, &mut SplitMix64::seeded(seed));
            grouping
        };
        assert_eq!(reshuffled(7), reshuffled(7));
        assert!(
            (0..20).any(|seed| reshuffled(seed).of != start.of),
            "some seed should deal differently"
        );
    }

    #[test]
    fn reshuffle_keeps_the_first_groups_when_the_roster_is_smaller_than_the_group_count() {
        let students = roster(6);
        let mut grouping = Grouping::split(&students, count(6, 6), &mut SplitMix64::seeded(1));
        let names = grouping.names().to_vec();
        let remaining = &students[..2];

        grouping.reshuffle(remaining, &mut SplitMix64::seeded(2));

        assert_eq!(grouping.names(), &names[..2]);
        grouping.assert_invariants(remaining);
        assert_eq!(sizes(&grouping), [1, 1]);
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
            centers: None,
            oracle: Oracle::default(),
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

    fn centers_json(names: &[&str], turn: usize) -> Centers {
        serde_json::from_value(serde_json::json!({"names": names, "turn": turn})).unwrap()
    }

    #[test]
    fn reconcile_fits_centers_to_the_groups() {
        let students = roster(4);
        let cases: [(&[&str], usize); 3] = [
            (&["A", "B", "C", "D", "E"], 4),
            (&["A"], 0),
            (&["A", "B", "C"], 7),
        ];
        for (names, turn) in cases {
            let mut grouping = doctored(3, &[(0, 0), (1, 1), (2, 2), (3, 0)]);
            grouping.centers = Some(centers_json(names, turn));

            grouping.reconcile(&students);

            grouping.assert_invariants(&students);
            let centers = grouping.centers().unwrap();
            let kept = names.len().min(3);
            let kept_names: Vec<&str> = centers.names()[..kept].iter().map(Label::as_str).collect();
            assert_eq!(kept_names, names[..kept], "{names:?} turn {turn}");
        }
    }

    #[test]
    fn reconcile_is_idempotent_for_centers_too() {
        let students = roster(4);
        let mut grouping = doctored(3, &[(0, 0), (1, 1), (2, 2), (3, 0)]);
        grouping.centers = Some(centers_json(&["A", "B", "C", "D", "E"], 9));
        grouping.reconcile(&students);
        let once = grouping.clone();
        grouping.reconcile(&students);
        assert_eq!(grouping, once);
    }

    #[test]
    fn reconcile_leaves_consistent_centers_untouched() {
        let (students, mut grouping) = two_groups();
        grouping.add_centers();
        grouping.rotate_centers();
        let before = grouping.clone();
        grouping.reconcile(&students);
        assert_eq!(grouping, before);
    }

    #[test]
    fn set_centers_refits_them_to_this_groupings_group_count() {
        let (_, mut grouping) = two_groups();

        grouping.set_centers(Some(centers_json(&["A", "B", "C", "D"], 3)));

        let centers = grouping.centers().unwrap();
        assert_eq!(centers.names(), [label("A"), label("B")]);
        assert_eq!(centers.round(), 2);

        assert_eq!(
            grouping.take_centers().as_ref(),
            Some(centers_json(&["A", "B"], 1)).as_ref()
        );
        assert!(grouping.centers().is_none());
        grouping.set_centers(None);
        assert!(grouping.centers().is_none());
    }

    #[test]
    fn center_edits_without_centers_do_nothing_or_report_an_unknown_center() {
        let (_, mut grouping) = two_groups();
        let before = grouping.clone();
        grouping.rotate_centers();
        grouping.remove_centers();
        assert_eq!(grouping, before);
        assert_eq!(
            grouping.rename_center(CenterIdx(0), label("x")),
            Err(MoveError::UnknownCenter)
        );
    }

    #[test]
    fn json_omits_centers_until_they_exist_and_reads_a_blob_that_has_none() {
        let (_, mut grouping) = two_groups();
        let without = serde_json::to_value(&grouping).unwrap();
        assert!(without.get("centers").is_none());
        assert_eq!(
            serde_json::from_value::<Grouping>(without).unwrap(),
            grouping
        );

        grouping.add_centers();
        grouping.rotate_centers();
        let with = serde_json::to_value(&grouping).unwrap();
        assert_eq!(
            with["centers"],
            serde_json::json!({"names": ["Center 1", "Center 2"], "turn": 1})
        );
        assert_eq!(serde_json::from_value::<Grouping>(with).unwrap(), grouping);
    }

    #[test]
    fn a_fresh_split_has_every_group_waiting_and_a_pick_uses_one_up() {
        let students = roster(9);
        let mut grouping = Grouping::split(&students, count(3, 9), &mut SplitMix64::seeded(1));
        assert_eq!(grouping.remaining(), 3);

        let picked = grouping.draw(&mut SplitMix64::seeded(2)).unwrap();

        assert!(picked.get() < 3);
        assert_eq!(grouping.remaining(), 2);
    }

    #[test]
    fn reconcile_forgets_drawn_groups_it_truncates() {
        let students = roster(5);
        let mut grouping = doctored(MAX_GROUPS + 10, &[]);
        let mut rng = SplitMix64::seeded(4);
        let picks: Vec<GroupIdx> = (0..MAX_GROUPS + 5)
            .map(|_| grouping.draw(&mut rng).unwrap())
            .collect();

        grouping.reconcile(&students);

        grouping.assert_invariants(&students);
        let survivors = picks.iter().filter(|idx| idx.get() < MAX_GROUPS).count();
        assert_eq!(grouping.remaining(), MAX_GROUPS - survivors);
    }

    #[test]
    fn json_omits_the_group_round_until_a_group_is_drawn_and_reads_a_blob_that_has_none() {
        let (_, mut grouping) = two_groups();
        let without = serde_json::to_value(&grouping).unwrap();
        assert!(without.get("oracle").is_none());
        assert_eq!(
            serde_json::from_value::<Grouping>(without).unwrap(),
            grouping
        );

        let drawn = grouping.draw(&mut SplitMix64::seeded(1)).unwrap();
        let with = serde_json::to_value(&grouping).unwrap();
        assert_eq!(with["oracle"], serde_json::json!({"drawn": [drawn.get()]}));
        assert_eq!(serde_json::from_value::<Grouping>(with).unwrap(), grouping);
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
