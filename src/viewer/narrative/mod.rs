//! Opt-in, deterministic contextual responses and bounded authored interactions.
//!
//! Games own gameplay facts, atomic command implementations, fixed-step scheduling,
//! presentation, and snapshots. This module never ticks, watches files, opens devices,
//! consumes random numbers, or saves files by itself. Enable Cargo's `narrative`
//! feature only for a consumer or authoring tool that needs these facilities.
mod model;
mod parser;
mod runtime;

pub use model::*;
pub use parser::*;
pub use runtime::*;
