//! An opt-in client kit for games that draw their own world.
//!
//! The stock runner renders a validated `GameDocument`. A game with its own simulation writes its own
//! renderer, and measured on a real one, more than half of what it wrote was the same generic
//! infrastructure every action game needs: dynamic batched meshes with materials, effects, HUD helpers,
//! sound. This module is that infrastructure, each piece independent, with no genre logic:
//!
//! | Piece | Job |
//! |---|---|
//! | [`View`] | camera description: eye, yaw/pitch/roll, FOV, screen projection |
//! | [`Template`], [`Batch`], [`Tint`] | build small meshes once, pack hundreds per frame into a few draw calls |
//! | [`Look`], [`Materials`] | lit + fogged + glowing world material, alpha and additive effect materials, a sky |
//! | [`Fx`] | particles, rings, fireballs, beams, popups, banners |
//! | [`hud`] | scaled outlined text, panels, bars, vignette, crosshair, popups and banners |
//! | [`SoundBank`] | effects with variants and music stems, rendered off-thread |
//! | [`capture::save_frame`] | screenshot evidence with the real pixel size |
//!
//! Headless companions live in [`devkit`](crate::viewer::devkit) (clock, input accumulator, playback,
//! saves, juice, synthesised audio). See `docs/CUSTOM_CLIENT.md` for the loop that ties them together.
pub mod audio;
pub mod batch;
pub mod capture;
pub mod fx;
pub mod hud;
pub mod look;
pub mod view;

pub use audio::{Rendered, SoundBank};
pub use batch::{Batch, Rgb, Template, Tint, Vert, MAX_MESH_INDICES, MAX_MESH_VERTICES};
pub use fx::{Banner, Fx, Particle, ParticleKind, Popup};
pub use look::{Look, Materials};
pub use view::View;
