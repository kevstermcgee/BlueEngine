use super::*;
use vesper3d::runtime::{assert_deterministic, snapshot};
fn step(g: &mut Embertrail, n: u32, i: Intent) {
    for _ in 0..n {
        g.step(&i);
    }
}
#[test]
fn public_route_is_deterministic_and_saves_resume_exactly() {
    let inputs: Vec<_> = (0..Embertrail::VERIFY_TICKS)
        .map(Embertrail::verification_input)
        .collect();
    assert_deterministic(|| Embertrail::new(7), &inputs);
    snapshot::assert_resumes_exactly(|| Embertrail::new(7), &inputs, 317);
    let mut game = Embertrail::new(7);
    for i in &inputs {
        game.step(i);
    }
    eprintln!(
        "route course={} x={} sparks={} lives={} checkpoint={} cleared={:x}",
        game.state.course,
        game.player().x,
        game.state.sparks,
        game.state.lives,
        game.state.checkpoint,
        game.state.cleared
    );
    assert_ne!(game.outcome(), "lost");
    assert!(game.state.score > 0);
    assert!(game.state.tick > 900);
    let (hash, outcome) = verify::<Embertrail>();
    assert_eq!(hash, game.state_hash());
    if let Ok(path) = std::env::var("BE2_VERIFY_REPORT") {
        std::fs::write(path,serde_json::json!({"hash":format!("{hash:016x}"),"outcome":outcome,"ticks":Embertrail::VERIFY_TICKS,"purpose":"Campaign introduction: public run/jump inputs exercise momentum, platform collisions, collectibles, enemies and checkpoint progression."}).to_string()).unwrap();
    }
}
#[test]
fn every_course_has_unique_layout_three_relics_and_a_reachable_secret() {
    let mut hashes = std::collections::HashSet::new();
    for course in 0..COURSES {
        let l = levels::build(course);
        assert!(hashes.insert(hash_json(&l)));
        assert_eq!(l.pickups.iter().filter(|p| p.kind == 5).count(), 3);
        assert!(l.width > 2900 && l.width < 5000);
        assert!(l.secret.w > 0);
        assert!(l
            .tiles
            .iter()
            .any(|t| t.r.contains(Point::new(l.checkpoint + 20, FLOOR))));
        assert_eq!(
            l.enemies.iter().filter(|e| e.kind == 4).count(),
            usize::from(course % 4 == 3)
        );
    }
}
#[test]
fn momentum_sprint_air_control_and_variable_jump_height() {
    let mut walk = Embertrail::new(7);
    step(
        &mut walk,
        25,
        Intent {
            x: 1,
            ..Default::default()
        },
    );
    let mut run = Embertrail::new(7);
    step(
        &mut run,
        25,
        Intent {
            x: 1,
            sprint: true,
            ..Default::default()
        },
    );
    assert!(run.player().x > walk.player().x + 20);
    let vx = run.state.vx;
    run.step(&Intent::default());
    assert!(run.state.vx > 0 && run.state.vx < vx);
    let mut tap = Embertrail::new(7);
    tap.step(&Intent {
        y: -1,
        ..Default::default()
    });
    step(&mut tap, 19, Intent::default());
    let mut hold = Embertrail::new(7);
    step(
        &mut hold,
        20,
        Intent {
            y: -1,
            ..Default::default()
        },
    );
    assert!(hold.player().y < tap.player().y - 30);
    hold.step(&Intent {
        x: -1,
        ..Default::default()
    });
    assert!(hold.state.vx < 0);
}
#[test]
fn jump_buffer_and_coyote_time_work() {
    let mut g = Embertrail::new(7);
    g.state.grounded = false;
    g.state.coyote = 3;
    g.state.py = 332 * SUB;
    g.step(&Intent {
        action: true,
        ..Default::default()
    });
    assert!(g.state.vy < -2000);
    let mut g = Embertrail::new(7);
    g.state.grounded = false;
    g.state.coyote = 0;
    g.state.py = 334 * SUB;
    g.state.vy = 2 * SUB;
    g.step(&Intent {
        action: true,
        ..Default::default()
    });
    step(&mut g, 3, Intent::default());
    assert!(g.state.vy < 0);
}
#[test]
fn power_cache_brick_break_and_projectiles() {
    let mut g = Embertrail::new(7);
    g.state.px = 450 * SUB;
    g.state.py = 294 * SUB;
    g.state.vy = -3 * SUB;
    g.step(&Intent::default());
    assert_eq!(
        g.state
            .level
            .tiles
            .iter()
            .find(|t| t.r.x == 448)
            .unwrap()
            .kind,
        4
    );
    assert!(g.state.level.pickups.iter().any(|p| p.kind == 1));
    g.state.level.pickups.push(Pickup {
        r: g.player(),
        kind: 2,
        active: true,
    });
    g.collect();
    assert_eq!(g.state.form, 2);
    g.step(&Intent {
        y: 1,
        action: true,
        ..Default::default()
    });
    assert_eq!(g.state.shots.len(), 1);
    g.hurt(false);
    assert_eq!(g.state.form, 1);
    let lives = g.state.lives;
    g.hurt(false);
    assert_eq!(g.state.lives, lives);
    g.state.shield = 0;
    g.state.form = 1;
    g.state.px = 480 * SUB;
    g.state.py = 294 * SUB;
    g.state.vy = -3 * SUB;
    g.step(&Intent::default());
    assert_eq!(
        g.state
            .level
            .tiles
            .iter()
            .find(|t| t.r.x == 480)
            .unwrap()
            .r
            .w,
        0
    );
}
#[test]
fn stomp_shell_kick_and_boss_gate() {
    let mut g = Embertrail::new(7);
    g.state.level.enemies.clear();
    g.state.level.enemies.push(Enemy {
        r: Rect::new(100, 342, 28, 26),
        kind: 1,
        dir: -1,
        vy: 0,
        phase: 0,
        active: true,
        shell: false,
        rolling: false,
        hp: 1,
        immunity: 0,
    });
    g.state.px = 100 * SUB;
    g.state.py = 320 * SUB;
    g.state.vy = SUB;
    g.enemy_contacts(Rect::new(100, 310, 22, 30));
    assert!(g.state.level.enemies[0].shell);
    assert!(g.state.vy < 0);
    g.state.py = 338 * SUB;
    g.state.px = 90 * SUB;
    g.state.vy = 0;
    g.enemy_contacts(g.player());
    assert!(g.state.level.enemies[0].rolling);
    g.state.unlocked = 3;
    g.enter_course(3);
    let exit = g.state.level.exit;
    g.state.px = exit.x * SUB;
    g.state.py = exit.y * SUB;
    g.step(&Intent::default());
    assert_eq!(g.state.transition, 0);
    g.state
        .level
        .enemies
        .iter_mut()
        .filter(|e| e.kind == 4)
        .for_each(|e| e.active = false);
    g.step(&Intent::default());
    assert!(g.state.transition > 0);
    assert_eq!(g.state.unlocked, 4);
}
#[test]
fn secret_checkpoint_death_restart_and_save_validation() {
    let mut g = Embertrail::new(7);
    g.state.px = g.state.level.secret.x * SUB;
    g.state.py = 338 * SUB;
    g.portal(&Intent {
        y: 1,
        ..Default::default()
    });
    assert!(g.state.secret_inside);
    let save = g.capture();
    let mut fresh = Embertrail::new(7);
    fresh.restore(save).unwrap();
    assert_eq!(g.state_hash(), fresh.state_hash());
    g.state.px = g.state.level.secret_return.x * SUB;
    g.state.portal_cooldown = 0;
    g.portal(&Intent {
        y: 1,
        ..Default::default()
    });
    assert!(!g.state.secret_inside);
    g.state.checkpoint = true;
    g.hurt(true);
    step(&mut g, 90, Intent::default());
    assert!(g.player().x >= g.state.level.checkpoint);
    g.state.unlocked = 9;
    g.state.cleared = 15;
    g.state.best_relics[0] = 3;
    g.restart();
    assert_eq!(g.state.unlocked, 9);
    assert_eq!(g.state.cleared, 15);
    assert_eq!(g.state.best_relics[0], 3);
    let mut bad = g.capture();
    bad.course = 32;
    assert!(g.restore(bad).is_err());
    let mut bad = g.capture();
    bad.level.enemies[0].vy = i32::MIN;
    assert!(g.restore(bad).is_err());
}
#[test]
fn loss_stops_the_authority_and_final_beacon_wins() {
    let mut g = Embertrail::new(7);
    g.state.lives = 1;
    g.hurt(true);
    assert_eq!(g.outcome(), "lost");
    let h = g.state_hash();
    g.step(&Intent {
        x: 1,
        ..Default::default()
    });
    assert_eq!(h, g.state_hash());
    let mut g = Embertrail::new(7);
    g.state.unlocked = 31;
    g.enter_course(31);
    g.complete();
    step(&mut g, 150, Intent::default());
    assert_eq!(g.outcome(), "won");
    assert_eq!(g.state.cleared, 1 << 31);
}

#[test]
fn crouching_keeps_feet_fixed_and_cannot_stand_inside_a_ceiling() {
    let mut g = Embertrail::new(7);
    let feet = g.player().y + g.player().h;
    g.step(&Intent {
        y: 1,
        ..Default::default()
    });
    assert_eq!(g.player().h, 18);
    assert_eq!(g.player().y + g.player().h, feet);
    g.state.level.tiles.push(Tile {
        r: Rect::new(70, 328, 50, 20),
        kind: 1,
        content: 0,
    });
    g.step(&Intent::default());
    assert!(g.state.crouching);
    g.state.level.tiles.pop();
    g.step(&Intent::default());
    assert!(!g.state.crouching);
    assert_eq!(g.player().y + g.player().h, feet);
}

#[test]
fn every_course_has_a_continuous_reachable_surface_route_to_its_exit() {
    // The measured held-Up jump rises over 170 px and travels over 300 px when running.
    // Connect supporting surfaces conservatively, keeping room for the 22 px collider.
    for course in 0..COURSES {
        let level = levels::build(course);
        let surfaces: Vec<_> = level
            .tiles
            .iter()
            .filter(|t| t.r.x < level.width && t.r.w > 0)
            .map(|t| t.r)
            .collect();
        let mut seen = vec![false; surfaces.len()];
        seen[0] = true;
        for _ in 0..surfaces.len() {
            let old = seen.clone();
            for (a, from) in surfaces.iter().enumerate() {
                if !old[a] {
                    continue;
                }
                for (b, to) in surfaces.iter().enumerate() {
                    let gap = (to.x - (from.x + from.w)).max(0);
                    if to.x >= from.x && gap <= 250 && from.y - to.y <= 160 {
                        seen[b] = true;
                    }
                }
            }
            if old == seen {
                break;
            }
        }
        assert!(
            surfaces.iter().enumerate().any(|(n, r)| seen[n]
                && r.x <= level.exit.x
                && r.x + r.w >= level.exit.x + level.exit.w),
            "No terrain route in {}",
            levels::NAMES[course]
        );
    }
}
