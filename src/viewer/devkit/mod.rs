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
//! | [`FpsCamera`], [`MouseLook`] | the one mouse-look convention (hand right = turn right, hand up = look up, `invert_y`, pitch limit), tested against `Controller::look` |
//! | [`Simulation`], [`StateHasher`], [`assert_deterministic`] | the deterministic-state contract and its test |
//! | [`Playback`], [`Timeline`], [`CapturePlan`] | scripted input and screenshot flags for an agent that cannot play |
//! | [`PerfReport`] | frame-time percentiles for a `--perf` flag |
//! | [`Rng`] | seeded random numbers, so a seed replays a run |
//! | [`MenuNav`], [`MenuStep`] | stick flick / D-pad to discrete menu steps with hysteresis and auto-repeat (no device dependency) |
//! | [`Bounds`], [`describe_length`], [`HUMAN_HEIGHT`] | metres-scale checks: `bounds.expect_longest("conch", 0.05..=0.30)` fails with the scale factor to apply, so a giant shell is caught without a window |
//! | [`Juice`], [`Pulse`] | screen shake, hit-stop, FOV kick, flash, landing dip |
//! | [`Settings`], [`Records`], [`load_or_default`], [`store_atomic`] | atomic, never-fatal settings and high-score files |
//! | [`Snapshot`], [`snapshot`], [`SavePolicy`] | save states of the simulation: F5 / F9, autosaves, migrations, all-or-nothing loads, and the resume contract a game declares (exact, or a physics continuation) |
//! | [`Lifecycle`] | the pieces above composed: flags in, one input per tick, quick save/load, capture and perf evidence out, so `main.rs` keeps only drawing and device mapping |
//! | [`path::ClosedPath`] | a smooth closed loop (track, patrol route): length, point and tangent at a distance, progress and lateral offset of any position, arc distance with wrap-around, plus the yaw helpers `forward`, `right`, `yaw_of`, `yaw_facing`, `wrap_angle` |
//! | [`wall_along_x`], [`wall_along_z`] | straight walls with doorway gaps cut out, for a hand-built indoor level (a `MapDocument` is not the only way to make rooms) |
//! | [`WaypointGraph`] | a small walkable graph (doorway thresholds, room centres) with breadth-first shortest-path and nearest-node lookup, for an NPC or monster moving through such a level — unlike `ClosedPath`'s single loop, this one can branch |
//! | [`TextField`], [`CharFilter`] | a single-line text box as pure logic: sanitised typing and paste, caret, hold-to-repeat (`TextField::feed_frame` reads the keyboard and clipboard) |
//! | [`resolve_ipv4`], [`ServerChoice`] | a typed server address to an IPv4 `SocketAddr` with player-facing errors, and the default-server chain (`--connect`, `server.txt`, last used, built in) |
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
mod level;
mod lifecycle;
mod look;
mod menu;
pub mod net_address;
pub mod path;
mod playback;
mod rng;
mod save;
mod scale;
mod sim;
pub mod snapshot;
pub mod synth;
pub mod text_field;
mod waypoints;

pub use crate::viewer::savestate::{
    SaveError, SaveHeader, SaveSlots, Source, AUTO_SLOT, QUICK_SLOT,
};
pub use clock::{FixedStepper, FrameClock, PerfReport, PerfSummary, MAX_FRAME, TICK, TICK_RATE};
pub use input::{clean_axis, Edges, InputAccumulator, Tick};
pub use juice::{Juice, Pulse, MAX_HITSTOP};
pub use level::{solid_ranges, wall_along_x, wall_along_z};
pub use lifecycle::{Lifecycle, Notice, Options, ScriptFrame};
pub use look::{
    stick_look, FpsCamera, MouseLook, DEFAULT_RADIANS_PER_PIXEL, PITCH_LIMIT, SENSITIVITY_RANGE,
    STICK_RADIANS_PER_SECOND,
};
pub use menu::{MenuNav, MenuStep, FLICK, RELEASE, REPEAT_DELAY, REPEAT_INTERVAL};
pub use net_address::{
    resolve_ipv4, resolve_ipv4_with, AddressError, ServerChoice, ServerOrigin, ServerSource,
};
pub use playback::{
    flag_value, has_flag, parse_frame_list, parse_size, CapturePlan, Cue, Playback, Timeline,
};
pub use rng::Rng;
pub use save::{
    beside_exe, downloads_dir, load_or_default, sanitize_filename, store_atomic, unique_path,
    Records, Settings, ShadowQuality,
};
pub use scale::{
    describe_length, Bounds, DOOR, EYE_HEIGHT, HUMAN_HEIGHT, ONE_HAND_LONGEST, TABLE_HEIGHT,
};
pub use sim::{assert_deterministic, run_inputs, Simulation, StateHasher, Trace};
pub use snapshot::{assert_resumes_as_promised, Migration, SavePolicy, Snapshot};
pub use text_field::{CharFilter, EditKey, TextField};
pub use waypoints::{Waypoint, WaypointGraph};
