use serde::{Deserialize, Serialize};

use super::rng::{Rng, choose};

/// A student's picture. Stored by name, so renaming or removing a variant is a schema change and
/// adding one is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Avatar {
    Bear,
    Panda,
    Cat,
    Fox,
    Bunny,
    Koala,
    Pig,
    Frog,
    Chick,
    Penguin,
    Mouse,
    Lion,
    Tiger,
    Puppy,
    Monkey,
    Raccoon,
    Hamster,
    Sheep,
}

impl Avatar {
    pub const ALL: [Avatar; 18] = [
        Avatar::Bear,
        Avatar::Panda,
        Avatar::Cat,
        Avatar::Fox,
        Avatar::Bunny,
        Avatar::Koala,
        Avatar::Pig,
        Avatar::Frog,
        Avatar::Chick,
        Avatar::Penguin,
        Avatar::Mouse,
        Avatar::Lion,
        Avatar::Tiger,
        Avatar::Puppy,
        Avatar::Monkey,
        Avatar::Raccoon,
        Avatar::Hamster,
        Avatar::Sheep,
    ];

    /// The stored name, which is also the suffix of the sprite symbol `avatar-<name>`.
    pub fn name(self) -> &'static str {
        match self {
            Avatar::Bear => "bear",
            Avatar::Panda => "panda",
            Avatar::Cat => "cat",
            Avatar::Fox => "fox",
            Avatar::Bunny => "bunny",
            Avatar::Koala => "koala",
            Avatar::Pig => "pig",
            Avatar::Frog => "frog",
            Avatar::Chick => "chick",
            Avatar::Penguin => "penguin",
            Avatar::Mouse => "mouse",
            Avatar::Lion => "lion",
            Avatar::Tiger => "tiger",
            Avatar::Puppy => "puppy",
            Avatar::Monkey => "monkey",
            Avatar::Raccoon => "raccoon",
            Avatar::Hamster => "hamster",
            Avatar::Sheep => "sheep",
        }
    }
}

/// Uniform among the avatars used least often in `taken`, so no avatar repeats until all of them
/// are in use and larger classes then fill each level evenly.
pub(crate) fn pick(taken: impl IntoIterator<Item = Avatar>, rng: &mut impl Rng) -> Avatar {
    let mut uses = [0usize; Avatar::ALL.len()];
    for avatar in taken {
        uses[avatar as usize] += 1;
    }
    let fewest = uses.iter().copied().fold(usize::MAX, usize::min);
    let candidates: Vec<Avatar> = Avatar::ALL
        .into_iter()
        .filter(|&avatar| uses[avatar as usize] == fewest)
        .collect();
    choose(&candidates, rng)
        .copied()
        .expect("the least used level has at least one avatar")
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;
    use crate::domain::rng::SplitMix64;

    /// Picks `n` avatars one after another, each against the ones already picked.
    fn picked(n: usize, seed: u64) -> Vec<Avatar> {
        let mut rng = SplitMix64::seeded(seed);
        let mut taken = Vec::new();
        for _ in 0..n {
            let next = pick(taken.iter().copied(), &mut rng);
            taken.push(next);
        }
        taken
    }

    fn uses(avatars: &[Avatar]) -> HashMap<Avatar, usize> {
        let mut uses = HashMap::new();
        for &avatar in avatars {
            *uses.entry(avatar).or_default() += 1;
        }
        uses
    }

    #[test]
    fn pick_never_repeats_until_every_avatar_is_used() {
        for seed in 0..30 {
            let avatars = picked(Avatar::ALL.len(), seed);
            assert_eq!(uses(&avatars).len(), Avatar::ALL.len(), "seed {seed}");
        }
    }

    #[test]
    fn thirty_six_students_use_each_avatar_exactly_twice() {
        for seed in 0..30 {
            let counts = uses(&picked(2 * Avatar::ALL.len(), seed));
            assert_eq!(counts.len(), Avatar::ALL.len(), "seed {seed}");
            assert!(counts.values().all(|&n| n == 2), "seed {seed}: {counts:?}");
        }
    }

    #[test]
    fn pick_keeps_every_avatars_use_within_one_of_the_others() {
        let avatars = picked(50, 7);
        for n in 1..=avatars.len() {
            let counts = uses(&avatars[..n]);
            let used = |avatar: &Avatar| counts.get(avatar).copied().unwrap_or(0);
            let most = Avatar::ALL.iter().map(used).max().unwrap();
            let fewest = Avatar::ALL.iter().map(used).min().unwrap();
            assert!(most - fewest <= 1, "after {n} picks: {counts:?}");
        }
    }

    #[test]
    fn the_first_pick_is_not_always_the_same_avatar() {
        let firsts: HashSet<_> = (0..40).map(|seed| picked(1, seed)[0]).collect();
        assert!(firsts.len() > 1);
    }

    #[test]
    fn every_avatar_is_stored_under_its_name() {
        for avatar in Avatar::ALL {
            let stored = serde_json::to_string(&avatar).unwrap();
            assert_eq!(stored, format!("\"{}\"", avatar.name()));
            assert_eq!(serde_json::from_str::<Avatar>(&stored).unwrap(), avatar);
        }
    }
}
