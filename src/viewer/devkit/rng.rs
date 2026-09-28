//! Tiny deterministic random numbers for simulations, effects and procedural audio.
//!
//! Everything random in a fixed-step simulation should go through one seeded [`Rng`], so a seed
//! reproduces a whole run bit for bit (tests, bots, replays and the balance surveys depend on it).
//! The generator is xorshift64* seeded through SplitMix64. It is not cryptographic and must never
//! guard anything; the network layer has its own secure randomness.
use serde::{Deserialize, Serialize};

/// Seeded xorshift64* generator. `Clone` snapshots the stream, so a copy replays it. It serialises as one
/// number (its state), so a simulation's random stream goes into a save file and comes back at the same
/// point; a state of zero, which would repeat forever, is refused when reading.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct Rng(u64);

impl TryFrom<u64> for Rng {
    type Error = &'static str;
    fn try_from(state: u64) -> Result<Self, Self::Error> {
        Self::from_state(state).ok_or("a random generator state of zero never changes")
    }
}
impl From<Rng> for u64 {
    fn from(rng: Rng) -> u64 {
        rng.0
    }
}

impl Rng {
    /// Any seed is fine, including zero: it is mixed first so small seeds give unrelated streams.
    pub fn new(seed: u64) -> Self {
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        Self(if z == 0 { 0x2545_F491_4F6C_DD1D } else { z })
    }
    /// The generator's whole state; [`Rng::from_state`] resumes the stream from it.
    pub fn state(&self) -> u64 {
        self.0
    }
    /// Resume a stream saved with [`Rng::state`]. `None` for zero, which is not a reachable state.
    pub fn from_state(state: u64) -> Option<Self> {
        (state != 0).then_some(Self(state))
    }
    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in `[0, 1)`.
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }
    /// Uniform integer in `[0, n)`. Returns 0 for `n == 0`.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
    /// True with probability `p`.
    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }
    /// -1.0 or 1.0.
    pub fn sign(&mut self) -> f32 {
        if self.next_u64() & 1 == 0 {
            -1.
        } else {
            1.
        }
    }
    /// A random element, or `None` for an empty slice.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.below(items.len())])
        }
    }
    /// In-place Fisher-Yates shuffle.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream_different_seed_differs() {
        let (mut a, mut b, mut c) = (Rng::new(7), Rng::new(7), Rng::new(8));
        let sa: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
        let sb: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
        let sc: Vec<u64> = (0..8).map(|_| c.next_u64()).collect();
        assert_eq!(sa, sb);
        assert_ne!(sa, sc);
    }

    #[test]
    fn state_and_serde_resume_the_stream_and_refuse_zero() {
        let mut a = Rng::new(5);
        for _ in 0..17 {
            a.next_u64();
        }
        let mut b = Rng::from_state(a.state()).unwrap();
        let mut c: Rng = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        for _ in 0..100 {
            let next = a.next_u64();
            assert_eq!((b.next_u64(), c.next_u64()), (next, next));
        }
        assert!(Rng::from_state(0).is_none());
        assert!(serde_json::from_str::<Rng>("0").is_err());
        assert_eq!(
            serde_json::to_string(&Rng::from_state(u64::MAX).unwrap()).unwrap(),
            u64::MAX.to_string()
        );
    }

    #[test]
    fn zero_is_a_valid_seed_and_a_clone_replays_the_stream() {
        let mut a = Rng::new(0);
        let mut snapshot = a.clone();
        assert_ne!(a.next_u64(), 0);
        let first = a.next_u64();
        snapshot.next_u64();
        assert_eq!(snapshot.next_u64(), first);
    }

    #[test]
    fn floats_stay_in_range_and_cover_it() {
        let mut r = Rng::new(1);
        let (mut lo, mut hi) = (1.0f32, 0.0f32);
        for _ in 0..10_000 {
            let x = r.f32();
            assert!((0. ..1.).contains(&x));
            lo = lo.min(x);
            hi = hi.max(x);
        }
        assert!(lo < 0.01 && hi > 0.99);
        assert_eq!(r.below(0), 0);
        assert!((0..100).all(|_| r.below(5) < 5));
        assert!((0..100).all(|_| (2.0..3.0).contains(&r.range(2., 3.))));
    }

    #[test]
    fn shuffle_keeps_every_element() {
        let mut r = Rng::new(3);
        let mut v: Vec<u32> = (0..20).collect();
        r.shuffle(&mut v);
        let mut sorted = v.clone();
        sorted.sort();
        assert_eq!(sorted, (0..20).collect::<Vec<_>>());
        assert_ne!(v, (0..20).collect::<Vec<_>>());
        assert_eq!(r.pick::<u8>(&[]), None);
    }
}
