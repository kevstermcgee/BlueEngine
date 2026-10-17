use super::*;
use vesper3d::runtime::{assert_deterministic, snapshot};
fn at(level: usize) -> Peelturn {
    let mut g = Peelturn::new(7);
    g.begin_level(level);
    g
}
fn launch(g: &mut Peelturn, packet: usize, dir: usize) {
    let p = packet_center(g.level().packets[packet].cell);
    g.step(&Intent {
        pointer: Some(p),
        action: true,
        ..Intent::default()
    });
    g.step(&Peelturn::aim(dir));
    g.step(&Intent {
        action: true,
        ..Intent::default()
    });
    for _ in 0..500 {
        if g.state.board.flight.is_none() || g.state.board.result != ResultState::Playing {
            return;
        }
        g.step(&Intent::default());
    }
    panic!("a launch did not finish");
}
#[test]
fn clockwise_deposition_and_self_stopper() {
    let mut g = at(1);
    launch(&mut g, 0, 0);
    assert_eq!(g.state.board.result, ResultState::Clear);
    let cells: Vec<_> = g.state.board.deposits.iter().map(|d| d.cell).collect();
    assert_eq!(cells, vec![[5, 2], [5, 5], [0, 5], [0, 0], [5, 0], [5, 1]]);
    assert_eq!(
        adjacent(cells[5], 1),
        cells[0],
        "sixth impact meets first bead"
    );
    assert_eq!(g.state.board.filled(g.level()), 2);
    assert!(g.state.board.alive.iter().all(|a| !a));
}
#[test]
fn initially_blocked_launch_rejects_without_spending_a_bead() {
    let mut g = at(2);
    let before = g.state.board.clone();
    launch(&mut g, 0, 1); // A is on the bottom edge.
    let b = &g.state.board;
    assert_eq!(b.alive, before.alive);
    assert_eq!(b.deposits, before.deposits);
    assert_eq!(b.launches, 0);
    assert!(g.state.history.is_empty());
    assert_eq!(b.result, ResultState::Playing);
    assert!(b.rejected > 0);
}
#[test]
fn jam_is_a_loss_and_undo_restores_one_whole_launch() {
    let mut g = at(2);
    let before = g.state.board.clone();
    launch(&mut g, 0, 0);
    assert_eq!(g.state.board.result, ResultState::Jammed);
    assert_eq!(g.outcome(), "lost");
    assert_eq!(g.state.board.deposits.len(), 1);
    assert_eq!(g.state.board.flight.as_ref().unwrap().left, 3);
    let lost = g.state.board.clone();
    g.step(&Intent {
        action: true,
        ..Intent::default()
    });
    assert_eq!(g.state.board, lost);
    g.step(&Peelturn::command(UNDO));
    assert_eq!(g.state.board, before);
    assert_eq!(g.outcome(), "playing");
    // Undo during a slide is also the entire launch, not one impact.
    g.step(&Intent {
        action: true,
        ..Intent::default()
    });
    g.step(&Intent::default());
    g.step(&Peelturn::command(UNDO));
    assert_eq!(g.state.board, before);
    launch(&mut g, 0, 0);
    g.restart();
    assert_eq!(g.state.level, 2);
    assert_eq!(g.state.board, before);
}
#[test]
fn exhaustion_and_trapped_packets_fail_with_unlimited_recovery() {
    let mut g = at(0);
    launch(&mut g, 0, 3);
    assert_eq!(g.state.board.result, ResultState::EmptySockets);
    assert_eq!(g.outcome(), "lost");
    for _ in 0..5 {
        g.step(&Peelturn::command(UNDO));
        launch(&mut g, 0, 3);
    }
    g.step(&Peelturn::command(UNDO));
    launch(&mut g, 0, 0);
    assert_eq!(g.state.board.result, ResultState::Clear);
    let mut g = at(8);
    launch(&mut g, 0, 0);
    launch(&mut g, 2, 3);
    assert_eq!(g.state.board.result, ResultState::Trapped);
    assert_eq!(g.outcome(), "lost");
    g.step(&Peelturn::command(UNDO));
    assert_eq!(g.outcome(), "playing");
}
fn solutions(g: Peelturn, sequence: &mut Vec<[usize; 2]>, wins: &mut Vec<Vec<[usize; 2]>>) {
    if g.state.board.result == ResultState::Clear {
        wins.push(sequence.clone());
        return;
    }
    if g.outcome() != "playing" {
        return;
    }
    for p in 0..g.level().packets.len() {
        if !g.state.board.alive[p] {
            continue;
        }
        for d in 0..4 {
            let mut next = Peelturn::new(7);
            next.restore(g.capture()).unwrap();
            let n = next.state.board.launches;
            launch(&mut next, p, d);
            if next.state.board.launches == n {
                continue;
            }
            sequence.push([p, d]);
            solutions(next, sequence, wins);
            sequence.pop();
        }
    }
}
#[test]
fn interacting_packets_require_order_and_exploit_a_deposited_stopper() {
    let mut wins = vec![];
    solutions(at(2), &mut vec![], &mut wins);
    assert_eq!(wins, vec![vec![[1, 0], [0, 2]]]);
    let mut g = at(2);
    launch(&mut g, 1, 0);
    assert!(!g.state.board.alive[1]);
    assert_eq!(g.state.board.deposits[0].cell, [5, 3]);
    launch(&mut g, 0, 2);
    assert_eq!(g.state.board.result, ResultState::Clear);
    assert_eq!(g.state.board.deposits.last().unwrap().cell, [5, 2]);
    assert!(
        g.state.board.deposits.iter().any(|d| d.cell == [5, 3]),
        "B's bead stops A's last descent"
    );
}
#[test]
fn every_authored_board_is_solvable_through_public_gameplay() {
    let mut g = Peelturn::new(7);
    let mut passed_empty_socket = false;
    for (index, l) in levels().iter().enumerate() {
        assert_eq!(g.state.level, index);
        assert!(!l.sockets.is_empty());
        for [p, d] in &l.solution {
            g.step(&Intent {
                pointer: Some(packet_center(l.packets[*p].cell)),
                action: true,
                ..Intent::default()
            });
            g.step(&Peelturn::aim(*d));
            g.step(&Intent {
                action: true,
                ..Intent::default()
            });
            for _ in 0..500 {
                if let Some(f) = &g.state.board.flight {
                    if l.sockets.contains(&f.cell)
                        && !g.state.board.deposits.iter().any(|b| b.cell == f.cell)
                    {
                        passed_empty_socket = true;
                        assert!(
                            g.state.board.filled(l) < l.sockets.len(),
                            "passing does not fill sockets"
                        );
                    }
                }
                if g.state.board.flight.is_none() || g.outcome() == "lost" {
                    break;
                }
                g.step(&Intent::default());
            }
            assert_ne!(g.outcome(), "lost", "board {}", index + 1);
        }
        assert_eq!(
            g.state.board.result,
            ResultState::Clear,
            "board {}",
            index + 1
        );
        if index + 1 < levels().len() {
            g.step(&Intent {
                action: true,
                ..Intent::default()
            });
        }
    }
    assert!(passed_empty_socket);
    assert_eq!(g.outcome(), "won");
}
#[test]
fn verification_determinism_and_engine_save_frame_resume_exactly() {
    let inputs: Vec<_> = (0..Peelturn::VERIFY_TICKS)
        .map(Peelturn::verification_input)
        .collect();
    assert_deterministic(|| Peelturn::new(7), &inputs);
    snapshot::assert_resumes_exactly(|| Peelturn::new(7), &inputs[..1800], 137);
    let (hash, outcome) = verify::<Peelturn>();
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
fn save_mid_impact_keeps_undo_and_rejects_invalid_state_atomically() {
    let mut g = at(2);
    launch(&mut g, 1, 0);
    g.step(&Peelturn::aim(2));
    g.step(&Intent {
        action: true,
        ..Intent::default()
    });
    for _ in 0..30 {
        g.step(&Intent::default());
    }
    let mut restored = Peelturn::new(7);
    restored.restore(g.capture()).unwrap();
    assert_eq!(g.state_hash(), restored.state_hash());
    for _ in 0..200 {
        g.step(&Intent::default());
        restored.step(&Intent::default());
        assert_eq!(g.state_hash(), restored.state_hash());
    }
    g.step(&Peelturn::command(UNDO));
    restored.step(&Peelturn::command(UNDO));
    assert_eq!(g.state_hash(), restored.state_hash());
    let hash = g.state_hash();
    let mut invalid = g.capture();
    invalid.board.dir = 99;
    assert!(g.restore(invalid).is_err());
    assert_eq!(g.state_hash(), hash);
    let mut invalid = g.capture();
    invalid.board.deposits.push(Deposit {
        cell: [999, 0],
        packet: 0,
        impact: 1,
    });
    assert!(g.restore(invalid).is_err());
    assert_eq!(g.state_hash(), hash);
}
#[test]
fn native_probe_and_packet_cycle_are_real_intentions() {
    let mut g = Peelturn::new(7);
    g.step(&Peelturn::probe_input());
    assert!(g.probe_success());
    let mut g = at(2);
    assert_eq!(g.state.board.selected, 0);
    g.step(&Peelturn::command(CYCLE));
    assert_eq!(g.state.board.selected, 1);
    g.step(&Peelturn::command(CYCLE));
    assert_eq!(g.state.board.selected, 0);
}

#[test]
fn departure_opens_the_start_cell_and_eight_beads_can_return_through_it() {
    let mut g = at(9);
    let start = g.level().packets[0].cell;
    launch(&mut g, 0, 3);
    assert_eq!(g.state.board.result, ResultState::Playing);
    assert_eq!(g.state.board.deposits.len(), 8);
    assert_eq!(g.state.board.deposits[3].cell, start);
    assert!(!g.state.board.alive[0]);
}
