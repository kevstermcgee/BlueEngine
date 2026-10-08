use super::*;
use std::collections::{HashSet, VecDeque};
use vesper3d::runtime::snapshot;
fn move_clicks(g: &mut GapFlip, token: usize, axis: Axis) {
    let from = g.state.tokens[token];
    let to = corridor(g.room(), &g.state.tokens, token, axis).destination;
    g.step(&click(g.room().center(from)));
    g.step(&click(center(axis.button())));
    g.step(&click(g.room().center(to)));
}
fn distance(room: &Room, locked: Option<usize>) -> Option<usize> {
    let mut visited = HashSet::from([room.start.to_vec()]);
    let mut queue = VecDeque::from([(room.start.to_vec(), 0)]);
    while let Some((state, depth)) = queue.pop_front() {
        if state == room.goals {
            return Some(depth);
        }
        for i in 0..state.len() {
            if Some(i) == locked {
                continue;
            }
            for axis in [Axis::Horizontal, Axis::Vertical] {
                let mut next = state.clone();
                next[i] = corridor(room, &state, i, axis).destination;
                if visited.insert(next.clone()) {
                    queue.push_back((next, depth + 1));
                }
            }
        }
    }
    None
}
#[test]
fn reflection_exchanges_gaps_and_is_its_own_inverse() {
    let r = &ROOMS[0];
    let g = corridor(r, r.start, 0, Axis::Horizontal);
    assert_eq!(
        g,
        Corridor {
            destination: 10,
            before: 1,
            after: 4
        }
    );
    let inverse = corridor(r, &[g.destination], 0, Axis::Horizontal);
    assert_eq!(
        inverse,
        Corridor {
            destination: 7,
            before: 4,
            after: 1
        }
    );
    let r = &ROOMS[2];
    let g = corridor(r, r.start, 0, Axis::Horizontal);
    assert_eq!(
        g,
        Corridor {
            destination: 1,
            before: 0,
            after: 1
        }
    ); // B, not the far board edge, bounds A.
    let mut positions = r.start.to_vec();
    positions[1] = 5;
    assert_eq!(corridor(r, &positions, 0, Axis::Horizontal).destination, 4);
    let r = &ROOMS[1];
    assert_eq!(corridor(r, &[6], 0, Axis::Horizontal).destination, 5); // pillar at column 2
    assert_eq!(corridor(r, &[6], 0, Axis::Vertical).destination, 16);
}
#[test]
fn every_two_token_corridor_preserves_boundaries_and_reverses_exactly() {
    // Enumerate floor placements, including offsets, equal gaps and neighbors on either axis.
    // The invariant is stronger than testing only the authored winning routes.
    for room in ROOMS {
        let floors: Vec<_> = (0..(room.width() * room.height()) as u8)
            .filter(|&p| {
                let (x, y) = room.xy(p);
                room.floor(x, y)
            })
            .collect();
        for &moving in &floors {
            for &boundary in &floors {
                if moving == boundary {
                    continue;
                }
                for axis in [Axis::Horizontal, Axis::Vertical] {
                    let gap = corridor(room, &[moving, boundary], 0, axis);
                    assert!(floors.contains(&gap.destination));
                    assert_ne!(gap.destination, boundary);
                    let (x, y) = room.xy(moving);
                    let (qx, qy) = room.xy(gap.destination);
                    match axis {
                        Axis::Horizontal => assert_eq!(y, qy),
                        Axis::Vertical => assert_eq!(x, qx),
                    }
                    assert_eq!(
                        (qx - x).abs() + (qy - y).abs(),
                        (i32::from(gap.before) - i32::from(gap.after)).abs()
                    );
                    let reverse = corridor(room, &[gap.destination, boundary], 0, axis);
                    assert_eq!(reverse.destination, moving);
                    assert_eq!((reverse.before, reverse.after), (gap.after, gap.before));
                }
            }
        }
    }
}
#[test]
fn exact_preview_needs_confirmation_and_invalid_clicks_cost_nothing() {
    let mut g = GapFlip::new(7);
    g.step(&click(center(HORIZONTAL)));
    assert_eq!(g.state.selected, None);
    g.step(&click(g.room().center(7)));
    g.step(&click(center(VERTICAL)));
    assert_eq!(g.state.axis, None); // equal 0/0
    assert!(g.state.history.is_empty());
    assert_eq!(g.state.tokens, [7]);
    g.step(&click(center(HORIZONTAL)));
    assert_eq!(g.preview().unwrap().destination, 10);
    assert_eq!(g.state.tokens, [7]);
    g.step(&click(g.room().center(9)));
    g.step(&click(Point::new(-100, 220)));
    g.step(&Intent {
        action: true,
        ..Default::default()
    });
    assert_eq!(g.state.tokens, [7]);
    g.step(&click(g.room().center(10)));
    assert!(g.solved());
    assert_eq!(g.state.history.len(), 1);
    assert_eq!(g.outcome(), "playing"); // completion can still be undone; no terminal loss
    g.step(&click(center(UNDO)));
    assert_eq!(g.state.tokens, [7]);
    assert!(!g.solved());
    g.step(&click(center(UNDO)));
    assert_eq!(g.state.tokens, [7]);
}
#[test]
fn solved_token_must_leave_and_return_and_restart_keeps_current_room() {
    let mut g = GapFlip::new(7);
    g.enter_room(3);
    let room = g.room();
    assert_eq!(g.state.tokens[0], room.goals[0]);
    assert_eq!(
        distance(room, Some(0)),
        None,
        "A cannot remain locked on its starting goal"
    );
    let mut left_goal = false;
    for &(token, axis) in room.solution {
        move_clicks(&mut g, token, axis);
        left_goal |= g.state.tokens[0] != room.goals[0];
        assert_ne!(g.outcome(), "lost");
    }
    assert!(left_goal && g.solved());
    g.step(&click(center(UNDO)));
    assert!(!g.solved());
    g.restart();
    assert_eq!(g.state.room, 3);
    assert_eq!(g.state.tokens, room.start);
    assert!(g.state.history.is_empty());
    // Full history, not just the last move, remains available.
    for &(token, axis) in room.solution {
        move_clicks(&mut g, token, axis);
    }
    for _ in room.solution {
        g.step(&click(center(UNDO)));
    }
    assert_eq!(g.state.tokens, room.start);
}
#[test]
fn ten_authored_rooms_are_solvable_escalating_and_require_dependencies() {
    assert_eq!(ROOMS.len(), 10);
    let mut previous = 0;
    for (index, room) in ROOMS.iter().enumerate() {
        assert!(room.width() <= 6 && room.height() <= 6);
        assert!(room.valid_positions(room.start));
        assert!(room.valid_positions(room.goals));
        let shortest = distance(room, None).expect("authored room has a solution");
        assert_eq!(
            shortest,
            room.solution.len(),
            "{} solution length",
            room.name
        );
        assert!(shortest > previous);
        previous = shortest;
        let mut game = GapFlip::new(7);
        game.enter_room(index);
        for &(token, axis) in room.solution {
            move_clicks(&mut game, token, axis);
        }
        assert!(game.solved(), "{} public click solution", room.name);
        if index < 2 {
            continue;
        }
        let independent = room.start.iter().enumerate().all(|(i, &start)| {
            let mut visited = HashSet::from([start]);
            let mut queue = VecDeque::from([start]);
            while let Some(p) = queue.pop_front() {
                let mut tokens = room.start.to_vec();
                tokens[i] = p;
                for axis in [Axis::Horizontal, Axis::Vertical] {
                    let q = corridor(room, &tokens, i, axis).destination;
                    if visited.insert(q) {
                        queue.push_back(q);
                    }
                }
            }
            visited.contains(&room.goals[i])
        });
        assert!(
            !independent,
            "{} must require a changing neighbor boundary",
            room.name
        );
    }
}
#[test]
fn public_campaign_is_deterministic_and_snapshot_resumes_every_kind_of_state() {
    let inputs: Vec<_> = (0..GapFlip::VERIFY_TICKS)
        .map(GapFlip::verification_input)
        .collect();
    assert!(verification_route().len() <= inputs.len());
    vesper3d::runtime::assert_deterministic(|| GapFlip::new(7), &inputs);
    snapshot::assert_resumes_exactly(|| GapFlip::new(7), &inputs, 223);
    let mut g = GapFlip::new(7);
    for input in &inputs {
        g.step(input);
    }
    assert_eq!(g.outcome(), "won");
    assert_eq!(g.state.room, 9);
    assert!(g.solved());
    let (hash, outcome) = verify::<GapFlip>();
    assert_eq!(hash, g.state_hash());
    if let Ok(path) = std::env::var("BE2_VERIFY_REPORT") {
        std::fs::write(
            path,
            serde_json::json!({"hash":format!("{hash:016x}"),"outcome":outcome}).to_string(),
        )
        .unwrap();
    }
    // Capture pending preview, then mid-exchange, then undo after restoring into a new game.
    let mut g = GapFlip::new(7);
    g.step(&GapFlip::probe_input());
    assert!(g.probe_success());
    g.step(&click(center(HORIZONTAL)));
    let preview = snapshot::save(&g, "preview").unwrap();
    let mut resumed = GapFlip::new(9);
    snapshot::restore(&mut resumed, &preview).unwrap();
    assert_eq!(resumed.preview(), g.preview());
    g.step(&click(g.room().center(10)));
    resumed.step(&click(g.room().center(10)));
    assert_eq!(g.state_hash(), resumed.state_hash());
    let bytes = snapshot::save(&g, "exchanging").unwrap();
    snapshot::restore(&mut resumed, &bytes).unwrap();
    for _ in 0..7 {
        g.step(&Intent::default());
        resumed.step(&Intent::default());
    }
    assert_eq!(g.state_hash(), resumed.state_hash());
    resumed.step(&click(center(UNDO)));
    assert_eq!(resumed.state.tokens, [7]);
    assert!(!resumed.solved());
}
#[test]
fn malformed_saves_fail_without_changing_state() {
    let mut g = GapFlip::new(7);
    let hash = g.state_hash();
    let mut bad = g.capture();
    bad.tokens[0] = 0;
    assert!(g.restore(bad).is_err());
    assert_eq!(g.state_hash(), hash);
    let mut bad = g.capture();
    bad.selected = Some(9);
    assert!(g.restore(bad).is_err());
    assert_eq!(g.state_hash(), hash);
    let mut bad = g.capture();
    bad.tokens[0] = 8;
    bad.history.push(vec![7]);
    assert!(g.restore(bad).is_err());
    assert_eq!(g.state_hash(), hash);
    let mut bad = g.capture();
    bad.finished = true;
    assert!(g.restore(bad).is_err());
    assert_eq!(g.state_hash(), hash);
    g.enter_room(2);
    let mut bad = g.capture();
    bad.tokens[1] = bad.tokens[0];
    let hash = g.state_hash();
    assert!(g.restore(bad).is_err());
    assert_eq!(g.state_hash(), hash);
}
