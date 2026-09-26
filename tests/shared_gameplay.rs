//! External callers use only the public engine API; no executable-source imports.
use vesper3d::{
    prelude::*,
    viewer::{controller::CharacterKind, creative, game::GameDocument, newgame::scaffold_new_game},
};
fn base() -> MapDocument {
    SceneBuilder::new("Base")
        .spawn(V(0., 0., 8.), 0.)
        .structural_box("floor", V(0., -0.1, 0.), V(20., 0.1, 20.), V::ONE)
        .build()
        .unwrap()
}
#[test]
fn arbitrary_asset_roots_place_and_remove_without_copied_sandbox_code() {
    let source = SceneBuilder::new("Kit")
        .spawn(V(0., 0., 8.), 0.)
        .structural_box("floor", V(0., -0.1, 0.), V(20., 0.1, 20.), V::ONE)
        .box_body("crate", V(0., 0.5, 0.), V(0.5, 0.5, 0.5), V::ONE)
        .build()
        .unwrap();
    let asset = creative::extract(source, "crate").unwrap();
    let base = base();
    let player = Controller::for_character_at(CharacterKind::Scientist, V(0., 0., 8.), 0.).unwrap();
    let edited = creative::place(&base, &asset, V(5., 0., 0.), 1, &player).unwrap();
    assert!(edited.entities.iter().any(|e| e.id == "creative-1"));
    let restored = creative::remove(&edited, "creative-1").unwrap();
    assert_eq!(
        serde_json::to_value(restored).unwrap(),
        serde_json::to_value(base).unwrap()
    );
}
#[test]
fn history_and_save_paths_have_bounds() {
    let mut history = creative::History::default();
    for _ in 0..20 {
        history.remember(base());
    }
    let mut count = 0;
    while history.pop().is_some() {
        count += 1;
    }
    assert_eq!(count, 16);
    for id in ["", "../escape", "a/b", "a\\b", "C:drive"] {
        assert!(creative::save_path(std::path::Path::new("saves"), id).is_err());
    }
    assert_eq!(
        creative::save_path(std::path::Path::new("saves"), "world-1").unwrap(),
        std::path::Path::new("saves/world-1.json")
    );
}
#[test]
fn generated_project_has_a_valid_game_document_and_shared_playable_entry() {
    let dir = std::env::temp_dir().join(format!(
        "blueengine-shared-starter-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    scaffold_new_game("shared-starter", &dir, Some("../BlueEngine")).unwrap();
    GameDocument::load(&dir.join("game.json"))
        .unwrap()
        .world()
        .unwrap();
    let main = std::fs::read_to_string(dir.join("src/main.rs")).unwrap();
    assert!(main.contains("local_client::run_map"));
    assert!(!main.contains("src/bin/sandbox"));
    assert!(scaffold_new_game("shared-starter", &dir, None).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
