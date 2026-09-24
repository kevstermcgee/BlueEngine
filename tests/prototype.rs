use vesper3d::prelude::*;

fn scene() -> SceneBuilder {
    SceneBuilder::new("Test").box_body("floor", V(0., -0.1, 0.), V(8., 0.1, 8.), V::ONE)
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
fn quickstart_is_the_compiled_thirty_line_example() {
    let source = include_str!("../examples/prototype.rs").replace("\r\n", "\n");
    let example = source.trim();
    assert_eq!(example.lines().count(), 30);
    assert!(include_str!("../docs/AI_QUICKSTART.md")
        .replace("\r\n", "\n")
        .contains(example));
}
