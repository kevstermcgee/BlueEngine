//! The smallest complete [`NetGame`]: a footrace along a line. Read it as a template for your own game, and
//! use it in tests of anything built on the kit.
//!
//! Each participant runs along a line at a speed set by their lobby choice; input is -1, 0 or +1; the first to
//! reach [`GOAL`] wins. Participants beyond the connected players run at a steady pace (the game's own "AI").
//! The client predicts its own runner and corrects it from snapshots.
//!
//! ```
//! use std::time::Instant;
//! use vesper3d::viewer::net::loopback::LoopNet;
//! use vesper3d::viewer::netplay::{toy::{ToyGame, ToyInput}, ClientConfig, ClientState, NetClient, NetServer, ServerConfig, Stage};
//!
//! let net = LoopNet::new(1, 0, 0., 7);
//! let server_at = "10.0.0.1:4000".parse().unwrap();
//! let cfg = ServerConfig { participants: 3, countdown_seconds: 1, auto_start_seconds: 0, ..Default::default() };
//! let mut server = NetServer::<ToyGame, _>::new(net.endpoint(server_at), cfg).unwrap();
//! let mut client = NetClient::<ToyGame, _>::new(
//!     net.endpoint("10.0.0.2:4001".parse().unwrap()),
//!     server_at,
//!     ClientConfig { name: "you".into(), key: String::new(), choice: 1 },
//! )
//! .unwrap();
//! for tick in 0..300u64 {
//!     net.advance();
//!     server.poll(Instant::now());
//!     server.step(Instant::now());
//!     client.poll(tick as f64 / 60.);
//!     if *client.state() == ClientState::Lobby {
//!         client.ready(true);
//!     }
//!     if *client.state() == ClientState::Playing {
//!         client.tick(ToyInput { throttle: 1 });
//!     }
//! }
//! assert_eq!(server.stage(), Stage::Match);
//! assert!(client.view().mine.is_some_and(|x| x > 10.), "the predicted runner is running");
//! ```
use super::{ClientView, NetGame, PredictionStats, Seat, SettingKind, SettingSpec};
use crate::viewer::net::codec::{Reader, WireError, WireResult, Writer};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Distance to win.
pub const GOAL: f32 = 600.;
const MAX_POSITION: f32 = 100_000.;

pub struct ToyGame;

/// How fast the game's own runners go, in percent of the normal pace (the toy's `ai-speed` setting).
static AI_SPEED_PERCENT: AtomicU32 = AtomicU32::new(100);
/// Whether the game's own runners fill the places no player took (the toy's `bots` setting).
static BOTS: AtomicBool = AtomicBool::new(true);

/// The toy's two settings: a template for [`NetGame::settings`] and what the hub tests configure rooms with.
pub const TOY_SETTINGS: [SettingSpec; 2] = [
    SettingSpec {
        id: 1,
        name: "ai-speed",
        flag: "ai-speed",
        kind: SettingKind::Int,
        min: 25,
        max: 400,
        default: 100,
    },
    SettingSpec {
        id: 2,
        name: "bots",
        flag: "bots",
        kind: SettingKind::Bool,
        min: 0,
        max: 1,
        default: 1,
    },
];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ToyInput {
    /// -1 back, 0 stop, +1 forward.
    pub throttle: i8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToyMatch {
    pub tick: u32,
    pub position: Vec<f32>,
    pub speed: Vec<f32>,
    pub human: Vec<bool>,
    pub winner: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToySnapshot {
    pub tick: u32,
    pub position: Vec<f32>,
    pub speed: Vec<f32>,
    pub human: Vec<bool>,
    pub winner: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToyEvent {
    Started,
    Finished { participant: usize },
}

/// What a client of the toy game keeps: the latest snapshot and its own predicted position.
#[derive(Default)]
pub struct ToyView {
    pub snapshot: Option<ToySnapshot>,
    /// This client's predicted position.
    pub mine: Option<f32>,
    participant: Option<usize>,
    speed: f32,
    stats: PredictionStats,
}

impl ClientView<ToyGame> for ToyView {
    fn new() -> Self {
        Self::default()
    }

    fn on_snapshot(
        &mut self,
        s: &ToySnapshot,
        participant: Option<usize>,
        pending: &[(u32, ToyInput)],
        _now: f64,
    ) {
        self.participant = participant;
        if let Some(p) = participant.filter(|p| *p < s.position.len()) {
            self.speed = s.speed[p];
            let mut x = s.position[p];
            for (_, input) in pending {
                x += self.speed * input.throttle as f32;
            }
            if let Some(before) = self.mine {
                let error = (before - x).abs();
                self.stats.last_error = error;
                self.stats.max_error = self.stats.max_error.max(error);
                if error > 0.01 {
                    self.stats.corrections += 1;
                }
            }
            self.mine = Some(x);
        }
        self.snapshot = Some(s.clone());
    }

    fn on_input(&mut self, input: &ToyInput) {
        if let Some(x) = self.mine.as_mut() {
            *x += self.speed * input.throttle as f32;
        }
    }

    fn frame(&mut self, _now: f64, _dt: f32) {}

    fn reset(&mut self) {
        *self = Self::default();
    }

    fn prediction(&self) -> PredictionStats {
        self.stats.clone()
    }
}

impl NetGame for ToyGame {
    type Input = ToyInput;
    type Match = ToyMatch;
    type View = ToyView;
    type Snapshot = ToySnapshot;
    type Event = ToyEvent;

    const NAME: &'static str = "toy-footrace";
    const MAX_SEATS: usize = 8;
    const CHOICES: u8 = 8;

    fn fingerprint() -> u32 {
        GOAL.to_bits()
    }

    fn write_input(input: &ToyInput, w: &mut Writer) {
        w.u8(input.throttle as u8);
    }
    fn read_input(r: &mut Reader) -> WireResult<ToyInput> {
        let throttle = r.u8()? as i8;
        if !(-1..=1).contains(&throttle) {
            return Err(WireError("throttle out of range"));
        }
        Ok(ToyInput { throttle })
    }
    fn write_snapshot(s: &ToySnapshot, w: &mut Writer) {
        w.u32(s.tick);
        w.u8(s.position.len() as u8);
        for i in 0..s.position.len() {
            w.f32(s.position[i]);
            w.f32(s.speed[i]);
            w.bool(s.human[i]);
        }
        w.u8(s.winner.map_or(255, |p| p as u8));
    }
    fn read_snapshot(r: &mut Reader) -> WireResult<ToySnapshot> {
        let tick = r.u32()?;
        let n = r.u8()? as usize;
        if n > 32 {
            return Err(WireError("too many runners"));
        }
        let (mut position, mut speed, mut human) = (Vec::new(), Vec::new(), Vec::new());
        for _ in 0..n {
            position.push(r.f32_within(MAX_POSITION)?);
            speed.push(r.f32_within(100.)?);
            human.push(r.bool()?);
        }
        let winner = match r.u8()? {
            255 => None,
            w if (w as usize) < n => Some(w as usize),
            _ => return Err(WireError("bad winner")),
        };
        Ok(ToySnapshot {
            tick,
            position,
            speed,
            human,
            winner,
        })
    }
    fn write_event(e: &ToyEvent, w: &mut Writer) {
        match e {
            ToyEvent::Started => w.u8(0),
            ToyEvent::Finished { participant } => {
                w.u8(1);
                w.u8(*participant as u8);
            }
        }
    }
    fn read_event(r: &mut Reader) -> WireResult<ToyEvent> {
        match r.u8()? {
            0 => Ok(ToyEvent::Started),
            1 => Ok(ToyEvent::Finished {
                participant: r.u8()? as usize,
            }),
            _ => Err(WireError("unknown event")),
        }
    }

    fn settings() -> &'static [SettingSpec] {
        &TOY_SETTINGS
    }

    fn configure(values: &[(u8, u32)]) -> Result<(), String> {
        for &(id, value) in values {
            match id {
                1 => AI_SPEED_PERCENT.store(value, Ordering::Relaxed),
                2 => BOTS.store(value != 0, Ordering::Relaxed),
                _ => return Err(format!("the toy game has no setting {id}")),
            }
        }
        Ok(())
    }

    fn start(_seed: u64, seats: &[Seat], participants: usize) -> (ToyMatch, Vec<usize>) {
        let n = if BOTS.load(Ordering::Relaxed) {
            participants.max(seats.len())
        } else {
            seats.len()
        };
        let ai = 0.9 * AI_SPEED_PERCENT.load(Ordering::Relaxed) as f32 / 100.;
        let mut speed = vec![ai; n];
        let mut human = vec![false; n];
        for (participant, seat) in seats.iter().enumerate() {
            speed[participant] = 0.8 + 0.1 * seat.choice as f32;
            human[participant] = true;
        }
        (
            ToyMatch {
                tick: 0,
                position: vec![0.; n],
                speed,
                human,
                winner: None,
            },
            (0..seats.len()).collect(),
        )
    }

    fn participants(m: &ToyMatch) -> usize {
        m.position.len()
    }

    fn step(m: &mut ToyMatch, inputs: &[Option<ToyInput>]) -> Vec<ToyEvent> {
        let mut events = Vec::new();
        if m.winner.is_some() {
            return events;
        }
        m.tick += 1;
        if m.tick == 1 {
            events.push(ToyEvent::Started);
        }
        for i in 0..m.position.len() {
            let throttle = match inputs.get(i).copied().flatten() {
                Some(input) => input.throttle as f32,
                None => 1.,
            };
            m.position[i] += m.speed[i] * throttle;
            if m.position[i] >= GOAL && m.winner.is_none() {
                m.winner = Some(i);
                events.push(ToyEvent::Finished { participant: i });
            }
        }
        events
    }

    fn release(m: &mut ToyMatch, participant: usize) {
        if let Some(h) = m.human.get_mut(participant) {
            *h = false;
        }
    }

    fn snapshot(m: &ToyMatch, _participant: Option<usize>) -> ToySnapshot {
        ToySnapshot {
            tick: m.tick,
            position: m.position.clone(),
            speed: m.speed.clone(),
            human: m.human.clone(),
            winner: m.winner,
        }
    }

    fn is_over(m: &ToyMatch) -> bool {
        m.winner.is_some()
    }

    fn report(m: &ToyMatch) -> serde_json::Value {
        json!({ "ticks": m.tick, "winner": m.winner, "positions": m.position })
    }
}
