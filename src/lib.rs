#![deny(unsafe_code)]
pub mod asset_model;
#[cfg(feature = "schema-generation")]
pub mod authoring_schemas;
#[cfg(feature = "model-import")]
pub mod model_import;
#[cfg(target_arch = "wasm32")]
compile_error!("Browser gameplay/WASM is retired. Use native Windows EXE delivery; see docs/BROWSER_WORKFLOW.md. Portable 2D/3D composition remains supported natively.");
pub mod geometry;
pub mod math;
#[cfg(feature = "offline")]
pub mod output;
pub mod prelude;
#[cfg(feature = "offline")]
pub mod render;
pub mod runtime;
pub mod scene;
pub mod two_d;
/// Shared native authoring path. Compose 2D and 3D presentation freely;
/// deterministic rules, devices, sound and storage stay on the same runtime.
pub mod portable {
    pub use crate::two_d::*;
}
pub mod viewer;

/// The physics library the engine is built on (`rapier3d`, `enhanced-determinism` on), re-exported so a game
/// that wants raw rigid bodies uses exactly the engine's version and features without repeating the pin in
/// its own manifest: `use vesper3d::rapier::prelude::*;`.
pub use rapier3d as rapier;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
