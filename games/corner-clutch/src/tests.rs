use super::*;
use vesper3d::runtime::snapshot;

use input::NativeCommand;
use vesper3d::runtime::InputAccumulator;

// Reproduce the shared client's accumulator and retained action target around
// the actual game-owned native adapter. Display frames need not run a tick.
struct NativeHarness {
    game: Clutch,
    inputs: InputAccumulator<Intent>,
    target: Option<Point>,
    replay: Vec<Intent>,
}
impl NativeHarness {
    fn new() -> Self {
        Self {
            game: Clutch::new(7),
            inputs: InputAccumulator::new(),
            target: None,
            replay: vec![],
        }
    }
    fn frame(&mut self, commands: impl IntoIterator<Item = NativeCommand>, ticks: usize) {
        // Cursor coincidence and a physically held key must neither lose the
        // synthetic target nor cause extra axis moves.
        let physical = Intent {
            pointer: Some(center(self.game.state.cells[2])),
            y: -1,
            ..Intent::default()
        };
        let mapped = self.game.native_input(physical, commands);
        if mapped.action {
            self.target = if mapped.pointer != physical.pointer {
                mapped.pointer
            } else {
                None
            };
        }
        self.inputs.feed(
            Intent {
                action: false,
                ..mapped
            },
            u32::from(mapped.action),
            [0.; 2],
        );
        for _ in 0..ticks {
            let tick = self.inputs.take_tick();
            let action = tick.pressed(1);
            let intent = Intent {
                action,
                pointer: if action { self.target.take() } else { None },
                ..tick.held
            };
            self.game.step(&intent);
            self.replay.push(intent);
        }
    }
}
fn native_step(direction: usize) -> NativeCommand {
    NativeCommand::Target(click(BUTTONS[direction]).pointer.unwrap())
}
#[test]
fn native_selection_then_movement_stays_ordered_before_a_fixed_tick() {
    let mut h = NativeHarness::new();
    // Two display frames, neither with a fixed tick: click C, then tap DOWN.
    h.frame(
        NativeCommand::pointer(&h.game.state.cells, center([4, 1])),
        0,
    );
    h.frame([native_step(2)], 0);
    h.frame([], 1);
    assert_eq!(
        h.game.state.selected, 2,
        "movement replaced the pending selection"
    );
    assert_eq!(
        h.game.state.cells,
        rooms()[0].start,
        "selection alone must not move"
    );
    h.frame([], 4);
    assert_eq!(h.game.state.cells, vec![[1, 2], [2, 3], [4, 2]]);
    assert_eq!(h.game.state.history.len(), 1);
    for _ in 0..8 {
        h.frame([], 4);
    }
    assert_eq!(h.game.state.history.len(), 1, "the queued edge repeated");
}
#[test]
fn native_selection_moves_tabs_and_undo_survive_mixed_render_schedules() {
    for ticks in [0, 1, 4] {
        for batch in [false, true] {
            let mut h = NativeHarness::new();
            let commands = [
                NativeCommand::Cycle,
                NativeCommand::Cycle,
                native_step(2),
                NativeCommand::Target(click(UNDO).pointer.unwrap()),
                NativeCommand::Select(1),
                native_step(0),
            ];
            if batch {
                h.frame(commands, ticks);
            } else {
                for command in commands {
                    h.frame([command], ticks);
                }
            }
            for _ in 0..12 {
                h.frame([], 4);
            }
            assert_eq!(h.game.state.selected, 1);
            assert_eq!(h.game.state.cells, vec![[1, 1], [2, 2], [4, 1]]);
            assert_eq!(h.game.state.history.len(), 1);
            let mut replay = Clutch::new(7);
            for i in &h.replay {
                replay.step(i);
            }
            assert_eq!(replay.state_hash(), h.game.state_hash());
            // Loading uses the engine save frame and retains the chosen driver.
            let saved = snapshot::save(&h.game, "ordered commands").unwrap();
            snapshot::restore(&mut replay, &saved).unwrap();
            replay.step(&click(UNDO));
            assert_eq!(replay.state.cells, rooms()[0].start);
            assert_eq!(replay.state.selected, 1);
        }
    }
}
#[test]
fn native_click_keeps_crate_identity_when_an_earlier_command_moves_it() {
    let mut h = NativeHarness::new();
    h.frame([native_step(0)], 0);
    let clicked = NativeCommand::pointer(&h.game.state.cells, center([2, 3])).unwrap();
    h.frame([clicked, native_step(3)], 0);
    for _ in 0..6 {
        h.frame([], 4);
    }
    assert_eq!(h.game.state.selected, 1);
    assert_eq!(h.game.state.cells, vec![[2, 1], [3, 2], [4, 1]]);
    assert_eq!(h.game.state.history.len(), 2);
}
#[test]
fn native_input_does_not_change_authority_and_restart_and_load_discard_pending_events() {
    for load in [false, true] {
        let mut g = Clutch::new(7);
        let saved = snapshot::save(&g, "before device input").unwrap();
        let before = g.state_hash();
        let mapped = g.native_input(
            Intent::default(),
            [NativeCommand::Select(2), native_step(2)],
        );
        assert!(mapped.action);
        assert_eq!(
            g.state_hash(),
            before,
            "device polling changed the simulation"
        );
        if load {
            snapshot::restore(&mut g, &saved).unwrap();
        } else {
            g.restart();
        }
        for _ in 0..4 {
            let mapped = g.native_input(Intent::default(), []);
            assert!(!mapped.action, "stale device event after reset/load");
            g.step(&mapped);
        }
        assert_eq!(g.state.cells, rooms()[0].start);
        assert_eq!(g.state.selected, 0);
        assert!(g.state.history.is_empty());
    }
}

#[test]
fn native_button_edge_clicks_remain_inside_the_button_when_deferred() {
    let cells = &rooms()[0].start;
    for r in BUTTONS.into_iter().chain([UNDO, HINT, NEXT]) {
        for x in [r.x, r.x + r.w - 1] {
            for y in [r.y, r.y + r.h - 1] {
                let command = NativeCommand::pointer(cells, Point::new(x, y)).unwrap();
                let mut g = Clutch::new(7);
                let center = click(r).pointer.unwrap();
                let mapped = g.native_input(
                    Intent {
                        pointer: Some(center),
                        ..Intent::default()
                    },
                    [command],
                );
                assert!(mapped.action);
                assert!(r.contains(mapped.pointer.unwrap()));
            }
        }
    }
    assert!(NativeCommand::pointer(cells, Point::new(799, 449)).is_none());
}

fn step_dir(g: &mut Clutch, dir: Cell) {
    g.step(&Intent::default());
    g.step(&Intent {
        x: dir[0],
        y: dir[1],
        ..Intent::default()
    });
}
#[test]
fn driver_dependent_pickup_is_nonrecursive_and_side_neighbors_do_not_ride() {
    let chain = vec![[1, 1], [2, 2], [3, 3], [1, 2]];
    assert_eq!(passengers(&chain, 0), vec![0, 1]);
    assert_eq!(passengers(&chain, 1), vec![0, 1, 2]);
    assert_eq!(passengers(&chain, 2), vec![1, 2]);
    let mut g = Clutch::new(7);
    g.load_room(2);
    step_dir(&mut g, [0, -1]);
    assert_eq!(g.state.cells, vec![[1, 0], [2, 1], [3, 3]]);
    g.restart();
    g.step(&select([2, 2]));
    step_dir(&mut g, [0, -1]);
    assert_eq!(g.state.cells, vec![[1, 0], [2, 1], [3, 2]]);
}
#[test]
fn a_single_blocker_rejects_every_passenger_then_undo_and_retry_recover() {
    let mut g = Clutch::new(7);
    g.load_room(1);
    let initial = g.state.cells.clone();
    step_dir(&mut g, [1, 0]);
    assert_eq!(g.state.cells, initial);
    assert!(g.state.history.is_empty());
    assert_eq!(g.state.blocked[0].kind, Obstacle::Crate);
    assert_eq!(g.outcome(), "playing"); // No artificial death or budget.
    let mut g = Clutch::new(7);
    step_dir(&mut g, [0, -1]);
    step_dir(&mut g, [0, -1]);
    let before = g.state.cells.clone();
    step_dir(&mut g, [0, -1]);
    assert_eq!(g.state.cells, before);
    assert_eq!(g.state.blocked[0].kind, Obstacle::Boundary);
    assert_eq!(g.state.history.len(), 2);
    g.step(&click(UNDO));
    assert_eq!(g.state.cells, vec![[1, 1], [2, 2], [4, 1]]);
    g.restart();
    assert_eq!(g.state.cells, rooms()[0].start);
    assert!(g.state.history.is_empty());
    g.load_room(1);
    g.state.cells = vec![[2, 1], [1, 2], [4, 4]];
    let before = g.state.cells.clone();
    step_dir(&mut g, [1, 0]);
    assert_eq!(g.state.cells, before);
    assert_eq!(g.state.blocked[0].kind, Obstacle::Wall);
}
#[test]
fn parked_helpers_remain_mobile_and_crews_are_recalculated() {
    let mut g = Clutch::new(7);
    g.load_room(4);
    assert_eq!(g.state.cells[1], g.room().goals[1]);
    g.step(&select(g.state.cells[1]));
    step_dir(&mut g, [-1, 0]);
    assert_ne!(g.state.cells[1], g.room().goals[1]);
    assert_eq!(g.outcome(), "playing");
    let initial = passengers(&g.state.cells, 0);
    g.state.cells = vec![[1, 1], [2, 2], [3, 3]];
    g.state.selected = 0;
    step_dir(&mut g, [0, 1]);
    assert_eq!(g.state.cells, vec![[1, 2], [2, 3], [3, 3]]);
    assert_ne!(initial, passengers(&g.state.cells, 2));
    assert_eq!(passengers(&g.state.cells, 2), vec![2]);
}
#[test]
fn keyboard_is_one_step_per_tap_and_click_buttons_share_the_same_rule() {
    let mut a = Clutch::new(7);
    let mut b = Clutch::new(7);
    for _ in 0..30 {
        a.step(&Intent {
            y: -1,
            ..Intent::default()
        });
    }
    assert_eq!(a.state.history.len(), 1);
    b.step(&click(BUTTONS[0]));
    assert_eq!(a.state.cells, b.state.cells);
    a.step(&Intent::default());
    a.step(&Intent {
        y: -1,
        ..Intent::default()
    });
    assert_eq!(a.state.history.len(), 2);
    a.step(&click(UNDO));
    assert_eq!(a.state.cells, b.state.cells);
}
#[test]
fn releasing_one_of_two_held_directions_never_issues_a_move() {
    for first in DIRS {
        for second in DIRS {
            if first[0] * second[0] + first[1] * second[1] != 0 {
                continue;
            }
            for remaining in [first, second] {
                let mut g = Clutch::new(7);
                let input = |d: Cell| Intent {
                    x: d[0],
                    y: d[1],
                    ..Intent::default()
                };
                g.step(&input(first));
                g.step(&input(add(first, second)));
                assert_eq!(g.state.history.len(), 2, "each new press moves once");
                let cells = g.state.cells.clone();
                // Save with both axes held; loading must retain their edge state.
                let saved = snapshot::save(&g, "two held directions").unwrap();
                let mut resumed = Clutch::new(7);
                snapshot::restore(&mut resumed, &saved).unwrap();
                for _ in 0..30 {
                    g.step(&input(remaining));
                    resumed.step(&input(remaining));
                    assert_eq!(g.state.cells, cells, "release moved {first:?}/{second:?}");
                    assert_eq!(g.state.history.len(), 2);
                    assert_eq!(g.state_hash(), resumed.state_hash());
                }
                // Releasing both and tapping again still moves.
                step_dir(&mut g, [-remaining[0], -remaining[1]]);
                assert_eq!(g.state.history.len(), 3);
            }
        }
    }
}
#[test]
fn version_one_saves_migrate_with_the_board_undo_history_and_held_direction() {
    struct Legacy(serde_json::Value);
    impl Simulation for Legacy {
        type Input = Intent;
        fn step(&mut self, _: &Intent) {}
        fn state_hash(&self) -> u64 {
            hash_json(&self.0)
        }
    }
    impl Snapshot for Legacy {
        const KIND: &'static str = Clutch::KIND;
        type State = serde_json::Value;
        fn capture(&self) -> Self::State {
            self.0.clone()
        }
        fn restore(&mut self, value: Self::State) -> Result<(), String> {
            self.0 = value;
            Ok(())
        }
        fn content(&self) -> Option<u64> {
            Clutch::new(7).content()
        }
    }
    let mut g = Clutch::new(7);
    step_dir(&mut g, [0, -1]);
    let mut old = serde_json::to_value(g.capture()).unwrap();
    let value = old.as_object_mut().unwrap();
    let axes = value.remove("held_axes").unwrap();
    value.insert("last_dir".into(), axes);
    let saved = snapshot::save(&Legacy(old), "version one").unwrap();
    let mut resumed = Clutch::new(7);
    snapshot::restore(&mut resumed, &saved).unwrap();
    assert_eq!(resumed.capture(), g.capture());
    resumed.step(&Intent {
        y: -1,
        ..Intent::default()
    });
    assert_eq!(resumed.state.history.len(), 1);
    resumed.step(&click(UNDO));
    assert_eq!(resumed.state.cells, rooms()[0].start);
}
#[test]
fn undo_and_driver_hotkeys_preserve_a_held_direction_across_render_schedules() {
    use vesper3d::runtime::InputAccumulator;
    // A frame may run zero, one, or several fixed ticks. The shared client
    // retains the action target separately until its accumulated edge is used.
    for ticks_in_command_frame in [0, 1, 4] {
        for undo in [true, false] {
            let mut g = Clutch::new(7);
            let held = Intent {
                y: -1,
                ..Intent::default()
            };
            g.step(&held);
            let moved = g.state.cells.clone();
            let target = if undo {
                click(UNDO).pointer.unwrap()
            } else {
                center(g.state.cells[2])
            };
            // Exact cursor coincidence used to cause the engine to discard Z,
            // number-key selection, Tab, and Enter as untargeted actions.
            let native = Intent {
                pointer: Some(target),
                ..held
            };
            let mapped = keyboard_target(native, target);
            assert_ne!(mapped.pointer, native.pointer);
            assert_eq!((mapped.x, mapped.y), (held.x, held.y));
            let mut target = mapped.pointer;
            let mut inputs = InputAccumulator::<Intent>::new();
            inputs.feed(mapped, u32::from(mapped.action), [0.; 2]);
            for count in [ticks_in_command_frame, 4] {
                for _ in 0..count {
                    let tick = inputs.take_tick();
                    let action = tick.pressed(1);
                    g.step(&Intent {
                        action,
                        pointer: if action { target.take() } else { None },
                        ..tick.held
                    });
                }
                // Next display frame still sees the movement key held.
                inputs.feed(held, 0, [0.; 2]);
            }
            if undo {
                assert_eq!(g.state.cells, rooms()[0].start);
                assert!(g.state.history.is_empty());
            } else {
                assert_eq!(g.state.cells, moved);
                assert_eq!(g.state.selected, 2);
                assert_eq!(g.state.history.len(), 1);
            }
            // Only a real release followed by another press allows a new step.
            step_dir(&mut g, [0, -1]);
            assert_eq!(g.state.history.len(), if undo { 1 } else { 2 });
        }
    }
}
#[test]
fn coincident_hotkey_targets_stay_inside_their_real_click_regions() {
    let mut g = Clutch::new(7);
    step_dir(&mut g, [0, -1]);
    for j in 0..g.state.cells.len() {
        let target = center(g.state.cells[j]);
        g.step(&keyboard_target(
            Intent {
                pointer: Some(target),
                ..Intent::default()
            },
            target,
        ));
        assert_eq!(g.state.selected, j);
    }
    let target = click(UNDO).pointer.unwrap();
    let command = keyboard_target(
        Intent {
            pointer: Some(target),
            ..Intent::default()
        },
        target,
    );
    assert!(UNDO.contains(command.pointer.unwrap()));
    g.step(&command);
    assert_eq!(g.state.cells, rooms()[0].start);
    for &[j, x, y] in &rooms()[0].solution {
        g.step(&select(g.state.cells[j as usize]));
        step_dir(&mut g, [x, y]);
    }
    let target = click(NEXT).pointer.unwrap();
    let command = keyboard_target(
        Intent {
            pointer: Some(target),
            ..Intent::default()
        },
        target,
    );
    assert!(NEXT.contains(command.pointer.unwrap()));
    g.step(&command);
    assert_eq!(g.state.room, 1);
}
#[test]
fn targeted_commands_consume_simultaneous_direction_edges_without_a_second_move() {
    let mut g = Clutch::new(7);
    step_dir(&mut g, [0, -1]);
    let left = Intent {
        x: -1,
        ..Intent::default()
    };
    g.step(&keyboard_target(left, click(UNDO).pointer.unwrap()));
    g.step(&left);
    assert_eq!(g.state.cells, rooms()[0].start);
    assert!(g.state.history.is_empty());
    // An explicit direction-button click remains a command even while a
    // different keyboard direction is newly pressed.
    g.step(&Intent {
        x: 1,
        ..click(BUTTONS[0])
    });
    assert_eq!(g.state.cells, vec![[1, 1], [2, 2], [4, 1]]);
    assert_eq!(g.state.history.len(), 1);
    // Mouse selection also works while a movement key remains held.
    g.step(&Intent {
        x: 1,
        ..select(g.state.cells[2])
    });
    g.step(&Intent {
        x: 1,
        ..Intent::default()
    });
    assert_eq!(g.state.selected, 2);
    assert_eq!(g.state.history.len(), 1);
}
#[test]
fn a_passenger_blocker_is_atomic_and_preview_never_changes_the_board() {
    let mut g = Clutch::new(7);
    g.load_room(1);
    // Driver A can move right, but its passenger B would hit the wall at (3,2).
    g.state.cells = vec![[1, 1], [2, 2], [5, 5]];
    g.state.slide_from = g.state.cells.clone();
    let before = g.capture();
    let preview = plan(g.room(), &g.state.cells, 0, [1, 0]);
    assert_eq!(g.capture(), before);
    assert_eq!(preview.members, vec![0, 1]);
    assert_eq!(preview.destinations, vec![(0, [2, 1]), (1, [3, 2])]);
    assert_eq!(
        preview.blocked,
        vec![Blocked {
            cell: [3, 2],
            kind: Obstacle::Wall
        }]
    );
    g.step(&click(BUTTONS[3]));
    assert_eq!(g.state.cells, before.cells);
    assert_eq!(g.state.blocked, preview.blocked);
    assert!(g.state.history.is_empty());
    assert_eq!(g.outcome(), "playing");
}
#[test]
fn new_diagonals_wait_for_the_next_command_and_undo_restores_every_move() {
    let mut g = Clutch::new(7);
    g.load_room(2);
    // Moving A+B right creates an A-C corner. C must stay put on this command.
    g.state.cells = vec![[1, 1], [2, 2], [3, 0]];
    g.state.slide_from = g.state.cells.clone();
    let start = g.state.cells.clone();
    g.step(&click(BUTTONS[3]));
    assert_eq!(g.state.cells, vec![[2, 1], [3, 2], [3, 0]]);
    assert_eq!(passengers(&g.state.cells, 0), vec![0, 1, 2]);
    g.step(&click(UNDO));
    assert_eq!(g.state.cells, start);
    assert_eq!(g.state.selected, 0);

    // No move budget or undo cap: every accepted command can be recovered.
    g.restart();
    let start = g.state.cells.clone();
    for _ in 0..120 {
        g.step(&click(BUTTONS[0]));
        g.step(&click(BUTTONS[2]));
    }
    assert_eq!(g.state.history.len(), 240);
    assert_eq!(g.outcome(), "playing");
    let saved = snapshot::save(&g, "long undo history").unwrap();
    let mut resumed = Clutch::new(7);
    snapshot::restore(&mut resumed, &saved).unwrap();
    for _ in 0..240 {
        resumed.step(&click(UNDO));
    }
    assert_eq!(resumed.state.cells, start);
    assert!(resumed.state.history.is_empty());
}
#[test]
fn all_eight_authored_routes_solve_via_public_inputs_and_different_drivers() {
    for (n, room) in rooms().iter().enumerate() {
        let mut g = Clutch::new(7);
        g.load_room(n);
        assert!(valid_cells(room, &room.start));
        assert!(valid_cells(room, &room.goals));
        for &[j, x, y] in &room.solution {
            let before = g.state.history.len();
            g.step(&select(g.state.cells[j as usize]));
            step_dir(&mut g, [x, y]);
            assert_eq!(
                g.state.history.len(),
                before + 1,
                "{} rejected a solution move",
                room.title
            );
        }
        assert!(g.solved(), "{} not solved", room.title);
        if n < 7 {
            g.step(&click(NEXT));
            assert_eq!(g.state.room, n + 1);
            g.restart();
            assert_eq!(g.state.room, n + 1);
        } else {
            assert_eq!(g.outcome(), "won");
            g.step(&click(UNDO));
            assert_eq!(g.outcome(), "playing");
        }
    }
}
#[test]
fn middle_room_teaches_a_whole_chain_carry_then_an_endpoint_split() {
    let mut g = Clutch::new(7);
    g.load_room(2);
    let solution = g.room().solution.clone();
    let mut crew_sizes = vec![];
    for [driver, x, y] in solution {
        g.step(&select(g.state.cells[driver as usize]));
        crew_sizes.push(passengers(&g.state.cells, g.state.selected).len());
        step_dir(&mut g, [x, y]);
    }
    assert_eq!(crew_sizes, vec![3, 3, 3, 2, 1, 1]);
    assert!(g.solved());
    // Recover the split through the public undo route, including its driver.
    for _ in 0..3 {
        g.step(&click(UNDO));
    }
    assert_eq!(g.state.cells, vec![[2, 3], [3, 4], [4, 5]]);
    assert_eq!(g.state.selected, 2);
    assert_eq!(passengers(&g.state.cells, 1), vec![0, 1, 2]);
    assert_eq!(passengers(&g.state.cells, 2), vec![1, 2]);
}
#[test]
fn wins_deterministically_and_save_resumes_including_undo_history() {
    let inputs: Vec<_> = (0..Clutch::VERIFY_TICKS)
        .map(Clutch::verification_input)
        .collect();
    assert!(verification_route().len() < Clutch::VERIFY_TICKS as usize);
    vesper3d::runtime::assert_deterministic(|| Clutch::new(7), &inputs);
    snapshot::assert_resumes_exactly(|| Clutch::new(7), &inputs, 173);
    let (hash, outcome) = verify::<Clutch>();
    assert_eq!(outcome, "won");
    if let Ok(path) = std::env::var("BE2_VERIFY_REPORT") {
        std::fs::write(
            path,
            serde_json::json!({"hash":format!("{hash:016x}"),"outcome":outcome}).to_string(),
        )
        .unwrap();
    }
    let mut a = Clutch::new(7);
    step_dir(&mut a, [0, -1]);
    let saved = a.capture();
    let mut b = Clutch::new(7);
    b.restore(saved).unwrap();
    a.step(&click(UNDO));
    b.step(&click(UNDO));
    assert_eq!(a.state, b.state);
}
#[test]
fn invalid_saves_are_atomic_and_restart_only_resets_the_current_room() {
    let mut g = Clutch::new(7);
    g.load_room(6);
    step_dir(&mut g, [0, -1]);
    let hash = g.state_hash();
    let mut invalid = g.capture();
    invalid.history[0].cells[0] = [90, 0];
    assert!(g.restore(invalid).is_err());
    assert_eq!(g.state_hash(), hash);
    let mut invalid = g.capture();
    invalid.cells[1] = invalid.cells[0];
    assert!(g.restore(invalid).is_err());
    assert_eq!(g.state_hash(), hash);
    g.restart();
    assert_eq!(g.state.room, 6);
    assert_eq!(g.state.cells, rooms()[6].start);
}

#[test]
fn a_reachable_deadlock_has_no_terminal_loss_and_undo_recovers_it() {
    let mut g = Clutch::new(7);
    g.load_room(6);
    let route = [
        (2, [-1, 0]),
        (3, [-1, 0]),
        (3, [-1, 0]),
        (3, [-1, 0]),
        (0, [0, 1]),
        (3, [0, -1]),
    ];
    for (driver, dir) in route {
        g.step(&select(g.state.cells[driver]));
        step_dir(&mut g, dir);
    }
    assert_eq!(g.state.cells, vec![[1, 2], [2, 3], [3, 2], [0, 3]]);
    assert_eq!(g.state.history.len(), route.len());
    assert!(!g.solved());
    for driver in 0..g.state.cells.len() {
        for dir in DIRS {
            assert!(!plan(g.room(), &g.state.cells, driver, dir)
                .blocked
                .is_empty());
        }
    }
    let trapped = g.state.cells.clone();
    for _ in 0..3600 {
        g.step(&Intent::default());
    }
    assert_eq!(g.outcome(), "playing");
    g.step(&click(BUTTONS[0]));
    assert_eq!(g.state.cells, trapped);
    assert_eq!(g.state.history.len(), route.len());

    // Engine-framed saves retain the recovery path even from a completely stuck board.
    let saved = snapshot::save(&g, "deadlocked room").unwrap();
    let mut resumed = Clutch::new(7);
    snapshot::restore(&mut resumed, &saved).unwrap();
    resumed.step(&click(UNDO));
    assert_ne!(resumed.state.cells, trapped);
    assert!(DIRS.iter().any(|&dir| plan(
        resumed.room(),
        &resumed.state.cells,
        resumed.state.selected,
        dir
    )
    .blocked
    .is_empty()));
    for _ in 1..route.len() {
        resumed.step(&click(UNDO));
    }
    assert_eq!(resumed.state.cells, rooms()[6].start);
    assert_eq!(resumed.state.room, 6);
    assert!(resumed.state.history.is_empty());
}
