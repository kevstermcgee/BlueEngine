use odd_matter::*;
use vesper3d::viewer::devkit::{assert_deterministic, snapshot, Simulation, Snapshot};

fn act(sim: &mut Sim, route: &str) {
    for c in route.chars() {
        sim.step(&command(c));
    }
}
fn room(n: usize) -> Sim {
    let mut s = Sim::new(7);
    for route in SOLUTIONS.iter().take(n) {
        act(&mut s, route);
        assert_eq!(s.board.status, Status::Cleared);
        s.step(&command('>'));
    }
    s
}
#[test]
fn every_chamber_is_winnable_by_public_input_using_real_depth_and_height() {
    let mut s = Sim::new(7);
    let mut axes = [false; 3];
    let mut successes = 0;
    for i in verification_route() {
        let before = s.board.clone();
        s.step(&i);
        if before.drone != s.board.drone && before.room == s.board.room {
            for (a, used) in axes.iter_mut().enumerate() {
                *used |= before.drone[a] != s.board.drone[a];
            }
        }
        for event in s.drain_events() {
            assert!(
                !matches!(event, Event::Blocked(_) | Event::Crushed),
                "authored route failed in {}: {event:?}",
                s.board.room
            );
            if matches!(event, Event::Cleared | Event::Won) {
                successes += 1;
            }
        }
    }
    assert_eq!(s.board.status, Status::Won);
    assert_eq!(successes, 6);
    assert_eq!(axes, [true; 3]);
}
#[test]
fn parity_cancels_two_layers_restores_three_and_opens_four() {
    let mut s = room(3);
    let c = s.room();
    let junction = [2, 1, 2];
    assert_eq!(c.layers(&s.board.offsets, junction), 3);
    assert!(c.solid(&s.board.offsets, junction));
    act(&mut s, "4QQ");
    assert_eq!(c.layers(&s.board.offsets, junction), 4);
    assert!(!c.solid(&s.board.offsets, junction));
    assert_eq!(c.layers(&s.board.offsets, [1, 1, 2]), 2);
    assert!(!c.solid(&s.board.offsets, [1, 1, 2]));
    assert_eq!(c.layers(&s.board.offsets, [1, 2, 2]), 1);
    assert!(c.solid(&s.board.offsets, [1, 2, 2]));
}
#[test]
fn closure_crushes_a_stationary_drone_and_undo_restores_the_entire_action() {
    let mut s = Sim::new(7);
    act(&mut s, "2E1DD2");
    let before = s.board.clone();
    let f = s.forecast(Direction::Up);
    assert!(f.crush);
    assert!(f.closed.contains(&s.board.drone));
    s.step(&command('E'));
    assert_eq!(s.board.status, Status::Crushed);
    assert_eq!(s.board.drone, before.drone);
    let crushed = s.board.clone();
    s.step(&command('D'));
    assert_eq!(s.board, crushed);
    s.step(&command('Z'));
    assert_eq!(s.board, before);
    act(&mut s, "1DDEE");
    assert_eq!(s.board.status, Status::Cleared);
}
#[test]
fn parked_drone_survives_a_passage_relocation_and_old_cavity_closes() {
    let mut s = room(4);
    act(&mut s, "1DDDD2");
    let parked = s.board.drone;
    let old = [2, 1, 1];
    let new = [2, 1, 3];
    assert!(!s.room().solid(&s.board.offsets, old));
    act(&mut s, "SS");
    assert_eq!(s.board.drone, parked);
    assert_eq!(s.board.status, Status::Playing);
    assert!(s.room().solid(&s.board.offsets, old));
    assert!(s.room().solid(&s.board.offsets, new), "two intersecting bars restore a plug");
    act(&mut s, "4QQQ");
    assert!(!s.room().solid(&s.board.offsets, new), "third bar reopens the plug");
}
#[test]
fn blocked_moves_wrong_rails_and_the_frame_do_not_consume_undo_or_change_matter() {
    let mut s = Sim::new(7);
    let at = s.board.drone;
    act(&mut s, "DA");
    assert_eq!(s.board.drone, at);
    assert_eq!(s.undo_len(), 0);
    act(&mut s, "2D");
    assert_eq!(s.board.offsets, [0, 0, 0]);
    assert_eq!(s.undo_len(), 0);
    act(&mut s, "Q");
    assert_eq!(s.board.offsets, [0, 0, 0]);
    assert!(s.room().solid(&s.board.offsets, [-1, 1, 2]));
    assert!(s.drain_events().contains(&Event::Blocked(Block::Rail)));
}
#[test]
fn forecast_agrees_with_actual_occupancy_and_does_not_mutate_state() {
    let mut s = room(5);
    act(&mut s, "4");
    let before = s.state_hash();
    let f = s.forecast(Direction::Up);
    assert_eq!(before, s.state_hash());
    let old = s.board.offsets;
    s.step(&command('E'));
    for p in cells() {
        assert_eq!(f.opened.contains(&p), s.room().solid(&old, p) && !s.room().solid(&s.board.offsets, p));
        assert_eq!(f.closed.contains(&p), !s.room().solid(&old, p) && s.room().solid(&s.board.offsets, p));
    }
}
#[test]
fn restart_stays_in_this_chamber_and_undo_can_reopen_a_cleared_chamber() {
    let mut s = room(2);
    act(&mut s, SOLUTIONS[2]);
    assert_eq!(s.board.status, Status::Cleared);
    s.step(&command('Z'));
    assert_eq!(s.board.status, Status::Playing);
    s.step(&command('R'));
    assert_eq!(s.board.room, 2);
    assert_eq!(s.board.drone, chamber(2).spawn);
    assert_eq!(s.board.moves, 0);
    assert_eq!(s.undo_len(), 0);
}
#[test]
fn full_campaign_and_crush_retry_are_deterministic() {
    assert_deterministic(|| Sim::new(7), &verification_route());
    let route: Vec<_> = "2E1DD2EZR2E1DDDDEE>".chars().map(command).collect();
    assert_deterministic(|| Sim::new(7), &route);
}
#[test]
fn timed_verification_input_really_completes_the_campaign() {
    let mut s = Sim::new(7);
    for t in 0..verification_route().len() as u64 * 12 {
        s.step(&verification_input(t));
    }
    assert_eq!(s.board.status, Status::Won);
}
#[test]
fn exact_engine_saves_resume_campaign_and_the_saved_undo_trail() {
    snapshot::assert_resumes_as_promised(|| Sim::new(7), &verification_route(), 3);
    let input: Vec<_> = "2E1DD2EZR2E1DDDDEE>Z>".chars().map(command).collect();
    snapshot::assert_resumes_as_promised(|| Sim::new(7), &input, 1);
    let mut s = Sim::new(7);
    act(&mut s, "2E1DD2E");
    let bytes = snapshot::save(&s, "Crushed attempt").unwrap();
    let mut restored = Sim::new(99);
    snapshot::restore(&mut restored, &bytes).unwrap();
    restored.step(&command('Z'));
    s.step(&command('Z'));
    assert_eq!(s.state_hash(), restored.state_hash());
    assert_eq!(restored.board.status, Status::Playing);
}
#[test]
fn corrupt_or_inconsistent_saves_are_refused_without_touching_the_game() {
    let mut s = Sim::new(7);
    act(&mut s, "2E1DD");
    let before = s.state_hash();
    let bytes = snapshot::save(&s, "Good").unwrap();
    let mut bad = bytes.clone();
    let n = bad.len() / 2;
    bad[n] ^= 0x20;
    assert!(snapshot::restore(&mut s, &bad).is_err());
    assert_eq!(s.state_hash(), before);
    let mut state = s.capture();
    state.board.offsets[0] = 5;
    assert!(s.restore(state).is_err());
    assert_eq!(s.state_hash(), before);
    let mut state = s.capture();
    state.board.status = Status::Crushed;
    assert!(s.restore(state).is_err());
    assert_eq!(s.state_hash(), before);
}
