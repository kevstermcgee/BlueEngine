use scatterlock::{chamber, point, sweep, Intent, Pulse, Sim, HALF};
use vesper3d::viewer::devkit::{assert_deterministic, snapshot, Simulation, Snapshot};
fn action(b: usize, m: Pulse) -> Intent {
    Intent { beacon: Some(b), mode: Some(m), execute: true, ..Default::default() }
}
fn script() -> Vec<Intent> {
    vec![
        action(0, Pulse::Spread),
        Intent { undo: true, ..Default::default() },
        action(0, Pulse::Spread),
        action(1, Pulse::Gather),
        Intent { next: true, ..Default::default() },
        action(0, Pulse::Gather),
        action(1, Pulse::Spread),
    ]
}
#[test]
fn all_six_chambers_have_public_input_solutions() {
    for n in 0..6 {
        let mut s = Sim::at_level(n);
        for (b, m) in chamber(n).solution {
            let p = action(b, m);
            s.step(&Intent { beacon: p.beacon, mode: p.mode, ..Default::default() });
            assert!(s.preview().obstruction.is_none(), "chamber {n}: {:?}", s.preview().obstruction);
            s.step(&Intent { execute: true, ..Default::default() });
        }
        assert!(s.won(), "chamber {n}");
    }
}
#[test]
fn scale_changes_every_coordinate_but_not_cube_size() {
    let mut s = Sim::new(1);
    let old = s.state.config.positions.clone();
    s.step(&action(0, Pulse::Spread));
    for (i, p) in old.iter().enumerate() {
        let a = chamber(0).beacons[0];
        assert_eq!(s.state.config.positions[i], std::array::from_fn(|j| a[j] + 2 * (p[j] - a[j])));
    }
    assert_eq!(HALF, 384);
    s.step(&action(1, Pulse::Gather));
    for (i, p) in old.iter().enumerate() {
        assert_eq!(s.state.config.positions[i], std::array::from_fn(|j| p[j] + [2, 1, 2][j] * 1024));
    }
}
#[test]
fn boundary_rejection_spends_nothing() {
    let mut s = Sim::new(0);
    s.step(&action(1, Pulse::Spread));
    let config = s.state.config.clone();
    s.step(&action(1, Pulse::Spread));
    assert!(s.state.rejected);
    assert_eq!(s.state.config, config);
}
#[test]
fn exhaustion_is_recoverable_with_exact_undo() {
    let mut s = Sim::new(0);
    s.step(&action(0, Pulse::Gather));
    let previous = s.state.config.clone();
    s.step(&action(0, Pulse::Spread));
    assert!(s.lost());
    s.step(&Intent { undo: true, ..Default::default() });
    assert_eq!(s.state.config, previous);
    assert!(!s.lost());
    s.step(&Intent { restart: true, ..Default::default() });
    assert_eq!(s.state.config, Sim::new(0).state.config);
}
#[test]
fn secured_cargo_is_solid_and_reverse_delivery_can_go_around_it() {
    let mut s = Sim::at_level(4);
    s.step(&action(0, Pulse::Spread));
    assert!(s.state.config.docked[0]);
    s.step(&action(1, Pulse::Gather));
    let before = s.state.config.clone();
    s.state.selected = 1;
    s.state.mode = Pulse::Gather;
    assert_eq!(s.preview().obstruction.unwrap().reason, "docked cargo");
    s.step(&Intent { execute: true, ..Default::default() });
    assert_eq!(s.state.config, before);
    s.step(&Intent { restart: true, ..Default::default() });
    for (b, m) in chamber(4).solution {
        s.step(&action(b, m));
    }
    assert!(s.won());
}
#[test]
fn swept_boxes_catch_tunneling_and_three_dimensional_clearance() {
    assert!(sweep(point(-3., 2., 0.), point(3., 2., 0.), point(0., 2., 0.), [HALF * 2; 3]).is_some());
    assert!(sweep(point(-3., 3., 1.), point(3., 3., 1.), point(0., 2., 0.), [HALF * 2; 3]).is_none());
}
#[test]
fn same_intentions_are_deterministic() {
    assert_deterministic(|| Sim::new(7), &script());
}
#[test]
fn saves_resume_exactly_including_undo_history() {
    snapshot::assert_resumes_as_promised(|| Sim::new(7), &script(), 1);
    let mut s = Sim::new(0);
    s.step(&action(0, Pulse::Spread));
    let bytes = snapshot::save(&s, "cargo").unwrap();
    let mut t = Sim::new(4);
    snapshot::restore(&mut t, &bytes).unwrap();
    s.step(&Intent { undo: true, ..Default::default() });
    t.step(&Intent { undo: true, ..Default::default() });
    assert_eq!(s.state_hash(), t.state_hash());
}
#[test]
fn invalid_save_is_atomic() {
    let mut s = Sim::new(0);
    let before = s.state_hash();
    let mut invalid = s.capture();
    invalid.config.positions[0][1] = -10000;
    assert!(s.restore(invalid).is_err());
    assert_eq!(before, s.state_hash());
}

#[test]
fn gathering_rejects_simultaneous_cube_collision() {
    let mut s = Sim::new(0);
    s.step(&action(0, Pulse::Gather));
    let before = s.state.config.clone();
    assert_eq!(s.preview().obstruction.as_ref().unwrap().reason, "cargo path");
    s.step(&Intent { execute: true, ..Default::default() });
    assert_eq!(s.state.config, before);
}
#[test]
fn solid_baffles_reject_a_public_pulse_without_spending_allowance() {
    let mut found = false;
    for n in 1..6 {
        for b in 0..chamber(n).beacons.len() {
            for mode in [Pulse::Spread, Pulse::Gather] {
                let mut s = Sim::at_level(n);
                s.step(&Intent { beacon: Some(b), mode: Some(mode), ..Default::default() });
                if s.preview().obstruction.as_ref().is_some_and(|o| o.reason == "solid baffle") {
                    let before = s.state.config.clone();
                    s.step(&Intent { execute: true, ..Default::default() });
                    assert_eq!(s.state.config, before);
                    assert!(s.state.rejected);
                    found = true;
                }
            }
        }
    }
    assert!(found, "at least one baffle meaningfully obstructs a pulse");
}
#[test]
fn verification_input_completes_campaign_through_real_intentions() {
    let mut s = Sim::new(0);
    for input in scatterlock::verification_input() {
        s.step(&input);
    }
    assert_eq!(s.state.level, 5);
    assert!(s.won());
}

#[test]
fn docking_happens_at_the_endpoint_and_then_freezes_cargo() {
    let mut s = Sim::at_level(2);
    s.step(&action(0, Pulse::Spread));
    assert_eq!(s.state.config.docked, vec![false, false]);
    s.step(&action(1, Pulse::Gather));
    assert_eq!(s.state.config.docked, vec![true, false]);
    let locked = s.state.config.positions[0];
    s.step(&action(2, Pulse::Spread));
    assert_eq!(s.state.config.positions[0], locked);
    assert!(s.won());
}
#[test]
fn saves_resume_across_every_chamber_transition() {
    snapshot::assert_resumes_as_promised(|| Sim::new(0), &scatterlock::verification_input(), 1);
}
