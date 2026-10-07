//! One reusable server `main` for every netplay game: [`serve`].
//!
//! Four games had copied the same 65-line server executable (listen address, transport, join key, auto-start,
//! report directory, seed, a participants flag), and a hub that supervises many games' servers needs more from
//! them than that copy gave: a machine-readable status line, a description of the game and its settings, and a
//! server that dies with its parent. A game now writes its server like this:
//!
//! ```no_run
//! use vesper3d::viewer::netplay::cli::{serve, Participants, ServeSpec};
//! use vesper3d::viewer::netplay::toy::ToyGame;
//!
//! fn main() -> vesper3d::Result<()> {
//!     serve::<ToyGame>(&ServeSpec {
//!         bin_name: "toy-server",
//!         about: "The toy footrace server.",
//!         default_listen: "0.0.0.0:4100",
//!         default_report_dir: "toy-data",
//!         join_key_env: "TOY_JOIN_KEY",
//!         participants: Participants::Flag { flag: "seats", min: 1, max: 8, default: 8 },
//!         default_auto_start: 45,
//!     })
//! }
//! ```
//!
//! # Flags
//! `--listen ADDR`, `--transport development|production`, `--join-key KEY` (or the environment variable the spec
//! names), `--auto-start SECONDS`, `--report-dir DIR`, `--seed N`, the participants flag the spec names (clamped
//! to its range, or fixed), one `--<flag>` per [`SettingSpec`] of the game and the generic
//! `--set ID=VALUE` a hub uses, `--status-lines`, `--exit-on-stdin-eof`, `--info` and `--help`. SIGINT and SIGTERM
//! stop the server cleanly through [`shutdown`](crate::viewer::shutdown).
//!
//! # `--info` (what a hub learns before it starts a room)
//! Prints `key=value` lines and exits with status 0 (see [`Info`]):
//! ```text
//! game=toy-footrace
//! fingerprint=40160000
//! build=a1b2c3d4
//! max_seats=8
//! tick_hz=60
//! setting=1:laps:laps:int:1:9:3
//! ```
//! `fingerprint` is the game's own [`NetGame::fingerprint`] (8 hex digits). `build` is [`build_id`]: the value a
//! client sends in its `Hello` (the fingerprint folded with a hash of the game's name) folded once more with the
//! netplay envelope version, so an engine bump that changes the wire also changes the build a hub reports.
//! `setting=<id>:<name>:<flag>:<kind>:<min>:<max>:<default>` appears once per setting.
//!
//! # `--status-lines`
//! Once a second the server prints, on stdout,
//! `STATUS game=<name> players=<n> max=<participants> stage=lobby|match|results build=<hex8>` ([`Status`]).
//!
//! Std only, no window: this module is always compiled.
use super::server::hello_fingerprint;
use super::{wire, NetGame, NetServer, ServerConfig, SettingKind, SettingSpec, Stage};
use crate::viewer::net::{server_transport, TransportProfile};
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The value a hub reports as a game's build, and a client compares with its own: the `Hello` fingerprint
/// ([`hello_fingerprint`]: game fingerprint xor a hash of the game name) xor the netplay envelope version
/// ([`wire::PROTOCOL`]) multiplied by the odd constant `0x9E37_79B1`. A change to the game, its name, or the
/// netplay wire changes the build.
pub fn build_id<G: NetGame>() -> u32 {
    fold_build(hello_fingerprint::<G>())
}

/// [`build_id`] from a `Hello` fingerprint (so it can be checked without a game type).
pub fn fold_build(hello: u32) -> u32 {
    hello ^ (wire::PROTOCOL as u32).wrapping_mul(0x9E37_79B1)
}

// ---- settings schema ------------------------------------------------------------------------------------------------

/// A [`SettingSpec`] with owned text: what `--info` prints and a hub parses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingDef {
    pub id: u8,
    pub name: String,
    pub flag: String,
    pub kind: SettingKind,
    pub min: u32,
    pub max: u32,
    pub default: u32,
}

impl From<&SettingSpec> for SettingDef {
    fn from(s: &SettingSpec) -> Self {
        Self {
            id: s.id,
            name: s.name.to_string(),
            flag: s.flag.to_string(),
            kind: s.kind,
            min: s.min,
            max: s.max,
            default: s.default,
        }
    }
}

fn word_ok(w: &str) -> bool {
    (1..=24).contains(&w.len())
        && w.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !w.starts_with('-')
}

impl SettingDef {
    /// The `setting=` value: `<id>:<name>:<flag>:<kind>:<min>:<max>:<default>`.
    pub fn to_field(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}:{}:{}",
            self.id,
            self.name,
            self.flag,
            self.kind.as_str(),
            self.min,
            self.max,
            self.default
        )
    }

    /// Parse the part after `setting=`. The result is checked with [`SettingDef::validate`].
    pub fn parse_field(field: &str) -> Result<Self, String> {
        let parts: Vec<&str> = field.split(':').collect();
        let [id, name, flag, kind, min, max, default] = parts[..] else {
            return Err(format!(
                "setting needs 7 colon-separated fields, got {}",
                parts.len()
            ));
        };
        let num = |what: &str, v: &str| {
            v.parse::<u32>()
                .map_err(|_| format!("setting {what} {v:?} is not a number"))
        };
        let def = Self {
            id: id
                .parse::<u8>()
                .map_err(|_| format!("setting id {id:?} is not 1..=255"))?,
            name: name.into(),
            flag: flag.into(),
            kind: SettingKind::parse(kind)
                .ok_or_else(|| format!("setting kind {kind:?} is not bool, int or choice"))?,
            min: num("min", min)?,
            max: num("max", max)?,
            default: num("default", default)?,
        };
        def.validate()?;
        Ok(def)
    }

    /// Id 1..=255, name and flag are 1..=24 of `a-z 0-9 -`, `min <= default <= max`, a bool is exactly `0..=1`.
    pub fn validate(&self) -> Result<(), String> {
        if self.id == 0 {
            return Err("setting ids start at 1".into());
        }
        if !word_ok(&self.name) {
            return Err(format!(
                "setting name {:?} must be 1-24 of a-z, 0-9 and -",
                self.name
            ));
        }
        if !word_ok(&self.flag) {
            return Err(format!(
                "setting flag {:?} must be 1-24 of a-z, 0-9 and -",
                self.flag
            ));
        }
        if self.min > self.max || self.default < self.min || self.default > self.max {
            return Err(format!(
                "setting {}: needs min <= default <= max, got {}..={} default {}",
                self.name, self.min, self.max, self.default
            ));
        }
        if self.kind == SettingKind::Bool && (self.min, self.max) != (0, 1) {
            return Err(format!("setting {}: a bool is exactly 0..=1", self.name));
        }
        Ok(())
    }

    /// `value` if it is within `min..=max`.
    pub fn check(&self, value: u32) -> Result<u32, String> {
        if value < self.min || value > self.max {
            Err(format!(
                "setting {} must be {}..={}, got {value}",
                self.name, self.min, self.max
            ))
        } else {
            Ok(value)
        }
    }
}

/// Every definition valid, ids, names and flags unique.
pub fn validate_schema(defs: &[SettingDef]) -> Result<(), String> {
    for (i, d) in defs.iter().enumerate() {
        d.validate()?;
        if defs[..i]
            .iter()
            .any(|o| o.id == d.id || o.name == d.name || o.flag == d.flag)
        {
            return Err(format!(
                "setting {} repeats an id, name or flag of another setting",
                d.name
            ));
        }
    }
    Ok(())
}

/// The full list of `(id, value)` a server is configured with: the defaults, overridden by `chosen`
/// (each checked against its range; an unknown id or a repeated one is an error). Sorted by id.
pub fn resolve_settings(
    defs: &[SettingDef],
    chosen: &[(u8, u32)],
) -> Result<Vec<(u8, u32)>, String> {
    let mut values: Vec<(u8, u32)> = defs.iter().map(|d| (d.id, d.default)).collect();
    let mut seen: Vec<u8> = Vec::new();
    for &(id, value) in chosen {
        let def = defs
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| format!("unknown setting id {id}"))?;
        if seen.contains(&id) {
            return Err(format!("setting {} given twice", def.name));
        }
        seen.push(id);
        let value = def.check(value)?;
        if let Some(slot) = values.iter_mut().find(|(i, _)| *i == id) {
            slot.1 = value;
        }
    }
    values.sort_by_key(|(id, _)| *id);
    Ok(values)
}

// ---- --info ---------------------------------------------------------------------------------------------------------

/// What `--info` prints: who the server is and which settings it takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    /// [`NetGame::NAME`].
    pub game: String,
    /// The game's raw [`NetGame::fingerprint`].
    pub fingerprint: u32,
    /// [`build_id`].
    pub build: u32,
    pub max_seats: usize,
    pub tick_hz: u64,
    pub settings: Vec<SettingDef>,
}

impl Info {
    pub fn of<G: NetGame>() -> Self {
        Self {
            game: G::NAME.to_string(),
            fingerprint: G::fingerprint(),
            build: build_id::<G>(),
            max_seats: G::MAX_SEATS,
            tick_hz: G::TICK_HZ,
            settings: G::settings().iter().map(SettingDef::from).collect(),
        }
    }

    /// The `--info` output.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "game={}", self.game);
        let _ = writeln!(out, "fingerprint={:08x}", self.fingerprint);
        let _ = writeln!(out, "build={:08x}", self.build);
        let _ = writeln!(out, "max_seats={}", self.max_seats);
        let _ = writeln!(out, "tick_hz={}", self.tick_hz);
        for s in &self.settings {
            let _ = writeln!(out, "setting={}", s.to_field());
        }
        out
    }

    /// Parse `--info` output. Unknown keys are ignored (a newer server may print more); a missing or malformed
    /// required key, or an invalid settings schema, is an error naming the line.
    pub fn parse(text: &str) -> Result<Self, String> {
        let (mut game, mut fingerprint, mut build, mut max_seats, mut tick_hz) =
            (None, None, None, None, None);
        let mut settings = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let at = |e: String| format!("--info line {}: {e}", n + 1);
            let Some((key, value)) = line.split_once('=') else {
                continue; // a stray log line before the data
            };
            let hex = |v: &str| {
                if v.len() == 8 {
                    u32::from_str_radix(v, 16).map_err(|_| format!("{key} is not 8 hex digits"))
                } else {
                    Err(format!("{key} is not 8 hex digits"))
                }
            };
            match key {
                "game" => {
                    if !game_id_ok(value) {
                        return Err(at(format!("game {value:?} is not 1-24 of a-z, 0-9 and -")));
                    }
                    game = Some(value.to_string());
                }
                "fingerprint" => fingerprint = Some(hex(value).map_err(at)?),
                "build" => build = Some(hex(value).map_err(at)?),
                "max_seats" => {
                    max_seats = Some(
                        value
                            .parse::<usize>()
                            .ok()
                            .filter(|n| (1..=16).contains(n))
                            .ok_or_else(|| at("max_seats is not 1..=16".into()))?,
                    )
                }
                "tick_hz" => {
                    tick_hz = Some(
                        value
                            .parse::<u64>()
                            .ok()
                            .filter(|n| *n > 0)
                            .ok_or_else(|| at("tick_hz is not a positive number".into()))?,
                    )
                }
                "setting" => settings.push(SettingDef::parse_field(value).map_err(at)?),
                _ => {}
            }
        }
        validate_schema(&settings)?;
        let missing = |k: &str| format!("--info output has no {k}= line");
        Ok(Self {
            game: game.ok_or_else(|| missing("game"))?,
            fingerprint: fingerprint.ok_or_else(|| missing("fingerprint"))?,
            build: build.ok_or_else(|| missing("build"))?,
            max_seats: max_seats.ok_or_else(|| missing("max_seats"))?,
            tick_hz: tick_hz.ok_or_else(|| missing("tick_hz"))?,
            settings,
        })
    }
}

/// A game id: 1..=24 of `a-z`, `0-9`, `-` (what `NetGame::NAME` must be for a hub to carry the game).
pub fn game_id_ok(id: &str) -> bool {
    word_ok(id)
}

// ---- --status-lines -------------------------------------------------------------------------------------------------

/// The stage a status line reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusStage {
    Lobby,
    Match,
    Results,
}

impl StatusStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lobby => "lobby",
            Self::Match => "match",
            Self::Results => "results",
        }
    }
}

impl From<Stage> for StatusStage {
    fn from(s: Stage) -> Self {
        match s {
            Stage::Lobby => Self::Lobby,
            Stage::Match => Self::Match,
            Stage::Results => Self::Results,
        }
    }
}

/// One `STATUS` line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub game: String,
    pub players: u8,
    /// Participants per match: the room's capacity.
    pub max: u8,
    pub stage: StatusStage,
    pub build: u32,
}

impl Status {
    pub fn to_line(&self) -> String {
        format!(
            "STATUS game={} players={} max={} stage={} build={:08x}",
            self.game,
            self.players,
            self.max,
            self.stage.as_str(),
            self.build
        )
    }

    /// `None` for anything that is not a well-formed status line. Unknown `key=value` words are ignored.
    pub fn parse_line(line: &str) -> Option<Self> {
        let mut words = line.split_whitespace();
        if words.next()? != "STATUS" {
            return None;
        }
        let (mut game, mut players, mut max, mut stage, mut build) = (None, None, None, None, None);
        for w in words {
            match w.split_once('=')? {
                ("game", v) => game = Some(v.to_string()),
                ("players", v) => players = Some(v.parse::<u8>().ok()?),
                ("max", v) => max = Some(v.parse::<u8>().ok()?),
                ("stage", "lobby") => stage = Some(StatusStage::Lobby),
                ("stage", "match") => stage = Some(StatusStage::Match),
                ("stage", "results") => stage = Some(StatusStage::Results),
                ("stage", _) => return None,
                ("build", v) => build = u32::from_str_radix(v, 16).ok(),
                _ => {}
            }
        }
        Some(Self {
            game: game?,
            players: players?,
            max: max?,
            stage: stage?,
            build: build?,
        })
    }
}

// ---- arguments ------------------------------------------------------------------------------------------------------

/// How a game's server decides how many participants a match has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Participants {
    /// A flag like `--racers N`, clamped to `min..=max`.
    Flag {
        flag: &'static str,
        min: usize,
        max: usize,
        default: usize,
    },
    /// Always this many; there is no flag.
    Fixed(usize),
}

/// What a game says about its server; everything else is shared.
#[derive(Clone, Copy, Debug)]
pub struct ServeSpec {
    /// The executable's name, for `--help`.
    pub bin_name: &'static str,
    /// One sentence for `--help`.
    pub about: &'static str,
    /// e.g. `0.0.0.0:4100`.
    pub default_listen: &'static str,
    pub default_report_dir: &'static str,
    /// The environment variable that may hold the join key, e.g. `SPOOKY_KART_JOIN_KEY`.
    pub join_key_env: &'static str,
    pub participants: Participants,
    /// Seconds after the first player joins before the countdown starts on its own (0 = never).
    pub default_auto_start: u32,
}

/// The parsed command line of a server that is going to run.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub listen: String,
    pub transport: TransportProfile,
    pub join_key: Option<String>,
    pub auto_start: u32,
    pub report_dir: PathBuf,
    /// Recent match reports retained in memory; the disk archive remains complete.
    pub history_limit: usize,
    pub seed: Option<u64>,
    pub participants: usize,
    pub status_lines: bool,
    pub exit_on_stdin_eof: bool,
    /// Every setting of the game with the value to apply, sorted by id.
    pub settings: Vec<(u8, u32)>,
}

/// What the command line asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Run(Options),
    Info,
    Help,
}

/// Parse a server command line (without the program name). `env_join_key` is the value of the spec's
/// environment variable, if set. Pure: no files, no sockets, no process exit.
pub fn parse_args(
    spec: &ServeSpec,
    schema: &[SettingDef],
    args: &[String],
    env_join_key: Option<String>,
) -> Result<Command, String> {
    let mut o = Options {
        listen: spec.default_listen.to_string(),
        transport: TransportProfile::Development,
        join_key: env_join_key.filter(|k| !k.is_empty()),
        auto_start: spec.default_auto_start,
        report_dir: PathBuf::from(spec.default_report_dir),
        history_limit: 256,
        seed: None,
        participants: match spec.participants {
            Participants::Flag {
                default, min, max, ..
            } => default.clamp(min, max),
            Participants::Fixed(n) => n,
        },
        status_lines: false,
        exit_on_stdin_eof: false,
        settings: Vec::new(),
    };
    let mut chosen: Vec<(u8, u32)> = Vec::new();
    let mut i = 0;
    let value = |i: &mut usize, flag: &str| -> Result<String, String> {
        *i += 1;
        args.get(*i)
            .cloned()
            .ok_or_else(|| format!("{flag} needs a value"))
    };
    fn number<T: std::str::FromStr>(flag: &str, v: &str) -> Result<T, String> {
        v.parse()
            .map_err(|_| format!("{flag} needs a number, got {v:?}"))
    }
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "--listen" => o.listen = value(&mut i, arg)?,
            "--transport" => {
                o.transport = value(&mut i, arg)?
                    .parse::<TransportProfile>()
                    .map_err(|e| e.to_string())?
            }
            "--join-key" => o.join_key = Some(value(&mut i, arg)?),
            "--auto-start" => o.auto_start = number(arg, &value(&mut i, arg)?)?,
            "--report-dir" => o.report_dir = value(&mut i, arg)?.into(),
            "--history-limit" => o.history_limit = number(arg, &value(&mut i, arg)?)?,
            "--seed" => o.seed = Some(number(arg, &value(&mut i, arg)?)?),
            "--status-lines" => o.status_lines = true,
            "--exit-on-stdin-eof" => o.exit_on_stdin_eof = true,
            "--info" => return Ok(Command::Info),
            "--help" | "-h" => return Ok(Command::Help),
            "--set" => {
                let v = value(&mut i, arg)?;
                let (id, val) = v
                    .split_once('=')
                    .ok_or_else(|| format!("--set needs ID=VALUE, got {v:?}"))?;
                chosen.push((number("--set id", id)?, number("--set value", val)?));
            }
            other => {
                if let (Participants::Flag { flag, min, max, .. }, Some(name)) =
                    (spec.participants, other.strip_prefix("--"))
                {
                    if name == flag {
                        let n: usize = number(other, &value(&mut i, other)?)?;
                        o.participants = n.clamp(min, max);
                        i += 1;
                        continue;
                    }
                }
                let def = other
                    .strip_prefix("--")
                    .and_then(|name| schema.iter().find(|d| d.flag == name));
                let Some(def) = def else {
                    return Err(format!("unknown argument {other} (try --help)"));
                };
                let v = if def.kind == SettingKind::Bool {
                    1
                } else {
                    number(other, &value(&mut i, other)?)?
                };
                chosen.push((def.id, v));
            }
        }
        i += 1;
    }
    o.settings = resolve_settings(schema, &chosen)?;
    Ok(Command::Run(o))
}

/// The `--help` text.
pub fn help_text(spec: &ServeSpec, schema: &[SettingDef]) -> String {
    let mut t = format!(
        "{}: {}\n\nUSAGE:\n  {} [OPTIONS]\n\nOPTIONS:\n",
        spec.bin_name, spec.about, spec.bin_name
    );
    let mut line = |flag: &str, text: String| {
        let _ = writeln!(t, "  {flag:<26} {text}");
    };
    line(
        "--listen ADDR",
        format!("address to listen on (default {})", spec.default_listen),
    );
    line(
        "--transport T",
        "development (raw UDP: LAN, tests, hubs) or production (QUIC/TLS 1.3)".into(),
    );
    line(
        "--join-key KEY",
        format!(
            "require this key in every Hello (or set {})",
            spec.join_key_env
        ),
    );
    line(
        "--auto-start SECONDS",
        format!(
            "start the countdown this long after the first player joins; 0 = never (default {})",
            spec.default_auto_start
        ),
    );
    line(
        "--report-dir DIR",
        format!(
            "where matches.jsonl goes (default {})",
            spec.default_report_dir
        ),
    );
    line("--seed N", "fixed seed, for reproducible matches".into());
    line(
        "--history-limit N",
        "recent reports kept in memory; 0 = none (default 256); disk archive unchanged".into(),
    );
    match spec.participants {
        Participants::Flag {
            flag,
            min,
            max,
            default,
        } => line(
            &format!("--{flag} N"),
            format!("participants per match, {min}..={max} (default {default})"),
        ),
        Participants::Fixed(n) => line("(participants)", format!("always {n}")),
    }
    for d in schema {
        let flag = match d.kind {
            SettingKind::Bool => format!("--{}", d.flag),
            _ => format!("--{} N", d.flag),
        };
        line(
            &flag,
            format!(
                "setting {} (id {}): {}..={}, default {}",
                d.name, d.id, d.min, d.max, d.default
            ),
        );
    }
    line(
        "--set ID=VALUE",
        "a setting by id (what a hub passes)".into(),
    );
    line(
        "--status-lines",
        "print a STATUS line on stdout once a second".into(),
    );
    line(
        "--exit-on-stdin-eof",
        "quit when stdin closes (a parent that dies takes the server with it)".into(),
    );
    line(
        "--info",
        "print game, fingerprint, build and settings, then exit".into(),
    );
    t
}

// ---- running --------------------------------------------------------------------------------------------------------

/// The whole server `main` for game `G`: parse `std::env::args`, handle `--info` and `--help`, otherwise run
/// until SIGINT or SIGTERM. Returns the error `main` should report; a bad command line is one.
pub fn serve<G: NetGame>(spec: &ServeSpec) -> crate::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stop = Arc::new(AtomicBool::new(false));
    let mut out = std::io::stdout();
    run::<G>(spec, &args, &mut out, stop, true)
}

/// [`serve`] with the arguments, output and stop flag given, so a test can drive it in-process.
/// `signals` installs the SIGINT/SIGTERM handlers (process-wide: leave it off in tests).
pub fn run<G: NetGame>(
    spec: &ServeSpec,
    args: &[String],
    out: &mut dyn Write,
    stop: Arc<AtomicBool>,
    signals: bool,
) -> crate::Result<()> {
    let schema: Vec<SettingDef> = G::settings().iter().map(SettingDef::from).collect();
    validate_schema(&schema).map_err(|e| format!("{}: bad settings declaration: {e}", G::NAME))?;
    if !game_id_ok(G::NAME) {
        return Err(format!(
            "NetGame::NAME {:?} must be 1-24 of a-z, 0-9 and - to be carried by a hub",
            G::NAME
        )
        .into());
    }
    let env_key = std::env::var(spec.join_key_env).ok();
    let opts = match parse_args(spec, &schema, args, env_key)? {
        Command::Info => {
            out.write_all(Info::of::<G>().to_text().as_bytes())?;
            return Ok(());
        }
        Command::Help => {
            out.write_all(help_text(spec, &schema).as_bytes())?;
            return Ok(());
        }
        Command::Run(o) => o,
    };
    G::configure(&opts.settings).map_err(|e| format!("{}: settings refused: {e}", G::NAME))?;
    let cfg = ServerConfig {
        join_key: opts.join_key.clone(),
        participants: opts.participants,
        auto_start_seconds: opts.auto_start,
        report_dir: Some(opts.report_dir.clone()),
        seed: opts.seed,
        ..Default::default()
    };
    super::say(format_args!(
        "[Server] {} on {}, {} transport, {} participants, join key {}",
        G::NAME,
        opts.listen,
        opts.transport,
        opts.participants,
        if opts.join_key.is_some() {
            "required"
        } else {
            "not required"
        }
    ));
    if opts.transport == TransportProfile::Development
        && opts.join_key.is_none()
        && !opts.listen.starts_with("127.")
    {
        super::say(format_args!(
            "[Server] Warning: development UDP is unencrypted; use --transport production for the internet"
        ));
    }
    let transport = server_transport(opts.transport, &opts.listen)?;
    let mut server = NetServer::<G, _>::new(transport, cfg)?;
    server.set_match_history_limit(Some(opts.history_limit));
    if opts.exit_on_stdin_eof {
        // The hub keeps our stdin open; when it dies (even by SIGKILL) the pipe closes and so do we: no orphans.
        std::thread::spawn(|| {
            let mut sink = [0u8; 256];
            let mut stdin = std::io::stdin();
            while matches!(stdin.read(&mut sink), Ok(n) if n > 0) {}
            std::process::exit(0);
        });
    }
    if signals {
        crate::viewer::shutdown::install(stop.clone())?;
    }
    let build = build_id::<G>();
    let result = if opts.status_lines {
        server.run_realtime_with(stop, None, |s| {
            let line = Status {
                game: G::NAME.to_string(),
                players: s.players.min(255) as u8,
                max: s.participants.clamp(1, 255) as u8,
                stage: s.stage.into(),
                build,
            }
            .to_line();
            let _ = writeln!(out, "{line}");
            let _ = out.flush();
        })
    } else {
        server.run_realtime(stop, None)
    };
    crate::viewer::shutdown::finished();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::netplay::toy::ToyGame;

    const SPEC: ServeSpec = ServeSpec {
        bin_name: "toy-server",
        about: "test",
        default_listen: "0.0.0.0:4100",
        default_report_dir: "toy-data",
        join_key_env: "TOY_JOIN_KEY_UNSET_FOR_TESTS",
        participants: Participants::Flag {
            flag: "seats",
            min: 1,
            max: 8,
            default: 8,
        },
        default_auto_start: 45,
    };

    fn defs() -> Vec<SettingDef> {
        vec![
            SettingDef::parse_field("1:kills:kills:int:1:500:40").unwrap(),
            SettingDef::parse_field("2:bots:bots:bool:0:1:0").unwrap(),
            SettingDef::parse_field("3:mode:mode:choice:0:2:1").unwrap(),
        ]
    }

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    fn run_args(s: &str) -> Result<Command, String> {
        parse_args(&SPEC, &defs(), &args(s), None)
    }

    #[test]
    fn build_is_the_hello_fingerprint_folded_with_the_envelope_version() {
        let hello = hello_fingerprint::<ToyGame>();
        assert_eq!(build_id::<ToyGame>(), fold_build(hello));
        assert_eq!(
            build_id::<ToyGame>(),
            hello ^ (wire::PROTOCOL as u32).wrapping_mul(0x9E37_79B1)
        );
        assert_ne!(build_id::<ToyGame>(), hello);
        assert_ne!(fold_build(hello), fold_build(hello ^ 1));
    }

    #[test]
    fn info_round_trips_and_has_the_documented_lines() {
        let info = Info {
            game: "toy-footrace".into(),
            fingerprint: 0x0123_abcd,
            build: 0xdead_beef,
            max_seats: 8,
            tick_hz: 60,
            settings: defs(),
        };
        let text = info.to_text();
        assert_eq!(
            text,
            "game=toy-footrace\nfingerprint=0123abcd\nbuild=deadbeef\nmax_seats=8\ntick_hz=60\n\
             setting=1:kills:kills:int:1:500:40\nsetting=2:bots:bots:bool:0:1:0\nsetting=3:mode:mode:choice:0:2:1\n"
        );
        assert_eq!(Info::parse(&text), Ok(info));
        let real = Info::of::<ToyGame>();
        assert_eq!(Info::parse(&real.to_text()), Ok(real));
    }

    #[test]
    fn info_parsing_is_strict_where_it_matters_and_lenient_elsewhere() {
        let good = "game=a\nfingerprint=00000001\nbuild=00000002\nmax_seats=4\ntick_hz=30\n";
        assert!(Info::parse(good).is_ok());
        // Unknown keys and log noise are ignored.
        assert!(Info::parse(&format!("hello\nfuture=1\n{good}")).is_ok());
        for broken in [
            "",
            "game=a\n",
            &good.replace("game=a", "game=Bad Name"),
            &good.replace("00000001", "1"),
            &good.replace("00000001", "0000000z"),
            &good.replace("max_seats=4", "max_seats=0"),
            &good.replace("max_seats=4", "max_seats=17"),
            &good.replace("tick_hz=30", "tick_hz=0"),
            &format!("{good}setting=1:a:a:int:5:1:3\n"),
            &format!("{good}setting=1:a:a:bool:0:2:0\n"),
            &format!("{good}setting=0:a:a:int:0:1:0\n"),
            &format!("{good}setting=1:A:a:int:0:1:0\n"),
            &format!("{good}setting=1:a:a:float:0:1:0\n"),
            &format!("{good}setting=1:a:a:int:0:1\n"),
            &format!("{good}setting=1:a:a:int:0:1:0\nsetting=1:b:b:int:0:1:0\n"),
            &format!("{good}setting=1:a:a:int:0:1:0\nsetting=2:a:b:int:0:1:0\n"),
        ] {
            assert!(Info::parse(broken).is_err(), "{broken:?}");
        }
    }

    #[test]
    fn status_lines_round_trip() {
        let s = Status {
            game: "toy-footrace".into(),
            players: 2,
            max: 8,
            stage: StatusStage::Match,
            build: 0xabcd_0123,
        };
        let line = s.to_line();
        assert_eq!(
            line,
            "STATUS game=toy-footrace players=2 max=8 stage=match build=abcd0123"
        );
        assert_eq!(Status::parse_line(&line), Some(s));
        for stage in [StatusStage::Lobby, StatusStage::Results] {
            let s = Status {
                game: "g".into(),
                players: 0,
                max: 1,
                stage,
                build: 0,
            };
            assert_eq!(Status::parse_line(&s.to_line()), Some(s));
        }
        // Extra words are ignored; broken lines are not lines.
        assert!(Status::parse_line(&format!("{line} future=1")).is_some());
        for bad in [
            "",
            "[Server] tick 5",
            "STATUS",
            "STATUS players=1",
            "STATUS game=a players=1 max=2 stage=playing build=00000000",
            "STATUS game=a players=300 max=2 stage=lobby build=00000000",
            "STATUS game=a players=1 max=2 stage=lobby build=zz",
            "status game=a players=1 max=2 stage=lobby build=00000000",
        ] {
            assert_eq!(Status::parse_line(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn defaults_and_the_shared_flags_parse() {
        let Ok(Command::Run(o)) = run_args("") else {
            panic!()
        };
        assert_eq!(o.listen, "0.0.0.0:4100");
        assert_eq!(o.transport, TransportProfile::Development);
        assert_eq!((o.auto_start, o.participants, o.seed), (45, 8, None));
        assert_eq!(o.report_dir, PathBuf::from("toy-data"));
        assert_eq!(o.history_limit, 256);
        assert!(!o.status_lines && !o.exit_on_stdin_eof && o.join_key.is_none());
        assert_eq!(o.settings, vec![(1, 40), (2, 0), (3, 1)]);
        let Ok(Command::Run(o)) = run_args(
            "--listen 127.0.0.1:0 --transport production --join-key k --auto-start 0 --report-dir /tmp/x --seed 7 \
             --seats 99 --status-lines --exit-on-stdin-eof",
        ) else {
            panic!()
        };
        assert_eq!(o.listen, "127.0.0.1:0");
        assert_eq!(o.transport, TransportProfile::Production);
        assert_eq!(o.join_key.as_deref(), Some("k"));
        assert_eq!((o.auto_start, o.seed, o.participants), (0, Some(7), 8));
        assert!(o.status_lines && o.exit_on_stdin_eof);
        let Ok(Command::Run(o)) = run_args("--seats 0") else {
            panic!()
        };
        assert_eq!(o.participants, 1, "clamped up");
    }

    #[test]
    fn history_limit_accepts_zero_and_rejects_invalid_values() {
        let Ok(Command::Run(o)) = run_args("--history-limit 0") else {
            panic!()
        };
        assert_eq!(o.history_limit, 0);
        for text in [
            "--history-limit",
            "--history-limit -1",
            "--history-limit many",
        ] {
            assert!(run_args(text).is_err());
        }
    }

    #[test]
    fn the_environment_supplies_the_join_key_and_a_flag_wins() {
        let parse = |a: &str, env: Option<&str>| {
            let Ok(Command::Run(o)) = parse_args(&SPEC, &defs(), &args(a), env.map(String::from))
            else {
                panic!()
            };
            o.join_key
        };
        assert_eq!(parse("", Some("e")).as_deref(), Some("e"));
        assert_eq!(parse("--join-key f", Some("e")).as_deref(), Some("f"));
        assert_eq!(parse("", Some("")), None, "an empty variable is no key");
    }

    #[test]
    fn fixed_participants_have_no_flag() {
        let spec = ServeSpec {
            participants: Participants::Fixed(12),
            ..SPEC
        };
        let Ok(Command::Run(o)) = parse_args(&spec, &[], &[], None) else {
            panic!()
        };
        assert_eq!(o.participants, 12);
        assert!(parse_args(&spec, &[], &args("--seats 3"), None).is_err());
    }

    #[test]
    fn settings_come_from_set_flags_and_their_own_flags_and_are_validated() {
        let Ok(Command::Run(o)) = run_args("--set 1=25 --bots --mode 2") else {
            panic!()
        };
        assert_eq!(o.settings, vec![(1, 25), (2, 1), (3, 2)]);
        let Ok(Command::Run(o)) = run_args("--kills 500") else {
            panic!()
        };
        assert_eq!(o.settings[0], (1, 500));
        for bad in [
            "--set 1=0",
            "--set 1=501",
            "--set 9=1",
            "--set 1",
            "--set x=1",
            "--set 1=-3",
            "--set 1=1 --kills 2",
            "--set 3=3",
            "--kills",
            "--kills many",
            "--seats",
            "--listen",
            "--transport carrier-pigeon",
            "--wat",
            "stray",
        ] {
            assert!(run_args(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn info_and_help_short_circuit_other_flags() {
        assert_eq!(run_args("--status-lines --info"), Ok(Command::Info));
        assert_eq!(run_args("--help"), Ok(Command::Help));
        assert_eq!(run_args("-h"), Ok(Command::Help));
        let help = help_text(&SPEC, &defs());
        for needle in [
            "--listen",
            "--transport",
            "--join-key",
            "--auto-start",
            "--report-dir",
            "--seed",
            "--seats N",
            "--kills N",
            "--bots",
            "--set ID=VALUE",
            "--status-lines",
            "--exit-on-stdin-eof",
            "--info",
            "TOY_JOIN_KEY_UNSET_FOR_TESTS",
        ] {
            assert!(help.contains(needle), "{needle} missing from\n{help}");
        }
    }

    #[test]
    fn resolve_fills_defaults_and_rejects_unknown_repeated_and_out_of_range() {
        let d = defs();
        assert_eq!(resolve_settings(&d, &[]), Ok(vec![(1, 40), (2, 0), (3, 1)]));
        assert_eq!(
            resolve_settings(&d, &[(3, 0), (1, 1)]),
            Ok(vec![(1, 1), (2, 0), (3, 0)])
        );
        assert!(resolve_settings(&d, &[(1, 1), (1, 2)]).is_err());
        assert!(resolve_settings(&d, &[(7, 1)]).is_err());
        assert!(resolve_settings(&d, &[(2, 2)]).is_err());
        assert_eq!(resolve_settings(&[], &[]), Ok(vec![]));
    }

    #[test]
    fn run_prints_info_and_help_without_binding_anything() {
        let stop = Arc::new(AtomicBool::new(false));
        let mut out = Vec::new();
        run::<ToyGame>(&SPEC, &args("--info"), &mut out, stop.clone(), false).unwrap();
        assert_eq!(
            Info::parse(&String::from_utf8(out).unwrap()),
            Ok(Info::of::<ToyGame>())
        );
        let mut out = Vec::new();
        run::<ToyGame>(&SPEC, &args("--help"), &mut out, stop.clone(), false).unwrap();
        assert!(String::from_utf8(out).unwrap().contains("--listen"));
        let mut out = Vec::new();
        assert!(run::<ToyGame>(&SPEC, &args("--bogus"), &mut out, stop, false).is_err());
    }

    #[test]
    fn a_running_server_prints_parseable_status_lines_and_stops_on_the_flag() {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        let mut out = Vec::new();
        run::<ToyGame>(
            &SPEC,
            &args("--listen 127.0.0.1:0 --status-lines --seats 5 --report-dir /nonexistent-never-written"),
            &mut out,
            stop,
            false,
        )
        .unwrap();
        handle.join().unwrap();
        let text = String::from_utf8(out).unwrap();
        let statuses: Vec<Status> = text.lines().filter_map(Status::parse_line).collect();
        assert!(statuses.len() >= 2, "{text:?}");
        let first = &statuses[0];
        assert_eq!(first.game, "toy-footrace");
        assert_eq!((first.players, first.max), (0, 5));
        assert_eq!(first.stage, StatusStage::Lobby);
        assert_eq!(first.build, build_id::<ToyGame>());
    }
}
