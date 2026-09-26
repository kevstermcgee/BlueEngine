pub mod protocol;
pub mod server;

use std::path::{Path, PathBuf};

pub const DEFAULT_SERVER: &str = "127.0.0.1:4100";
pub const DEFAULT_KEY: &str = "bluedm-family";

pub fn content_path() -> PathBuf {
    let local = Path::new("maps/foundry.json");
    if local.is_file() {
        local.to_owned()
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("maps/foundry.json")
    }
}
