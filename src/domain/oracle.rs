use serde::{Deserialize, Serialize};

use super::class::{Student, StudentId};
use super::rng::{Rng, choose};

/// Remembers who has been chosen this round, in draw order, so a reload mid-lesson does not
/// re-pick children who already went. Who is left is derived as roster minus drawn, so a roster
/// edit needs no sync step beyond [`Oracle::retain_roster`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Oracle {
    drawn: Vec<StudentId>,
}

impl Oracle {
    /// Uniform over roster students not yet drawn. When everyone has gone, a new round starts and
    /// its first pick excludes the previous pick (when the roster has more than one student), so
    /// nobody is chosen twice in a row across the refill. `None` only for an empty roster.
    pub fn draw(&mut self, roster: &[Student], rng: &mut impl Rng) -> Option<StudentId> {
        let mut eligible: Vec<StudentId> = self.waiting(roster).collect();
        if eligible.is_empty() && !roster.is_empty() {
            let previous = self.drawn.last().copied().filter(|_| roster.len() > 1);
            self.drawn.clear();
            eligible = self
                .waiting(roster)
                .filter(|&id| Some(id) != previous)
                .collect();
        }
        let pick = *choose(&eligible, rng)?;
        self.drawn.push(pick);
        Some(pick)
    }

    /// Roster students still waiting this round, for "12 left in the bag".
    pub fn remaining(&self, roster: &[Student]) -> usize {
        self.waiting(roster).count()
    }

    fn waiting<'a>(&'a self, roster: &'a [Student]) -> impl Iterator<Item = StudentId> + 'a {
        roster
            .iter()
            .map(|s| s.id)
            .filter(|id| !self.drawn.contains(id))
    }

    /// Forgets drawn ids that are no longer on the roster.
    pub(super) fn retain_roster(&mut self, roster: &[Student]) {
        self.drawn.retain(|id| roster.iter().any(|s| s.id == *id));
    }

    #[cfg(test)]
    pub(super) fn assert_invariants(&self, roster: &[Student]) {
        for (i, id) in self.drawn.iter().enumerate() {
            assert!(
                roster.iter().any(|s| s.id == *id),
                "drawn id is not on the roster"
            );
            assert!(
                !self.drawn[..i].contains(id),
                "drawn id repeated within a round"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::domain::rng::SplitMix64;
    use crate::domain::testing::roster;

    fn draw_all(oracle: &mut Oracle, students: &[Student], rng: &mut SplitMix64) -> Vec<StudentId> {
        (0..students.len())
            .map(|_| oracle.draw(students, rng).unwrap())
            .collect()
    }

    #[test]
    fn a_round_picks_every_student_exactly_once() {
        for n in [1, 2, 5, 24] {
            for seed in 0..30 {
                let students = roster(n);
                let mut oracle = Oracle::default();
                let picks = draw_all(&mut oracle, &students, &mut SplitMix64::seeded(seed));
                let unique: HashSet<_> = picks.iter().collect();
                assert_eq!(unique.len(), n as usize, "n={n} seed={seed}");
                assert_eq!(oracle.remaining(&students), 0);
            }
        }
    }

    #[test]
    fn remaining_counts_down_one_per_draw() {
        let students = roster(6);
        let mut oracle = Oracle::default();
        let mut rng = SplitMix64::seeded(4);
        for expected in (0..6).rev() {
            oracle.draw(&students, &mut rng).unwrap();
            assert_eq!(oracle.remaining(&students), expected);
        }
    }

    #[test]
    fn the_next_round_refills_without_repeating_the_last_pick() {
        for n in [2, 3, 10] {
            for seed in 0..100 {
                let students = roster(n);
                let mut oracle = Oracle::default();
                let mut rng = SplitMix64::seeded(seed);
                let mut previous = None;
                for _ in 0..4 {
                    for pick in draw_all(&mut oracle, &students, &mut rng) {
                        assert_ne!(Some(pick), previous, "n={n} seed={seed}");
                        previous = Some(pick);
                    }
                }
            }
        }
    }

    #[test]
    fn a_refill_leaves_the_rest_of_the_class_waiting() {
        let students = roster(5);
        let mut oracle = Oracle::default();
        let mut rng = SplitMix64::seeded(2);
        draw_all(&mut oracle, &students, &mut rng);
        assert_eq!(oracle.remaining(&students), 0);
        oracle.draw(&students, &mut rng).unwrap();
        assert_eq!(oracle.remaining(&students), 4);
    }

    #[test]
    fn a_class_of_one_picks_that_student_every_time() {
        let students = roster(1);
        let mut oracle = Oracle::default();
        let mut rng = SplitMix64::seeded(0);
        for _ in 0..3 {
            assert_eq!(oracle.draw(&students, &mut rng), Some(students[0].id));
        }
    }

    #[test]
    fn an_empty_roster_yields_nothing() {
        let mut oracle = Oracle::default();
        assert_eq!(oracle.draw(&[], &mut SplitMix64::seeded(0)), None);
        assert_eq!(oracle.remaining(&[]), 0);
    }

    #[test]
    fn the_first_pick_of_a_round_varies_with_the_seed() {
        let students = roster(10);
        let firsts: HashSet<_> = (0..40)
            .map(|seed| {
                Oracle::default()
                    .draw(&students, &mut SplitMix64::seeded(seed))
                    .unwrap()
            })
            .collect();
        assert!(firsts.len() > 5);
    }

    #[test]
    fn students_removed_from_the_roster_are_never_drawn() {
        let students = roster(6);
        let mut oracle = Oracle::default();
        let mut rng = SplitMix64::seeded(8);
        let first = oracle.draw(&students, &mut rng).unwrap();
        let smaller: Vec<Student> = students.iter().filter(|s| s.id != first).cloned().collect();
        oracle.retain_roster(&smaller);
        oracle.assert_invariants(&smaller);

        assert_eq!(oracle.remaining(&smaller), 5);
        let picks = draw_all(&mut oracle, &smaller, &mut rng);
        assert!(!picks.contains(&first));
        assert_eq!(picks.iter().collect::<HashSet<_>>().len(), 5);
    }

    #[test]
    fn a_student_added_mid_round_can_be_drawn_in_that_round() {
        let mut students = roster(3);
        let mut oracle = Oracle::default();
        let mut rng = SplitMix64::seeded(1);
        oracle.draw(&students, &mut rng).unwrap();
        let newcomer = roster(4).pop().unwrap();
        students.push(newcomer.clone());
        assert_eq!(oracle.remaining(&students), 3);

        let rest: Vec<_> = (0..3)
            .map(|_| oracle.draw(&students, &mut rng).unwrap())
            .collect();
        assert!(rest.contains(&newcomer.id));
        assert_eq!(oracle.remaining(&students), 0);
    }

    #[test]
    fn stale_ids_do_not_count_as_waiting_or_gone() {
        let students = roster(3);
        let oracle = Oracle {
            drawn: vec![StudentId(99), students[0].id],
        };
        assert_eq!(oracle.remaining(&students), 2);
    }

    #[test]
    fn retaining_the_roster_keeps_draw_order() {
        let students = roster(4);
        let mut oracle = Oracle {
            drawn: vec![students[2].id, StudentId(99), students[0].id],
        };
        oracle.retain_roster(&students);
        assert_eq!(oracle.drawn, [students[2].id, students[0].id]);
    }
}
