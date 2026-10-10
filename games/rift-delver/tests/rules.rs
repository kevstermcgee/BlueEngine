use rift_delver::*;
use vesper3d::math::V;
use vesper3d::viewer::devkit::{assert_deterministic, snapshot};
fn input() -> Input {
    Input { interact: true, ..Default::default() }
}
#[test]
fn identical_seed_and_intentions_replay_identically() {
    let mut inputs = vec![input()];
    for n in 0..1800 {
        inputs.push(Input {
            forward: (n % 240 < 90) as u8 as f32,
            right: 0.5,
            look: [0.015, 0.],
            fire: true,
            dash: n % 100 == 0,
            pulse: n % 420 == 0,
            ..Default::default()
        });
    }
    assert_deterministic(|| Sim::new(7), &inputs);
}
#[test]
fn an_expedition_resumes_exactly_including_rng_and_enemy_attacks() {
    let mut inputs = vec![input()];
    for n in 0..1000 {
        inputs.push(Input { right: 1., look: [0.02, 0.], fire: true, dash: n % 120 == 0, ..Default::default() });
    }
    snapshot::assert_resumes_as_promised(|| Sim::new(11), &inputs, 137);
}
#[test]
fn dash_is_invulnerable_and_has_a_real_cooldown() {
    let mut s = Sim::new(1);
    s.start_run();
    s.s.invuln = 0;
    s.step(&Input { dash: true, ..Default::default() });
    assert!(s.s.invuln > 0);
    let cd = s.s.dash_cd;
    s.step(&Input { dash: true, ..Default::default() });
    assert_eq!(s.s.dash_cd, cd - 1);
}
#[test]
fn cleared_waves_offer_three_distinct_perks_and_heal() {
    let mut s = Sim::new(1);
    s.start_run();
    s.s.remaining = 0;
    s.s.enemies.clear();
    s.s.health = 25.;
    s.step(&Input::default());
    assert_eq!(s.s.phase, Phase::Draft);
    assert!(s.s.health > 25.);
    assert_ne!(s.s.offers[0], s.s.offers[1]);
    s.step(&Input { choose: 1, ..Default::default() });
    assert_eq!(s.s.phase, Phase::Combat);
    assert_eq!(s.s.wave, 2);
    assert_eq!(s.s.perks.iter().sum::<u32>(), 1);
}
#[test]
fn third_wave_unlocks_gate_and_extract_banks_earned_currency() {
    let mut s = Sim::new(1);
    s.start_run();
    s.s.wave = 3;
    s.s.remaining = 0;
    s.s.haul = 100;
    s.step(&Input::default());
    s.step(&Input { choose: 1, ..Default::default() });
    assert_eq!(s.s.phase, Phase::Gate);
    assert_eq!(s.s.meta.best, 1);
    s.step(&Input { choose: 1, ..Default::default() });
    assert_eq!(s.s.phase, Phase::Camp);
    assert_eq!(s.s.meta.bank, 115);
    assert_eq!(s.s.haul, 0);
    s.step(&Input { choose: 1, ..Default::default() });
    assert_eq!(s.s.meta.levels[0], 1);
    assert_eq!(s.s.meta.bank, 70);
    s.start_run();
    assert_eq!(s.s.health, 112.);
}
#[test]
fn deeper_gate_changes_layout_and_retains_build() {
    let mut s = Sim::new(1);
    s.s.phase = Phase::Gate;
    s.s.perks[4] = 2;
    s.step(&input());
    assert_eq!(s.s.depth, 2);
    assert_eq!(s.s.perks[4], 2);
    assert_eq!(s.walls[5].min, obstacles(2)[5].min);
}
#[test]
fn shooting_cannot_hit_behind_cover() {
    let mut s = Sim::new(1);
    s.start_run();
    s.player.position = V(-8., 1.6, 0.);
    s.s.enemies = vec![Enemy {
        id: 1,
        kind: Kind::Seeker,
        pos: V(-8., 1.6, -10.),
        hp: 100.,
        max_hp: 100.,
        timer: 100,
        slow: 0,
        flash: 0,
        dir: V::ZERO,
    }];
    s.step(&Input { fire: true, ..Default::default() });
    assert_eq!(s.s.enemies[0].hp, 100.);
}
#[test]
fn shots_hit_spatial_targets_and_create_loot() {
    let mut s = Sim::new(1);
    s.start_run();
    s.s.enemies = vec![Enemy {
        id: 1,
        kind: Kind::Drone,
        pos: V(0., 1.6, 10.),
        hp: 1.,
        max_hp: 1.,
        timer: 100,
        slow: 0,
        flash: 0,
        dir: V::ZERO,
    }];
    s.step(&Input { fire: true, ..Default::default() });
    assert!(s.s.enemies.is_empty());
    assert_eq!(s.s.run_kills, 1);
    assert!(!s.s.drops.is_empty());
}
#[test]
fn heat_prevents_infinite_fire_and_cools_back_to_ready() {
    let mut s = Sim::new(1);
    s.start_run();
    s.s.remaining = 999;
    for _ in 0..500 {
        s.s.health = 100.;
        s.s.invuln = 100;
        s.step(&Input { fire: true, ..Default::default() });
    }
    assert!(s.s.heat > 0.);
    s.s.overheat = true;
    for _ in 0..400 {
        s.s.invuln = 100;
        s.step(&Input::default());
    }
    assert!(!s.s.overheat);
    assert_eq!(s.s.heat, 0.);
}
#[test]
fn bosses_spawn_every_third_depth() {
    let mut s = Sim::new(1);
    s.s.depth = 3;
    s.s.wave = 2;
    s.s.phase = Phase::Draft;
    s.step(&Input { choose: 1, ..Default::default() });
    assert!(s.s.enemies.iter().any(|e| e.kind == Kind::Warden));
}
#[test]
fn workshop_cannot_spend_money_it_does_not_have() {
    let mut s = Sim::new(1);
    s.step(&Input { choose: 1, ..Default::default() });
    assert_eq!(s.s.meta.levels[0], 0);
    assert_eq!(s.s.meta.bank, 0);
}
#[test]
fn weapon_unlocks_depend_on_completed_depths() {
    let mut s = Sim::new(1);
    assert_eq!(s.unlocked(), 1);
    s.s.meta.best = 3;
    assert_eq!(s.unlocked(), 2);
    s.s.meta.best = 6;
    assert_eq!(s.unlocked(), 3);
}

#[test]
fn long_session_stays_bounded_and_progresses_through_real_combat() {
    let mut s = Sim::new(731);
    let mut gates = 0;
    for _ in 0..180_000 {
        let mut input = autopilot(&s);
        if s.s.phase == Phase::Gate {
            gates += 1;
            if s.s.depth >= 3 {
                input.choose = 1;
            }
        }
        s.step(&input);
        s.drain_events();
        assert!(s.s.enemies.len() <= 29);
        assert!(s.s.bolts.len() < 1000);
        assert!(s.s.health.is_finite());
    }
    println!(
        "endurance: gates {gates}, runs {}, kills {}, best {}, bank {}",
        s.s.meta.runs, s.s.meta.kills, s.s.meta.best, s.s.meta.bank
    );
    assert!(s.s.meta.kills > 100);
    assert!(gates > 0, "a bot using ordinary inputs should be able to clear a depth");
}

fn target(id: u32, z: f32, hp: f32) -> Enemy {
    Enemy {
        id,
        kind: Kind::Seeker,
        pos: V(0., 1.6, z),
        hp,
        max_hp: hp.max(1.),
        timer: 100,
        slow: 0,
        flash: 0,
        dir: V::ZERO,
    }
}
#[test]
fn pierce_chain_freeze_and_explosion_are_real_rule_effects() {
    let mut s = Sim::new(1);
    s.start_run();
    s.s.perks[3] = 1;
    s.s.perks[4] = 1;
    s.s.perks[9] = 1;
    s.s.enemies = vec![target(1, 10., 100.), target(2, 7., 100.)];
    s.step(&Input { fire: true, ..Default::default() });
    assert!(s.s.enemies.iter().all(|e| e.hp < 100. && e.slow > 0));
    s.s.perks[5] = 1;
    s.s.enemies = vec![target(3, 10., 0.), target(4, 8., 10.)];
    s.step(&Input::default());
    assert!(s.s.enemies.is_empty());
}
#[test]
fn blood_circuit_and_reinforced_restore_health() {
    let mut s = Sim::new(1);
    s.start_run();
    s.s.health = 40.;
    s.s.perks[6] = 2;
    s.s.enemies = vec![target(1, 10., 0.)];
    s.step(&Input::default());
    assert_eq!(s.s.health, 44.);
    s.s.phase = Phase::Draft;
    s.s.offers = [7, 0, 1];
    s.step(&Input { choose: 1, ..Default::default() });
    assert_eq!(s.max_health(), 125.);
    assert_eq!(s.s.health, 69.);
}
#[test]
fn quick_circuit_overcharge_cold_sink_and_flux_change_tuning() {
    let mut a = Sim::new(2);
    let mut b = Sim::new(2);
    a.start_run();
    b.start_run();
    b.s.perks[0] = 1;
    b.s.perks[1] = 1;
    b.s.perks[2] = 1;
    b.s.perks[8] = 1;
    b.s.perks[10] = 1;
    for s in [&mut a, &mut b] {
        s.s.enemies = vec![target(1, 10., 100.)];
        s.step(&Input { fire: true, ..Default::default() });
    }
    assert!(b.s.enemies[0].hp < a.s.enemies[0].hp);
    assert!(b.s.fire_cd < a.s.fire_cd);
    assert!(b.s.heat < a.s.heat);
    assert!(b.dash_time() < a.dash_time());
    assert!(b.pulse_time() < a.pulse_time());
}
#[test]
fn lucky_strike_raises_critical_rate_over_seeded_shots() {
    let mut a = Sim::new(22);
    let mut b = Sim::new(22);
    a.start_run();
    b.start_run();
    b.s.perks[11] = 10;
    let mut counts = [0, 0];
    for _ in 0..120 {
        for (i, s) in [&mut a, &mut b].into_iter().enumerate() {
            s.s.enemies = vec![target(1, 10., 10000.)];
            s.s.fire_cd = 0;
            s.s.heat = 0.;
            s.s.overheat = false;
            s.step(&Input { fire: true, ..Default::default() });
            counts[i] += s.drain_events().iter().filter(|e| matches!(e, Event::Hit { crit: true, .. })).count();
        }
    }
    assert!(counts[1] > counts[0] * 3);
}
#[test]
fn defeat_keeps_insurance_and_bank_but_seals_the_lost_haul() {
    let mut s = Sim::new(1);
    s.start_run();
    s.s.meta.bank = 50;
    s.s.meta.levels[3] = 2;
    s.s.health = 1.;
    s.s.invuln = 0;
    s.s.haul = 100;
    s.s.enemies = vec![target(1, 15., 100.)];
    s.step(&Input::default());
    assert_eq!(s.s.phase, Phase::Lost);
    assert_eq!(s.s.meta.bank, 85);
    assert_eq!(s.s.haul, 0);
    s.step(&Input { interact: true, ..Default::default() });
    assert_eq!(s.s.phase, Phase::Camp);
}
#[test]
fn shockwave_damages_nearby_enemies_and_clears_nearby_projectiles() {
    let mut s = Sim::new(1);
    s.start_run();
    s.s.enemies = vec![target(1, 10., 100.)];
    s.s.bolts = vec![Bolt { pos: V(0., 1.6, 12.), vel: V::ZERO, life: 100, damage: 10. }];
    s.step(&Input { pulse: true, ..Default::default() });
    assert!(s.s.enemies[0].hp < 100.);
    assert!(s.s.bolts.is_empty());
    assert!(s.s.pulse_cd > 0);
}
