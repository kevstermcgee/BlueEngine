use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
use vesper3d::{
    math::V,
    viewer::{
        controller::{Controller, Movement},
        game::{GameDocument, GameRuntime, GameState, LoadedGame},
        game_example,
        net::{InputFrame, Packet, UdpTransport, PROTOCOL_VERSION},
        profile::ControllerProfile,
        server::DedicatedServer,
        simulation::HeadlessWorld,
    },
};

fn fixture() -> LoadedGame {
    let (document, map) = game_example::documents().unwrap();
    LoadedGame { document, map }
}
fn aim(world: &mut HeadlessWorld, player: u64, x: f32) {
    let profile = world.game.as_ref().unwrap().document().player_profile;
    *world.player_mut(player).unwrap() =
        Controller::for_profile(profile, V(x, 0., 3.), 0.).unwrap();
}
fn press(world: &mut HeadlessWorld, player: u64, x: f32) {
    aim(world, player, x);
    assert!(world.request_interaction(player));
    world.step();
}

#[test]
fn three_switches_unlock_exit_with_shared_deterministic_rules() {
    let loaded = fixture();
    let room = loaded.map.build().unwrap();
    let mut local = GameRuntime::compile(loaded.document.clone(), &loaded.map).unwrap();
    let mut world = loaded.world().unwrap();
    world.join(1);
    assert_eq!(world.player(1).unwrap().position, V(-3., 1.68, 3.));
    // Locked exit cannot complete; each unique switch contributes once.
    for x in [3., -3., -3., -1., 1., 3.] {
        let controller =
            Controller::for_profile(ControllerProfile::default(), V(x, 0., 3.), 0.).unwrap();
        local.interact(&room, &controller, 1);
        press(&mut world, 1, x);
        assert_eq!(world.game.as_ref().unwrap().state(), local.state());
    }
    assert_eq!(local.state().counters, [3]);
    assert!(local.state().completed);
    let checkpoint = world.checksum();
    let mut repeat = fixture().world().unwrap();
    repeat.join(1);
    for x in [3., -3., -3., -1., 1., 3.] {
        press(&mut repeat, 1, x);
    }
    assert_eq!(checkpoint, repeat.checksum());
}

#[test]
fn validation_rejects_unknown_fields_references_duplicates_and_bad_spawns() {
    let loaded = fixture();
    let valid = loaded.document.clone();
    let check = |d: &GameDocument| assert!(d.validate(&loaded.map).is_err());
    let mut d = valid.clone();
    d.schema_version = 2;
    check(&d);
    let mut d = valid.clone();
    d.spawn_points.push(d.spawn_points[0].clone());
    check(&d);
    let mut d = valid.clone();
    d.rules[1].id = d.rules[0].id.clone();
    check(&d);
    let mut d = valid.clone();
    d.interactables.push(d.interactables[0].clone());
    check(&d);
    let mut d = valid.clone();
    d.rules[0].on_interact = Some("missing".into());
    check(&d);
    let mut d = valid.clone();
    d.rules[0].condition = Some(vesper3d::viewer::game::Condition {
        counter: "missing".into(),
        equals: 1,
    });
    check(&d);
    let mut d = valid.clone();
    d.player_profile.eye_height = d.player_profile.height;
    check(&d);
    let mut d = valid.clone();
    d.player_profile.walk_speed = f32::NAN;
    check(&d);
    let mut d = valid.clone();
    d.spawn_points[0].feet = V(-3., 1.4, 1.);
    check(&d);
    let mut d = valid.clone();
    d.spawn_points[0].feet.1 = -1.;
    check(&d);
    let mut d = valid.clone();
    d.counters.insert("too-large".into(), 1_000_001);
    check(&d);
    let mut d = valid.clone();
    d.rules[0].actions = vec![];
    check(&d);
    let mut d = valid.clone();
    d.trigger_zones.push(vesper3d::viewer::game::TriggerZone {
        id: "duplicate".into(),
        bounds: vesper3d::viewer::controller::Collider {
            min: V::ZERO,
            max: V::ONE,
        },
        enabled: true,
    });
    d.trigger_zones.push(d.trigger_zones[0].clone());
    check(&d);
    let mut d = valid.clone();
    d.rules[0].on_enter = Some("missing-zone".into());
    check(&d);
    let mut d = valid.clone();
    d.rules[0].on_enter = Some("zone-a".into());
    d.rules[0].on_interact = Some(d.interactables[0].entity.clone());
    check(&d);
    let mut d = valid.clone();
    d.movers.push(vesper3d::viewer::game::Mover {
        id: "duplicate-mover".into(),
        entity: "exit".into(),
        translation: V(0., 3., 0.),
        duration_ticks: 60,
        initial_open: false,
    });
    d.movers.push(d.movers[0].clone());
    check(&d);
    let mut d = valid.clone();
    d.movers.push(vesper3d::viewer::game::Mover {
        id: "bad-mover".into(),
        entity: "nonexistent".into(),
        translation: V(0., 3., 0.),
        duration_ticks: 60,
        initial_open: false,
    });
    check(&d);
    let mut d = valid.clone();
    d.movers.push(vesper3d::viewer::game::Mover {
        id: "bad-ticks".into(),
        entity: "exit".into(),
        translation: V(0., 3., 0.),
        duration_ticks: 0,
        initial_open: false,
    });
    check(&d);
    let mut d = valid.clone();
    d.rules[0].actions = vec![vesper3d::viewer::game::GameAction::SetMover {
        mover: "missing-mover".into(),
        open: true,
    }];
    check(&d);
    let mut d = valid.clone();
    d.timers.push(vesper3d::viewer::game::TimerDefinition {
        id: "timer-a".into(),
        duration_ticks: 60,
        repeats: false,
        auto_start: false,
    });
    d.timers.push(d.timers[0].clone());
    check(&d);
    let mut d = valid.clone();
    d.timers.push(vesper3d::viewer::game::TimerDefinition {
        id: "zero-timer".into(),
        duration_ticks: 0,
        repeats: false,
        auto_start: false,
    });
    check(&d);
    let mut d = valid.clone();
    d.rules.push(vesper3d::viewer::game::Rule {
        id: "bad-timer-rule".into(),
        on_interact: None,
        on_enter: None,
        on_exit: None,
        on_timer: Some("missing-timer".into()),
        condition: None,
        once: false,
        actions: vec![vesper3d::viewer::game::GameAction::Complete],
    });
    check(&d);
    let mut d = valid.clone();
    d.rules[0].actions = vec![vesper3d::viewer::game::GameAction::StartTimer {
        timer: "missing-timer".into(),
    }];
    check(&d);
    let mut d = valid.clone();
    d.rules[0].actions = vec![vesper3d::viewer::game::GameAction::StopTimer {
        timer: "missing-timer".into(),
    }];
    check(&d);
    let mut value = serde_json::to_value(&valid).unwrap();
    value["script"] = "anything".into();
    assert!(serde_json::from_value::<GameDocument>(value).is_err());
    let mut value = serde_json::to_value(&valid).unwrap();
    value["rules"][0]["actions"][0]["action"] = "execute".into();
    assert!(serde_json::from_value::<GameDocument>(value).is_err());
}

#[test]
fn game_interactions_require_range_line_of_sight_and_registered_player() {
    let mut loaded = fixture();
    loaded.map = loaded
        .map
        .apply(&[vesper3d::viewer::authoring::Edit::AddBox {
            id: "occluder".into(),
            label: "wall".into(),
            center: V(-3., 1.5, 2.),
            half_extents: V(0.5, 0.5, 0.1),
            color: V::ONE,
        }])
        .unwrap();
    let mut world = loaded.world().unwrap();
    assert!(!world.request_interaction(99));
    world.join(1);
    press(&mut world, 1, -3.);
    assert_eq!(world.game.as_ref().unwrap().state().counters, [0]);
    aim(&mut world, 1, -1.);
    world.player_mut(1).unwrap().position.2 = 7.;
    world.request_interaction(1);
    world.step();
    assert_eq!(world.game.as_ref().unwrap().state().counters, [0]);
}

#[test]
fn profiles_preserve_custom_eye_height_movement_and_spawn_feet() {
    let profile = ControllerProfile {
        height: 1.,
        crouched_height: 0.5,
        eye_height: 0.9,
        walk_speed: 2.,
        ..Default::default()
    };
    let mut controller = Controller::for_profile(profile, V(0., 2., 0.), 0.).unwrap();
    assert_eq!(controller.feet_height(), 2.);
    assert!((controller.position.1 - 2.9).abs() < 0.001);
    for _ in 0..60 {
        controller.update(
            Movement {
                forward: 1.,
                ..Default::default()
            },
            1. / 60.,
            &[],
        );
    }
    assert!((controller.velocity().length() - 2.).abs() < 0.001);
    controller.set_physics_state(V(1., 3.9, 1.), 0., true);
    assert!((controller.feet_height() - 3.).abs() < 0.001);
}

#[test]
fn game_state_mirror_rejects_reordered_or_invalid_snapshots_and_fits_budget() {
    let loaded = fixture();
    let mut replica = GameRuntime::compile(loaded.document, &loaded.map).unwrap();
    let mut state = replica.state().clone();
    state.counters[0] = 2;
    assert!(replica.accept_snapshot(10, state.clone()));
    assert!(!replica.accept_snapshot(9, GameState::default()));
    let mut invalid = state.clone();
    invalid.counters.clear();
    assert!(!replica.accept_snapshot(11, invalid));
    assert_eq!(replica.state(), &state);
    assert!(replica.accept_snapshot(11, state));
    let worst = GameState {
        counters: vec![-1_000_000; 8],
        enabled: u16::MAX,
        enabled_zones: u16::MAX,
        mover_targets: u16::MAX,
        active_timers: u16::MAX,
        fired: u16::MAX,
        completed: true,
    };
    assert!(
        Packet::GameState {
            tick: u64::MAX,
            state: worst
        }
        .encode()
        .unwrap()
        .len()
            < 256
    );
}

fn poll_until(server: &mut DedicatedServer, done: impl Fn(&DedicatedServer) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        server.poll_network().unwrap();
        if done(server) {
            break;
        }
        assert!(Instant::now() < deadline, "UDP fixture timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn send_input(server: &mut DedicatedServer, client: &UdpTransport, id: u64, tick: u64) {
    client
        .send_packet(
            &Packet::Input(InputFrame {
                client_tick: tick,
                interact: true,
                ..Default::default()
            }),
            server.local_addr,
        )
        .unwrap();
    poll_until(server, |s| {
        s.sessions
            .get(&id)
            .is_some_and(|session| session.last_client_tick == tick)
    });
    server.step();
}

#[test]
fn two_udp_clients_share_authoritative_rules_and_game_mismatch_is_rejected() {
    let mut server =
        DedicatedServer::with_world("127.0.0.1:0", fixture().world().unwrap()).unwrap();
    let mut clients = [
        UdpTransport::bind("127.0.0.1:0").unwrap(),
        UdpTransport::bind("127.0.0.1:0").unwrap(),
    ];
    let mut different = fixture();
    different.document.player_profile.walk_speed += 1.;
    let other_hash = different.world().unwrap().content_hash;
    assert_ne!(server.world.content_hash, other_hash);
    clients[0]
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 0,
                content_hash: other_hash,
            },
            server.local_addr,
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        server.poll_network().unwrap();
        if let Some((Packet::Rejected { .. }, _)) = clients[0].recv_packet().unwrap() {
            break;
        }
        assert!(Instant::now() < deadline);
    }
    assert!(server.sessions.is_empty());
    for (i, client) in clients.iter().enumerate() {
        client
            .send_packet(
                &Packet::Hello {
                    protocol_version: PROTOCOL_VERSION,
                    player_id: 0,
                    content_hash: server.world.content_hash,
                },
                server.local_addr,
            )
            .unwrap();
        poll_until(&mut server, |s| s.sessions.len() == i + 1);
    }
    // Forged results have no server dispatch path.
    clients[0]
        .send_packet(
            &Packet::GameState {
                tick: 1,
                state: GameState {
                    completed: true,
                    ..Default::default()
                },
            },
            server.local_addr,
        )
        .unwrap();
    send_input(&mut server, &clients[0], 1, 1);
    send_input(&mut server, &clients[1], 2, 1);
    assert_eq!(server.world.game.as_ref().unwrap().state().counters, [2]);
    assert!(!server.world.game.as_ref().unwrap().state().completed);
    // Positioning is host-only fixture setup; clients still send only an E edge.
    aim(&mut server.world, 1, 1.);
    send_input(&mut server, &clients[0], 1, 2);
    aim(&mut server.world, 2, 3.);
    send_input(&mut server, &clients[1], 2, 2);
    assert!(server.world.game.as_ref().unwrap().state().completed);
    server.broadcast_snapshots();
    for client in &mut clients {
        let loaded = fixture();
        let mut mirror = GameRuntime::compile(loaded.document, &loaded.map).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !mirror.state().completed {
            if let Some((Packet::GameState { tick, state }, _)) = client.recv_packet().unwrap() {
                mirror.accept_snapshot(tick, state);
            }
            assert!(Instant::now() < deadline);
        }
        assert_eq!(mirror.state(), server.world.game.as_ref().unwrap().state());
    }
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        Self(std::env::temp_dir().join(format!(
            "blue-game-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn tool(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_be2-tools"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn source_free_example_validates_runs_headless_and_preserves_output() {
    let scratch = Scratch::new();
    let dir = scratch.0.to_str().unwrap();
    let made = tool(&["game-example", dir]);
    assert!(made.status.success(), "{:?}", made);
    let game = scratch.0.join("game.json");
    let path = game.to_str().unwrap();
    let bytes = std::fs::read(&game).unwrap();
    assert!(!tool(&["game-example", dir]).status.success());
    assert_eq!(std::fs::read(&game).unwrap(), bytes);
    assert!(tool(&["game-validate", path]).status.success());
    for name in ["game-schema", "game-describe"] {
        assert!(tool(&[name]).status.success());
    }
    let run = Command::new(env!("CARGO_BIN_EXE_be2-headless"))
        .args(["--game", path, "--ticks", "10"])
        .output()
        .unwrap();
    assert!(run.status.success(), "{:?}", run);
    assert!(String::from_utf8_lossy(&run.stdout).contains("game_state"));
    let mut document = GameDocument::load(&game).unwrap().document;
    document.map = "../outside.json".into();
    std::fs::write(&game, serde_json::to_vec(&document).unwrap()).unwrap();
    assert!(GameDocument::load(&game).is_err());
    let run = Command::new(env!("CARGO_BIN_EXE_be2-headless"))
        .args(["--game", path, "--ticks", "10"])
        .output()
        .unwrap();
    assert!(!run.status.success());
    // The previous --map interface still loads the same geometry independently.
    let map = scratch.0.join("map.json");
    assert!(Command::new(env!("CARGO_BIN_EXE_be2-headless"))
        .args(["--map", map.to_str().unwrap(), "--ticks", "1"])
        .status()
        .unwrap()
        .success());
}

#[test]
fn fallible_world_startup_reports_physics_errors() {
    let map = vesper3d::prelude::SceneBuilder::new("broken physics")
        .prop("apple", "apple", V(2., 0., 0.))
        .build()
        .unwrap();
    let mut room = map.build().unwrap();
    let material = room.compiled.scene.nodes[0].material.clone();
    room.compiled.scene.materials.remove(&material);
    assert!(HeadlessWorld::try_with_room(room).is_err());
    assert!(GameDocument::load(Path::new("assets/games/three-switches/game.json")).is_ok());
}

#[test]
fn trigger_volumes_fire_on_enter_and_exit_with_shared_rules() {
    let mut loaded = fixture();
    loaded.document.trigger_zones = vec![
        vesper3d::viewer::game::TriggerZone {
            id: "plate-a".into(),
            bounds: vesper3d::viewer::controller::Collider {
                min: V(0.0, 0.0, 0.0),
                max: V(2.0, 1.0, 2.0),
            },
            enabled: true,
        },
        vesper3d::viewer::game::TriggerZone {
            id: "exit-zone".into(),
            bounds: vesper3d::viewer::controller::Collider {
                min: V(4.0, 0.0, 4.0),
                max: V(6.0, 2.0, 6.0),
            },
            enabled: false,
        },
    ];
    loaded.document.counters.insert("plate_steps".into(), 0);
    loaded.document.counters.insert("plate_exits".into(), 0);
    loaded.document.rules = vec![
        vesper3d::viewer::game::Rule {
            id: "step-plate-a".into(),
            on_interact: None,
            on_enter: Some("plate-a".into()),
            on_exit: None,
            on_timer: None,
            condition: None,
            once: false,
            actions: vec![
                vesper3d::viewer::game::GameAction::Increment {
                    counter: "plate_steps".into(),
                    amount: 1,
                },
                vesper3d::viewer::game::GameAction::SetEnabled {
                    entity: "exit-zone".into(),
                    enabled: true,
                },
            ],
        },
        vesper3d::viewer::game::Rule {
            id: "exit-plate-a".into(),
            on_interact: None,
            on_enter: None,
            on_exit: Some("plate-a".into()),
            on_timer: None,
            condition: None,
            once: false,
            actions: vec![vesper3d::viewer::game::GameAction::Increment {
                counter: "plate_exits".into(),
                amount: 1,
            }],
        },
        vesper3d::viewer::game::Rule {
            id: "reach-exit-zone".into(),
            on_interact: None,
            on_enter: Some("exit-zone".into()),
            on_exit: None,
            on_timer: None,
            condition: None,
            once: true,
            actions: vec![vesper3d::viewer::game::GameAction::Complete],
        },
    ];
    let mut world = loaded.world().unwrap();
    world.join(1);

    let steps_idx = world
        .game
        .as_ref()
        .unwrap()
        .document()
        .counters
        .keys()
        .position(|k| k == "plate_steps")
        .unwrap();
    let exits_idx = world
        .game
        .as_ref()
        .unwrap()
        .document()
        .counters
        .keys()
        .position(|k| k == "plate_exits")
        .unwrap();

    // Initial position outside plate-a
    assert_eq!(world.game.as_ref().unwrap().state().counters[steps_idx], 0);
    assert_eq!(world.game.as_ref().unwrap().state().counters[exits_idx], 0);

    // Move player inside plate-a (1.0, 0.0, 1.0)
    world.player_mut(1).unwrap().position = V(1.0, 1.68, 1.0);
    world.step();
    // Entering plate-a triggers increment and enables exit-zone
    assert_eq!(world.game.as_ref().unwrap().state().counters[steps_idx], 1);
    assert_eq!(world.game.as_ref().unwrap().state().counters[exits_idx], 0);
    assert!(world.game.as_ref().unwrap().zone_enabled(1)); // exit-zone enabled!

    // Stepping again while remaining inside does NOT re-fire on_enter
    world.step();
    assert_eq!(world.game.as_ref().unwrap().state().counters[steps_idx], 1);
    assert_eq!(world.game.as_ref().unwrap().state().counters[exits_idx], 0);

    // Move player outside plate-a (-5.0, 0.0, -5.0)
    world.player_mut(1).unwrap().position = V(-5.0, 1.68, -5.0);
    world.step();
    // Exiting plate-a triggers plate_exits increment!
    assert_eq!(world.game.as_ref().unwrap().state().counters[steps_idx], 1);
    assert_eq!(world.game.as_ref().unwrap().state().counters[exits_idx], 1);

    // Step again outside, no change
    world.step();
    assert_eq!(world.game.as_ref().unwrap().state().counters[exits_idx], 1);

    // Now move player into the newly enabled exit-zone (5.0, 0.0, 5.0)
    assert!(!world.game.as_ref().unwrap().state().completed);
    world.player_mut(1).unwrap().position = V(5.0, 1.68, 5.0);
    world.step();
    assert!(world.game.as_ref().unwrap().state().completed);
}

#[test]
fn kinematic_movers_translate_colliders_and_respond_to_rules() {
    let mut loaded = fixture();
    // Declare a mover on "exit" translating up by 2.0 metres over 10 ticks
    loaded.document.movers = vec![vesper3d::viewer::game::Mover {
        id: "exit-door".into(),
        entity: "exit".into(),
        translation: V(0., 2.0, 0.),
        duration_ticks: 10,
        initial_open: false,
    }];
    // Rule: pressing button-a opens the mover
    loaded.document.rules.push(vesper3d::viewer::game::Rule {
        id: "open-exit-door".into(),
        on_interact: Some("button-a".into()),
        on_enter: None,
        on_exit: None,
        on_timer: None,
        condition: None,
        once: true,
        actions: vec![vesper3d::viewer::game::GameAction::SetMover {
            mover: "exit-door".into(),
            open: true,
        }],
    });
    let mut world = loaded.world().unwrap();
    world.join(1);

    let base_bounds = world.game.as_ref().unwrap().movers()[0]
        .base_collider
        .clone();
    assert_eq!(world.game.as_ref().unwrap().mover_progress(0), Some(0.0));
    assert!(!world.game.as_ref().unwrap().mover_open(0));

    // Player interacts with button-a
    press(&mut world, 1, -3.);
    assert!(world.game.as_ref().unwrap().mover_open(0));

    // Step through the 10 ticks
    for tick in 1..=10 {
        world.step();
        let expected_progress = tick as f32 / 10.0;
        let progress = world.game.as_ref().unwrap().mover_progress(0).unwrap();
        assert!((progress - expected_progress).abs() < 0.001);

        let current_bounds = world.game.as_ref().unwrap().mover_bounds(0).unwrap();
        let expected_min_y = base_bounds.min.1 + 2.0 * expected_progress;
        assert!((current_bounds.min.1 - expected_min_y).abs() < 0.001);

        // Verify room.colliders was updated dynamically
        let room_collider = world
            .room
            .colliders
            .iter()
            .find(|c| {
                (c.min.0 - base_bounds.min.0).abs() < 0.001
                    && (c.min.2 - base_bounds.min.2).abs() < 0.001
            })
            .unwrap();
        assert!((room_collider.min.1 - expected_min_y).abs() < 0.001);
    }

    // Now at tick 10, mover progress is 1.0 (fully open)
    assert!((world.game.as_ref().unwrap().mover_progress(0).unwrap() - 1.0).abs() < 0.001);

    // Stepping further keeps it clamped at 1.0
    world.step();
    assert!((world.game.as_ref().unwrap().mover_progress(0).unwrap() - 1.0).abs() < 0.001);
}

#[test]
fn timers_count_down_and_dispatch_delayed_actions() {
    let mut loaded = fixture();
    loaded.document.timers = vec![
        vesper3d::viewer::game::TimerDefinition {
            id: "delay-bomb".into(),
            duration_ticks: 3,
            repeats: false,
            auto_start: false,
        },
        vesper3d::viewer::game::TimerDefinition {
            id: "pulse".into(),
            duration_ticks: 2,
            repeats: true,
            auto_start: false,
        },
    ];
    loaded.document.counters.insert("ticks_fired".into(), 0);
    loaded.document.counters.insert("pulses_fired".into(), 0);
    loaded.document.rules = vec![
        vesper3d::viewer::game::Rule {
            id: "start-timers".into(),
            on_interact: Some("button-a".into()),
            on_enter: None,
            on_exit: None,
            on_timer: None,
            condition: None,
            once: true,
            actions: vec![
                vesper3d::viewer::game::GameAction::StartTimer {
                    timer: "delay-bomb".into(),
                },
                vesper3d::viewer::game::GameAction::StartTimer {
                    timer: "pulse".into(),
                },
            ],
        },
        vesper3d::viewer::game::Rule {
            id: "on-delay-bomb".into(),
            on_interact: None,
            on_enter: None,
            on_exit: None,
            on_timer: Some("delay-bomb".into()),
            condition: None,
            once: false,
            actions: vec![vesper3d::viewer::game::GameAction::Increment {
                counter: "ticks_fired".into(),
                amount: 1,
            }],
        },
        vesper3d::viewer::game::Rule {
            id: "on-pulse".into(),
            on_interact: None,
            on_enter: None,
            on_exit: None,
            on_timer: Some("pulse".into()),
            condition: None,
            once: false,
            actions: vec![vesper3d::viewer::game::GameAction::Increment {
                counter: "pulses_fired".into(),
                amount: 1,
            }],
        },
        vesper3d::viewer::game::Rule {
            id: "stop-pulse".into(),
            on_interact: Some("button-b".into()),
            on_enter: None,
            on_exit: None,
            on_timer: None,
            condition: None,
            once: true,
            actions: vec![vesper3d::viewer::game::GameAction::StopTimer {
                timer: "pulse".into(),
            }],
        },
    ];
    let mut world = loaded.world().unwrap();
    world.join(1);

    let ticks_idx = world
        .game
        .as_ref()
        .unwrap()
        .document()
        .counters
        .keys()
        .position(|k| k == "ticks_fired")
        .unwrap();
    let pulses_idx = world
        .game
        .as_ref()
        .unwrap()
        .document()
        .counters
        .keys()
        .position(|k| k == "pulses_fired")
        .unwrap();

    assert_eq!(world.game.as_ref().unwrap().state().counters[ticks_idx], 0);
    assert_eq!(world.game.as_ref().unwrap().state().counters[pulses_idx], 0);
    assert!(!world.game.as_ref().unwrap().timer_active(0));
    assert!(!world.game.as_ref().unwrap().timer_active(1));

    // Player interacts with button-a to start both timers
    press(&mut world, 1, -3.);
    assert!(world.game.as_ref().unwrap().timer_active(0));
    assert!(world.game.as_ref().unwrap().timer_active(1));
    assert_eq!(world.game.as_ref().unwrap().timer_remaining(0), Some(3));
    assert_eq!(world.game.as_ref().unwrap().timer_remaining(1), Some(2));

    // Tick 1
    world.step();
    assert_eq!(world.game.as_ref().unwrap().timer_remaining(0), Some(2));
    assert_eq!(world.game.as_ref().unwrap().timer_remaining(1), Some(1));
    assert_eq!(world.game.as_ref().unwrap().state().counters[ticks_idx], 0);
    assert_eq!(world.game.as_ref().unwrap().state().counters[pulses_idx], 0);

    // Tick 2: pulse reaches 0, fires, auto-resets to 2
    world.step();
    assert_eq!(world.game.as_ref().unwrap().timer_remaining(0), Some(1));
    assert_eq!(world.game.as_ref().unwrap().timer_remaining(1), Some(2));
    assert_eq!(world.game.as_ref().unwrap().state().counters[ticks_idx], 0);
    assert_eq!(world.game.as_ref().unwrap().state().counters[pulses_idx], 1);

    // Tick 3: delay-bomb reaches 0, fires, stops; pulse decrements to 1
    world.step();
    assert!(!world.game.as_ref().unwrap().timer_active(0));
    assert_eq!(world.game.as_ref().unwrap().timer_remaining(0), None);
    assert_eq!(world.game.as_ref().unwrap().timer_remaining(1), Some(1));
    assert_eq!(world.game.as_ref().unwrap().state().counters[ticks_idx], 1);
    assert_eq!(world.game.as_ref().unwrap().state().counters[pulses_idx], 1);

    // Tick 4: pulse fires again
    world.step();
    assert_eq!(world.game.as_ref().unwrap().state().counters[ticks_idx], 1);
    assert_eq!(world.game.as_ref().unwrap().state().counters[pulses_idx], 2);

    // Stop pulse via button-b
    press(&mut world, 1, -1.);
    assert!(!world.game.as_ref().unwrap().timer_active(1));

    // Step further, no more increments
    world.step();
    world.step();
    assert_eq!(world.game.as_ref().unwrap().state().counters[ticks_idx], 1);
    assert_eq!(world.game.as_ref().unwrap().state().counters[pulses_idx], 2);
}
