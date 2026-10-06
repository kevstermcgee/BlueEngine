//! Canonical authoring, dynamic routing, executed-step sequencing and stock HUD acceptance.
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use vesper3d::{
    math::V,
    viewer::{
        controller::Collider,
        game::{GameDocument, LoadedGame},
        game_explore::explore,
        game_scenario::win_scenario,
        game_session::GameSession,
        pathing::plan_route_with_profile,
        profile::ControllerProfile,
        scenario::{evaluate_scenario, load_scenario, InputDriver, Scenario},
    },
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/games/observatory/content")
        .join(name)
}
fn loaded(name: &str) -> LoadedGame {
    GameDocument::load(&fixture(name)).unwrap()
}

#[test]
fn canonical_authoring_and_fractional_edit_reproduce_strictly_valid_content() {
    let dir = std::env::temp_dir().join(format!("be2-observatory-author-{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    let python = if cfg!(windows) { "python" } else { "python3" };
    let output = Command::new(python)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/games/observatory/author.py"))
        .arg(&dir)
        .args(["--tools", env!("CARGO_BIN_EXE_be2-tools")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let made = GameDocument::load(&dir.join("game.json")).unwrap();
    let expected = loaded("game.json");
    assert_eq!(
        serde_json::to_value(made.document).unwrap(),
        serde_json::to_value(&expected.document).unwrap()
    );
    assert_eq!(
        serde_json::to_value(made.map).unwrap(),
        serde_json::to_value(&expected.map).unwrap()
    );
    let evidence: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("authoring-evidence.json")).unwrap())
            .unwrap();
    assert_eq!(
        evidence
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["command"][0] == "add-interactable")
            .count(),
        3
    );
    // A mismatched record is still refused; translation must not implicitly repair it.
    let mut invalid = expected.map.clone();
    invalid.colliders.get_mut("calibrator").unwrap().min.2 += 0.001;
    let op = serde_json::from_value(json!({"op":"translate", "nodes":["calibrator"],
        "colliders":["calibrator"], "entities":["calibrator"], "delta":[0,0,0.2]}))
    .unwrap();
    let edited = invalid.apply(&[op]).unwrap();
    assert!(expected.document.validate(&edited).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn changing_shutter_collision_and_turning_route_are_executed_before_the_win() {
    let g = loaded("game.json");
    let mut world = g.clone().world().unwrap();
    world.join(1);
    let profile = g.document.player_profile;
    let from = V(-4., 0., 0.);
    let to = V(3.5, 0., 1.);
    assert!(plan_route_with_profile(&world.room.colliders, from, to, profile).is_err());
    let path = fixture("game.json").to_string_lossy().into_owned();
    let scenario = win_scenario(&g, &explore(&g, 10_000).unwrap(), &path, &path)
        .unwrap()
        .scenario;
    let mut driver = InputDriver::new(&scenario.inputs);
    let mut went_around = false;
    let mut opened_route = false;
    for tick in 1..=scenario.ticks {
        driver.before_step(&mut world, tick);
        world.step();
        let player = world.player(1).unwrap();
        went_around |= player.position.2.abs() > 2.3 && player.position.0 > 0.;
        if tick == 120 {
            let route = plan_route_with_profile(&world.room.colliders, from, to, profile).unwrap();
            opened_route = route.len() > 2;
        }
    }
    assert!(driver.problems().is_empty(), "{:?}", driver.problems());
    assert!(
        went_around && opened_route,
        "must turn around the baffle after the shutter opens"
    );
    assert!(world.game.as_ref().unwrap().state().completed);
    assert!(evaluate_scenario(&scenario).unwrap().ok);
}

#[test]
fn game_lint_keeps_initial_findings_and_requires_individual_physical_proof() {
    use vesper3d::viewer::lint::{lint_game, lint_map};
    let g = loaded("game.json");
    let static_report = lint_map(&g.map, false, &[]);
    assert_eq!(static_report.errors, 2);
    let mut scenario = load_scenario(&fixture("win.json")).unwrap();
    let proven = lint_game(&g.map, &g, &scenario).unwrap();
    assert!(proven.ok);
    assert_eq!(
        proven
            .findings
            .iter()
            .filter(|f| f.code == "initial-unreachable-now-reached")
            .count(),
        2
    );
    // A passing loss-only run has no reachability proof; merely having a mover proves nothing.
    scenario.inputs.clear();
    scenario.ticks = 1200;
    scenario.assertions =
        vec![serde_json::from_value(json!({"tick":1200,"failed_equals":true})).unwrap()];
    let unresolved = lint_game(&g.map, &g, &scenario).unwrap();
    assert!(!unresolved.ok && unresolved.errors == 2);
    let mut wrong = load_scenario(&fixture("win.json")).unwrap();
    wrong.game_path = Some(fixture("closed-gate.json").to_string_lossy().into_owned());
    assert!(lint_game(&g.map, &g, &wrong)
        .unwrap_err()
        .to_string()
        .contains("differs"));
    let negative = loaded("closed-gate.json");
    assert!(lint_game(&negative.map, &negative, &wrong)
        .unwrap_err()
        .to_string()
        .contains("failed physical"));
    wrong = load_scenario(&fixture("win.json")).unwrap();
    wrong.players[0].spawn = Some([5., 1.68, 1.]);
    assert!(lint_game(&g.map, &g, &wrong)
        .unwrap_err()
        .to_string()
        .contains("spawn overrides"));
}

#[test]
fn never_opening_gate_is_abstractly_winnable_but_physically_unproven() {
    let g = loaded("closed-gate.json");
    let report = explore(&g, 10_000).unwrap();
    assert_eq!(report.winnable, Some(true));
    let path = fixture("closed-gate.json").to_string_lossy().into_owned();
    let error = win_scenario(&g, &report, &path, &path)
        .unwrap_err()
        .to_string();
    assert!(error.contains("physical completion unproven"), "{error}");
    let output = Command::new(env!("CARGO_BIN_EXE_be2-tools"))
        .args(["game-explore", &path, "--scenario=unused-negative.json"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["scenario"]["verified"], false);
    assert!(json["scenario"]["written"].is_null());
}

#[test]
fn overdue_repeated_presses_after_a_blocking_walk_use_separate_executed_steps() {
    let g = loaded("game.json");
    let win = load_scenario(&fixture("win.json")).unwrap();
    let mut inputs = win.inputs;
    inputs.retain(|i| i.face.as_deref() != Some("calibrator") && i.tick < 280);
    for tick in 22..=24 {
        inputs.push(
            serde_json::from_value(json!({"tick":tick,"player":1,
            "face":"calibrator","interact":true,"wait_ticks":1}))
            .unwrap(),
        );
    }
    let run = |inputs| {
        let s: Scenario = serde_json::from_value(json!({"name":"queued presses",
            "game_path":fixture("game.json"), "ticks":310, "players":[{"id":1}],
            "inputs":inputs, "assertions":[{"tick":310,"counter":"calibration","counter_equals":3}]})).unwrap();
        evaluate_scenario(&s).unwrap()
    };
    assert!(run(inputs.clone()).ok);
    // Different timestamps become overdue together after walking: authority still coalesces them.
    for input in &mut inputs {
        input.wait_ticks = 0;
    }
    assert!(!run(inputs).ok);
    assert_eq!(g.document.counters["calibration"], 0);
}

#[test]
fn loss_restart_success_and_rendered_session_share_authoritative_state() {
    let scenario = load_scenario(&fixture("loss-restart-win.json")).unwrap();
    let report = evaluate_scenario(&scenario).unwrap();
    assert!(report.ok, "{:?}", report.assertions);
    let mut session = GameSession::local(loaded("game.json")).unwrap();
    let mut driver = InputDriver::new(&scenario.inputs);
    for tick in 1..=scenario.ticks {
        session.advance_scenario(&mut driver, tick).unwrap();
    }
    assert_eq!(
        session.world().checksum(),
        report.trace.checkpoints.last().unwrap().checksum
    );
    let state = session.world().game.as_ref().unwrap().state();
    assert!(state.completed && !state.failed && state.round == 1);
}

#[test]
fn routing_uses_the_active_body_dimensions() {
    let colliders = [
        Collider {
            min: V(-3., -0.2, -3.),
            max: V(3., 0., 3.),
        },
        Collider {
            min: V(-3., 0., -3.),
            max: V(3., 3., -0.4),
        },
        Collider {
            min: V(-3., 0., 0.4),
            max: V(3., 3., 3.),
        },
    ];
    let small = ControllerProfile {
        radius: 0.1,
        ..Default::default()
    };
    let large = ControllerProfile {
        radius: 0.5,
        ..small
    };
    assert!(plan_route_with_profile(&colliders, V(-2., 0., 0.), V(2., 0., 0.), small).is_ok());
    assert!(plan_route_with_profile(&colliders, V(-2., 0., 0.), V(2., 0., 0.), large).is_err());
}

#[test]
fn stock_presentation_formats_authority_and_preserves_legacy_defaults() {
    let g = loaded("game.json");
    let world = g.clone().world().unwrap();
    let state = world.game.as_ref().unwrap().state();
    let display = g.document.presentation.as_ref().unwrap();
    assert_eq!(
        display.status(&g.document, state),
        "Battery: 0:20   Calibration: 0 / 3"
    );
    let mut won = state.clone();
    won.completed = true;
    assert!(display
        .status(&g.document, &won)
        .starts_with("Observation transmitted."));
    won.completed = false;
    won.failed = true;
    assert!(display
        .status(&g.document, &won)
        .starts_with("Battery depleted."));
    let (legacy, map) = vesper3d::viewer::game_example::documents().unwrap();
    let value = serde_json::to_value(&legacy).unwrap();
    assert!(value.get("presentation").is_none());
    let old: GameDocument = serde_json::from_value(value).unwrap();
    old.validate(&map).unwrap();
    let mut invalid = display.clone();
    invalid.hud.scale = 2.;
    assert!(invalid.validate(&g.document.counters).is_err());
    invalid = display.clone();
    invalid
        .counters
        .insert("invented".into(), Default::default());
    assert!(invalid.validate(&g.document.counters).is_err());
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../tools/game.schema.json")).unwrap();
    assert!(schema["properties"]["presentation"].is_object());
}
