//! Browser builds expose shared contracts, not native 3D/UDP/QUIC services.
pub mod identity;
pub mod savestate;
pub mod devkit {
    pub use crate::runtime::playback::{flag_value, has_flag};
    pub use crate::runtime::*;
    pub fn beside_exe(name: &str) -> std::path::PathBuf {
        name.into()
    }
}
