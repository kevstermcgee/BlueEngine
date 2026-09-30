//! Rule conditions beyond `counter == n`, and the authoritative `fail` outcome.
use vesper3d::{
    math::V,
    viewer::{
        controller::Controller,
        game::{Condition, GameAction, GameRuntime, GameState, LoadedGame},
        game_example,
        profile::ControllerProfile,
    },
};

fn fixture() -> LoadedGame {
    let (document, map) = game_example::documents().unwrap();
    LoadedGame { document, map }
}

fn condition(json: &str) -> Condition {
    serde_json::from_str(json).unwrap_or_else(|e| panic!("{json}: {e}"))
}

/// Press switch A with `switches` starting at `value`; the guarded rule ends the match as a loss,
/// so `failed` tells whether the condition held.
fn holds(json: &str, value: i32) -> bool {
    let mut loaded = fixture();
    loaded.document.counters.insert("switches".into(), value);
    loaded.document.rules[0].condition = Some(condition(json));
    loaded.document.rules[0].actions = vec![GameAction::Fail];
    let room = loaded.map.build().unwrap();
    let mut game = GameRuntime::compile(loaded.document, &loaded.map).unwrap();
    let at_a = Controller::for_profile(ControllerProfile::default(), V(-3., 0., 3.), 0.).unwrap();
    game.interact(&room, &at_a, 1)
        .expect("switch A is aimed at");
    game.state().failed
}

fn rejected(json: &str) -> String {
    let mut loaded = fixture();
    let Ok(parsed) = serde_json::from_str::<Condition>(json) else {
        return "does not parse".into();
    };
    loaded.document.rules[0].condition = Some(parsed);
    loaded
        .document
        .validate(&loaded.map)
        .expect_err(&format!("{json} should be rejected"))
        .to_string()
}

#[test]
fn the_original_counter_equals_form_still_parses_and_round_trips() {
    let c = condition(r#"{"counter":"switches","equals":3}"#);
    assert_eq!(c, Condition::counter_equals("switches", 3));
    assert_eq!(
        serde_json::to_string(&c).unwrap(),
        r#"{"counter":"switches","equals":3}"#
    );
    assert!(holds(r#"{"counter":"switches","equals":3}"#, 3));
    assert!(!holds(r#"{"counter":"switches","equals":3}"#, 2));
}

#[test]
fn comparisons_cover_every_operator_at_the_boundary() {
    for (op, held, not_held) in [
        ("equals", [5].as_slice(), [4, 6].as_slice()),
        ("not_equals", &[4, 6], &[5]),
        ("less_than", &[4], &[5, 6]),
        ("greater_than", &[6], &[4, 5]),
        ("at_most", &[4, 5], &[6]),
        ("at_least", &[5, 6], &[4]),
    ] {
        let json = format!(r#"{{"counter":"switches","{op}":5}}"#);
        for v in held {
            assert!(holds(&json, *v), "{op} 5 should hold at {v}");
        }
        for v in not_held {
            assert!(!holds(&json, *v), "{op} 5 should not hold at {v}");
        }
    }
}

#[test]
fn several_comparisons_on_one_counter_must_all_hold() {
    let range = r#"{"counter":"switches","at_least":2,"at_most":4}"#;
    let held: Vec<_> = (0..7).filter(|v| holds(range, *v)).collect();
    assert_eq!(held, [2, 3, 4]);
}

#[test]
fn modulo_compares_the_remainder_and_stays_positive_for_negative_counters() {
    let even = r#"{"counter":"switches","modulo":2,"equals":0}"#;
    assert!(holds(even, 4));
    assert!(!holds(even, 5));
    assert!(!holds(even, -3), "-3 mod 2 is 1, not -1");
    assert!(holds(even, -4));
    assert!(holds(
        r#"{"counter":"switches","modulo":3,"at_least":2}"#,
        -1
    ));
}

#[test]
fn all_any_and_not_compose_and_nest() {
    let both =
        r#"{"all":[{"counter":"switches","at_least":2},{"counter":"switches","at_most":3}]}"#;
    assert_eq!(
        (0..6).filter(|v| holds(both, *v)).collect::<Vec<_>>(),
        [2, 3]
    );
    let either = r#"{"any":[{"counter":"switches","equals":1},{"counter":"switches","equals":4}]}"#;
    assert_eq!(
        (0..6).filter(|v| holds(either, *v)).collect::<Vec<_>>(),
        [1, 4]
    );
    let not = r#"{"not":{"counter":"switches","equals":0}}"#;
    assert!(!holds(not, 0) && holds(not, 1));
    let nested = r#"{"all":[{"not":{"counter":"switches","equals":2}},{"any":[{"counter":"switches","less_than":1},{"counter":"switches","greater_than":3}]}]}"#;
    assert_eq!(
        (0..6).filter(|v| holds(nested, *v)).collect::<Vec<_>>(),
        [0, 4, 5]
    );
}

#[test]
fn malformed_conditions_are_rejected_with_a_reason() {
    for (json, reason) in [
        ("{}", "exactly one form"),
        (r#"{"counter":"switches"}"#, "no comparison"),
        (r#"{"equals":1}"#, "need a counter"),
        (r#"{"counter":"nope","equals":1}"#, "unknown counter"),
        (r#"{"counter":"switches","equals":2000000}"#, "out of range"),
        (
            r#"{"counter":"switches","modulo":0,"equals":0}"#,
            "out of range",
        ),
        (
            r#"{"counter":"switches","equals":1,"all":[{"counter":"switches","equals":1}]}"#,
            "exactly one form",
        ),
        (r#"{"all":[]}"#, "at least one"),
        (r#"{"any":[]}"#, "at least one"),
        (r#"{"counter":"switches","eq":1}"#, "does not parse"),
    ] {
        let message = rejected(json);
        assert!(message.contains(reason), "{json}: {message}");
    }
}

#[test]
fn nesting_and_size_are_bounded() {
    let leaf = r#"{"counter":"switches","equals":0}"#;
    let mut deep = leaf.to_string();
    for _ in 0..4 {
        deep = format!(r#"{{"not":{deep}}}"#);
    }
    // The leaf sits 4 levels down: the deepest allowed.
    let mut loaded = fixture();
    loaded.document.rules[0].condition = Some(condition(&deep));
    loaded.document.validate(&loaded.map).unwrap();
    assert!(rejected(&format!(r#"{{"not":{deep}}}"#)).contains("deeper"));
    let wide = format!(r#"{{"all":[{}]}}"#, vec![leaf; 16].join(","));
    assert!(rejected(&wide).contains("more than"));
    let fits = format!(r#"{{"all":[{}]}}"#, vec![leaf; 15].join(","));
    let mut loaded = fixture();
    loaded.document.rules[0].condition = Some(condition(&fits));
    loaded.document.validate(&loaded.map).unwrap();
}

#[test]
fn fail_ends_the_match_ignores_further_events_and_restart_clears_it() {
    let mut loaded = fixture();
    loaded.document.rules[0].actions = vec![GameAction::Fail];
    let mut world = loaded.world().unwrap();
    world.join(1);
    let profile = world.game.as_ref().unwrap().document().player_profile;
    let stand = |world: &mut vesper3d::viewer::simulation::HeadlessWorld, x: f32| {
        *world.player_mut(1).unwrap() = Controller::for_profile(profile, V(x, 0., 3.), 0.).unwrap();
    };
    let before = world.checksum();
    stand(&mut world, -3.);
    assert!(world.request_interaction(1));
    world.step();
    let state = world.game.as_ref().unwrap().state().clone();
    assert!(state.failed && !state.completed && state.finished());
    assert_ne!(world.checksum(), before);

    // Nothing else counts once the match is lost.
    stand(&mut world, -1.);
    world.request_interaction(1);
    world.step();
    assert_eq!(
        world.game.as_ref().unwrap().state().counters,
        state.counters
    );

    // The shared action policy restarts a lost match just like a won one.
    assert!(world.game_action(1).unwrap());
    let restarted = world.game.as_ref().unwrap().state();
    assert!(!restarted.failed && !restarted.finished());
    assert_eq!(restarted.round, state.round + 1);
}

#[test]
fn timers_stop_firing_after_a_loss() {
    let mut loaded = fixture();
    loaded
        .document
        .timers
        .push(vesper3d::viewer::game::TimerDefinition {
            id: "fuse".into(),
            duration_ticks: 5,
            auto_start: true,
            repeats: true,
        });
    loaded.document.rules.push(vesper3d::viewer::game::Rule {
        id: "boom".into(),
        on_interact: None,
        on_enter: None,
        on_exit: None,
        on_timer: Some("fuse".into()),
        condition: None,
        once: false,
        actions: vec![
            GameAction::Increment {
                counter: "switches".into(),
                amount: 1,
            },
            GameAction::Fail,
        ],
    });
    let mut world = loaded.world().unwrap();
    world.join(1);
    for _ in 0..30 {
        world.step();
    }
    let state = world.game.as_ref().unwrap().state();
    assert!(state.failed);
    assert_eq!(
        state.counters,
        [1],
        "the fuse fired once, then the match was over"
    );
}

#[test]
fn failed_is_absent_from_json_until_set_and_old_states_still_load() {
    let idle = serde_json::to_value(GameState::default()).unwrap();
    assert!(
        idle.get("failed").is_none(),
        "unchanged wire format for games that never fail"
    );
    let old = r#"{"counters":[1],"enabled":0,"fired":0,"completed":false}"#;
    let parsed: GameState = serde_json::from_str(old).unwrap();
    assert!(!parsed.failed);
    let lost = GameState {
        failed: true,
        ..GameState::default()
    };
    let round_trip: GameState =
        serde_json::from_str(&serde_json::to_string(&lost).unwrap()).unwrap();
    assert_eq!(round_trip, lost);
}
