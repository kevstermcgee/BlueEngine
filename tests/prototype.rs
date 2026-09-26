use vesper3d::prelude::*;

fn scene() -> SceneBuilder {
    SceneBuilder::new("Test")
        .spawn(V(0., 0., 4.6), -0.10)
        .box_body("floor", V(0., -0.1, 0.), V(8., 0.1, 8.), V::ONE)
}

#[test]
fn builder_roundtrip_runs_physics_and_movement() {
    let map = scene()
        .prop("ball", "apple", V(2., 2., 0.))
        .build()
        .unwrap();
    let saved = serde_json::to_vec(&map).unwrap();
    let loaded: MapDocument = serde_json::from_slice(&saved).unwrap();
    assert!(loaded.colliders.contains_key("ball"));
    assert!(loaded.entities.iter().any(|e| e.id == "ball"));
    let mut world = HeadlessWorld::try_with_room(loaded.build().unwrap()).unwrap();
    let initial = world.prop_position("ball").unwrap();
    assert!(world.join(1));
    assert!(world.input(
        1,
        Movement {
            forward: 1.,
            ..Default::default()
        },
        0.,
        0.
    ));
    for _ in 0..30 {
        world.step();
    }
    assert!(world.prop_position("ball").unwrap().1 < initial.1);
    assert!(world.player(1).unwrap().position.2 < 4.6);
    assert!(world.impulse("ball", V(0., 1., 0.)));
    assert!(!world.impulse("ball", V(f32::NAN, 0., 0.)));
    assert!(!world.impulse("floor", V::ONE));
    assert!(!world.impulse("unknown", V::ONE));
    assert!(world.prop_position("floor").is_none());
}

#[test]
fn invalid_builder_transactions_fail_without_partial_documents() {
    assert!(SceneBuilder::new("missing spawn")
        .structural_box("floor", V(0., -0.1, 0.), V(2., 0.1, 2.), V::ONE)
        .build()
        .is_err());
    assert!(scene().prop("floor", "apple", V::ZERO).build().is_err());
    assert!(scene().prop("bad id", "apple", V::ZERO).build().is_err());
    assert!(scene()
        .prop("unknown", "not-a-kind", V::ZERO)
        .build()
        .is_err());
    assert!(scene()
        .box_body("negative", V::ZERO, V(-1., 1., 1.), V::ONE)
        .build()
        .is_err());
    assert!(scene()
        .box_body("nan", V(f32::NAN, 0., 0.), V::ONE, V::ONE)
        .build()
        .is_err());
    assert!(scene()
        .box_body("blocked", V(0., 1., 4.6), V::ONE, V::ONE)
        .build()
        .is_err());
}

#[test]
fn structural_geometry_is_not_a_semantic_reachability_target() {
    let map = SceneBuilder::new("Structure")
        .spawn(V(0., 0., 0.), 0.)
        .structural_box("floor", V(0., -0.1, 0.), V(4., 0.1, 4.), V::ONE)
        .structural_box("ceiling", V(0., 3., 0.), V(4., 0.1, 4.), V::ONE)
        .box_body("switch", V(1., 1., 0.), V(0.1, 0.1, 0.1), V::ONE)
        .build()
        .unwrap();
    assert!(map.colliders.contains_key("ceiling"));
    assert!(!map.entities.iter().any(|entity| entity.id == "ceiling"));
    assert!(map.entities.iter().any(|entity| entity.id == "switch"));
    let report = vesper3d::viewer::reach::analyze_reach(&map, None).unwrap();
    assert!(!report.unreachable_entities.contains(&"ceiling".to_owned()));

    let mut missing_spawn = map.clone();
    missing_spawn.default_spawn = None;
    assert!(vesper3d::viewer::reach::analyze_reach(&missing_spawn, None).is_err());
}

#[test]
fn map_change_is_transactional_and_respawns_existing_players() {
    let mut world = scene().world().unwrap();
    assert!(world.join(7));
    world.step();
    let tick = world.tick;
    let old_hash = world.content_hash;
    let next = SceneBuilder::new("Next")
        .spawn(V(2., 0., 1.), 0.5)
        .structural_box("floor", V(0., -0.1, 0.), V(8., 0.1, 8.), V::ONE)
        .build()
        .unwrap();
    world.change_map(&next).unwrap();
    assert_eq!(world.tick, tick);
    assert_ne!(world.content_hash, old_hash);
    assert_eq!(world.player(7).unwrap().feet_height(), 0.);
    assert_eq!(world.player(7).unwrap().position.0, 2.);

    let mut invalid = next.clone();
    invalid.default_spawn = None;
    let hash = world.content_hash;
    assert!(world.change_map(&invalid).is_err());
    assert_eq!(world.content_hash, hash);
    assert!(world.player(7).is_some());
}

#[test]
fn quickstart_is_the_compiled_example() {
    let source = include_str!("../examples/prototype.rs").replace("\r\n", "\n");
    let example = source.trim();
    assert_eq!(example.lines().count(), 31);
    assert!(include_str!("../docs/AI_QUICKSTART.md")
        .replace("\r\n", "\n")
        .contains(example));
}
