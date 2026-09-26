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
