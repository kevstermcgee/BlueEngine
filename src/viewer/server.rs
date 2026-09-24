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

pub const STALE_INPUT_WINDOW_TICKS: u64 = 6; // 100 ms at 60 Hz
pub const KEYFRAME_INTERVAL: u32 = 20; // Full snapshot every 20 snapshots (~1s at 20 Hz)

/// Session tracking state for one connected client.
#[derive(Clone, Debug)]
pub struct ClientSession {
    pub player_id: u64,
    pub addr: SocketAddr,
    pub connected_at: Instant,
    pub last_seen: Instant,
    pub last_client_tick: u64,
    pub last_input_tick: u64,
    pub last_acked_tick: u64,
    pub pistol_cooldown: f32,
    pub wrench_cooldown: f32,
    pub snapshot_history: std::collections::VecDeque<crate::viewer::net::WorldSnapshot>,
    pub snapshots_since_keyframe: u32,
    pub keyframe_requested: bool,
}

/// Authoritative dedicated server running HeadlessWorld over UDP.
pub struct DedicatedServer {
    pub transport: UdpTransport,
    pub world: HeadlessWorld,
    pub sessions: HashMap<u64, ClientSession>,
    pub clients: HashMap<SocketAddr, u64>,
    pub recent_disconnects: HashMap<u64, (SocketAddr, Instant)>,
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
            recent_disconnects: HashMap::new(),
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
    pub fn handle_hello(
        &mut self,
        src: SocketAddr,
        protocol_version: u32,
        req_id: u64,
        content_hash: u64,
    ) {
        if protocol_version != PROTOCOL_VERSION {
            eprintln!(
                "[Server] Rejected client from {src}: protocol mismatch {protocol_version} != {PROTOCOL_VERSION}"
            );
            return;
        }
        if content_hash != self.world.content_hash {
            let _ = self.transport.send_packet(
                &Packet::Rejected {
                    reason: "Map content mismatch; load the same map as the server".into(),
                },
                src,
            );
            return;
        }
        if !self.clients.contains_key(&src) && self.sessions.len() >= 8 {
            let _ = self.transport.send_packet(
                &Packet::Rejected {
                    reason: "Server is full (8 players)".into(),
                },
                src,
            );
            return;
        }

        // Expire disconnect reservations older than 60s
        let now = Instant::now();
        self.recent_disconnects
            .retain(|_, (_, time)| now.duration_since(*time) < Duration::from_secs(60));

        // Session security: Server assigns authoritative IDs.
        // Reconnecting clients from the same socket address or with valid unexpired reservation retain their ID.
        // Arbitrary requested IDs from untrusted sockets are rejected and assigned fresh sequential IDs.
        let player_id = if let Some(&existing_id) = self.clients.get(&src) {
            existing_id
        } else if req_id > 0
            && self
                .recent_disconnects
                .get(&req_id)
                .is_some_and(|(addr, _)| *addr == src)
        {
            self.recent_disconnects.remove(&req_id);
            req_id
        } else {
            let id = self.next_player_id;
            self.next_player_id += 1;
            id
        };

        // Reset player in world if reconnecting
        self.world.leave(player_id);

        let spawn = self.spawn_point_for(player_id);
        let joined = if self.world.game.is_some() {
            self.world.join(player_id)
        } else {
            self.world.join_at(player_id, spawn)
        };
        if !joined {
            let _ = self.transport.send_packet(
                &Packet::Rejected {
                    reason: "World is full (8 players)".into(),
                },
                src,
            );
            return;
        }

        self.clients.insert(src, player_id);
        self.sessions.insert(
            player_id,
            ClientSession {
                player_id,
                addr: src,
                connected_at: now,
                last_seen: now,
                last_client_tick: 0,
                last_input_tick: self.world.tick,
                last_acked_tick: 0,
                pistol_cooldown: 0.0,
                wrench_cooldown: 0.0,
                snapshot_history: std::collections::VecDeque::with_capacity(64),
                snapshots_since_keyframe: 0,
                keyframe_requested: false,
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
        while count < 256 {
            let Some((packet, src)) = self.transport.recv_packet()? else {
                break;
            };
            count += 1;
            match packet {
                Packet::Hello {
                    protocol_version,
                    player_id,
                    content_hash,
                } => {
                    self.handle_hello(src, protocol_version, player_id, content_hash);
                }
                Packet::Input(frame) => {
                    if let Some(&player_id) = self.clients.get(&src) {
                        let mut should_fire_pistol = false;
                        let mut should_fire_wrench = false;

                        if let Some(session) = self.sessions.get_mut(&player_id) {
                            // Input sequencing enforcement: reject duplicate or out-of-order input frames
                            if frame.client_tick <= session.last_client_tick {
                                continue;
                            }
                            session.last_seen = Instant::now();
                            session.last_client_tick = frame.client_tick;
                            session.last_input_tick = self.world.tick;
                            if frame.ack_server_tick > session.last_acked_tick {
                                session.last_acked_tick = frame.ack_server_tick;
                            }

                            if frame.fire_pistol && session.pistol_cooldown <= 0.0 {
                                session.pistol_cooldown = crate::viewer::weapons::SHOT_INTERVAL;
                                should_fire_pistol = true;
                            }
                            if frame.fire_wrench && session.wrench_cooldown <= 0.0 {
                                session.wrench_cooldown = crate::viewer::wrench::SWING_TIME;
                                should_fire_wrench = true;
                            }
                        }

                        // Authoritative movement
                        if !self
                            .world
                            .input(player_id, frame.movement, frame.yaw, frame.pitch)
                        {
                            continue;
                        }

                        // Multiplayer prop interaction with contention resolution
                        if frame.interact {
                            if self.world.game.is_some() {
                                self.world.request_interaction(player_id);
                            } else if let Some(p) = self.world.player(player_id) {
                                let ray = p.ray();
                                if let Some(ref mut physics) = self.world.prop_physics {
                                    physics.toggle_for_player(player_id, &self.world.room, ray);
                                }
                            }
                        }

                        // Authoritative weapon hitscan and impulse application
                        if should_fire_pistol && self.world.game.is_none() {
                            self.world.fire_pistol(player_id);
                        }
                        if should_fire_wrench && self.world.game.is_none() {
                            self.world.fire_wrench(player_id);
                        }
                    }
                }
                Packet::RequestKeyframe => {
                    if let Some(&player_id) = self.clients.get(&src) {
                        if let Some(session) = self.sessions.get_mut(&player_id) {
                            session.keyframe_requested = true;
                        }
                    }
                }
                Packet::Disconnect { player_id } => {
                    // Session security: Verify that sender actually owns this player ID
                    if self.clients.get(&src) == Some(&player_id) {
                        self.world.leave(player_id);
                        self.sessions.remove(&player_id);
                        self.clients.remove(&src);
                        self.recent_disconnects
                            .insert(player_id, (src, Instant::now()));
                        println!("[Server] Client #{player_id} disconnected gracefully");
                    } else {
                        eprintln!(
                            "[Server] Rejected unauthorized disconnect attempt for #{player_id} from {src}"
                        );
                    }
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
            self.recent_disconnects.insert(id, (addr, Instant::now()));
            println!("[Server] Client #{id} timed out (disconnected)");
            ids.push(id);
        }
        ids
    }

    /// Broadcast spatially filtered snapshots to each connected client with per-client acknowledged delta tracking.
    pub fn broadcast_snapshots(&mut self) {
        for (&id, session) in &mut self.sessions {
            if let Some(game) = &self.world.game {
                let _ = self.transport.send_packet(
                    &Packet::GameState {
                        tick: self.world.tick,
                        state: game.state().clone(),
                    },
                    session.addr,
                );
            }
            let snap = self.world.snapshot_for_player(id, session.last_client_tick);
            let acked_base = if session.last_acked_tick > 0 {
                session
                    .snapshot_history
                    .iter()
                    .find(|s| s.tick == session.last_acked_tick)
            } else {
                None
            };

            let send_keyframe = session.keyframe_requested
                || acked_base.is_none()
                || session.snapshots_since_keyframe >= KEYFRAME_INTERVAL;

            if send_keyframe {
                let _ = self
                    .transport
                    .send_packet(&Packet::Snapshot(snap.clone()), session.addr);
                session.snapshots_since_keyframe = 0;
                session.keyframe_requested = false;
            } else {
                let base = acked_base.unwrap();
                let delta = snap.compute_delta(base);
                let _ = self
                    .transport
                    .send_packet(&Packet::Delta(delta), session.addr);
                session.snapshots_since_keyframe += 1;
            }

            session.snapshot_history.push_back(snap);
            if session.snapshot_history.len() > 60 {
                session.snapshot_history.pop_front();
            }
        }
    }

    /// Step authoritative simulation one tick, neutralize stale inputs, and broadcast snapshots every 3 ticks (20 Hz).
    pub fn step(&mut self) {
        // Cooldown ticks and stale input neutralization
        for (&id, session) in &mut self.sessions {
            session.pistol_cooldown =
                (session.pistol_cooldown - crate::viewer::simulation::TICK_SECONDS).max(0.0);
            session.wrench_cooldown =
                (session.wrench_cooldown - crate::viewer::simulation::TICK_SECONDS).max(0.0);
            if self.world.tick.saturating_sub(session.last_input_tick) > STALE_INPUT_WINDOW_TICKS {
                self.world.neutralize_input(id);
            }
        }

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
