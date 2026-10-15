//! Acceptance A: an existing stock game needs no narrative configuration or feature.
use std::path::Path;
use vesper3d::viewer::{game::GameDocument, game_explore::explore};

#[test]
fn existing_timed_relay_is_playable_without_story_files_or_narrative_state() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/games/timed-relay/content");
    let files = std::fs::read_dir(&root)
        .unwrap()
        .map(|p| p.unwrap().file_name())
        .collect::<Vec<_>>();
    assert!(files.iter().all(|p| !p.to_string_lossy().ends_with(".poe")));
    let game = GameDocument::load(&root.join("game.json")).unwrap();
    let document = serde_json::to_value(&game.document).unwrap();
    assert!(document.get("narrative").is_none());
    let result = explore(&game, 10_000).unwrap();
    assert_eq!(result.winnable, Some(true));
    let world = game.world().unwrap();
    let saved: serde_json::Value =
        serde_json::from_slice(&serde_json::to_vec(world.game.as_ref().unwrap().state()).unwrap())
            .unwrap();
    assert!(saved.get("narrative").is_none());
}
