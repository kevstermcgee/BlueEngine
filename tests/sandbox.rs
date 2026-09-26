//! Content and shared-controller regressions; no GPU or device input needed.
#[allow(dead_code)]
#[path = "../src/bin/sandbox/content.rs"]
mod content;
use std::path::Path;
use vesper3d::prelude::*;
use vesper3d::viewer::{controller::CharacterKind, simulation::PlayerStepper};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn every_registered_asset_and_destination_compiles_with_clear_spawn() {
    let catalog = content::Catalog::load(root()).unwrap();
    assert_eq!(catalog.assets.len(), 78);
    let mut ids = std::collections::HashSet::new();
    for a in &catalog.assets {
        assert!(ids.insert(&a.id), "duplicate {}", a.id);
        assert!(root().join(&a.source).is_file(), "{}", a.source);
        let doc = content::load_map(root(), &a.map).unwrap();
        let room = doc.build_standalone().unwrap();
        assert!(room.entities.iter().any(|e| e.id == "specimen"));
    }
    for map in &catalog.maps {
        let doc = content::load_map(root(), &map.path).unwrap();
        doc.build_standalone().unwrap();
    }
    assert!(catalog
        .matching_assets("sandbox pine")
        .iter()
        .any(|i| catalog.assets[*i].id == "sandbox/pine"));
    assert!(catalog.matching_assets("no-such-asset-99").is_empty());
}

#[test]
fn gallery_native_materials_keep_physics_eligibility() {
    let doc = content::load_map(
        root(),
        "assets/games/blueengine-sandbox/previews/core-native-apple_red.json",
    )
    .unwrap();
    let mut world = HeadlessWorld::try_with_room(doc.build().unwrap()).unwrap();
    assert!(world.prop_position("specimen").is_some());
    assert!(world.join(1));
    for _ in 0..120 {
        world.step();
    }
    assert!(world.prop_position("specimen").unwrap().finite());
}

#[test]
fn both_profiles_have_frame_rate_independent_open_aisle_movement() {
    let doc = content::load_map(
        root(),
        "assets/games/blueengine-sandbox/maps/calibration.json",
    )
    .unwrap();
    let room = doc.build().unwrap();
    for kind in [CharacterKind::Scientist, CharacterKind::Feta] {
        let mut endpoints = vec![];
        for fps in [30, 60, 144] {
            let mut p = Controller::for_character_at(kind, V(0., 0., 18.), 0.).unwrap();
            let mut stepper = PlayerStepper::default();
            stepper.reset(&p);
            let mut ticks = 0;
            for _ in 0..fps * 2 {
                ticks += stepper.advance(
                    &mut p,
                    Movement {
                        forward: 1.,
                        ..Default::default()
                    },
                    1. / fps as f32,
                    &room.colliders,
                );
            }
            assert!((119..=120).contains(&ticks));
            assert!(p.position.2 < 16., "must actually move, not just tick");
            assert!(p.feet_height().abs() < 0.01);
            endpoints.push(p.position);
        }
        for p in &endpoints[1..] {
            assert!((*p - endpoints[0]).length() < 0.06);
        }
    }
}

#[test]
fn calibration_clearances_distinguish_rat_crouch_and_standing_human() {
    let room = content::load_map(
        root(),
        "assets/games/blueengine-sandbox/maps/calibration.json",
    )
    .unwrap()
    .build()
    .unwrap();
    for (kind, z, crouch, pass) in [
        (CharacterKind::Scientist, 6., false, false),
        (CharacterKind::Scientist, 0., false, false),
        (CharacterKind::Scientist, 0., true, true),
        (CharacterKind::Scientist, -6., false, true),
        (CharacterKind::Feta, 6., false, true),
    ] {
        let mut p = Controller::for_character_at(kind, V(9., 0., z + 3.), 0.).unwrap();
        for _ in 0..300 {
            p.update(
                Movement {
                    forward: 1.,
                    crouch,
                    ..Default::default()
                },
                TICK_SECONDS,
                &room.colliders,
            );
        }
        assert_eq!(
            p.position.2 < z - 1.5,
            pass,
            "{kind:?} z={z} crouch={crouch} at {:?}",
            p.position
        );
    }
}
use vesper3d::viewer::creative;

fn creative_base() -> MapDocument {
    SceneBuilder::new("Creative test")
        .spawn(V(0., 0., 30.), 0.)
        .structural_box("floor", V(0., -0.1, 0.), V(50., 0.1, 50.), V(0.3, 0.4, 0.5))
        .build()
        .unwrap()
}
#[test]
fn every_catalog_asset_places_without_its_studio_and_rotates_consistently() {
    use vesper3d::{math::Mat, scene::Track};
    let catalog = content::Catalog::load(root()).unwrap();
    let base = creative_base();
    let player =
        Controller::for_character_at(CharacterKind::Scientist, V(0., 0., 30.), 0.).unwrap();
    for asset in &catalog.assets {
        let specimen = creative::specimen(content::load_map(root(), &asset.map).unwrap()).unwrap();
        assert!(!specimen.colliders.contains_key("floor"));
        assert!(specimen
            .scene
            .nodes
            .iter()
            .all(|n| n.id.starts_with("specimen/")));
        for turns in 0..4 {
            let at = V(12., 2., -12.);
            let doc = creative::place(&base, &specimen, at, turns, &player)
                .unwrap_or_else(|e| panic!("{}: {e}", asset.id));
            assert_eq!(doc.entities.len(), 1);
            assert_eq!(
                doc.scene.nodes.len(),
                base.scene.nodes.len() + specimen.scene.nodes.len()
            );
            for (before, after) in specimen
                .scene
                .nodes
                .iter()
                .zip(doc.scene.nodes.iter().skip(base.scene.nodes.len()))
            {
                let (Track::Fixed(p), Track::Fixed(r), Track::Fixed(scale)) =
                    (&before.pos, &before.rot, &before.scale)
                else {
                    panic!()
                };
                let (Track::Fixed(q), Track::Fixed(s), Track::Fixed(size)) =
                    (&after.pos, &after.rot, &after.scale)
                else {
                    panic!()
                };
                let old = Mat::trs(*p, *r, *scale);
                let new = Mat::trs(*q, *s, *size);
                for corner in [V(1., 1., 1.), V(-1., 1., -1.), V(1., -1., -1.)] {
                    let expected = creative::rotate(old.point(corner), turns) + at;
                    assert!(
                        (new.point(corner) - expected).length() < 0.002,
                        "{} transform",
                        asset.id
                    );
                }
            }
            doc.build().unwrap();
        }
    }
}
#[test]
fn creative_ids_removal_and_save_reload_preserve_the_base_world() {
    let base = creative_base();
    let player =
        Controller::for_character_at(CharacterKind::Scientist, V(0., 0., 30.), 0.).unwrap();
    let asset = creative::specimen(
        content::load_map(
            root(),
            "assets/games/blueengine-sandbox/previews/sandbox-shipping-crate.json",
        )
        .unwrap(),
    )
    .unwrap();
    let first = creative::place(&base, &asset, V(8., 0., 0.), 1, &player).unwrap();
    let second = creative::place(&first, &asset, V(12., 0., 0.), 2, &player).unwrap();
    assert_eq!(second.entities[0].id, "creative-1");
    assert_eq!(second.entities[1].id, "creative-2");
    assert!(creative::remove(&second, "floor").is_err());
    let removed = creative::remove(&second, "creative-1").unwrap();
    assert_eq!(removed.entities.len(), 1);
    assert_eq!(removed.entities[0].id, "creative-2");
    assert_eq!(
        serde_json::to_value(&removed.colliders["floor"]).unwrap(),
        serde_json::to_value(&base.colliders["floor"]).unwrap()
    );
    let directory = root().join(".be2-work").join(format!(
        "creative-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let file = directory.join("world.json");
    creative::save(&second, &file).unwrap();
    creative::save(&removed, &file).unwrap();
    let reloaded = MapDocument::load(&file).unwrap();
    assert_eq!(
        serde_json::to_value(reloaded).unwrap(),
        serde_json::to_value(removed).unwrap()
    );
    let mut invalid = base.clone();
    invalid.schema_version = 99;
    assert!(creative::save(&invalid, &file).is_err());
    assert_eq!(MapDocument::load(&file).unwrap().entities.len(), 1);
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn creative_placement_rejects_player_overlap_and_blocked_spawn() {
    let base = creative_base();
    let asset = creative::specimen(
        content::load_map(
            root(),
            "assets/games/blueengine-sandbox/previews/sandbox-shipping-crate.json",
        )
        .unwrap(),
    )
    .unwrap();
    let player = Controller::for_character_at(CharacterKind::Scientist, V(5., 0., 5.), 0.).unwrap();
    assert!(creative::place(&base, &asset, V(5., 0., 5.), 0, &player).is_err());
    assert!(creative::place(&base, &asset, V(0., 0., 30.), 0, &player).is_err());
    assert!(creative::place(&base, &asset, V(f32::NAN, 0., 0.), 0, &player).is_err());
    assert_eq!(base.entities.len(), 0);
}
