#![forbid(unsafe_code)]
pub mod geometry;
pub mod math;
pub mod output;
pub mod render;
pub mod scene;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
