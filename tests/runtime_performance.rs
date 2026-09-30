use vesper3d::{prelude::*, viewer::lifecycle::LifecycleState};

fn world() -> HeadlessWorld {
    SceneBuilder::new("Cache regression")
        .spawn(V(-4., 0., -4.), 0.)
        .prop("a", "apple", V(0., 1., 0.))
        .prop("b", "apple", V(2., 1., 0.))
        .world()
        .unwrap()
}

#[test]
fn lifecycle_mapping_survives_public_reordering_removal_insertion_and_restore() {
    let mut w = world();
    let save = w.save_state().unwrap();
    w.lifecycle.objects.reverse();
    w.impulse("a", V(0., 0.2, 0.));
    w.step();
    assert_eq!(
        w.lifecycle.get("a").unwrap().state,
        LifecycleState::DynamicEntity
    );
    assert!(
        (w.lifecycle.get("a").unwrap().position - w.prop_position("a").unwrap()).length() < 0.001
    );
    w.lifecycle.objects.retain(|o| o.id != "a");
    w.step();
    w.lifecycle.register("a".into(), "Apple".into(), V::ZERO);
    w.step();
    assert_eq!(
        w.lifecycle.get("a").unwrap().state,
        LifecycleState::DynamicEntity
    );
    // Restore validates the original identity set and can replace registry ordering.
    w.restore_state(&save).unwrap();
    w.step();
    assert!(
        (w.lifecycle.get("a").unwrap().position - w.prop_position("a").unwrap()).length() < 0.001
    );
    for _ in 0..600 {
        w.step();
    }
    assert_eq!(
        w.lifecycle.get("a").unwrap().state,
        LifecycleState::InteractiveStatic
    );
    let replacement = SceneBuilder::new("Replacement")
        .spawn(V(-4., 0., -4.), 0.)
        .prop("c", "apple", V(0., 1., 0.))
        .build()
        .unwrap();
    w.change_map(&replacement).unwrap();
    w.impulse("c", V(0., 0.2, 0.));
    w.step();
    assert!(w.lifecycle.get("a").is_none());
    assert_eq!(
        w.lifecycle.get("c").unwrap().state,
        LifecycleState::DynamicEntity
    );
}

#[test]
fn sleeping_correction_updates_collision_bounds_and_query_geometry() {
    let mut w = world();
    for _ in 0..600 {
        w.step();
    }
    let old = w
        .room
        .entities
        .iter()
        .find(|e| e.id == "a")
        .unwrap()
        .bounds
        .clone();
    let physics = w.prop_physics.as_mut().unwrap();
    assert!(physics.set_prop_transform_and_vel(
        "a",
        V(6., 1., 0.),
        [0., 0., 0., 1.],
        V::ZERO,
        V::ZERO,
        true
    ));
    physics.sync(&mut w.room);
    let bounds = w
        .room
        .entities
        .iter()
        .find(|e| e.id == "a")
        .unwrap()
        .bounds
        .clone();
    assert!(bounds.min.0 > old.max.0 + 4.);
    assert!(w
        .room
        .colliders
        .iter()
        .any(|c| c.min.0 > 5. && c.max.0 < 7.));
    assert!(w
        .room
        .dynamic_world
        .instances
        .iter()
        .any(|i| i.bounds.lo.0 > 5. && i.bounds.hi.0 < 7.));
    let ray = vesper3d::math::Ray {
        o: V(6., 3., 0.),
        d: V(0., -1., 0.),
    };
    assert!(w.room.dynamic_world.hit(ray, 4., false).is_some());
    let pointer = w.room.dynamic_world.instances.as_ptr();
    physics.sync(&mut w.room);
    assert_eq!(
        pointer,
        w.room.dynamic_world.instances.as_ptr(),
        "unchanged sleeping world must retain its query data"
    );
}
