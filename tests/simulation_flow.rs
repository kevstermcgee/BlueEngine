//! Public lifecycle contract for the local simulation; no transport is involved.
use vesper3d::viewer::{controller::Movement, simulation::HeadlessWorld};

#[test]
fn pending_jump_survives_replacement_and_is_consumed_once() {
    let mut world = HeadlessWorld::new().unwrap();
    assert!(world.join(10));
    assert!(world.join(20));
    assert!(world.input(
        10,
        Movement {
            jump: true,
            ..Default::default()
        },
        0.,
        0.
    ));
    // A newer intent before the tick must not erase the press edge.
    assert!(world.input(10, Movement::default(), 0., 0.));
    world.step();
    assert!(!world.player(10).unwrap().is_grounded());
    assert!(world.player(20).unwrap().is_grounded());
    for _ in 0..120 {
        world.step();
    }
    assert!(world.player(10).unwrap().is_grounded());
    world.step();
    assert!(world.player(10).unwrap().is_grounded());
    assert_eq!(world.tick, 122);

    world.leave(10);
    assert!(world.player(10).is_none());
    assert!(!world.input(10, Movement::default(), 0., 0.));
    world.leave(10);
    assert!(world.join(10));
    world.step();
    assert!(world.player(10).unwrap().is_grounded());
}

#[test]
fn held_intent_persists_and_rejected_input_preserves_state() {
    let mut world = HeadlessWorld::new().unwrap();
    assert!(world.join(1));
    let start = world.player(1).unwrap().position;
    assert!(world.input(
        1,
        Movement {
            right: 1.,
            ..Default::default()
        },
        0.,
        0.
    ));
    let before = world.player(1).unwrap().clone();
    assert!(!world.input(1, Movement::default(), f32::NAN, 1.));
    assert_eq!(world.player(1).unwrap().position, before.position);
    assert_eq!(world.player(1).unwrap().yaw, before.yaw);
    assert_eq!(world.player(1).unwrap().pitch, before.pitch);
    world.step();
    let first = world.player(1).unwrap().position;
    world.step();
    let second = world.player(1).unwrap().position;
    assert!((first - start).length() > 0.);
    assert!((second - first).length() > 0.);

    // Leaving clears held movement as well as membership; reusing the ID is fresh.
    world.leave(1);
    assert!(world.join(1));
    world.step();
    assert_eq!(world.player(1).unwrap().position, start);
}
