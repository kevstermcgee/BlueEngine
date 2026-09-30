//! The network client: connect, choose, play, for any [`NetGame`].
//!
//! It keeps the plumbing a game should not have to write: a Hello handshake that retries, lobby choices that
//! are re-sent until the server's lobby agrees (datagrams get lost), a stream of inputs bundled with the last
//! few so a lost datagram costs nothing, acknowledgement of the newest server tick and of events (each is
//! delivered once), round-trip measurement and connection timeouts. What to draw and how to predict is the
//! game's [`ClientView`].
use super::wire::{
    decode_server, encode_client, ClientMsg, LobbyState, ServerMsg, Token, INPUT_BUNDLE,
};
use super::{ClientView, NetGame};
use crate::viewer::net::{random_nonce, DatagramTransport};
use std::collections::VecDeque;
use std::net::SocketAddr;

/// Seconds between Hellos while connecting, and before giving up.
const HELLO_EVERY: f64 = 0.5;
const CONNECT_TIMEOUT: f64 = 8.;
/// Silence from the server this long means the connection is gone.
const SILENCE_LIMIT: f64 = 5.;
const PING_EVERY: f64 = 1.;
/// Inputs kept for replay (two seconds at 60 Hz).
const MAX_PENDING: usize = 120;

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub name: String,
    pub key: String,
    /// Preferred lobby choice; the server gives the first free one if it is taken.
    pub choice: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClientState {
    Connecting,
    Lobby,
    /// In a match (or looking at its results until the server returns everyone to the lobby).
    Playing,
    Rejected(String),
    Disconnected(String),
}

/// How prediction is doing, reported by the game's [`ClientView`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PredictionStats {
    /// Snapshots that disagreed with the prediction enough to move the predicted entity.
    pub corrections: u64,
    /// Corrections too big to ease (a hard snap).
    pub snaps: u64,
    pub last_error: f32,
    pub max_error: f32,
}

impl PredictionStats {
    /// Two spans of statistics as one: counts add, errors keep the larger.
    pub fn merged(&self, other: &PredictionStats) -> PredictionStats {
        PredictionStats {
            corrections: self.corrections + other.corrections,
            snaps: self.snaps + other.snaps,
            last_error: other.last_error,
            max_error: self.max_error.max(other.max_error),
        }
    }
}

/// What the network looked like from this side.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetStats {
    pub rtt_ms: f32,
    pub packets_in: u64,
    pub bytes_in: u64,
    pub snapshots: u64,
    pub prediction: PredictionStats,
}

pub struct NetClient<G: NetGame, T: DatagramTransport> {
    transport: T,
    server: SocketAddr,
    cfg: ClientConfig,
    state: ClientState,
    token: Option<Token>,
    id: u8,
    nonce: [u64; 2],
    lobby: Option<LobbyState>,
    want_choice: u8,
    want_ready: bool,
    sync_attempts: u32,
    last_sync: f64,
    view: G::View,
    /// Prediction statistics of matches already over: a view is reset between matches, the record is not.
    carried: PredictionStats,
    pending: VecDeque<(u32, G::Input)>,
    next_seq: u32,
    participant: Option<usize>,
    last_server_tick: u32,
    last_event_tick: u32,
    events: Vec<G::Event>,
    started_at: Option<f64>,
    last_hello: f64,
    last_ping: f64,
    last_packet: f64,
    now: f64,
    stats: NetStats,
}

impl<G: NetGame, T: DatagramTransport> NetClient<G, T> {
    pub fn new(transport: T, server: SocketAddr, cfg: ClientConfig) -> crate::Result<Self> {
        let want_choice = cfg.choice;
        Ok(Self {
            transport,
            server,
            cfg,
            state: ClientState::Connecting,
            token: None,
            id: 0,
            nonce: random_nonce()?,
            lobby: None,
            want_choice,
            want_ready: false,
            sync_attempts: 0,
            last_sync: f64::NEG_INFINITY,
            view: G::View::new(),
            carried: PredictionStats::default(),
            pending: VecDeque::new(),
            next_seq: 1,
            participant: None,
            last_server_tick: 0,
            last_event_tick: 0,
            events: Vec::new(),
            started_at: None,
            last_hello: f64::NEG_INFINITY,
            last_ping: f64::NEG_INFINITY,
            last_packet: 0.,
            now: 0.,
            stats: NetStats::default(),
        })
    }

    pub fn state(&self) -> &ClientState {
        &self.state
    }
    pub fn lobby(&self) -> Option<&LobbyState> {
        self.lobby.as_ref()
    }
    /// The game's view of the match: what to draw.
    pub fn view(&self) -> &G::View {
        &self.view
    }
    pub fn view_mut(&mut self) -> &mut G::View {
        &mut self.view
    }
    /// The participant this client drives, once the first snapshot has said so.
    pub fn participant(&self) -> Option<usize> {
        self.participant
    }
    /// The lobby seat number the server gave this client.
    pub fn seat(&self) -> u8 {
        self.id
    }
    /// This client's own lobby entry, if the server has listed it.
    pub fn my_entry(&self) -> Option<&super::LobbyEntry> {
        self.lobby
            .as_ref()?
            .entries
            .iter()
            .find(|e| e.slot == self.id)
    }
    /// The session token (a secret; exposed so tests can play the attacker).
    pub fn token(&self) -> Option<Token> {
        self.token
    }
    pub fn stats(&self) -> NetStats {
        NetStats {
            prediction: self.carried.merged(&self.view.prediction()),
            ..self.stats.clone()
        }
    }
    /// Events for effects and sound, oldest first, each delivered once.
    pub fn drain_events(&mut self) -> Vec<G::Event> {
        std::mem::take(&mut self.events)
    }

    fn send(&self, msg: &ClientMsg<G::Input>) {
        let _ = self
            .transport
            .try_send(self.server, &encode_client::<G>(msg));
    }

    /// Choose in the lobby. Un-readies you, as on the server.
    pub fn select(&mut self, choice: u8) {
        self.want_choice = choice;
        self.want_ready = false;
        self.sync_attempts = 0;
        self.last_sync = f64::NEG_INFINITY;
        if let (Some(token), ClientState::Lobby) = (self.token, &self.state) {
            self.send(&ClientMsg::Select { token, choice });
        }
    }

    pub fn ready(&mut self, ready: bool) {
        self.want_ready = ready;
        self.sync_attempts = 0;
        self.last_sync = f64::NEG_INFINITY;
        if let (Some(token), ClientState::Lobby) = (self.token, &self.state) {
            self.send(&ClientMsg::Ready { token, ready });
        }
    }

    /// Leave politely.
    pub fn leave(&mut self) {
        if let Some(token) = self.token {
            self.send(&ClientMsg::Leave { token });
        }
        self.state = ClientState::Disconnected("You left the game".into());
    }

    /// Re-send the lobby choice the server has not yet confirmed (a few times, then accept its answer).
    fn sync_lobby(&mut self, now: f64) {
        let (Some(token), Some(lobby)) = (self.token, &self.lobby) else {
            return;
        };
        if lobby.stage != 0 || now - self.last_sync < 0.25 {
            return;
        }
        let Some(me) = lobby.entries.iter().find(|e| e.slot == self.id).cloned() else {
            return;
        };
        if self.sync_attempts == 0
            && self.want_choice == self.cfg.choice
            && me.choice != self.want_choice
            && !self.want_ready
        {
            // The server gave us another choice at join time (ours was taken): take its answer.
            self.want_choice = me.choice;
        }
        if self.sync_attempts >= 8 {
            self.want_choice = me.choice;
            self.want_ready = me.ready;
            return;
        }
        let msg = if me.choice != self.want_choice {
            Some(ClientMsg::Select {
                token,
                choice: self.want_choice,
            })
        } else if me.ready != self.want_ready {
            Some(ClientMsg::Ready {
                token,
                ready: self.want_ready,
            })
        } else {
            self.sync_attempts = 0;
            None
        };
        if let Some(msg) = msg {
            self.sync_attempts += 1;
            self.last_sync = now;
            self.send(&msg);
        }
    }

    /// Read the network and keep the connection alive. Call every frame with a steady clock in seconds.
    pub fn poll(&mut self, now: f64) {
        self.now = now;
        let started = *self.started_at.get_or_insert(now);
        if let Ok(datagrams) = self.transport.receive() {
            for d in datagrams {
                if d.peer != self.server {
                    continue;
                }
                if let Ok(msg) = decode_server::<G>(&d.data) {
                    self.stats.packets_in += 1;
                    self.stats.bytes_in += d.data.len() as u64;
                    self.last_packet = now;
                    self.on_message(msg);
                }
            }
        }
        match &self.state {
            ClientState::Connecting => {
                if now - started > CONNECT_TIMEOUT {
                    self.state = ClientState::Rejected("Could not reach the server".into());
                } else if now - self.last_hello >= HELLO_EVERY {
                    self.last_hello = now;
                    self.send(&ClientMsg::Hello {
                        key: self.cfg.key.clone(),
                        name: self.cfg.name.clone(),
                        choice: self.cfg.choice,
                        nonce: self.nonce,
                        fingerprint: G::fingerprint() ^ super::server::name_hash_of::<G>(),
                    });
                }
            }
            ClientState::Lobby | ClientState::Playing => {
                if self.state == ClientState::Lobby {
                    self.sync_lobby(now);
                }
                if now - self.last_packet > SILENCE_LIMIT {
                    self.state = ClientState::Disconnected("Lost connection to the server".into());
                } else if now - self.last_ping >= PING_EVERY {
                    self.last_ping = now;
                    if let Some(token) = self.token {
                        self.send(&ClientMsg::Ping {
                            token,
                            stamp: (now * 1000.) as u32,
                            rtt_ms: self.stats.rtt_ms.round().clamp(0., 65535.) as u16,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    fn on_message(&mut self, msg: ServerMsg<G::Snapshot, G::Event>) {
        match msg {
            ServerMsg::Welcome { token, slot, .. } => {
                self.token = Some(token);
                self.id = slot;
                if self.state == ClientState::Connecting {
                    self.state = ClientState::Lobby;
                }
            }
            ServerMsg::Rejected { reason } => self.state = ClientState::Rejected(reason),
            ServerMsg::Lobby(l) => {
                let stage = l.stage;
                self.lobby = Some(l);
                match (&self.state, stage) {
                    (ClientState::Playing, 0) => self.back_to_lobby(),
                    (ClientState::Lobby, 1) => self.state = ClientState::Playing,
                    _ => {}
                }
            }
            ServerMsg::Snapshot(s) => {
                if s.server_tick <= self.last_server_tick {
                    return; // late or duplicated
                }
                self.last_server_tick = s.server_tick;
                self.stats.snapshots += 1;
                if self.state == ClientState::Lobby {
                    self.state = ClientState::Playing;
                }
                for (tick, event) in &s.events {
                    if *tick > self.last_event_tick {
                        self.events.push(event.clone());
                    }
                }
                if let Some(newest) = s.events.iter().map(|e| e.0).max() {
                    self.last_event_tick = self.last_event_tick.max(newest);
                }
                self.participant = (s.you != 255).then_some(s.you as usize);
                while self
                    .pending
                    .front()
                    .is_some_and(|(seq, _)| *seq <= s.applied_seq)
                {
                    self.pending.pop_front();
                }
                let pending: Vec<(u32, G::Input)> = self.pending.iter().cloned().collect();
                self.view
                    .on_snapshot(&s.snapshot, self.participant, &pending, self.now);
            }
            ServerMsg::Pong { stamp } => {
                let rtt = (self.now * 1000.) - stamp as f64;
                if (0. ..60_000.).contains(&rtt) {
                    let rtt = rtt as f32;
                    self.stats.rtt_ms = if self.stats.rtt_ms == 0. {
                        rtt
                    } else {
                        self.stats.rtt_ms * 0.8 + rtt * 0.2
                    };
                }
            }
        }
    }

    fn back_to_lobby(&mut self) {
        self.state = ClientState::Lobby;
        self.participant = None;
        self.pending.clear();
        self.carried = self.carried.merged(&self.view.prediction());
        self.view.reset();
    }

    /// One fixed tick of local input: predicted at once and sent. Call only while `state()` is `Playing`.
    pub fn tick(&mut self, input: G::Input) {
        if self.state != ClientState::Playing {
            return;
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        self.pending.push_back((seq, input));
        while self.pending.len() > MAX_PENDING {
            self.pending.pop_front();
        }
        self.view.on_input(&input);
        if let Some(token) = self.token {
            let frames: Vec<(u32, G::Input)> = self
                .pending
                .iter()
                .rev()
                .take(INPUT_BUNDLE)
                .rev()
                .cloned()
                .collect();
            self.send(&ClientMsg::Input {
                token,
                frames,
                ack_tick: self.last_server_tick,
            });
        }
    }

    /// Once per rendered frame: let the view interpolate and ease corrections.
    pub fn frame(&mut self, now: f64, dt: f32) {
        self.now = now;
        self.view.frame(now, dt);
    }
}
