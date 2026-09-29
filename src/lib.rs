#![deny(unsafe_code)]
pub mod geometry;
pub mod math;
#[cfg(feature = "offline")]
pub mod output;
pub mod prelude;
#[cfg(feature = "offline")]
pub mod render;
pub mod scene;
pub mod viewer;

/// The physics library the engine is built on (`rapier3d`, `enhanced-determinism` on), re-exported so a game
/// that wants raw rigid bodies uses exactly the engine's version and features without repeating the pin in
/// its own manifest: `use vesper3d::rapier::prelude::*;`.
pub use rapier3d as rapier;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
