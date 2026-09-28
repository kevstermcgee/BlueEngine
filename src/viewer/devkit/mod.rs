//! Building blocks for games that own their simulation.
//!
//! The stock runner (`playable::run_game_with_options`) plays a validated `GameDocument`. A game
//! with enemies, projectiles, scoring, AI or per-frame physics keeps its rules in Rust and owns its
//! window loop. Everything a hand-written loop needs beyond drawing lives here, is graphics-free,
//! and is what the `custom-sim` starter (`be2-tools new-game NAME DIR ENGINE_PATH custom-sim`) uses.
//!
//! AI-BOUNDARY ARCH-DEVKIT-001: devkit is graphics-free and always compiled: no macroquad, no audio
//! device and no wall clock inside simulation helpers (`python tools/check_headless.py` enforces the
//! dependency half).
//!
//! | Piece | Job |
//! |---|---|
//! | [`FrameClock`] | wall-clock frame length (macroquad's `get_frame_time()` is bumpy under vsync) |
//! | [`FixedStepper`] | frame lengths in, whole 60 Hz ticks out, bounded catch-up, render `alpha` |
//! | [`InputAccumulator`] | device frames in, exactly one input per tick out (edges once, look immediate) |
//! | [`Simulation`], [`StateHasher`], [`assert_deterministic`] | the deterministic-state contract and its test |
//! | [`Playback`], [`Timeline`], [`CapturePlan`] | scripted input and screenshot flags for an agent that cannot play |
//! | [`PerfReport`] | frame-time percentiles for a `--perf` flag |
//! | [`Rng`] | seeded random numbers, so a seed replays a run |
//! | [`Juice`], [`Pulse`] | screen shake, hit-stop, FOV kick, flash, landing dip |
//! | [`Settings`], [`Records`], [`load_or_default`], [`store_atomic`] | atomic, never-fatal settings and high-score files |
//! | [`Snapshot`], [`snapshot`] | save states of the simulation: F5 / F9, autosaves, migrations, all-or-nothing loads |
//! | [`synth`] | procedural sound effects, a music loop, WAV writer and loudness/pitch measurement: audio with no recordings |
//!
//! The simulation stays authoritative and rendering-free; the window only reads it. One frame of a
//! custom loop looks like this (this example runs headless):
//!
//! ```
//! use vesper3d::viewer::devkit::{FixedStepper, FrameClock, InputAccumulator, Simulation};
//! use std::time::Duration;
//!
//! #[derive(Default)]
//! struct Walker { x: f32 }
//! impl Simulation for Walker {
//!     type Input = f32; // forward axis
//!     fn step(&mut self, forward: &f32) { self.x += forward / 60.; }
//!     fn state_hash(&self) -> u64 { u64::from(self.x.to_bits()) }
//! }
//!
//! let (mut clock, mut stepper) = (FrameClock::new(), FixedStepper::new());
//! let mut input = InputAccumulator::<f32>::new();
//! let mut sim = Walker::default();
//! for _ in 0..120 {
//!     let dt = clock.tick_after(Duration::from_secs_f32(1. / 120.)); // a 120 Hz display
//!     input.feed(1.0, 0, [0.; 2]); // held forward; read the real devices here
//!     for _ in 0..stepper.advance(dt) {
//!         sim.step(&input.take_tick().held);
//!     }
//!     // draw(&sim, stepper.alpha()) goes here
//! }
//! assert!((sim.x - 1.0).abs() < 0.03, "one second of walking at 60 ticks/s");
//! ```
mod clock;
mod input;
mod juice;
mod playback;
mod rng;
mod save;
mod sim;
pub mod snapshot;
pub mod synth;

pub use crate::viewer::savestate::{
    SaveError, SaveHeader, SaveSlots, Source, AUTO_SLOT, QUICK_SLOT,
};
pub use clock::{FixedStepper, FrameClock, PerfReport, PerfSummary, MAX_FRAME, TICK, TICK_RATE};
pub use input::{clean_axis, Edges, InputAccumulator, Tick};
pub use juice::{Juice, Pulse, MAX_HITSTOP};
pub use playback::{
    flag_value, has_flag, parse_frame_list, parse_size, CapturePlan, Cue, Playback, Timeline,
};
pub use rng::Rng;
pub use save::{beside_exe, load_or_default, store_atomic, Records, Settings};
pub use sim::{assert_deterministic, run_inputs, Simulation, StateHasher, Trace};
pub use snapshot::{Migration, Snapshot};
