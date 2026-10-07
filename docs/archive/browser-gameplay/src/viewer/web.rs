//! Browser builds expose shared contracts, not native 3D/UDP/QUIC services.
pub mod controller;
pub mod identity;
#[cfg(feature = "presentation")]
#[path = "web_kit.rs"]
pub mod kit;
pub mod profile;
pub mod savestate;
#[path = "devkit/scale.rs"]
mod scale;
pub mod camera {
    pub use crate::runtime::camera_boom::sweep_boom;
}
pub mod devkit {
    pub use super::scale::{
        describe_length, Bounds, DOOR, EYE_HEIGHT, HUMAN_HEIGHT, ONE_HAND_LONGEST, TABLE_HEIGHT,
    };
    pub use crate::runtime::shadow_quality::ShadowQuality;

    pub use crate::runtime::clock::TICK;
    pub use crate::runtime::playback::{flag_value, has_flag};
    pub use crate::runtime::*;
    pub fn beside_exe(name: &str) -> std::path::PathBuf {
        name.into()
    }
}
