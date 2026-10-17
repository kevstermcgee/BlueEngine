use pinstride::*;
use vesper3d::viewer::devkit::{assert_deterministic, snapshot, Simulation, Snapshot};
fn commit(sim: &mut Sim, i: usize) {
    sim.step(&Intent { select: Some(i), commit: true, ..Default::default() });
    for _ in 0..SWING_TICKS {
        sim.step(&Intent::default());
    }
}
fn script() -> Vec<Intent> {
    let mut out = vec![];
    let mut s = Sim::new(7);
    for room in 0..ROOM_COUNT {
        let route = solve(s.room(), s.state.pose).unwrap_or_else(|| panic!("room {room} has no solution"));
        for i in route {
            let input = Intent { select: Some(i), commit: true, ..Default::default() };
            out.push(input);
            s.step(&input);
            for _ in 0..SWING_TICKS {
                out.push(Intent::default());
                s.step(&Intent::default());
            }
        }
        assert!(s.won());
        let input = Intent { next: true, ..Default::default() };
        out.push(input);
        s.step(&input);
    }
    out
}
#[test]
fn every_room_has_a_public_route_and_escalates() {
    let mut sim = Sim::new(7);
    for room in 0..ROOM_COUNT {
        assert_eq!(sim.state.room, room);
        assert!(pose_clear(sim.room(), sim.state.pose));
        let route = solve(sim.room(), sim.state.pose).unwrap_or_else(|| panic!("no solution for {room}"));
        println!(
            "{} {}: {} swings {:?}",
            room + 1,
            sim.room().name,
            route.len(),
            route.iter().map(|i| i + 1).collect::<Vec<_>>()
        );
        if room >= 3 {
            assert!(route.len() >= 5);
        }
        let mut vertical = false;
        let mut depth = false;
        for i in route {
            let dir = sim.state.pose.directions()[i];
            vertical |= dir[1] != 0;
            depth |= dir[2] != 0;
            commit(&mut sim, i);
        }
        if room >= 3 {
            assert!(vertical, "vertical movement required");
        }
        if room >= 4 {
            assert!(depth, "depth movement required");
        }
        assert!(sim.won());
        sim.step(&Intent { next: true, ..Default::default() });
    }
    assert!(sim.finished());
    assert_eq!(sim.state.completed, 255);
}
#[test]
fn handoff_keeps_pin_stationary_then_changes_it() {
    let mut s = Sim::new(1);
    let p = s.state.pose;
    let i = p.directions().iter().position(|d| *d == [0, 1, 0]).unwrap();
    s.step(&Intent { select: Some(i), commit: true, ..Default::default() });
    for _ in 0..SWING_TICKS - 1 {
        s.step(&Intent::default());
        assert_eq!(s.state.pose, p);
    }
    s.step(&Intent::default());
    let n = s.state.pose;
    assert_eq!(n.tips[0], p.tips[0]);
    assert_eq!(n.tips[1], [-1, 2, 0]);
    assert_eq!(n.pin, 1);
    assert!(n.directions().iter().all(|d| d[1] == 0), "no 180 or translation choice");
    let before = s.state.pose;
    commit(&mut s, 0);
    assert_eq!(s.state.pose.tips[1], before.tips[1]);
    assert_eq!(s.state.pose.pin, 0);
}
#[test]
fn clear_endpoints_can_have_a_blocked_swept_arc() {
    let c = chamber(2);
    let p = c.start;
    let up = p.directions().iter().position(|d| *d == [0, 1, 0]).unwrap();
    assert!(pose_clear(&c, p));
    assert!(pose_clear(&c, p.destination(up)));
    let hit = obstruction(&c, p, up).unwrap();
    assert_eq!(hit.block, Some(0));
    assert!(hit.fraction > 0. && hit.fraction < 1.);
}
#[test]
fn blocked_request_does_not_move_or_change_pin_and_retry_is_unlimited() {
    let mut s = Sim::new(1);
    let p = s.state.pose;
    let down = p.directions().iter().position(|d| *d == [0, -1, 0]).unwrap();
    // Initial tutorial downward swing is legal; first swing down to the floor, then into its bound.
    commit(&mut s, down);
    let p = s.state.pose;
    let bad = (0..4).find(|i| obstruction(s.room(), p, *i).is_some()).unwrap();
    for _ in 0..20 {
        commit(&mut s, bad);
        assert_eq!(s.state.pose, p);
        assert!(s.state.swing.is_none());
    }
    s.step(&Intent { undo: true, ..Default::default() });
    assert_eq!(s.state.pose, chamber(0).start);
    commit(&mut s, 0);
    s.step(&Intent { restart: true, ..Default::default() });
    assert_eq!(s.state.pose, chamber(0).start);
    assert!(s.state.history.is_empty());
}
#[test]
fn an_undocked_dead_end_has_no_legal_outgoing_swing() {
    let mut c = chamber(0);
    let p = c.start;
    let pin = world(p.tips[p.pin]);
    // A small free-tip pocket: each quarter turn hits a surrounding block while the rest pose fits.
    c.blocks = vec![
        Block::new([-2.3, 2.3, -2.3], [0.3, 2.7, 2.3]),
        Block::new([-2.3, 1.3, -2.3], [0.3, 1.7, 2.3]),
        Block::new([-2.3, 1.7, 0.3], [0.3, 2.3, 0.7]),
        Block::new([-2.3, 1.7, -0.7], [0.3, 2.3, -0.3]),
    ];
    assert_eq!(pin, [-2., 2., 0.]);
    assert!(pose_clear(&c, p));
    assert!(!docked(&c, p));
    assert_eq!(legal_choices(&c, p), [false; 4]);
}
#[test]
fn undo_cancels_in_flight_and_rewinds_completed_handoffs() {
    let mut s = Sim::new(1);
    let initial = s.state.pose;
    s.step(&Intent { select: Some(2), commit: true, ..Default::default() });
    s.step(&Intent { undo: true, ..Default::default() });
    assert_eq!(s.state.pose, initial);
    assert!(s.state.history.is_empty());
    assert!(s.state.swing.is_none());
    commit(&mut s, 2);
    assert_ne!(s.state.pose, initial);
    s.step(&Intent { undo: true, ..Default::default() });
    assert_eq!(s.state.pose, initial);
}
#[test]
fn swapped_colours_are_not_a_win() {
    let c = chamber(0);
    let p = Pose { tips: [c.sockets[1], c.sockets[0]], pin: 0 };
    assert!(!docked(&c, p));
}
#[test]
fn deterministic_all_chambers_and_save_load_mid_swing() {
    let inputs = script();
    assert_deterministic(|| Sim::new(7), &inputs);
    snapshot::assert_resumes_as_promised(|| Sim::new(7), &inputs, 37);
}
#[test]
fn invalid_load_is_atomic() {
    let mut s = Sim::new(1);
    commit(&mut s, 2);
    let h = s.state_hash();
    let mut state = s.capture();
    state.pose.pin = 9;
    assert!(s.restore(state).is_err());
    assert_eq!(s.state_hash(), h);
}
#[test]
fn engine_snapshot_preserves_undo_history() {
    let mut s = Sim::new(1);
    commit(&mut s, 2);
    let bytes = snapshot::save(&s, "handoff").unwrap();
    let expected = s.state_hash();
    commit(&mut s, 0);
    snapshot::restore(&mut s, &bytes).unwrap();
    assert_eq!(s.state_hash(), expected);
    s.step(&Intent { undo: true, ..Default::default() });
    assert_eq!(s.state.pose, chamber(0).start);
}

#[test]
fn authored_dead_ends_can_be_retried_with_public_undo() {
    use std::collections::{HashMap, VecDeque};
    let mut found = None;
    for room in 0..ROOM_COUNT {
        let c = chamber(room);
        let mut q = VecDeque::from([c.start]);
        let mut prev = HashMap::new();
        prev.insert(c.start, None);
        while let Some(p) = q.pop_front() {
            if docked(&c, p) {
                continue;
            }
            let legal = legal_choices(&c, p);
            if legal.iter().all(|x| !*x) {
                let mut route = vec![];
                let mut at = p;
                while let Some((parent, i)) = prev[&at] {
                    route.push(i);
                    at = parent;
                }
                route.reverse();
                found = Some((room, route));
                break;
            }
            for (i, ok) in legal.iter().enumerate() {
                if !ok {
                    continue;
                }
                let n = p.destination(i);
                if let std::collections::hash_map::Entry::Vacant(e) = prev.entry(n) {
                    e.insert(Some((p, i)));
                    q.push_back(n);
                }
            }
        }
        if found.is_some() {
            break;
        }
    }
    let (room, path) = found.expect("an authored route should demonstrate the stated dead-end retry loop");
    println!("dead end in room {} after {:?}", room + 1, path.iter().map(|i| i + 1).collect::<Vec<_>>());
    let mut s = Sim::new(7);
    while s.state.room < room {
        for i in solve(s.room(), s.state.pose).unwrap() {
            commit(&mut s, i);
        }
        s.step(&Intent { next: true, ..Default::default() });
    }
    for i in path {
        commit(&mut s, i);
    }
    assert!(s.trapped());
    assert!(!s.won());
    s.step(&Intent { undo: true, ..Default::default() });
    assert!(!s.trapped());
}

#[test]
fn verification_input_finishes_through_public_gameplay() {
    let mut s = Sim::new(0);
    for _ in 0..3000 {
        let input = verification_input(&s);
        s.step(&input);
        if s.finished() {
            break;
        }
    }
    assert!(s.finished());
    assert_eq!(s.state.completed, 255);
}
