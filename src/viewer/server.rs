//! Authoritative dedicated multiplayer server for Blue Engine V2.
//!
//! Manages transport-agnostic datagram communication, connection handshakes, player sessions,
//! fixed 60 Hz simulation stepping with Rapier physics, graceful disconnects,
//! timeouts, and spatial room interest replication.

use crate::math::V;
use crate::viewer::{
    net::{
        random_nonce, random_salt, random_token, verify_auth_proof, ConnectionNonce,
        DatagramTransport, HandshakeLimiter, Packet, SessionRegistry, SessionToken, UdpTransport,
        PROTOCOL_VERSION,
    },
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
    pub session_token: SessionToken,
    pub authenticated: bool,
    pub action_tracker: crate::viewer::net::action_counters::ActionCountersTracker,
}

/// Authoritative dedicated server running [`HeadlessWorld`] over any datagram transport.
pub struct DedicatedServer<T: DatagramTransport = UdpTransport> {
    pub transport: T,
    pub world: HeadlessWorld,
    pub sessions: HashMap<u64, ClientSession>,
    pub clients: HashMap<SocketAddr, u64>,
    pub recent_disconnects: HashMap<u64, (SocketAddr, Instant)>,
    pub next_player_id: u64,
    pub client_timeout: Duration,
    pub local_addr: SocketAddr,
    pub auth_key: Option<String>,
    pub pending_challenges: HashMap<SocketAddr, (ConnectionNonce, [u8; 16], Instant)>,
    pub handshake_limiter: HandshakeLimiter,
    pub session_registry: SessionRegistry<u64>,
}

impl DedicatedServer<UdpTransport> {
    /// Bind to a local address (e.g. `"0.0.0.0:4000"` or `"127.0.0.1:0"`) with the default Test Lab map.
    pub fn bind(addr: &str) -> crate::Result<Self> {
        let world = HeadlessWorld::new()?;
        Self::with_world(addr, world)
    }

    /// Bind to a local address with a custom authoritative simulation world.
    pub fn with_world(addr: &str, world: HeadlessWorld) -> crate::Result<Self> {
        let transport = UdpTransport::bind(addr)?;
        Self::with_transport(transport, world)
    }
}

impl<T: DatagramTransport> DedicatedServer<T> {
    /// Construct an authoritative server over an already configured transport.
    pub fn with_transport(transport: T, world: HeadlessWorld) -> crate::Result<Self> {
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
            auth_key: None,
            pending_challenges: HashMap::new(),
            handshake_limiter: HandshakeLimiter::new(64),
            session_registry: SessionRegistry::new(16, Duration::from_secs(5)),
        })
    }

    /// Configure a shared secret key for mandatory client authentication.
    pub fn with_auth(mut self, auth_key: &str) -> Self {
        self.auth_key = Some(auth_key.to_string());
        self
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
        let now = Instant::now();
        if !self.handshake_limiter.allow(now) {
            eprintln!("[Server] Handshake rate limit exceeded for {src}");
            return;
        }
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

        // If authentication key is required, send cryptographic AuthChallenge
        if self.auth_key.is_some() {
            let nonce = random_nonce().unwrap_or([req_id, now.elapsed().as_nanos() as u64]);
            let salt = random_salt().unwrap_or([0u8; 16]);
            self.pending_challenges.insert(src, (nonce, salt, now));
            let challenge = Packet::AuthChallenge { nonce, salt };
            let _ = self.transport.send_packet(&challenge, src);
            return;
        }

        // Expire disconnect reservations older than 60s
        self.recent_disconnects
            .retain(|_, (_, time)| now.duration_since(*time) < Duration::from_secs(60));

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

        let token = random_token().unwrap_or([player_id, 0xcafe]);
        let _ = self
            .session_registry
            .register(src, [player_id, 0], now, player_id);

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
                session_token: token,
                authenticated: true,
                action_tracker: Default::default(),
            },
        );

        let welcome = Packet::Welcome {
            player_id,
            server_tick: self.world.tick,
            map_name: self.world.room.name.clone(),
            session_token: Some(token),
        };
        let _ = self.transport.send_packet(&welcome, src);
        println!("[Server] Client #{player_id} connected from {src} (spawn {spawn:?})");
    }

    /// Handle client authentication response (`Packet::AuthResponse`).
    pub fn handle_auth_response(
        &mut self,
        src: SocketAddr,
        player_id: u64,
        nonce: ConnectionNonce,
        proof: [u8; 32],
        content_hash: u64,
    ) {
        let now = Instant::now();
        let Some((expected_nonce, salt, challenge_time)) = self.pending_challenges.remove(&src)
        else {
            let _ = self.transport.send_packet(
                &Packet::Rejected {
                    reason: "No pending authentication challenge".into(),
                },
                src,
            );
            return;
        };

        if now.duration_since(challenge_time) > Duration::from_secs(10) {
            let _ = self.transport.send_packet(
                &Packet::Rejected {
                    reason: "Authentication challenge expired".into(),
                },
                src,
            );
            return;
        }

        if nonce != expected_nonce {
            let _ = self.transport.send_packet(
                &Packet::Rejected {
                    reason: "Authentication nonce mismatch".into(),
                },
                src,
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

        if let Some(ref key) = self.auth_key {
            if !verify_auth_proof(key, nonce, player_id, &salt, &proof) {
                eprintln!("[Server] Client from {src} failed HMAC authentication proof");
                let _ = self.transport.send_packet(
                    &Packet::Rejected {
                        reason: "Invalid authentication credentials or proof".into(),
                    },
                    src,
                );
                return;
            }
        }

        let assigned_id = if let Some(&existing_id) = self.clients.get(&src) {
            existing_id
        } else if player_id > 0
            && self
                .recent_disconnects
                .get(&player_id)
                .is_some_and(|(addr, _)| *addr == src)
        {
            self.recent_disconnects.remove(&player_id);
            player_id
        } else {
            let id = self.next_player_id;
            self.next_player_id += 1;
            id
        };

        self.world.leave(assigned_id);

        let spawn = self.spawn_point_for(assigned_id);
        let joined = if self.world.game.is_some() {
            self.world.join(assigned_id)
        } else {
            self.world.join_at(assigned_id, spawn)
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

        let token = random_token().unwrap_or([assigned_id, 0xcafe]);
        let _ = self.session_registry.register(src, nonce, now, assigned_id);

        self.clients.insert(src, assigned_id);
        self.sessions.insert(
            assigned_id,
            ClientSession {
                player_id: assigned_id,
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
                session_token: token,
                authenticated: true,
                action_tracker: Default::default(),
            },
        );

        let welcome = Packet::Welcome {
            player_id: assigned_id,
            server_tick: self.world.tick,
            map_name: self.world.room.name.clone(),
            session_token: Some(token),
        };
        let _ = self.transport.send_packet(&welcome, src);
        println!(
            "[Server] Authenticated client #{assigned_id} connected from {src} (spawn {spawn:?})"
        );
    }

    /// Poll and process all pending incoming network packets non-blockingly.
    pub fn poll_network(&mut self) -> crate::Result<usize> {
        let mut count = 0;
        for (packet, src) in self.transport.receive_packets()?.into_iter().take(256) {
            count += 1;
            match packet {
                Packet::Hello {
                    protocol_version,
                    player_id,
                    content_hash,
                } => {
                    self.handle_hello(src, protocol_version, player_id, content_hash);
                }
                Packet::AuthResponse {
                    player_id,
                    nonce,
                    proof,
                    content_hash,
                } => {
                    self.handle_auth_response(src, player_id, nonce, proof, content_hash);
                }
                Packet::Input(frame) => {
                    if let Some(&player_id) = self.clients.get(&src) {
                        let mut should_fire_pistol = false;
                        let mut should_fire_wrench = false;

                        if let Some(session) = self.sessions.get_mut(&player_id) {
                            if self.auth_key.is_some()
                                && frame.session_token != Some(session.session_token)
                            {
                                eprintln!("[Server] Dropping unauthorized input frame from {src}");
                                continue;
                            }
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
                Packet::SequencedInput(frame) => {
                    if let Some(&player_id) = self.clients.get(&src) {
                        let mut should_fire_pistol = false;
                        let mut should_fire_wrench = false;
                        let mut should_interact = false;

                        if let Some(session) = self.sessions.get_mut(&player_id) {
                            if self.auth_key.is_some()
                                && frame.session_token != Some(session.session_token)
                            {
                                eprintln!("[Server] Dropping unauthorized sequenced input frame from {src}");
                                continue;
                            }
                            if frame.client_tick <= session.last_client_tick {
                                continue;
                            }
                            session.last_seen = Instant::now();
                            session.last_client_tick = frame.client_tick;
                            session.last_input_tick = self.world.tick;
                            if frame.ack_server_tick > session.last_acked_tick {
                                session.last_acked_tick = frame.ack_server_tick;
                            }

                            let edges = session.action_tracker.update(&frame.counters);
                            if edges.secondary && session.wrench_cooldown <= 0.0 {
                                session.wrench_cooldown = crate::viewer::wrench::SWING_TIME;
                                should_fire_wrench = true;
                            }
                            if edges.primary && session.pistol_cooldown <= 0.0 {
                                session.pistol_cooldown = crate::viewer::weapons::SHOT_INTERVAL;
                                should_fire_pistol = true;
                            }
                            if edges.interact {
                                should_interact = true;
                            }
                        }

                        if !self
                            .world
                            .input(player_id, frame.movement, frame.yaw, frame.pitch)
                        {
                            continue;
                        }

                        if should_interact {
                            if self.world.game.is_some() {
                                self.world.request_interaction(player_id);
                            } else if let Some(p) = self.world.player(player_id) {
                                let ray = p.ray();
                                if let Some(ref mut physics) = self.world.prop_physics {
                                    physics.toggle_for_player(player_id, &self.world.room, ray);
                                }
                            }
                        }

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
                Packet::Disconnect {
                    player_id,
                    session_token,
                } => {
                    // Session security: Verify that sender actually owns this player ID
                    if self.clients.get(&src) == Some(&player_id) {
                        if let Some(session) = self.sessions.get(&player_id) {
                            if self.auth_key.is_some()
                                && session_token.is_some()
                                && session_token != Some(session.session_token)
                            {
                                eprintln!(
                                    "[Server] Rejected disconnect with mismatched session token for #{player_id}"
                                );
                                continue;
                            }
                        }
                        self.world.leave(player_id);
                        if let Some(session) = self.sessions.remove(&player_id) {
                            self.session_registry.remove(&session.session_token);
                        }
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
        let mut runner = crate::viewer::metrics::FixedTickRunner::new(60);
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
                    "[Server] Tick {} | Clients: {} | Active Props: {} | Checksum: {:016x} | mean_us={} max_us={}",
                    self.world.tick,
                    self.sessions.len(),
                    perf.active_dynamic_bodies,
                    self.world.checksum(),
                    runner.metrics.mean_us(),
                    runner.metrics.max_us
                );
                runner.metrics.reset();
                last_status = Instant::now();
            }

            let elapsed = tick_start.elapsed().as_micros();
            runner.sleep_until_next_tick(elapsed);
        }

        println!(
            "[Server] Shutdown complete after {} ticks ({:.2}s)",
            self.world.tick,
            started.elapsed().as_secs_f64()
        );
        Ok(())
    }
}
