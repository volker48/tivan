use serde::{Deserialize, Serialize};

use super::rng::{Rng, choose};

/// Remembers which of a pool of ids have been chosen this round, in draw order, so a reload
/// mid-lesson does not re-pick children or groups who already went. Who is left is derived as pool
/// minus drawn, so a pool edit needs no sync step beyond [`Oracle::retain`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Oracle<I> {
    drawn: Vec<I>,
}

impl<I> Default for Oracle<I> {
    fn default() -> Self {
        Oracle { drawn: Vec::new() }
    }
}

impl<I> Oracle<I> {
    pub fn is_empty(&self) -> bool {
        self.drawn.is_empty()
    }
}

impl<I: Copy + Eq> Oracle<I> {
    /// Uniform over pool ids not yet drawn. When all have gone, a new round starts and its first
    /// pick excludes the previous pick (when the pool has more than one id), so nothing is chosen
    /// twice in a row across the refill. `None` only for an empty pool.
    pub fn draw(&mut self, pool: &[I], rng: &mut impl Rng) -> Option<I> {
        let mut eligible: Vec<I> = self.waiting(pool).collect();
        if eligible.is_empty() && !pool.is_empty() {
            let previous = self.drawn.last().copied().filter(|_| pool.len() > 1);
            self.drawn.clear();
            eligible = self
                .waiting(pool)
                .filter(|&id| Some(id) != previous)
                .collect();
        }
        let pick = *choose(&eligible, rng)?;
        self.drawn.push(pick);
        Some(pick)
    }

    /// Pool ids still waiting this round, for "12 left in the bag".
    pub fn remaining(&self, pool: &[I]) -> usize {
        self.waiting(pool).count()
    }

    fn waiting<'a>(&'a self, pool: &'a [I]) -> impl Iterator<Item = I> + 'a {
        pool.iter().copied().filter(|id| !self.drawn.contains(id))
    }

    /// Forgets drawn ids that are no longer in the pool.
    pub(super) fn retain(&mut self, pool: &[I]) {
        self.drawn.retain(|id| pool.contains(id));
    }

    #[cfg(test)]
    pub(super) fn assert_invariants(&self, pool: &[I]) {
        for (i, id) in self.drawn.iter().enumerate() {
            assert!(pool.contains(id), "drawn id is not in the pool");
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
    use crate::domain::StudentId;
    use crate::domain::rng::SplitMix64;
    use crate::domain::testing::roster;

    /// The pool of a class of `n`, the way `Class` would build it from its roster.
    fn ids(n: u32) -> Vec<StudentId> {
        roster(n).iter().map(|s| s.id).collect()
    }

    fn draw_all(
        oracle: &mut Oracle<StudentId>,
        pool: &[StudentId],
        rng: &mut SplitMix64,
    ) -> Vec<StudentId> {
        (0..pool.len())
            .map(|_| oracle.draw(pool, rng).unwrap())
            .collect()
    }

    #[test]
    fn a_round_picks_every_id_exactly_once() {
        for n in [1, 2, 5, 24] {
            for seed in 0..30 {
                let pool = ids(n);
                let mut oracle = Oracle::default();
                let picks = draw_all(&mut oracle, &pool, &mut SplitMix64::seeded(seed));
                let unique: HashSet<_> = picks.iter().collect();
                assert_eq!(unique.len(), n as usize, "n={n} seed={seed}");
                assert_eq!(oracle.remaining(&pool), 0);
            }
        }
    }

    #[test]
    fn remaining_counts_down_one_per_draw() {
        let pool = ids(6);
        let mut oracle = Oracle::default();
        let mut rng = SplitMix64::seeded(4);
        for expected in (0..6).rev() {
            oracle.draw(&pool, &mut rng).unwrap();
            assert_eq!(oracle.remaining(&pool), expected);
        }
    }

    #[test]
    fn the_next_round_refills_without_repeating_the_last_pick() {
        for n in [2, 3, 10] {
            for seed in 0..100 {
                let pool = ids(n);
                let mut oracle = Oracle::default();
                let mut rng = SplitMix64::seeded(seed);
                let mut previous = None;
                for _ in 0..4 {
                    for pick in draw_all(&mut oracle, &pool, &mut rng) {
                        assert_ne!(Some(pick), previous, "n={n} seed={seed}");
                        previous = Some(pick);
                    }
                }
            }
        }
    }

    #[test]
    fn a_refill_leaves_the_rest_of_the_pool_waiting() {
        let pool = ids(5);
        let mut oracle = Oracle::default();
        let mut rng = SplitMix64::seeded(2);
        draw_all(&mut oracle, &pool, &mut rng);
        assert_eq!(oracle.remaining(&pool), 0);
        oracle.draw(&pool, &mut rng).unwrap();
        assert_eq!(oracle.remaining(&pool), 4);
    }

    #[test]
    fn a_pool_of_one_picks_that_id_every_time() {
        let pool = ids(1);
        let mut oracle = Oracle::default();
        let mut rng = SplitMix64::seeded(0);
        for _ in 0..3 {
            assert_eq!(oracle.draw(&pool, &mut rng), Some(pool[0]));
        }
    }

    #[test]
    fn an_empty_pool_yields_nothing() {
        let mut oracle = Oracle::<StudentId>::default();
        assert_eq!(oracle.draw(&[], &mut SplitMix64::seeded(0)), None);
        assert_eq!(oracle.remaining(&[]), 0);
    }

    #[test]
    fn the_first_pick_of_a_round_varies_with_the_seed() {
        let pool = ids(10);
        let firsts: HashSet<_> = (0..40)
            .map(|seed| {
                Oracle::default()
                    .draw(&pool, &mut SplitMix64::seeded(seed))
                    .unwrap()
            })
            .collect();
        assert!(firsts.len() > 5);
    }

    #[test]
    fn ids_removed_from_the_pool_are_never_drawn() {
        let pool = ids(6);
        let mut oracle = Oracle::default();
        let mut rng = SplitMix64::seeded(8);
        let first = oracle.draw(&pool, &mut rng).unwrap();
        let smaller: Vec<StudentId> = pool.iter().copied().filter(|&id| id != first).collect();
        oracle.retain(&smaller);
        oracle.assert_invariants(&smaller);

        assert_eq!(oracle.remaining(&smaller), 5);
        let picks = draw_all(&mut oracle, &smaller, &mut rng);
        assert!(!picks.contains(&first));
        assert_eq!(picks.iter().collect::<HashSet<_>>().len(), 5);
    }

    #[test]
    fn an_id_added_mid_round_can_be_drawn_in_that_round() {
        let mut pool = ids(3);
        let mut oracle = Oracle::default();
        let mut rng = SplitMix64::seeded(1);
        oracle.draw(&pool, &mut rng).unwrap();
        let newcomer = ids(4).pop().unwrap();
        pool.push(newcomer);
        assert_eq!(oracle.remaining(&pool), 3);

        let rest: Vec<_> = (0..3)
            .map(|_| oracle.draw(&pool, &mut rng).unwrap())
            .collect();
        assert!(rest.contains(&newcomer));
        assert_eq!(oracle.remaining(&pool), 0);
    }

    #[test]
    fn stale_ids_do_not_count_as_waiting_or_gone() {
        let pool = ids(3);
        let oracle = Oracle {
            drawn: vec![StudentId(99), pool[0]],
        };
        assert_eq!(oracle.remaining(&pool), 2);
    }

    #[test]
    fn retaining_the_pool_keeps_draw_order() {
        let pool = ids(4);
        let mut oracle = Oracle {
            drawn: vec![pool[2], StudentId(99), pool[0]],
        };
        oracle.retain(&pool);
        assert_eq!(oracle.drawn, [pool[2], pool[0]]);
    }
}
