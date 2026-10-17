use crosshaul::{chambers, preview, Intent, Obstruction, Sim, TRAVEL};
use vesper3d::viewer::devkit::{assert_deterministic, snapshot, Simulation, Snapshot};
fn send(sim: &mut Sim, i: Intent, log: &mut Vec<Intent>) {
    sim.step(&i);
    log.push(i);
}
fn wait(sim: &mut Sim, log: &mut Vec<Intent>) {
    for _ in 0..TRAVEL {
        send(sim, Intent::default(), log);
    }
}
fn exchange(sim: &mut Sim, a: usize, b: usize, k: usize, log: &mut Vec<Intent>) {
    for old in sim.pair.clone() {
        send(sim, Intent { select: Some(old as u8), ..Default::default() }, log);
    }
    send(sim, Intent { select: Some(a as u8), ..Default::default() }, log);
    send(sim, Intent { select: Some(b as u8), axis: Some(k as u8), ..Default::default() }, log);
    send(sim, Intent { execute: true, ..Default::default() }, log);
    wait(sim, log);
}
fn full_game() -> Vec<Intent> {
    let mut sim = Sim::new(0);
    let mut log = vec![];
    for c in chambers() {
        for [a, b, k] in &c.solution {
            exchange(&mut sim, *a, *b, *k, &mut log);
        }
        assert!(sim.won(), "{}", c.name);
        send(&mut sim, Intent { next: true, ..Default::default() }, &mut log);
    }
    log
}
#[test]
fn exchange_changes_only_one_coordinate_and_preserves_the_inventory() {
    let mut s = Sim::new(1);
    let before = s.positions.clone();
    let mut l = vec![];
    exchange(&mut s, 0, 1, 1, &mut l);
    assert_eq!(s.positions[0], [before[0][0], before[1][1], before[0][2]]);
    assert_eq!(s.positions[1], [before[1][0], before[0][1], before[1][2]]);
    assert_eq!(s.positions[2], before[2]);
    assert!(s.won());
    assert_eq!(s.used, 1);
}
#[test]
fn both_sweeps_reject_baffles_cargo_destinations_and_the_moving_partner() {
    let mut c = chambers()[0].clone();
    c.baffles = vec![[4, 1, 4], [4, 2, 4]];
    let pos = vec![[0, 4, 0], [4, 0, 4], [2, 2, 2]];
    let p = preview(&c, &pos, 0, 1, 1);
    assert!(!p.legal());
    assert!(p.hits[0].is_none());
    assert_eq!(p.hits[1].unwrap().at, [4, 1, 4], "first obstruction from B, not the farther baffle");
    c.baffles.clear();
    let pos = vec![[0, 4, 0], [4, 0, 4], [0, 2, 0]];
    assert_eq!(preview(&c, &pos, 0, 1, 1).hits[0].unwrap().kind, Obstruction::Cargo(2));
    let pos = vec![[0, 0, 0], [4, 4, 4], [0, 4, 0]];
    assert!(!preview(&c, &pos, 0, 1, 1).legal(), "occupied destination is blocked");
    let pos = vec![[0, 0, 0], [0, 4, 0], [2, 2, 2]];
    let p = preview(&c, &pos, 0, 1, 1);
    assert_eq!(p.hits[0].unwrap().kind, Obstruction::Partner);
    assert!(!p.legal());
}
#[test]
fn rejected_commands_and_noops_cost_nothing() {
    let mut s = Sim::at(2);
    let mut l = vec![];
    // A's vertical path at x=0,z=4 is clear, B's x=4,z=0 is clear; Z hits a baffle for B.
    let mut blocked = None;
    for a in 0..3 {
        for b in a + 1..3 {
            for k in 0..3 {
                if !preview(s.level(), &s.positions, a, b, k).legal() {
                    blocked = Some((a, b, k));
                }
            }
        }
    }
    let (a, b, k) = blocked.unwrap();
    let before = s.positions.clone();
    exchange(&mut s, a, b, k, &mut l);
    assert_eq!(s.positions, before);
    assert_eq!(s.used, 0);
    assert!(s.history.is_empty());
    assert!(s.rejected > 0);
    let mut c = chambers()[0].clone();
    c.baffles.clear();
    assert!(!preview(&c, &[[0, 0, 0], [4, 0, 4], [2, 4, 2]], 0, 1, 1).legal());
}
#[test]
fn allowance_failure_undo_refund_and_restart_are_real_public_inputs() {
    let mut s = Sim::at(1);
    let mut l = vec![];
    let [a, b, k] = s.level().solution[0];
    for _ in 0..s.level().allowance {
        exchange(&mut s, a, b, k, &mut l);
    }
    assert!(s.failed());
    let positions = s.positions.clone();
    send(&mut s, Intent { execute: true, ..Default::default() }, &mut l);
    assert_eq!(s.positions, positions);
    send(&mut s, Intent { undo: true, ..Default::default() }, &mut l);
    assert!(!s.failed());
    assert_eq!(s.used, s.level().allowance - 1);
    send(&mut s, Intent { restart: true, ..Default::default() }, &mut l);
    assert_eq!(s.positions, s.level().start);
    assert_eq!(s.used, 0);
    assert!(s.history.is_empty());
}
#[test]
fn eight_chambers_are_solved_by_public_gameplay_and_later_ones_need_height_and_depth() {
    assert_eq!(chambers().len(), 8);
    let log = full_game();
    assert!(log.len() > 900);
    for c in &chambers()[1..] {
        for k in [1, 2] {
            assert!(c.start.iter().zip(&c.docks).any(|(a, b)| a[k] != b[k]), "{} requires {}", c.name, k);
        }
    }
}
#[test]
fn the_docked_third_cube_must_carry_a_coordinate() {
    let mut s = Sim::at(2);
    assert_eq!(s.positions[2], s.level().docks[2]);
    let mut l = vec![];
    let [a, b, k] = s.level().solution[0];
    exchange(&mut s, a, b, k, &mut l);
    assert_ne!(s.positions[2], s.level().docks[2], "temporarily dismantles the correct dock");
    for [a, b, k] in s.level().solution.iter().copied().skip(1) {
        exchange(&mut s, a, b, k, &mut l);
    }
    assert!(s.won());
    let c = &chambers()[2];
    let mut frontier = vec![c.start.clone()];
    for _ in 0..=c.allowance {
        assert!(!frontier.contains(&c.docks), "A and B alone cannot finish within allowance");
        let mut next = vec![];
        for p in frontier {
            for k in 0..3 {
                let v = preview(c, &p, 0, 1, k);
                if v.legal() {
                    let mut t = p.clone();
                    t[0] = v.destinations[0];
                    t[1] = v.destinations[1];
                    if !next.contains(&t) {
                        next.push(t);
                    }
                }
            }
        }
        frontier = next;
    }
}
#[test]
fn deterministic_and_exact_saves_resume_during_both_routes_and_across_all_chambers() {
    let inputs = full_game();
    assert_deterministic(|| Sim::new(7), &inputs);
    snapshot::assert_resumes_as_promised(|| Sim::new(7), &inputs, 13);
    let mut s = Sim::new(0);
    for i in &inputs[..10] {
        s.step(i);
    }
    let bytes = snapshot::save(&s, "in transit").unwrap();
    let hash = s.state_hash();
    let mut restored = Sim::new(2);
    snapshot::restore(&mut restored, &bytes).unwrap();
    assert_eq!(hash, restored.state_hash());
    restored.step(&Intent { undo: true, ..Default::default() });
    assert_eq!(restored.used, 0);
    assert_eq!(restored.positions, restored.level().start);
}
#[test]
fn malformed_save_rejection_is_atomic_and_cannot_invent_coordinates_or_allowance() {
    let mut s = Sim::new(1);
    let before = s.state_hash();
    let mut bad = s.capture();
    bad.positions[0][1] = 3;
    assert!(s.restore(bad).is_err());
    assert_eq!(s.state_hash(), before);
    let mut bad = s.capture();
    bad.used = 9;
    assert!(s.restore(bad).is_err());
    assert_eq!(s.state_hash(), before);
    let mut bad = s.capture();
    bad.pair = vec![1, 1];
    assert!(s.restore(bad).is_err());
}
#[test]
fn hints_use_the_same_collision_rules() {
    for n in 0..8 {
        let mut s = Sim::at(n);
        let mut l = vec![];
        for _ in 0..s.level().allowance {
            if s.won() {
                break;
            }
            let [a, b, k] = s.hint().expect("solvable");
            assert!(preview(s.level(), &s.positions, a, b, k).legal());
            exchange(&mut s, a, b, k, &mut l);
        }
        assert!(s.won());
    }
}
#[test]
fn verification_input_is_public_gameplay_and_reaches_the_final_goal() {
    let log: Vec<Intent> = serde_json::from_str(include_str!("../assets/verification_input.json")).unwrap();
    let mut s = Sim::new(0);
    for i in log {
        s.step(&i);
    }
    assert_eq!(s.chamber, 7);
    assert!(s.won());
}
