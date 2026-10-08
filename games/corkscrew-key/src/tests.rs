use super::*;
use std::collections::{HashMap, VecDeque};
use vesper3d::runtime::snapshot;
fn execute(g: &mut Corkscrew, axis: usize, sign: i32) {
    g.step(&command(axis as i32 + 1));
    g.step(&command(if sign > 0 { 4 } else { 5 }));
    g.step(&command(6));
    for _ in 0..TURN_TICKS {
        g.step(&Intent::default());
    }
}
#[test]
fn screw_turns_and_advances_together_and_inverse_restores_pose() {
    for axis in 0..3 {
        for sign in [-1, 1] {
            let p = Pose::default();
            let q = p.screw(axis, sign);
            assert_eq!(q.center[axis], sign);
            assert_ne!(q.basis, p.basis);
            assert_eq!(q.screw(axis, -sign), p);
            assert_eq!(q.cells().len(), 5);
            assert!(q.valid());
        }
    }
    let mut seen = std::collections::HashSet::new();
    let mut queue = vec![Pose::default()];
    while let Some(p) = queue.pop() {
        if !seen.insert(p.basis) {
            continue;
        }
        for a in 0..3 {
            let mut q = p.screw(a, 1);
            q.center = [0; 3];
            queue.push(q);
        }
    }
    assert_eq!(seen.len(), 24);
}
#[test]
fn order_matters_and_closed_loop_reorients_without_translation() {
    let p = Pose::default();
    assert_ne!(p.screw(0, 1).screw(1, 1), p.screw(1, 1).screw(0, 1));
    let q = goal(4);
    assert_eq!(p.center, q.center);
    assert_ne!(p.basis, q.basis);
}
#[test]
fn selecting_previews_does_not_move_and_click_commands_share_keyboard_rules() {
    let mut game = Corkscrew::new(7);
    let pose = game.state.pose;
    for code in [3, 5, 2, 4, 1] {
        game.step(&command(code));
        assert_eq!(game.state.pose, pose);
        assert_eq!(game.moves(), 0);
    }
    game.step(&Intent {
        action: true,
        pointer: Some(Point::new(50, 180)),
        ..Default::default()
    });
    assert_eq!(game.state.pose, pose.screw(0, 1));
    assert_eq!(game.moves(), 1);
    // An ordinary idle tick cannot repeat a press; undo can interrupt animation.
    game.step(&Intent::default());
    assert_eq!(game.moves(), 1);
    game.step(&command(7));
    assert_eq!(game.state.pose, pose);
    assert_eq!(game.state.turning, 0);
    assert!(!game.state.docked);
    assert_eq!(button_at(Point::new(244, 86)), Some(3));
}
#[test]
fn all_six_authored_routes_are_clear_and_win_through_public_input() {
    for (n, route) in ROUTES.iter().enumerate() {
        let mut p = Pose::default();
        assert!(pose_clear(p, &blocks(n)), "start {n}");
        for &(a, s) in *route {
            assert_eq!(
                sweep(p, a, s, &blocks(n)),
                None,
                "chamber {n}, pose {p:?}, step {a} {s}"
            );
            p = p.screw(a, s);
        }
        assert_eq!(p, goal(n));
    }
    let mut g = Corkscrew::new(7);
    for tick in 0..Corkscrew::VERIFY_TICKS {
        g.step(&Corkscrew::verification_input(tick));
    }
    assert_eq!(g.outcome(), "won");
    assert_eq!(g.state.chamber, 5);
    let (hash, outcome) = verify::<Corkscrew>();
    assert_eq!(outcome, "won");
    if let Ok(path) = std::env::var("BE2_VERIFY_REPORT") {
        std::fs::write(
            path,
            serde_json::json!({"hash":format!("{hash:016x}"),"outcome":outcome}).to_string(),
        )
        .unwrap();
    }
}
#[test]
fn swept_collision_rejects_a_clear_destination_without_changing_pose_or_history() {
    // Cube arm crosses this pillar between two clear endpoint poses.
    let p = Pose::default();
    let block = Block::new([1, 1, 1], [1, 1, 1]);
    assert!(pose_clear(p, &[block]));
    assert!(pose_clear(p.screw(1, 1), &[block]));
    assert!(sweep(p, 1, 1, &[block]).is_some());
    let mut g = Corkscrew::new(7);
    g.load_chamber(4);
    let pose = g.state.pose;
    execute(&mut g, 0, -1);
    assert_eq!(g.state.pose, pose);
    assert_eq!(g.moves(), 0);
    assert_eq!(g.state.rejected, 1);
    assert_eq!(g.outcome(), "playing");
    execute(&mut g, 0, 1);
    assert_ne!(g.state.pose, pose);
    g.step(&command(7));
    assert_eq!(g.state.pose, pose);
    execute(&mut g, 0, 1);
    g.restart();
    assert_eq!(g.state.chamber, 4);
    assert_eq!(g.state.pose, pose);
    assert_eq!(g.moves(), 0);
}
#[test]
fn deterministic_and_snapshot_continuation_including_mid_turn_and_undo() {
    let mut inputs: Vec<Intent> = (0..Corkscrew::VERIFY_TICKS)
        .map(Corkscrew::verification_input)
        .collect();
    inputs[15] = command(8);
    vesper3d::runtime::assert_deterministic(|| Corkscrew::new(7), &inputs);
    for at in [4, 19, 75, 123, 360] {
        snapshot::assert_resumes_exactly(|| Corkscrew::new(7), &inputs, at);
    }
    let mut g = Corkscrew::new(7);
    execute(&mut g, 0, 1);
    let saved = g.capture();
    let mut loaded = Corkscrew::new(99);
    loaded.restore(saved).unwrap();
    g.step(&command(7));
    loaded.step(&command(7));
    assert_eq!(g.state_hash(), loaded.state_hash());
    let before = loaded.state_hash();
    let mut bad = loaded.capture();
    bad.pose.basis[0] = [1, 1, 0];
    assert!(loaded.restore(bad).is_err());
    assert_eq!(before, loaded.state_hash());
}
fn search(n: usize, allow_y: bool) -> Option<Vec<(usize, i32)>> {
    let start = Pose::default();
    let target = goal(n);
    let obstacles = blocks(n);
    let mut queue = VecDeque::from([start]);
    let mut parents = HashMap::from([(start, None)]);
    while let Some(p) = queue.pop_front() {
        if p == target {
            let mut path = vec![];
            let mut q = p;
            while let Some((prev, a, s)) = parents[&q] {
                path.push((a, s));
                q = prev;
            }
            path.reverse();
            return Some(path);
        }
        for a in 0..3 {
            if a == 1 && !allow_y {
                continue;
            }
            for s in [-1, 1] {
                let next = p.screw(a, s);
                if parents.contains_key(&next) || !next.valid() || !pose_clear(next, &obstacles) {
                    continue;
                }
                if sweep(p, a, s, &obstacles).is_none() {
                    parents.insert(next, Some((p, a, s)));
                    queue.push_back(next);
                }
            }
        }
    }
    None
}
#[test]
fn state_space_search_proves_solutions_and_final_vertical_requirement() {
    for n in 0..6 {
        let route = search(n, true).expect("solvable chamber");
        println!("chamber {} shortest: {:?}", n + 1, route);
        assert!(!route.is_empty());
    }
    assert!(
        search(5, false).is_none(),
        "final must require real vertical movement"
    );
}
