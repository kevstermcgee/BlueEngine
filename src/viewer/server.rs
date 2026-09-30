//! Authoritative dedicated multiplayer server for Blue Engine V2.
//!
//! Manages transport-agnostic datagram communication, connection handshakes, player sessions,
//! fixed 60 Hz simulation stepping with Rapier physics, graceful disconnects,
//! timeouts, and spatial room interest replication.

use crate::math::V;
use crate::viewer::{
    net::{
        random_auth_challenge, verify_auth_proof, ConnectionNonce, DatagramTransport,
        HandshakeLimiter, Packet, SessionRegistry, SessionToken, UdpTransport, PROTOCOL_VERSION,
    },
    savestate::{SaveSlots, AUTO_SLOT},
    simulation::HeadlessWorld,
    test_lab::{SPAWN_PLAYER_1, SPAWN_PLAYER_2},
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const STALE_INPUT_WINDOW_TICKS: u64 = 6; // 100 ms at 60 Hz
pub const KEYFRAME_INTERVAL: u32 = 20; // Legacy compatibility constant; recovery is now explicit.

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
    /// Legacy history is no longer populated; use replication.baseline().
    pub snapshot_history: std::collections::VecDeque<crate::viewer::net::WorldSnapshot>,
    pub snapshots_since_keyframe: u32,
    pub keyframe_requested: bool,
    pub resync_after_tick: u64,
    pub session_token: SessionToken,
    pub authenticated: bool,
    pub replication: crate::viewer::net::ReplicationSender,
    pub game_replication: crate::viewer::net::ReplicationCounters,
    pub action_tracker: crate::viewer::net::action_counters::ActionCountersTracker,
}

/// What the server's autosave has done so far; a status line or a monitor reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutosaveStats {
    /// Saves written successfully.
    pub written: u64,
    /// Saves that failed (disk full, folder not writable, ...). The server carries on and tries again.
    pub failed: u64,
    /// The most recent failure, as a plain sentence.
    pub last_error: Option<String>,
}

struct Autosave {
    slots: SaveSlots,
    every_ticks: u64,
    keep: usize,
    last_tick: u64,
}

/// Below this many peers a broadcast is never split across threads: measured on 4 cores, splitting saves
/// under 0.3 ms there (and more total CPU) but saves milliseconds from about 64 (see `docs/perf/README.md`).
pub const PARALLEL_MIN_PEERS: usize = 32;

/// Authoritative dedicated server running [`HeadlessWorld`] over any datagram transport.
pub struct DedicatedServer<T: DatagramTransport = UdpTransport> {
    pub transport: T,
    /// First actionable replication failure. Checked runners propagate it.
    pub replication_error: Option<String>,
    replication_round: usize,
    pub world: HeadlessWorld,
    pub sessions: HashMap<u64, ClientSession>,
    pub clients: HashMap<SocketAddr, u64>,
    pub recent_disconnects: HashMap<u64, (SocketAddr, Instant)>,
    pub next_player_id: u64,
    pub client_timeout: Duration,
    pub local_addr: SocketAddr,
    pub auth_key: Option<String>,
    max_players: usize,
    network_threads: usize,
    clock: crate::viewer::net::Clock,
    pub pending_challenges: HashMap<SocketAddr, (ConnectionNonce, [u8; 16], Instant)>,
    pub handshake_limiter: HandshakeLimiter,
    pub session_registry: SessionRegistry<u64>,
    /// Outcome of [`Self::with_autosave`] so far.
    pub autosave_stats: AutosaveStats,
    autosave: Option<Autosave>,
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
            replication_error: None,
            replication_round: 0,
            world,
            sessions: HashMap::new(),
            clients: HashMap::new(),
            recent_disconnects: HashMap::new(),
            next_player_id: 1,
            client_timeout: Duration::from_secs(5),
            local_addr,
            auth_key: None,
            max_players: crate::viewer::simulation::DEFAULT_MAX_PLAYERS,
            network_threads: 1,
            clock: crate::viewer::net::Clock::real(),
            pending_challenges: HashMap::new(),
            handshake_limiter: HandshakeLimiter::new(64),
            session_registry: SessionRegistry::new(16, Duration::from_secs(5)),
            autosave_stats: AutosaveStats::default(),
            autosave: None,
        })
    }

    /// Save the world into `slots` as a rotating autosave (`auto` is the newest, then `auto-2`, ... up to `keep`
    /// files) every `seconds` of game time, and once more when [`Self::run_realtime`] ends. A failing disk is
    /// counted in [`Self::autosave_stats`] and printed; it never stops the server. Resume with
    /// `HeadlessWorld::restore_state_with(.., RestoreOptions { players: false })`, which `be2-headless --load`
    /// does.
    pub fn with_autosave(mut self, slots: SaveSlots, seconds: f32, keep: usize) -> Self {
        let every_ticks = (seconds.clamp(0.1, 86_400.) * 60.).round() as u64;
        self.autosave = Some(Autosave {
            slots,
            every_ticks,
            keep,
            last_tick: self.world.tick,
        });
        self
    }

    /// Write the autosave now (also what the schedule calls). Returns whether it was written; `false` too when
    /// no autosave is configured.
    pub fn autosave_now(&mut self) -> bool {
        let Some(autosave) = &mut self.autosave else {
            return false;
        };
        autosave.last_tick = self.world.tick;
        let label = format!("Autosave, tick {}", self.world.tick);
        let saved = self.world.save_bytes(&label).and_then(|bytes| {
            autosave
                .slots
                .save_ring_framed(AUTO_SLOT, autosave.keep, &bytes)
        });
        match saved {
            Ok(()) => {
                self.autosave_stats.written += 1;
                true
            }
            Err(error) => {
                eprintln!("[Server] Autosave failed: {error}");
                self.autosave_stats.failed += 1;
                self.autosave_stats.last_error = Some(error.to_string());
                false
            }
        }
    }

    /// How many players this server admits (default 8, at most 1024). This raises every limit that would
    /// otherwise refuse the ninth: the world's join cap, the session registry, and the handshake rate (a
    /// crowd arriving together needs more than 64 a second).
    pub fn with_max_players(mut self, max: usize) -> Self {
        self.max_players = max.clamp(1, crate::viewer::simulation::MAX_PLAYERS_LIMIT);
        self.world.set_max_players(self.max_players);
        self.session_registry.set_max_sessions(self.max_players * 2);
        self.handshake_limiter
            .set_limit(64.max(self.max_players * 2));
        self
    }
    pub fn max_players(&self) -> usize {
        self.max_players
    }

    /// Threads used to prepare each peer's world update (default 1: everything on the calling thread).
    /// `0` means one per available core. Preparing an update reads the shared world and touches only that
    /// peer's own replication state, so peers are independent: the bytes every peer receives, and the order
    /// they are sent in, are identical at any thread count. Sending stays on the calling thread, because
    /// transports are not required to be `Sync`. Fewer than [`PARALLEL_MIN_PEERS`] peers are never split:
    /// spawning threads would cost more than the work.
    pub fn with_network_threads(mut self, threads: usize) -> Self {
        self.network_threads = if threads == 0 {
            std::thread::available_parallelism().map_or(1, |n| n.get())
        } else {
            threads
        }
        .clamp(1, 64);
        self
    }
    pub fn network_threads(&self) -> usize {
        self.network_threads
    }

    /// Take "now" for timeouts, the handshake rate limit and the reconnect reservation from `clock` instead of the
    /// operating system. A test passes [`Clock::manual`](crate::viewer::net::Clock::manual), keeps a clone and calls
    /// `advance` to move time instead of sleeping; real-time pacing in [`Self::run_realtime`] is unaffected.
    pub fn with_clock(mut self, clock: crate::viewer::net::Clock) -> Self {
        self.clock = clock;
        self
    }
    pub fn clock(&self) -> &crate::viewer::net::Clock {
        &self.clock
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

    fn welcome_existing(&self, src: SocketAddr) -> bool {
        let Some(session) = self.clients.get(&src).and_then(|id| self.sessions.get(id)) else {
            return false;
        };
        let _ = self.transport.send_packet(
            &Packet::Welcome {
                player_id: session.player_id,
                server_tick: self.world.tick,
                map_name: self.world.room.name.clone(),
                session_token: Some(session.session_token),
            },
            src,
        );
        true
    }

    /// Handle client connection handshake (`Packet::Hello`).
    pub fn handle_hello(
        &mut self,
        src: SocketAddr,
        protocol_version: u32,
        req_id: u64,
        content_hash: u64,
    ) {
        let now = self.clock.now();
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
        if !self.clients.contains_key(&src) && self.sessions.len() >= self.max_players {
            let _ = self.transport.send_packet(
                &Packet::Rejected {
                    reason: format!("Server is full ({} players)", self.max_players),
                },
                src,
            );
            return;
        }

        // Hello retries resend the welcome without resetting authority or replication.
        if self.welcome_existing(src) {
            return;
        }

        // If authentication key is required, send cryptographic AuthChallenge
        if self.auth_key.is_some() {
            let (nonce, salt) = match random_auth_challenge() {
                Ok(challenge) => challenge,
                Err(detail) => {
                    eprintln!("[Server] Rejected authentication challenge for {src}: {detail}");
                    let _ = self.transport.send_packet(
                        &Packet::Rejected {
                            reason: "Server could not create secure credentials".into(),
                        },
                        src,
                    );
                    return;
                }
            };
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
                    reason: format!("World is full ({} players)", self.max_players),
                },
                src,
            );
            return;
        }

        let token = match self
            .session_registry
            .register(src, [player_id, 0], now, player_id)
        {
            Ok(token) => token,
            Err(error) => {
                self.world.leave(player_id);
                eprintln!("[Server] Could not register client from {src}: {error}");
                let _ = self.transport.send_packet(
                    &Packet::Rejected {
                        reason: format!("Could not create session: {error}"),
                    },
                    src,
                );
                return;
            }
        };

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
                snapshot_history: std::collections::VecDeque::new(),
                replication: crate::viewer::net::ReplicationSender::for_session(token),
                game_replication: Default::default(),
                snapshots_since_keyframe: 0,
                keyframe_requested: false,
                resync_after_tick: 0,
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
        let now = self.clock.now();
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

        if self.welcome_existing(src) {
            return;
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

        let token = match self.session_registry.register(src, nonce, now, assigned_id) {
            Ok(token) => token,
            Err(error) => {
                self.world.leave(assigned_id);
                eprintln!("[Server] Could not register authenticated client from {src}: {error}");
                let _ = self.transport.send_packet(
                    &Packet::Rejected {
                        reason: format!("Could not create session: {error}"),
                    },
                    src,
                );
                return;
            }
        };

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
                snapshot_history: std::collections::VecDeque::new(),
                replication: crate::viewer::net::ReplicationSender::for_session(token),
                game_replication: Default::default(),
                snapshots_since_keyframe: 0,
                keyframe_requested: false,
                resync_after_tick: 0,
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
        let _poll = crate::viewer::spans::span("server.poll");
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
                            session.last_seen = self.clock.now();
                            self.session_registry
                                .touch(&session.session_token, session.last_seen);
                            session.last_client_tick = frame.client_tick;
                            session.last_input_tick = self.world.tick;
                            if frame.session_token == Some(session.session_token)
                                && session.replication.acknowledge(frame.ack_server_tick)
                            {
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
                                if let Err(error) = self.world.game_action(player_id) {
                                    eprintln!("Game action failed: {error}");
                                }
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
                Packet::SequencedInput(mut frame) => {
                    if self
                        .world
                        .game
                        .as_ref()
                        .is_some_and(|g| frame.round != g.state().round)
                    {
                        continue;
                    }
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
                            session.last_seen = self.clock.now();
                            self.session_registry
                                .touch(&session.session_token, session.last_seen);
                            session.last_client_tick = frame.client_tick;
                            session.last_input_tick = self.world.tick;
                            if frame.session_token == Some(session.session_token)
                                && session.replication.acknowledge(frame.ack_server_tick)
                            {
                                session.last_acked_tick = frame.ack_server_tick;
                            }

                            let edges = session.action_tracker.update(&frame.counters);
                            frame.movement.jump = edges.jump;
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
                                if let Err(error) = self.world.game_action(player_id) {
                                    eprintln!("Game action failed: {error}");
                                }
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
                Packet::Resynchronize {
                    session: token,
                    after_tick,
                } => {
                    if let Some(&player_id) = self.clients.get(&src) {
                        if let Some(session) = self.sessions.get_mut(&player_id) {
                            if session.session_token == token {
                                session.keyframe_requested = true;
                                session.resync_after_tick =
                                    session.resync_after_tick.max(after_tick);
                            }
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
                            .insert(player_id, (src, self.clock.now()));
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
        self.session_registry.set_timeout(self.client_timeout);
        let timed_out = self.session_registry.evict_timeouts(self.clock.now());
        let mut ids = Vec::new();
        for entry in timed_out {
            let id = entry.data;
            let addr = entry.peer;
            self.world.leave(id);
            self.sessions.remove(&id);
            self.clients.remove(&addr);
            self.recent_disconnects.insert(id, (addr, self.clock.now()));
            println!("[Server] Client #{id} timed out (disconnected)");
            ids.push(id);
        }
        ids
    }

    /// Compatibility wrapper; failures are latched and logged once, never hidden.
    /// Checked runners return them; custom loops can use try_broadcast_snapshots.
    pub fn broadcast_snapshots(&mut self) {
        if self.replication_error.is_some() {
            return;
        }
        if let Err(error) = self.try_broadcast_snapshots() {
            eprintln!("[Server] Replication stopped: {error}");
            self.replication_error = Some(error.to_string());
        }
    }

    /// At most one world packet and one independent game-state record per peer.
    /// Queue saturation is normal backpressure. Other errors remain actionable.
    ///
    /// Three stages: bookkeeping and payload limits (sequential), preparing each peer's update (parallel, see
    /// [`Self::with_network_threads`]), then sending in the fairness-rotated order (sequential). The first
    /// error, in send order, is returned after the peers before it have been sent, as it always was.
    pub fn try_broadcast_snapshots(&mut self) -> crate::Result<()> {
        use crate::viewer::net::{SendOutcome, MAX_PACKET_BYTES};
        let mut peers: Vec<_> = self.sessions.keys().copied().collect();
        peers.sort_unstable();
        let count = peers.len().max(1);
        peers.rotate_left(self.replication_round % count);
        let game_first = (self.replication_round / count).is_multiple_of(2);
        self.replication_round = self.replication_round.wrapping_add(1);

        // Stage 1: keyframe requests and each peer's payload budget.
        let mut limits = Vec::with_capacity(peers.len());
        for id in &peers {
            let session = self.sessions.get_mut(id).unwrap();
            if session.keyframe_requested {
                session
                    .replication
                    .resynchronize_after(session.resync_after_tick);
                session.keyframe_requested = false;
                session.last_acked_tick = session.replication.baseline().map_or(0, |s| s.tick);
            }
            limits.push(self.transport.payload_limit(session.addr));
        }

        // Stage 2: prepare every peer's update. Pure per peer: reads the world, writes only its own session.
        let stage_span = crate::viewer::spans::span("server.broadcast.stage");
        let staged = {
            let world = &self.world;
            let mut by_id: std::collections::HashMap<u64, &mut ClientSession> =
                self.sessions.iter_mut().map(|(id, s)| (*id, s)).collect();
            let mut work: Vec<(u64, usize, &mut ClientSession)> = peers
                .iter()
                .zip(&limits)
                .map(|(id, limit)| (*id, *limit, by_id.remove(id).unwrap()))
                .collect();
            let stage = |(id, limit, session): &mut (u64, usize, &mut ClientSession)| {
                let _peer = crate::viewer::spans::span("replication.stage");
                let snap = world.snapshot_for_player(*id, session.last_client_tick);
                session.replication.stage(*limit, &snap, *id)
            };
            let threads = if work.len() < PARALLEL_MIN_PEERS {
                1
            } else {
                self.network_threads.min(work.len())
            };
            if threads <= 1 {
                work.iter_mut().map(stage).collect::<Vec<_>>()
            } else {
                let per_thread = work.len().div_ceil(threads);
                std::thread::scope(|scope| {
                    let handles: Vec<_> = work
                        .chunks_mut(per_thread)
                        .map(|part| {
                            scope.spawn(move || part.iter_mut().map(stage).collect::<Vec<_>>())
                        })
                        .collect();
                    handles
                        .into_iter()
                        .flat_map(|h| h.join().expect("a replication worker panicked"))
                        .collect::<Vec<_>>()
                })
            }
        };

        drop(stage_span);
        let _transmit = crate::viewer::spans::span("server.broadcast.transmit");
        // Stage 3: send, in the rotated order. World gets first access to a congested queue; the order
        // alternates by broadcast to avoid starving the independent game-state lane.
        for (id, staged) in peers.into_iter().zip(staged) {
            let session = self.sessions.get_mut(&id).unwrap();
            let game = self.world.game.as_ref().map(|g| Packet::GameState {
                session: Some(session.session_token),
                tick: self.world.tick,
                state: g.state().clone(),
            });
            let send_game = |session: &mut ClientSession| -> crate::Result<()> {
                if let Some(packet) = &game {
                    let bytes = serde_json::to_vec(packet)?;
                    let limit = self
                        .transport
                        .payload_limit(session.addr)
                        .min(MAX_PACKET_BYTES);
                    if bytes.len() > limit {
                        return Err(format!("GameState record requires {} bytes; active transport allows {limit}. Reduce counters/movers or use a transport with sufficient payload", bytes.len()).into());
                    }
                    match self.transport.try_send(session.addr, &bytes) {
                        Ok(SendOutcome::Accepted { bytes }) => {
                            session.game_replication.accepted_packets += 1;
                            session.game_replication.accepted_bytes += bytes as u64;
                        }
                        Ok(SendOutcome::Backpressured) => {
                            session.game_replication.backpressured += 1
                        }
                        Err(e) => {
                            session.game_replication.send_errors += 1;
                            return Err(e);
                        }
                    }
                }
                Ok(())
            };
            if game_first {
                send_game(session)?;
            }
            staged?;
            session
                .replication
                .transmit(&self.transport, session.addr)?;
            if !game_first {
                send_game(session)?;
            }
        }
        Ok(())
    }

    /// Step authoritative simulation one tick, neutralize stale inputs, and broadcast snapshots every 3 ticks (20 Hz).
    pub fn step(&mut self) {
        let _step = crate::viewer::spans::span("server.step");
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
        if self
            .autosave
            .as_ref()
            .is_some_and(|a| self.world.tick >= a.last_tick + a.every_ticks)
        {
            self.autosave_now();
        }
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
            if let Some(error) = &self.replication_error {
                return Err(error.clone().into());
            }
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
            if let Some(error) = &self.replication_error {
                return Err(error.clone().into());
            }

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
                let accepted: u64 = self
                    .sessions
                    .values()
                    .map(|s| {
                        s.replication.counters.accepted_bytes + s.game_replication.accepted_bytes
                    })
                    .sum();
                let blocked: u64 = self
                    .sessions
                    .values()
                    .map(|s| {
                        s.replication.counters.backpressured + s.game_replication.backpressured
                    })
                    .sum();
                let retries: u64 = self
                    .sessions
                    .values()
                    .map(|s| s.replication.counters.retries)
                    .sum();
                println!("[Server] Replication local_accepted_bytes={accepted} backpressured={blocked} retries={retries}");
                if crate::viewer::spans::enabled() {
                    // Where the last status window went, biggest first (see `viewer::spans`).
                    print!("[Profile]\n{}", crate::viewer::spans::summary().text());
                    crate::viewer::spans::reset();
                }
                runner.metrics.reset();
                last_status = Instant::now();
            }

            let elapsed = tick_start.elapsed().as_micros();
            runner.sleep_until_next_tick(elapsed);
        }

        if self.autosave_now() {
            println!("[Server] Final save written at tick {}", self.world.tick);
        }
        println!(
            "[Server] Shutdown complete after {} ticks ({:.2}s)",
            self.world.tick,
            started.elapsed().as_secs_f64()
        );
        Ok(())
    }
}
