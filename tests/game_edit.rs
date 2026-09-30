//! `add-interactable`: one description creates the five records a game needs, or nothing at all.
use std::path::{Path, PathBuf};
use vesper3d::{
    math::V,
    viewer::{
        game::GameDocument,
        game_edit::{add_interactable, InteractableSpec},
        game_example,
    },
};

/// The three-switches example, written to a fresh directory.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("be2-game-edit-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    game_example::write(&dir).unwrap();
    dir
}
fn spec(id: &str, x: f32) -> InteractableSpec {
    InteractableSpec::new(id, V(x, 1.5, -2.))
}
fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

#[test]
fn a_dry_run_reports_the_records_and_changes_nothing() {
    let dir = scratch("dry");
    let game = dir.join("game.json");
    let before = (read(&game), read(&dir.join("map.json")));
    let report = add_interactable(&game, &spec("vent", 6.), false).unwrap();
    assert!(!report.written);
    assert_eq!((read(&game), read(&dir.join("map.json"))), before);
    for key in [
        "map.scene.materials",
        "map.scene.nodes",
        "map.colliders",
        "map.entities",
        "game.interactables",
    ] {
        assert!(
            !report.records[key].is_null(),
            "{key} missing from {}",
            report.records
        );
    }
}

#[test]
fn writing_produces_a_game_that_loads_and_a_rule_can_use() {
    let dir = scratch("write");
    let game = dir.join("game.json");
    let mut wanted = spec("vent", 6.);
    wanted.label = Some("Air vent".into());
    wanted.enabled = false;
    let report = add_interactable(&game, &wanted, true).unwrap();
    assert!(report.written);
    let loaded = GameDocument::load(&game).unwrap();
    let declared = loaded.document.interactables.last().unwrap();
    assert_eq!(
        (declared.entity.as_str(), declared.enabled),
        ("vent", false)
    );
    let entity = loaded.map.entities.iter().find(|e| e.id == "vent").unwrap();
    assert_eq!(entity.label, "Air vent");
    assert_eq!(loaded.map.colliders["vent"].min, entity.bounds.min);
    assert!(loaded.map.scene.nodes.iter().any(|n| n.id == "vent"));
    assert!(loaded.map.scene.materials.contains_key("edit-vent"));
    // No temporary files are left behind.
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn a_rule_less_interactable_is_flagged_but_timer_rules_do_not_count_as_reacting() {
    let dir = scratch("warn");
    let game = dir.join("game.json");
    let mut doc: serde_json::Value = serde_json::from_slice(&read(&game)).unwrap();
    // A rule that only listens to a timer must not silence the warning.
    doc["timers"] =
        serde_json::json!([{"id":"t","duration_ticks":30,"auto_start":true,"repeats":true}]);
    doc["rules"] = serde_json::json!([{"id":"tick","on_timer":"t","condition":null,"once":false,
        "actions":[{"action":"increment","counter":"switches","amount":1}]}]);
    std::fs::write(&game, serde_json::to_vec_pretty(&doc).unwrap()).unwrap();
    let report = add_interactable(&game, &spec("vent", 6.), false).unwrap();
    assert!(
        report.warnings.iter().any(|w| w.contains("No rule reacts")),
        "{:?}",
        report.warnings
    );
    assert_eq!(report.rule_template["on_interact"], "vent");

    // A rule aimed at this exact target does silence it.
    doc["rules"].as_array_mut().unwrap().push(serde_json::json!({"id":"press","on_interact":"vent",
        "condition":null,"once":true,"actions":[{"action":"increment","counter":"switches","amount":1}]}));
    std::fs::write(&game, serde_json::to_vec_pretty(&doc).unwrap()).unwrap();
    // The game is validated first, so the (still unknown) target has to exist: add it, then check quietly.
    let error = add_interactable(&game, &spec("vent", 6.), false);
    assert!(
        error.is_err(),
        "a rule may not name a target that does not exist yet"
    );
}

#[test]
fn overlapping_an_existing_interactable_is_warned_about() {
    let dir = scratch("overlap");
    let game = dir.join("game.json");
    let loaded = GameDocument::load(&game).unwrap();
    let first = loaded
        .map
        .entities
        .iter()
        .find(|e| e.id == "button-a")
        .unwrap();
    let centre = (first.bounds.min + first.bounds.max) * 0.5;
    let report = add_interactable(&game, &InteractableSpec::new("twin", centre), false).unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("overlaps interactable button-a")),
        "{:?}",
        report.warnings
    );
    let clear = add_interactable(&game, &spec("apart", 6.), false).unwrap();
    assert!(clear.warnings.iter().all(|w| !w.contains("overlaps")));
}

#[test]
fn refusals_leave_both_files_untouched() {
    let dir = scratch("refuse");
    let game = dir.join("game.json");
    let before = (read(&game), read(&dir.join("map.json")));
    let mut bad_colour = spec("x", 6.);
    bad_colour.color = V(2., 0., 0.);
    let mut flat = spec("y", 6.);
    flat.half_extents = V(0., 0.3, 0.3);
    for (label, attempt) in [
        ("duplicate interactable", spec("button-a", 6.)),
        ("occupied map id", spec("lab-terminal", 6.)),
        ("bad id", spec("has space", 6.)),
        ("colour out of range", bad_colour),
        ("zero-size box", flat),
    ] {
        assert!(
            add_interactable(&game, &attempt, true).is_err(),
            "{label} should be refused"
        );
    }
    assert_eq!((read(&game), read(&dir.join("map.json"))), before);
}

/// If the game cannot be replaced after the map was, the map goes back to what it was.
#[cfg(unix)]
#[test]
fn a_failed_game_write_restores_the_map() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("rollback");
    // Move the map into a writable subdirectory, then make the game's own directory read-only.
    std::fs::create_dir(dir.join("maps")).unwrap();
    std::fs::rename(dir.join("map.json"), dir.join("maps/map.json")).unwrap();
    let game = dir.join("game.json");
    let mut doc: serde_json::Value = serde_json::from_slice(&read(&game)).unwrap();
    doc["map"] = "maps/map.json".into();
    std::fs::write(&game, serde_json::to_vec_pretty(&doc).unwrap()).unwrap();
    let map_before = read(&dir.join("maps/map.json"));
    let game_before = read(&game);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let probe = std::fs::File::create(dir.join("probe"));
    if probe.is_ok() {
        // Running as a user the read-only bit does not bind (e.g. root): nothing to test here.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let outcome = add_interactable(&game, &spec("vent", 6.), true);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let error = outcome
        .expect_err("the game file cannot be replaced")
        .to_string();
    assert!(error.contains("the map was left as it was"), "{error}");
    assert_eq!(read(&dir.join("maps/map.json")), map_before);
    assert_eq!(read(&game), game_before);
}
