//! How stale is each player's state for a watching client as the crowd grows, and does distance matter?
//!
//! `cargo run --profile fast --example net_freshness_bench`
//!
//! Admits N synthetic clients to a real `DedicatedServer` on a transport that only records, moves everyone so every
//! record changes every broadcast, and for one watched client measures how many broadcasts (50 ms each) ago each
//! other player was last sent, separately for the 8 nearest and the 8 farthest players. A packet holds only about six
//! player records, so past that size freshness is decided entirely by how the budget is spent.
use std::{collections::HashMap, net::SocketAddr, sync::Mutex};
use vesper3d::viewer::{
    controller::Movement,
    net::{Datagram, DatagramTransport, Packet, PROTOCOL_VERSION},
    server::DedicatedServer,
    simulation::HeadlessWorld,
};
#[derive(Default)]
struct Rec {
    last: Mutex<HashMap<SocketAddr, Vec<u8>>>,
}
impl DatagramTransport for Rec {
    fn send(&self, peer: SocketAddr, data: &[u8]) -> vesper3d::Result<usize> {
        if let Ok(Packet::Delta(_) | Packet::Snapshot(_)) = Packet::decode(data) {
            self.last.lock().unwrap().insert(peer, data.to_vec());
        }
        Ok(data.len())
    }
    fn receive(&mut self) -> vesper3d::Result<Vec<Datagram>> {
        Ok(vec![])
    }
    fn local_addr(&self) -> vesper3d::Result<SocketAddr> {
        Ok("127.0.0.1:1".parse().unwrap())
    }
}
fn main() -> vesper3d::Result<()> {
    println!("{:>7} {:>10} {:>10} {:>9} | staleness of the 8 NEAREST players: mean / max | 8 FARTHEST: mean / max  (broadcasts; 1 = 50 ms)", "players", "mean", "p95", "max");
    for n in [8usize, 16, 32, 64, 128, 256] {
        let mut server = DedicatedServer::with_transport(Rec::default(), HeadlessWorld::new()?)?
            .with_max_players(n);
        let hash = server.world.content_hash;
        let addrs: Vec<SocketAddr> = (0..n)
            .map(|i| {
                format!("10.0.{}.{}:{}", i / 250, i % 250 + 1, 4000 + i)
                    .parse()
                    .unwrap()
            })
            .collect();
        for (i, a) in addrs.iter().enumerate() {
            server.handle_hello(*a, PROTOCOL_VERSION, i as u64 + 1, hash);
        }
        let ids: Vec<u64> = {
            let mut v: Vec<u64> = server.sessions.keys().copied().collect();
            v.sort();
            v
        };
        // one peer to observe (index 0); everyone keeps moving
        let watcher = ids[0];
        let watcher_addr = server.sessions[&watcher].addr;
        let mut last_seen: HashMap<u64, u64> = HashMap::new();
        let (mut all, mut near, mut far) = (Vec::new(), Vec::new(), Vec::new());
        for round in 0..400u64 {
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
            server.try_broadcast_snapshots()?;
            for s in server.sessions.values_mut() {
                if let Some(t) = s.replication.pending_target() {
                    s.replication.acknowledge(t);
                }
            }
            if let Some(bytes) = server.transport.last.lock().unwrap().remove(&watcher_addr) {
                match Packet::decode(&bytes).unwrap() {
                    Packet::Delta(d) => d.changed_players.iter().for_each(|p| {
                        last_seen.insert(p.id, round);
                    }),
                    Packet::Snapshot(s) => s.players.iter().for_each(|p| {
                        last_seen.insert(p.id, round);
                    }),
                    _ => {}
                }
            }
            if round >= 100 && round % 5 == 0 {
                let me = server
                    .world
                    .snapshot_for_player(watcher, 0)
                    .players
                    .into_iter()
                    .find(|p| p.id == watcher)
                    .map(|p| p.position)
                    .unwrap();
                let mut ranked: Vec<(f32, u64)> = server
                    .world
                    .snapshot_for_player(watcher, 0)
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
        let stat = |v: &mut Vec<f64>| {
            v.sort_by(f64::total_cmp);
            (
                v.iter().sum::<f64>() / v.len().max(1) as f64,
                v[v.len() * 95 / 100],
                *v.last().unwrap_or(&0.),
            )
        };
        let (m, p, x) = stat(&mut all);
        let (nm, _, nx) = stat(&mut near);
        let (fm, _, fx) = stat(&mut far);
        println!("{:>7} {:>10.1} {:>10.0} {:>9.0} |                                   {:>5.1} / {:<5.0} |                        {:>5.1} / {:<5.0}", n, m, p, x, nm, nx, fm, fx);
    }
    Ok(())
}
