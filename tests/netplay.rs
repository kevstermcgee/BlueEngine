//! The netplay kit end to end on the toy game: a real server and real clients on an in-memory network with
//! configurable delay and loss. What these prove holds for every game built on the kit.
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use vesper3d::viewer::net::codec::{Reader, Writer};
use vesper3d::viewer::net::loopback::{LoopEnd, LoopNet};
use vesper3d::viewer::net::DatagramTransport;
use vesper3d::viewer::netplay::toy::{ToyEvent, ToyGame, ToyInput, ToySnapshot};
use vesper3d::viewer::netplay::wire::{
    decode_client, decode_server, encode_client, encode_server, ClientMsg, LobbyEntry, LobbyState,
    ServerMsg, SnapshotMsg, MAX_DATAGRAM,
};
use vesper3d::viewer::netplay::{
    ClientConfig, ClientState, ConnectFailure, MatchLog, NetClient, NetGame, NetServer,
    ServerConfig, Stage,
};

fn addr(n: u16) -> SocketAddr {
    format!("10.0.0.{}:{}", n % 200 + 1, 4000 + n)
        .parse()
        .unwrap()
}

type Server = NetServer<ToyGame, LoopEnd>;
type Client = NetClient<ToyGame, LoopEnd>;

struct World {
    net: LoopNet,
    server: Server,
    clients: Vec<Client>,
    tick: u64,
}

fn config() -> ServerConfig {
    ServerConfig {
        participants: 6,
        countdown_seconds: 1,
        results_seconds: 2,
        auto_start_seconds: 0,
        seed: Some(11),
        ..Default::default()
    }
}

fn client_at(net: &LoopNet, n: u16, choice: u8, key: &str) -> Client {
    NetClient::new(
        net.endpoint(addr(n)),
        addr(0),
        ClientConfig {
            name: format!("Player {n}"),
            key: key.into(),
            choice,
        },
    )
    .unwrap()
}

fn world(latency: u64, jitter: u64, loss: f32, wanted: &[u8], cfg: ServerConfig) -> World {
    let net = LoopNet::new(latency, jitter, loss, 99);
    let server = NetServer::new(net.endpoint(addr(0)), cfg.clone()).unwrap();
    let key = cfg.join_key.clone().unwrap_or_default();
    let clients = wanted
        .iter()
        .enumerate()
        .map(|(i, c)| client_at(&net, i as u16 + 1, *c, &key))
        .collect();
    World {
        net,
        server,
        clients,
        tick: 0,
    }
}

impl World {
    fn step(&mut self) {
        self.tick += 1;
        let now = self.tick as f64 / 60.;
        self.net.advance();
        let wall = Instant::now();
        self.server.poll(wall);
        self.server.step(wall);
        for c in &mut self.clients {
            c.poll(now);
            if *c.state() == ClientState::Playing {
                c.tick(ToyInput { throttle: 1 });
            }
            c.frame(now, 1. / 60.);
        }
    }

    fn run_until(&mut self, limit: u64, done: impl Fn(&World) -> bool) -> bool {
        for _ in 0..limit {
            if done(self) {
                return true;
            }
            self.step();
        }
        done(self)
    }

    fn all_in_lobby(&self) -> bool {
        self.clients
            .iter()
            .all(|c| *c.state() == ClientState::Lobby)
    }

    fn ready_everyone(&mut self) {
        for c in &mut self.clients {
            c.ready(true);
        }
    }
}

fn play_a_match(w: &mut World) {
    assert!(
        w.run_until(300, |w| w.all_in_lobby()),
        "everyone reaches the lobby"
    );
    w.ready_everyone();
    assert!(
        w.run_until(900, |w| w.server.stage() == Stage::Match),
        "the match starts once everyone is ready"
    );
    assert!(
        w.run_until(60 * 60, |w| w.server.stage() == Stage::Results),
        "the match ends"
    );
}

#[test]
fn players_join_choose_ready_up_play_and_finish() {
    let mut w = world(3, 1, 0., &[0, 0, 3], config());
    assert!(w.run_until(300, |w| w.all_in_lobby()));
    let lobby = w.clients[0].lobby().unwrap().clone();
    let mut choices: Vec<u8> = lobby.entries.iter().map(|e| e.choice).collect();
    choices.sort();
    choices.dedup();
    assert_eq!(
        choices.len(),
        3,
        "every player has a different choice: {lobby:?}"
    );
    w.clients[2].select(1);
    w.run_until(30, |_| false);
    assert!(w.clients[0]
        .lobby()
        .unwrap()
        .entries
        .iter()
        .any(|e| e.choice == 1));
    play_a_match(&mut w);
    let log = w
        .server
        .match_log()
        .last()
        .expect("the server logged the match")
        .clone();
    assert_eq!(log.game, "toy-footrace");
    assert!(
        log.report["winner"].is_number(),
        "the game's own report is carried: {}",
        log.report
    );
    assert_eq!(log.net.peers.len(), 3);
    w.run_until(40, |_| false);
    for c in &w.clients {
        let stats = c.stats();
        assert!(stats.snapshots > 100, "snapshots kept arriving");
        assert!(
            stats.prediction.max_error < 3.,
            "prediction error stays small: {}",
            stats.prediction.max_error
        );
        assert!(
            c.view()
                .snapshot
                .as_ref()
                .is_some_and(|s| s.winner.is_some()),
            "the client saw the result"
        );
    }
}

#[test]
fn lobby_choices_survive_heavy_packet_loss() {
    // 30% loss: a one-shot Ready would usually be lost. The client keeps re-sending until the server agrees.
    let mut w = world(2, 1, 30., &[0, 1, 2, 3], config());
    assert!(w.run_until(1200, |w| w.all_in_lobby()));
    w.ready_everyone();
    assert!(
        w.run_until(1500, |w| w.server.stage() == Stage::Match),
        "everyone became ready despite the loss"
    );
}

#[test]
fn a_laggy_lossy_network_still_produces_a_finished_match() {
    let mut w = world(6, 4, 10., &[1, 2, 3, 0], config());
    play_a_match(&mut w);
    let log = w.server.match_log().last().unwrap().clone();
    let (sent, dropped) = w.net.counts();
    assert!(
        dropped > sent / 20,
        "the network really was lossy: {dropped} of {sent}"
    );
    let received: u64 = log.net.peers.iter().map(|p| p.stats.inputs_received).sum();
    let repeated: u64 = log.net.peers.iter().map(|p| p.stats.ticks_repeated).sum();
    assert!(
        repeated * 10 < received,
        "redundant bundles: {repeated} repeated ticks of {received} inputs"
    );
    for p in &log.net.peers {
        assert!(
            p.rtt_ms_mean > 60. && p.rtt_ms_mean < 400.,
            "rtt reported: {}",
            p.rtt_ms_mean
        );
    }
}

#[test]
fn prediction_statistics_survive_the_return_to_the_lobby() {
    // Loss and jitter make the server repeat inputs, so the predicted runner is corrected now and then.
    let mut w = world(6, 4, 15., &[0, 1], config());
    play_a_match(&mut w);
    let before: Vec<_> = w.clients.iter().map(|c| c.stats().prediction).collect();
    assert!(
        before.iter().any(|p| p.corrections > 0),
        "the scenario really did force corrections: {before:?}"
    );
    assert!(
        w.run_until(600, |w| w.server.stage() == Stage::Lobby
            && w.all_in_lobby()),
        "back in the lobby"
    );
    for (c, was) in w.clients.iter().zip(&before) {
        let now = c.stats().prediction;
        assert!(
            now.corrections >= was.corrections,
            "the record was kept: {was:?} then {now:?}"
        );
        assert!(now.max_error >= was.max_error);
    }
}

#[test]
fn a_full_server_a_wrong_key_and_a_wrong_version_are_turned_away() {
    let cfg = ServerConfig {
        join_key: Some("hunter2".into()),
        ..config()
    };
    let mut w = world(1, 0, 0., &[0, 1, 2, 3, 0, 1, 2, 3], cfg);
    assert!(w.run_until(600, |w| w.all_in_lobby()));
    let mut ninth = client_at(&w.net, 50, 0, "hunter2");
    let mut wrong_key = w.net.endpoint(addr(51));
    let mut old_build = w.net.endpoint(addr(52));
    let hello = |key: &str, nonce, fingerprint| {
        encode_client::<ToyGame>(&ClientMsg::Hello {
            key: key.into(),
            name: "x".into(),
            choice: 0,
            nonce,
            fingerprint,
        })
    };
    let good = w.server.fingerprint();
    wrong_key
        .send(addr(0), &hello("nope", [1, 1], good))
        .unwrap();
    old_build
        .send(addr(0), &hello("hunter2", [2, 2], 12345))
        .unwrap();
    for i in 0..120 {
        w.step();
        ninth.poll((w.tick + i) as f64 / 60.);
    }
    assert!(
        matches!(ninth.state(), ClientState::Rejected(r) if r.contains("full")),
        "{:?}",
        ninth.state()
    );
    assert_eq!(ninth.failure(), Some(ConnectFailure::Full));
    assert_eq!(w.server.players(), 8);
    let reasons: Vec<String> = [&mut wrong_key, &mut old_build]
        .into_iter()
        .flat_map(|end| end.receive().unwrap())
        .filter_map(|d| match decode_server::<ToyGame>(&d.data) {
            Ok(ServerMsg::Rejected { reason }) => Some(reason),
            _ => None,
        })
        .collect();
    assert!(reasons.iter().any(|r| r.contains("key")), "{reasons:?}");
    assert!(reasons.iter().any(|r| r.contains("version")), "{reasons:?}");
}

#[test]
fn another_address_cannot_use_a_players_token() {
    let mut w = world(1, 0, 0., &[0, 1], config());
    assert!(w.run_until(300, |w| w.all_in_lobby()));
    let victim = w.clients[0].token().unwrap();
    let attacker = w.net.endpoint(addr(60));
    for msg in [
        ClientMsg::Ready {
            token: victim,
            ready: true,
        },
        ClientMsg::Select {
            token: victim,
            choice: 3,
        },
        ClientMsg::Leave { token: victim },
    ] {
        attacker
            .send(addr(0), &encode_client::<ToyGame>(&msg))
            .unwrap();
    }
    w.run_until(60, |_| false);
    let lobby = w.clients[1].lobby().unwrap();
    assert_eq!(lobby.entries.len(), 2, "the victim was not removed");
    assert!(
        lobby.entries.iter().all(|e| !e.ready && e.choice != 3),
        "nothing changed: {lobby:?}"
    );
}

#[test]
fn garbage_is_counted_and_the_match_carries_on() {
    let mut w = world(2, 0, 0., &[0, 1], config());
    assert!(w.run_until(300, |w| w.all_in_lobby()));
    let junk = w.net.endpoint(addr(70));
    let mut x = 7u64;
    for _ in 0..300 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let bytes: Vec<u8> = (0..(x % 300)).map(|j| (x >> (j % 56)) as u8).collect();
        junk.send(addr(0), &bytes).unwrap();
    }
    w.ready_everyone();
    assert!(w.run_until(900, |w| w.server.stage() == Stage::Match));
    assert!(w.run_until(60 * 60, |w| w.server.stage() == Stage::Results));
    assert!(
        w.server
            .match_log()
            .last()
            .unwrap()
            .net
            .server
            .bad_datagrams
            >= 250,
        "junk was counted"
    );
}

#[test]
fn a_player_who_leaves_mid_match_is_handed_to_the_games_ai() {
    let mut w = world(2, 0, 0., &[0, 1, 2], config());
    assert!(w.run_until(300, |w| w.all_in_lobby()));
    w.ready_everyone();
    assert!(w.run_until(900, |w| w.server.stage() == Stage::Match));
    w.run_until(120, |_| false);
    let leaver = w.clients[1].participant().expect("knows its participant");
    w.clients[1].leave();
    w.run_until(30, |_| false);
    assert!(
        !w.server.current().unwrap().human[leaver],
        "the game was told to take over"
    );
    assert_eq!(w.server.players(), 2);
    assert!(w.run_until(60 * 60, |w| w.server.stage() == Stage::Results));
    assert!(w
        .server
        .match_log()
        .last()
        .unwrap()
        .net
        .peers
        .iter()
        .any(|p| p.left_early));
}

#[test]
fn a_silent_player_times_out_and_is_handed_over() {
    let cfg = ServerConfig {
        session_timeout: Duration::from_millis(150),
        ..config()
    };
    let mut w = world(1, 0, 0., &[0, 1], cfg);
    assert!(w.run_until(300, |w| w.all_in_lobby()));
    w.ready_everyone();
    assert!(w.run_until(900, |w| w.server.stage() == Stage::Match));
    w.run_until(60, |_| false);
    let dark = w.clients.pop().unwrap();
    w.run_until(15, |_| false); // its last datagrams land
    std::thread::sleep(Duration::from_millis(250));
    w.run_until(5, |_| false);
    assert_eq!(w.server.players(), 1, "the silent player was dropped");
    assert_eq!(
        w.server
            .current()
            .unwrap()
            .human
            .iter()
            .filter(|h| **h)
            .count(),
        1
    );
    drop(dark);
}

#[test]
fn a_late_joiner_waits_out_the_match_and_is_welcome_afterwards() {
    let mut w = world(1, 0, 0., &[0, 1], config());
    assert!(w.run_until(300, |w| w.all_in_lobby()));
    w.ready_everyone();
    assert!(w.run_until(900, |w| w.server.stage() == Stage::Match));
    let mut newcomer = client_at(&w.net, 80, 3, "");
    for i in 0..30 {
        w.step();
        newcomer.poll((w.tick + i) as f64 / 60.);
    }
    assert!(
        matches!(newcomer.state(), ClientState::Rejected(r) if r.contains("match")),
        "{:?}",
        newcomer.state()
    );
    assert_eq!(newcomer.failure(), Some(ConnectFailure::MatchInProgress));
    assert!(newcomer
        .failure()
        .unwrap()
        .hint()
        .contains("between rounds"));
    assert!(w.run_until(60 * 60, |w| w.server.stage() == Stage::Results));
    assert!(w.run_until(600, |w| w.server.stage() == Stage::Lobby));
    let mut second = client_at(&w.net, 81, 3, "");
    for i in 0..90 {
        w.step();
        second.poll((w.tick + i) as f64 / 60.);
    }
    assert_eq!(*second.state(), ClientState::Lobby);
    assert_eq!(w.server.players(), 3);
}

#[test]
fn events_reach_every_client_exactly_once_even_with_loss() {
    let mut w = world(3, 1, 5., &[0, 1], config());
    assert!(w.run_until(400, |w| w.all_in_lobby()));
    w.ready_everyone();
    assert!(w.run_until(900, |w| w.server.stage() == Stage::Match));
    let mut seen: Vec<Vec<ToyEvent>> = vec![Vec::new(); 2];
    let mut ticks = 0;
    while w.server.stage() != Stage::Results && ticks < 60 * 60 {
        w.step();
        ticks += 1;
        for (i, c) in w.clients.iter_mut().enumerate() {
            seen[i].extend(c.drain_events());
        }
    }
    w.run_until(30, |_| false);
    for (i, c) in w.clients.iter_mut().enumerate() {
        seen[i].extend(c.drain_events());
        let started = seen[i]
            .iter()
            .filter(|e| matches!(e, ToyEvent::Started))
            .count();
        let finished = seen[i]
            .iter()
            .filter(|e| matches!(e, ToyEvent::Finished { .. }))
            .count();
        assert_eq!((started, finished), (1, 1), "client {i} saw {:?}", seen[i]);
    }
}

#[test]
fn every_finished_match_appends_one_json_line_to_matches_jsonl() {
    let dir = std::env::temp_dir().join(format!("netplay-log-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cfg = ServerConfig {
        report_dir: Some(dir.clone()),
        ..config()
    };
    let mut w = world(1, 0, 0., &[0, 1], cfg);
    play_a_match(&mut w);
    let text = std::fs::read_to_string(dir.join("matches.jsonl")).expect("the log was written");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1);
    let entry: MatchLog = serde_json::from_str(lines[0]).expect("a MatchLog per line");
    assert_eq!(entry.match_index, 1);
    assert!(entry.net.server.ticks > 100 && entry.net.server.snapshot_bytes_max <= MAX_DATAGRAM);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the envelope ----

fn sample_snapshot() -> ToySnapshot {
    ToySnapshot {
        tick: 9,
        position: vec![1., 2., 3.],
        speed: vec![0.9; 3],
        human: vec![true, false, true],
        winner: Some(1),
    }
}

#[test]
fn every_message_round_trips_through_the_envelope() {
    let token = [0x1122_3344_5566_7788, 0x99aa_bbcc_ddee_ff00];
    let clients = [
        ClientMsg::Hello {
            key: "k".into(),
            name: "Kevin".into(),
            choice: 3,
            nonce: [7, 9],
            fingerprint: 5,
        },
        ClientMsg::Select { token, choice: 2 },
        ClientMsg::Ready { token, ready: true },
        ClientMsg::Input {
            token,
            ack_tick: 1234,
            frames: vec![
                (10, ToyInput { throttle: -1 }),
                (11, ToyInput { throttle: 1 }),
            ],
        },
        ClientMsg::Ping {
            token,
            stamp: 42,
            rtt_ms: 31,
        },
        ClientMsg::Leave { token },
    ];
    for m in clients {
        assert_eq!(
            decode_client::<ToyGame>(&encode_client::<ToyGame>(&m)).unwrap(),
            m
        );
    }
    let servers = [
        ServerMsg::Welcome {
            token,
            slot: 4,
            fingerprint: 99,
        },
        ServerMsg::Rejected {
            reason: "Server is full".into(),
        },
        ServerMsg::Lobby(LobbyState {
            stage: 0,
            seconds_left: 5,
            participants: 8,
            entries: vec![LobbyEntry {
                slot: 0,
                choice: 1,
                ready: true,
                name: "Ann".into(),
            }],
        }),
        ServerMsg::Snapshot(SnapshotMsg {
            server_tick: 777,
            applied_seq: 770,
            you: 2,
            snapshot: sample_snapshot(),
            events: vec![
                (5, ToyEvent::Started),
                (9, ToyEvent::Finished { participant: 1 }),
            ],
        }),
        ServerMsg::Pong { stamp: 5 },
    ];
    for m in servers {
        assert_eq!(
            decode_server::<ToyGame>(&encode_server::<ToyGame>(&m)).unwrap(),
            m
        );
    }
}

#[test]
fn truncated_and_garbage_datagrams_never_panic_the_decoders() {
    let token = [1, 2];
    let samples: Vec<Vec<u8>> = vec![
        encode_server::<ToyGame>(&ServerMsg::Snapshot(SnapshotMsg {
            server_tick: 1,
            applied_seq: 1,
            you: 0,
            snapshot: sample_snapshot(),
            events: vec![(1, ToyEvent::Started)],
        })),
        encode_client::<ToyGame>(&ClientMsg::Input {
            token,
            ack_tick: 1,
            frames: vec![(1, ToyInput { throttle: 1 }); 4],
        }),
        encode_client::<ToyGame>(&ClientMsg::Hello {
            key: "k".into(),
            name: "n".into(),
            choice: 0,
            nonce: [1, 2],
            fingerprint: 1,
        }),
    ];
    for bytes in &samples {
        for len in 0..bytes.len() {
            let _ = decode_server::<ToyGame>(&bytes[..len]);
            let _ = decode_client::<ToyGame>(&bytes[..len]);
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert!(
            decode_server::<ToyGame>(&longer).is_err()
                && decode_client::<ToyGame>(&longer).is_err()
        );
    }
    let mut x = 0x2545_F491_4F6C_DD1Du64;
    for _ in 0..20_000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let len = (x % 200) as usize;
        let mut bytes: Vec<u8> = (0..len)
            .map(|_| x.rotate_left(len as u32) as u8 ^ (x >> 9) as u8)
            .collect();
        if x.is_multiple_of(2) && bytes.len() >= 4 {
            bytes[..2].copy_from_slice(b"NP");
            bytes[2] = 1;
            bytes[3] = [1, 2, 3, 4, 5, 6, 101, 102, 103, 104, 105][(x % 11) as usize];
        }
        let _ = decode_client::<ToyGame>(&bytes);
        let _ = decode_server::<ToyGame>(&bytes);
    }
}

#[test]
fn a_snapshot_that_cannot_fit_a_datagram_is_reported_by_its_size_so_it_is_never_sent() {
    let huge = ToySnapshot {
        tick: 1,
        position: vec![0.; 200],
        speed: vec![1.; 200],
        human: vec![false; 200],
        winner: None,
    };
    let bytes = encode_server::<ToyGame>(&ServerMsg::Snapshot(SnapshotMsg {
        server_tick: 1,
        applied_seq: 0,
        you: 0,
        snapshot: huge,
        events: vec![],
    }));
    assert!(bytes.len() > MAX_DATAGRAM);
    assert!(
        decode_server::<ToyGame>(&bytes).is_err(),
        "and the receiver would refuse it anyway"
    );
}

#[test]
fn oversize_and_foreign_datagrams_are_refused() {
    let good = encode_client::<ToyGame>(&ClientMsg::Ping {
        token: [1, 2],
        stamp: 1,
        rtt_ms: 0,
    });
    let mut wrong_magic = good.clone();
    wrong_magic[0] = b'X';
    let mut wrong_version = good.clone();
    wrong_version[2] += 1;
    assert!(decode_client::<ToyGame>(&wrong_magic).is_err());
    assert!(decode_client::<ToyGame>(&wrong_version).is_err());
    assert!(decode_client::<ToyGame>(&vec![0u8; MAX_DATAGRAM + 1]).is_err());
    let mut w = Writer::new();
    w.f32(f32::NAN);
    assert!(Reader::new(&w.finish()).f32().is_err());
    let _ = ToyGame::NAME;
}

/// Run a fresh client against a live server until it settles, returning how it failed.
fn failure_of(w: &mut World, mut client: Client) -> (ClientState, Option<ConnectFailure>) {
    for i in 0..120 {
        w.step();
        client.poll((w.tick + i) as f64 / 60.);
    }
    (client.state().clone(), client.failure())
}

#[test]
fn a_wrong_key_and_a_wrong_version_each_classify_as_what_they_are() {
    let cfg = ServerConfig {
        join_key: Some("hunter2".into()),
        ..config()
    };
    let mut w = world(1, 0, 0., &[], cfg);
    let wrong_key = client_at(&w.net, 70, 0, "nope");
    let (state, failure) = failure_of(&mut w, wrong_key);
    assert!(matches!(state, ClientState::Rejected(_)), "{state:?}");
    assert_eq!(failure, Some(ConnectFailure::WrongKey));

    // A build whose fingerprint differs: a client of another game name hashes differently, so forge the
    // Hello by hand and read the answer through the client's own decoder path.
    let mut old_build = w.net.endpoint(addr(71));
    old_build
        .send(
            addr(0),
            &encode_client::<ToyGame>(&ClientMsg::Hello {
                key: "hunter2".into(),
                name: "x".into(),
                choice: 0,
                nonce: [3, 3],
                fingerprint: 1,
            }),
        )
        .unwrap();
    for _ in 0..30 {
        w.step();
    }
    let reason = old_build
        .receive()
        .unwrap()
        .into_iter()
        .find_map(|d| match decode_server::<ToyGame>(&d.data) {
            Ok(ServerMsg::Rejected { reason }) => Some(reason),
            _ => None,
        })
        .expect("the server answers a stale build");
    assert_eq!(
        ConnectFailure::classify(&reason),
        ConnectFailure::VersionMismatch
    );
    assert_eq!(w.server.players(), 0);
}

#[test]
fn a_closed_loopback_port_is_unreachable_not_refused() {
    use vesper3d::viewer::net::UdpTransport;
    // Bind to learn a free port, then close it: nothing listens there any more.
    let closed: SocketAddr = {
        let probe = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        probe.local_addr().unwrap()
    };
    let mut client: NetClient<ToyGame, UdpTransport> = NetClient::new(
        UdpTransport::bind("127.0.0.1:0").unwrap(),
        closed,
        ClientConfig {
            name: "Lost".into(),
            key: String::new(),
            choice: 0,
        },
    )
    .unwrap();
    for i in 0..=(9 * 10) {
        client.poll(i as f64 / 10.);
    }
    assert_eq!(
        client.failure(),
        Some(ConnectFailure::Unreachable {
            addr: closed,
            waited_secs: 8
        })
    );
    let ClientState::Rejected(text) = client.state() else {
        panic!("{:?}", client.state())
    };
    assert_eq!(text, &format!("No reply from {closed} after 8 s"));
    assert!(!text.contains("turned you away"));
}
