//! A tiny deterministic random number generator.

/// A source of randomness the composing crates can be generic over.
pub trait Rng {
    /// The next 64 random bits.
    fn next_u64(&mut self) -> u64;

    /// A uniform value in `[0, 1)`.
    fn next_f64(&mut self) -> f64 {
        // 53 mantissa bits give a uniformly spaced grid in [0, 1).
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A uniform integer in `0..n` (`0` when `n == 0`).
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_f64() * n as f64) as usize % n
    }

    /// A uniform integer in `lo..=hi` (`lo` when the range is empty).
    fn range_i32(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + self.below((hi - lo + 1) as usize) as i32
    }

    /// `true` with probability `p`.
    fn chance(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }

    /// A uniformly chosen element, or `None` for an empty slice.
    fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            items.get(self.below(items.len()))
        }
    }

    /// An index chosen with probability proportional to `weights`, or `None`
    /// if every weight is zero or negative.
    fn weighted(&mut self, weights: &[f64]) -> Option<usize> {
        let total: f64 = weights.iter().filter(|w| **w > 0.0).sum();
        if total <= 0.0 {
            return None;
        }
        let mut r = self.next_f64() * total;
        let mut last = None;
        for (i, &w) in weights.iter().enumerate() {
            if w <= 0.0 {
                continue;
            }
            last = Some(i);
            if r < w {
                return Some(i);
            }
            r -= w;
        }
        last
    }

    /// Shuffles `items` in place (Fisher–Yates).
    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }

    /// A standard normal deviate (Box–Muller).
    fn normal(&mut self) -> f64 {
        let u1 = self.next_f64().max(1e-12);
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (core::f64::consts::TAU * u2).cos()
    }
}

/// The SplitMix64 generator: small, fast, and good enough for art.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// A generator seeded from `seed`.
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// A generator whose stream is a deterministic function of this one's
    /// current state and `salt`, for independent sub-streams.
    pub fn fork(&mut self, salt: u64) -> Self {
        let s = self.next_u64() ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        Self::new(s)
    }
}

impl Rng for SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

impl Default for SplitMix64 {
    fn default() -> Self {
        Self::new(0x5EED_1789)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_bounded() {
        let mut a = SplitMix64::new(7);
        let mut b = SplitMix64::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
            let f = a.next_f64();
            assert!((0.0..1.0).contains(&f));
            b.next_f64();
        }
        for _ in 0..100 {
            assert!(a.below(5) < 5);
            let r = a.range_i32(-2, 2);
            assert!((-2..=2).contains(&r));
        }
    }

    #[test]
    fn weighted_respects_zero_weights() {
        let mut r = SplitMix64::new(1);
        for _ in 0..200 {
            let i = r.weighted(&[0.0, 3.0, 0.0]).unwrap();
            assert_eq!(i, 1);
        }
        assert_eq!(r.weighted(&[0.0, 0.0]), None);
    }
}
