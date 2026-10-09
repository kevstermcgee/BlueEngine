//! End-to-end CLI contracts: preservation, transactions, runtime compatibility.
use std::{
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use vesper3d::math::V;
use vesper3d::viewer::authoring::{Edit, MapDocument};
static SERIAL: AtomicUsize = AtomicUsize::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "be2-tools-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scaffold_dependency(dir: &std::path::Path) -> PathBuf {
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    let dependency = manifest
        .lines()
        .find(|line| line.starts_with("vesper3d ="))
        .unwrap();
    let quoted = dependency
        .split("path = ")
        .nth(1)
        .unwrap()
        .split(", default-features")
        .next()
        .unwrap();
    let path: String = serde_json::from_str(quoted).unwrap();
    dir.join(path)
}

#[test]
fn cli_scaffold_resolves_engine_paths() {
    let s = Scratch::new();
    let engine = s.0.join("engine with spaces");
    std::fs::create_dir_all(engine.join(".git")).unwrap();
    std::fs::write(engine.join("Cargo.toml"), "[package]\nname = \"be2\"\n").unwrap();
    std::fs::write(engine.join("Cargo.lock"), "# retained engine pins\n").unwrap();
    std::fs::write(
        engine.join(".git/HEAD"),
        "0123456789abcdef0123456789abcdef01234567\n",
    )
    .unwrap();
    for (name, cwd, output, engine_arg, template) in [
        (
            "nested",
            engine.clone(),
            PathBuf::from("games/nested"),
            PathBuf::from("."),
            "two-d",
        ),
        (
            "absolute",
            s.0.clone(),
            s.0.join("outside/absolute"),
            engine.clone(),
            "custom-sim",
        ),
        (
            "sibling",
            s.0.clone(),
            PathBuf::from("games with spaces/sibling"),
            PathBuf::from("engine with spaces"),
            "stock",
        ),
        (
            "parents",
            engine.clone(),
            PathBuf::from("games/../nested/parents"),
            PathBuf::from("."),
            "three-d",
        ),
    ] {
        let run = Command::new(env!("CARGO_BIN_EXE_be2-tools"))
            .current_dir(&cwd)
            .args(["new-game", name])
            .arg(&output)
            .arg(&engine_arg)
            .arg(template)
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&run.stdout)
        );
        let game = cwd.join(output);
        assert_eq!(
            scaffold_dependency(&game).canonicalize().unwrap(),
            engine.canonicalize().unwrap()
        );
        assert_eq!(
            std::fs::read(game.join("Cargo.lock")).unwrap(),
            std::fs::read(engine.join("Cargo.lock")).unwrap()
        );
        let identity: serde_json::Value =
            serde_json::from_slice(&std::fs::read(game.join("assets/identity.json")).unwrap())
                .unwrap();
        assert_eq!(identity["engine_revision"], "0123456789ab");
    }
}

#[test]
fn cli_scaffold_dependency_is_accepted_by_cargo() {
    let s = Scratch::new();
    let game = s.0.join("game with spaces");
    let engine = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let run = Command::new(env!("CARGO_BIN_EXE_be2-tools"))
        .current_dir(engine)
        .args(["new-game", "cargo-proof"])
        .arg(&game)
        .args([".", "two-d"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(
        scaffold_dependency(&game).canonicalize().unwrap(),
        engine.canonicalize().unwrap()
    );
    let metadata = Command::new("cargo")
        .current_dir(&game)
        .args(["metadata", "--offline", "--no-deps", "--format-version=1"])
        .output()
        .unwrap();
    assert!(
        metadata.status.success(),
        "{}",
        String::from_utf8_lossy(&metadata.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let dependency = metadata["packages"][0]["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|dep| dep["name"] == "be2")
        .unwrap();
    assert_eq!(
        std::path::Path::new(dependency["path"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        engine.canonicalize().unwrap()
    );
}

#[test]
fn cli_scaffold_rejects_bad_engine_paths_before_creating_output() {
    let s = Scratch::new();
    let empty = s.0.join("empty");
    std::fs::create_dir(&empty).unwrap();
    for engine in [s.0.join("missing"), empty] {
        let output = s.0.join("new/nested/game");
        let run = Command::new(env!("CARGO_BIN_EXE_be2-tools"))
            .args(["new-game", "reject"])
            .arg(&output)
            .arg(engine)
            .arg("two-d")
            .output()
            .unwrap();
        assert!(!run.status.success());
        assert!(String::from_utf8_lossy(&run.stderr).contains("ENGINE_PATH"));
        assert!(!s.0.join("new").exists());
    }
    std::fs::write(s.0.join("Cargo.toml"), "[package]\nname = \"be2\"\n").unwrap();
    let before = std::fs::read(s.0.join("Cargo.toml")).unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_be2-tools"))
        .current_dir(&s.0)
        .args(["new-game", "reject", ".", ".", "two-d"])
        .output()
        .unwrap();
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("self dependency"));
    assert_eq!(std::fs::read(s.0.join("Cargo.toml")).unwrap(), before);
    assert!(!s.0.join("src").exists());
}

#[cfg(unix)]
#[test]
fn cli_scaffold_resolves_symlinks_before_making_the_dependency_relative() {
    let s = Scratch::new();
    let real = s.0.join("real/engine");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::write(real.join("Cargo.toml"), "[package]\nname = \"be2\"\n").unwrap();
    std::os::unix::fs::symlink(&real, s.0.join("alias")).unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_be2-tools"))
        .current_dir(&s.0)
        .args([
            "new-game",
            "linked",
            "alias/../games/linked",
            "alias",
            "two-d",
        ])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stdout)
    );
    let game = s.0.join("real/games/linked");
    assert_eq!(
        scaffold_dependency(&game).canonicalize().unwrap(),
        real.canonicalize().unwrap()
    );
}

#[test]
fn export_roundtrip_preserves_geometry_collision_and_entities() {
    let d = MapDocument::house().unwrap();
    let copy: MapDocument = serde_json::from_slice(&serde_json::to_vec(&d).unwrap()).unwrap();
    let a = d.build().unwrap();
    let b = copy.build().unwrap();
    assert_eq!(a.world.instances.len(), b.world.instances.len());
    assert_eq!(a.colliders.len(), b.colliders.len());
    assert_eq!(a.entities.len(), b.entities.len());
    let mut world = vesper3d::viewer::simulation::HeadlessWorld::with_room(b);
    assert!(world.join(1));
    world.step();
}
#[test]
fn edits_are_transactional_and_sync_added_object_components() {
    let d = MapDocument::house().unwrap();
    let original = serde_json::to_value(&d).unwrap();
    let edit = Edit::AddProp {
        id: "test-apple".into(),
        label: "Test apple".into(),
        kind: "apple".into(),
        origin: V(0., 0., -10.),
    };
    let a = d.apply(&[edit]).unwrap();
    assert!(a.colliders.contains_key("test-apple"));
    assert!(a.entities.iter().any(|e| e.id == "test-apple"));
    let nodes: Vec<_> = a
        .scene
        .nodes
        .iter()
        .filter(|n| n.id.starts_with("test-apple/"))
        .map(|n| n.id.clone())
        .collect();
    assert!(!nodes.is_empty());
    let moved = a
        .apply(&[Edit::Translate {
            nodes: nodes.clone(),
            colliders: vec!["test-apple".into()],
            entities: vec!["test-apple".into()],
            delta: V(1., 0., 0.),
        }])
        .unwrap();
    assert_eq!(
        moved.colliders["test-apple"].min.0,
        a.colliders["test-apple"].min.0 + 1.
    );
    let removed = moved
        .apply(&[Edit::Remove {
            nodes,
            colliders: vec!["test-apple".into()],
            entities: vec!["test-apple".into()],
        }])
        .unwrap();
    assert!(!removed.colliders.contains_key("test-apple"));
    assert!(d
        .apply(&[Edit::AddBox {
            id: "blocked".into(),
            label: "Blocked".into(),
            center: V(0., 1., 4.6),
            half_extents: V(1., 1., 1.),
            color: V(1., 0., 0.),
            structural: false,
        }])
        .is_err());
    assert_eq!(serde_json::to_value(&d).unwrap(), original);
}
#[test]
fn invalid_versions_unknown_ids_and_animated_maps_fail() {
    let mut d = MapDocument::house().unwrap();
    d.schema_version = 2;
    assert!(d.validate().is_err());
    d.schema_version = 1;
    assert!(d
        .apply(&[Edit::Remove {
            nodes: vec!["typo".into()],
            colliders: vec![],
            entities: vec![]
        }])
        .is_err());
    d.scene.nodes[0].motion.bob = 1.;
    assert!(d.validate().is_err());
    assert!(serde_json::from_str::<Edit>(
        r#"{"op":"remove","nodes":[],"colliders":[],"entities":[],"typo":true}"#
    )
    .is_err());
}
#[test]
fn cli_refuses_overwrite_and_failed_patch_leaves_no_output() {
    let s = Scratch::new();
    let map = s.0.join("map.json");
    let out = s.0.join("after.json");
    let patch = s.0.join("bad.json");
    let run = |args: &[&std::ffi::OsStr]| {
        Command::new(env!("CARGO_BIN_EXE_be2-tools"))
            .args(args)
            .output()
            .unwrap()
    };
    assert!(run(&["export-house".as_ref(), map.as_os_str()])
        .status
        .success());
    let before = std::fs::read(&map).unwrap();
    assert!(!run(&["export-house".as_ref(), map.as_os_str()])
        .status
        .success());
    assert_eq!(std::fs::read(&map).unwrap(), before);
    std::fs::write(
        &patch,
        r#"[{"op":"remove","nodes":["missing"],"colliders":[],"entities":[]}]"#,
    )
    .unwrap();
    assert!(!run(&[
        "apply".as_ref(),
        map.as_os_str(),
        patch.as_os_str(),
        out.as_os_str()
    ])
    .status
    .success());
    assert!(!out.exists());
    assert_eq!(std::fs::read(&map).unwrap(), before);
    assert!(run(&["audit".as_ref(), map.as_os_str()]).status.success());
    let svg = s.0.join("plan.svg");
    assert!(
        run(&["floorplan".as_ref(), map.as_os_str(), svg.as_os_str()])
            .status
            .success()
    );
    assert!(std::fs::read_to_string(svg).unwrap().contains("<svg"));
}

#[test]
fn successful_cli_patch_and_spatial_queries_are_consistent() {
    let s = Scratch::new();
    let map = s.0.join("map.json");
    let edited = s.0.join("edited.json");
    let patch = s.0.join("patch.json");
    let run = |args: &[&std::ffi::OsStr]| {
        Command::new(env!("CARGO_BIN_EXE_be2-tools"))
            .args(args)
            .output()
            .unwrap()
    };
    assert!(run(&["export-house".as_ref(), map.as_os_str()])
        .status
        .success());
    std::fs::write(
        &patch,
        include_str!("../tools/examples/add-garden-props.json"),
    )
    .unwrap();
    assert!(run(&[
        "apply".as_ref(),
        map.as_os_str(),
        patch.as_os_str(),
        edited.as_os_str()
    ])
    .status
    .success());
    let result = run(&[
        "select".as_ref(),
        edited.as_os_str(),
        "garden-apple".as_ref(),
    ]);
    assert!(result.status.success());
    let selection: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(selection["nodes"].as_array().unwrap().len(), 2);
    let result = run(&[
        "near".as_ref(),
        edited.as_os_str(),
        "-1.8,0.9,-11.2".as_ref(),
        "0.1".as_ref(),
    ]);
    assert!(result.status.success());
    assert!(String::from_utf8(result.stdout)
        .unwrap()
        .contains("garden-apple"));
    let route = s.0.join("route.json");
    std::fs::write(
        &route,
        include_str!("../tools/examples/upstairs.route.json"),
    )
    .unwrap();
    assert!(
        run(&["route".as_ref(), edited.as_os_str(), route.as_os_str()])
            .status
            .success()
    );
    assert!(Command::new(env!("CARGO_BIN_EXE_be2-headless"))
        .args(["--map", edited.to_str().unwrap(), "--ticks", "1"])
        .output()
        .unwrap()
        .status
        .success());
}
