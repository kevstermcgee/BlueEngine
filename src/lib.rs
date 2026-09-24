#![forbid(unsafe_code)]
pub mod geometry;
pub mod math;
#[cfg(feature = "offline")]
pub mod output;
pub mod prelude;
#[cfg(feature = "offline")]
pub mod render;
pub mod scene;
pub mod viewer;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
