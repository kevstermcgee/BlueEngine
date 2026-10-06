//! Shared rendering-free contracts. 2D and 3D use the same simulation, RNG and save frame.
#[path = "../viewer/devkit/audio_project.rs"]
pub mod audio_project;
#[path = "../viewer/devkit/clock.rs"]
pub mod clock;
pub mod hash;
#[path = "../viewer/devkit/input.rs"]
pub mod input;
#[path = "../viewer/devkit/playback.rs"]
pub mod playback;
#[path = "../viewer/devkit/procedural.rs"]
pub mod procedural;
#[path = "../viewer/devkit/rng.rs"]
pub mod rng;
#[path = "../viewer/devkit/sim.rs"]
pub mod sim;
#[path = "../viewer/devkit/snapshot.rs"]
pub mod snapshot;
pub mod storage;
#[path = "../viewer/devkit/synth.rs"]
pub mod synth;
pub use clock::FixedStepper;
pub use input::InputAccumulator;
pub use rng::Rng;
pub use sim::{assert_deterministic, run_inputs, Simulation, StateHasher};
pub use snapshot::{SavePolicy, Snapshot};
pub mod camera_boom;
pub mod shadow_quality;
