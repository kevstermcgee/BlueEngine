//! Development traversal search. Every branch advances only through public Intent values.
//! This is a reachability probe, not an unattended campaign completion claim.
//!
//! Search an isolated, normally initialized course (zero-based index):
//! `cargo run --locked --no-default-features --example route_probe -- 31 900`
//! Replay its saved inputs from a fresh course, independently checking completion:
//! `cargo run --locked --no-default-features --example route_probe -- --replay 31 .blue-check/route-32.json`
//! Each route entry holds direction and a jump edge for eight simulation ticks;
//! sprint is held and casting is attempted once per entry. Search restores only
//! genuine prior branch states. Replays never change health, position or enemies.
//! Individual course initialization is not evidence of contiguous campaign completion.
use embertrail::{Embertrail, State, SUB};
use std::collections::HashSet;
use vesper3d::two_d::{GameLogic, Intent, Simulation};

#[derive(Clone)]
struct Node {
    state: State,
    route: Vec<(i32, bool)>,
    score: i64,
}
fn replay(course: usize, route: &[(i32, bool)]) -> Embertrail {
    let mut game = Embertrail::new(7);
    game.state.unlocked = course;
    game.enter_course(course);
    for &(x, jump) in route {
        for tick in 0..8 {
            game.step(&Intent {
                x,
                action: jump && tick == 0,
                sprint: true,
                secondary: tick == 0,
                ..Default::default()
            });
            assert!(
                game.state.lives >= 5 && game.state.dead == 0,
                "The replay lost a life"
            );
            if game.state.transition > 0 {
                break;
            }
        }
        if game.state.transition > 0 {
            break;
        }
    }
    assert_ne!(
        game.state.cleared & (1 << course),
        0,
        "The recorded public route did not clear its course"
    );
    assert!(
        game.state
            .level
            .enemies
            .iter()
            .all(|e| e.kind != 4 || !e.active),
        "The guardian remains active"
    );
    if course == embertrail::COURSES - 1 {
        for _ in 0..150 {
            game.step(&Intent::default());
        }
        assert_eq!(game.outcome(), "won");
    }
    game
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|v| v == "--replay") {
        let course = args
            .get(2)
            .expect("Course index required")
            .parse()
            .expect("Invalid course");
        let route: Vec<(i32, bool)> = serde_json::from_slice(
            &std::fs::read(args.get(3).expect("Route path required")).expect("Cannot read route"),
        )
        .expect("Invalid route");
        let game = replay(course, &route);
        println!(
            "{}",
            serde_json::json!({"course":course + 1,"ticks":game.state.tick,"lives":game.state.lives,"outcome":game.outcome(),"hash":format!("{:016x}",game.state_hash()),"public_inputs_only":true,"initialization":"isolated course entrance"})
        );
        return;
    }
    let course: usize = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(0);
    let limit: usize = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(260);
    let mut g = Embertrail::new(7);
    g.state.unlocked = course;
    g.enter_course(course);
    let initial_lives = g.state.lives;
    let initial_hp: i32 = g
        .state
        .level
        .enemies
        .iter()
        .filter(|e| e.kind == 4)
        .map(|e| e.hp as i32)
        .sum();
    let mut beam = vec![Node {
        state: g.state.clone(),
        route: Vec::new(),
        score: 0,
    }];
    let mut furthest = 0;
    for depth in 0..limit {
        let mut next = Vec::new();
        for n in &beam {
            for x in [1, 0, -1] {
                for jump in [false, true] {
                    g.state = n.state.clone();
                    let _ = g.take_cues();
                    for t in 0..8 {
                        g.step(&Intent {
                            x,
                            action: jump && t == 0,
                            sprint: true,
                            secondary: t == 0,
                            ..Default::default()
                        });
                        if g.state.dead > 0 || g.state.transition > 0 {
                            break;
                        }
                    }
                    if g.state.dead > 0 || g.state.lives < initial_lives || g.state.secret_inside {
                        continue;
                    }
                    let mut route = n.route.clone();
                    route.push((x, jump));
                    if g.state.transition > 0 {
                        let folder = std::env::var_os("EMBERTRAIL_PROBE_DIR")
                            .map(std::path::PathBuf::from)
                            .unwrap_or_else(|| ".blue-check".into());
                        let path = folder.join(format!("route-{:02}.json", course + 1));
                        std::fs::create_dir_all(&folder).unwrap();
                        std::fs::write(&path, serde_json::to_vec(&route).unwrap()).unwrap();
                        let replayed = replay(course, &route);
                        let proof = serde_json::json!({"course":course + 1,"ticks":replayed.state.tick,"lives":replayed.state.lives,"outcome":replayed.outcome(),"hash":format!("{:016x}",replayed.state_hash()),"route":path,"public_inputs_only":true,"independent_replay":true,"initialization":"isolated course entrance"});
                        std::fs::write(
                            folder.join(format!("route-{:02}-proof.json", course + 1)),
                            serde_json::to_vec_pretty(&proof).unwrap(),
                        )
                        .unwrap();
                        println!(
                            "course={} cleared ticks<={} lives={} route={}",
                            course + 1,
                            (depth + 1) * 8,
                            g.state.lives,
                            path.display()
                        );
                        return;
                    }
                    let px = g.state.px / SUB;
                    furthest = furthest.max(px);
                    let hp: i32 = g
                        .state
                        .level
                        .enemies
                        .iter()
                        .filter(|e| e.kind == 4 && e.active)
                        .map(|e| e.hp as i32)
                        .sum();
                    let boss = g
                        .state
                        .level
                        .enemies
                        .iter()
                        .find(|e| e.kind == 4 && e.active && e.phase > 0);
                    let score = if let Some(boss) = boss {
                        let distance = (px + 11 - boss.r.x - boss.r.w / 2).abs();
                        (g.state.level.width - embertrail::levels::BOSS_COURT) as i64 * 10
                            // Entering a court must increase the objective. The old
                            // distance penalty made waiting just outside score higher
                            // than every first encounter state, starving the search.
                            + 20_000 + (initial_hp - hp) as i64 * 9000
                            - distance as i64 * 8
                            + (boss.r.y - g.state.py / SUB).clamp(0, 130) as i64 * 4
                            + g.state.form as i64 * 300
                    } else {
                        px as i64 * 10
                            + (initial_hp - hp) as i64 * 9000
                            + if initial_hp > 0 && hp == 0 { 20_000 } else { 0 }
                            + g.state.form as i64 * 130
                            + (338 - g.state.py / SUB) as i64 / 2
                    };
                    next.push(Node {
                        state: g.state.clone(),
                        route,
                        score,
                    });
                }
            }
        }
        next.sort_unstable_by_key(|n| -n.score);
        let mut seen = HashSet::new();
        next.retain(|n| {
            let hp: u8 = n
                .state
                .level
                .enemies
                .iter()
                .filter(|e| e.kind == 4 && e.active)
                .map(|e| e.hp)
                .sum();
            let boss = n
                .state
                .level
                .enemies
                .iter()
                .find(|e| e.kind == 4 && e.active);
            seen.insert((
                n.state.px / (SUB * 10),
                n.state.py / (SUB * 10),
                n.state.vx / 256,
                n.state.vy / 512,
                n.state.form,
                hp,
                n.state.shield / 30,
                boss.map(|e| (e.r.x / 20, e.r.y / 20, e.phase / 12)),
            ))
        });
        next.truncate(64);
        if std::env::var_os("EMBERTRAIL_PROBE_TRACE").is_some() && depth % 60 == 59 {
            if let Some(best) = next.first() {
                eprintln!(
                    "depth={} x={} y={} form={} boss={:?} score={}",
                    depth + 1,
                    best.state.px / SUB,
                    best.state.py / SUB,
                    best.state.form,
                    best.state
                        .level
                        .enemies
                        .iter()
                        .find(|e| e.kind == 4 && e.active)
                        .map(|e| (e.hp, e.phase, e.r.x)),
                    best.score
                );
            }
        }
        beam = next;
        if beam.is_empty() {
            break;
        }
    }
    eprintln!("course={} search incomplete furthest={} (bounded search failure does not prove unreachable)", course+1, furthest);
    std::process::exit(2);
}
