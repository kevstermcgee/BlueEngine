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
    assert_eq!(g.take_cues(), vec![Cue::EnemyBounce as usize]);
    g.state.py = 338 * SUB;
    g.state.px = 90 * SUB;
    g.state.vy = 0;
    g.enemy_contacts(g.player());
    assert!(g.state.level.enemies[0].rolling);
    assert_eq!(g.take_cues(), vec![Cue::ShellKick as usize]);
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
    step(&mut g, u32::from(STAIR_TICKS), Intent::default());
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
    step(&mut g, u32::from(STAIR_TICKS), Intent::default());
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
    assert!(g.take_cues().contains(&(Cue::Homecoming as usize)));
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
        let closed_sounds = g.take_cues();
        assert!(closed_sounds.contains(&(Cue::EnemyBounce as usize)));
        assert!(!closed_sounds.contains(&(Cue::BossHit as usize)));
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
            let sounds = g.take_cues();
            assert!(sounds.contains(&(Cue::BossHit as usize)));
            assert!(!sounds.contains(&(Cue::SparkPickup as usize)));
            if remaining == 0 {
                assert!(sounds.contains(&(Cue::GuardianFreed as usize)));
            }
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
        assert!(g.take_cues().contains(&(Cue::CourseClear as usize)));
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
    value["level"].as_object_mut().unwrap().remove("grotto");
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
fn story_greetings_are_automatic_and_portal_camera_changes_while_hidden() {
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
    g.take_cues();
    g.state.px = g.state.level.secret.x * SUB;
    g.state.py = 338 * SUB;
    g.portal(&Intent {
        y: 1,
        ..Default::default()
    });
    step(&mut g, u32::from(STAIR_TICKS), Intent::default());
    assert_eq!(g.take_cues(), vec![Cue::GrottoEnter as usize]);
    assert_eq!(g.state.camera, g.state.level.width + 128);
    g.state.px = g.state.level.secret_return.x * SUB;
    g.state.portal_cooldown = 0;
    g.portal(&Intent {
        y: 1,
        ..Default::default()
    });
    step(&mut g, u32::from(STAIR_TICKS), Intent::default());
    assert_eq!(g.take_cues(), vec![Cue::GrottoExit as usize]);
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

fn measure_release(world: usize, inside: bool, airborne: bool) -> (u32, i32) {
    let mut game = Embertrail::new(7);
    game.state.unlocked = world * 4;
    game.enter_course(world * 4);
    if inside {
        game.state.px = game.state.level.secret.x * SUB;
        game.step(&Intent {
            y: 1,
            ..Default::default()
        });
        step(&mut game, u32::from(STAIR_TICKS), Intent::default());
        assert!(game.state.secret_inside);
    }
    step(
        &mut game,
        16,
        Intent {
            x: 1,
            sprint: true,
            ..Default::default()
        },
    );
    assert_eq!(game.state.vx, 1470);
    if airborne {
        game.step(&Intent {
            x: 1,
            action: true,
            sprint: true,
            ..Default::default()
        });
    }
    let start = game.state.px;
    let mut ticks = 0;
    while game.state.vx != 0 && ticks < 60 {
        game.step(&Intent::default());
        ticks += 1;
    }
    assert_eq!(game.state.vx, 0);
    (ticks, game.state.px - start)
}

#[test]
fn normal_ground_and_air_stop_promptly_while_frost_ice_is_ground_only() {
    let ground = measure_release(0, false, false);
    let air = measure_release(0, false, true);
    let frost_ground = measure_release(3, false, false);
    let frost_air = measure_release(3, false, true);
    let warm_grotto = measure_release(3, true, false);
    assert!(ground.0 <= 3 && ground.1 < 6 * SUB);
    assert!(air.0 <= 5 && air.1 < 12 * SUB);
    assert!(frost_ground.0 > 10 && frost_ground.1 > ground.1 * 4);
    assert_eq!(
        frost_air, air,
        "An ice world must not make the air slippery"
    );
    assert_eq!(
        warm_grotto, ground,
        "Dry warmwell stone has ordinary traction"
    );
    // These prior-release baselines are calculated from its published fixed-step
    // 155/36 braking coefficients at the same 1470 subpixel sprint velocity.
    let report = serde_json::json!({
        "new_measured":{"normal_ground":{"ticks":ground.0,"pixels":ground.1 as f64/SUB as f64},
            "normal_air":{"ticks":air.0,"pixels":air.1 as f64/SUB as f64},
            "frost_ground":{"ticks":frost_ground.0,"pixels":frost_ground.1 as f64/SUB as f64},
            "frost_air":{"ticks":frost_air.0,"pixels":frost_air.1 as f64/SUB as f64},
            "frost_grotto":{"ticks":warm_grotto.0,"pixels":warm_grotto.1 as f64/SUB as f64}},
        "v020_calculated_from_published_coefficients":{"normal_ground":{"ticks":10,"pixels":24.43359375},"normal_air":{"ticks":41,"pixels":114.375}}
    });
    eprintln!("traction {report}");
    if let Ok(path) = std::env::var("EMBERTRAIL_TRACTION_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}

#[test]
fn reversal_is_responsive_and_held_run_jump_keeps_its_full_range() {
    for airborne in [false, true] {
        let mut game = Embertrail::new(7);
        step(
            &mut game,
            16,
            Intent {
                x: 1,
                sprint: true,
                ..Default::default()
            },
        );
        if airborne {
            game.step(&Intent {
                x: 1,
                action: true,
                sprint: true,
                ..Default::default()
            });
        }
        for _ in 0..if airborne { 4 } else { 3 } {
            game.step(&Intent {
                x: -1,
                sprint: true,
                ..Default::default()
            });
        }
        assert!(
            game.state.vx < 0,
            "Reverse direction promptly, including in the air"
        );
        assert_eq!(game.state.facing, -1);
    }
    let mut game = Embertrail::new(7);
    game.state.level.tiles = vec![Tile {
        r: Rect::new(0, FLOOR, 2000, 96),
        kind: 0,
        content: 0,
    }];
    game.state.level.pickups.clear();
    game.state.level.enemies.clear();
    game.state.level.platforms.clear();
    step(
        &mut game,
        16,
        Intent {
            x: 1,
            sprint: true,
            ..Default::default()
        },
    );
    let start_x = game.state.px;
    let mut apex = game.player().y;
    game.step(&Intent {
        x: 1,
        action: true,
        sprint: true,
        ..Default::default()
    });
    let mut ticks = 1;
    while !game.state.grounded && ticks < 80 {
        game.step(&Intent {
            x: 1,
            sprint: true,
            ..Default::default()
        });
        apex = apex.min(game.player().y);
        ticks += 1;
    }
    assert_eq!(ticks, 51, "The established jump timing is unchanged");
    assert_eq!(game.state.px - start_x, 1470 * 51);
    assert!((147..=149).contains(&(338 - apex)));
}

#[test]
fn every_hidden_room_has_unique_authored_geometry_and_live_movers() {
    let mut layouts = std::collections::HashSet::new();
    for course in 0..COURSES {
        let mut game = Embertrail::new(7);
        game.state.unlocked = course;
        game.enter_course(course);
        let room = game.state.level.grotto.clone();
        assert_eq!(room.archetype as usize, course / 4);
        assert_eq!(room.variant as usize, course % 4);
        let terrain: Vec<_> = game
            .state
            .level
            .tiles
            .iter()
            .filter(|t| t.r.x >= room.bounds.x)
            .map(|t| (t.r.x - room.bounds.x, t.r.y, t.r.w, t.r.h, t.kind))
            .collect();
        assert!(
            layouts.insert(hash_json(&terrain)),
            "Hidden layout repeats at course {course}"
        );
        assert!(room
            .bounds
            .contains(Point::new(game.state.level.secret_return.x, 338)));
        assert!(game
            .state
            .level
            .tiles
            .iter()
            .any(|t| t.kind == 0 && t.r.contains(Point::new(room.entrance.x, FLOOR))));
        assert!(game.state.level.tiles.iter().any(|t| t.kind == 0
            && t.r
                .contains(Point::new(game.state.level.secret_return.x, FLOOR))));
        let before: Vec<_> = game
            .state
            .level
            .platforms
            .iter()
            .filter(|p| p.home.x >= room.bounds.x)
            .map(|p| p.r)
            .collect();
        assert!(!before.is_empty());
        game.state.px = game.state.level.secret.x * SUB;
        game.step(&Intent {
            y: 1,
            ..Default::default()
        });
        step(&mut game, u32::from(STAIR_TICKS), Intent::default());
        assert!(game.state.secret_inside);
        let mut moved = false;
        for _ in 0..53 {
            game.step(&Intent::default());
            let after: Vec<_> = game
                .state
                .level
                .platforms
                .iter()
                .filter(|p| p.home.x >= room.bounds.x)
                .map(|p| p.r)
                .collect();
            moved |= before != after;
        }
        assert!(
            moved,
            "A hidden room must actually move during the observed interval"
        );
        let save = snapshot::save(&game, "Inside a live hidden room").unwrap();
        let mut resumed = Embertrail::new(7);
        snapshot::restore(&mut resumed, &save).unwrap();
        for tick in 0..90 {
            let input = Intent {
                x: if tick % 48 < 24 { 1 } else { -1 },
                action: tick % 47 == 0,
                ..Default::default()
            };
            game.step(&input);
            resumed.step(&input);
            assert_eq!(game.state_hash(), resumed.state_hash());
        }
        game.state.px = game.grotto_entry_gate().x * SUB;
        game.state.py = 338 * SUB;
        game.state.grounded = true;
        game.state.vx = 0;
        game.state.vy = 0;
        game.state.portal_cooldown = 0;
        game.step(&Intent {
            y: 1,
            ..Default::default()
        });
        step(&mut game, u32::from(STAIR_TICKS), Intent::default());
        assert!(
            !game.state.secret_inside,
            "The entry stairs always offer a safe return"
        );
    }
}

#[test]
fn springs_launch_once_with_a_distinct_cue_and_room_death_keeps_the_checkpoint() {
    let mut game = Embertrail::new(7);
    game.state.level.tiles.push(Tile {
        r: Rect::new(72, FLOOR - 4, 44, 8),
        kind: 5,
        content: 0,
    });
    game.step(&Intent::default());
    assert_eq!(game.state.vy, -15 * SUB);
    assert!(!game.state.grounded);
    assert_eq!(game.take_cues(), vec![Cue::Spring as usize]);
    game.step(&Intent::default());
    assert!(!game.take_cues().contains(&(Cue::Spring as usize)));
    let save = snapshot::save(&game, "Spring launch").unwrap();
    let mut resumed = Embertrail::new(7);
    snapshot::restore(&mut resumed, &save).unwrap();
    assert_eq!(game.state_hash(), resumed.state_hash());
    game.state.checkpoint = true;
    game.state.secret_inside = true;
    game.hurt(true);
    step(&mut game, 90, Intent::default());
    assert!(!game.state.secret_inside);
    assert_eq!(game.player().x, game.state.level.checkpoint + 20);
}

#[test]
fn interaction_cues_follow_the_actual_object_or_player_event() {
    for (kind, cue) in [
        (0, Cue::SparkPickup),
        (1, Cue::BloomUnfurl),
        (2, Cue::EmberIgnite),
        (3, Cue::InvincibilityShimmer),
        (4, Cue::ExtraLife),
        (5, Cue::ShardPickup),
    ] {
        let mut game = Embertrail::new(7);
        game.state.level.pickups = vec![Pickup {
            r: game.player(),
            kind,
            active: true,
        }];
        game.collect();
        assert_eq!(game.take_cues(), vec![cue as usize]);
    }
    let mut game = Embertrail::new(7);
    game.state.sparks = 99;
    game.state.level.pickups = vec![Pickup {
        r: game.player(),
        kind: 0,
        active: true,
    }];
    game.collect();
    assert_eq!(game.take_cues(), vec![Cue::ExtraLife as usize]);
    let mut game = Embertrail::new(7);
    game.state.px = 514 * SUB;
    game.state.py = 294 * SUB;
    game.state.vy = -3 * SUB;
    game.step(&Intent::default());
    assert_eq!(game.take_cues(), vec![Cue::BlockReveal as usize]);
    game.state.form = 1;
    game.state.px = 482 * SUB;
    game.state.py = 294 * SUB;
    game.state.vy = -3 * SUB;
    game.step(&Intent::default());
    assert_eq!(game.take_cues(), vec![Cue::ClayBreak as usize]);
    game.state.px = 80 * SUB;
    game.state.py = 338 * SUB;
    game.state.vy = 0;
    game.state.grounded = true;
    game.state.form = 2;
    game.step(&Intent {
        action: true,
        secondary: true,
        ..Default::default()
    });
    let sounds = game.take_cues();
    assert!(sounds.contains(&(Cue::Jump as usize)) && sounds.contains(&(Cue::SeedCast as usize)));
    game.state.shield = 0;
    game.hurt(false);
    assert_eq!(game.take_cues(), vec![Cue::Hurt as usize]);
    game.state.shield = 0;
    game.state.form = 0;
    game.hurt(false);
    assert_eq!(game.take_cues(), vec![Cue::Death as usize]);
}

#[test]
fn published_v2_payloads_migrate_surface_and_inside_saves_through_engine_framing() {
    for inside in [false, true] {
        let mut old = Embertrail::new(7);
        old.state.unlocked = 13;
        old.enter_course(13);
        old.state.cleared = 0x1fff;
        old.state.best_relics[4] = 3;
        old.state.score = 54321;
        old.state.sparks = 76;
        old.state.relics = 2;
        old.state.checkpoint = true;
        old.state.timer = 12345;
        old.state.level.pickups[0].active = false;
        // Published 0.2.0 used this exact small vault and had no spring kind or
        // Grotto metadata. Exercise its actual old payload shape, not a v3 room.
        let width = old.state.level.width;
        old.state.level.tiles.retain(|p| p.r.x < width);
        for (r, kind) in [
            (Rect::new(width + 128, FLOOR, 736, 96), 0),
            (Rect::new(width + 96, 80, 32, 384), 1),
            (Rect::new(width + 864, 80, 32, 384), 1),
            (Rect::new(width + 128, 80, 736, 32), 1),
            (Rect::new(width + 600, 306, 64, 16), 1),
        ] {
            old.state.level.tiles.push(Tile {
                r,
                kind,
                content: 0,
            });
        }
        old.state.level.pickups.retain(|p| p.r.x < width);
        for row in 0..3 {
            for col in 0..7 {
                old.state.level.pickups.push(Pickup {
                    r: Rect::new(width + 280 + col * 52, 208 + row * 44, 16, 20),
                    kind: 0,
                    active: true,
                });
            }
        }
        old.state.level.pickups.push(Pickup {
            r: Rect::new(width + 620, 264, 16, 20),
            kind: 4,
            active: true,
        });
        old.state.level.enemies.retain(|p| p.r.x < width);
        old.state.level.platforms.retain(|p| p.home.x < width);
        old.state.level.hazards.retain(|p| p.x < width);
        old.state.px = if inside {
            (old.state.level.width + 600) * SUB
        } else {
            700 * SUB
        };
        old.state.secret_inside = inside;
        old.state.level.secret_used = inside;
        old.state.camera = if inside { width + 128 } else { 420 };
        old.state.level.secret_return = Rect::new(old.state.level.width + 800, 320, 40, 48);
        let mut value = serde_json::to_value(old.capture()).unwrap();
        value["level"].as_object_mut().unwrap().remove("grotto");
        let payload = serde_json::json!({"hash":"0000000000000000","state":value});
        let header =
            vesper3d::viewer::savestate::SaveHeader::new("embertrail", 2, "Published 0.2.0 save");
        let bytes =
            vesper3d::viewer::savestate::encode(&header, &serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let mut game = Embertrail::new(7);
        snapshot::restore(&mut game, &bytes).unwrap();
        assert_eq!(game.state.course, 13);
        assert_eq!(game.state.unlocked, 13);
        assert_eq!(game.state.cleared, 0x1fff);
        assert_eq!(game.state.best_relics[4], 3);
        assert_eq!(game.state.score, 54321);
        assert_eq!(game.state.sparks, 76);
        assert_eq!(game.state.relics, 2);
        assert!(game.state.checkpoint);
        assert_eq!(game.state.timer, 12345);
        assert!(!game.state.level.pickups[0].active);
        assert_eq!(game.state.secret_inside, inside);
        assert_eq!(
            game.player().x,
            if inside {
                game.state.level.grotto.entrance.x
            } else {
                700
            }
        );
        let current = snapshot::save(&game, "Migrated v3").unwrap();
        let mut resumed = Embertrail::new(7);
        snapshot::restore(&mut resumed, &current).unwrap();
        assert_eq!(game.state_hash(), resumed.state_hash());
    }
    let mut game = Embertrail::new(7);
    let mut invalid = game.capture();
    invalid.level.grotto.bounds.x = i32::MAX;
    assert!(
        game.restore(invalid).is_err(),
        "Malformed room metadata must be rejected before bound arithmetic"
    );
}
#[test]
fn moving_platform_carry_never_embeds_rider_in_authored_stationary_geometry() {
    let mut cases = 0;
    let mut receipt = None;
    for course in 0..COURSES {
        let level = levels::build(course);
        let solids: Vec<_> = level
            .tiles
            .iter()
            .filter(|tile| tile.r.w > 0 && tile.kind != 5)
            .map(|tile| tile.r)
            .collect();
        for platform in &level.platforms {
            for tick in 1..=240 {
                let at = |tick: u32| {
                    let t = (tick + platform.phase) % 240;
                    let n = if t < 120 { t } else { 240 - t } as i32;
                    Rect::new(
                        platform.home.x
                            + if platform.vertical {
                                0
                            } else {
                                n * platform.span / 120
                            },
                        platform.home.y
                            - if platform.vertical {
                                n * platform.span / 120
                            } else {
                                0
                            },
                        platform.r.w,
                        platform.r.h,
                    )
                };
                let previous = at(tick - 1);
                let next = at(tick);
                for x in (previous.x - 20..previous.x + previous.w).step_by(4) {
                    let rider = Rect::new(x, previous.y - 30, 22, 30);
                    if solids.iter().any(|tile| tile.overlaps(rider)) {
                        continue;
                    }
                    let carried = Rect::new(
                        rider.x + next.x - previous.x,
                        rider.y + next.y - previous.y,
                        rider.w,
                        rider.h,
                    );
                    if !solids.iter().any(|tile| tile.overlaps(carried)) {
                        continue;
                    }
                    let mut game = Embertrail::new(7);
                    game.enter_course(course);
                    let mut mover = platform.clone();
                    mover.r = previous;
                    mover.previous = previous;
                    game.state.level.platforms = vec![mover];
                    game.state.px = rider.x * SUB;
                    game.state.py = rider.y * SUB;
                    game.state.grounded = true;
                    game.state.tick = tick;
                    game.move_platforms();
                    cases += 1;
                    assert!(
                        !solids.iter().any(|tile| tile.overlaps(game.player())),
                        "course={} tick={} platform={:?} rider={rider:?} carried={:?}",
                        course + 1,
                        tick,
                        platform.home,
                        game.player()
                    );
                    if receipt.is_none() {
                        let after = game.player();
                        let wall = *solids.iter().find(|tile| tile.overlaps(carried)).unwrap();
                        let input = Intent {
                            x: (previous.x - next.x).signum(),
                            sprint: true,
                            ..Default::default()
                        };
                        step(&mut game, 8, input);
                        assert!(!solids.iter().any(|tile| tile.overlaps(game.player())));
                        assert!(
                            (game.player().x - after.x).abs() > 8,
                            "The rider must immediately be able to move away"
                        );
                        receipt = Some(
                            serde_json::json!({"course":course+1,"tick":tick,"platform_home":platform.home,"original_rect":rider,"delta":{"x":next.x-previous.x,"y":next.y-previous.y},"stationary_tile":wall,"old_unchecked_result":carried,"old_overlap":true,"postfix_result":after,"postfix_overlap":false,"escape_input":{"x":input.x,"sprint":true,"ticks":8},"escape_result":game.player(),"escape_overlap":false,"scope":"Authored blocked carry contact fixture; escape uses public input. Not a continuous campaign run."}),
                        );
                    }
                }
            }
        }
    }
    assert!(
        cases > 0,
        "The regression must exercise authored blocked carry"
    );
    if let Some(path) = std::env::var_os("EMBERTRAIL_PLATFORM_REPORT") {
        let mut receipt = receipt.unwrap();
        receipt["authored_blocked_contacts_tested"] = cases.into();
        std::fs::write(path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    }
}

#[test]
fn rising_lift_pauses_below_a_ceiling_and_resumes_without_crushing_or_catching_up() {
    let mut game = Embertrail::new(7);
    game.state.level.tiles = vec![
        Tile {
            r: Rect::new(0, FLOOR, 800, 96),
            kind: 0,
            content: 0,
        },
        Tile {
            r: Rect::new(80, 240, 120, 20),
            kind: 1,
            content: 0,
        },
    ];
    game.state.level.enemies.clear();
    game.state.level.pickups.clear();
    game.state.level.platforms = vec![Platform {
        r: Rect::new(80, 332, 80, 14),
        previous: Rect::new(80, 332, 80, 14),
        home: Point::new(80, 332),
        span: 160,
        vertical: true,
        phase: 0,
    }];
    game.state.px = 100 * SUB;
    game.state.py = 302 * SUB;
    for _ in 0..100 {
        game.step(&Intent::default());
        assert!(!game
            .state
            .level
            .tiles
            .iter()
            .any(|t| t.r.overlaps(game.player())));
        assert!(game.state.level.platforms[0].r.y >= game.player().y + game.player().h);
    }
    assert_eq!(game.player().y, 260);
    let stopped = game.state.level.platforms[0].r;
    for _ in 0..28 {
        game.step(&Intent {
            x: 1,
            sprint: true,
            ..Default::default()
        });
        assert!(!game
            .state
            .level
            .tiles
            .iter()
            .any(|t| t.r.overlaps(game.player())));
        let platform = &game.state.level.platforms[0];
        assert!(
            (platform.r.y - platform.previous.y).abs() <= 2,
            "No catch-up teleport after a blocked lift"
        );
    }
    assert!(
        game.player().x > 200,
        "The player can leave the stalled lift immediately"
    );
    assert_ne!(game.state.level.platforms[0].r, stopped);
}

#[test]
fn rising_platform_sweeps_a_falling_actor_but_does_not_cancel_an_ascending_jump() {
    for vy in [SUB, -SUB] {
        let mut game = Embertrail::new(7);
        game.state.level.tiles.clear();
        game.state.level.enemies.clear();
        game.state.level.platforms = vec![Platform {
            r: Rect::new(80, 332, 80, 14),
            previous: Rect::new(80, 332, 80, 14),
            home: Point::new(80, 332),
            span: 240,
            vertical: true,
            phase: 0,
        }];
        game.state.tick = 1;
        game.state.px = 100 * SUB;
        game.state.py = 301 * SUB;
        game.state.vy = vy;
        game.state.grounded = false;
        game.move_platforms();
        if vy > 0 {
            assert_eq!(game.player().y, 300);
            assert!(game.state.grounded);
            assert_eq!(game.state.vy, 0);
        } else {
            assert_eq!(game.player().y, 301);
            assert_eq!(game.state.vy, vy);
            assert!(!game.state.grounded);
        }
    }
}

#[test]
fn stairs_lock_motion_swap_only_at_midpoint_and_require_a_fresh_down_press() {
    let mut game = Embertrail::new(7);
    game.state.px = game.state.level.secret.x * SUB;
    game.state.py = 338 * SUB;
    let source = game.player();
    let source_camera = game.state.camera;
    let tick = game.state.tick;
    let timer = game.state.timer;
    let down = Intent {
        y: 1,
        x: 1,
        action: true,
        secondary: true,
        ..Default::default()
    };
    game.step(&down);
    assert_eq!(game.state.stairs.as_ref().unwrap().elapsed, 0);
    assert_eq!(game.take_cues(), vec![Cue::GrottoEnter as usize]);
    for elapsed in 1..STAIR_TICKS {
        game.step(&down);
        assert_eq!(game.state.stairs.as_ref().unwrap().elapsed, elapsed);
        assert_eq!(game.state.tick, tick);
        assert_eq!(game.state.timer, timer);
        assert_eq!((game.state.vx, game.state.vy), (0, 0));
        assert!(game.state.shots.is_empty());
        if elapsed < STAIR_TICKS / 2 {
            assert!(!game.state.secret_inside);
            assert_eq!(game.player(), source);
            assert_eq!(game.state.camera, source_camera);
            assert_eq!(game.stair_transition_gate(), Some(game.state.level.secret));
        } else {
            assert!(game.state.secret_inside);
            assert_eq!(game.player().x, game.state.level.grotto.entrance.x);
            assert_eq!(game.state.camera, game.state.level.grotto.bounds.x);
            assert_eq!(game.stair_transition_gate(), Some(game.grotto_entry_gate()));
        }
    }
    game.step(&down);
    assert!(game.state.stairs.is_none());
    assert!(game.state.secret_inside);
    // Explicit return fixture now tests a held Down cannot start another stair.
    game.state.px = game.grotto_entry_gate().x * SUB;
    game.state.py = 338 * SUB;
    step(
        &mut game,
        65,
        Intent {
            y: 1,
            ..Default::default()
        },
    );
    assert!(game.state.stairs.is_none() && game.state.secret_inside);
    game.step(&Intent::default());
    game.step(&Intent {
        y: 1,
        ..Default::default()
    });
    assert!(game.state.stairs.is_some());
    step(
        &mut game,
        u32::from(STAIR_TICKS),
        Intent {
            y: 1,
            ..Default::default()
        },
    );
    assert!(!game.state.secret_inside && game.state.stairs.is_none());
    step(
        &mut game,
        65,
        Intent {
            y: 1,
            ..Default::default()
        },
    );
    assert!(!game.state.secret_inside && game.state.stairs.is_none());
}

#[test]
fn stair_saves_resume_exactly_in_both_halves_and_real_v3_saves_migrate() {
    for exiting in [false, true] {
        let mut game = Embertrail::new(7);
        game.state.secret_inside = exiting;
        game.state.px = if exiting {
            game.grotto_entry_gate().x
        } else {
            game.state.level.secret.x
        } * SUB;
        game.state.py = 338 * SUB;
        game.step(&Intent {
            y: 1,
            ..Default::default()
        });
        for elapsed in 0..STAIR_TICKS {
            let save = snapshot::save(&game, "During stairs").unwrap();
            let mut copy = Embertrail::new(7);
            snapshot::restore(&mut copy, &save).unwrap();
            assert_eq!(copy.state_hash(), game.state_hash());
            let input = Intent {
                y: 1,
                action: elapsed % 2 == 0,
                ..Default::default()
            };
            game.step(&input);
            copy.step(&input);
            assert_eq!(copy.state_hash(), game.state_hash());
        }
        assert!(game.state.stairs.is_none());
    }
    for inside in [false, true] {
        let mut old = Embertrail::new(7);
        old.state.unlocked = 11;
        old.enter_course(11);
        old.state.score = 12345;
        old.state.cleared = 0x7ff;
        old.state.checkpoint = true;
        old.state.secret_inside = inside;
        old.state.level.secret_used = inside;
        if inside {
            old.state.px = old.state.level.grotto.entrance.x * SUB;
            old.state.camera = old.state.level.grotto.bounds.x;
        } else {
            old.state
                .level
                .enemies
                .iter_mut()
                .find(|e| e.kind == 4)
                .unwrap()
                .phase = 30;
        }
        let mut value = serde_json::to_value(old.capture()).unwrap();
        for key in ["stairs", "down_held", "boss_seen"] {
            value.as_object_mut().unwrap().remove(key);
        }
        let old_value = value.clone();
        let payload = serde_json::json!({"hash":"0000000000000000","state":value});
        let header =
            vesper3d::viewer::savestate::SaveHeader::new("embertrail", 3, "Published 0.2.1");
        let bytes =
            vesper3d::viewer::savestate::encode(&header, &serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let mut migrated = Embertrail::new(7);
        snapshot::restore(&mut migrated, &bytes).unwrap();
        let mut current = serde_json::to_value(migrated.capture()).unwrap();
        for key in ["stairs", "down_held", "boss_seen"] {
            current.as_object_mut().unwrap().remove(key);
        }
        assert_eq!(
            current, old_value,
            "Every released v3 state field remains exact"
        );
        assert_eq!(migrated.state.boss_seen, !inside);
        let saved = snapshot::save(&migrated, "Migrated v4").unwrap();
        let mut resumed = Embertrail::new(7);
        snapshot::restore(&mut resumed, &saved).unwrap();
        assert_eq!(migrated.state_hash(), resumed.state_hash());
    }
}

#[test]
fn boss_theme_latches_at_the_first_visible_pixel_and_survives_the_atlas() {
    let mut game = Embertrail::new(7);
    game.state.unlocked = 3;
    game.enter_course(3);
    let boss = game.boss_view_bounds(
        game.state
            .level
            .enemies
            .iter()
            .find(|e| e.kind == 4)
            .unwrap(),
    );
    game.state.camera = boss.x - 800;
    assert!(!game.boss_on_screen() && !game.boss_battle_active());
    game.state.camera += 1;
    assert!(game.boss_on_screen() && game.boss_battle_active());
    assert!(
        !game.boss_active(),
        "Music starts before the old arena phase trigger"
    );
    game.state.boss_seen = true;
    game.state.camera = 0;
    assert!(!game.boss_on_screen() && game.boss_battle_active());
    game.state.atlas = true;
    assert!(game.boss_battle_active());
    game.state.atlas = false;
    game.state.dead = 10;
    assert!(!game.boss_battle_active());
    game.state.dead = 0;
    game.state.secret_inside = true;
    assert!(!game.boss_battle_active());
    game.state.secret_inside = false;
    game.state
        .level
        .enemies
        .iter_mut()
        .find(|e| e.kind == 4)
        .unwrap()
        .active = false;
    assert!(!game.boss_battle_active());
    game.restart_checkpoint();
    assert!(!game.state.boss_seen);
}

#[test]
fn real_v3_embedded_platform_save_recovers_one_pixel_without_resetting_the_course() {
    let mut old = Embertrail::new(7);
    old.state.unlocked = 5;
    old.enter_course(1);
    old.state.tick = 175;
    old.state.px = 1167 * SUB;
    old.state.py = 286 * SUB;
    old.state.score = 54321;
    old.state.sparks = 72;
    old.state.cleared = 1;
    old.state.best_relics[0] = 3;
    old.state.checkpoint = true;
    old.state.level.pickups[0].active = false;
    let solids: Vec<_> = old
        .state
        .level
        .tiles
        .iter()
        .filter(|t| t.kind != 5 && t.r.w > 0)
        .map(|t| t.r)
        .collect();
    assert!(solids.iter().any(|r| r.overlaps(old.player())));
    let mut value = serde_json::to_value(old.capture()).unwrap();
    for key in ["stairs", "down_held", "boss_seen"] {
        value.as_object_mut().unwrap().remove(key);
    }
    let header =
        vesper3d::viewer::savestate::SaveHeader::new("embertrail", 3, "Published embedded carry");
    let bytes = vesper3d::viewer::savestate::encode(
        &header,
        &serde_json::to_vec(&serde_json::json!({"hash":"0000000000000000","state":value})).unwrap(),
    )
    .unwrap();
    let mut game = Embertrail::new(7);
    snapshot::restore(&mut game, &bytes).unwrap();
    assert_eq!(game.player().x, 1168);
    assert_eq!(game.player().y, 286);
    assert!(!solids.iter().any(|r| r.overlaps(game.player())));
    assert_eq!(game.state.score, 54321);
    assert_eq!(game.state.sparks, 72);
    assert_eq!(game.state.cleared, 1);
    assert_eq!(game.state.best_relics[0], 3);
    assert!(game.state.checkpoint);
    assert!(!game.state.level.pickups[0].active);
    let position = game.player();
    step(
        &mut game,
        8,
        Intent {
            x: 1,
            sprint: true,
            ..Default::default()
        },
    );
    assert!(game.player().x > position.x + 8);
    assert!(!solids.iter().any(|r| r.overlaps(game.player())));
}
