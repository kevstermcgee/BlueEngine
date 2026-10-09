//! The authoritative server: a lobby, then a match, then results, forever, for any [`NetGame`].
//!
//! Generic over the game and over the engine's [`DatagramTransport`], so the same code runs on UDP, on QUIC/TLS
//! and on the in-memory [`LoopNet`](crate::viewer::net::loopback::LoopNet) that the tests use.
use super::failure::{full_reason, REASON_KEY, REASON_MATCH, REASON_VERSION};
use super::wire::{
    decode_client, ClientMsg, LobbyEntry, LobbyState, ServerMsg, SnapshotMsg, Token, MAX_DATAGRAM,
};
use super::{NetGame, Seat};
use crate::viewer::devkit::Rng;
use crate::viewer::net::{
    constant_time_eq, random_token, DatagramTransport, HandshakeLimiter, SendOutcome,
    SessionRegistry,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Ticks of events the server remembers for clients that have not acknowledged them yet.
const EVENT_MEMORY: u64 = 180;
/// Inputs a client may run ahead of the server before the oldest are dropped.
const MAX_BUFFERED_INPUTS: usize = 12;
/// Buffered inputs after which a missing sequence number is declared lost and skipped.
const GAP_TOLERANCE: usize = 3;

#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// Required in every Hello when set. Sent inside the encrypted channel on the production transport.
    pub join_key: Option<String>,
    /// Participants per match, players plus the game's own AI (at least the number of players).
    pub participants: usize,
    /// Start the countdown this long after the first player joins even if not everyone is ready; 0 = never.
    pub auto_start_seconds: u32,
    pub countdown_seconds: u32,
    pub results_seconds: u32,
    /// Where `matches.jsonl` goes; `None` keeps the server from writing files.
    pub report_dir: Option<PathBuf>,
    pub session_timeout: Duration,
    /// Fixed seed for reproducible matches (tests); `None` draws a fresh one from the OS.
    pub seed: Option<u64>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            join_key: None,
            participants: 8,
            auto_start_seconds: 45,
            countdown_seconds: 5,
            results_seconds: 12,
            report_dir: None,
            session_timeout: Duration::from_secs(6),
            seed: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Lobby,
    Match,
    Results,
}

/// What the network did for one player during one match.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PeerStats {
    pub inputs_received: u64,
    /// Inputs that arrived after the server had already moved past them.
    pub inputs_late: u64,
    /// Ticks the server had no fresh input and repeated the last one.
    pub ticks_repeated: u64,
    /// Sequence numbers declared lost and skipped.
    pub inputs_skipped: u64,
    pub packets_in: u64,
    pub packets_out: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub rtt_samples: u64,
    pub rtt_sum_ms: u64,
    pub rtt_max_ms: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PeerReport {
    pub name: String,
    pub participant: usize,
    pub choice: u8,
    pub rtt_ms_mean: f32,
    pub rtt_ms_max: u16,
    pub left_early: bool,
    #[serde(flatten)]
    pub stats: PeerStats,
}

/// The server's own load during a match.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ServerLoad {
    pub ticks: u64,
    pub tick_us_mean: f32,
    pub tick_us_max: u32,
    pub snapshots_sent: u64,
    pub snapshot_bytes_mean: f32,
    pub snapshot_bytes_max: usize,
    pub bad_datagrams: u64,
}

/// Local transport submissions, reset when a match starts. Acceptance is not peer delivery.
/// Archived as an additional `send` object without changing the existing match-report types.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerSendStats {
    pub attempts: u64,
    pub accepted: u64,
    pub accepted_bytes: u64,
    pub backpressured: u64,
    pub errors: u64,
    pub oversized: u64,
    pub snapshots_accepted: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NetReport {
    pub peers: Vec<PeerReport>,
    pub server: ServerLoad,
}

/// One line of `matches.jsonl`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatchLog {
    pub game: String,
    pub unix_seconds: u64,
    pub match_index: u32,
    /// The game's own summary (`NetGame::report`).
    pub report: serde_json::Value,
    pub net: NetReport,
}

/// Reliable event stream counters. Retained cache is bounded; receipt is measured by client gaps and ACKs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerEventStats {
    pub emitted: u64,
    pub retained: usize,
    pub retained_bytes: usize,
    pub oversized: u64,
}

struct Player {
    id: u8,
    name: String,
    choice: u8,
    ready: bool,
    /// The participant this player drives in the current match.
    participant: Option<usize>,
    next_seq: u32,
    started: bool,
    applied_seq: u32,
    acked_tick: u32,
    event_ack: u64,
    event_cursor: u64,
    left_early: bool,
    stats: PeerStats,
}

/// Counters that reset every match.
#[derive(Default)]
struct LoadCounters {
    send: ServerSendStats,
    ticks: u64,
    tick_us_sum: u64,
    tick_us_max: u32,
    snapshots: u64,
    snapshot_bytes: u64,
    snapshot_bytes_max: usize,
    bad_datagrams: u64,
}

pub struct NetServer<G: NetGame, T: DatagramTransport> {
    transport: T,
    sessions: SessionRegistry<Player>,
    /// Buffered inputs per player token (kept beside the registry so `Player` stays game-agnostic).
    inputs: std::collections::HashMap<Token, InputQueue<G::Input>>,
    limiter: HandshakeLimiter,
    cfg: ServerConfig,
    stage: Stage,
    current: Option<G::Match>,
    tick: u64,
    stage_since: u64,
    countdown: Option<u64>,
    first_join_tick: Option<u64>,
    lobby_dirty: bool,
    events: VecDeque<(u32, G::Event)>,
    match_index: u32,
    load: LoadCounters,
    departed: Vec<PeerReport>,
    rng: Rng,
    log: Vec<MatchLog>,
    match_history_limit: Option<usize>,
    fingerprint: u32,
    capacity: usize,
    minimum_players: usize,
    event_sender: super::event_channel::Sender,
}

struct InputQueue<I> {
    buffer: BTreeMap<u32, I>,
    last: I,
}

impl<G: NetGame, T: DatagramTransport> NetServer<G, T> {
    pub fn new(transport: T, cfg: ServerConfig) -> crate::Result<Self> {
        if cfg
            .join_key
            .as_ref()
            .is_some_and(|key| key.len() > super::wire::MAX_TEXT)
        {
            return Err("Admission key exceeds the netplay wire limit of 32 bytes".into());
        }
        assert!(
            G::MAX_SEATS >= 1 && G::MAX_SEATS <= 16,
            "MAX_SEATS must be 1..=16"
        );
        let seed = match cfg.seed {
            Some(s) => s,
            None => random_token()?[0],
        };
        let fingerprint = hello_fingerprint::<G>();
        let capacity = G::lobby_capacity().clamp(1, G::MAX_SEATS);
        Ok(Self {
            transport,
            sessions: SessionRegistry::new(capacity, cfg.session_timeout),
            inputs: Default::default(),
            limiter: HandshakeLimiter::new(32),
            cfg,
            stage: Stage::Lobby,
            current: None,
            tick: 0,
            stage_since: 0,
            countdown: None,
            first_join_tick: None,
            lobby_dirty: false,
            events: VecDeque::new(),
            match_index: 0,
            load: LoadCounters::default(),
            departed: Vec::new(),
            rng: Rng::new(seed),
            log: Vec::new(),
            match_history_limit: None,
            fingerprint,
            capacity,
            minimum_players: G::minimum_players().clamp(1, capacity),
            event_sender: Default::default(),
        })
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    /// Connected players.
    pub fn players(&self) -> usize {
        self.sessions.count()
    }
    /// The running (or just finished) match.
    pub fn current(&self) -> Option<&G::Match> {
        self.current.as_ref()
    }
    /// Completed matches retained in memory (also appended to `matches.jsonl` when configured).
    /// The library retains all matches unless [`Self::set_match_history_limit`] is called.
    pub fn match_log(&self) -> &[MatchLog] {
        &self.log
    }
    /// Keep the most recent `limit` reports in memory; `None` retains all and `Some(0)` retains none.
    /// This never trims the on-disk archive or resets match numbering.
    pub fn set_match_history_limit(&mut self, limit: Option<usize>) {
        self.match_history_limit = limit;
        self.trim_match_history();
    }

    fn trim_match_history(&mut self) {
        if let Some(limit) = self.match_history_limit {
            let excess = self.log.len().saturating_sub(limit);
            self.log.drain(..excess);
        }
    }
    pub fn local_addr(&self) -> crate::Result<SocketAddr> {
        self.transport.local_addr()
    }
    pub fn fingerprint(&self) -> u32 {
        self.fingerprint
    }
    /// Local send outcomes since the current match began (or process start before its first match).
    pub fn send_stats(&self) -> ServerSendStats {
        self.load.send
    }

    /// Bounded cache and encoding failures for opt-in reliable events.
    pub fn event_stats(&self) -> ServerEventStats {
        self.event_sender.stats()
    }

    fn send(&mut self, peer: SocketAddr, msg: &ServerMsg<G::Snapshot, G::Event>) {
        let bytes = self.message_bytes(peer, msg);
        self.send_encoded(peer, &bytes);
        if matches!(msg, ServerMsg::Snapshot(_)) {
            self.send_events(peer);
        }
    }

    fn message_bytes(&self, peer: SocketAddr, msg: &ServerMsg<G::Snapshot, G::Event>) -> Vec<u8> {
        let limit = self.transport.payload_limit(peer).min(MAX_DATAGRAM);
        if G::RELIABLE_EVENTS && matches!(msg, ServerMsg::Snapshot(_) | ServerMsg::Lobby(_)) {
            if let Some(session) = self.sessions.iter().find(|s| s.peer == peer) {
                let body = super::wire::encode_server_with_limit::<G>(
                    msg,
                    limit.saturating_sub(super::event_channel::STATE_HEADER),
                );
                return if matches!(msg, ServerMsg::Lobby(_)) {
                    super::event_channel::lobby(session.token, self.match_index as u64, &body)
                } else {
                    super::event_channel::state(session.token, self.match_index as u64, &body)
                };
            }
        }
        super::wire::encode_server_with_limit::<G>(msg, limit)
    }

    fn send_events(&mut self, peer: SocketAddr) {
        if !G::RELIABLE_EVENTS {
            return;
        }
        let Some(session) = self.sessions.get_by_peer_mut(&peer) else {
            return;
        };
        let packets = self.event_sender.packets(
            session.token,
            self.match_index as u64,
            session.data.event_ack,
            self.transport.payload_limit(peer),
            &mut session.data.event_cursor,
        );
        for packet in packets {
            self.send_encoded(peer, &packet);
        }
    }

    fn send_encoded(&mut self, peer: SocketAddr, bytes: &[u8]) -> bool {
        self.load.send.attempts += 1;
        if bytes.len() > MAX_DATAGRAM.min(self.transport.payload_limit(peer)) {
            self.load.send.oversized += 1;
            eprintln!(
                "[Server] A {}-byte message does not fit a datagram; not sent",
                bytes.len()
            );
            return false;
        }
        if let Some(entry) = self.sessions.get_by_peer_mut(&peer) {
            entry.data.stats.packets_out += 1;
            entry.data.stats.bytes_out += bytes.len() as u64;
        }
        // Backpressure just drops this datagram: the next snapshot supersedes it.
        match self.transport.try_send(peer, bytes) {
            Ok(SendOutcome::Accepted { bytes: n }) if n == bytes.len() => {
                self.load.send.accepted += 1;
                self.load.send.accepted_bytes += n as u64;
                true
            }
            Ok(SendOutcome::Backpressured) => {
                self.load.send.backpressured += 1;
                false
            }
            Ok(SendOutcome::Accepted { .. }) | Err(_) => {
                self.load.send.errors += 1;
                false
            }
        }
    }

    fn reject(&mut self, peer: SocketAddr, reason: &str) {
        self.send(
            peer,
            &ServerMsg::Rejected {
                reason: reason.into(),
            },
        );
    }

    fn lobby_state(&self) -> LobbyState {
        let mut entries: Vec<LobbyEntry> = self
            .sessions
            .iter()
            .map(|s| LobbyEntry {
                slot: s.data.id,
                choice: s.data.choice,
                ready: s.data.ready,
                name: s.data.name.clone(),
            })
            .collect();
        entries.sort_by_key(|e| e.slot);
        LobbyState {
            stage: match self.stage {
                Stage::Lobby => 0,
                Stage::Match => 1,
                Stage::Results => 2,
            },
            seconds_left: self.countdown.map_or(0, |t| t.div_ceil(G::TICK_HZ) as u8),
            participants: self.cfg.participants.min(self.capacity).clamp(1, 255) as u8,
            entries,
        }
    }

    fn broadcast_lobby(&mut self) {
        let msg = ServerMsg::Lobby(self.lobby_state());
        let peers: Vec<SocketAddr> = self.sessions.iter().map(|s| s.peer).collect();
        for peer in peers {
            self.send(peer, &msg);
        }
        self.lobby_dirty = false;
    }

    /// Read whatever has arrived and act on it.
    pub fn poll(&mut self, now: Instant) {
        let Ok(datagrams) = self.transport.receive() else {
            return;
        };
        for d in datagrams {
            if G::RELIABLE_EVENTS && super::event_channel::recognizes(&d.data) {
                match super::event_channel::decode(&d.data) {
                    Ok(super::event_channel::Frame::Ack {
                        token,
                        epoch,
                        sequence,
                    }) if epoch == self.match_index as u64
                        && sequence <= self.event_sender.last() =>
                    {
                        if let Some(p) = self.owned(d.peer, &token) {
                            p.event_ack = p.event_ack.max(sequence);
                            p.stats.packets_in += 1;
                            p.stats.bytes_in += d.data.len() as u64;
                            self.sessions.touch(&token, now);
                        }
                    }
                    _ => self.load.bad_datagrams += 1,
                }
                continue;
            }
            match decode_client::<G>(&d.data) {
                Ok(msg) => {
                    if let Some(entry) = self.sessions.get_by_peer_mut(&d.peer) {
                        entry.data.stats.packets_in += 1;
                        entry.data.stats.bytes_in += d.data.len() as u64;
                    }
                    self.on_message(d.peer, msg, now);
                }
                Err(_) => self.load.bad_datagrams += 1,
            }
        }
    }

    fn free_choice(&self, wanted: u8) -> Option<u8> {
        let taken: Vec<u8> = self.sessions.iter().map(|s| s.data.choice).collect();
        let free = |c: u8| c < G::CHOICES && (!G::UNIQUE_CHOICES || !taken.contains(&c));
        if free(wanted) {
            return Some(wanted);
        }
        (0..G::CHOICES).find(|c| free(*c))
    }

    /// The session behind `token`, only if it really belongs to `peer` (a token alone is not enough).
    fn owned(&mut self, peer: SocketAddr, token: &Token) -> Option<&mut Player> {
        let entry = self.sessions.get_mut(token)?;
        (entry.peer == peer).then_some(&mut entry.data)
    }

    fn on_message(&mut self, peer: SocketAddr, msg: ClientMsg<G::Input>, now: Instant) {
        match msg {
            ClientMsg::Hello {
                key,
                name,
                choice,
                nonce,
                fingerprint,
            } => {
                if !self.limiter.allow(now) {
                    return;
                }
                if fingerprint != self.fingerprint {
                    return self.reject(peer, REASON_VERSION);
                }
                if let Some(expected) = &self.cfg.join_key {
                    if !constant_time_eq(expected.as_bytes(), key.as_bytes()) {
                        return self.reject(peer, REASON_KEY);
                    }
                }
                // A repeated Hello from a connected peer (a lost Welcome) gets the same answer again.
                let existing = self
                    .sessions
                    .get_by_peer(&peer)
                    .filter(|e| e.nonce == nonce)
                    .map(|e| (e.token, e.data.id));
                if let Some((token, id)) = existing {
                    self.send(
                        peer,
                        &ServerMsg::Welcome {
                            token,
                            slot: id,
                            fingerprint: self.fingerprint,
                        },
                    );
                    self.lobby_dirty = true;
                    return;
                }
                if self.stage == Stage::Match {
                    return self.reject(peer, REASON_MATCH);
                }
                let full = full_reason(self.capacity);
                if self.sessions.get_by_peer(&peer).is_none() && self.sessions.is_full() {
                    return self.reject(peer, &full);
                }
                let Some(choice) = self.free_choice(choice) else {
                    return self.reject(peer, &full);
                };
                let used: Vec<u8> = self
                    .sessions
                    .iter()
                    .filter(|s| s.peer != peer)
                    .map(|s| s.data.id)
                    .collect();
                let id = (0..self.capacity as u8)
                    .find(|i| !used.contains(i))
                    .unwrap_or(0);
                let player = Player {
                    id,
                    name: sanitize(&name, id),
                    choice,
                    ready: false,
                    participant: None,
                    next_seq: 0,
                    started: false,
                    applied_seq: 0,
                    acked_tick: 0,
                    event_ack: 0,
                    event_cursor: 0,
                    left_early: false,
                    stats: PeerStats::default(),
                };
                match self.sessions.register(peer, nonce, now, player) {
                    Ok(token) => {
                        self.inputs.insert(
                            token,
                            InputQueue {
                                buffer: BTreeMap::new(),
                                last: G::Input::default(),
                            },
                        );
                        self.first_join_tick.get_or_insert(self.tick);
                        self.send(
                            peer,
                            &ServerMsg::Welcome {
                                token,
                                slot: id,
                                fingerprint: self.fingerprint,
                            },
                        );
                        self.broadcast_lobby();
                    }
                    Err(e) => self.reject(peer, &e.to_string()),
                }
            }
            ClientMsg::Select { token, choice } => {
                if self.stage != Stage::Lobby {
                    return;
                }
                let taken = G::UNIQUE_CHOICES
                    && self
                        .sessions
                        .iter()
                        .any(|s| s.data.choice == choice && s.token != token);
                if choice >= G::CHOICES || taken {
                    self.lobby_dirty = true; // tell the client what it really has
                    return;
                }
                let changed = self
                    .owned(peer, &token)
                    .map(|p| {
                        p.choice = choice;
                        p.ready = false;
                    })
                    .is_some();
                self.lobby_dirty |= changed;
            }
            ClientMsg::Ready { token, ready } => {
                if self.stage != Stage::Lobby {
                    return;
                }
                let changed = self.owned(peer, &token).map(|p| p.ready = ready).is_some();
                self.lobby_dirty |= changed;
            }
            ClientMsg::Input {
                token,
                frames,
                ack_tick,
            } => {
                if self.owned(peer, &token).is_none() {
                    return;
                }
                self.sessions.touch(&token, now);
                let Some(entry) = self.sessions.get_mut(&token) else {
                    return;
                };
                let p = &mut entry.data;
                let Some(queue) = self.inputs.get_mut(&token) else {
                    return;
                };
                p.acked_tick = p.acked_tick.max(ack_tick);
                for (seq, input) in frames {
                    if !p.started {
                        p.next_seq = seq;
                        p.started = true;
                    }
                    if seq < p.next_seq || queue.buffer.contains_key(&seq) {
                        if seq < p.next_seq {
                            p.stats.inputs_late += 1;
                        }
                        continue;
                    }
                    p.stats.inputs_received += 1;
                    queue.buffer.insert(seq, input);
                }
                while queue.buffer.len() > MAX_BUFFERED_INPUTS {
                    if let Some((&oldest, _)) = queue.buffer.iter().next() {
                        queue.buffer.remove(&oldest);
                        p.next_seq = p.next_seq.max(oldest + 1);
                        p.stats.inputs_skipped += 1;
                    }
                }
            }
            ClientMsg::Ping {
                token,
                stamp,
                rtt_ms,
            } => {
                if self.owned(peer, &token).is_none() {
                    return;
                }
                self.sessions.touch(&token, now);
                if let Some(entry) = self.sessions.get_mut(&token) {
                    let s = &mut entry.data.stats;
                    if rtt_ms > 0 {
                        s.rtt_samples += 1;
                        s.rtt_sum_ms += rtt_ms as u64;
                        s.rtt_max_ms = s.rtt_max_ms.max(rtt_ms);
                    }
                }
                self.send(peer, &ServerMsg::Pong { stamp });
            }
            ClientMsg::Leave { token } => {
                if self.owned(peer, &token).is_some() {
                    if let Some(entry) = self.sessions.remove(&token) {
                        self.inputs.remove(&entry.token);
                        self.depart(entry.data);
                    }
                }
            }
        }
    }

    /// A player is gone. Mid-match their participant is handed to the game's own AI.
    fn depart(&mut self, mut player: Player) {
        if self.stage == Stage::Match {
            if let (Some(m), Some(participant)) = (self.current.as_mut(), player.participant) {
                G::release(m, participant);
                player.left_early = true;
                self.departed.push(peer_report(&player));
            }
        }
        self.lobby_dirty = true;
        if self.sessions.count() == 0 {
            self.first_join_tick = None;
            self.countdown = None;
        }
    }

    /// Advance the server exactly one tick.
    pub fn step(&mut self, now: Instant) {
        let started = Instant::now();
        self.tick += 1;
        for entry in self.sessions.evict_timeouts(now) {
            self.inputs.remove(&entry.token);
            self.depart(entry.data);
        }
        match self.stage {
            Stage::Lobby => self.step_lobby(),
            Stage::Match => self.step_match(),
            Stage::Results => self.step_results(),
        }
        if self.lobby_dirty || (self.stage == Stage::Lobby && self.tick.is_multiple_of(G::TICK_HZ))
        {
            self.broadcast_lobby();
        }
        if self.stage == Stage::Match {
            let elapsed = started.elapsed().as_micros() as u64;
            self.load.ticks += 1;
            self.load.tick_us_sum += elapsed;
            self.load.tick_us_max = self.load.tick_us_max.max(elapsed as u32);
        }
    }

    fn step_lobby(&mut self) {
        let humans = self.sessions.count();
        let all_ready =
            humans >= self.minimum_players && self.sessions.iter().all(|s| s.data.ready);
        let waited_too_long = self.cfg.auto_start_seconds > 0
            && humans >= self.minimum_players
            && self
                .first_join_tick
                .is_some_and(|t| self.tick - t >= self.cfg.auto_start_seconds as u64 * G::TICK_HZ);
        match (self.countdown, all_ready || waited_too_long) {
            (None, true) => {
                self.countdown = Some(self.cfg.countdown_seconds as u64 * G::TICK_HZ);
                self.lobby_dirty = true;
            }
            (Some(_), false) => {
                // Somebody un-readied or left: stop the clock.
                self.countdown = None;
                self.lobby_dirty = true;
            }
            (Some(left), true) => {
                let left = left.saturating_sub(1);
                if left == 0 {
                    self.countdown = None;
                    self.start_match();
                } else {
                    self.countdown = Some(left);
                    self.lobby_dirty |= left.is_multiple_of(G::TICK_HZ);
                }
            }
            (None, false) => {}
        }
    }

    fn start_match(&mut self) {
        self.match_index += 1;
        let seed = self.rng.next_u64();
        let mut seats: Vec<Seat> = self
            .sessions
            .iter()
            .map(|s| Seat {
                id: s.data.id,
                choice: s.data.choice,
                name: s.data.name.clone(),
            })
            .collect();
        seats.sort_by_key(|s| s.id);
        let participants = self.cfg.participants.min(self.capacity).max(seats.len());
        let (m, assigned) = G::start(seed, &seats, participants);
        for (seat, participant) in seats.iter().zip(&assigned) {
            let token = self
                .sessions
                .iter()
                .find(|s| s.data.id == seat.id)
                .map(|s| s.token);
            if let Some(token) = token {
                if let Some(e) = self.sessions.get_mut(&token) {
                    e.data.participant = Some(*participant);
                    e.data.started = false;
                    e.data.applied_seq = 0;
                    e.data.acked_tick = 0;
                    e.data.event_ack = 0;
                    e.data.event_cursor = 0;
                    e.data.left_early = false;
                    e.data.stats = PeerStats::default();
                }
                if let Some(q) = self.inputs.get_mut(&token) {
                    q.buffer.clear();
                    q.last = G::Input::default();
                }
            }
        }
        self.current = Some(m);
        self.events.clear();
        self.event_sender.reset();
        // Junk received while waiting in the lobby is still worth reporting: it carries into this match's line.
        self.load = LoadCounters {
            bad_datagrams: self.load.bad_datagrams,
            ..LoadCounters::default()
        };
        self.departed.clear();
        self.stage = Stage::Match;
        self.stage_since = self.tick;
        self.countdown = None;
        self.lobby_dirty = true;
    }

    fn step_match(&mut self) {
        let tick32 = self.tick as u32;
        let Some(m) = self.current.as_mut() else {
            return;
        };
        let mut inputs: Vec<Option<G::Input>> = vec![None; G::participants(m)];
        for token in self.sessions.iter().map(|s| s.token).collect::<Vec<_>>() {
            let Some(e) = self.sessions.get_mut(&token) else {
                continue;
            };
            let p = &mut e.data;
            let (Some(participant), Some(queue)) = (p.participant, self.inputs.get_mut(&token))
            else {
                continue;
            };
            let input = next_input::<G::Input>(p, queue);
            if participant < inputs.len() {
                inputs[participant] = Some(input);
            }
        }
        for e in G::step(m, &inputs) {
            if G::RELIABLE_EVENTS {
                self.event_sender.push::<G>(tick32, &e);
            } else {
                self.events.push_back((tick32, e));
            }
        }
        while self
            .events
            .front()
            .is_some_and(|(t, _)| (tick32 - t) as u64 > EVENT_MEMORY)
        {
            self.events.pop_front();
        }
        let over = self.current.as_ref().is_some_and(G::is_over);
        if self.tick.is_multiple_of(G::SNAPSHOT_EVERY) || over {
            self.send_snapshots();
        }
        if over {
            self.finish_match();
        }
    }

    fn snapshot_msg(
        &self,
        applied_seq: u32,
        acked_tick: u32,
        participant: Option<usize>,
        budget: usize,
    ) -> ServerMsg<G::Snapshot, G::Event> {
        let m = self
            .current
            .as_ref()
            .expect("snapshots are only built during a match");
        ServerMsg::Snapshot(SnapshotMsg {
            server_tick: self.tick as u32,
            applied_seq,
            you: participant.map_or(255, |p| p.min(254) as u8),
            snapshot: G::snapshot_with_budget(m, participant, budget),
            events: self
                .events
                .iter()
                .filter(|(t, _)| *t > acked_tick)
                .cloned()
                .collect(),
        })
    }

    fn send_snapshots(&mut self) {
        let targets: Vec<(SocketAddr, u32, u32, Option<usize>)> = self
            .sessions
            .iter()
            .map(|s| {
                (
                    s.peer,
                    s.data.applied_seq,
                    s.data.acked_tick,
                    s.data.participant,
                )
            })
            .collect();
        for (peer, applied, acked, participant) in targets {
            let msg = self.snapshot_msg(
                applied,
                acked,
                participant,
                self.transport
                    .payload_limit(peer)
                    .min(MAX_DATAGRAM)
                    .saturating_sub(
                        16 + if G::RELIABLE_EVENTS {
                            super::event_channel::STATE_HEADER
                        } else {
                            0
                        },
                    ),
            );
            let bytes = self.message_bytes(peer, &msg);
            let len = bytes.len();
            self.load.snapshots += 1;
            self.load.snapshot_bytes += len as u64;
            self.load.snapshot_bytes_max = self.load.snapshot_bytes_max.max(len);
            if self.send_encoded(peer, &bytes) {
                self.load.send.snapshots_accepted += 1;
            }
            self.send_events(peer);
        }
    }

    fn finish_match(&mut self) {
        let Some(m) = self.current.as_ref() else {
            return;
        };
        let mut peers: Vec<PeerReport> = self.departed.clone();
        peers.extend(
            self.sessions
                .iter()
                .filter(|s| s.data.participant.is_some())
                .map(|s| peer_report(&s.data)),
        );
        peers.sort_by_key(|p| p.participant);
        let ticks = self.load.ticks.max(1);
        let entry = MatchLog {
            game: G::NAME.to_string(),
            unix_seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            match_index: self.match_index,
            report: G::report(m),
            net: NetReport {
                peers,
                server: ServerLoad {
                    ticks: self.load.ticks,
                    tick_us_mean: self.load.tick_us_sum as f32 / ticks as f32,
                    tick_us_max: self.load.tick_us_max,
                    snapshots_sent: self.load.snapshots,
                    snapshot_bytes_mean: self.load.snapshot_bytes as f32
                        / self.load.snapshots.max(1) as f32,
                    snapshot_bytes_max: self.load.snapshot_bytes_max,
                    bad_datagrams: self.load.bad_datagrams,
                },
            },
        };
        if let Some(dir) = &self.cfg.report_dir {
            if let Err(e) = append_log(
                dir,
                &entry,
                self.load.send,
                G::RELIABLE_EVENTS.then(|| self.event_stats()),
            ) {
                eprintln!("[Server] Could not write the match report: {e}");
            }
        }
        super::say(format_args!(
            "[Server] Match {} finished: {} players, server tick mean {:.0} us max {} us",
            entry.match_index,
            entry.net.peers.len(),
            entry.net.server.tick_us_mean,
            entry.net.server.tick_us_max
        ));
        self.log.push(entry);
        self.trim_match_history();
        self.load.bad_datagrams = 0;
        self.stage = Stage::Results;
        self.stage_since = self.tick;
        self.lobby_dirty = true;
    }

    fn step_results(&mut self) {
        // Finish flushing combat events at the regular snapshot cadence. Slowing this to
        // 10 Hz can strand the tail of a busy match before a short results screen closes.
        let every = if G::RELIABLE_EVENTS {
            G::SNAPSHOT_EVERY.max(1)
        } else {
            (G::TICK_HZ / 10).max(1)
        };
        if self.tick.is_multiple_of(every) {
            let targets: Vec<(SocketAddr, u32, u32, Option<usize>)> = self
                .sessions
                .iter()
                .map(|s| {
                    (
                        s.peer,
                        s.data.applied_seq,
                        s.data.acked_tick,
                        s.data.participant,
                    )
                })
                .collect();
            for (peer, applied, acked, participant) in targets {
                let msg = self.snapshot_msg(
                    applied,
                    acked,
                    participant,
                    self.transport
                        .payload_limit(peer)
                        .min(MAX_DATAGRAM)
                        .saturating_sub(
                            16 + if G::RELIABLE_EVENTS {
                                super::event_channel::STATE_HEADER
                            } else {
                                0
                            },
                        ),
                );
                self.send(peer, &msg);
            }
        }
        if self.tick - self.stage_since >= self.cfg.results_seconds as u64 * G::TICK_HZ {
            self.current = None;
            self.stage = Stage::Lobby;
            self.first_join_tick = (self.sessions.count() > 0).then_some(self.tick);
            for token in self.sessions.iter().map(|s| s.token).collect::<Vec<_>>() {
                if let Some(e) = self.sessions.get_mut(&token) {
                    e.data.ready = false;
                    e.data.participant = None;
                }
            }
            self.lobby_dirty = true;
        }
    }

    /// Run in real time until `stop` is set or `max_ticks` have passed. Prints a status line every 5 s.
    pub fn run_realtime(
        &mut self,
        stop: Arc<AtomicBool>,
        max_ticks: Option<u64>,
    ) -> crate::Result<()> {
        let mut seconds = 0u32;
        self.run_realtime_with(stop, max_ticks, |s| {
            seconds += 1;
            if seconds.is_multiple_of(5) {
                super::say(format_args!(
                    "[Server] tick {} | stage {:?} | players {} | matches {}",
                    s.tick, s.stage, s.players, s.matches
                ));
            }
        })
    }

    /// [`NetServer::run_realtime`] with a hook: `on_second` is called about once a second (and once right at the
    /// start) with what the server looks like. [`cli::serve`](super::cli::serve) uses it to print the
    /// machine-readable `STATUS` line a hub reads; the human status line of `run_realtime` is not printed here.
    pub fn run_realtime_with(
        &mut self,
        stop: Arc<AtomicBool>,
        max_ticks: Option<u64>,
        mut on_second: impl FnMut(&StatusSnapshot),
    ) -> crate::Result<()> {
        let frame = Duration::from_micros(1_000_000 / G::TICK_HZ);
        let mut next = Instant::now();
        let mut last_status: Option<Instant> = None;
        super::say(format_args!(
            "[Server] {} listening on {} (fingerprint {:08x})",
            G::NAME,
            self.local_addr()?,
            self.fingerprint
        ));
        while !stop.load(Ordering::Relaxed) {
            let now = Instant::now();
            self.poll(now);
            self.step(now);
            if max_ticks.is_some_and(|m| self.tick >= m) {
                break;
            }
            if last_status.is_none_or(|t| t.elapsed() >= Duration::from_secs(1)) {
                last_status = Some(Instant::now());
                on_second(&self.status_snapshot());
            }
            next += frame;
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else if now - next > frame * 30 {
                next = now; // fell far behind (a suspended machine): do not try to catch up
            }
        }
        Ok(())
    }

    /// What the server looks like right now.
    pub fn status_snapshot(&self) -> StatusSnapshot {
        StatusSnapshot {
            tick: self.tick,
            stage: self.stage,
            players: self.sessions.count(),
            participants: self.cfg.participants.min(self.capacity),
            matches: self.match_index,
        }
    }
}

/// The server's state at one moment, handed to the hook of [`NetServer::run_realtime_with`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusSnapshot {
    pub tick: u64,
    pub stage: Stage,
    /// Connected players.
    pub players: usize,
    /// Participants per match (`ServerConfig::participants`): the room's capacity as the lobby shows it.
    pub participants: usize,
    /// Matches finished since the server started.
    pub matches: u32,
}

/// The input for this tick: the next in sequence, the next after a lost gap, or the last held.
fn next_input<I: Copy + Default>(p: &mut Player, q: &mut InputQueue<I>) -> I {
    if let Some(input) = q.buffer.remove(&p.next_seq) {
        p.applied_seq = p.next_seq;
        p.next_seq += 1;
        q.last = input;
        return input;
    }
    if let Some((&first, _)) = q.buffer.iter().next() {
        if q.buffer.len() >= GAP_TOLERANCE {
            // The missing input is not coming: skip to what we have.
            p.stats.inputs_skipped += (first - p.next_seq) as u64;
            let input = q.buffer.remove(&first).unwrap_or_default();
            p.applied_seq = first;
            p.next_seq = first + 1;
            q.last = input;
            return input;
        }
    }
    // Nothing fresh: keep doing what they were doing.
    if p.started {
        p.stats.ticks_repeated += 1;
    }
    q.last
}

fn peer_report(p: &Player) -> PeerReport {
    PeerReport {
        name: p.name.clone(),
        participant: p.participant.unwrap_or(0),
        choice: p.choice,
        rtt_ms_mean: if p.stats.rtt_samples > 0 {
            p.stats.rtt_sum_ms as f32 / p.stats.rtt_samples as f32
        } else {
            0.
        },
        rtt_ms_max: p.stats.rtt_max_ms,
        left_early: p.left_early,
        stats: p.stats.clone(),
    }
}

fn sanitize(name: &str, id: u8) -> String {
    let clean: String = name.chars().filter(|c| !c.is_control()).take(16).collect();
    let clean = clean.trim().to_string();
    if clean.is_empty() {
        format!("Player {}", id + 1)
    } else {
        clean
    }
}

/// The value a client must send in `Hello` (and a server compares): the game's fingerprint folded with a hash
/// of its name, so two different games never accept each other's players even on equal fingerprints.
pub fn hello_fingerprint<G: NetGame>() -> u32 {
    G::fingerprint()
        ^ name_hash(G::NAME)
        ^ if G::RELIABLE_EVENTS {
            name_hash(super::event_channel::PROTOCOL)
        } else {
            0
        }
}

fn name_hash(name: &str) -> u32 {
    name.bytes().fold(0x811c_9dc5u32, |h, b| {
        (h ^ b as u32).wrapping_mul(0x0100_0193)
    })
}

fn append_log(
    dir: &std::path::Path,
    entry: &MatchLog,
    send: ServerSendStats,
    reliable_events: Option<ServerEventStats>,
) -> std::io::Result<()> {
    #[derive(Serialize)]
    struct ArchivedMatch<'a> {
        #[serde(flatten)]
        entry: &'a MatchLog,
        send: ServerSendStats,
        #[serde(skip_serializing_if = "Option::is_none")]
        reliable_events: Option<ServerEventStats>,
    }
    std::fs::create_dir_all(dir)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("matches.jsonl"))?;
    let line = serde_json::to_string(&ArchivedMatch {
        entry,
        send,
        reliable_events,
    })
    .map_err(std::io::Error::other)?;
    writeln!(file, "{line}")
}
