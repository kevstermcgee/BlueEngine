use super::*;
use std::collections::{HashSet, VecDeque};
use vesper3d::runtime::snapshot;
fn fixture(player: Cell, crates: &[Cell]) -> Afterpull {
    let mut g = Afterpull::new(7);
    g.state.board.player = player;
    g.state.board.crates = crates.to_vec();
    g
}
#[test]
fn simultaneous_jam_does_not_cancel_a_remote_pull_or_chain() {
    let mut g = fixture(
        Cell::new(3, 3),
        &[Cell::new(2, 3), Cell::new(4, 3), Cell::new(3, 1)],
    );
    let p = g.plan(2).unwrap();
    assert_eq!(p.pulls.len(), 3);
    assert_eq!(p.pulls.iter().filter(|p| p.jam).count(), 2);
    g.step(&Afterpull::direction(2));
    assert_eq!(
        g.state.board.crates,
        vec![Cell::new(2, 3), Cell::new(4, 3), Cell::new(3, 2)]
    );
    assert_eq!(g.state.board.jams, 1);
    assert_eq!(g.state.board.turns, 1);
    g.step(&Afterpull::command(-11, 0));
    assert_eq!(g.state.board.player, Cell::new(3, 3));
    assert_eq!(g.state.board.crates[2], Cell::new(3, 1));
}
#[test]
fn player_walls_and_first_crate_shield_and_docks_do_not_anchor() {
    let g = fixture(
        Cell::new(3, 3),
        &[Cell::new(3, 1), Cell::new(1, 3), Cell::new(2, 3)],
    );
    let p = g.plan(0).unwrap();
    assert!(!p.pulls.iter().any(|p| p.from == Cell::new(3, 1)));
    assert!(p.pulls.iter().any(|p| p.from == Cell::new(2, 3)));
    assert!(!p.pulls.iter().any(|p| p.from == Cell::new(1, 3)));
    let mut g = Afterpull::new(7);
    g.state.room = 1;
    g.state.board = Afterpull::initial(1);
    g.step(&Afterpull::direction(0));
    assert_eq!(g.state.board.crates[0], Cell::new(3, 2));
    g.step(&Afterpull::direction(1));
    assert_eq!(g.state.board.crates[0], Cell::new(3, 3));
    let g = fixture(Cell::new(1, 1), &[Cell::new(3, 1), Cell::new(4, 1)]);
    assert!(g.plan(3).is_none());
    assert_eq!(g.plan(2).unwrap().pulls.len(), 1);
}
#[test]
fn no_pushing_blocked_steps_and_preview_are_free_and_held_keys_do_not_repeat() {
    let mut g = Afterpull::new(7);
    let before = g.state.board.clone();
    g.step(&Afterpull::preview(0));
    assert_eq!(g.state.board, before);
    assert!(g.state.history.is_empty());
    assert!(g.state.preview.is_some());
    g.step(&Afterpull::direction(0));
    let after = g.state.board.clone();
    assert_eq!(after.turns, 1);
    let mut held = Intent {
        x: 1,
        action: true,
        ..Default::default()
    };
    g.step(&held);
    let turns = g.state.board.turns;
    held.action = false;
    for _ in 0..30 {
        g.step(&held);
    }
    assert_eq!(g.state.board.turns, turns);
    // Keyboard axes can change on a release or remain held after undo.
    g.step(&Intent {
        y: -1,
        ..Default::default()
    });
    assert_eq!(g.state.board.turns, turns);
    g.step(&Afterpull::command(-11, 0));
    let undone = g.state.board.clone();
    for _ in 0..10 {
        g.step(&held);
    }
    assert_eq!(g.state.board, undone);
    g.state.board.player = Cell::new(3, 2);
    g.state.board.crates = vec![Cell::new(4, 2), Cell::new(4, 4)];
    let before = g.state.board.clone();
    g.step(&Afterpull::direction(1));
    assert_eq!(g.state.board, before);
    g.state.board.player = Cell::new(1, 1);
    let before = g.state.board.clone();
    g.step(&Afterpull::direction(0));
    assert_eq!(g.state.board, before);
}
#[test]
fn buffered_preview_stays_free_when_space_releases_before_the_fixed_tick() {
    use vesper3d::runtime::InputAccumulator;
    let mut g = Afterpull::new(7);
    let before = g.state.board.clone();
    let mut inputs = InputAccumulator::<Intent>::new();
    let mut frame = Afterpull::preview(0);
    frame.sprint = true;
    // The shared client retains the explicit pointer target with the press
    // edge, while held fields (including Space) are replaced every frame.
    let retained_target = frame.pointer;
    frame.action = false;
    inputs.feed(frame, 1, [0.; 2]);
    // Multiple native frames with no 60 Hz tick: release Space and direction.
    for _ in 0..3 {
        inputs.feed(Intent::default(), 0, [0.; 2]);
    }
    let tick = inputs.take_tick();
    let mut delivered = tick.held;
    delivered.action = tick.pressed(1);
    delivered.pointer = retained_target;
    assert!(!delivered.sprint);
    g.step(&delivered);
    assert_eq!(g.state.board, before);
    assert!(g.state.history.is_empty());
    assert_eq!(g.state.preview, Some(0));
    for _ in 0..4 {
        let tick = inputs.take_tick();
        let mut delivered = tick.held;
        delivered.action = tick.pressed(1);
        g.step(&delivered);
    }
    assert_eq!(g.state.board, before);
    assert_eq!(g.state.preview, None);
    // A fresh press commits exactly one turn, even if Space was subsequently
    // pressed before delivery; command semantics belong to the original edge.
    let mut step = Afterpull::direction(0);
    step.sprint = true;
    g.step(&step);
    assert_eq!(g.state.board.turns, 1);
    g.step(&Intent {
        sprint: true,
        y: -1,
        ..Intent::default()
    });
    assert_eq!(g.state.board.turns, 1);
}

#[test]
fn delayed_preview_of_a_pit_pull_never_fails_or_changes_undo_history() {
    let mut g = Afterpull::new(7);
    g.state.room = 5;
    g.state.board = Afterpull::initial(5);
    g.state.board.player = Cell::new(1, 4);
    g.state.board.crates[0] = Cell::new(1, 1);
    let before = g.state.board.clone();
    assert!(g
        .plan(1)
        .unwrap()
        .pulls
        .iter()
        .any(|p| p.to == Cell::new(1, 2)));
    // No held Space modifier remains on this delivered preview command.
    g.step(&Afterpull::preview(1));
    assert!(!g.failed());
    assert_eq!(g.state.board, before);
    assert!(g.state.history.is_empty());
    let mut loaded = Afterpull::new(0);
    loaded.restore(g.capture()).unwrap();
    assert_eq!(loaded.state_hash(), g.state_hash());
    loaded.step(&Intent::default());
    assert_eq!(loaded.state.board, before);
    loaded.step(&Afterpull::direction(1));
    assert!(loaded.failed());
    loaded.step(&Afterpull::command(-11, 0));
    assert_eq!(loaded.state.board, before);
}
#[test]
fn pit_loss_is_immediate_and_undo_retry_and_saves_work_after_failure() {
    let mut g = Afterpull::new(7);
    g.state.room = 5;
    g.state.board = Afterpull::initial(5);
    // The crate at (1,1) falls into (1,2) when leaving (1,4) sideways.
    g.state.board.player = Cell::new(1, 4);
    g.state.board.crates = vec![
        Cell::new(1, 1),
        Cell::new(4, 2),
        Cell::new(4, 4),
        Cell::new(3, 3),
    ];
    let before = g.state.board.clone();
    g.step(&Afterpull::direction(1));
    assert_eq!(g.outcome(), "lost");
    assert!(g.failed());
    let lost = g.state.board.clone();
    g.step(&Afterpull::direction(0));
    assert_eq!(g.state.board, lost);
    let save = g.capture();
    let mut restored = Afterpull::new(0);
    restored.restore(save).unwrap();
    assert_eq!(restored.outcome(), "lost");
    restored.step(&Afterpull::command(-11, 0));
    assert_eq!(restored.state.board, before);
    assert_eq!(restored.outcome(), "playing");
    g.restart();
    assert_eq!(g.state.room, 5);
    assert_eq!(g.state.board, Afterpull::initial(5));
    assert!(!g.failed());
    let g = Afterpull::new(7);
    assert!(g.plan(0).is_some());
}
#[test]
fn all_six_rooms_clear_with_real_intents_and_completion_can_be_undone() {
    let mut g = Afterpull::new(7);
    for (index, room) in ROOMS.iter().enumerate() {
        assert_eq!(g.state.room, index);
        for ch in room.solution.chars() {
            assert!(
                !g.cleared(),
                "solution has a redundant tail in room {}",
                index + 1
            );
            g.step(&Afterpull::direction("URDL".find(ch).unwrap()));
            assert!(!g.failed());
        }
        assert!(g.cleared(), "room {} unsolved", index + 1);
        let solved = g.state.board.clone();
        g.step(&Afterpull::command(-11, 0));
        assert!(!g.cleared());
        g.step(&Afterpull::direction(
            "URDL".find(room.solution.chars().last().unwrap()).unwrap(),
        ));
        assert_eq!(g.state.board, solved);
        g.step(&Afterpull::command(-12, 0));
    }
    assert_eq!(g.outcome(), "won");
    g.step(&Afterpull::command(-11, 0));
    assert_eq!(g.outcome(), "playing");
    assert!(!g.cleared());
}
#[test]
fn the_jam_room_cannot_be_solved_without_jamming() {
    let mut g = Afterpull::new(7);
    g.state.room = 4;
    g.state.board = Afterpull::initial(4);
    let mut queue = VecDeque::from([g.state.board.clone()]);
    let mut seen = HashSet::new();
    while let Some(b) = queue.pop_front() {
        g.state.board = b;
        assert!(!g.cleared(), "jam room admits a route with no jam");
        for dir in 0..4 {
            let Some(p) = g.plan(dir) else {
                continue;
            };
            if p.pulls.iter().any(|p| p.jam) {
                continue;
            }
            let mut cs = g.state.board.crates.clone();
            for c in &mut cs {
                if let Some(p) = p.pulls.iter().find(|p| p.from == *c) {
                    *c = p.to;
                }
            }
            cs.sort();
            let key = (p.player, cs.clone());
            if !seen.insert(key) {
                continue;
            }
            queue.push_back(Board {
                player: p.player,
                crates: cs,
                turns: 0,
                jams: 0,
                multi: 0,
            });
        }
    }
    assert!(seen.len() > 100);
}
#[test]
fn public_route_is_deterministic_and_snapshot_resumes_with_undo_history() {
    let inputs: Vec<_> = (0..Afterpull::VERIFY_TICKS)
        .map(Afterpull::verification_input)
        .collect();
    vesper3d::runtime::assert_deterministic(|| Afterpull::new(7), &inputs);
    snapshot::assert_resumes_exactly(|| Afterpull::new(7), &inputs, 137);
    let (hash, outcome) = verify::<Afterpull>();
    assert_eq!(outcome, "won");
    let mut g = Afterpull::new(7);
    g.step(&Afterpull::direction(0));
    let save = g.capture();
    let mut h = Afterpull::new(9);
    h.restore(save).unwrap();
    h.step(&Afterpull::command(-11, 0));
    assert_eq!(h.state.board, Afterpull::initial(0));
    let before = h.state_hash();
    let mut bad = h.capture();
    bad.room = 99;
    assert!(h.restore(bad).is_err());
    assert_eq!(h.state_hash(), before);
    if let Ok(path) = std::env::var("BE2_VERIFY_REPORT") {
        std::fs::write(
            path,
            serde_json::json!({"hash":format!("{hash:016x}"),"outcome":outcome}).to_string(),
        )
        .unwrap();
    }
}

#[test]
fn interior_wall_stops_sight_and_player_cannot_enter_a_pit() {
    let mut g = Afterpull::new(7);
    g.state.room = 1;
    g.state.board = Board {
        player: Cell::new(1, 2),
        crates: vec![Cell::new(3, 2), Cell::new(5, 4)],
        turns: 0,
        jams: 0,
        multi: 0,
    };
    assert!(g.plan(2).unwrap().pulls.is_empty());
    g.state.room = 5;
    g.state.board = Afterpull::initial(5);
    g.state.board.player = Cell::new(2, 1);
    assert!(g.plan(1).is_none());
    let before = g.state.board.clone();
    g.step(&Afterpull::direction(1));
    assert_eq!(g.state.board, before);
}

#[test]
fn finale_failure_preview_save_undo_and_retry_use_public_play_from_the_start() {
    let mut g = Afterpull::new(7);
    for room in &ROOMS[..5] {
        for ch in room.solution.chars() {
            g.step(&Afterpull::direction("URDL".find(ch).unwrap()));
        }
        assert!(g.cleared());
        g.step(&Afterpull::command(-12, 0));
    }
    assert_eq!(g.state.room, 5);
    for ch in "RLLURUUL".chars() {
        g.step(&Afterpull::direction("URDL".find(ch).unwrap()));
        assert!(!g.failed());
    }
    let safe = g.state.board.clone();
    let history = g.state.history.clone();
    let forecast = g.plan(1).unwrap();
    assert!(forecast
        .pulls
        .iter()
        .any(|p| !p.jam && ROOMS[5].pits.contains(&p.to)));
    g.step(&Afterpull::preview(1));
    assert_eq!(g.state.board, safe);
    assert_eq!(g.state.history, history);
    let mut resumed = Afterpull::new(0);
    resumed.restore(g.capture()).unwrap();
    assert_eq!(resumed.state_hash(), g.state_hash());
    resumed.step(&Afterpull::direction(1));
    assert_eq!(resumed.outcome(), "lost");
    assert_eq!(resumed.take_cues(), vec![1]);
    resumed.step(&Afterpull::command(-11, 0));
    assert_eq!(resumed.state.board, safe);
    assert_eq!(resumed.state.history, history);
    resumed.step(&Afterpull::command(-13, 0));
    assert_eq!(resumed.state.room, 5);
    assert_eq!(resumed.state.board, Afterpull::initial(5));
    assert!(resumed.state.history.is_empty());
    for ch in ROOMS[5].solution.chars() {
        resumed.step(&Afterpull::direction("URDL".find(ch).unwrap()));
    }
    assert!(resumed.cleared());
}

#[test]
fn movement_jams_and_dock_arrivals_emit_feedback_without_changing_rules() {
    let mut g = Afterpull::new(7);
    g.step(&Afterpull::direction(0));
    assert_eq!(g.filled(), 1);
    assert!(!g.cleared());
    assert_eq!(
        g.take_cues(),
        vec![2],
        "a dock arrival gets its own success cue"
    );
    let mut g = fixture(Cell::new(3, 3), &[Cell::new(3, 1), Cell::new(5, 5)]);
    g.step(&Afterpull::direction(1));
    assert_eq!(g.take_cues(), vec![0]);
    let mut g = fixture(Cell::new(3, 3), &[Cell::new(2, 3), Cell::new(4, 3)]);
    let crates = g.state.board.crates.clone();
    g.step(&Afterpull::direction(2));
    assert_eq!(g.state.board.crates, crates);
    assert_eq!(g.state.board.jams, 1);
    assert_eq!(
        g.take_cues(),
        vec![0],
        "a jam-only turn still gives feedback"
    );
}

#[test]
fn focus_loss_discards_buffered_commands_and_requires_a_neutral_return() {
    use crate::focus_gate::FocusGate;
    use vesper3d::runtime::InputAccumulator;
    for command in [
        Afterpull::direction(0),
        Afterpull::preview(0),
        Afterpull::command(-11, 0),
        Afterpull::command(-12, 0),
    ] {
        let mut g = Afterpull::new(7);
        // A real solved room gives undo and Enter observable effects too.
        for ch in ROOMS[0].solution.chars() {
            g.step(&Afterpull::direction("URDL".find(ch).unwrap()));
        }
        let before = g.capture();
        let hash = g.state_hash();
        let mut gate = FocusGate::default();
        let mut inputs = InputAccumulator::<Intent>::new();
        let target = command.pointer;
        let mut held = command;
        held.action = false;
        inputs.feed(held, 1, [0.; 2]); // press before the next 60 Hz tick
        gate.observe(false, true, false);
        // Focus returns before ANY tick. The old command still must be discarded.
        gate.observe(true, false, false);
        assert!(!gate.active());
        inputs.feed(Intent::default(), 0, [0.; 2]);
        let tick = inputs.take_tick();
        let mut delivered = tick.held;
        delivered.action = tick.pressed(1);
        delivered.pointer = target;
        gate.step(&mut g, &delivered);
        assert_eq!(g.state_hash(), hash);
        // Held controls cannot arm the gate, even after many consumed ticks.
        for _ in 0..20 {
            gate.observe(true, false, false);
            gate.step(&mut g, &command);
        }
        assert_eq!(g.state_hash(), hash);
        assert_eq!(g.state.history, before.history);
        gate.observe(true, false, true);
        assert!(gate.active());
        gate.step(&mut g, &Afterpull::command(-11, 0));
        assert!(
            !g.cleared(),
            "fresh undo after releasing controls must work"
        );
        assert_eq!(g.state.tick, before.tick + 1);
    }
}

#[test]
fn focus_loss_freezes_time_and_clears_a_pit_forecast_without_failing() {
    use crate::focus_gate::FocusGate;
    let mut g = Afterpull::new(7);
    for tick in 0..Afterpull::VERIFY_TICKS {
        if g.state.room == 5 {
            break;
        }
        g.step(&Afterpull::verification_input(tick));
    }
    for ch in "RLLURUUL".chars() {
        g.step(&Afterpull::direction("URDL".find(ch).unwrap()));
    }
    g.step(&Afterpull::preview(1));
    let board = g.state.board.clone();
    let tick = g.tick();
    let history = g.state.history.clone();
    let mut gate = FocusGate::default();
    gate.observe(false, true, false);
    for _ in 0..120 {
        gate.step(&mut g, &Afterpull::direction(1));
    }
    assert_eq!(g.state.board, board);
    assert_eq!(g.state.history, history);
    assert_eq!(g.tick(), tick);
    assert_eq!(g.state.preview, None);
    assert!(!g.failed());
    gate.observe(true, false, true);
    gate.step(&mut g, &Afterpull::direction(1));
    assert!(
        g.failed(),
        "a fresh intentional press still follows the pit rule"
    );
}
