use super::*;
use vesper3d::runtime::{assert_deterministic, snapshot};
fn step(g: &mut Embertrail, n: u32, i: Intent) {
    for _ in 0..n {
        g.step(&i);
    }
}
#[test]
fn compact_atlas_button_opens_replays_and_closes_without_jumping() {
    let mut game = Embertrail::new(7);
    let timer = game.state.timer;
    let click = |x, y| Intent {
        action: true,
        pointer: Some(Point::new(x, y)),
        ..Default::default()
    };
    game.step(&click(760, 20));
    assert!(game.state.atlas);
    assert_eq!(game.state.timer, timer);
    game.step(&click(320, 100));
    assert!(!game.state.atlas);
    assert_eq!(game.state.course, 0);
    assert!(game.state.grounded);
    game.step(&click(760, 20));
    assert!(game.state.atlas);
    game.step(&click(790, 30));
    assert!(!game.state.atlas);
    assert!(game.state.grounded);
    game.step(&click(710, 40));
    assert!(
        !game.state.atlas,
        "The old oversized hit area must not open Atlas"
    );
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
fn momentum_sprint_air_control_and_one_fixed_jump_for_every_device() {
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
        action: true,
        ..Default::default()
    });
    step(&mut tap, 19, Intent::default());
    let mut hold = Embertrail::new(7);
    step(
        &mut hold,
        20,
        Intent {
            action: true,
            y: -1,
            ..Default::default()
        },
    );
    assert_eq!(
        hold.player().y,
        tap.player().y,
        "Up must never alter the jump arc"
    );
    assert_eq!(hold.state.vy, tap.state.vy);
    hold.step(&Intent {
        x: -1,
        ..Default::default()
    });
    assert!(hold.state.vx < 0);
    let mut up = Embertrail::new(7);
    step(
        &mut up,
        30,
        Intent {
            y: -1,
            ..Default::default()
        },
    );
    assert_eq!(
        up.player().y,
        338,
        "W/Up only aim or navigate; jump has one button"
    );
    let mut held = Embertrail::new(7);
    step(
        &mut held,
        100,
        Intent {
            action: true,
            ..Default::default()
        },
    );
    assert!(
        held.state.grounded,
        "Holding the button must not auto-jump on landing"
    );
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
        secondary: true,
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
    // The single action jump rises 148 px and travels about 280 px when running.
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
                    if to.x >= from.x && gap <= 245 && from.y - to.y <= 140 {
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

fn at_boss(world: usize) -> Embertrail {
    let mut g = Embertrail::new(7);
    g.state.unlocked = world * 4 + 3;
    g.enter_course(world * 4 + 3);
    g.state.px = (g.state.level.width - levels::BOSS_COURT + 120) * SUB;
    g.state.py = 338 * SUB;
    g.state.shield = 0;
    g
}

#[test]
fn all_eight_bosses_have_tells_recovery_distinct_patterns_and_safe_checkpoints() {
    let mut patterns = std::collections::HashSet::new();
    let mut previous_health = 0;
    for (world, &(tell, attack, recovery)) in BOSS_CYCLES.iter().enumerate() {
        let mut g = at_boss(world);
        let index = g
            .state
            .level
            .enemies
            .iter()
            .position(|e| e.kind == 4)
            .unwrap();
        let boss = &g.state.level.enemies[index];
        assert_eq!(g.boss_phase(boss), BossPhase::Dormant);
        assert!(boss.hp >= previous_health);
        previous_health = boss.hp;
        assert_eq!(
            g.state.level.checkpoint,
            g.state.level.width - levels::BOSS_COURT + 16
        );
        g.enemy_step();
        assert!(g.boss_active());
        assert!(g.state.banner.contains(story::BOSS_NAMES[world]));
        assert!(tell >= 48 && recovery >= 100);
        let mut trace = Vec::new();
        for t in 1..tell + attack + recovery {
            let boss = &g.state.level.enemies[index];
            trace.push((boss.r.x, boss.r.y, g.state.shots.len()));
            if t < tell - 1 {
                assert_eq!(g.boss_phase(boss), BossPhase::Telegraph);
                assert!(g.state.shots.is_empty());
            }
            if t == tell + attack + 2 {
                assert_eq!(g.boss_phase(boss), BossPhase::Recover);
                assert!(g.boss_telegraph(boss).contains("open"));
            }
            g.enemy_step();
        }
        assert!(
            patterns.insert(hash_json(&trace)),
            "Boss patterns must differ in world {world}"
        );
    }
}

#[test]
fn each_guardian_requires_an_open_collar_and_can_be_freed_by_stomping() {
    for (world, &(tell, attack, _)) in BOSS_CYCLES.iter().enumerate() {
        let mut g = at_boss(world);
        let index = g
            .state
            .level
            .enemies
            .iter()
            .position(|e| e.kind == 4)
            .unwrap();
        let r = g.state.level.enemies[index].r;
        let hp = g.state.level.enemies[index].hp;
        g.state.level.enemies[index].phase = 1;
        g.state.px = r.x * SUB;
        g.state.py = (r.y - 20) * SUB;
        g.state.vy = 3 * SUB;
        let before = Rect::new(r.x, r.y - 34, 22, 30);
        g.enemy_contacts(before);
        assert_eq!(
            g.state.level.enemies[index].hp, hp,
            "A closed crown must resist stomps"
        );
        assert!(
            g.state.vy < 0,
            "A crown stomp safely bounces even while armored"
        );
        for remaining in (0..hp).rev() {
            g.state.level.enemies[index].phase = tell + attack + 1;
            g.state.level.enemies[index].immunity = 0;
            g.state.py = (r.y - 20) * SUB;
            g.state.vy = 3 * SUB;
            g.enemy_contacts(before);
            assert_eq!(g.state.level.enemies[index].hp, remaining);
        }
        assert!(!g.state.level.enemies[index].active);
        assert!(!g.boss_active());
        assert!(g.state.score >= 5000);
        assert!(g
            .state
            .banner
            .contains(if world == 7 { "Vesper" } else { "collar" }));
        let exit = g.state.level.exit;
        g.state.px = exit.x * SUB;
        g.state.py = 338 * SUB;
        g.step(&Intent::default());
        assert!(g.state.transition > 0);
        assert_eq!(g.dawn_bells(), 1);
        assert_eq!(g.state.banner, story::BELL_LINES[world]);
    }
}

#[test]
fn projectiles_respect_boss_recovery_immunity_and_clear_on_defeat() {
    for (world, &(tell, attack, _)) in BOSS_CYCLES.iter().enumerate() {
        let mut g = at_boss(world);
        let index = g
            .state
            .level
            .enemies
            .iter()
            .position(|e| e.kind == 4)
            .unwrap();
        let boss = &mut g.state.level.enemies[index];
        boss.phase = 1;
        let r = boss.r;
        let hp = boss.hp;
        let shot = Shot {
            r: Rect::new(r.x + 8, r.y + 8, 10, 10),
            vx: 0,
            vy: 0,
            life: 20,
            hostile: false,
        };
        g.state.shots.push(shot.clone());
        g.shots_step();
        assert_eq!(g.state.level.enemies[index].hp, hp);
        g.state.level.enemies[index].phase = tell + attack + 1;
        g.state.shots.push(shot.clone());
        g.shots_step();
        assert_eq!(g.state.level.enemies[index].hp, hp - 1);
        g.state.shots.push(shot.clone());
        g.shots_step();
        assert_eq!(
            g.state.level.enemies[index].hp,
            hp - 1,
            "Hit immunity prevents a burst deleting a boss"
        );
        g.state.level.enemies[index].hp = 1;
        g.state.level.enemies[index].immunity = 0;
        g.state.shots.push(Shot {
            r: Rect::new(r.x - 80, 300, 12, 12),
            hostile: true,
            ..shot.clone()
        });
        g.state.shots.push(shot);
        g.shots_step();
        assert!(!g.state.level.enemies[index].active);
        assert!(g.state.shots.iter().all(|s| !s.hostile));
    }
    let mut g = Embertrail::new(7);
    g.state.shots.push(Shot {
        r: g.player(),
        vx: 0,
        vy: 0,
        life: 20,
        hostile: true,
    });
    g.shots_step();
    assert_eq!(g.state.lives, 4, "Hostile seeds actually hurt the player");
}

#[test]
fn death_restarts_a_guardian_at_the_arena_checkpoint_and_cast_never_replaces_jump() {
    let mut g = at_boss(3);
    g.state.checkpoint = true;
    let index = g
        .state
        .level
        .enemies
        .iter()
        .position(|e| e.kind == 4)
        .unwrap();
    let health = g.state.level.enemies[index].hp;
    g.state.level.enemies[index].hp = 1;
    g.state.level.enemies[index].phase = 80;
    g.hurt(true);
    step(&mut g, 90, Intent::default());
    assert_eq!(g.player().x, g.state.level.checkpoint + 20);
    assert_eq!(g.state.level.enemies[index].hp, health);
    assert_eq!(g.state.level.enemies[index].phase, 0);
    assert_eq!(g.state.form, 1);
    g.state.form = 2;
    g.step(&Intent {
        action: true,
        secondary: true,
        ..Default::default()
    });
    assert!(g.state.vy < 0);
    assert!(g.state.shots.iter().any(|s| !s.hostile));
    assert!(!g.state.crouching);
}

#[test]
fn fixed_jump_physically_clears_the_largest_required_gap_and_rise() {
    // Public inputs traverse an authored 192 px bridge gap without a lift, then
    // a 120 px tower. These are the campaign's largest mandatory empty gap/rise.
    let mut g = Embertrail::new(7);
    g.state.level.tiles = vec![
        Tile {
            r: Rect::new(0, FLOOR, 320, 100),
            kind: 0,
            content: 0,
        },
        Tile {
            r: Rect::new(512, FLOOR, 280, 100),
            kind: 0,
            content: 0,
        },
        Tile {
            r: Rect::new(792, FLOOR - 120, 240, 220),
            kind: 1,
            content: 0,
        },
    ];
    g.state.level.enemies.clear();
    g.state.level.pickups.clear();
    step(
        &mut g,
        35,
        Intent {
            x: 1,
            sprint: true,
            ..Default::default()
        },
    );
    while g.player().x < 282 {
        g.step(&Intent {
            x: 1,
            sprint: true,
            ..Default::default()
        });
    }
    g.step(&Intent {
        x: 1,
        action: true,
        sprint: true,
        ..Default::default()
    });
    for _ in 0..60 {
        if g.state.grounded && g.player().x > 512 {
            break;
        }
        g.step(&Intent {
            x: 1,
            sprint: true,
            ..Default::default()
        });
    }
    assert!(g.player().x > 512 && g.state.grounded);
    while g.player().x < 712 {
        g.step(&Intent {
            x: 1,
            sprint: true,
            ..Default::default()
        });
    }
    g.step(&Intent {
        x: 1,
        action: true,
        sprint: true,
        ..Default::default()
    });
    for _ in 0..60 {
        if g.state.grounded {
            break;
        }
        g.step(&Intent {
            x: 1,
            sprint: true,
            ..Default::default()
        });
    }
    assert!(g.player().x > 792 && g.state.grounded);
    assert_eq!(g.player().y, FLOOR - 120 - 30);
}

#[test]
fn old_v1_save_migrates_through_engine_framing_and_keeps_campaign_discoveries() {
    let mut old = Embertrail::new(7);
    old.state.unlocked = 11;
    old.enter_course(11);
    old.state.cleared = 0x7ff;
    old.state.best_relics[4] = 3;
    old.state.score = 12345;
    let mut value = serde_json::to_value(old.capture()).unwrap();
    value.as_object_mut().unwrap().remove("jump_held");
    value.as_object_mut().unwrap().remove("npc_greeted");
    let payload = serde_json::json!({"hash":"0000000000000000", "state": value});
    let header = vesper3d::viewer::savestate::SaveHeader::new("embertrail", 1, "Old campaign");
    let bytes =
        vesper3d::viewer::savestate::encode(&header, &serde_json::to_vec(&payload).unwrap())
            .unwrap();
    let mut new = Embertrail::new(7);
    snapshot::restore(&mut new, &bytes).unwrap();
    assert_eq!(new.state.course, 11);
    assert_eq!(new.state.unlocked, 11);
    assert_eq!(new.state.cleared, 0x7ff);
    assert_eq!(new.state.best_relics[4], 3);
    assert_eq!(new.state.score, 12345);
    assert_eq!(new.player().x, 80);
    assert_eq!(new.state.level.width, levels::build(11).width);
    let saved = snapshot::save(&new, "Migrated").unwrap();
    let mut resumed = Embertrail::new(7);
    snapshot::restore(&mut resumed, &saved).unwrap();
    assert_eq!(new.state_hash(), resumed.state_hash());
}

#[test]
fn story_greetings_are_automatic_and_portal_camera_returns_immediately() {
    let mut g = Embertrail::new(7);
    assert!(g.state.banner.contains("Dawnheart"));
    step(
        &mut g,
        25,
        Intent {
            x: 1,
            ..Default::default()
        },
    );
    assert!(g.state.npc_greeted);
    assert!(g.state.banner.contains("GRANDMA WICK"));
    assert!(g.state.banner.contains("Vesper"));
    let banner = g.state.banner.clone();
    step(&mut g, 10, Intent::default());
    assert_eq!(g.state.banner, banner);
    g.state.px = g.state.level.secret.x * SUB;
    g.state.py = 338 * SUB;
    g.portal(&Intent {
        y: 1,
        ..Default::default()
    });
    assert_eq!(g.state.camera, g.state.level.width + 128);
    g.state.px = g.state.level.secret_return.x * SUB;
    g.state.portal_cooldown = 0;
    g.portal(&Intent {
        y: 1,
        ..Default::default()
    });
    assert!(!g.state.secret_inside);
    assert!(g.state.camera <= g.state.level.width - 800);
}

#[test]
fn a_saved_boss_encounter_resumes_its_tell_attack_and_recovery_exactly() {
    let inputs: Vec<_> = (0..420)
        .map(|t| Intent {
            x: if t % 140 < 70 { 1 } else { -1 },
            action: t % 51 == 0,
            secondary: t % 29 == 0,
            sprint: true,
            ..Default::default()
        })
        .collect();
    let make = || {
        let mut g = at_boss(7);
        g.state.form = 2;
        g.state.lives = 20;
        g.state.shield = 500;
        g
    };
    snapshot::assert_resumes_exactly(make, &inputs, 47);
}

#[test]
fn controller_atlas_grid_and_checkpoint_restart_preserve_discoveries() {
    let mut g = Embertrail::new(7);
    g.state.unlocked = 12;
    g.state.cleared = 3;
    g.state.best_relics[0] = 2;
    g.open_atlas();
    g.step(&Intent {
        y: 1,
        ..Default::default()
    });
    assert_eq!(g.state.map_selection, 4);
    g.step(&Intent::default());
    g.step(&Intent {
        x: 1,
        ..Default::default()
    });
    assert_eq!(g.state.map_selection, 5);
    g.step(&Intent {
        action: true,
        ..Default::default()
    });
    assert_eq!(g.state.course, 5);
    assert!(!g.state.atlas);
    assert!(
        g.state.grounded,
        "Atlas confirmation must not leak into a jump"
    );
    g.state.checkpoint = true;
    g.state.lives = 0;
    g.restart_checkpoint();
    assert_eq!(g.player().x, g.state.level.checkpoint + 20);
    assert_eq!(g.outcome(), "playing");
    assert_eq!(g.state.cleared, 3);
    assert_eq!(g.state.best_relics[0], 2);
}
