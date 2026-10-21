#[path = "../src/docking_audio.rs"]
mod docking_audio;

use crosshaul::{Intent, Sim, TRAVEL};
use docking_audio::DockingArrival;
use vesper3d::viewer::devkit::Snapshot;

fn tick(sim: &mut Sim, intent: Intent) -> bool {
    let docking = DockingArrival::finishing(sim);
    sim.step(&intent);
    docking.is_some_and(|arrival| arrival.completed(sim))
}

fn start_exchange(chamber: usize) -> Sim {
    let mut sim = Sim::at(chamber);
    let [a, b, axis] = sim.level().solution[0];
    assert!(!tick(&mut sim, Intent { select: Some(a as u8), ..Default::default() }));
    assert!(!tick(&mut sim, Intent { select: Some(b as u8), axis: Some(axis as u8), ..Default::default() }));
    assert!(!tick(&mut sim, Intent { execute: true, ..Default::default() }));
    assert!(sim.motion.is_some());
    sim
}

#[test]
fn ordinary_dock_arrival_clicks_once_when_the_slide_finishes() {
    let mut sim = start_exchange(1);
    // Endpoint docking changes immediately, before a rendered slide arrives.
    assert_eq!(sim.docked(), 1);
    for _ in 1..TRAVEL {
        assert!(!tick(&mut sim, Intent::default()));
    }
    assert!(tick(&mut sim, Intent::default()));
    assert!(!sim.won(), "ordinary arrivals must sound before final success");
    for _ in 0..TRAVEL {
        assert!(!tick(&mut sim, Intent::default()), "idle frames cannot repeat the click");
    }
}

#[test]
fn undo_and_restart_on_the_final_travel_tick_cancel_the_arrival() {
    for command in [Intent { undo: true, ..Default::default() }, Intent { restart: true, ..Default::default() }] {
        let mut sim = start_exchange(1);
        for _ in 1..TRAVEL {
            assert!(!tick(&mut sim, Intent::default()));
        }
        assert!(!tick(&mut sim, command));
        assert_eq!(sim.positions, sim.level().start);
        assert!(!tick(&mut sim, Intent::default()));
    }
}

#[test]
fn restored_in_flight_slide_and_final_success_keep_the_arrival_event() {
    let mut sim = start_exchange(0);
    for _ in 0..7 {
        assert!(!tick(&mut sim, Intent::default()));
    }
    let mut restored = Sim::at(0);
    restored.restore(sim.capture()).unwrap();
    let mut arrivals = 0;
    while restored.motion.is_some() {
        arrivals += usize::from(tick(&mut restored, Intent::default()));
    }
    assert!(restored.won());
    assert_eq!(arrivals, 1);
    assert!(!tick(&mut restored, Intent::default()));
}
