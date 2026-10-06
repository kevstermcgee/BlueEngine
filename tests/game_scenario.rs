//! `game-explore --scenario`: the shortest win, played as a scenario that the ordinary runner verifies.
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};
use vesper3d::viewer::{
    game::{GameDocument, LoadedGame},
    game_example,
    game_explore::explore,
    game_scenario::{game_path_relative_to, win_scenario},
    scenario::evaluate_scenario,
};

fn example_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("be2-genscn-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    game_example::write(&dir).unwrap();
    dir
}
/// The example game with its rules replaced, using only the targets the rules name.
fn variant(counters: Value, timers: Value, rules: Value) -> LoadedGame {
    let (document, map) = game_example::documents().unwrap();
    let mut doc = serde_json::to_value(&document).unwrap();
    doc["counters"] = counters;
    doc["timers"] = timers;
    doc["rules"] = rules;
    let named: Vec<&str> = ["button-a", "button-b", "button-c", "exit"]
        .into_iter()
        .filter(|t| doc["rules"].to_string().contains(&format!("\"{t}\"")))
        .collect();
    doc["interactables"] = json!(named
        .iter()
        .map(|e| json!({"entity": e, "enabled": true, "visible": true}))
        .collect::<Vec<_>>());
    let document: GameDocument = serde_json::from_value(doc).unwrap();
    document.validate(&map).unwrap();
    LoadedGame { document, map }
}
fn rule(id: &str, trigger: Value, condition: Value, actions: Value) -> Value {
    let mut r = json!({"id": id, "condition": condition, "once": false, "actions": actions});
    for (k, v) in trigger.as_object().unwrap() {
        r[k] = v.clone();
    }
    r
}

#[test]
fn the_example_games_shortest_win_becomes_a_scenario_that_the_runner_confirms() {
    let dir = example_dir("example");
    let loaded = GameDocument::load(&dir.join("game.json")).unwrap();
    let report = explore(&loaded, 10_000).unwrap();
    let path = dir.join("game.json").to_string_lossy().into_owned();
    let made = win_scenario(&loaded, &report, &path, &path).unwrap();
    let presses = made.scenario.inputs.iter().filter(|i| i.interact).count();
    assert_eq!(
        presses,
        report.win_events.len(),
        "one press per planned event"
    );
    assert!(made
        .scenario
        .inputs
        .iter()
        .filter(|i| i.interact)
        .all(|i| i.face.is_some()));
    let run = evaluate_scenario(&made.scenario).unwrap();
    assert!(run.ok, "{:?}", run.assertions);
}

#[test]
fn maintained_timed_relay_runs_success_expiry_and_restart_on_real_authority() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/games/timed-relay/content");
    for name in [
        "success.json",
        "deadline-boundary.json",
        "loss-restart-success.json",
    ] {
        let scenario = vesper3d::viewer::scenario::load_scenario(&root.join(name)).unwrap();
        let run = evaluate_scenario(&scenario).unwrap();
        assert!(run.ok, "{name}: {:?}", run.assertions);
        assert!(
            !run.assertions.is_empty(),
            "{name} must assert authority, not just finish"
        );
        // Exercise the stock client's existing frame adapter against the same authority.
        let mut session = vesper3d::viewer::game_session::GameSession::local(
            GameDocument::load(&root.join("game.json")).unwrap(),
        )
        .unwrap();
        let initial = session.world().game.as_ref().unwrap().state().clone();
        let mut driver = vesper3d::viewer::scenario::InputDriver::new(&scenario.inputs);
        for tick in 1..=scenario.ticks {
            session.advance_scenario(&mut driver, tick).unwrap();
            if name == "loss-restart-success.json" && tick == 1301 {
                let mut reset = initial.clone();
                reset.round = 1;
                assert_eq!(
                    session.world().game.as_ref().unwrap().state(),
                    &reset,
                    "restart must reset timers, once flags, counters and target eligibility"
                );
            }
        }
        let state = session.world().game.as_ref().unwrap().state();
        assert_eq!(
            state.active_timers, 0,
            "{name}: one-shot expiry or completion must clear the timer"
        );
        assert_eq!(
            session.world().checksum(),
            run.trace.checkpoints.last().unwrap().checksum,
            "{name}: stock frame adapter and headless runner must share authority"
        );
    }
}

#[test]
fn a_press_that_is_only_right_in_one_phase_of_a_timer_waits_for_that_phase() {
    // The exit only counts while the clock's phase is odd. The player arrives at an even phase and has to wait.
    let g = variant(
        json!({"phase": 0}),
        json!([{"id": "clock", "duration_ticks": 45, "auto_start": true, "repeats": true}]),
        json!([
            rule(
                "tick",
                json!({"on_timer": "clock"}),
                Value::Null,
                json!([{"action":"increment","counter":"phase","amount":1}])
            ),
            rule(
                "open",
                json!({"on_interact": "exit"}),
                json!({"counter":"phase","modulo":2,"equals":1}),
                json!([{"action":"complete"}])
            ),
        ]),
    );
    let report = explore(&g, 10_000).unwrap();
    assert_eq!(
        report.shortest_win.clone().unwrap(),
        ["timer clock runs out", "press exit"]
    );
    // The verifying run needs a real file, so write the variant out.
    let dir = std::env::temp_dir().join(format!("be2-genscn-phase-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut doc = serde_json::to_value(&g.document).unwrap();
    doc["map"] = "map.json".into();
    std::fs::write(
        dir.join("game.json"),
        serde_json::to_vec_pretty(&doc).unwrap(),
    )
    .unwrap();
    std::fs::write(
        dir.join("map.json"),
        serde_json::to_vec_pretty(&g.map).unwrap(),
    )
    .unwrap();
    let file = dir.join("game.json").to_string_lossy().into_owned();
    let made = win_scenario(&g, &report, &file, &file).unwrap();
    let press = made.scenario.inputs.iter().find(|i| i.interact).unwrap();
    assert!(
        press.tick >= 45,
        "the press waited for the first pulse to make the phase odd: tick {}",
        press.tick
    );
    assert!(evaluate_scenario(&made.scenario).unwrap().ok);
}

#[test]
fn losing_or_zone_only_paths_are_refused_with_a_reason() {
    let dir = example_dir("refuse");
    let loaded = GameDocument::load(&dir.join("game.json")).unwrap();
    let mut report = explore(&loaded, 10_000).unwrap();
    let p = "x.json";
    report.win_events.clear();
    assert!(win_scenario(&loaded, &report, p, p)
        .unwrap_err()
        .to_string()
        .contains("no winning path"));
    let mut report = explore(&loaded, 10_000).unwrap();
    report.win_events = vec![vesper3d::viewer::game::ModelEvent::EnterZone(0)];
    assert!(win_scenario(&loaded, &report, p, p)
        .unwrap_err()
        .to_string()
        .contains("trigger zones"));
}

#[test]
fn the_scenario_names_the_game_relative_to_where_it_is_saved() {
    let dir = example_dir("paths");
    let game = dir.join("game.json");
    assert_eq!(
        game_path_relative_to(&dir.join("out.json"), &game).unwrap(),
        "game.json"
    );
    let elsewhere = std::env::temp_dir();
    let named = game_path_relative_to(&elsewhere.join("out.json"), &game).unwrap();
    assert!(
        PathBuf::from(&named).is_absolute() && named.ends_with("game.json"),
        "{named}"
    );
}

#[test]
fn the_command_line_writes_a_scenario_that_sim_then_passes() {
    let dir = example_dir("cli");
    let tool = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_be2-tools"))
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap()
    };
    let made = tool(&["game-explore", "game.json", "--scenario=win.json"]);
    let report: Value = serde_json::from_slice(&made.stdout).unwrap();
    assert!(made.status.success(), "{report}");
    assert_eq!(report["scenario"]["verified"], true, "{report}");
    let sim = tool(&["sim", "win.json"]);
    let out: Value = serde_json::from_slice(&sim.stdout).unwrap();
    assert!(sim.status.success() && out["ok"] == true, "{out}");
    assert!(
        out["players"][0]["position"].is_array(),
        "the sim reports where the player ended up"
    );
    // Existing files are never overwritten.
    let again = tool(&["game-explore", "game.json", "--scenario=win.json"]);
    assert!(!again.status.success());
}
