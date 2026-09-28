use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use vesper3d::viewer::{
    game::{GameDocument, LoadedGame},
    game_session::{GameInput, GameSession},
    net::UdpTransport,
    newgame::scaffold_new_game,
    server::DedicatedServer,
};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        // A counter, not just the clock: several tests in this file call `Fixture::new` from their own
        // thread (cargo runs test functions in parallel within one process), and two threads can read an
        // identical `SystemTime::now()`, which is not guaranteed to be sub-microsecond on every platform.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "blue-game-runtime-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        scaffold_new_game("runtime-proof", &dir, Some(env!("CARGO_MANIFEST_DIR"))).unwrap();
        Self(dir)
    }
    fn load(&self) -> LoadedGame {
        GameDocument::load(&self.0.join("game.json")).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn action() -> GameInput {
    GameInput {
        interact: true,
        ..Default::default()
    }
}
#[test]
fn generated_game_runs_rules_physics_pause_and_transactional_restart() {
    let fixture = Fixture::new();
    let mut session = GameSession::local(fixture.load()).unwrap();
    let start = session.controller().position;
    let prop = session.world().prop_position("apple").unwrap();
    session.advance(action(), 1. / 240., true).unwrap();
    assert!(!session.world().game.as_ref().unwrap().state().completed);
    for _ in 0..3 {
        session
            .advance(GameInput::default(), 1. / 240., true)
            .unwrap();
    }
    assert!(
        session.world().game.as_ref().unwrap().state().completed,
        "interaction edge must survive a sub-tick frame"
    );
    for _ in 0..60 {
        session
            .advance(GameInput::default(), 1. / 60., true)
            .unwrap();
    }
    assert!(session.world().prop_position("apple").unwrap().1 < prop.1 - 0.1);
    let tick = session.world().tick;
    session.advance(action(), 1., false).unwrap();
    assert_eq!(session.world().tick, tick);
    assert_eq!(session.world().game.as_ref().unwrap().state().round, 0);
    session.advance(action(), 1. / 60., true).unwrap();
    assert_eq!(session.world().tick, tick + 1);
    assert_eq!(session.world().game.as_ref().unwrap().state().round, 1);
    assert!(!session.world().game.as_ref().unwrap().state().completed);
    assert!((session.controller().position - start).length() < 0.01);
    assert!((session.world().prop_position("apple").unwrap() - prop).length() < 0.02);
}
#[test]
fn gameplay_is_fixed_rate_at_30_60_144_and_240_hz() {
    let fixture = Fixture::new();
    let mut positions = Vec::new();
    for hz in [30, 60, 144, 240] {
        let mut session = GameSession::local(fixture.load()).unwrap();
        for _ in 0..hz {
            session
                .advance(
                    GameInput {
                        movement: vesper3d::viewer::controller::Movement {
                            right: 1.,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    1. / hz as f32,
                    true,
                )
                .unwrap();
        }
        assert_eq!(session.world().tick, 60);
        positions.push(session.controller().position);
    }
    for p in &positions[1..] {
        assert!((*p - positions[0]).length() < 0.001);
    }
}
fn pump(server: &mut DedicatedServer, clients: &mut [GameSession; 2], intents: [GameInput; 2]) {
    for (client, input) in clients.iter_mut().zip(intents) {
        client.advance(input, 1. / 60., true).unwrap();
    }
    server.poll_network().unwrap();
    server.step();
    std::thread::sleep(Duration::from_millis(1));
}
#[test]
fn two_shared_clients_observe_server_owned_win_reset_and_props() {
    let fixture = Fixture::new();
    let mut server = DedicatedServer::with_world("127.0.0.1:0", fixture.load().world().unwrap())
        .unwrap()
        .with_auth("test-only-key");
    let mut clients = std::array::from_fn(|_| {
        GameSession::connect(
            fixture.load(),
            Box::new(UdpTransport::bind("127.0.0.1:0").unwrap()),
            server.local_addr,
            Some("test-only-key".into()),
        )
        .unwrap()
    });
    for _ in 0..30 {
        pump(&mut server, &mut clients, [GameInput::default(); 2]);
    }
    assert!(clients.iter().all(GameSession::connected));
    assert!(
        clients.iter().all(|c| c.world().tick == 0),
        "online mirrors must never step gameplay"
    );
    let player = clients[0].player_id().unwrap();
    let start = server.world.player(player).unwrap().position;
    let walk = GameInput {
        movement: vesper3d::viewer::controller::Movement {
            forward: 1.,
            ..Default::default()
        },
        ..Default::default()
    };
    for _ in 0..12 {
        pump(&mut server, &mut clients, [walk, GameInput::default()]);
    }
    for _ in 0..6 {
        pump(&mut server, &mut clients, [GameInput::default(); 2]);
    }
    assert!((server.world.player(player).unwrap().position - start).length() > 0.2);
    assert!(
        (clients[0].controller().position - server.world.player(player).unwrap().position).length()
            < 0.1
    );
    let paused_at = server.world.player(player).unwrap().position;
    for _ in 0..12 {
        clients[0].advance(walk, 1. / 60., false).unwrap();
        clients[1]
            .advance(GameInput::default(), 1. / 60., true)
            .unwrap();
        server.poll_network().unwrap();
        server.step();
    }
    assert!(
        (server.world.player(player).unwrap().position - paused_at).length() < 0.1,
        "online menu sends neutral movement"
    );
    pump(&mut server, &mut clients, [action(), GameInput::default()]);
    for _ in 0..12 {
        pump(&mut server, &mut clients, [GameInput::default(); 2]);
    }
    assert!(server.world.game.as_ref().unwrap().state().completed);
    assert!(clients
        .iter()
        .all(|c| c.world().game.as_ref().unwrap().state().completed));
    assert!(clients.iter().all(|c| c.remote_players().len() == 1));
    pump(&mut server, &mut clients, [GameInput::default(), action()]);
    for _ in 0..12 {
        pump(&mut server, &mut clients, [GameInput::default(); 2]);
    }
    assert_eq!(server.world.game.as_ref().unwrap().state().round, 1);
    for client in &clients {
        assert_eq!(client.world().game.as_ref().unwrap().state().round, 1);
        assert!(!client.world().game.as_ref().unwrap().state().completed);
        assert!(
            (client.controller().position
                - server
                    .world
                    .player(client.player_id().unwrap())
                    .unwrap()
                    .position)
                .length()
                < 0.1
        );
        assert!(client.world().prop_position("apple").is_some());
    }
}
#[test]
fn generated_content_runs_on_separate_headless_process_with_two_scripted_clients() {
    let fixture = Fixture::new();
    let socket = UdpTransport::bind("127.0.0.1:0").unwrap();
    let address = socket.local_addr().unwrap();
    drop(socket);
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_be2-headless"))
        .args([
            "--server",
            &address.to_string(),
            "--game",
            fixture.0.join("game.json").to_str().unwrap(),
            "--ticks",
            "240",
        ])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let result = std::panic::catch_unwind(|| {
        let mut clients: [GameSession; 2] = std::array::from_fn(|_| {
            GameSession::connect(
                fixture.load(),
                Box::new(UdpTransport::bind("127.0.0.1:0").unwrap()),
                address,
                None,
            )
            .unwrap()
        });
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut won = false;
        let mut restarted = false;
        let mut sent_win = false;
        let mut sent_reset = false;
        while Instant::now() < deadline {
            for (i, client) in clients.iter_mut().enumerate() {
                let mut intent = GameInput::default();
                if i == 0 && client.connected() && !sent_win {
                    intent = action();
                    sent_win = true;
                }
                if i == 1 && won && !sent_reset {
                    intent = action();
                    sent_reset = true;
                }
                client.advance(intent, 1. / 60., true).unwrap();
            }
            won |= clients
                .iter()
                .all(|c| c.world().game.as_ref().unwrap().state().completed);
            restarted |= clients
                .iter()
                .all(|c| c.world().game.as_ref().unwrap().state().round == 1);
            if restarted {
                break;
            }
            std::thread::sleep(Duration::from_millis(16));
        }
        assert!(
            won && restarted,
            "both clients must observe the authoritative win and restart"
        );
    });
    let _ = child.kill();
    let _ = child.wait();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
#[test]
fn prop_pickup_drop_and_restart_use_the_shared_authoritative_action() {
    let fixture = Fixture::new();
    let mut session = GameSession::local(fixture.load()).unwrap();
    let aim = session.world().prop_position("apple").unwrap() - session.controller().position;
    let look = [aim.0.atan2(-aim.2), -(aim.1 / aim.length()).asin()];
    session
        .advance(
            GameInput {
                look,
                interact: true,
                ..Default::default()
            },
            1. / 60.,
            true,
        )
        .unwrap();
    assert!(session
        .world()
        .prop_physics
        .as_ref()
        .unwrap()
        .held_for_player(1)
        .is_some());
    session.advance(action(), 1. / 60., true).unwrap();
    assert!(session
        .world()
        .prop_physics
        .as_ref()
        .unwrap()
        .held_for_player(1)
        .is_none());
    let mut world = fixture.load().world().unwrap();
    world.join(1);
    world.join(2);
    assert!(!world.game_action(999).unwrap());
    world.request_interaction(1);
    world.step();
    assert!(world.game.as_ref().unwrap().state().completed);
    world.game_action(2).unwrap();
    assert_eq!(world.game.as_ref().unwrap().state().round, 1);
    assert!(world.player(1).is_some() && world.player(2).is_some());
}

#[test]
fn server_movers_are_mirrored_without_running_client_timers_or_triggers() {
    use vesper3d::viewer::game::{GameAction, Mover, TimerDefinition};
    let fixture = Fixture::new();
    let mut loaded = fixture.load();
    loaded.document.movers.push(Mover {
        id: "lift".into(),
        entity: "objective".into(),
        translation: vesper3d::math::V(0., 1., 0.),
        duration_ticks: 60,
        initial_open: false,
    });
    loaded.document.timers.push(TimerDefinition {
        id: "clock".into(),
        duration_ticks: 20,
        auto_start: true,
        repeats: false,
    });
    loaded.document.rules[0].on_interact = None;
    loaded.document.rules[0].on_timer = Some("clock".into());
    loaded.document.rules[0].actions = vec![GameAction::SetMover {
        mover: "lift".into(),
        open: true,
    }];
    let mut server =
        DedicatedServer::with_world("127.0.0.1:0", loaded.clone().world().unwrap()).unwrap();
    let mut clients = std::array::from_fn(|_| {
        GameSession::connect(
            loaded.clone(),
            Box::new(UdpTransport::bind("127.0.0.1:0").unwrap()),
            server.local_addr,
            None,
        )
        .unwrap()
    });
    for _ in 0..42 {
        pump(&mut server, &mut clients, [GameInput::default(); 2]);
    }
    // Flush the latest snapshot without stepping either client authority.
    for client in &mut clients {
        client.advance(GameInput::default(), 0., false).unwrap();
    }
    let server_game = server.world.game.as_ref().unwrap();
    assert!(server_game.mover_progress(0).unwrap() > 0.2);
    for client in &clients {
        let game = client.world().game.as_ref().unwrap();
        assert_eq!(game.mover_progress(0), server_game.mover_progress(0));
        assert_eq!(client.world().tick, 0);
    }
    let before = clients[0].world().game.as_ref().unwrap().mover_progress(0);
    for _ in 0..40 {
        clients[0]
            .advance(GameInput::default(), 1. / 60., false)
            .unwrap();
    }
    assert_eq!(
        clients[0].world().game.as_ref().unwrap().mover_progress(0),
        before,
        "no locally advanced mover clock"
    );
}
#[test]
fn reordered_snapshots_cannot_undo_a_round_reset_or_run_client_rules() {
    use std::{cell::RefCell, rc::Rc};
    use vesper3d::viewer::net::{Datagram, DatagramTransport, Packet, WorldSnapshot};
    struct Queue(Rc<RefCell<Vec<Datagram>>>);
    impl DatagramTransport for Queue {
        fn send(&self, _: std::net::SocketAddr, data: &[u8]) -> vesper3d::Result<usize> {
            Ok(data.len())
        }
        fn receive(&mut self) -> vesper3d::Result<Vec<Datagram>> {
            Ok(std::mem::take(&mut *self.0.borrow_mut()))
        }
        fn local_addr(&self) -> vesper3d::Result<std::net::SocketAddr> {
            Ok("127.0.0.1:11111".parse().unwrap())
        }
    }
    let fixture = Fixture::new();
    let address = "127.0.0.1:11112".parse().unwrap();
    let queue = Rc::new(RefCell::new(Vec::new()));
    let mut client = GameSession::connect(
        fixture.load(),
        Box::new(Queue(queue.clone())),
        address,
        None,
    )
    .unwrap();
    let mut world = fixture.load().world().unwrap();
    world.join(1);
    let mut state = world.game.as_ref().unwrap().state().clone();
    state.completed = true;
    let enqueue = |packet: Packet| {
        queue.borrow_mut().push(Datagram {
            peer: address,
            data: packet.encode().unwrap(),
        })
    };
    enqueue(Packet::Welcome {
        player_id: 1,
        server_tick: 0,
        map_name: "test".into(),
        session_token: Some([7, 8]),
    });
    // Delayed state from a previous connection must not poison the new session's
    // monotonic tick filter, even if its tick is larger than any current update.
    enqueue(Packet::GameState {
        session: Some([1, 2]),
        tick: u64::MAX,
        state: state.clone(),
    });
    let mut stale = world.snapshot(0);
    stale.tick = u64::MAX;
    stale.session = Some([1, 2]);
    stale.players[0].position.0 = 100.;
    enqueue(Packet::Snapshot(stale));
    client.advance(GameInput::default(), 0., false).unwrap();
    assert!(!client.world().game.as_ref().unwrap().state().completed);
    enqueue(Packet::GameState {
        session: Some([7, 8]),
        tick: 10,
        state: state.clone(),
    });
    client.advance(GameInput::default(), 0., false).unwrap();
    assert!(client.world().game.as_ref().unwrap().state().completed);
    state.round = 1;
    state.completed = false;
    enqueue(Packet::GameState {
        session: Some([7, 8]),
        tick: 20,
        state: state.clone(),
    });
    state.round = 0;
    state.completed = true;
    enqueue(Packet::GameState {
        session: Some([7, 8]),
        tick: 15,
        state,
    });
    let mut snapshot = world.snapshot(0);
    snapshot.tick = 19;
    snapshot.session = Some([7, 8]);
    snapshot.players[0].position.0 = 100.;
    enqueue(Packet::Snapshot(snapshot));
    enqueue(Packet::Snapshot(WorldSnapshot {
        tick: 18,
        ..Default::default()
    }));
    client.advance(GameInput::default(), 0., false).unwrap();
    assert_eq!(client.world().game.as_ref().unwrap().state().round, 1);
    assert!(!client.world().game.as_ref().unwrap().state().completed);
    assert!(client.controller().position.0 < 0.);
    assert_eq!(client.world().tick, 0);
}
#[test]
fn old_round_inputs_cannot_move_or_complete_the_restarted_world() {
    use vesper3d::viewer::net::{ActionCounters, Packet, SequencedInputFrame, PROTOCOL_VERSION};
    let fixture = Fixture::new();
    let mut server =
        DedicatedServer::with_world("127.0.0.1:0", fixture.load().world().unwrap()).unwrap();
    let client = UdpTransport::bind("127.0.0.1:0").unwrap();
    server.handle_hello(
        client.local_addr().unwrap(),
        PROTOCOL_VERSION,
        0,
        server.world.content_hash,
    );
    server.world.restart_game().unwrap();
    let before = server.world.player(1).unwrap().position;
    client
        .send_packet(
            &Packet::SequencedInput(SequencedInputFrame {
                client_tick: 100,
                round: 0,
                movement: vesper3d::viewer::controller::Movement {
                    forward: 1.,
                    ..Default::default()
                },
                counters: ActionCounters {
                    interact: 5,
                    ..Default::default()
                },
                ..Default::default()
            }),
            server.local_addr,
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(5));
    server.poll_network().unwrap();
    server.step();
    assert_eq!(server.sessions[&1].last_client_tick, 0);
    assert!((server.world.player(1).unwrap().position - before).length() < 0.01);
    assert!(!server.world.game.as_ref().unwrap().state().completed);
}
