//! Preparing peers' updates on several threads changes how fast a broadcast is, never what anyone receives.
use serde_json::Value;
use std::{collections::BTreeMap, net::SocketAddr, sync::Mutex};
use vesper3d::viewer::{
    controller::Movement,
    net::{replication::ReplicationSender, Datagram, DatagramTransport, Packet, PROTOCOL_VERSION},
    server::{DedicatedServer, PARALLEL_MIN_PEERS},
    simulation::HeadlessWorld,
};

/// Records every datagram per peer, in send order. `Sync`, like a real socket.
#[derive(Default)]
struct Recorder {
    sent: Mutex<BTreeMap<SocketAddr, Vec<Vec<u8>>>>,
}
impl DatagramTransport for Recorder {
    fn send(&self, peer: SocketAddr, data: &[u8]) -> vesper3d::Result<usize> {
        self.sent
            .lock()
            .unwrap()
            .entry(peer)
            .or_default()
            .push(data.to_vec());
        Ok(data.len())
    }
    fn receive(&mut self) -> vesper3d::Result<Vec<Datagram>> {
        Ok(Vec::new())
    }
    fn local_addr(&self) -> vesper3d::Result<SocketAddr> {
        Ok("127.0.0.1:1".parse().unwrap())
    }
}

fn addr(i: usize) -> SocketAddr {
    format!("10.0.{}.{}:{}", i / 250, i % 250 + 1, 4000 + i)
        .parse()
        .unwrap()
}

/// Session tokens are random per server, so they are blanked before two runs are compared.
fn without_tokens(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("session");
            map.remove("session_token");
            map.values_mut().for_each(without_tokens);
        }
        Value::Array(list) => list.iter_mut().for_each(without_tokens),
        _ => {}
    }
}

fn first_difference(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                match y.get(k) {
                    None => return Some(format!("{path}.{k} missing on the right")),
                    Some(w) => {
                        if let Some(d) = first_difference(v, w, &format!("{path}.{k}")) {
                            return Some(d);
                        }
                    }
                }
            }
            y.keys()
                .find(|k| !x.contains_key(*k))
                .map(|k| format!("{path}.{k} missing on the left"))
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!("{path}: lengths {} vs {}", x.len(), y.len()));
            }
            x.iter()
                .zip(y)
                .enumerate()
                .find_map(|(i, (v, w))| first_difference(v, w, &format!("{path}[{i}]")))
        }
        _ => (a != b).then(|| format!("{path}: {a} vs {b}")),
    }
}

/// What every peer was sent after `rounds` broadcasts of a crowd that keeps moving, with acks and a few losses.
fn crowd(
    players: usize,
    threads: usize,
    rounds: usize,
    ack_every: usize,
) -> BTreeMap<SocketAddr, Vec<Value>> {
    let mut server =
        DedicatedServer::with_transport(Recorder::default(), HeadlessWorld::new().unwrap())
            .unwrap()
            .with_max_players(players)
            .with_network_threads(threads);
    let hash = server.world.content_hash;
    for i in 0..players {
        server.handle_hello(addr(i), PROTOCOL_VERSION, i as u64 + 1, hash);
    }
    assert_eq!(server.sessions.len(), players);
    let ids: Vec<u64> = {
        let mut ids: Vec<u64> = server.sessions.keys().copied().collect();
        ids.sort_unstable();
        ids
    };
    // Tokens are random per server and are written into every packet as two integers of varying width, which
    // moves the envelope size by a byte and so sometimes the number of records that fit. Pin them so two
    // servers can be compared packet for packet.
    for (k, id) in ids.iter().enumerate() {
        let token = [0x1000 + k as u64, 0x2000 + k as u64];
        let session = server.sessions.get_mut(id).unwrap();
        session.session_token = token;
        session.replication = ReplicationSender::for_session(token);
    }
    for round in 0..rounds {
        for (k, &id) in ids.iter().enumerate() {
            let turn = (round as f32 * 0.07 + k as f32).sin();
            server.world.input(
                id,
                Movement {
                    forward: 1.0,
                    right: turn,
                    ..Movement::default()
                },
                turn,
                0.0,
            );
        }
        for _ in 0..3 {
            server.world.step();
        }
        server.try_broadcast_snapshots().unwrap();
        for (k, id) in ids.iter().enumerate() {
            // Most peers acknowledge every packet; a few never do, so they keep retransmitting.
            if k % ack_every != 0 {
                let session = server.sessions.get_mut(id).unwrap();
                if let Some(target) = session.replication.pending_target() {
                    session.replication.acknowledge(target);
                }
            }
        }
    }
    let sent = server.transport.sent.lock().unwrap();
    sent.iter()
        .map(|(peer, packets)| {
            let decoded = packets
                .iter()
                .map(|bytes| {
                    Packet::decode(bytes).expect("every datagram is a protocol packet");
                    let mut v: Value = serde_json::from_slice(bytes).unwrap();
                    without_tokens(&mut v);
                    v
                })
                .collect();
            (*peer, decoded)
        })
        .collect()
}

#[test]
fn every_peer_receives_the_same_packets_at_any_thread_count() {
    let players = PARALLEL_MIN_PEERS * 3;
    let single = crowd(players, 1, 30, 7);
    for threads in [2, 4, 7] {
        let many = crowd(players, threads, 30, 7);
        assert_eq!(single.len(), many.len());
        for (peer, expected) in &single {
            let got = &many[peer];
            assert_eq!(
                expected.len(),
                got.len(),
                "peer {peer}: packet count differs at {threads} threads"
            );
            for (index, (a, b)) in expected.iter().zip(got).enumerate() {
                assert!(
                    a == b,
                    "peer {peer}, packet {index} differs at {threads} threads: {:?}",
                    first_difference(a, b, "")
                );
            }
        }
    }
    let packets: usize = single.values().map(Vec::len).sum();
    assert!(
        packets > players * 30,
        "world and game-state packets were both sent: {packets}"
    );
}

#[test]
fn small_crowds_are_never_split_and_still_work_with_threads_configured() {
    let few = PARALLEL_MIN_PEERS - 1;
    assert_eq!(crowd(few, 1, 10, 5), crowd(few, 8, 10, 5));
}

#[test]
fn zero_threads_means_one_per_core_and_the_count_is_bounded() {
    let server = |n| {
        DedicatedServer::with_transport(Recorder::default(), HeadlessWorld::new().unwrap())
            .unwrap()
            .with_network_threads(n)
            .network_threads()
    };
    let cores = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .clamp(1, 64);
    assert_eq!(server(0), cores);
    assert_eq!(server(3), 3);
    assert_eq!(server(10_000), 64);
}

#[test]
fn the_player_cap_defaults_to_eight_and_can_be_raised() {
    let hello = |server: &mut DedicatedServer<Recorder>, i: usize| {
        let hash = server.world.content_hash;
        server.handle_hello(addr(i), PROTOCOL_VERSION, i as u64 + 1, hash);
    };
    let mut default =
        DedicatedServer::with_transport(Recorder::default(), HeadlessWorld::new().unwrap())
            .unwrap();
    (0..9).for_each(|i| hello(&mut default, i));
    assert_eq!(default.sessions.len(), 8, "the ninth is refused by default");
    let refused = default.transport.sent.lock().unwrap();
    let last = refused[&addr(8)].last().unwrap();
    assert!(String::from_utf8_lossy(last).contains("Server is full (8 players)"));
    drop(refused);

    let mut big =
        DedicatedServer::with_transport(Recorder::default(), HeadlessWorld::new().unwrap())
            .unwrap()
            .with_max_players(40);
    (0..41).for_each(|i| hello(&mut big, i));
    assert_eq!(big.sessions.len(), 40);
    assert_eq!(big.world.max_players(), 40);
    let refused = big.transport.sent.lock().unwrap();
    assert!(String::from_utf8_lossy(refused[&addr(40)].last().unwrap())
        .contains("Server is full (40 players)"));
}

#[test]
fn the_comparison_itself_is_deterministic_for_a_single_thread() {
    let players = PARALLEL_MIN_PEERS * 3;
    let (a, b) = (crowd(players, 1, 30, 7), crowd(players, 1, 30, 7));
    for (peer, expected) in &a {
        for (index, (x, y)) in expected.iter().zip(&b[peer]).enumerate() {
            assert!(
                x == y,
                "peer {peer}, packet {index}: {:?}",
                first_difference(x, y, "")
            );
        }
    }
}
