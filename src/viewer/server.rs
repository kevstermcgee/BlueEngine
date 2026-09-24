//! Authoritative dedicated multiplayer server for Blue Engine V2.
//!
//! Manages UDP socket communication, connection handshakes, player sessions,
//! fixed 60 Hz simulation stepping with Rapier physics, graceful disconnects,
//! timeouts, and spatial room interest replication.

use crate::math::V;
use crate::viewer::{
    net::{Packet, UdpTransport, PROTOCOL_VERSION},
    simulation::HeadlessWorld,
    test_lab::{SPAWN_PLAYER_1, SPAWN_PLAYER_2},
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Session tracking state for one connected client.
#[derive(Clone, Debug)]
pub struct ClientSession {
    pub player_id: u64,
    pub addr: SocketAddr,
    pub connected_at: Instant,
    pub last_seen: Instant,
    pub last_client_tick: u64,
}

/// Authoritative dedicated server running HeadlessWorld over UDP.
pub struct DedicatedServer {
    pub transport: UdpTransport,
    pub world: HeadlessWorld,
    pub sessions: HashMap<u64, ClientSession>,
    pub clients: HashMap<SocketAddr, u64>,
    pub next_player_id: u64,
    pub client_timeout: Duration,
    pub local_addr: SocketAddr,
}

impl DedicatedServer {
    /// Bind to a local address (e.g. `"0.0.0.0:4000"` or `"127.0.0.1:0"`) with the default Test Lab map.
    pub fn bind(addr: &str) -> crate::Result<Self> {
        let world = HeadlessWorld::new()?;
        Self::with_world(addr, world)
    }

    /// Bind to a local address with a custom authoritative simulation world.
    pub fn with_world(addr: &str, world: HeadlessWorld) -> crate::Result<Self> {
        let transport = UdpTransport::bind(addr)?;
        let local_addr = transport.local_addr()?;
        Ok(Self {
            transport,
            world,
            sessions: HashMap::new(),
            clients: HashMap::new(),
            next_player_id: 1,
            client_timeout: Duration::from_secs(5),
            local_addr,
        })
    }

    /// Calculate the default spawn location for a given player ID.
    pub fn spawn_point_for(&self, player_id: u64) -> V {
        match player_id {
            1 => SPAWN_PLAYER_1,
            2 => SPAWN_PLAYER_2,
            n => V((n as f32 - 1.0) * 1.5, 1.68, 6.0),
        }
    }

    /// Handle client connection handshake (`Packet::Hello`).
    pub fn handle_hello(&mut self, src: SocketAddr, protocol_version: u32, req_id: u64) {
        if protocol_version != PROTOCOL_VERSION {
            eprintln!(
                "[Server] Rejected client from {src}: protocol mismatch {protocol_version} != {PROTOCOL_VERSION}"
            );
            return;
        }

        // Determine player ID: if requested ID is already owned by this socket, or available, reuse it
        let player_id = if req_id > 0
            && (!self.sessions.contains_key(&req_id) || self.clients.get(&src) == Some(&req_id))
        {
            req_id
        } else if let Some(&existing_id) = self.clients.get(&src) {
            existing_id
        } else {
            let id = self.next_player_id;
            self.next_player_id += 1;
            id
        };

        // Reset player in world if reconnecting
        self.world.leave(player_id);

        let spawn = self.spawn_point_for(player_id);
        self.world.join_at(player_id, spawn);

        let now = Instant::now();
        self.clients.insert(src, player_id);
        self.sessions.insert(
            player_id,
            ClientSession {
                player_id,
                addr: src,
                connected_at: now,
                last_seen: now,
                last_client_tick: 0,
            },
        );

        let welcome = Packet::Welcome {
            player_id,
            server_tick: self.world.tick,
            map_name: self.world.room.name.clone(),
        };
        let _ = self.transport.send_packet(&welcome, src);
        println!("[Server] Client #{player_id} connected from {src} (spawn {spawn:?})");
    }

    /// Poll and process all pending incoming network packets non-blockingly.
    pub fn poll_network(&mut self) -> crate::Result<usize> {
        let mut count = 0;
        while let Some((packet, src)) = self.transport.recv_packet()? {
            count += 1;
            match packet {
                Packet::Hello {
                    protocol_version,
                    player_id,
                } => {
                    self.handle_hello(src, protocol_version, player_id);
                }
                Packet::Input(frame) => {
                    if let Some(&player_id) = self.clients.get(&src) {
                        if let Some(session) = self.sessions.get_mut(&player_id) {
                            session.last_seen = Instant::now();
                            session.last_client_tick = frame.client_tick;
                        }
                        self.world
                            .input(player_id, frame.movement, frame.yaw, frame.pitch);
                        if frame.interact {
                            if let Some(p) = self.world.player(player_id) {
                                let ray = p.ray();
                                if let Some(ref mut physics) = self.world.prop_physics {
                                    physics.toggle(&self.world.room, ray);
                                }
                            }
                        }
                    }
                }
                Packet::Disconnect { player_id } => {
                    self.world.leave(player_id);
                    self.sessions.remove(&player_id);
                    self.clients.remove(&src);
                    println!("[Server] Client #{player_id} disconnected gracefully");
                }
                Packet::Ping { seq, send_time_ms } => {
                    let pong = Packet::Pong { seq, send_time_ms };
                    let _ = self.transport.send_packet(&pong, src);
                }
                _ => {}
            }
        }
        Ok(count)
    }

    /// Check for timed-out client sessions and remove them from the world.
    pub fn check_timeouts(&mut self) -> Vec<u64> {
        let timeout = self.client_timeout;
        let mut timed_out = Vec::new();
        for (&id, session) in &self.sessions {
            if session.last_seen.elapsed() > timeout {
                timed_out.push((id, session.addr));
            }
        }
        let mut ids = Vec::new();
        for (id, addr) in timed_out {
            self.world.leave(id);
            self.sessions.remove(&id);
            self.clients.remove(&addr);
            println!("[Server] Client #{id} timed out (disconnected)");
            ids.push(id);
        }
        ids
    }

    /// Broadcast spatially filtered snapshots to each connected client.
    pub fn broadcast_snapshots(&mut self) {
        for (&id, session) in &self.sessions {
            let snap = self.world.snapshot_for_player(id, session.last_client_tick);
            let _ = self
                .transport
                .send_packet(&Packet::Snapshot(snap), session.addr);
        }
    }

    /// Step authoritative simulation one tick and broadcast snapshots every 3 ticks (20 Hz).
    pub fn step(&mut self) {
        self.world.step();
        if self.world.tick.is_multiple_of(3) {
            self.broadcast_snapshots();
        }
    }

    /// Run for a fixed number of ticks (useful for automated testing and deterministic benchmarking).
    pub fn run_ticks(&mut self, ticks: u64) -> crate::Result<()> {
        for _ in 0..ticks {
            self.poll_network()?;
            self.check_timeouts();
            self.step();
        }
        Ok(())
    }

    /// Run continuous 60 Hz real-time server loop until stop signal is set or max_ticks is reached.
    pub fn run_realtime(
        &mut self,
        stop_signal: Arc<AtomicBool>,
        max_ticks: Option<u64>,
    ) -> crate::Result<()> {
        let tick_duration = Duration::from_secs_f64(1.0 / 60.0);
        let started = Instant::now();
        let mut last_status = Instant::now();

        println!(
            "[Server] Dedicated server listening on {} (Map: {})",
            self.local_addr, self.world.room.name
        );

        while !stop_signal.load(Ordering::Relaxed) {
            let tick_start = Instant::now();

            self.poll_network()?;
            self.check_timeouts();
            self.step();

            if let Some(max) = max_ticks {
                if self.world.tick >= max {
                    break;
                }
            }

            if last_status.elapsed() >= Duration::from_secs(5) {
                let perf = self
                    .world
                    .performance_snapshot(tick_start.elapsed().as_secs_f64() * 1_000_000.0);
                println!(
                    "[Server] Tick {} | Clients: {} | Active Props: {} | Checksum: {:016x}",
                    self.world.tick,
                    self.sessions.len(),
                    perf.active_dynamic_bodies,
                    self.world.checksum()
                );
                last_status = Instant::now();
            }

            let elapsed = tick_start.elapsed();
            if elapsed < tick_duration {
                std::thread::sleep(tick_duration - elapsed);
            }
        }

        println!(
            "[Server] Shutdown complete after {} ticks ({:.2}s)",
            self.world.tick,
            started.elapsed().as_secs_f64()
        );
        Ok(())
    }
}
