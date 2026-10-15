//! The game's identity is one file (`assets/identity.json`); its window title, game document and icon
//! assets must agree, so the window, the desktop shortcut and the exe's version info can never drift.
//! Placeholder titles are not rejected here (`scripts/check.py` does that at the ship gate).
use vesper3d::viewer::{game::GameDocument, identity::Identity};

fn root(file: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(file)
}

#[test]
fn identity_icon_and_game_document_agree() {
    let text = std::fs::read_to_string(root("assets/identity.json")).expect("assets/identity.json");
    let identity: Identity = serde_json::from_str(&text).expect("assets/identity.json is valid JSON");
    assert!(!identity.title.trim().is_empty(), "give the game a title in assets/identity.json");
    if root("game.json").is_file() {
        let game = GameDocument::load(&root("game.json")).unwrap();
        assert_eq!(game.document.name, identity.title, "game.json name and identity title must match");
    }
    // The window icon blobs must be exactly the sizes miniquad wants (the include_bytes! in main.rs
    // would not compile otherwise; this also catches a stale copy).
    for (size, bytes) in [(16, 1024), (32, 4096), (64, 16384)] {
        let blob = std::fs::read(root(&format!("assets/icon_{size}.rgba"))).unwrap();
        assert_eq!(blob.len(), bytes, "assets/icon_{size}.rgba is {size}x{size} RGBA");
    }
    assert!(std::fs::metadata(root("assets/icon.ico")).unwrap().len() > 1000, "assets/icon.ico");
}
