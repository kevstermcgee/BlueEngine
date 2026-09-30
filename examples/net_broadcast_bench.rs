//! How much one snapshot broadcast costs as the player count grows, on a transport that only counts.
//!
//! `cargo run --profile fast --example net_broadcast_bench -- [MAX_PLAYERS]`
//! (`THREADS=n` prepares peers on n threads, 0 = every core; `EXACT=1` sizes every record by serializing the whole delta, the original method.)
//!
//! The transport swallows every send, so this measures what the server thread spends encoding and handing
//! off world state for N clients (delta against each peer's baseline, JSON serialization, the send call),
//! not the operating system's socket cost. Clients walk in circles so every snapshot has something to say.
use std::{
    net::SocketAddr,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};
use vesper3d::viewer::{
    controller::Movement,
    net::{Datagram, DatagramTransport, PROTOCOL_VERSION},
    server::DedicatedServer,
    simulation::HeadlessWorld,
};

#[derive(Default)]
struct Sink {
    packets: AtomicU64,
    bytes: AtomicU64,
}
impl DatagramTransport for Sink {
    fn send(&self, _peer: SocketAddr, data: &[u8]) -> vesper3d::Result<usize> {
        self.packets.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(data.len() as u64, Ordering::Relaxed);
        Ok(data.len())
    }
    fn receive(&mut self) -> vesper3d::Result<Vec<Datagram>> {
        Ok(Vec::new())
    }
    fn local_addr(&self) -> vesper3d::Result<SocketAddr> {
        Ok("127.0.0.1:1".parse().unwrap())
    }
}

fn main() -> vesper3d::Result<()> {
    let threads: usize = std::env::var("THREADS")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(1);
    let max: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(256);
    println!(
        "{:>8} {:>13} {:>10} {:>13} {:>10} {:>9}",
        "players", "broadcast us", "ack us", "total us/peer", "pkts", "avg bytes"
    );
    let mut n: usize = std::env::var("START")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(8);
    while n <= max {
        let mut server = DedicatedServer::with_transport(Sink::default(), HeadlessWorld::new()?)?
            .with_max_players(n)
            .with_network_threads(threads);
        let hash = server.world.content_hash;
        for i in 0..n {
            let addr: SocketAddr = format!("10.0.{}.{}:{}", i / 250, i % 250 + 1, 4000 + i)
                .parse()
                .unwrap();
            server.handle_hello(addr, PROTOCOL_VERSION, i as u64 + 1, hash);
        }
        assert_eq!(
            server.sessions.len(),
            n,
            "every synthetic client was admitted"
        );
        if std::env::var_os("EXACT").is_some() {
            server
                .sessions
                .values_mut()
                .for_each(|s| s.replication.set_exact_sizing(true));
        }
        let ids: Vec<u64> = server.sessions.keys().copied().collect();
        let (mut total, mut ack_total, mut rounds) = (0.0, 0.0, 0u32);
        for round in 0..140 {
            for (k, &id) in ids.iter().enumerate() {
                let turn = (round as f32 * 0.05 + k as f32).sin();
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
            let before = Instant::now();
            server.try_broadcast_snapshots()?;
            let broadcast_us = before.elapsed().as_secs_f64() * 1e6;
            // Every client acknowledges what it was sent, as a live connection does about 20 times a second.
            let before = Instant::now();
            for session in server.sessions.values_mut() {
                if let Some(target) = session.replication.pending_target() {
                    session.replication.acknowledge(target);
                }
            }
            let ack_us = before.elapsed().as_secs_f64() * 1e6;
            if round >= 20 {
                total += broadcast_us;
                ack_total += ack_us;
                rounds += 1;
            }
        }
        let mean = total / f64::from(rounds);
        let sent = server.transport.packets.load(Ordering::Relaxed).max(1);
        let avg = server.transport.bytes.load(Ordering::Relaxed) / sent;
        println!(
            "{:>8} {:>13.0} {:>10.0} {:>13.1} {:>10} {:>9}",
            n,
            mean,
            ack_total / f64::from(rounds),
            (mean + ack_total / f64::from(rounds)) / n as f64,
            sent,
            avg
        );
        n = if std::env::var_os("STEP").is_some() {
            n + 16
        } else {
            n * 2
        };
    }
    Ok(())
}
