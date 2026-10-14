use automation_workshop::{content, persistence, Definitions, Workshop};
use std::path::PathBuf;
use vesper3d::runtime::{snapshot, Simulation, Snapshot};
use vesper3d::viewer::savestate::{decode, encode};
fn temporary(name: &str) -> PathBuf {
    let p =
        std::env::temp_dir().join(format!("workshop-{name}-{}-{:?}", std::process::id(), std::thread::current().id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}
#[test]
fn real_v1_save_keeps_queued_frozen_work_and_original_file() {
    let original = include_bytes!("fixtures/v1.be2save");
    let frame = decode(original).unwrap();
    assert_eq!(frame.header.version, 1);
    assert_eq!(frame.header.tick, 3);
    let mut old = Workshop::new(Definitions::from_json(content::V1_CONTENT, "frozen-v1").unwrap(), 2).unwrap();
    snapshot::restore(&mut old, original).unwrap();
    let expected = old.capture();
    assert_eq!(expected.machines["workshop:machine-a"].queue.len(), 2);
    let directory = temporary("good");
    let _ = std::fs::remove_file(directory.join("migrated-v2.be2save"));
    let mut current = Workshop::standard();
    persistence::migrate_v1(&mut current, original, &directory).unwrap();
    assert_eq!(current.capture(), expected);
    let copy = std::fs::read(directory.join("migrated-v2.be2save")).unwrap();
    let migrated = decode(&copy).unwrap();
    assert_eq!(migrated.header.version, 2);
    assert_eq!(migrated.header.content, current.definitions.fingerprint());
    assert_eq!(original, include_bytes!("fixtures/v1.be2save"));
    for _ in 0..12 {
        old.step(&vec![]);
        current.step(&vec![]);
        assert_eq!(old.state_hash(), current.state_hash());
    }
    let before = current.capture();
    assert!(persistence::migrate_v1(&mut current, original, &directory).is_err());
    assert_eq!(current.capture(), before);
    assert_eq!(std::fs::read(directory.join("migrated-v2.be2save")).unwrap(), copy);
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn direct_content_mismatch_corruption_future_schema_and_bad_source_preserve_live() {
    let original = include_bytes!("fixtures/v1.be2save");
    let mut sim = Workshop::standard();
    let before = sim.capture();
    assert!(snapshot::restore(&mut sim, original).is_err());
    assert_eq!(sim.capture(), before);
    let mut corrupt = original.to_vec();
    corrupt[40] ^= 1;
    let dir = temporary("failure");
    assert!(persistence::migrate_v1(&mut sim, &corrupt, &dir).is_err());
    assert_eq!(sim.capture(), before);
    assert!(!dir.join("migrated-v2.be2save").exists());
    let current = snapshot::save(&sim, "Current").unwrap();
    let frame = decode(&current).unwrap();
    let mut header = frame.header;
    header.version = 99;
    let unsupported = encode(&header, frame.payload).unwrap();
    assert!(snapshot::restore(&mut sim, &unsupported).unwrap_err().to_string().contains("99"));
    assert_eq!(sim.capture(), before);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn valid_engine_frame_with_wrong_old_state_hash_refuses_copy_migration() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v1.be2save");
    let original = std::fs::read(path).unwrap();
    let frame = decode(&original).unwrap();
    let mut payload: serde_json::Value = serde_json::from_slice(frame.payload).unwrap();
    payload["hash"] = serde_json::json!("0000000000000000");
    let forged = encode(&frame.header, &serde_json::to_vec(&payload).unwrap()).unwrap();
    assert!(decode(&forged).is_ok(), "framing/checksum alone intentionally remains valid");
    let mut sim = Workshop::standard();
    let before = sim.capture();
    let dir = temporary("wrong-old-hash");
    let error = persistence::migrate_v1(&mut sim, &forged, &dir).unwrap_err();
    assert!(error.contains("saved state"), "{error}");
    assert_eq!(sim.capture(), before);
    assert!(!dir.join("migrated-v2.be2save").exists());
    assert_eq!(std::fs::read(path).unwrap(), original);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn migration_output_io_failure_preserves_original_live_and_completed_file() {
    let original_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v1.be2save");
    let original = std::fs::read(original_path).unwrap();
    let mut sim = Workshop::standard();
    let before = sim.capture();
    let hash = sim.state_hash();
    let dir = temporary("output-io-failure");
    let destination = dir.join("blocked-output-path");
    let completed = b"Already completed unrelated output";
    std::fs::write(&destination, completed).unwrap();
    let error = persistence::migrate_v1(&mut sim, &original, &destination).unwrap_err();
    assert!(error.contains("blocked-output-path"), "{error}");
    assert_eq!(sim.capture(), before);
    assert_eq!(sim.state_hash(), hash);
    assert_eq!(std::fs::read(&destination).unwrap(), completed);
    assert_eq!(std::fs::read(original_path).unwrap(), original);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn incompatible_v2_recipe_and_upgrade_retuning_refuse_copy_update() {
    let old = Workshop::standard();
    let original = snapshot::save(&old, "Before retuning").unwrap();
    let original_state = old.capture();
    let original_hash = old.state_hash();
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/workshop.json")).unwrap();
    let base: serde_json::Value = serde_json::from_str(&text).unwrap();
    for group in ["recipes", "upgrades"] {
        let mut doc = base.clone();
        let entry = &mut doc[group].as_array_mut().unwrap()[0];
        let field = if group == "recipes" { "duration_ticks" } else { "duration_divisor" };
        entry[field] = serde_json::json!(entry[field].as_u64().unwrap() + 1);
        let target = Definitions::from_json(&doc.to_string(), "fixture:incompatible-retuning").unwrap();
        let mut live = Workshop::new(target, 2).unwrap();
        let before = live.capture();
        let hash = live.state_hash();
        let dir = temporary(&format!("retuning-{group}"));
        let original_path = dir.join("original-v2.be2save");
        std::fs::write(&original_path, &original).unwrap();
        let error =
            persistence::migrate_content(&mut live, &original, old.definitions.clone(), &dir, "retuned").unwrap_err();
        assert!(error.contains("incompatible V2"), "{error}");
        assert_eq!(live.capture(), before);
        assert_eq!(live.state_hash(), hash);
        assert!(!dir.join("retuned.be2save").exists());
        assert_eq!(old.capture(), original_state);
        assert_eq!(old.state_hash(), original_hash);
        assert_eq!(std::fs::read(original_path).unwrap(), original);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn append_only_content_update_migrates_exact_source_without_engine_relaxation() {
    let old = Workshop::standard();
    let bytes = snapshot::save(&old, "Pre-content-update").unwrap();
    let mut document: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/workshop.json")).unwrap(),
    )
    .unwrap();
    document["upgrades"].as_array_mut().unwrap().push(serde_json::json!({"id":"workshop:fixture-wire-tools","label":"Wire tools","cost":[["workshop:plate",1]],"duration_divisor":1}));
    document["recipes"].as_array_mut().unwrap().push(serde_json::json!({"id":"workshop:fixture-draw-wire","label":"Draw wire","duration_ticks":5,"inputs":[["workshop:plate",1]],"outputs":[["workshop:wire",2]],"required_upgrade":"workshop:fixture-wire-tools"}));
    let defs = Definitions::from_json(&document.to_string(), "extended").unwrap();
    let mut extended = Workshop::new(defs, 2).unwrap();
    assert!(snapshot::restore(&mut extended, &bytes).is_err());
    let dir = temporary("extension");
    let _ = std::fs::remove_file(dir.join("extended.be2save"));
    persistence::migrate_content(&mut extended, &bytes, old.definitions.clone(), &dir, "extended").unwrap();
    assert_eq!(old.capture(), extended.capture());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn source_aware_content_edit_rejects_duplicate_and_unresolved_refs_before_write() {
    let dir = temporary("edit");
    let p = dir.join("workshop.json");
    let original = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/workshop.json")).unwrap();
    std::fs::write(&p, &original).unwrap();
    let bad = serde_json::json!({"recipes":[{"id":"workshop:smelt","label":"Duplicate","duration_ticks":1,"inputs":[["workshop:missing",1]],"outputs":[["workshop:plate",1]],"required_upgrade":null}]});
    let error = content::edit(&p, &bad).unwrap_err();
    assert!(error.contains("DUPLICATE_ID"));
    assert!(error.contains("/recipes/"));
    assert_eq!(std::fs::read(&p).unwrap(), original);
    std::fs::remove_dir_all(dir).unwrap();
}
