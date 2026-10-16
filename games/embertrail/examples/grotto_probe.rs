//! Isolated root-room reachability: public Down entry, reward, then far-stair exit.
//! The initial fixture places Mote at the real surface entrance. It does not
//! establish access from the course start or a contiguous campaign playthrough.
use embertrail::{Embertrail, State, FLOOR, SUB};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use vesper3d::two_d::{GameLogic, Intent, Simulation};

#[derive(Clone, Copy, Serialize, Deserialize)]
struct Command {
    x: i32,
    jump: bool,
    down: bool,
}
#[derive(Clone)]
struct Node {
    state: State,
    route: Vec<Command>,
    score: i64,
}
fn start(course: usize) -> Embertrail {
    let mut game = Embertrail::new(7);
    game.state.unlocked = course;
    game.enter_course(course);
    // Let ordinary spawn protection expire through real ticks before placing
    // this isolated-room fixture, so it cannot hide an entrance enemy hit.
    for _ in 0..120 {
        game.step(&Intent::default());
    }
    assert_eq!(game.state.shield, 0, "Spawn protection did not expire");
    assert_eq!(game.state.invincible, 0);
    assert_eq!(game.state.lives, 5, "The entrance setup lost a life");
    let entrance = game.state.level.secret;
    // Explicit scenario setup, before the room route. No changes assist play
    // after this point: geometry, health, enemies and moving objects stay real.
    game.state.px = (entrance.x + entrance.w / 2 - game.player().w / 2) * SUB;
    game.state.py = (FLOOR - game.player().h) * SUB;
    game.step(&Intent {
        y: 1,
        ..Default::default()
    });
    assert!(
        game.state.secret_inside,
        "Down did not enter the real root opening"
    );
    assert_eq!(game.state.shield, 0, "Room entry added spawn protection");
    assert!(
        !reward_taken(&game.state),
        "The room must begin with a real reward"
    );
    game
}
fn reward_taken(state: &State) -> bool {
    let reward = state.level.grotto.reward;
    state
        .level
        .pickups
        .iter()
        .any(|p| !p.active && p.kind == 4 && p.r == reward)
}
fn advance(game: &mut Embertrail, command: Command) -> bool {
    let _ = game.take_cues();
    for tick in 0..4 {
        let at_far_stairs = game.state.level.secret_return.overlaps(game.player());
        game.step(&Intent {
            x: command.x,
            y: i32::from(command.down),
            action: command.jump && tick == 0,
            sprint: true,
            ..Default::default()
        });
        if !game.state.secret_inside {
            assert!(command.down, "The room exited without public Down input");
            return at_far_stairs;
        }
        if game.state.dead > 0 || !game.state.secret_inside {
            break;
        }
    }
    false
}
fn replay(course: usize, route: &[Command]) -> Embertrail {
    let mut game = start(course);
    let mut far_exit = false;
    for command in route {
        assert!(
            game.state.secret_inside,
            "The route continued after exiting"
        );
        far_exit |= advance(&mut game, *command);
        assert_eq!(game.state.dead, 0, "The replay died");
        assert!(game.state.lives >= 5, "The replay lost a life");
    }
    assert!(
        reward_taken(&game.state),
        "The room's reward was not collected"
    );
    assert!(!game.state.secret_inside, "The far stairs were not used");
    assert!(far_exit, "The replay used the emergency entrance stairs");
    assert_eq!(game.state.course, course);
    game
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--replay") {
        let course: usize = args[2].parse().expect("Invalid course");
        let route: Vec<Command> =
            serde_json::from_slice(&std::fs::read(&args[3]).expect("Missing recorded route"))
                .expect("Invalid recorded route");
        let game = replay(course, &route);
        println!(
            "{}",
            serde_json::json!({"course":course+1,"independent_replay":true,"tick":game.state.tick,"lives":game.state.lives,"hash":format!("{:016x}",game.state_hash())})
        );
        return;
    }
    let course: usize = args.get(1).and_then(|arg| arg.parse().ok()).unwrap_or(0);
    assert!(course < embertrail::COURSES);
    let limit: usize = args.get(2).and_then(|arg| arg.parse().ok()).unwrap_or(600);
    let mut game = start(course);
    let mut beam = vec![Node {
        state: game.state.clone(),
        route: Vec::new(),
        score: 0,
    }];
    let mut best_distance = i32::MAX;
    for depth in 0..limit {
        let mut next = Vec::new();
        for node in &beam {
            game.state = node.state.clone();
            let can_exit =
                reward_taken(&node.state) && node.state.level.secret_return.overlaps(game.player());
            let mut commands = Vec::with_capacity(7);
            for x in [1, 0, -1] {
                for jump in [false, true] {
                    commands.push(Command {
                        x,
                        jump,
                        down: false,
                    });
                }
            }
            if can_exit {
                commands.push(Command {
                    x: 0,
                    jump: false,
                    down: true,
                });
            }
            for command in commands {
                game.state = node.state.clone();
                advance(&mut game, command);
                if game.state.dead > 0 || game.state.lives < 5 {
                    continue;
                }
                let collected = reward_taken(&game.state);
                if !game.state.secret_inside && !collected {
                    continue;
                }
                let mut route = node.route.clone();
                route.push(command);
                if !game.state.secret_inside && collected {
                    let verified = replay(course, &route);
                    let base = format!(".blue-check/grotto-route-{:02}", course + 1);
                    std::fs::create_dir_all(".blue-check").unwrap();
                    std::fs::write(format!("{base}.json"), serde_json::to_vec(&route).unwrap())
                        .unwrap();
                    let proof = serde_json::json!({
                        "course":course+1,
                        "archetype":verified.state.level.grotto.archetype,
                        "variant":verified.state.level.grotto.variant,
                        "ticks":verified.state.tick,
                        "setup_ticks":120,
                        "room_ticks":verified.state.tick-121,
                        "spawn_protection_after_entry":0,
                        "lives":verified.state.lives,
                        "reward_collected":true,
                        "far_stair_exit":true,
                        "public_inputs_after_initialization":true,
                        "independent_replay":true,
                        "initialization":"isolated real surface root entrance fixture; initial position only, then public Down entry",
                        "scope":"room traversal only; not surface access or campaign completion",
                        "hash":format!("{:016x}",verified.state_hash()),
                        "route":format!("{base}.json")
                    });
                    std::fs::write(
                        format!("{base}-proof.json"),
                        serde_json::to_vec_pretty(&proof).unwrap(),
                    )
                    .unwrap();
                    println!("{proof}");
                    return;
                }
                let target = if collected {
                    game.state.level.secret_return
                } else {
                    game.state.level.grotto.reward
                };
                let player = game.player();
                let dx = (player.x + player.w / 2 - target.x - target.w / 2).abs();
                let dy = (player.y + player.h / 2 - target.y - target.h / 2).abs();
                best_distance = best_distance.min(dx + dy);
                // Retain a permanent reward bonus across the objective change.
                // Geometry and time remain in the beam, including waiting for lifts.
                let score = i64::from(collected) * 100_000 - i64::from(dx) * 12 - i64::from(dy) * 4;
                next.push(Node {
                    state: game.state.clone(),
                    route,
                    score,
                });
            }
        }
        next.sort_unstable_by_key(|node| -node.score);
        let mut seen = HashSet::new();
        let mut regions = HashMap::new();
        next.retain(|node| {
            // Preserve room-scale alternatives: a direct chase can become
            // trapped beneath a shelf and discard the approach to its lift.
            let count = regions
                .entry((
                    node.state.px / (SUB * 48),
                    node.state.py / (SUB * 32),
                    node.state.grounded,
                    reward_taken(&node.state),
                ))
                .or_insert(0);
            if *count >= 3 {
                return false;
            }
            let unique = seen.insert((
                node.state.px / (SUB * 4),
                node.state.py / (SUB * 4),
                node.state.vx / 128,
                node.state.vy / 128,
                node.state.grounded,
                node.state.jump_held,
                node.state.form,
                node.state.shield / 15,
                reward_taken(&node.state),
                node.state.tick % 480 / 4,
            ));
            if unique {
                *count += 1;
            }
            unique
        });
        next.truncate(512);
        if std::env::var_os("EMBERTRAIL_PROBE_TRACE").is_some() && depth % 120 == 119 {
            if let Some(node) = next.first() {
                eprintln!(
                    "depth={} x={} y={} reward={} score={}",
                    depth + 1,
                    node.state.px / SUB,
                    node.state.py / SUB,
                    reward_taken(&node.state),
                    node.score
                );
            }
        }
        beam = next;
        if beam.is_empty() {
            break;
        }
    }
    eprintln!("grotto={} bounded search incomplete; closest distance={best_distance}; this does not prove the room unreachable",course+1);
    std::process::exit(2);
}
