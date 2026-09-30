//! Online play for games that own their simulation: a lobby, matches, results, on any [`DatagramTransport`](super::net::DatagramTransport).
//!
//! The stock runner networks `GameDocument` games. A game with its own rules (`custom-sim`) used to write its
//! own server and client from the transport up. Measured on Spooky Kart, that was about 2,000 lines, none of
//! them specific to karts: sessions, a lobby, input buffering, snapshot pacing, event delivery, prediction
//! plumbing, bot takeover, statistics. This module is those lines. A game supplies:
//!
//! 1. an implementation of [`NetGame`]: its rules as a match that steps on inputs, its input/snapshot/event
//!    layouts (written with [`codec`](super::net::codec)), and how a seat becomes a participant;
//! 2. a [`ClientView`]: what a client keeps between snapshots (typically a replica of the world plus its own
//!    predicted entity) and how it reconciles and interpolates.
//!
//! Everything else is here: [`NetServer`] runs lobby, countdown, match, results and back again, forever;
//! [`NetClient`] connects, keeps its lobby choices in sync despite packet loss, streams redundant input
//! bundles, and feeds snapshots and events to the view. [`toy`] is the smallest complete game, a few dozen
//! lines, and the tests use it. `docs/NETPLAY.md` explains the model and its guarantees.
//!
//! The server is authoritative: clients send intentions, never state. Snapshots go out at
//! `TICK_HZ / SNAPSHOT_EVERY`. A player who leaves or times out mid-match is handed to the game's own AI at once
//! ([`NetGame::release`]). Every finished match appends one JSON line to `matches.jsonl` (the game's report
//! plus per-player network quality and server load).
pub mod client;
pub mod server;
pub mod toy;
pub mod wire;

pub use client::{ClientConfig, ClientState, NetClient, NetStats, PredictionStats};
pub use server::{
    MatchLog, NetReport, NetServer, PeerReport, PeerStats, ServerConfig, ServerLoad, Stage,
};
pub use wire::{LobbyEntry, LobbyState, MAX_DATAGRAM};

use super::net::codec::{Reader, WireResult, Writer};

/// A player waiting in the lobby, as the game sees them when a match starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seat {
    /// Lobby seat number, unique among connected players.
    pub id: u8,
    /// The lobby choice made (a character, a car, a team), `0..NetGame::CHOICES`.
    pub choice: u8,
    pub name: String,
}

/// Everything the kit needs to know about a game. Implemented once per game, on any type (usually a unit struct).
pub trait NetGame: Sized + 'static {
    /// One player's intent for one tick: throttle, steering, buttons.
    type Input: Copy + Default + PartialEq + 'static;
    /// The running match on the server: the game's simulation.
    type Match;
    /// What a client keeps and draws: see [`ClientView`].
    type View: ClientView<Self>;
    /// What the server tells a client about the match at one tick.
    type Snapshot: Clone;
    /// Something that happened, for sound and effects: delivered to each client once.
    type Event: Clone;

    /// Names the game in fingerprints and logs.
    const NAME: &'static str;
    /// Most players at once (at most 16).
    const MAX_SEATS: usize;
    /// How many lobby choices exist (`Seat::choice` is below this).
    const CHOICES: u8;
    /// Simulation rate.
    const TICK_HZ: u64 = 60;
    /// A snapshot goes out every this many ticks.
    const SNAPSHOT_EVERY: u64 = 2;
    /// Whether two players may not pick the same choice.
    const UNIQUE_CHOICES: bool = true;

    /// A number that changes whenever anything both sides must agree on changes (rules, numbers, map). Peers
    /// with a different fingerprint are refused, so a stale client never plays a new server.
    fn fingerprint() -> u32;

    fn write_input(input: &Self::Input, w: &mut Writer);
    fn read_input(r: &mut Reader) -> WireResult<Self::Input>;
    fn write_snapshot(snapshot: &Self::Snapshot, w: &mut Writer);
    fn read_snapshot(r: &mut Reader) -> WireResult<Self::Snapshot>;
    fn write_event(event: &Self::Event, w: &mut Writer);
    fn read_event(r: &mut Reader) -> WireResult<Self::Event>;

    /// Begin a match. `participants` is how many entities should take part in total (players plus the game's
    /// own AI); the game decides who fills the rest. Returns the match and, for each seat in the order given,
    /// the participant that seat drives.
    fn start(seed: u64, seats: &[Seat], participants: usize) -> (Self::Match, Vec<usize>);
    /// How many participants the match has.
    fn participants(m: &Self::Match) -> usize;
    /// Advance one tick. `inputs[i]` is participant `i`'s input if a human drives it, `None` if the game's own
    /// AI does. Returns what happened.
    fn step(m: &mut Self::Match, inputs: &[Option<Self::Input>]) -> Vec<Self::Event>;
    /// A human left: from now on the game's own AI drives this participant.
    fn release(m: &mut Self::Match, participant: usize);
    /// The state for a client driving `participant` (`None` for a spectator).
    fn snapshot(m: &Self::Match, participant: Option<usize>) -> Self::Snapshot;
    fn is_over(m: &Self::Match) -> bool;
    /// A summary of the finished match for `matches.jsonl` (results, per-participant statistics).
    fn report(m: &Self::Match) -> serde_json::Value;
}

/// What a client does with the server's answers. It owns the replica the game draws from.
pub trait ClientView<G: NetGame> {
    fn new() -> Self;
    /// A snapshot arrived at time `now` (seconds). `participant` is the entity this client drives (`None` if
    /// spectating); `pending` are the inputs it has sent that the server had not yet applied, oldest first:
    /// replay them on top of the snapshot so prediction survives the round trip.
    fn on_snapshot(
        &mut self,
        snapshot: &G::Snapshot,
        participant: Option<usize>,
        pending: &[(u32, G::Input)],
        now: f64,
    );
    /// One fixed tick of local input is about to be sent: predict its effect now.
    fn on_input(&mut self, input: &G::Input);
    /// Once per rendered frame: interpolate others, ease corrections out of your own entity.
    fn frame(&mut self, now: f64, dt: f32);
    /// Back to the lobby: forget the match.
    fn reset(&mut self);
    /// How well prediction is doing (for statistics).
    fn prediction(&self) -> PredictionStats {
        PredictionStats::default()
    }
}
