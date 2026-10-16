//! The slot-machine spin shown before the oracle's answer. [`Class::draw`] has already chosen and
//! recorded the winner, so a [`Reel`] only schedules flashes that end on it. It reads nothing from
//! the oracle and writes nothing to the class.
//!
//! [`Class::draw`]: super::class::Class::draw

use super::class::Pick;
use super::rng::{Rng, shuffle};
use super::timer::Millis;

/// Flashes in a full spin, the landing included.
const FRAMES: usize = 24;

/// How long a full spin lasts, the landing's hold included.
const SPIN: Millis = Millis(2_000);

/// How many times longer the landing is held than the first flash.
const SLOWDOWN: u64 = 6;

/// One flash of the spin: who is on screen and for how long.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub pick: Pick,
    pub hold: Millis,
}

/// The flashes of one spin, in order. Empty when there is nothing to spin through, and the UI
/// then shows the answer straight away.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reel {
    frames: Vec<Frame>,
}

impl Reel {
    /// Flashes through `candidates` in a random cyclic order, fast and then slowing, and ends on
    /// `landing`. Consecutive frames are never the same candidate, and a pool of at most
    /// [`FRAMES`] shows everyone. Fewer than two candidates leave nothing to spin through.
    ///
    /// # Panics
    ///
    /// When there are two or more candidates and `landing` is not one of them.
    pub fn new(mut candidates: Vec<Pick>, landing: &Pick, rng: &mut impl Rng) -> Reel {
        let pool = candidates.len();
        if pool < 2 {
            return Reel::default();
        }
        shuffle(&mut candidates, rng);
        let landing_at = candidates
            .iter()
            .position(|candidate| candidate == landing)
            .expect("the landing is one of the candidates");
        let frames = holds()
            .into_iter()
            .enumerate()
            .map(|(i, hold)| {
                // The walk steps forward through the ring and finishes on the landing.
                let behind = (FRAMES - 1 - i) % pool;
                let at = (landing_at + pool - behind) % pool;
                Frame {
                    pick: candidates[at].clone(),
                    hold,
                }
            })
            .collect();
        Reel { frames }
    }

    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

/// Holds that grow with the cube of progress, `1 + (SLOWDOWN - 1) * t^3` for `t` from 0 to 1,
/// scaled to total [`SPIN`]. Each hold is rounded alone so the sequence stays non-decreasing, which
/// leaves the total off by a few milliseconds.
fn holds() -> Vec<Millis> {
    let last = FRAMES as u64 - 1;
    // Multiplying every weight by `last^3` keeps the arithmetic in integers.
    let weights: Vec<u64> = (0..=last)
        .map(|i| last.pow(3) + (SLOWDOWN - 1) * i.pow(3))
        .collect();
    let total: u64 = weights.iter().sum();
    weights
        .into_iter()
        .map(|weight| Millis((SPIN.0 * weight + total / 2) / total))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::class::{Class, ClassId, Draw};
    use crate::domain::grouping::GroupCount;
    use crate::domain::rng::SplitMix64;
    use crate::domain::testing::{label, numbered, roster};

    fn students(n: u32) -> Vec<Pick> {
        roster(n).into_iter().map(Pick::Student).collect()
    }

    fn reel_of(pool: &[Pick], landing: usize, seed: u64) -> Reel {
        Reel::new(pool.to_vec(), &pool[landing], &mut SplitMix64::seeded(seed))
    }

    /// A reel for every pool size from two through past the frame count, every landing, and a few
    /// seeds.
    fn each_reel(check: impl Fn(&[Pick], usize, &Reel)) {
        for size in 2..=FRAMES as u32 + 6 {
            let pool = students(size);
            for landing in 0..pool.len() {
                for seed in 0..4 {
                    check(&pool, landing, &reel_of(&pool, landing, seed));
                }
            }
        }
    }

    #[test]
    fn a_full_spin_has_one_frame_per_flash() {
        each_reel(|_, _, reel| assert_eq!(reel.frames().len(), FRAMES));
    }

    #[test]
    fn the_last_frame_is_the_landing() {
        each_reel(|pool, landing, reel| {
            assert_eq!(reel.frames().last().unwrap().pick, pool[landing]);
        });
    }

    #[test]
    fn no_two_consecutive_frames_show_the_same_candidate() {
        each_reel(|pool, landing, reel| {
            for pair in reel.frames().windows(2) {
                assert_ne!(
                    pair[0].pick,
                    pair[1].pick,
                    "pool of {}, landing {landing}",
                    pool.len()
                );
            }
        });
    }

    #[test]
    fn only_candidates_flash_by() {
        each_reel(|pool, _, reel| {
            for frame in reel.frames() {
                assert!(pool.contains(&frame.pick));
            }
        });
    }

    #[test]
    fn everyone_flashes_by_when_the_pool_fits_in_the_spin() {
        for size in 2..=FRAMES as u32 {
            let pool = students(size);
            for seed in 0..10 {
                let reel = reel_of(&pool, seed as usize % pool.len(), seed);
                for candidate in &pool {
                    assert!(
                        reel.frames().iter().any(|frame| frame.pick == *candidate),
                        "pool of {size} left someone out"
                    );
                }
            }
        }
    }

    #[test]
    fn the_flashes_slow_down_and_fill_the_spin() {
        each_reel(|_, _, reel| {
            let holds: Vec<Millis> = reel.frames().iter().map(|frame| frame.hold).collect();
            assert!(holds.is_sorted(), "{holds:?}");
            assert!(holds.first() < holds.last());
            let total: u64 = holds.iter().map(|hold| hold.0).sum();
            assert!(
                total.abs_diff(SPIN.0) <= FRAMES as u64 / 2,
                "holds total {total}ms"
            );
        });
    }

    #[test]
    fn a_pool_of_fewer_than_two_has_nothing_to_spin_through() {
        let lone = students(1);
        assert!(Reel::new(Vec::new(), &lone[0], &mut SplitMix64::seeded(1)).is_empty());
        assert!(Reel::new(lone.clone(), &lone[0], &mut SplitMix64::seeded(1)).is_empty());
    }

    #[test]
    fn spinning_never_spends_a_draw_or_changes_the_class() {
        for kind in [Draw::Student, Draw::Group] {
            let mut rng = SplitMix64::seeded(7);
            let mut class = Class::new(ClassId(0), label("Room 1"), numbered(6), &mut rng);
            class.regroup(GroupCount::new(3, 6).unwrap(), &mut rng);
            let pool = class.oracle_remaining(kind);
            let mut seen = Vec::new();
            for spent in 1..=pool {
                let pick = class.draw(kind, &mut rng).unwrap();
                let before = class.clone();
                let reel = Reel::new(class.candidates(kind), &pick, &mut rng);
                assert_eq!(class, before);
                assert_eq!(reel.frames().last().unwrap().pick, pick);
                assert_eq!(class.oracle_remaining(kind), pool - spent);
                assert!(!seen.contains(&pick), "{pick:?} came up twice in a round");
                seen.push(pick);
            }
        }
    }
}
