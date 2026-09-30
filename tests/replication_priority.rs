//! Replication spends its packet budget on what is near the observer first, without starving anything.
use std::{collections::HashMap, net::SocketAddr, sync::Mutex};
use vesper3d::viewer::{
    controller::Movement,
    net::{Datagram, DatagramTransport, Packet, PROTOCOL_VERSION},
    server::DedicatedServer,
    simulation::HeadlessWorld,
};

#[derive(Default)]
struct Recorder {
    last: Mutex<HashMap<SocketAddr, Vec<u8>>>,
}
impl DatagramTransport for Recorder {
    fn send(&self, peer: SocketAddr, data: &[u8]) -> vesper3d::Result<usize> {
        if matches!(
            Packet::decode(data),
            Ok(Packet::Delta(_) | Packet::Snapshot(_))
        ) {
            self.last.lock().unwrap().insert(peer, data.to_vec());
        }
        Ok(data.len())
    }
    fn receive(&mut self) -> vesper3d::Result<Vec<Datagram>> {
        Ok(Vec::new())
    }
    fn local_addr(&self) -> vesper3d::Result<SocketAddr> {
        Ok("127.0.0.1:1".parse().unwrap())
    }
}

struct Outcome {
    /// Broadcasts since each other player was last sent to the watcher, sampled every 5th round.
    all: Vec<f64>,
    near: Vec<f64>,
    far: Vec<f64>,
    mean_wait: f64,
    wait_max: u64,
    delivered_every_round: bool,
}

/// One watched peer in a crowd where everyone keeps moving, so every record changes every broadcast.
fn crowd(players: usize) -> Outcome {
    let mut server =
        DedicatedServer::with_transport(Recorder::default(), HeadlessWorld::new().unwrap())
            .unwrap()
            .with_max_players(players);
    let hash = server.world.content_hash;
    let addr = |i: usize| -> SocketAddr {
        format!("10.0.{}.{}:{}", i / 250, i % 250 + 1, 4000 + i)
            .parse()
            .unwrap()
    };
    for i in 0..players {
        server.handle_hello(addr(i), PROTOCOL_VERSION, i as u64 + 1, hash);
    }
    let ids: Vec<u64> = {
        let mut v: Vec<u64> = server.sessions.keys().copied().collect();
        v.sort_unstable();
        v
    };
    let watcher = ids[0];
    let watcher_addr = server.sessions[&watcher].addr;
    let mut last_seen: HashMap<u64, u64> = HashMap::new();
    let (mut all, mut near, mut far) = (Vec::new(), Vec::new(), Vec::new());
    let mut delivered_every_round = true;
    for round in 0..240u64 {
        for (k, &id) in ids.iter().enumerate() {
            let t = (round as f32 * 0.05 + k as f32).sin();
            server.world.input(
                id,
                Movement {
                    forward: 1.0,
                    right: t,
                    ..Movement::default()
                },
                t,
                0.0,
            );
        }
        for _ in 0..3 {
            server.world.step();
        }
        server.try_broadcast_snapshots().unwrap();
        for s in server.sessions.values_mut() {
            if let Some(target) = s.replication.pending_target() {
                s.replication.acknowledge(target);
            }
        }
        let mut seen_now = 0;
        if let Some(bytes) = server.transport.last.lock().unwrap().remove(&watcher_addr) {
            match Packet::decode(&bytes).unwrap() {
                Packet::Delta(d) => d.changed_players.iter().for_each(|p| {
                    last_seen.insert(p.id, round);
                    seen_now += 1;
                }),
                Packet::Snapshot(s) => s.players.iter().for_each(|p| {
                    last_seen.insert(p.id, round);
                    seen_now += 1;
                }),
                _ => {}
            }
        }
        if round >= 60 && seen_now < players - 1 {
            delivered_every_round = false;
        }
        if round >= 60 && round % 5 == 0 {
            let snapshot = server.world.snapshot_for_player(watcher, 0);
            let me = snapshot
                .players
                .iter()
                .find(|p| p.id == watcher)
                .unwrap()
                .position;
            let mut ranked: Vec<(f32, u64)> = snapshot
                .players
                .iter()
                .filter(|p| p.id != watcher)
                .map(|p| {
                    (
                        ((p.position.0 - me.0).powi(2) + (p.position.2 - me.2).powi(2)).sqrt(),
                        p.id,
                    )
                })
                .collect();
            ranked.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (i, (_, id)) in ranked.iter().enumerate() {
                let age = (round - last_seen.get(id).copied().unwrap_or(0)) as f64;
                all.push(age);
                if i < 8 {
                    near.push(age);
                }
                if i + 8 >= ranked.len() {
                    far.push(age);
                }
            }
        }
    }
    let counters = &server.sessions[&watcher].replication.counters;
    Outcome {
        all,
        near,
        far,
        mean_wait: counters.mean_wait(),
        wait_max: counters.wait_max,
        delivered_every_round,
    }
}
fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}
fn max(v: &[f64]) -> f64 {
    v.iter().copied().fold(0., f64::max)
}

#[test]
fn the_nearest_players_are_much_fresher_than_the_farthest() {
    let crowd = crowd(48);
    let (near, far) = (mean(&crowd.near), mean(&crowd.far));
    assert!(
        near * 3. < far,
        "nearest 8 average {near:.1} broadcasts stale, farthest 8 {far:.1}"
    );
    assert!(
        near < 5.,
        "the nearest players are sent within a few broadcasts: {near:.1}"
    );
}

#[test]
fn nobody_starves_however_far_away() {
    let crowd = crowd(48);
    assert!(
        max(&crowd.all) <= 70.,
        "the longest any player went unsent: {}",
        max(&crowd.all)
    );
    assert!(crowd.wait_max <= 70, "counters agree: {}", crowd.wait_max);
    assert!(
        crowd.all.iter().all(|age| *age < 1e9),
        "every player was sent at least once"
    );
}

#[test]
fn a_crowd_that_fits_one_packet_is_sent_everything_every_broadcast() {
    let crowd = crowd(4);
    assert!(
        crowd.delivered_every_round,
        "with 3 others every changed record fits every packet"
    );
    assert_eq!(max(&crowd.all), 0.0);
}

#[test]
fn the_freshness_counters_measure_the_wait_directly() {
    let small = crowd(4);
    assert!(
        (small.mean_wait - 1.0).abs() < 0.05,
        "sent at the first chance: {}",
        small.mean_wait
    );
    let large = crowd(48);
    assert!(
        large.mean_wait > 2.0 && large.mean_wait < 40.0,
        "{}",
        large.mean_wait
    );
    assert!(large.wait_max as f64 >= large.mean_wait);
}
