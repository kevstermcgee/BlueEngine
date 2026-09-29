//! The prop API every physics game rebuilt by hand: a configurable floor, mass, a real throw, ID-based
//! accessors, centre of mass, props-only scenes and the re-exported physics crate.
use vesper3d::{math::Ray, prelude::*};

fn lane() -> SceneBuilder {
    SceneBuilder::new("Prop API")
        .spawn(V(0., 0., 4.6), 0.)
        .structural_box("shelf", V(3., 0.5, 0.), V(0.5, 0.5, 0.5), V::ONE)
}

fn run(world: &mut HeadlessWorld, ticks: usize) {
    for _ in 0..ticks {
        world.step();
    }
}

#[test]
fn props_fall_through_a_removed_floor_and_rest_on_a_moved_one_even_after_a_restore() {
    let mut world = lane()
        .prop("apple", "apple", V(0., 1., 0.))
        .world()
        .unwrap();
    assert_eq!(world.prop_physics.as_ref().unwrap().floor(), Some(0.));
    world.set_prop_floor(None);
    assert_eq!(world.prop_physics.as_ref().unwrap().floor(), None);
    run(&mut world, 120);
    let y = world.prop_position("apple").unwrap().1;
    assert!(y < -5., "the apple is at y {y} after 2 s with no floor");

    let mut world = lane()
        .prop("apple", "apple", V(0., 1., 0.))
        .world()
        .unwrap();
    world.set_prop_floor(Some(-3.));
    world.set_prop_floor(Some(f32::NAN));
    assert_eq!(world.prop_physics.as_ref().unwrap().floor(), Some(-3.));
    run(&mut world, 300);
    let rest = world.prop_position("apple").unwrap();
    assert!(rest.1 > -3. && rest.1 < -2.7, "the apple rests at {rest:?}");
    assert_eq!(world.is_prop_sleeping("apple"), Some(true));
    // A restore rebuilds the scene from the pristine one: the moved floor must be part of it, or the apple
    // (below the original plane) would fall forever after a load.
    let saved = world.save_state().unwrap();
    run(&mut world, 60);
    world.restore_state(&saved).unwrap();
    run(&mut world, 120);
    assert_eq!(
        world.prop_position("apple"),
        Some(rest),
        "the restored world keeps its floor"
    );
    // Lowering the floor under a sleeping prop wakes it: it falls and settles half a metre lower.
    world.set_prop_floor(Some(-3.5));
    run(&mut world, 300);
    let lower = world.prop_position("apple").unwrap();
    assert!(
        (lower.1 - (rest.1 - 0.5)).abs() < 0.02,
        "the apple settled at {lower:?} after the floor moved from {rest:?}"
    );
}

#[test]
fn mass_half_extents_and_centre_of_mass_are_reported_by_id_and_index() {
    let world = lane()
        .prop("cereal", "cereal", V(0., 0., 0.))
        .prop("apple", "apple", V(1., 0., 0.))
        .world()
        .unwrap();
    let mass = world.prop_mass("cereal").unwrap();
    assert!(
        (mass - 2.77).abs() <= 0.0277,
        "the cereal box is {mass} kg, not 2.77 kg within 1 %"
    );
    let apple = world.prop_mass("apple").unwrap();
    assert!(
        (apple - 0.62).abs() <= 0.03,
        "the apple is {apple} kg, not 0.62 kg within 5 %"
    );
    assert_eq!(world.prop_half_extents("cereal"), Some(V(0.14, 0.22, 0.07)));
    let (origin, centre) = (
        world.prop_position("apple").unwrap(),
        world.prop_center_of_mass("apple").unwrap(),
    );
    assert!(
        (centre - origin).length() < world.prop_half_extents("apple").unwrap().length(),
        "the centre of mass {centre:?} is inside the apple at {origin:?}"
    );
    for id in ["shelf", "missing"] {
        assert!(world.prop_index(id).is_none());
        assert!(world.prop_mass(id).is_none());
        assert!(world.prop_half_extents(id).is_none());
        assert!(world.prop_center_of_mass(id).is_none());
    }
}

#[test]
fn a_throw_leaves_at_the_requested_velocity_where_a_drop_is_capped() {
    let mut world = lane()
        .prop("cereal", "cereal", V(0., 0., 3.))
        .world()
        .unwrap();
    assert!(world.join(1));
    let index = world.prop_index("cereal").unwrap();
    world
        .prop_physics
        .as_mut()
        .unwrap()
        .set_held_for_player(1, index);
    run(&mut world, 30);
    assert_eq!(world.held_prop(1), Some("cereal"));
    assert_eq!(world.prop_holder("cereal"), Some(1));
    assert!(
        !world.throw(1, V(f32::NAN, 0., 0.), V::ZERO),
        "a non-finite throw is refused"
    );
    assert_eq!(world.held_prop(1), Some("cereal"), "and changes nothing");
    let direction = V(0., 0.3, -1.).norm();
    assert!(world.throw(1, direction * 15., V(0., 0., 4.)));
    assert!(world.held_prop(1).is_none() && world.prop_holder("cereal").is_none());
    world.step();
    let velocity = world.prop_linear_velocity("cereal").unwrap();
    assert!(
        (velocity - direction * 15.).length() < 0.3,
        "one tick after a 15 m/s throw the box moves at {velocity:?}"
    );
    assert!(!world.throw(1, V::ONE, V::ZERO), "nothing is held any more");
    assert!(!world.throw(2, V::ONE, V::ZERO), "player 2 does not exist");

    // The plain drop keeps at most 4 m/s of carry momentum: that is why it cannot throw.
    let mut world = lane()
        .prop("cereal", "cereal", V(0., 0., 3.))
        .world()
        .unwrap();
    assert!(world.join(1));
    let physics = world.prop_physics.as_mut().unwrap();
    physics.set_held_for_player(1, index);
    physics.apply_impulse(index, V(0., 0., -100.));
    physics.drop_for_player(1);
    let speed = physics.prop_linear_velocity(index).unwrap().length();
    assert!(
        (3.99..=4.001).contains(&speed),
        "a drop releases at {speed} m/s"
    );
}

#[test]
fn every_id_accessor_agrees_with_its_index_twin() {
    let mut world = lane()
        .prop("cereal", "cereal", V(0., 0.5, 0.))
        .prop("apple", "apple", V(1., 1., 0.))
        .prop("lamp", "table-lamp", V(-1., 0., 1.))
        .world()
        .unwrap();
    assert!(world.impulse("apple", V(1., 2., 0.)));
    run(&mut world, 20);
    let physics = world.prop_physics.as_ref().unwrap();
    for id in ["cereal", "apple", "lamp"] {
        let i = world.prop_index(id).unwrap();
        assert_eq!(physics.prop_id(i), Some(id));
        assert_eq!(physics.prop_index(id), Some(i));
        assert_eq!(world.prop_position(id), physics.prop_position(i));
        assert_eq!(world.prop_rotation(id), physics.prop_rotation(i));
        assert_eq!(
            world.prop_linear_velocity(id),
            physics.prop_linear_velocity(i)
        );
        assert_eq!(
            world.prop_angular_velocity(id),
            physics.prop_angular_velocity(i)
        );
        assert_eq!(world.prop_mass(id), physics.prop_mass(i));
        assert_eq!(world.prop_half_extents(id), physics.prop_half_extents(i));
        assert_eq!(
            world.prop_center_of_mass(id),
            physics.prop_center_of_mass(i)
        );
        assert_eq!(
            world.is_prop_sleeping(id),
            Some(physics.is_prop_sleeping(i))
        );
        assert_eq!(world.prop_holder(id), physics.holder_of(i));
    }
    assert!(physics.prop_id(3).is_none());
    for id in ["shelf", "missing"] {
        assert!(world.prop_index(id).is_none());
        assert!(world.prop_rotation(id).is_none());
        assert!(world.prop_linear_velocity(id).is_none());
        assert!(world.prop_angular_velocity(id).is_none());
        assert!(world.is_prop_sleeping(id).is_none());
        assert!(world.prop_holder(id).is_none());
    }
    let ray = Ray {
        o: V(0.05, 5., 0.02),
        d: V(0., -1., 0.),
    };
    let (hit, distance) = world.hit_prop(ray, 10.).unwrap();
    assert_eq!(hit, "cereal");
    assert!(distance > 4. && distance < 5., "hit at {distance} m");
    assert_eq!(
        physics
            .hit_prop(ray, 10.)
            .map(|(i, d)| (physics.prop_id(i).unwrap(), d)),
        Some((hit, distance))
    );
    assert!(world.hit_prop(ray, 1.).is_none());
}

#[test]
fn a_props_only_scene_builds_steps_and_saves_without_a_spawn() {
    let mut world = SceneBuilder::new("Course")
        .without_spawn()
        .structural_box("green", V(0., -0.1, 0.), V(5., 0.1, 5.), V::ONE)
        .prop("ball", "apple", V(0., 1., 0.))
        .world()
        .unwrap();
    assert!(!world.join(1), "there is nowhere to spawn a player");
    let start = world.prop_position("ball").unwrap();
    run(&mut world, 60);
    assert_eq!(world.tick, 60);
    assert!(world.prop_position("ball").unwrap().1 < start.1);
    let saved = world.save_state().unwrap();
    run(&mut world, 60);
    world.restore_state(&saved).unwrap();
    assert_eq!(world.tick, 60);
    assert!(SceneBuilder::new("no spawn either way").build().is_err());
    let map = SceneBuilder::new("Doc").without_spawn().build().unwrap();
    assert!(map.default_spawn.is_none());
    assert!(
        map.build_standalone().is_err(),
        "standalone play still needs a spawn"
    );
}

#[test]
fn the_physics_crate_is_re_exported_with_the_engine_pin() {
    use vesper3d::rapier::prelude::*;
    let mut bodies = RigidBodySet::new();
    let body = bodies.insert(RigidBodyBuilder::dynamic().build());
    let mut colliders = ColliderSet::new();
    colliders.insert_with_parent(
        ColliderBuilder::ball(0.1).density(160.).build(),
        body,
        &mut bodies,
    );
    let expected = 160. * 4. / 3. * std::f32::consts::PI * 0.1f32.powi(3);
    assert!((bodies[body].mass() - expected).abs() < 1e-4);
}
