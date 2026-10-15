use std::num::NonZeroUsize;

/// The only randomness the domain knows. The web shell backs it with `Math.random`; tests use
/// [`SplitMix64`] so every outcome is reproducible.
pub trait Rng {
    /// Uniform in `0..n`.
    fn below(&mut self, n: NonZeroUsize) -> usize;
}

/// Deterministic generator that avoids `rand`'s wasm backend configuration.
#[derive(Clone, Debug)]
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn seeded(seed: u64) -> Self {
        SplitMix64(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

impl Rng for SplitMix64 {
    fn below(&mut self, n: NonZeroUsize) -> usize {
        // Multiply-high maps the 64-bit output onto 0..n without a division; the bias is
        // below 2^-40 for classroom-sized n.
        ((u128::from(self.next_u64()) * n.get() as u128) >> 64) as usize
    }
}

/// Fisher-Yates.
pub fn shuffle<T>(items: &mut [T], rng: &mut impl Rng) {
    for i in (1..items.len()).rev() {
        let j = rng.below(NonZeroUsize::MIN.saturating_add(i));
        items.swap(i, j);
    }
}

pub fn choose<'a, T>(items: &'a [T], rng: &mut impl Rng) -> Option<&'a T> {
    let n = NonZeroUsize::new(items.len())?;
    items.get(rng.below(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nz(n: usize) -> NonZeroUsize {
        NonZeroUsize::new(n).unwrap()
    }

    #[test]
    fn splitmix64_matches_the_reference_sequence() {
        // Reference outputs for seed 0 from the published SplitMix64 algorithm.
        let mut rng = SplitMix64::seeded(0);
        assert_eq!(rng.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(rng.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        assert_eq!(rng.next_u64(), 0x06C4_5D18_8009_454F);
    }

    #[test]
    fn below_stays_in_range_and_reaches_every_value() {
        for n in [1, 2, 3, 7, 40] {
            let mut rng = SplitMix64::seeded(n as u64);
            let mut seen = vec![false; n];
            for _ in 0..2_000 {
                let v = rng.below(nz(n));
                assert!(v < n);
                seen[v] = true;
            }
            assert!(seen.iter().all(|&s| s), "n={n} left a value unreachable");
        }
    }

    #[test]
    fn same_seed_gives_same_sequence() {
        let mut a = SplitMix64::seeded(99);
        let mut b = SplitMix64::seeded(99);
        let xs: Vec<_> = (0..20).map(|_| a.below(nz(1000))).collect();
        let ys: Vec<_> = (0..20).map(|_| b.below(nz(1000))).collect();
        assert_eq!(xs, ys);
    }

    #[test]
    fn shuffle_is_a_permutation() {
        for seed in 0..50 {
            let mut items: Vec<u32> = (0..17).collect();
            shuffle(&mut items, &mut SplitMix64::seeded(seed));
            let mut sorted = items.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, (0..17).collect::<Vec<_>>());
        }
    }

    #[test]
    fn shuffle_actually_reorders() {
        let mut items: Vec<u32> = (0..10).collect();
        shuffle(&mut items, &mut SplitMix64::seeded(1));
        assert_ne!(items, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn shuffle_handles_empty_and_single_slices() {
        let mut empty: [u8; 0] = [];
        shuffle(&mut empty, &mut SplitMix64::seeded(0));
        let mut one = [5];
        shuffle(&mut one, &mut SplitMix64::seeded(0));
        assert_eq!(one, [5]);
    }

    #[test]
    fn choose_returns_none_only_for_empty_input() {
        let mut rng = SplitMix64::seeded(3);
        assert_eq!(choose::<u8>(&[], &mut rng), None);
        assert_eq!(choose(&[9], &mut rng), Some(&9));
        let items = [1, 2, 3, 4];
        for _ in 0..50 {
            assert!(items.contains(choose(&items, &mut rng).unwrap()));
        }
    }
}
