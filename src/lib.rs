#![deny(unsafe_code)]
#[cfg(all(target_arch = "wasm32", any(feature = "client", feature = "offline")))]
compile_error!("Browser games require default-features=false and features=[two-d]. Use scripts/blue web build from the two-d starter; native 3D/client/offline features are not supported on WASM.");
#[cfg(not(target_arch = "wasm32"))]
pub mod geometry;
#[cfg(not(target_arch = "wasm32"))]
pub mod math;
#[cfg(feature = "offline")]
pub mod output;
#[cfg(not(target_arch = "wasm32"))]
pub mod prelude;
#[cfg(feature = "offline")]
pub mod render;
pub mod runtime;
#[cfg(not(target_arch = "wasm32"))]
pub mod scene;
pub mod two_d;
#[cfg(not(target_arch = "wasm32"))]
pub mod viewer;
#[cfg(target_arch = "wasm32")]
#[path = "viewer/web.rs"]
pub mod viewer;

/// The physics library the engine is built on (`rapier3d`, `enhanced-determinism` on), re-exported so a game
/// that wants raw rigid bodies uses exactly the engine's version and features without repeating the pin in
/// its own manifest: `use vesper3d::rapier::prelude::*;`.
#[cfg(not(target_arch = "wasm32"))]
pub use rapier3d as rapier;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
