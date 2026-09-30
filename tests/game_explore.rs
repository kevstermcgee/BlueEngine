//! `game-explore`: reachability and dead-rule analysis of a GameDocument.
use serde_json::{json, Value};
use vesper3d::viewer::{
    game::{GameDocument, LoadedGame},
    game_example,
    game_explore::{explore, explore_with, Level, Report},
};

/// A game on the example map (targets button-a/b/c and exit), with the given rule pieces.
fn game(counters: Value, timers: Value, zones: Value, rules: Value) -> LoadedGame {
    let (document, map) = game_example::documents().unwrap();
    let mut doc = serde_json::to_value(&document).unwrap();
    doc["counters"] = counters;
    doc["timers"] = timers;
    doc["trigger_zones"] = zones;
    doc["rules"] = rules;
    // Declare exactly the targets the rules name, so an unrelated one is not itself a finding.
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
fn on(target: &str) -> Value {
    json!({ "on_interact": target })
}
fn win() -> Value {
    json!([{"action": "complete"}])
}
fn kinds(report: &Report, level: Level) -> Vec<&'static str> {
    report
        .findings
        .iter()
        .filter(|f| f.level == level)
        .map(|f| f.kind)
        .collect()
}
fn has(report: &Report, kind: &str) -> bool {
    report.findings.iter().any(|f| f.kind == kind)
}

#[test]
fn the_shortest_win_is_reported_and_a_healthy_game_has_no_warnings() {
    let g = game(
        json!({"n": 0}),
        json!([]),
        json!([]),
        json!([
            rule(
                "a",
                on("button-a"),
                Value::Null,
                json!([{"action":"increment","counter":"n","amount":1}])
            ),
            rule(
                "b",
                on("button-b"),
                json!({"counter":"n","at_least":1}),
                win()
            ),
        ]),
    );
    let r = explore(&g, 1000).unwrap();
    assert_eq!(r.winnable, Some(true));
    assert_eq!(
        r.shortest_win.as_deref(),
        Some(&["press button-a".to_string(), "press button-b".into()][..])
    );
    assert!(!r.truncated && !r.has_errors());
    assert!(kinds(&r, Level::Warning).is_empty(), "{:?}", r.findings);
}

#[test]
fn a_game_that_can_never_be_won_is_an_error_naming_the_rule() {
    let g = game(
        json!({"n": 0}),
        json!([]),
        json!([]),
        json!([
            rule(
                "a",
                on("button-a"),
                Value::Null,
                json!([{"action":"increment","counter":"n","amount":1}])
            ),
            rule(
                "finish",
                on("exit"),
                json!({"counter":"n","equals":5}),
                win()
            ),
        ]),
    );
    // n is only ever 0 or 1 (button-a is pressed repeatedly, but each press adds 1: 5 is reachable),
    // so make the goal genuinely unreachable instead.
    let mut never = g.clone();
    never.document.rules[1].condition =
        Some(serde_json::from_value(json!({"counter":"n","less_than":0})).unwrap());
    let r = explore(&never, 1000).unwrap();
    assert_eq!(r.winnable, Some(false));
    assert!(r.has_errors() && has(&r, "unwinnable"));
    assert!(r
        .findings
        .iter()
        .any(|f| f.kind == "unwinnable" && f.message.contains("finish")));
    assert!(has(&r, "rule-never-fires"));
    // The original is winnable: five presses of button-a, then the exit.
    let ok = explore(&g, 1000).unwrap();
    assert_eq!(
        ok.shortest_win.unwrap(),
        ["press button-a x5", "press exit"]
    );
}

#[test]
fn a_game_with_no_complete_action_is_an_error() {
    let g = game(
        json!({"n": 0}),
        json!([]),
        json!([]),
        json!([rule(
            "a",
            on("button-a"),
            Value::Null,
            json!([{"action":"increment","counter":"n","amount":1}])
        )]),
    );
    let r = explore(&g, 1000).unwrap();
    assert!(has(&r, "no-win") && r.has_errors());
}

#[test]
fn a_counter_nothing_reads_is_reported_and_does_not_blow_up_the_search() {
    let g = game(
        json!({"score": 0, "hits": 0}),
        json!([{"id":"t","duration_ticks":30,"auto_start":true,"repeats":true}]),
        json!([]),
        json!([
            rule(
                "tick",
                json!({"on_timer":"t"}),
                Value::Null,
                json!([{"action":"increment","counter":"score","amount":-1}])
            ),
            rule(
                "hit",
                on("button-a"),
                Value::Null,
                json!([{"action":"increment","counter":"hits","amount":1}])
            ),
            rule(
                "done",
                on("exit"),
                json!({"counter":"hits","equals":1}),
                win()
            ),
        ]),
    );
    let r = explore(&g, 1000).unwrap();
    assert!(
        !r.truncated,
        "an unread counter must not multiply the state space"
    );
    assert!(r
        .findings
        .iter()
        .any(|f| f.kind == "counter-unread" && f.message.contains("'score'")));
    assert_eq!(r.winnable, Some(true));
}

#[test]
fn a_timer_nobody_listens_to_and_one_never_started_are_reported() {
    let g = game(
        json!({"n": 0}),
        json!([{"id":"idle","duration_ticks":30,"auto_start":true,"repeats":true},
               {"id":"asleep","duration_ticks":30,"auto_start":false,"repeats":false}]),
        json!([]),
        json!([
            rule("a", on("button-a"), Value::Null, win()),
            rule(
                "z",
                json!({"on_timer":"asleep"}),
                Value::Null,
                json!([{"action":"increment","counter":"n","amount":1}])
            ),
        ]),
    );
    let r = explore(&g, 1000).unwrap();
    assert!(r
        .findings
        .iter()
        .any(|f| f.kind == "timer-unheard" && f.message.contains("'idle'")));
    assert!(r
        .findings
        .iter()
        .any(|f| f.kind == "timer-never-started" && f.message.contains("'asleep'")));
    assert!(r
        .findings
        .iter()
        .any(|f| f.kind == "rule-never-fires" && f.message.contains("'z'")));
}

#[test]
fn losing_is_found_with_a_run_length_path_and_an_unreachable_fail_is_flagged() {
    let timers = json!([{"id":"fuse","duration_ticks":30,"auto_start":true,"repeats":true}]);
    let counters = json!({"left": 3});
    let g = game(
        counters.clone(),
        timers.clone(),
        json!([]),
        json!([
            rule(
                "burn",
                json!({"on_timer":"fuse"}),
                Value::Null,
                json!([{"action":"increment","counter":"left","amount":-1}])
            ),
            rule(
                "boom",
                json!({"on_timer":"fuse"}),
                json!({"counter":"left","at_most":0}),
                json!([{"action":"fail"}])
            ),
            rule("defuse", on("button-a"), Value::Null, win()),
        ]),
    );
    let r = explore(&g, 1000).unwrap();
    assert!(r.can_lose);
    assert_eq!(r.shortest_loss.clone().unwrap(), ["timer fuse runs out x3"]);
    assert!(!has(&r, "cannot-lose") && !has(&r, "fail-unreachable"));

    let mut safe = g.clone();
    safe.document.rules[1].condition =
        Some(serde_json::from_value(json!({"counter":"left","less_than":-5})).unwrap());
    let r = explore(&safe, 1000).unwrap();
    // The fuse can still run out below -5 (the timer keeps repeating), so this is losable...
    assert!(r.can_lose);
    // ...but a fail behind an impossible condition is flagged.
    safe.document.rules[1].condition =
        Some(serde_json::from_value(json!({"counter":"left","greater_than":100})).unwrap());
    let r = explore(&safe, 1000).unwrap();
    assert!(!r.can_lose && has(&r, "fail-unreachable"));
}

#[test]
fn a_press_that_walls_off_the_win_is_a_stuck_state_with_the_way_in() {
    let g = game(
        json!({"x": 0}),
        json!([]),
        json!([]),
        json!([
            rule(
                "a",
                on("button-a"),
                Value::Null,
                json!([{"action":"set_enabled","entity":"button-b","enabled":false}])
            ),
            rule("b", on("button-b"), Value::Null, win()),
        ]),
    );
    let r = explore(&g, 1000).unwrap();
    assert_eq!(r.winnable, Some(true));
    let stuck = r
        .findings
        .iter()
        .find(|f| f.kind == "can-get-stuck")
        .expect("stuck finding");
    assert_eq!(stuck.path, ["press button-a"]);
}

#[test]
fn an_armed_target_whose_press_does_nothing_is_noted() {
    let g = game(
        json!({"n": 0}),
        json!([]),
        json!([]),
        json!([
            rule(
                "a",
                on("button-a"),
                Value::Null,
                json!([{"action":"increment","counter":"n","amount":1}])
            ),
            rule(
                "early",
                on("exit"),
                json!({"counter":"n","equals":2}),
                win()
            ),
        ]),
    );
    let r = explore(&g, 1000).unwrap();
    let f = r
        .findings
        .iter()
        .find(|f| f.kind == "press-does-nothing")
        .expect("finding");
    assert!(f.message.contains("'exit'"));
    assert_eq!(f.path.last().unwrap(), "press exit (does nothing)");
}

#[test]
fn trigger_zones_are_entered_and_left_and_an_unheard_zone_is_noted() {
    let bounds = json!({"min": [-6.0, 0.0, 5.0], "max": [-5.0, 2.0, 6.0]});
    let g = game(
        json!({"trips": 0}),
        json!([]),
        json!([{"id":"wire","bounds":bounds,"enabled":true},{"id":"mat","bounds":{"min":[5.0,0.0,5.0],"max":[6.0,2.0,6.0]},"enabled":true}]),
        json!([
            rule(
                "trip",
                json!({"on_enter":"wire"}),
                Value::Null,
                json!([{"action":"increment","counter":"trips","amount":1}])
            ),
            rule(
                "caught",
                json!({"on_enter":"wire"}),
                json!({"counter":"trips","at_least":2}),
                json!([{"action":"fail"}])
            ),
            rule("a", on("button-a"), Value::Null, win()),
        ]),
    );
    let r = explore(&g, 1000).unwrap();
    assert_eq!(
        r.shortest_loss.unwrap(),
        ["enter wire", "leave wire", "enter wire"]
    );
    assert!(r
        .findings
        .iter()
        .any(|f| f.kind == "zone-unheard" && f.message.contains("'mat'")));
}

#[test]
fn unbounded_counters_fold_into_a_finite_search_without_changing_the_answers() {
    // "phase" only grows and is only read by modulo; "level" only grows and is read by a threshold.
    let g = game(
        json!({"phase": 0, "level": 0}),
        json!([{"id":"clock","duration_ticks":30,"auto_start":true,"repeats":true}]),
        json!([]),
        json!([
            rule(
                "tick",
                json!({"on_timer":"clock"}),
                Value::Null,
                json!([
                {"action":"increment","counter":"phase","amount":1},
                {"action":"increment","counter":"level","amount":1}])
            ),
            rule(
                "rise",
                json!({"on_timer":"clock"}),
                json!({"counter":"level","at_least":6}),
                json!([{"action":"fail"}])
            ),
            rule(
                "safe",
                on("button-a"),
                json!({"counter":"phase","modulo":2,"equals":1}),
                win()
            ),
        ]),
    );
    let folded = explore_with(&g, 100_000, true).unwrap();
    let exact = explore_with(&g, 100_000, false).unwrap();
    assert!(
        !folded.truncated && !exact.truncated,
        "fail bounds the level, so both finish"
    );
    assert!(folded.states <= exact.states);
    assert_eq!(folded.winnable, exact.winnable);
    assert_eq!(folded.shortest_win, exact.shortest_win);
    assert_eq!(folded.shortest_loss, exact.shortest_loss);
    assert_eq!(folded.can_lose, exact.can_lose);
    let names = |r: &Report| {
        r.findings
            .iter()
            .map(|f| (f.kind, f.message.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&folded), names(&exact));
    assert_eq!(
        folded.shortest_win.unwrap(),
        ["timer clock runs out", "press button-a"]
    );
}

#[test]
fn a_modulo_only_counter_is_finite_even_when_nothing_else_bounds_it() {
    let g = game(
        json!({"phase": 0}),
        json!([{"id":"clock","duration_ticks":30,"auto_start":true,"repeats":true}]),
        json!([]),
        json!([
            rule(
                "tick",
                json!({"on_timer":"clock"}),
                Value::Null,
                json!([{"action":"increment","counter":"phase","amount":1}])
            ),
            rule(
                "gate",
                on("button-a"),
                json!({"counter":"phase","modulo":3,"equals":2}),
                win()
            ),
        ]),
    );
    let r = explore(&g, 1000).unwrap();
    assert!(!r.truncated && r.states <= 6, "states: {}", r.states);
    assert_eq!(
        r.shortest_win.unwrap(),
        ["timer clock runs out x2", "press button-a"]
    );
    // Without folding the search never ends by itself.
    assert!(explore_with(&g, 500, false).unwrap().truncated);
}

#[test]
fn hitting_the_state_limit_says_so_and_downgrades_never_findings() {
    let g = game(
        json!({"n": 0}),
        json!([{"id":"clock","duration_ticks":30,"auto_start":true,"repeats":true}]),
        json!([]),
        json!([
            rule(
                "tick",
                json!({"on_timer":"clock"}),
                Value::Null,
                json!([{"action":"increment","counter":"n","amount":1},{"action":"increment","counter":"n","amount":-2}])
            ),
            rule(
                "gate",
                on("button-a"),
                json!({"counter":"n","equals":-1000000}),
                win()
            ),
        ]),
    );
    let r = explore(&g, 200).unwrap();
    assert!(r.truncated && has(&r, "truncated"));
    assert_eq!(r.winnable, None);
    assert!(
        !r.has_errors(),
        "an unfinished search must not claim the game is unwinnable"
    );
}

#[test]
fn a_target_no_rule_reacts_to_is_a_warning_but_timer_rules_do_not_count() {
    let (document, map) = game_example::documents().unwrap();
    let mut doc = serde_json::to_value(&document).unwrap();
    doc["counters"] = json!({"n": 0});
    doc["timers"] = json!([{"id":"t","duration_ticks":30,"auto_start":true,"repeats":true}]);
    doc["rules"] = json!([
        rule(
            "tick",
            json!({"on_timer":"t"}),
            Value::Null,
            json!([{"action":"increment","counter":"n","amount":1}])
        ),
        rule(
            "a",
            on("button-a"),
            json!({"counter":"n","at_least":1}),
            win()
        ),
    ]);
    doc["interactables"] =
        json!(["button-a", "button-b"]
            .map(|e| json!({"entity": e, "enabled": true, "visible": true})));
    let document: GameDocument = serde_json::from_value(doc).unwrap();
    document.validate(&map).unwrap();
    let r = explore(&LoadedGame { document, map }, 1000).unwrap();
    let missing: Vec<_> = r
        .findings
        .iter()
        .filter(|f| f.kind == "target-no-rule")
        .collect();
    assert_eq!(missing.len(), 1, "{:?}", r.findings);
    assert!(missing[0].message.contains("'button-b'"));
}

/// A tiny deterministic generator, so a failing case can be reproduced from its seed.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> i64 {
        (self.next() % n) as i64
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len() as u64) as usize]
    }
}

fn random_condition(rng: &mut Lcg) -> Value {
    let counter = rng.pick(&["p", "q"]);
    let mut leaf = json!({ "counter": counter });
    if rng.below(3) == 0 {
        leaf["modulo"] = json!(2 + rng.below(2));
    }
    let op = rng.pick(&[
        "equals",
        "not_equals",
        "less_than",
        "greater_than",
        "at_most",
        "at_least",
    ]);
    leaf[op] = json!(rng.below(6) - 2);
    match rng.below(5) {
        0 => json!({"not": leaf}),
        1 => json!({"all": [leaf, {"counter": "q", "at_least": rng.below(3)}]}),
        _ => leaf,
    }
}

fn random_actions(rng: &mut Lcg) -> Value {
    let mut actions = Vec::new();
    for _ in 0..1 + rng.below(2) {
        actions.push(match rng.below(8) {
            0..=2 => json!({"action": "increment", "counter": rng.pick(&["p", "q"]), "amount": rng.below(5) - 2}),
            3 => json!({"action": "set_counter", "counter": rng.pick(&["p", "q"]), "value": rng.below(6) - 2}),
            4 => json!({"action": "set_enabled", "entity": rng.pick(&["button-a", "button-b"]), "enabled": rng.below(2) == 0}),
            5 => json!({"action": "fail"}),
            _ => json!({"action": "complete"}),
        });
    }
    Value::Array(actions)
}

/// Folding counters is exact: with it off (and the search allowed to finish) every answer is the same.
#[test]
fn folding_never_changes_an_answer_on_random_games() {
    let (mut compared, mut folded_smaller) = (0, 0);
    for seed in 0..400u64 {
        let mut rng = Lcg(seed * 7919 + 1);
        let mut rules = vec![rule(
            "win",
            on("button-b"),
            random_condition(&mut rng),
            json!([{"action":"complete"}]),
        )];
        for i in 0..2 + rng.below(3) {
            let trigger = match rng.below(3) {
                0 => json!({"on_timer": "clock"}),
                1 => on("button-a"),
                _ => on("button-b"),
            };
            let condition = if rng.below(3) == 0 {
                Value::Null
            } else {
                random_condition(&mut rng)
            };
            rules.push(rule(
                &format!("r{i}"),
                trigger,
                condition,
                random_actions(&mut rng),
            ));
        }
        let (document, map) = game_example::documents().unwrap();
        let mut doc = serde_json::to_value(&document).unwrap();
        doc["counters"] = json!({"p": rng.below(4) - 1, "q": rng.below(3)});
        doc["timers"] =
            json!([{"id":"clock","duration_ticks":30,"auto_start":true,"repeats":true}]);
        doc["trigger_zones"] = json!([]);
        doc["rules"] = Value::Array(rules);
        doc["interactables"] = json!([
            {"entity": "button-a", "enabled": true, "visible": true},
            {"entity": "button-b", "enabled": true, "visible": true}]);
        let Ok(document) = serde_json::from_value::<GameDocument>(doc) else {
            continue;
        };
        if document.validate(&map).is_err() {
            continue;
        }
        let g = LoadedGame { document, map };
        let exact = explore_with(&g, 8_000, false).unwrap();
        if exact.truncated {
            continue; // unbounded without folding: nothing to compare against
        }
        let folded = explore_with(&g, 8_000, true).unwrap();
        compared += 1;
        folded_smaller += usize::from(folded.states < exact.states);
        let describe = |r: &Report| {
            (
                r.winnable,
                r.can_lose,
                r.shortest_win.as_ref().map(|p| p.len()),
                r.shortest_loss.as_ref().map(|p| p.len()),
                r.findings
                    .iter()
                    .map(|f| (f.kind, f.message.clone()))
                    .collect::<Vec<_>>(),
            )
        };
        assert!(!folded.truncated, "seed {seed}: folding must not be worse");
        assert_eq!(describe(&folded), describe(&exact), "seed {seed}");
    }
    assert!(
        compared >= 100,
        "only {compared} of 400 random games finished exactly"
    );
    assert!(
        folded_smaller > 0,
        "folding never reduced anything: the test is not exercising it"
    );
}

#[test]
fn a_reset_that_re_enables_a_used_up_once_rule_is_flagged_and_a_repeatable_rule_is_not() {
    // button-a counts a strike and switches itself off (once); button-b "resets" by switching it back on.
    let build = |once: bool| {
        let mut strike = rule(
            "strike",
            on("button-a"),
            Value::Null,
            json!([
                {"action": "increment", "counter": "n", "amount": 1},
                {"action": "set_enabled", "entity": "button-a", "enabled": false}
            ]),
        );
        strike["once"] = json!(once);
        game(
            json!({"n": 0}),
            json!([]),
            json!([]),
            json!([
                strike,
                rule(
                    "reset",
                    on("button-b"),
                    Value::Null,
                    json!([{"action": "set_enabled", "entity": "button-a", "enabled": true}])
                ),
                rule(
                    "win",
                    on("exit"),
                    json!({"counter": "n", "at_least": 2}),
                    win()
                ),
            ]),
        )
    };
    let flagged = explore(&build(true), 1000).unwrap();
    let f = flagged
        .findings
        .iter()
        .find(|f| f.kind == "once-exhausted")
        .expect("the once trap is found");
    assert_eq!(f.level, Level::Warning);
    assert!(
        f.message.contains("'button-a'")
            && f.message.contains("strike")
            && f.message.contains("once: false"),
        "{}",
        f.message
    );
    assert_eq!(
        f.path,
        [
            "press button-a",
            "press button-b (switches 'button-a' back on)"
        ]
    );
    // With the trap the game really cannot be won: two strikes are needed but the second never registers.
    assert_eq!(flagged.winnable, Some(false));

    let repeatable = explore(&build(false), 1000).unwrap();
    assert!(
        !has(&repeatable, "once-exhausted"),
        "{:?}",
        repeatable.findings
    );
    assert_eq!(repeatable.winnable, Some(true));
}

#[test]
fn enabling_a_target_for_the_first_time_is_not_a_once_trap() {
    // a's once rule enables b; b's once rule has not fired yet, so nothing is exhausted.
    let mut enable_b = rule(
        "a",
        on("button-a"),
        Value::Null,
        json!([{"action": "set_enabled", "entity": "button-b", "enabled": true}]),
    );
    enable_b["once"] = json!(true);
    let mut finish = rule("b", on("button-b"), Value::Null, win());
    finish["once"] = json!(true);
    let mut g = game(
        json!({"n": 0}),
        json!([]),
        json!([]),
        json!([enable_b, finish]),
    );
    g.document
        .interactables
        .iter_mut()
        .find(|i| i.entity == "button-b")
        .unwrap()
        .enabled = false;
    let r = explore(&g, 1000).unwrap();
    assert!(!has(&r, "once-exhausted"), "{:?}", r.findings);
    assert_eq!(r.winnable, Some(true));
}
