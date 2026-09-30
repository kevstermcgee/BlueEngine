//! Scenario intents: `walk_to` and `face` read as "go there, look at that", and change nothing for old inputs.
use serde_json::json;
use std::path::PathBuf;
use vesper3d::{
    math::V,
    viewer::{
        controller::Movement,
        game::GameDocument,
        game_example,
        scenario::{evaluate_scenario, verify_replay_trace, Scenario},
    },
};

/// The three-switches example in a fresh directory: switches at x = -3, -1, 1 and the exit at x = 3, all at z = 1.
fn game(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("be2-intents-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    game_example::write(&dir).unwrap();
    dir.join("game.json")
}
fn scenario(
    game: &PathBuf,
    ticks: u64,
    inputs: serde_json::Value,
    assertions: serde_json::Value,
) -> Scenario {
    serde_json::from_value(json!({
        "name": "intents", "game_path": game, "ticks": ticks,
        "players": [{"id": 1}], "inputs": inputs, "assertions": assertions,
    }))
    .unwrap()
}
fn failures(s: &Scenario) -> Vec<String> {
    evaluate_scenario(s)
        .unwrap()
        .assertions
        .into_iter()
        .filter(|a| !a.ok)
        .map(|a| a.detail)
        .collect()
}

#[test]
fn walk_to_arrives_close_to_the_point_and_holds_later_inputs_until_it_does() {
    let g = game("walk");
    // The press is due at tick 2, but the player is still walking then: it must wait for arrival.
    let s = scenario(
        &g,
        200,
        json!([
            {"tick": 1, "player": 1, "walk_to": [-1.0, 3.0]},
            {"tick": 2, "player": 1, "face": "button-b", "interact": true}]),
        json!([
            {"tick": 20, "counter": "switches", "counter_equals": 0},
            {"tick": 199, "counter": "switches", "counter_equals": 1, "target": "button-b", "enabled_equals": false},
            {"tick": 199, "player": 1, "position_near": [-1.0, 1.68, 3.0], "tolerance": 0.15}]),
    );
    assert_eq!(failures(&s), Vec::<String>::new());
}

#[test]
fn face_aims_at_a_target_from_an_angle_and_a_distance_that_a_fixed_yaw_would_miss() {
    let g = game("face");
    // Standing off to one side (2.2 m away, in range): yaw 0 (straight ahead) would not hit button-c at x = 1.
    let s = scenario(
        &g,
        200,
        json!([
            {"tick": 1, "player": 1, "walk_to": [-0.5, 2.6]},
            {"tick": 1, "player": 1, "face": "button-c", "interact": true}]),
        json!([{"tick": 199, "counter": "switches", "counter_equals": 1, "target": "button-c", "enabled_equals": false}]),
    );
    assert_eq!(failures(&s), Vec::<String>::new());
    // Control: the same walk and press with the default heading (straight ahead) presses nothing.
    let blind = scenario(
        &g,
        200,
        json!([
            {"tick": 1, "player": 1, "walk_to": [-0.5, 2.6]},
            {"tick": 1, "player": 1, "interact": true}]),
        json!([{"tick": 199, "counter": "switches", "counter_equals": 1}]),
    );
    assert!(
        !failures(&blind).is_empty(),
        "without face the press should miss"
    );
}

#[test]
fn walk_to_without_face_keeps_the_players_heading() {
    let g = game("heading");
    let s = scenario(
        &g,
        120,
        json!([
            {"tick": 1, "player": 1, "yaw": 0.0, "face": "button-c"},
            {"tick": 2, "player": 1, "walk_to": [-3.0, 4.0]}]),
        json!([{"tick": 119, "player": 1, "position_near": [-3.0, 1.68, 4.0], "tolerance": 0.15}]),
    );
    let report = evaluate_scenario(&s).unwrap();
    assert!(report.ok, "{:?}", report.assertions);
    let facing_c = report.players[0].yaw;
    assert!(
        facing_c > 0.5,
        "still facing east after the walk, not reset to 0: {facing_c}"
    );
}

#[test]
fn problems_a_scenario_cannot_assert_are_reported_as_failures() {
    let g = game("problems");
    let s = scenario(
        &g,
        30,
        json!([{"tick": 1, "player": 1, "face": "no-such-entity"}]),
        json!([]),
    );
    let bad = failures(&s);
    assert!(
        bad.iter()
            .any(|d| d.contains("cannot face 'no-such-entity'")),
        "{bad:?}"
    );
    // Walls stop the walk; it is reported, not left to hang.
    let stuck = scenario(
        &g,
        1900,
        json!([{"tick": 1, "player": 1, "walk_to": [500.0, 500.0]}]),
        json!([]),
    );
    let bad = failures(&stuck);
    assert!(bad.iter().any(|d| d.contains("did not reach")), "{bad:?}");
}

#[test]
fn a_walking_scenario_replays_deterministically() {
    let g = game("replay");
    let s = scenario(
        &g,
        200,
        json!([
            {"tick": 1, "player": 1, "walk_to": [-3.0, 3.0]},
            {"tick": 1, "player": 1, "face": "button-a", "interact": true},
            {"tick": 1, "player": 1, "walk_to": [-1.0, 3.0]},
            {"tick": 1, "player": 1, "face": "button-b", "interact": true}]),
        json!([{"tick": 199, "counter": "switches", "counter_equals": 2}]),
    );
    let report = evaluate_scenario(&s).unwrap();
    assert!(report.ok, "{:?}", report.assertions);
    let replay = verify_replay_trace(&report.trace, None).unwrap();
    assert!(replay.deterministic, "{replay:?}");
    assert!(replay.verified_checkpoints > 0);
}

/// Old-style inputs (no walk_to, no face) behave exactly as before the driver existed.
#[test]
fn inputs_without_intents_match_the_original_loop_exactly() {
    let g = game("legacy");
    let inputs = json!([
        {"tick": 1, "player": 1, "right": 1.0, "yaw": 0.0},
        {"tick": 38, "player": 1, "right": 0.0, "yaw": 0.0},
        {"tick": 50, "player": 1, "interact": true, "yaw": 0.0},
        {"tick": 51, "player": 1, "forward": -1.0, "yaw": 1.0, "jump": true}]);
    let s = scenario(&g, 90, inputs, json!([]));
    let via_driver = evaluate_scenario(&s)
        .unwrap()
        .trace
        .checkpoints
        .last()
        .unwrap()
        .checksum;

    let mut world = GameDocument::load(&g).unwrap().world().unwrap();
    world.join(1);
    for tick in 1..=90u64 {
        for i in s.inputs.iter().filter(|i| i.tick == tick) {
            let mv = Movement {
                forward: i.forward,
                right: i.right,
                sprint: i.sprint,
                jump: i.jump,
                crouch: i.crouch,
            };
            world.input(i.player, mv, i.yaw, i.pitch);
            if i.interact {
                world.request_interaction(i.player);
            }
        }
        world.step();
    }
    assert_eq!(via_driver, world.checksum());
    let _ = V(0., 0., 0.);
}
