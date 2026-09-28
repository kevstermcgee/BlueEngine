//! An optional shape for games that own their simulation, plus the helpers that make it testable.
//!
//! BlueEngine's stock runner executes a validated `GameDocument`. A game with enemies, projectiles,
//! scoring or per-frame physics keeps its rules in Rust instead. The engine asks the same of both:
//! authoritative state is fixed-step, rendering-free and deterministic, and presentation only reads
//! it. [`Simulation`] names that contract so scripted playback, determinism checks and state traces
//! work for any such game without the engine knowing its rules. Implementing it is optional.
use serde::Serialize;

/// A deterministic, rendering-free, fixed-step game state (one call to [`Simulation::step`] = one
/// 60 Hz tick).
pub trait Simulation {
    /// One tick of player intent. `Default` is "no input".
    type Input: Clone + Default;
    /// Advance exactly one fixed tick.
    fn step(&mut self, input: &Self::Input);
    /// A hash of everything that must replay identically: feed positions, timers, scores and RNG
    /// state to a [`StateHasher`]. Never include wall-clock or presentation state.
    fn state_hash(&self) -> u64;
}

/// FNV-1a 64-bit hash with typed writers, for implementing [`Simulation::state_hash`].
///
/// Floats are hashed by bit pattern with `-0.0` folded to `0.0` and every NaN folded to one value, so
/// two states that compare equal hash equal.
///
/// ```
/// use vesper3d::viewer::devkit::StateHasher;
/// let mut a = StateHasher::new();
/// a.f32(1.5).u32(3);
/// let mut b = StateHasher::new();
/// b.f32(1.5).u32(3);
/// assert_eq!(a.finish(), b.finish());
/// ```
#[derive(Clone, Debug)]
pub struct StateHasher(u64);

impl Default for StateHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl StateHasher {
    /// A fresh hasher.
    pub fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    /// Mix raw bytes.
    pub fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
        self
    }
    /// Mix a `u64`.
    pub fn u64(&mut self, value: u64) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }
    /// Mix a `u32`.
    pub fn u32(&mut self, value: u32) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }
    /// Mix an `i32`.
    pub fn i32(&mut self, value: i32) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }
    /// Mix a `bool`.
    pub fn bool(&mut self, value: bool) -> &mut Self {
        self.bytes(&[u8::from(value)])
    }
    /// Mix an `f32` by bit pattern (see the type documentation).
    pub fn f32(&mut self, value: f32) -> &mut Self {
        let bits = if value.is_nan() {
            0x7fc0_0000
        } else if value == 0. {
            0
        } else {
            value.to_bits()
        };
        self.u32(bits)
    }
    /// Mix a string (length-prefixed, so `"ab" + "c"` differs from `"a" + "bc"`).
    pub fn str(&mut self, value: &str) -> &mut Self {
        self.u64(value.len() as u64).bytes(value.as_bytes())
    }
    /// The hash so far.
    pub fn finish(&self) -> u64 {
        self.0
    }
}

/// The state hash after every tick of a run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Trace {
    /// Number of ticks run.
    pub ticks: usize,
    /// Hash after tick `i` at index `i`.
    pub hashes: Vec<u64>,
}

impl Trace {
    /// Hash after the last tick (or of an empty run: 0).
    pub fn last(&self) -> u64 {
        self.hashes.last().copied().unwrap_or(0)
    }
    /// Index of the first tick at which two traces differ, if any.
    pub fn first_divergence(&self, other: &Trace) -> Option<usize> {
        let common = self.hashes.len().min(other.hashes.len());
        (0..common)
            .find(|i| self.hashes[*i] != other.hashes[*i])
            .or((self.hashes.len() != other.hashes.len()).then_some(common))
    }
}

/// Run `inputs` (one per tick) through `sim` and record the state hash after each tick.
pub fn run_inputs<S: Simulation>(sim: &mut S, inputs: &[S::Input]) -> Trace {
    let hashes = inputs
        .iter()
        .map(|input| {
            sim.step(input);
            sim.state_hash()
        })
        .collect();
    Trace {
        ticks: inputs.len(),
        hashes,
    }
}

/// Build the simulation twice with `make`, run the same inputs, and panic naming the first tick at
/// which the runs diverge. Use it in a test: a failure means something non-deterministic (wall
/// clock, an unseeded RNG, iteration order of a hash map) reached the simulation.
pub fn assert_deterministic<S: Simulation>(make: impl Fn() -> S, inputs: &[S::Input]) {
    let first = run_inputs(&mut make(), inputs);
    let second = run_inputs(&mut make(), inputs);
    if let Some(tick) = first.first_divergence(&second) {
        panic!("simulation is not deterministic: state hashes diverge at tick {tick}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A counter whose step depends only on its input.
    #[derive(Default)]
    struct Counter {
        value: i32,
        ticks: u32,
    }
    impl Simulation for Counter {
        type Input = i32;
        fn step(&mut self, input: &i32) {
            self.value += *input;
            self.ticks += 1;
        }
        fn state_hash(&self) -> u64 {
            let mut h = StateHasher::new();
            h.i32(self.value).u32(self.ticks);
            h.finish()
        }
    }

    #[test]
    fn identical_runs_produce_identical_traces_and_input_changes_diverge() {
        let inputs = [1, 2, 3, 0, -5];
        let a = run_inputs(&mut Counter::default(), &inputs);
        let b = run_inputs(&mut Counter::default(), &inputs);
        assert_eq!(a, b);
        assert_eq!(a.ticks, 5);
        assert_eq!(a.first_divergence(&b), None);
        let other = run_inputs(&mut Counter::default(), &[1, 2, 4, 0, -5]);
        assert_eq!(a.first_divergence(&other), Some(2));
        assert_ne!(a.last(), other.last());
        assert_eq!(
            a.first_divergence(&Trace {
                ticks: 2,
                hashes: a.hashes[..2].to_vec()
            }),
            Some(2)
        );
        assert_eq!(
            Trace {
                ticks: 0,
                hashes: vec![]
            }
            .last(),
            0
        );
        assert_deterministic(Counter::default, &inputs);
    }

    #[test]
    #[should_panic(expected = "diverge at tick 0")]
    fn a_hidden_source_of_nondeterminism_is_caught() {
        use std::cell::Cell;
        thread_local!(static RUNS: Cell<i32> = const { Cell::new(0) });
        struct Leaky(i32);
        impl Simulation for Leaky {
            type Input = ();
            fn step(&mut self, _: &()) {
                self.0 += 1;
            }
            fn state_hash(&self) -> u64 {
                // The second simulation built sees a different "wall clock".
                u64::from(self.0 as u32) + u64::from(RUNS.with(Cell::get) as u32)
            }
        }
        assert_deterministic(
            || {
                RUNS.with(|r| r.set(r.get() + 1));
                Leaky(0)
            },
            &[(), ()],
        );
    }

    #[test]
    fn hasher_treats_equal_floats_alike_and_separates_strings() {
        let hash = |f: f32| {
            let mut h = StateHasher::new();
            h.f32(f);
            h.finish()
        };
        assert_eq!(hash(0.), hash(-0.));
        assert_eq!(hash(f32::NAN), hash(-f32::NAN));
        assert_ne!(hash(1.), hash(1.000_000_1));
        let (mut a, mut b) = (StateHasher::new(), StateHasher::new());
        a.str("ab").str("c");
        b.str("a").str("bc");
        assert_ne!(a.finish(), b.finish());
        let mut c = StateHasher::new();
        c.bool(true).u64(9).bytes(&[1, 2]);
        assert_ne!(c.finish(), StateHasher::new().finish());
    }
}
