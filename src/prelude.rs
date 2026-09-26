//! Small rendering-independent surface for prototypes. See `docs/AI_QUICKSTART.md`.
pub use crate::math::V;
pub use crate::viewer::authoring::MapDocument;
pub use crate::viewer::builder::SceneBuilder;
pub use crate::viewer::controller::{Controller, Movement};
pub use crate::viewer::fps::{
    AimState, FireMode, OperativeModel, Team, TeamDeathmatch, TeamDeathmatchConfig, WeaponCatalog,
    WeaponDefinition, WeaponState,
};
pub use crate::viewer::simulation::{HeadlessWorld, TICK_SECONDS};
pub use crate::Result;
