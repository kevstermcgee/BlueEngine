//! Online play for custom simulations: NetGame::MAX_SEATS sets player join limits; cli::serve supplies the server and hub rooms.
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
/// Print a progress line to stdout without panicking when nobody is reading it. `println!` panics on a closed pipe, and a
/// closed pipe is exactly what a supervisor (`be2-hub`) leaves behind when it retires a room: the room's server must carry
/// on until it is told to stop (or its stdin closes), not die with a panic message.
pub(crate) fn say(args: std::fmt::Arguments<'_>) {
    use std::io::Write as _;
    let _ = writeln!(std::io::stdout(), "{args}");
}

pub mod cli;
pub mod client;
mod event_channel;
pub mod failure;
pub mod hub;
pub mod server;
pub mod toy;
pub mod wire;

pub use client::{ClientConfig, ClientState, NetClient, NetStats, PredictionStats};
pub use failure::ConnectFailure;
pub use server::{
    hello_fingerprint, MatchLog, NetReport, NetServer, PeerReport, PeerStats, ServerConfig,
    ServerEventStats, ServerLoad, ServerSendStats, Stage, StatusSnapshot,
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

/// What kind of value a [`SettingSpec`] holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingKind {
    /// 0 or 1.
    Bool,
    /// A number between the spec's `min` and `max`.
    Int,
    /// One of `min..=max` options, by index (the game names them).
    Choice,
}

impl SettingKind {
    /// The word used in `--info` output: `bool`, `int` or `choice`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Int => "int",
            Self::Choice => "choice",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "bool" => Some(Self::Bool),
            "int" => Some(Self::Int),
            "choice" => Some(Self::Choice),
            _ => None,
        }
    }
}

/// One match setting a game's server accepts, so a hub can offer it to players without knowing the game.
///
/// Settings are typed numbers, never strings: a hub validates a request against `min..=max` and starts the
/// room's server with `--set <id>=<value>`, so nothing a player typed reaches a command line. See
/// [`cli`] for how the server prints and applies them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettingSpec {
    /// 1..=255, unique within the game, and never reused for something else (it is on the wire).
    pub id: u8,
    /// Lower-case words and hyphens, e.g. `kills` or `bot-skill`: what registries and `--flag` use.
    pub name: &'static str,
    /// The command-line flag (without the dashes) that sets it by hand when running a server yourself:
    /// `--flag VALUE`, or just `--flag` for a [`SettingKind::Bool`]. Usually equal to `name`.
    pub flag: &'static str,
    pub kind: SettingKind,
    pub min: u32,
    pub max: u32,
    pub default: u32,
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
    /// Opt into the bounded, independently acknowledged event stream. Changes the handshake fingerprint.
    /// Events must fit one datagram (at most 1047 encoded bytes at the standard payload limit).
    /// Retains at most 4096 events / 1 MiB / ten seconds; explicit gaps are observable on the client.
    const RELIABLE_EVENTS: bool = false;
    /// Human seats for this process's room settings. Clamped to `1..=MAX_SEATS`.
    fn lobby_capacity() -> usize {
        Self::MAX_SEATS
    }
    /// Connected humans required before ready or automatic countdown may start.
    fn minimum_players() -> usize {
        1
    }

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
    /// Optional payload adaptation: keep required state and prioritize nearby optional effects.
    /// The budget excludes engine framing. Defaults to the original snapshot for source compatibility.
    fn snapshot_with_budget(
        m: &Self::Match,
        participant: Option<usize>,
        max_bytes: usize,
    ) -> Self::Snapshot {
        let _ = max_bytes;
        Self::snapshot(m, participant)
    }
    fn is_over(m: &Self::Match) -> bool;
    /// A summary of the finished match for `matches.jsonl` (results, per-participant statistics).
    fn report(m: &Self::Match) -> serde_json::Value;

    /// Match settings a hub may choose per room (kill target, bots on or off). Optional: the default offers none.
    /// [`cli::serve`] prints them in `--info` and passes the chosen values to [`NetGame::configure`].
    fn settings() -> &'static [SettingSpec] {
        &[]
    }
    /// Apply the settings this server process was started with: one `(id, value)` for every entry of
    /// [`NetGame::settings`] (the default for any not chosen), values already checked against `min..=max`.
    /// Called once, before the server binds a socket; a process serves one set of settings (a hub starts one
    /// process per room), so storing them in a process-wide cell is fine. The default accepts and ignores them.
    fn configure(values: &[(u8, u32)]) -> Result<(), String> {
        let _ = values;
        Ok(())
    }
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
    ///
    /// OVERRIDE THIS if the game predicts its own entity. The default reports all zeros, so a game that
    /// forgets shows `corrections: 0, max_error: 0` in its stats and in `matches.jsonl` as if prediction were
    /// perfect. The default prints one warning to stderr naming the view type the first time it is used; a
    /// game with genuinely nothing to predict silences it by overriding with `PredictionStats::default()`.
    fn prediction(&self) -> PredictionStats {
        static WARNED: std::sync::Once = std::sync::Once::new();
        WARNED.call_once(|| {
            eprintln!(
                "[netplay] {} does not override ClientView::prediction(): its prediction statistics will read 0. \
                 Report the real numbers, or override it with PredictionStats::default() to silence this.",
                std::any::type_name::<Self>()
            );
        });
        PredictionStats::default()
    }
}
