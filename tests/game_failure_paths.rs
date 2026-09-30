//! Behaviour beyond the winning path, played through the real runtime and the ordinary scenario runner:
//! losing on a timer, restarting and winning again, presses in the wrong order, retrying after a mistake,
//! repeating timer cycles and the once-per-match rule.
//!
//! The game is an ordered puzzle on the example map: press A, then B, then C, then the exit, within 600 ticks.
//! A wrong press of B or C resets the puzzle and counts a mistake. A 60-tick repeating clock counts cycles.
//! A `once` bonus rule on A shows that `once` is per match: a retry does not repeat it, a restart does.
use serde_json::{json, Value};
use std::path::PathBuf;
use vesper3d::viewer::{game_example, scenario::evaluate_scenario, scenario::Scenario};

fn act(value: Value) -> Value {
    value
}

fn rule(
    id: &str,
    trigger: (&str, &str),
    condition: Value,
    once: bool,
    actions: Vec<Value>,
) -> Value {
    let mut r = json!({"id": id, "condition": condition, "once": once, "actions": actions});
    r[trigger.0] = json!(trigger.1);
    r
}

fn inc(counter: &str) -> Value {
    act(json!({"action":"increment","counter":counter,"amount":1}))
}
fn set(counter: &str, value: i32) -> Value {
    act(json!({"action":"set_counter","counter":counter,"value":value}))
}
fn eq(counter: &str, value: i32) -> Value {
    json!({"counter": counter, "equals": value})
}
fn ne(counter: &str, value: i32) -> Value {
    json!({"counter": counter, "not_equals": value})
}

/// The puzzle game. `with_deadline: false` removes the losing timer rule, for the negative checks.
fn write_game(name: &str, with_deadline: bool, with_reset_rules: bool) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("be2-paths-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    game_example::write(&dir).unwrap();
    let mut doc: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("game.json")).unwrap()).unwrap();
    doc["counters"] = json!({"step": 0, "mistakes": 0, "cycles": 0, "bonus": 0});
    doc["timers"] = json!([
        {"id": "clock", "duration_ticks": 60, "auto_start": true, "repeats": true},
        {"id": "deadline", "duration_ticks": 600, "auto_start": true, "repeats": false}
    ]);
    let mut rules = Vec::new();
    // Wrong presses first: they see the counter before the right-press rules change it.
    if with_reset_rules {
        rules.push(rule(
            "wrong-b",
            ("on_interact", "button-b"),
            ne("step", 1),
            false,
            vec![set("step", 0), inc("mistakes")],
        ));
        rules.push(rule(
            "wrong-c",
            ("on_interact", "button-c"),
            ne("step", 2),
            false,
            vec![set("step", 0), inc("mistakes")],
        ));
    }
    rules.push(rule(
        "a",
        ("on_interact", "button-a"),
        eq("step", 0),
        false,
        vec![inc("step")],
    ));
    rules.push(rule(
        "a-bonus",
        ("on_interact", "button-a"),
        Value::Null,
        true,
        vec![inc("bonus")],
    ));
    rules.push(rule(
        "b",
        ("on_interact", "button-b"),
        eq("step", 1),
        false,
        vec![inc("step")],
    ));
    rules.push(rule(
        "c",
        ("on_interact", "button-c"),
        eq("step", 2),
        false,
        vec![
            inc("step"),
            json!({"action":"set_enabled","entity":"exit","enabled":true}),
        ],
    ));
    rules.push(rule(
        "finish",
        ("on_interact", "exit"),
        Value::Null,
        false,
        vec![json!({"action":"complete"})],
    ));
    rules.push(rule(
        "tick",
        ("on_timer", "clock"),
        Value::Null,
        false,
        vec![inc("cycles")],
    ));
    if with_deadline {
        rules.push(rule(
            "time-up",
            ("on_timer", "deadline"),
            Value::Null,
            false,
            vec![json!({"action":"fail"})],
        ));
    }
    doc["rules"] = json!(rules);
    std::fs::write(
        dir.join("game.json"),
        serde_json::to_string_pretty(&doc).unwrap(),
    )
    .unwrap();
    dir
}

/// Walk to a button's usual standing point, face it and press.
fn press(at_tick: u64, target: &str) -> Vec<Value> {
    let x = match target {
        "button-a" => -3.0,
        "button-b" => -1.0,
        "button-c" => 1.0,
        _ => 3.0,
    };
    vec![
        json!({"tick": at_tick, "player": 1, "walk_to": [x, 2.5]}),
        json!({"tick": at_tick, "player": 1, "face": target, "interact": true}),
    ]
}

fn counter(tick: u64, name: &str, value: i32) -> Value {
    json!({"tick": tick, "counter": name, "counter_equals": value})
}
fn outcome(tick: u64, completed: bool, failed: bool) -> Value {
    json!({"tick": tick, "tolerance": 0.0, "completed_equals": completed, "failed_equals": failed})
}

struct Run {
    ok: bool,
    failures: Vec<String>,
}

fn play(dir: &std::path::Path, ticks: u64, inputs: Vec<Value>, assertions: Vec<Value>) -> Run {
    let scenario: Scenario = serde_json::from_value(json!({
        "name": "failure paths",
        "game_path": dir.join("game.json").to_string_lossy(),
        "ticks": ticks,
        "players": [{"id": 1}],
        "inputs": inputs,
        "assertions": assertions,
    }))
    .unwrap();
    let report = evaluate_scenario(&scenario).unwrap();
    let failures = report
        .assertions
        .iter()
        .filter(|a| !a.ok)
        .map(|a| format!("{a:?}"))
        .collect();
    Run {
        ok: report.ok,
        failures,
    }
}

fn winning_route(start: u64) -> Vec<Value> {
    let mut inputs = Vec::new();
    for (i, target) in ["button-a", "button-b", "button-c", "exit"]
        .iter()
        .enumerate()
    {
        inputs.extend(press(start + 50 * i as u64, target));
    }
    inputs
}

#[test]
fn the_winning_route_wins_with_no_mistakes_and_one_bonus() {
    let dir = write_game("win", true, true);
    let run = play(
        &dir,
        240,
        winning_route(1),
        vec![
            counter(230, "mistakes", 0),
            counter(230, "bonus", 1),
            counter(230, "step", 3),
            outcome(232, true, false),
        ],
    );
    assert!(run.ok, "{:?}", run.failures);
}

#[test]
fn running_out_the_timer_loses_and_a_loss_freezes_the_match() {
    let dir = write_game("loss", true, true);
    // Nobody does anything for the whole deadline; then a press on A is ignored because the match is over.
    let mut inputs = press(640, "button-a");
    inputs.truncate(2);
    let run = play(
        &dir,
        700,
        inputs,
        vec![
            outcome(300, false, false),
            counter(300, "cycles", 5),
            outcome(610, false, true),
            counter(690, "step", 0),
            counter(690, "bonus", 0),
            outcome(690, false, true),
        ],
    );
    assert!(run.ok, "{:?}", run.failures);
}

#[test]
fn a_lost_match_restarts_and_can_then_be_won() {
    let dir = write_game("restart", true, true);
    let mut inputs = Vec::new();
    // Lose at tick 600, restart as a player pressing the action button does, then win in the new round.
    inputs.push(json!({"tick": 620, "player": 1, "restart": true}));
    inputs.extend(winning_route(640));
    let run = play(
        &dir,
        960,
        inputs,
        vec![
            counter(610, "cycles", 10),
            outcome(610, false, true),
            // Right after the restart everything is back to the start, including the clock's cycle count.
            outcome(630, false, false),
            counter(630, "step", 0),
            counter(630, "mistakes", 0),
            counter(630, "bonus", 0),
            counter(630, "cycles", 0),
            // The new round is winnable, and it is a fresh match: the `once` bonus is available again.
            counter(930, "bonus", 1),
            counter(930, "step", 3),
            outcome(940, true, false),
        ],
    );
    assert!(run.ok, "{:?}", run.failures);
}

#[test]
fn the_exit_does_nothing_until_the_puzzle_is_solved() {
    let dir = write_game("early-exit", true, true);
    let run = play(
        &dir,
        120,
        press(1, "exit"),
        vec![
            json!({"tick": 100, "target": "exit", "enabled_equals": false}),
            outcome(100, false, false),
            counter(100, "step", 0),
        ],
    );
    assert!(run.ok, "{:?}", run.failures);
}

#[test]
fn pressing_in_the_wrong_order_counts_a_mistake_and_a_retry_still_wins() {
    let dir = write_game("wrong-order", true, true);
    let mut inputs = press(1, "button-b"); // wrong: B before A
    inputs.extend(press(40, "button-c")); // wrong again: C before A
    inputs.extend(winning_route(90));
    let run = play(
        &dir,
        360,
        inputs,
        vec![
            counter(80, "mistakes", 1),
            counter(80, "step", 0),
            counter(110, "mistakes", 2),
            counter(110, "step", 0),
            counter(330, "mistakes", 2),
            counter(330, "step", 3),
            outcome(350, true, false),
        ],
    );
    assert!(run.ok, "{:?}", run.failures);
}

#[test]
fn a_mistake_half_way_through_resets_progress_but_the_once_bonus_is_not_given_twice() {
    let dir = write_game("retry", true, true);
    let mut inputs = press(1, "button-a"); // right: step 1, bonus 1
    inputs.extend(press(50, "button-c")); // wrong at step 1: back to the start
    inputs.extend(winning_route(100)); // A again (bonus must not repeat), then B, C, exit
    let run = play(
        &dir,
        400,
        inputs,
        vec![
            counter(40, "step", 1),
            counter(40, "bonus", 1),
            counter(150, "step", 0),
            counter(150, "mistakes", 1),
            counter(150, "bonus", 1),
            counter(340, "step", 3),
            counter(340, "bonus", 1),
            counter(340, "mistakes", 1),
            outcome(390, true, false),
        ],
    );
    assert!(run.ok, "{:?}", run.failures);
}

#[test]
fn a_repeating_timer_fires_once_per_cycle() {
    let dir = write_game("cycles", true, true);
    let run = play(
        &dir,
        400,
        vec![],
        vec![
            counter(59, "cycles", 0),
            counter(62, "cycles", 1),
            counter(125, "cycles", 2),
            counter(185, "cycles", 3),
            counter(365, "cycles", 6),
        ],
    );
    assert!(run.ok, "{:?}", run.failures);
}

// Mutation checks: the same scenarios must FAIL against a game that lacks the behaviour they verify, or the
// assertions above would be passing for the wrong reason.

#[test]
fn without_the_deadline_rule_the_loss_scenario_fails() {
    let dir = write_game("no-deadline", false, true);
    let run = play(&dir, 700, vec![], vec![outcome(610, false, true)]);
    assert!(
        !run.ok,
        "a game that never fails must not satisfy failed_equals"
    );
}

#[test]
fn without_the_reset_rules_the_wrong_order_scenario_fails() {
    let dir = write_game("no-reset", true, false);
    let mut inputs = press(1, "button-b");
    inputs.extend(winning_route(50));
    let run = play(
        &dir,
        300,
        inputs,
        vec![counter(45, "mistakes", 1), counter(290, "mistakes", 1)],
    );
    assert!(
        !run.ok,
        "mistakes are never counted without the reset rules"
    );
}

#[test]
fn restarting_a_match_that_is_still_running_fails_the_scenario() {
    let dir = write_game("early-restart", true, true);
    let run = play(
        &dir,
        30,
        vec![json!({"tick": 10, "player": 1, "restart": true})],
        vec![],
    );
    assert!(
        !run.ok,
        "a restart at the wrong moment must not pass quietly"
    );
}
