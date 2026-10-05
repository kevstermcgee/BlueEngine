use signal_garden::{Cell, Event, Input, Outcome, Sim, BEACON, CHARGE_TICKS, LIMIT, MOVE_TICKS, PADS, RELAYS};
use std::collections::{HashMap, VecDeque};
use vesper3d::viewer::devkit::{assert_deterministic, snapshot, Settings, Simulation, Snapshot};

fn walk(sim: &mut Sim, goal: Cell, inputs: &mut Vec<Input>) {
    let start = sim.state.player;
    let mut queue = VecDeque::from([start]);
    let mut from = HashMap::from([((start.0, start.1), start)]);
    while let Some(at) = queue.pop_front() {
        if at == goal {
            break;
        }
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let next = Cell(at.0 + dx, at.1 + dy);
            if next.walkable() && !from.contains_key(&(next.0, next.1)) {
                from.insert((next.0, next.1), at);
                queue.push_back(next);
            }
        }
    }
    let mut route = Vec::new();
    let mut at = goal;
    while at != start {
        route.push(at);
        at = from[&(at.0, at.1)];
    }
    route.reverse();
    for at in route {
        let input = Input { dx: at.0 - sim.state.player.0, dy: at.1 - sim.state.player.1, ..Default::default() };
        for _ in 0..MOVE_TICKS {
            sim.step(&input);
            inputs.push(input);
        }
        assert_eq!(sim.state.player, at);
        assert_eq!(sim.state.outcome, Outcome::Playing);
    }
}
/// This solver sends only public movement/interaction inputs, never edits state or disables hazards.
fn winning_run(seed: u64) -> (Sim, Vec<Input>) {
    let mut sim = Sim::new(seed);
    let mut inputs = Vec::new();
    while sim.state.relay < 6 {
        if sim.state.hearts < 4 {
            walk(&mut sim, BEACON, &mut inputs);
            for _ in 0..1801 {
                let i = Input { interact: true, ..Default::default() };
                sim.step(&i);
                inputs.push(i);
                if sim.state.hearts == 5 {
                    break;
                }
            }
        }
        while sim.state.cargo < 3 {
            let pad = PADS
                .iter()
                .enumerate()
                .min_by_key(|(i, p)| sim.state.pads[*i] + p.distance(sim.state.player) as u32 * MOVE_TICKS)
                .unwrap()
                .1;
            walk(&mut sim, *pad, &mut inputs);
            while sim.state.cargo < 3 && sim.state.pads[PADS.iter().position(|p| p == pad).unwrap()] > 0 {
                let i = Input::default();
                sim.step(&i);
                inputs.push(i);
                if sim.state.cargo > 0 {
                    break;
                }
            }
        }
        let relay = sim.state.relay;
        walk(&mut sim, RELAYS[relay], &mut inputs);
        for _ in 0..3 * CHARGE_TICKS {
            let i = Input { interact: true, ..Default::default() };
            sim.step(&i);
            inputs.push(i);
        }
    }
    walk(&mut sim, BEACON, &mut inputs);
    sim.step(&Input { interact: true, ..Default::default() });
    inputs.push(Input { interact: true, ..Default::default() });
    (sim, inputs)
}
#[test]
fn a_real_input_route_wins_and_replays_across_seeds() {
    for seed in [1, 7, 42] {
        let (mut sim, inputs) = winning_run(seed);
        assert_eq!(sim.state.outcome, Outcome::Won);
        assert!(sim.drain_events().contains(&Event::Won));
        assert_deterministic(|| Sim::new(seed), &inputs);
        let tick = sim.state.tick;
        sim.step(&Input::default());
        assert_eq!(sim.state.tick, tick);
        eprintln!(
            "seed {seed}: win in {} ticks ({:.1} simulated seconds), hash {:016x}",
            tick,
            tick as f32 / 60.,
            sim.state_hash()
        );
    }
}
#[test]
fn mid_charge_saves_resume_exactly_and_damaged_loads_are_atomic() {
    let (_, inputs) = winning_run(7);
    snapshot::assert_resumes_as_promised(|| Sim::new(7), &inputs, 331);
    let mut sim = Sim::new(9);
    let before = sim.state_hash();
    let mut state = sim.capture();
    state.cargo = 99;
    assert!(sim.restore(state).is_err());
    assert_eq!(sim.state_hash(), before);
    let mut bytes = snapshot::save(&sim, "garden").unwrap();
    bytes[80] ^= 1;
    assert!(snapshot::restore(&mut sim, &bytes).is_err());
    assert_eq!(sim.state_hash(), before);
}
#[test]
fn timeout_damage_wrong_relay_and_restart_are_real_rules() {
    let mut sim = Sim::new(1);
    for _ in 0..LIMIT {
        sim.step(&Input::default());
    }
    assert_eq!(sim.state.outcome, Outcome::Lost);
    if let Some(dir) = std::env::var_os("GARDEN_OUTCOME_SAVES") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("lost.be2save"), snapshot::save(&sim, "verified timeout defeat").unwrap()).unwrap();
    }
    assert!(sim.drain_events().contains(&Event::Lost));
    let fresh = Sim::new(1);
    assert_eq!(fresh.state.outcome, Outcome::Playing);
    assert_eq!(fresh.state.hearts, 5);
    let mut sim = Sim::new(1);
    sim.state.player = RELAYS[1];
    sim.state.cargo = 3;
    for _ in 0..CHARGE_TICKS * 3 {
        sim.step(&Input { interact: true, ..Default::default() });
    }
    assert_eq!(sim.state.charges, [0; 6]);
    let mut sim = Sim::new(1);
    sim.state.player = sim.patrols()[0];
    sim.step(&Input::default());
    assert_eq!(sim.state.hearts, 4);
    sim.step(&Input::default());
    assert_eq!(sim.state.hearts, 4, "contact invulnerability prevents damage every tick");
}
#[test]
fn settings_roundtrip_keeps_volumes_when_toggled() {
    let path = std::env::temp_dir().join(format!("garden-settings-{}.json", std::process::id()));
    let mut settings = Settings::default();
    settings.toggle_music();
    settings.toggle_sfx();
    settings.cycle_shadow_quality();
    assert!(settings.store(&path));
    let loaded = Settings::load(&path);
    assert!(!loaded.music_on && !loaded.sfx_on);
    assert_eq!(loaded.music, settings.music);
    assert_eq!(loaded.shadow_quality, settings.shadow_quality);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn write_public_input_playthrough_for_graphical_replay() {
    let (sim, inputs) = winning_run(7);
    assert_eq!(sim.state.outcome, Outcome::Won);
    // An opt-in export of this exact winning route, replayed by main.rs using engine Playback.
    if let Some(path) = std::env::var_os("GARDEN_REPLAY_OUT") {
        std::fs::write(path, serde_json::to_vec(&inputs).unwrap()).unwrap();
    }
}

#[test]
fn shield_spends_one_spark_prevents_hits_expires_and_is_saved() {
    let mut sim = Sim::new(7);
    sim.state.player = sim.patrols()[0];
    sim.state.cargo = 2;
    sim.step(&Input { shield: true, ..Default::default() });
    assert_eq!(sim.state.cargo, 1);
    assert_eq!(sim.state.hearts, 5);
    assert_eq!(sim.state.shield_ticks, 180);
    assert!(sim.drain_events().iter().any(|e| matches!(e, Event::Shield(_))));
    let mut inputs = vec![Input::default(); 181];
    inputs[0].shield = true;
    let make = || {
        let mut s = Sim::new(7);
        s.state.cargo = 2;
        s.state.player = s.patrols()[0];
        s
    };
    snapshot::assert_resumes_as_promised(make, &inputs, 11);
    for _ in 0..180 {
        sim.step(&Input::default());
    }
    assert_eq!(sim.state.shield_ticks, 0);
    let mut sim = Sim::new(7);
    sim.step(&Input { shield: true, ..Default::default() });
    assert_eq!(sim.state.shield_ticks, 0, "cannot shield with no spark");
}
#[test]
fn the_real_version_one_save_migrates_and_finishes_the_original_route() {
    let (expected, inputs) = winning_run(7);
    let mut loaded = Sim::new(999);
    snapshot::restore(&mut loaded, include_bytes!("fixtures/v1.be2save")).unwrap();
    assert_eq!(loaded.state.tick, 331);
    assert_eq!(loaded.state.shield_ticks, 0);
    for input in &inputs[331..] {
        loaded.step(input);
    }
    assert_eq!(loaded.state, expected.state);
}
