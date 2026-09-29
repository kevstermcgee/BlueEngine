//! The rules are a pure library, so they are tested without a window: same seed and inputs must give the
//! same run, and each rule is checked by driving `Sim` the way the window does.
use vesper3d::math::V;
use vesper3d::viewer::devkit::{
    assert_deterministic, run_inputs, snapshot, Playback, SaveError, SaveSlots, Simulation, QUICK_SLOT,
};
use {{lib}}::{Event, Input, Sim, ORBS, PLATFORM_HALF};

/// A scripted "player": walks, weaves, turns and hops for ten seconds.
fn scripted() -> Vec<Input> {
    (0..600)
        .map(|t| Input {
            forward: 1.,
            right: if (t / 90) % 2 == 0 { 0.3 } else { -0.3 },
            look: [if t % 120 < 30 { 0.02 } else { 0. }, 0.],
            jump: t % 100 == 50,
        })
        .collect()
}

#[test]
fn the_same_seed_and_inputs_replay_identically() {
    assert_deterministic(|| Sim::new(7), &scripted());
}

#[test]
fn different_seeds_play_differently() {
    let a = run_inputs(&mut Sim::new(1), &scripted());
    let b = run_inputs(&mut Sim::new(2), &scripted());
    assert_eq!(a.first_divergence(&b), Some(0), "the orbs are placed from the seed");
}

#[test]
fn recorded_inputs_round_trip_through_json_playback() {
    let json = serde_json::to_vec(&scripted()).unwrap();
    let mut play = Playback::<Input>::from_json(&json).unwrap();
    assert_eq!(play.len(), 600);
    assert_eq!(play.next_input(), scripted()[0]);
}

#[test]
fn walking_off_the_platform_ends_the_run() {
    let mut sim = Sim::new(3);
    let straight_on = Input { forward: 1., ..Default::default() };
    for _ in 0..600 {
        sim.step(&straight_on);
    }
    assert!(sim.over, "nothing catches you past the edge");
    assert!(sim.drain_events().contains(&Event::Fell));
    let tick = sim.tick;
    sim.step(&straight_on);
    assert_eq!(sim.tick, tick, "a finished run ignores input");
}

#[test]
fn touching_an_orb_scores_and_a_new_one_appears() {
    let mut sim = Sim::new(5);
    let (x, z) = (sim.player.position.0, sim.player.position.2);
    sim.orbs[0] = V(x, 0.6, z);
    sim.step(&Input::default());
    assert_eq!(sim.score, 1);
    assert_eq!(sim.orbs.len(), ORBS);
    assert!(sim.drain_events().iter().any(|e| matches!(e, Event::Collected { score: 1, .. })));
    assert!(sim.orbs.iter().all(|o| o.0.abs() < PLATFORM_HALF && o.2.abs() < PLATFORM_HALF));
}

#[test]
fn a_bumper_shoves_the_player_with_an_impulse_that_fades() {
    let mut sim = Sim::new(9);
    let (x, z) = (sim.player.position.0, sim.player.position.2);
    sim.bumpers[0].pos = V(x - 0.3, 0.4, z);
    sim.step(&Input::default());
    assert!(sim.drain_events().iter().any(|e| matches!(e, Event::Bumped { .. })));
    assert!(sim.player.push_velocity().length() > 3., "shoved away from the bumper");
    sim.bumpers.clear(); // no second shove while we watch the first one fade
    for _ in 0..240 {
        sim.step(&Input::default());
    }
    assert_eq!(sim.player.push_velocity().length(), 0., "the push has died out");
}

#[test]
fn a_save_from_any_tick_resumes_exactly_in_a_brand_new_game() {
    // Also with a bumper's shove (and its cooldown) in flight when the saves are taken.
    // A game whose `Sim` embeds a rigid-body world cannot pass this while bodies touch: use
    // `snapshot::assert_loads_replay_identically` and `snapshot::assert_resumes_within` instead
    // (docs/SAVE_STATE.md, "Which contract a physics game can meet").
    let shoved = || {
        let mut sim = Sim::new(9);
        let p = sim.player.position;
        sim.bumpers[0].pos = V(p.0 - 0.3, 0.4, p.2);
        sim
    };
    snapshot::assert_resumes_exactly(|| Sim::new(7), &scripted(), 25);
    snapshot::assert_resumes_exactly(shoved, &scripted(), 5);
}

#[test]
fn a_bad_save_is_refused_and_the_running_game_is_left_alone() {
    let mut sim = Sim::new(11);
    for input in scripted().iter().take(100) {
        sim.step(input);
    }
    let bytes = snapshot::save(&sim, "good").unwrap();
    for input in scripted().iter().take(50) {
        sim.step(input);
    }
    let before = sim.state_hash();
    let mut damaged = bytes.clone();
    let mid = damaged.len() / 2;
    damaged[mid] ^= 0x20;
    assert!(snapshot::restore(&mut sim, &damaged).is_err(), "a flipped bit is always caught");
    assert!(snapshot::restore(&mut sim, &bytes[..bytes.len() / 2]).is_err());
    assert_eq!(sim.state_hash(), before, "a refused load changes nothing");
    snapshot::restore(&mut sim, &bytes).unwrap();
    assert_eq!(sim.tick, 100);
}

#[test]
fn quick_save_and_quick_load_go_through_a_slot_with_a_backup() {
    let dir = std::env::temp_dir().join(format!("{{name}}-slots-{}", std::process::id()));
    let slots = SaveSlots::new(&dir);
    let mut sim = Sim::new(13);
    assert!(matches!(snapshot::load_from_slot(&mut sim, &slots, QUICK_SLOT), Err(SaveError::NotFound(_))));
    for input in scripted().iter().take(60) {
        sim.step(input);
    }
    snapshot::save_to_slot(&sim, &slots, QUICK_SLOT, "first").unwrap();
    let saved = sim.state_hash();
    for input in scripted().iter().take(60) {
        sim.step(input);
    }
    let (header, _) = snapshot::load_from_slot(&mut sim, &slots, QUICK_SLOT).unwrap();
    assert_eq!((header.label.as_str(), sim.state_hash()), ("first", saved));
    let _ = std::fs::remove_dir_all(&dir);
}
