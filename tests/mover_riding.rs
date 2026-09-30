//! Movers carry the players standing on them and push the ones they would move through.
use vesper3d::{
    math::V,
    viewer::{
        controller::{Controller, Movement},
        game::{GameAction, LoadedGame, Mover, Rule, TimerDefinition},
        game_example,
        simulation::HeadlessWorld,
    },
};

/// A world with one mover on button-a (a 0.6 m box spanning x -3.3..-2.7, y 1.2..1.8, z 0.7..1.3) that a
/// timer sets moving on tick 3: `open` says which way, `initial_open` where it starts.
fn world(translation: V, initial_open: bool, open: bool) -> HeadlessWorld {
    world_with(translation, 20, initial_open, open)
}
fn world_with(
    translation: V,
    duration_ticks: u32,
    initial_open: bool,
    open: bool,
) -> HeadlessWorld {
    let (mut document, map) = game_example::documents().unwrap();
    document.movers = vec![Mover {
        id: "lift".into(),
        entity: "button-a".into(),
        translation,
        duration_ticks,
        initial_open,
    }];
    document.timers = vec![TimerDefinition {
        id: "go".into(),
        duration_ticks: 3,
        auto_start: true,
        repeats: false,
    }];
    document.rules.push(Rule {
        id: "start".into(),
        on_interact: None,
        on_enter: None,
        on_exit: None,
        on_timer: Some("go".into()),
        condition: None,
        once: false,
        actions: vec![GameAction::SetMover {
            mover: "lift".into(),
            open,
        }],
    });
    let mut world = LoadedGame { document, map }.world().unwrap();
    world.join(1);
    world
}
fn stand(world: &mut HeadlessWorld, x: f32, feet: f32, z: f32) {
    let profile = world.game.as_ref().unwrap().document().player_profile;
    *world.player_mut(1).unwrap() = Controller::for_profile(profile, V(x, feet, z), 0.).unwrap();
}
fn feet_and_x(world: &HeadlessWorld) -> (f32, f32, bool) {
    let c = world.player(1).unwrap();
    (c.feet_height(), c.position.0, c.is_grounded())
}
fn run(world: &mut HeadlessWorld, ticks: u32) {
    for _ in 0..ticks {
        world.step();
    }
}

#[test]
fn a_rising_lift_carries_the_player_standing_on_it_and_they_stay_on_top() {
    // The room's ceiling is at y = 4, so a 1.8 m body can be lifted at most to feet = 2.2.
    let mut w = world(V(0., 0.35, 0.), false, true);
    stand(&mut w, -3.0, 1.8, 1.0);
    run(&mut w, 60);
    let (feet, x, grounded) = feet_and_x(&w);
    assert!(
        (feet - 2.15).abs() < 0.02,
        "feet at {feet}, expected on top of the raised box (2.15)"
    );
    assert!((x + 3.0).abs() < 0.01 && grounded);
    run(&mut w, 60);
    assert!(
        (feet_and_x(&w).0 - 2.15).abs() < 0.02,
        "and they stay there"
    );
}

#[test]
fn a_descending_lift_carries_the_player_down_with_it_instead_of_leaving_them_in_the_air() {
    // The box drops 0.35 m in 2 ticks, much faster than the player would fall on their own.
    let mut w = world_with(V(0., 0.35, 0.), 2, true, false);
    stand(&mut w, -3.0, 2.15, 1.0); // the box starts raised by 0.35 m: its top is at 2.15
    let mut worst_gap = 0.0_f32;
    for _ in 0..12 {
        w.step();
        let top = w.game.as_ref().unwrap().mover_bounds(0).unwrap().max.1;
        worst_gap = worst_gap.max((w.player(1).unwrap().feet_height() - top).abs());
    }
    assert!(
        worst_gap < 0.02,
        "the player stayed on the box the whole way down (worst gap {worst_gap})"
    );
    let (feet, _, grounded) = feet_and_x(&w);
    assert!((feet - 1.8).abs() < 0.02 && grounded, "feet at {feet}");
}

#[test]
fn a_sliding_platform_carries_the_player_sideways() {
    let mut w = world(V(0., 0., 1.5), false, true);
    stand(&mut w, -3.0, 1.8, 1.0);
    run(&mut w, 60);
    let c = w.player(1).unwrap();
    assert!((c.position.2 - 2.5).abs() < 0.02, "z {}", c.position.2);
    assert!((c.feet_height() - 1.8).abs() < 0.02 && c.is_grounded());
}

#[test]
fn a_player_beside_the_platform_is_left_alone() {
    let mut w = world(V(0., 0.35, 0.), false, true);
    stand(&mut w, -2.0, 0.0, 1.0); // clear of both neighbouring boxes
    let before = w.player(1).unwrap().position;
    run(&mut w, 60);
    assert_eq!(w.player(1).unwrap().position, before);
}

#[test]
fn a_sliding_box_never_ends_up_overlapping_a_player_in_its_path() {
    // Ordinary collision moves the player out of the way; riding must not disturb that.
    let mut w = world(V(0., 0., 1.0), false, true);
    stand(&mut w, -3.0, 0.0, 1.9);
    for _ in 0..60 {
        w.step();
        let bounds = w.game.as_ref().unwrap().mover_bounds(0).unwrap();
        let c = w.player(1).unwrap();
        assert!(!bounds.overlaps_body(c.position, c.feet_height(), c.body_height(), 0.23));
    }
    assert!(
        w.player(1).unwrap().position.2 > 2.3,
        "the player was pushed ahead of the box"
    );
}

#[test]
fn a_player_can_still_walk_and_jump_off_while_riding() {
    let mut w = world(V(0., 0.35, 0.), false, true);
    stand(&mut w, -3.0, 1.8, 1.0);
    run(&mut w, 30);
    // Walk off the side of the raised box: they drop to the floor, so riding did not glue them to it.
    let walk = Movement {
        right: 1.0,
        ..Movement::default()
    };
    for _ in 0..90 {
        w.input(1, walk, 0.0, 0.0);
        w.step();
    }
    let (feet, x, grounded) = feet_and_x(&w);
    assert!(x > -2.4, "walked off to the side: x {x}");
    assert!(feet < 0.05 && grounded, "landed on the floor: feet {feet}");
}

#[test]
fn riding_is_deterministic() {
    let checksum = || {
        let mut w = world(V(0., 0.35, 0.), false, true);
        stand(&mut w, -3.0, 1.8, 1.0);
        run(&mut w, 45);
        w.checksum()
    };
    assert_eq!(checksum(), checksum());
}
