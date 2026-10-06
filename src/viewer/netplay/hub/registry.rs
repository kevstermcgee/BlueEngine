//! Which games this hub carries: the config file, the games' `--info`, and the checks on both.
//!
//! The wire never names a path. A client says `game = "spooky-kart"`; this registry, loaded from a file only
//! the box's owner can write, says which executable that is, what its settings may be, and whether the game has
//! a permanent Public room. A game whose server is missing or whose `--info` fails is skipped with a log line
//! and the others carry on.
//!
//! # The config file
//! Line based, std-only parsing. Blank lines and lines starting with `#` or `;` are ignored (comments are whole
//! lines: a `#` inside a value is part of the value). `[hub]` is optional; `[game ID]` appears once per game.
//! Keys are lower case, `key = value`, each at most once per section. Relative paths are relative to the
//! directory of the config file.
//!
//! ```text
//! [hub]
//! listen = 0.0.0.0:4100          # the hub's port; every game is reached through it
//! bind_ip = 0.0.0.0              # the IP room servers bind (default: the listen IP)
//! pool_start = 4101              # rooms use pool_start .. pool_start + pool_size - 1 (default: listen port + 1)
//! pool_size = 16                 #   (1..=64, default 16); the listen port itself must not be in the pool
//! report_dir = /home/kevin/.local/share/blueengine/reports   # <report_dir>/<game>/port-<n>/matches.jsonl
//! legacy = serve                 # answer old DFHB Deadfall clients: serve | refuse (default serve)
//! max_rooms_per_ip = 2           # rooms one creator IP may have open (default 2)
//! max_processes = 16             # room servers in all, Public rooms included (default: pool_size)
//! rate_burst = 10                # requests one source address may send at once (default 10) ...
//! rate_per_sec = 2               # ... and per second after that (default 2); more is met with silence
//!
//! [game deadfall]
//! server = /home/kevin/blueengine/deadfall-server   # required; built with netplay::cli::serve
//! public = on                    # a permanent Public room, restarted if it dies (default off)
//! public_name = Public           # (default Public)
//! public_set = bots=1            # settings of the Public room, by name: name=value,name=value
//! user_set = kills=40            # defaults for player-made rooms (a player's own choice wins)
//! client_settings = bots,kills   # which settings players may choose (default: all of the game's)
//! max_rooms = 4                  # player-made rooms at once (default 4, at most 24)
//! transport = development        # the only transport hub rooms have (default); production is refused, see below
//! auto_start = 30                # passed as --auto-start (default: the server's own)
//!
//! [game spooky-kart]
//! server = /home/kevin/blueengine/spooky-kart-server
//! public = on
//! public_set = laps=3
//! max_rooms = 4
//! ```
//!
//! # Transport
//! Rooms the hub starts are raw UDP (the engine's "development" transport, no encryption, no server
//! authentication): the hub protocol carries neither a transport nor a join key, so a client has no way to
//! learn that a room wants anything else. `transport = production` is therefore **refused at load** with an
//! explanation instead of being passed to the server (a room nobody could join) or quietly run as raw UDP (a
//! downgrade). A game that needs QUIC/TLS is run by hand with `--transport production` (docs/HOSTING.md). The
//! hub's rate limits and creation cookies keep abuse down; they are not encryption and not authentication.
use super::legacy::Mode;
use super::wire::{game_id_ok, sanitize_name};
use crate::viewer::netplay::cli::{resolve_settings, Info, SettingDef};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

/// Most player-made rooms one game may be configured for.
pub const MAX_ROOMS_HARD: usize = 24;
/// Most ports in the pool.
pub const MAX_POOL: u16 = 64;
/// How long `--info` may take before the server is declared broken.
pub const INFO_TIMEOUT: Duration = Duration::from_secs(5);
/// How much `--info` may print before the server is declared broken (a real one prints well under 4 KB).
pub const INFO_MAX_BYTES: usize = 64 * 1024;

/// A problem in the config file, with its line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    /// 1-based; 0 when the problem is not on one line.
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line > 0 {
            write!(f, "line {}: {}", self.line, self.message)
        } else {
            f.write_str(&self.message)
        }
    }
}

impl std::error::Error for ConfigError {}

/// The `[hub]` section as written (everything optional).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HubSection {
    pub listen: Option<SocketAddr>,
    pub bind_ip: Option<Ipv4Addr>,
    pub pool_start: Option<u16>,
    pub pool_size: Option<u16>,
    pub report_dir: Option<PathBuf>,
    pub legacy: Option<Mode>,
    pub max_rooms_per_ip: Option<usize>,
    pub max_processes: Option<usize>,
    /// Requests one source address may burst, and per second after that (default 10 and 2).
    pub rate_burst: Option<f64>,
    pub rate_per_sec: Option<f64>,
}

/// The hub's settings after defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct HubSettings {
    pub listen: SocketAddr,
    pub bind_ip: Ipv4Addr,
    pub pool_start: u16,
    pub pool_size: u16,
    pub report_dir: PathBuf,
    pub legacy: Mode,
    pub max_rooms_per_ip: usize,
    pub max_processes: usize,
    pub rate_burst: f64,
    pub rate_per_sec: f64,
}

impl HubSettings {
    /// The UDP ports to open on the router: the hub's, then the pool.
    pub fn ports(&self) -> Vec<u16> {
        std::iter::once(self.listen.port())
            .chain(self.pool_start..self.pool_start + self.pool_size)
            .collect()
    }
}

impl HubSection {
    /// The settings with `over` (command-line flags) winning over this (the file) winning over the defaults.
    pub fn resolve(&self, over: &HubSection) -> Result<HubSettings, String> {
        let listen = over
            .listen
            .or(self.listen)
            .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], super::wire::DEFAULT_PORT)));
        let SocketAddr::V4(listen4) = listen else {
            return Err("listen must be an IPv4 address and port like 0.0.0.0:4100".into());
        };
        let pool_size = over.pool_size.or(self.pool_size).unwrap_or(16);
        if !(1..=MAX_POOL).contains(&pool_size) {
            return Err(format!("pool_size must be 1..={MAX_POOL}"));
        }
        let pool_start = over
            .pool_start
            .or(self.pool_start)
            .unwrap_or_else(|| listen.port().saturating_add(1));
        if pool_start == 0 || pool_start as u32 + pool_size as u32 - 1 > 65_535 {
            return Err("the room ports would run past 65535; use a lower pool_start".into());
        }
        let pool = pool_start..pool_start + pool_size;
        if pool.contains(&listen.port()) {
            return Err(format!(
                "the hub's own port {} is inside the room pool {}-{}",
                listen.port(),
                pool.start,
                pool.end - 1
            ));
        }
        let max_processes = over
            .max_processes
            .or(self.max_processes)
            .unwrap_or(pool_size as usize)
            .clamp(1, pool_size as usize);
        Ok(HubSettings {
            listen,
            bind_ip: over.bind_ip.or(self.bind_ip).unwrap_or(*listen4.ip()),
            pool_start,
            pool_size,
            report_dir: over
                .report_dir
                .clone()
                .or_else(|| self.report_dir.clone())
                .unwrap_or_else(|| "be2-hub-data".into()),
            legacy: over.legacy.or(self.legacy).unwrap_or(Mode::Serve),
            max_rooms_per_ip: over.max_rooms_per_ip.or(self.max_rooms_per_ip).unwrap_or(2),
            max_processes,
            rate_burst: over.rate_burst.or(self.rate_burst).unwrap_or(10.),
            rate_per_sec: over.rate_per_sec.or(self.rate_per_sec).unwrap_or(2.),
        })
    }
}

/// One `[game ID]` section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameConfig {
    pub id: String,
    pub server: PathBuf,
    pub public: bool,
    pub public_name: String,
    /// `(setting name, value)`, checked against the game's schema when the game is loaded.
    pub public_set: Vec<(String, u32)>,
    pub user_set: Vec<(String, u32)>,
    /// `None` = players may choose any setting of the game.
    pub client_settings: Option<Vec<String>>,
    pub max_rooms: usize,
    pub transport: String,
    pub auto_start: Option<u32>,
}

/// A parsed config file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Config {
    pub hub: HubSection,
    pub games: Vec<GameConfig>,
}

/// Parse a config file's text. `base_dir` resolves relative paths.
pub fn parse_config(text: &str, base_dir: &Path) -> Result<Config, ConfigError> {
    #[derive(PartialEq)]
    enum Section {
        None,
        Hub,
        Game(usize),
    }
    let err = |line: usize, message: String| ConfigError { line, message };
    let mut cfg = Config::default();
    let mut section = Section::None;
    let mut seen_hub = false;
    let mut seen: Vec<String> = Vec::new();
    let mut games_server: Vec<bool> = Vec::new();
    let path = |v: &str| {
        let p = PathBuf::from(v);
        if p.is_absolute() {
            p
        } else {
            base_dir.join(p)
        }
    };
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.trim_start_matches('\u{feff}').trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(head) = line.strip_prefix('[') {
            let head = head
                .strip_suffix(']')
                .ok_or_else(|| err(n, "a section header must end with ]".into()))?
                .trim();
            seen.clear();
            let mut words = head.split_whitespace();
            match (words.next(), words.next(), words.next()) {
                (Some("hub"), None, _) => {
                    if seen_hub {
                        return Err(err(n, "[hub] appears twice".into()));
                    }
                    seen_hub = true;
                    section = Section::Hub;
                }
                (Some("game"), Some(id), None) => {
                    if !game_id_ok(id) {
                        return Err(err(
                            n,
                            format!("game id {id:?} must be 1-24 of a-z, 0-9 and -"),
                        ));
                    }
                    if cfg.games.iter().any(|g| g.id == id) {
                        return Err(err(n, format!("[game {id}] appears twice")));
                    }
                    cfg.games.push(GameConfig {
                        id: id.to_string(),
                        server: PathBuf::new(),
                        public: false,
                        public_name: "Public".into(),
                        public_set: Vec::new(),
                        user_set: Vec::new(),
                        client_settings: None,
                        max_rooms: 4,
                        transport: "development".into(),
                        auto_start: None,
                    });
                    games_server.push(false);
                    section = Section::Game(cfg.games.len() - 1);
                }
                _ => {
                    return Err(err(
                        n,
                        format!("unknown section [{head}] (use [hub] or [game ID])"),
                    ))
                }
            }
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .map(|(k, v)| (k.trim(), v.trim()))
            .ok_or_else(|| err(n, format!("expected key = value, got {line:?}")))?;
        if section == Section::None {
            return Err(err(n, format!("{key} is outside any section")));
        }
        if seen.iter().any(|k| k == key) {
            return Err(err(n, format!("{key} is given twice in this section")));
        }
        seen.push(key.to_string());
        let num = |what: &str| -> Result<u64, ConfigError> {
            value
                .parse::<u64>()
                .map_err(|_| err(n, format!("{what} needs a number, got {value:?}")))
        };
        let flag = || -> Result<bool, ConfigError> {
            match value {
                "on" | "yes" | "true" => Ok(true),
                "off" | "no" | "false" => Ok(false),
                _ => Err(err(n, format!("{key} must be on or off, got {value:?}"))),
            }
        };
        let pairs = || -> Result<Vec<(String, u32)>, ConfigError> {
            let mut out: Vec<(String, u32)> = Vec::new();
            for item in value.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                let (name, v) = item.split_once('=').ok_or_else(|| {
                    err(n, format!("{key} items look like name=value, got {item:?}"))
                })?;
                let (name, v) = (name.trim(), v.trim());
                if !game_id_ok(name) {
                    return Err(err(n, format!("{key}: {name:?} is not a setting name")));
                }
                if out.iter().any(|(o, _)| o == name) {
                    return Err(err(n, format!("{key}: {name} is given twice")));
                }
                out.push((
                    name.to_string(),
                    v.parse::<u32>()
                        .map_err(|_| err(n, format!("{key}: {name} needs a number, got {v:?}")))?,
                ));
            }
            Ok(out)
        };
        match section {
            Section::None => unreachable!(),
            Section::Hub => {
                let h = &mut cfg.hub;
                match key {
                    "listen" => {
                        h.listen = Some(
                            value
                                .parse::<SocketAddr>()
                                .ok()
                                .filter(SocketAddr::is_ipv4)
                                .ok_or_else(|| {
                                    err(n, format!("listen must look like 0.0.0.0:4100, got {value:?}"))
                                })?,
                        )
                    }
                    "bind_ip" => {
                        h.bind_ip = Some(value.parse().map_err(|_| {
                            err(n, format!("bind_ip must be an IPv4 address, got {value:?}"))
                        })?)
                    }
                    "pool_start" => {
                        h.pool_start = Some(
                            u16::try_from(num("pool_start")?)
                                .ok()
                                .filter(|p| *p != 0)
                                .ok_or_else(|| err(n, "pool_start is not a port".into()))?,
                        )
                    }
                    "pool_size" => {
                        h.pool_size = Some(
                            u16::try_from(num("pool_size")?)
                                .map_err(|_| err(n, "pool_size is too large".into()))?,
                        )
                    }
                    "report_dir" => h.report_dir = Some(path(value)),
                    "legacy" => h.legacy = Some(value.parse().map_err(|e| err(n, e))?),
                    "max_rooms_per_ip" => h.max_rooms_per_ip = Some(num("max_rooms_per_ip")? as usize),
                    "max_processes" => h.max_processes = Some(num("max_processes")? as usize),
                    "rate_burst" | "rate_per_sec" => {
                        let v = value
                            .parse::<f64>()
                            .ok()
                            .filter(|v| v.is_finite() && *v > 0.)
                            .ok_or_else(|| err(n, format!("{key} needs a positive number, got {value:?}")))?;
                        if key == "rate_burst" {
                            h.rate_burst = Some(v);
                        } else {
                            h.rate_per_sec = Some(v);
                        }
                    }
                    _ => {
                        return Err(err(
                            n,
                            format!(
                                "unknown [hub] key {key} (known: listen, bind_ip, pool_start, pool_size, report_dir, legacy, max_rooms_per_ip, max_processes, rate_burst, rate_per_sec)"
                            ),
                        ))
                    }
                }
            }
            Section::Game(g) => {
                let game = &mut cfg.games[g];
                match key {
                    "server" => {
                        if value.is_empty() {
                            return Err(err(n, "server needs a path".into()));
                        }
                        game.server = path(value);
                        games_server[g] = true;
                    }
                    "public" => game.public = flag()?,
                    "public_name" => {
                        game.public_name = sanitize_name(value).map_err(|e| {
                            err(n, format!("public_name is not a valid room name: {}", e.text()))
                        })?
                    }
                    "public_set" => game.public_set = pairs()?,
                    "user_set" => game.user_set = pairs()?,
                    "client_settings" => {
                        let names: Vec<String> = value
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        if let Some(bad) = names.iter().find(|s| !game_id_ok(s)) {
                            return Err(err(n, format!("client_settings: {bad:?} is not a setting name")));
                        }
                        game.client_settings = Some(names);
                    }
                    "max_rooms" => {
                        let v = num("max_rooms")? as usize;
                        if v > MAX_ROOMS_HARD {
                            return Err(err(n, format!("max_rooms is at most {MAX_ROOMS_HARD}")));
                        }
                        game.max_rooms = v;
                    }
                    "transport" => match value {
                        "development" => game.transport = value.to_string(),
                        "production" => {
                            return Err(err(
                                n,
                                format!(
                                    "transport = production is not available for hub rooms: the hub starts raw UDP rooms and its protocol carries neither a transport nor a join key, so [game {}] could not be joined. Remove the line (development is the only hub transport), or run that game's server by hand with --transport production (docs/HOSTING.md)",
                                    game.id
                                ),
                            ))
                        }
                        _ => {
                            return Err(err(
                                n,
                                format!("transport must be development, got {value:?}"),
                            ))
                        }
                    },
                    "auto_start" => {
                        game.auto_start = Some(
                            u32::try_from(num("auto_start")?)
                                .map_err(|_| err(n, "auto_start is too large".into()))?,
                        )
                    }
                    _ => {
                        return Err(err(
                            n,
                            format!(
                                "unknown [game] key {key} (known: server, public, public_name, public_set, user_set, client_settings, max_rooms, transport, auto_start)"
                            ),
                        ))
                    }
                }
            }
        }
    }
    for (g, has) in cfg.games.iter().zip(games_server) {
        if !has {
            return Err(err(0, format!("[game {}] has no server = path", g.id)));
        }
    }
    Ok(cfg)
}

// ---- running `--info` ---------------------------------------------------------------------------------------------------

/// Which version of a file this is: when it was last written and how big it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileKey {
    pub mtime: Option<SystemTime>,
    pub size: u64,
}

impl FileKey {
    pub fn of(path: &Path) -> Option<FileKey> {
        let m = std::fs::metadata(path).ok()?;
        m.is_file().then(|| FileKey {
            mtime: m.modified().ok(),
            size: m.len(),
        })
    }
}

/// Where the registry learns about a game's server: the real one runs `--info`, tests use a fake.
pub trait InfoSource {
    /// The server file's current version, `None` if it is missing.
    fn key(&mut self, server: &Path) -> Option<FileKey>;
    /// The server's `--info`, and the file version it came from.
    fn info(&mut self, server: &Path) -> Result<(Info, FileKey), String>;
}

/// Runs `<server> --info` and caches the answer by (path, mtime, size): the program runs again only when the
/// file changed.
pub struct ProcessInfo {
    cache: HashMap<PathBuf, (FileKey, Info)>,
    /// How many times a server was really run (tests).
    pub runs: usize,
    timeout: Duration,
}

impl Default for ProcessInfo {
    fn default() -> Self {
        Self {
            cache: HashMap::new(),
            runs: 0,
            timeout: INFO_TIMEOUT,
        }
    }
}

impl ProcessInfo {
    pub fn new() -> Self {
        Self::default()
    }

    /// Like [`ProcessInfo::new`] with another `--info` time limit (`be2-hub verify --info-timeout`).
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            timeout,
            ..Self::default()
        }
    }

    /// Run `<server> --info` once, with the standard time and output limits.
    pub fn run(server: &Path) -> Result<Info, String> {
        Self::run_with(server, INFO_TIMEOUT)
    }

    /// Run `<server> --info` with a time limit and at most [`INFO_MAX_BYTES`] of output: a server that hangs,
    /// floods its output or keeps the pipe open through a child process is killed and reported, never waited for.
    pub fn run_with(server: &Path, timeout: Duration) -> Result<Info, String> {
        use std::io::Read;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{mpsc, Arc};
        let mut child = Command::new(server)
            .arg("--info")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot run {} --info: {e}", server.display()))?;
        let mut out = child.stdout.take().ok_or("no stdout")?;
        let too_long = Arc::new(AtomicBool::new(false));
        let flag = too_long.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut text = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                match out.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) if text.len() + n > INFO_MAX_BYTES => {
                        flag.store(true, Ordering::SeqCst);
                        break;
                    }
                    Ok(n) => text.extend_from_slice(&chunk[..n]),
                }
            }
            let _ = tx.send(text);
        });
        let started = Instant::now();
        let status = loop {
            if too_long.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} --info printed more than {} bytes",
                    server.display(),
                    INFO_MAX_BYTES
                ));
            }
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) if started.elapsed() < timeout => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "{} --info did not finish in {} s",
                        server.display(),
                        timeout.as_secs().max(1)
                    ));
                }
                Err(e) => return Err(format!("waiting for {} --info: {e}", server.display())),
            }
        };
        let text = match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(text) => text,
            Err(_) if too_long.load(Ordering::SeqCst) => {
                return Err(format!(
                    "{} --info printed more than {} bytes",
                    server.display(),
                    INFO_MAX_BYTES
                ))
            }
            Err(_) => {
                return Err(format!(
                    "{} --info exited but left its output open (a child process holds it)",
                    server.display()
                ))
            }
        };
        if !status.success() {
            return Err(format!("{} --info exited with {status}", server.display()));
        }
        Info::parse(&String::from_utf8_lossy(&text))
            .map_err(|e| format!("{}: {e}", server.display()))
    }
}

impl InfoSource for ProcessInfo {
    fn key(&mut self, server: &Path) -> Option<FileKey> {
        FileKey::of(server)
    }

    fn info(&mut self, server: &Path) -> Result<(Info, FileKey), String> {
        let key = FileKey::of(server)
            .ok_or_else(|| format!("the server binary {} does not exist", server.display()))?;
        if let Some((k, info)) = self.cache.get(server) {
            if *k == key {
                return Ok((info.clone(), key));
            }
        }
        let info = Self::run_with(server, self.timeout)?;
        self.runs += 1;
        self.cache.insert(server.to_path_buf(), (key, info.clone()));
        Ok((info, key))
    }
}

// ---- the registry ---------------------------------------------------------------------------------------------------------

/// A game the hub carries: its config, what its server said, and the settings resolved to ids.
#[derive(Clone, Debug)]
pub struct GameEntry {
    pub config: GameConfig,
    pub info: Info,
    /// The full `(id, value)` list a Public room starts with.
    pub public_settings: Vec<(u8, u32)>,
    /// The full list a player-made room starts with when the player chooses nothing.
    pub user_defaults: Vec<(u8, u32)>,
    /// The setting ids a player may choose.
    pub client_ids: Vec<u8>,
    key: Option<FileKey>,
    /// A changed file version seen once; applied once it is seen unchanged a second time.
    pending: Option<FileKey>,
    /// A file version whose `--info` failed: not tried again until the file changes.
    failed: Option<FileKey>,
}

/// Why a setting request was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingFault(pub String);

impl GameEntry {
    pub fn new(config: GameConfig, info: Info, key: Option<FileKey>) -> Result<GameEntry, String> {
        let by_name = |pairs: &[(String, u32)], what: &str| -> Result<Vec<(u8, u32)>, String> {
            pairs
                .iter()
                .map(|(name, v)| {
                    let def = info
                        .settings
                        .iter()
                        .find(|d| &d.name == name)
                        .ok_or_else(|| {
                            format!(
                                "{what} names the setting {name}, which {} does not have",
                                config.id
                            )
                        })?;
                    def.check(*v)
                        .map(|v| (def.id, v))
                        .map_err(|e| format!("{what}: {e}"))
                })
                .collect()
        };
        let public_settings =
            resolve_settings(&info.settings, &by_name(&config.public_set, "public_set")?)?;
        let user_defaults =
            resolve_settings(&info.settings, &by_name(&config.user_set, "user_set")?)?;
        let client_ids = match &config.client_settings {
            None => info.settings.iter().map(|d| d.id).collect(),
            Some(names) => names
                .iter()
                .map(|name| {
                    info.settings
                        .iter()
                        .find(|d| &d.name == name)
                        .map(|d| d.id)
                        .ok_or_else(|| {
                            format!(
                                "client_settings names {name}, which {} does not have",
                                config.id
                            )
                        })
                })
                .collect::<Result<_, _>>()?,
        };
        Ok(GameEntry {
            config,
            info,
            public_settings,
            user_defaults,
            client_ids,
            key,
            pending: None,
            failed: None,
        })
    }

    pub fn id(&self) -> &str {
        &self.config.id
    }

    pub fn schema(&self) -> &[SettingDef] {
        &self.info.settings
    }

    /// The settings of a player-made room: the game's defaults, the config's `user_set`, then what the player
    /// chose. A player may only choose settings in `client_settings`, each at most once, each within its range.
    pub fn room_settings(&self, chosen: &[(u8, u32)]) -> Result<Vec<(u8, u32)>, SettingFault> {
        let mut out = self.user_defaults.clone();
        let mut seen: Vec<u8> = Vec::new();
        for &(id, value) in chosen {
            if !self.client_ids.contains(&id) {
                return Err(SettingFault(format!("setting {id} cannot be chosen here")));
            }
            if seen.contains(&id) {
                return Err(SettingFault(format!("setting {id} given twice")));
            }
            seen.push(id);
            let def = self
                .info
                .settings
                .iter()
                .find(|d| d.id == id)
                .ok_or_else(|| SettingFault(format!("unknown setting {id}")))?;
            let value = def.check(value).map_err(SettingFault)?;
            if let Some(slot) = out.iter_mut().find(|(i, _)| *i == id) {
                slot.1 = value;
            }
        }
        Ok(out)
    }
}

/// What a check of the server files found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// The game's server file changed and its `--info` was read again: retire the game's old rooms.
    Updated(String),
    /// The file changed but its `--info` failed; the old entry stays.
    Failed(String, String),
}

/// The games the hub carries.
#[derive(Clone, Debug, Default)]
pub struct Registry {
    games: Vec<GameEntry>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a registry from a parsed config, running each game's `--info`. A game that cannot be loaded is
    /// left out and named in the returned problem list.
    pub fn load(config: &Config, src: &mut dyn InfoSource) -> (Registry, Vec<String>) {
        let mut reg = Registry::new();
        let mut problems = Vec::new();
        for g in &config.games {
            match load_game(g, src) {
                Ok((entry, warnings)) => {
                    problems.extend(warnings);
                    reg.games.push(entry);
                }
                Err(e) => problems.push(format!("game {} skipped: {e}", g.id)),
            }
        }
        (reg, problems)
    }

    pub fn get(&self, id: &str) -> Option<&GameEntry> {
        self.games.iter().find(|g| g.config.id == id)
    }

    pub fn games(&self) -> &[GameEntry] {
        &self.games
    }

    pub fn add(&mut self, entry: GameEntry) {
        self.games.retain(|g| g.config.id != entry.config.id);
        self.games.push(entry);
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.games.len();
        self.games.retain(|g| g.config.id != id);
        self.games.len() != before
    }

    /// Look at each game's server file. A changed file is read (`--info`) once it has stayed the same for two
    /// checks in a row, so a binary that is still being copied is not run half written.
    pub fn poll_changes(&mut self, src: &mut dyn InfoSource) -> Vec<Change> {
        let mut changes = Vec::new();
        for g in &mut self.games {
            let Some(now) = src.key(&g.config.server) else {
                continue; // missing for the moment (an install in progress): keep the old entry
            };
            if Some(now) == g.key {
                g.pending = None;
                continue;
            }
            if g.failed == Some(now) {
                continue;
            }
            if g.pending != Some(now) {
                g.pending = Some(now);
                continue;
            }
            g.pending = None;
            match load_game(&g.config, src) {
                Ok((mut fresh, _)) => {
                    fresh.failed = None;
                    *g = fresh;
                    changes.push(Change::Updated(g.config.id.clone()));
                }
                Err(e) => {
                    g.failed = Some(now);
                    changes.push(Change::Failed(g.config.id.clone(), e));
                }
            }
        }
        changes
    }
}

fn load_game(
    config: &GameConfig,
    src: &mut dyn InfoSource,
) -> Result<(GameEntry, Vec<String>), String> {
    let (info, key) = src.info(&config.server)?;
    let mut warnings = Vec::new();
    if info.game != config.id {
        warnings.push(format!(
            "warning: [game {}] runs a server whose game is called {} (the id in the config is what clients use)",
            config.id, info.game
        ));
    }
    Ok((GameEntry::new(config.clone(), info, Some(key))?, warnings))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::viewer::netplay::cli::SettingDef;

    pub fn schema() -> Vec<SettingDef> {
        vec![
            SettingDef::parse_field("1:bots:bots:bool:0:1:0").unwrap(),
            SettingDef::parse_field("2:kills:kills:int:1:500:40").unwrap(),
            SettingDef::parse_field("3:skill:skill:int:0:2:1").unwrap(),
        ]
    }

    pub fn fake_info(game: &str, fingerprint: u32) -> Info {
        Info {
            game: game.into(),
            fingerprint,
            build: crate::viewer::netplay::cli::fold_build(fingerprint),
            max_seats: 12,
            tick_hz: 60,
            settings: schema(),
        }
    }

    pub fn game_config(id: &str) -> GameConfig {
        GameConfig {
            id: id.into(),
            server: format!("/srv/{id}-server").into(),
            public: true,
            public_name: "Public".into(),
            public_set: vec![("bots".into(), 1)],
            user_set: vec![("kills".into(), 30)],
            client_settings: None,
            max_rooms: 4,
            transport: "development".into(),
            auto_start: Some(30),
        }
    }

    pub fn entry(id: &str, fingerprint: u32) -> GameEntry {
        GameEntry::new(game_config(id), fake_info(id, fingerprint), None).unwrap()
    }

    fn parse(text: &str) -> Result<Config, ConfigError> {
        parse_config(text, Path::new("/etc/be2"))
    }

    const FULL: &str = "\
# a comment
[hub]
listen = 0.0.0.0:4100
bind_ip = 10.1.2.3
pool_start = 4101
pool_size = 16
report_dir = reports
legacy = refuse
max_rooms_per_ip = 3
max_processes = 8
rate_burst = 20
rate_per_sec = 5.5

[game deadfall]
server = /opt/deadfall-server
public = on
public_name = Public
public_set = bots=1, kills=50
user_set = kills=40
client_settings = bots, kills
max_rooms = 4
transport = development
auto_start = 30

; another comment
[game spooky-kart]
server = bin/spooky-kart-server
";

    #[test]
    fn a_full_config_parses() {
        let c = parse(FULL).unwrap();
        assert_eq!(c.hub.listen, Some("0.0.0.0:4100".parse().unwrap()));
        assert_eq!(c.hub.bind_ip, Some("10.1.2.3".parse().unwrap()));
        assert_eq!((c.hub.pool_start, c.hub.pool_size), (Some(4101), Some(16)));
        assert_eq!(c.hub.report_dir, Some("/etc/be2/reports".into()));
        assert_eq!(c.hub.legacy, Some(Mode::Refuse));
        assert_eq!(
            (c.hub.max_rooms_per_ip, c.hub.max_processes),
            (Some(3), Some(8))
        );
        assert_eq!(
            (c.hub.rate_burst, c.hub.rate_per_sec),
            (Some(20.), Some(5.5))
        );
        assert_eq!(c.games.len(), 2);
        let d = &c.games[0];
        assert_eq!(d.id, "deadfall");
        assert_eq!(d.server, PathBuf::from("/opt/deadfall-server"));
        assert!(d.public);
        assert_eq!(
            d.public_set,
            vec![("bots".to_string(), 1), ("kills".to_string(), 50)]
        );
        assert_eq!(d.user_set, vec![("kills".to_string(), 40)]);
        assert_eq!(
            d.client_settings,
            Some(vec!["bots".to_string(), "kills".to_string()])
        );
        assert_eq!(
            (d.max_rooms, d.transport.as_str(), d.auto_start),
            (4, "development", Some(30))
        );
        let k = &c.games[1];
        assert_eq!(
            k.server,
            PathBuf::from("/etc/be2/bin/spooky-kart-server"),
            "relative to the file"
        );
        assert!(
            !k.public && k.max_rooms == 4 && k.transport == "development" && k.auto_start.is_none()
        );
        assert!(k.client_settings.is_none());
    }

    #[test]
    fn the_deploy_example_parses_and_names_deadfall_and_spooky_kart() {
        let text = include_str!("../../../../deploy/hub/hub.conf.example");
        let c = parse(text).unwrap();
        let ids: Vec<_> = c.games.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(ids, ["deadfall", "spooky-kart"]);
        assert_eq!(c.games[0].public_set, vec![("bots".to_string(), 1)]);
        let s = c.hub.resolve(&HubSection::default()).unwrap();
        assert_eq!(
            (s.listen.port(), s.pool_start, s.pool_size, s.legacy),
            (4100, 4101, 16, Mode::Serve)
        );
    }

    #[test]
    fn an_empty_or_comment_only_file_is_a_valid_empty_registry() {
        assert_eq!(parse("").unwrap(), Config::default());
        assert_eq!(parse("# nothing\n\n; here\n").unwrap(), Config::default());
        assert_eq!(
            parse("\u{feff}[hub]\r\nlisten = 127.0.0.1:5000\r\n")
                .unwrap()
                .hub
                .listen
                .unwrap()
                .port(),
            5000
        );
    }

    #[test]
    fn config_errors_name_the_line() {
        let cases: &[(&str, usize, &str)] = &[
            ("listen = 1.2.3.4:5", 1, "outside any section"),
            ("[hub\nx=1", 1, "must end with ]"),
            ("[hub]\n[hub]", 2, "twice"),
            ("[nope]", 1, "unknown section"),
            ("[game]", 1, "unknown section"),
            ("[game Bad_Id]", 1, "game id"),
            ("[game a]\nserver=x\n[game a]", 3, "twice"),
            ("[game a]\nserver=x\nserver=y", 3, "given twice"),
            ("[game a]\nserver=x\npublic=maybe", 3, "on or off"),
            ("[game a]\nserver=x\nmax_rooms=25", 3, "at most 24"),
            ("[game a]\nserver=x\nmax_rooms=lots", 3, "needs a number"),
            (
                "[game a]\nserver=x\ntransport=carrier",
                3,
                "must be development",
            ),
            (
                "[game a]\nserver=x\ntransport=production",
                3,
                "not available for hub rooms",
            ),
            ("[game a]\nserver=x\nwat=1", 3, "unknown [game] key"),
            ("[game a]\nserver=x\npublic_name=<b>", 3, "valid room name"),
            ("[game a]\nserver=x\npublic_set=bots", 3, "name=value"),
            (
                "[game a]\nserver=x\npublic_set=bots=yes",
                3,
                "needs a number",
            ),
            ("[game a]\nserver=x\npublic_set=bots=1,bots=0", 3, "twice"),
            (
                "[game a]\nserver=x\nclient_settings=Bad Name",
                3,
                "not a setting name",
            ),
            ("[game a]\nserver=", 2, "needs a path"),
            ("[game a]\npublic=on", 0, "no server"),
            ("[hub]\nlisten=[::1]:4100", 2, "listen must look like"),
            ("[hub]\nlisten=nonsense", 2, "listen must look like"),
            ("[hub]\nbind_ip=localhost", 2, "IPv4"),
            ("[hub]\npool_start=0", 2, "not a port"),
            ("[hub]\npool_start=70000", 2, "not a port"),
            ("[hub]\nlegacy=sometimes", 2, "serve or refuse"),
            ("[hub]\nrate_burst=0", 2, "positive number"),
            ("[hub]\nrate_per_sec=fast", 2, "positive number"),
            ("[hub]\nwat=1", 2, "unknown [hub] key"),
            ("[hub]\njust some words", 2, "key = value"),
        ];
        for (text, line, needle) in cases {
            let e = parse(text).expect_err(text);
            assert_eq!(e.line, *line, "{text:?}: {e}");
            assert!(e.to_string().contains(needle), "{text:?}: {e}");
        }
    }

    #[test]
    fn hub_settings_resolve_with_flags_over_file_over_defaults() {
        let d = HubSection::default()
            .resolve(&HubSection::default())
            .unwrap();
        assert_eq!(d.listen, "0.0.0.0:4100".parse().unwrap());
        assert_eq!(
            (
                d.pool_start,
                d.pool_size,
                d.max_processes,
                d.max_rooms_per_ip
            ),
            (4101, 16, 16, 2)
        );
        assert_eq!(d.legacy, Mode::Serve);
        assert_eq!(d.bind_ip, Ipv4Addr::UNSPECIFIED);
        assert_eq!((d.rate_burst, d.rate_per_sec), (10., 2.));
        assert_eq!(d.ports().len(), 17);
        assert_eq!(
            (d.ports()[0], d.ports()[1], d.ports()[16]),
            (4100, 4101, 4116)
        );

        let file = parse(FULL).unwrap().hub;
        let s = file.resolve(&HubSection::default()).unwrap();
        assert_eq!(
            (s.bind_ip, s.legacy, s.max_processes),
            ("10.1.2.3".parse().unwrap(), Mode::Refuse, 8)
        );
        let over = HubSection {
            pool_size: Some(4),
            legacy: Some(Mode::Serve),
            listen: Some("127.0.0.1:9000".parse().unwrap()),
            pool_start: Some(9001),
            ..Default::default()
        };
        let s = file.resolve(&over).unwrap();
        assert_eq!(
            (s.pool_size, s.legacy, s.listen.port(), s.pool_start),
            (4, Mode::Serve, 9000, 9001)
        );
        assert_eq!(s.max_processes, 4, "never more processes than ports");
        assert_eq!(s.bind_ip, "10.1.2.3".parse::<Ipv4Addr>().unwrap());
        // Listening on a loopback address binds rooms there too, unless told otherwise.
        let lo = HubSection {
            listen: Some("127.0.0.1:9000".parse().unwrap()),
            ..Default::default()
        };
        assert_eq!(
            lo.resolve(&HubSection::default()).unwrap().bind_ip,
            Ipv4Addr::LOCALHOST
        );
    }

    #[test]
    fn bad_hub_settings_are_refused() {
        let with = |h: HubSection| h.resolve(&HubSection::default());
        assert!(with(HubSection {
            pool_size: Some(0),
            ..Default::default()
        })
        .is_err());
        assert!(with(HubSection {
            pool_size: Some(65),
            ..Default::default()
        })
        .is_err());
        assert!(with(HubSection {
            pool_start: Some(65_530),
            pool_size: Some(16),
            ..Default::default()
        })
        .is_err());
        let inside = with(HubSection {
            pool_start: Some(4090),
            pool_size: Some(16),
            ..Default::default()
        });
        assert!(inside.unwrap_err().contains("inside the room pool"));
        assert!(with(HubSection {
            listen: Some("[::1]:4100".parse().unwrap()),
            ..Default::default()
        })
        .is_err());
    }

    #[test]
    fn settings_resolve_to_ids_and_players_are_limited_to_allowed_ones() {
        let e = entry("deadfall", 0x1234);
        assert_eq!(e.public_settings, vec![(1, 1), (2, 40), (3, 1)]);
        assert_eq!(e.user_defaults, vec![(1, 0), (2, 30), (3, 1)]);
        assert_eq!(e.client_ids, vec![1, 2, 3]);
        assert_eq!(e.room_settings(&[]).unwrap(), e.user_defaults);
        assert_eq!(
            e.room_settings(&[(1, 1), (2, 99)]).unwrap(),
            vec![(1, 1), (2, 99), (3, 1)]
        );
        for bad in [
            vec![(9, 1)],
            vec![(2, 0)],
            vec![(2, 501)],
            vec![(1, 2)],
            vec![(2, 5), (2, 6)],
        ] {
            assert!(e.room_settings(&bad).is_err(), "{bad:?}");
        }
        let mut cfg = game_config("deadfall");
        cfg.client_settings = Some(vec!["bots".into()]);
        let e = GameEntry::new(cfg, fake_info("deadfall", 1), None).unwrap();
        assert_eq!(e.client_ids, vec![1]);
        assert!(
            e.room_settings(&[(2, 10)]).is_err(),
            "kills is not the players' to choose"
        );
        assert!(e.room_settings(&[(1, 1)]).is_ok());
    }

    #[test]
    fn config_naming_unknown_settings_or_bad_values_is_refused_at_load() {
        for (field, set) in [
            ("public_set", "wat=1"),
            ("user_set", "kills=0"),
            ("public_set", "bots=2"),
        ] {
            let mut cfg = game_config("deadfall");
            match field {
                "public_set" => cfg.public_set = pairs(set),
                _ => cfg.user_set = pairs(set),
            }
            assert!(
                GameEntry::new(cfg, fake_info("deadfall", 1), None).is_err(),
                "{set}"
            );
        }
        let mut cfg = game_config("deadfall");
        cfg.client_settings = Some(vec!["wat".into()]);
        assert!(GameEntry::new(cfg, fake_info("deadfall", 1), None).is_err());
    }

    fn pairs(s: &str) -> Vec<(String, u32)> {
        s.split(',')
            .map(|p| {
                let (a, b) = p.split_once('=').unwrap();
                (a.into(), b.parse().unwrap())
            })
            .collect()
    }

    // ---- a scripted InfoSource ----------------------------------------------------------------------------------------

    #[derive(Default)]
    pub struct Scripted {
        pub files: HashMap<PathBuf, (FileKey, Result<Info, String>)>,
        pub runs: usize,
    }

    impl Scripted {
        // These fixtures name fictional servers with Unix-rooted paths. Windows config
        // resolution attaches the config directory's drive to such paths; compare the
        // same rooted identity in the fake source without changing production resolution.
        fn fixture_path(path: &Path) -> PathBuf {
            #[cfg(windows)]
            if path.has_root() {
                return path
                    .components()
                    .filter(|c| !matches!(c, std::path::Component::Prefix(_)))
                    .collect();
            }
            path.to_path_buf()
        }

        pub fn put(&mut self, path: &str, size: u64, info: Result<Info, String>) {
            self.files.insert(
                Self::fixture_path(Path::new(path)),
                (FileKey { mtime: None, size }, info),
            );
        }
    }

    impl InfoSource for Scripted {
        fn key(&mut self, server: &Path) -> Option<FileKey> {
            self.files.get(&Self::fixture_path(server)).map(|(k, _)| *k)
        }
        fn info(&mut self, server: &Path) -> Result<(Info, FileKey), String> {
            self.runs += 1;
            let (k, i) = self
                .files
                .get(&Self::fixture_path(server))
                .ok_or("missing")?;
            i.clone().map(|i| (i, *k))
        }
    }

    #[test]
    fn load_skips_games_that_cannot_run_and_keeps_the_others() {
        let cfg = parse("[game a]\nserver=/srv/a\n[game b]\nserver=/srv/b\n[game c]\nserver=/srv/c\npublic_set=wat=1\n[game d]\nserver=/srv/d\n")
            .unwrap();
        let mut src = Scripted::default();
        src.put("/srv/a", 1, Ok(fake_info("a", 1)));
        src.put("/srv/b", 1, Err("--info exited with status: 1".into()));
        src.put("/srv/c", 1, Ok(fake_info("c", 1)));
        src.put("/srv/d", 1, Ok(fake_info("not-d", 1)));
        let (reg, problems) = Registry::load(&cfg, &mut src);
        assert_eq!(
            reg.games().iter().map(|g| g.id()).collect::<Vec<_>>(),
            ["a", "d"]
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("game b skipped") && p.contains("exited")),
            "{problems:?}"
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("game c skipped") && p.contains("wat")),
            "{problems:?}"
        );
        assert!(
            problems.iter().any(|p| p.contains("not-d")),
            "a name mismatch is a warning: {problems:?}"
        );
        assert!(reg.get("b").is_none() && reg.get("a").is_some());
    }

    #[test]
    fn a_changed_server_file_is_reread_once_it_has_settled() {
        let cfg = parse("[game a]\nserver=/srv/a\n").unwrap();
        let mut src = Scripted::default();
        src.put("/srv/a", 1, Ok(fake_info("a", 1)));
        let (mut reg, _) = Registry::load(&cfg, &mut src);
        assert!(reg.poll_changes(&mut src).is_empty(), "unchanged");
        let runs = src.runs;
        // The file changes: the first sighting only notes it (it may still be being written).
        src.put("/srv/a", 2, Ok(fake_info("a", 2)));
        assert!(reg.poll_changes(&mut src).is_empty());
        assert_eq!(src.runs, runs, "not run yet");
        // Still changing: the wait starts over.
        src.put("/srv/a", 3, Ok(fake_info("a", 3)));
        assert!(reg.poll_changes(&mut src).is_empty());
        assert_eq!(
            reg.poll_changes(&mut src),
            vec![Change::Updated("a".into())]
        );
        assert_eq!(reg.get("a").unwrap().info.fingerprint, 3);
        assert!(reg.poll_changes(&mut src).is_empty(), "applied once");
        // A broken new build keeps the old entry and is not retried until the file changes again.
        src.put("/srv/a", 4, Err("crashes".into()));
        assert!(reg.poll_changes(&mut src).is_empty());
        let fail = reg.poll_changes(&mut src);
        assert!(
            matches!(&fail[..], [Change::Failed(g, e)] if g == "a" && e == "crashes"),
            "{fail:?}"
        );
        assert_eq!(reg.get("a").unwrap().info.fingerprint, 3);
        let runs = src.runs;
        assert!(reg.poll_changes(&mut src).is_empty() && reg.poll_changes(&mut src).is_empty());
        assert_eq!(src.runs, runs, "no retry storm");
        // A missing file (an install in progress) changes nothing.
        src.files.remove(Path::new("/srv/a"));
        assert!(reg.poll_changes(&mut src).is_empty());
    }

    #[test]
    fn add_and_remove() {
        let mut reg = Registry::new();
        reg.add(entry("a", 1));
        reg.add(entry("a", 2));
        assert_eq!(reg.games().len(), 1);
        assert_eq!(reg.get("a").unwrap().info.fingerprint, 2);
        assert!(reg.remove("a") && !reg.remove("a"));
    }

    // ---- real processes -----------------------------------------------------------------------------------------------

    #[cfg(unix)]
    mod unix {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        fn temp(name: &str) -> PathBuf {
            let dir =
                std::env::temp_dir().join(format!("be2hub-reg-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            dir
        }

        fn script(dir: &Path, body: &str) -> PathBuf {
            let path = dir.join("server");
            let tmp = dir.join("server.tmp");
            std::fs::write(&tmp, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::rename(&tmp, &path).unwrap();
            path
        }

        const INFO: &str = "echo game=s; echo fingerprint=0000000a; echo build=0000000b; echo max_seats=4; echo tick_hz=30; echo setting=1:bots:bots:bool:0:1:0";

        #[test]
        fn info_is_run_once_per_file_version_and_failures_are_reported() {
            let dir = temp("cache");
            let counter = dir.join("runs");
            let path = script(
                &dir,
                &format!(
                    "echo x >> {}\n[ \"$1\" = --info ] || exit 3\n{INFO}",
                    counter.display()
                ),
            );
            let mut src = ProcessInfo::new();
            let (info, key) = src.info(&path).unwrap();
            assert_eq!(
                (
                    info.game.as_str(),
                    info.fingerprint,
                    info.build,
                    info.max_seats
                ),
                ("s", 10, 11, 4)
            );
            assert_eq!(info.settings.len(), 1);
            assert_eq!(src.key(&path), Some(key));
            src.info(&path).unwrap();
            src.info(&path).unwrap();
            assert_eq!(
                (
                    src.runs,
                    std::fs::read_to_string(&counter).unwrap().lines().count()
                ),
                (1, 1),
                "cached"
            );
            // A different file (size changes) is run again.
            let path = script(
                &dir,
                &format!(
                    "echo y >> {}\n{INFO}\n# padding to change the size",
                    counter.display()
                ),
            );
            src.info(&path).unwrap();
            assert_eq!(src.runs, 2);

            let bad = script(&dir, "exit 4");
            assert!(src.info(&bad).unwrap_err().contains("exited"));
            let garbage = script(&dir, "echo hello");
            assert!(src.info(&garbage).unwrap_err().contains("no game= line"));
            assert!(src
                .info(&dir.join("missing"))
                .unwrap_err()
                .contains("does not exist"));
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn a_server_that_hangs_in_info_is_given_up_on() {
            let dir = temp("hang");
            let path = script(&dir, "exec sleep 30");
            let started = Instant::now();
            assert!(ProcessInfo::run(&path)
                .unwrap_err()
                .contains("did not finish"));
            assert!(started.elapsed() < Duration::from_secs(15));
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn a_server_that_floods_or_leaks_its_info_output_is_given_up_on() {
            let dir = temp("flood");
            let started = Instant::now();
            let path = script(&dir, "exec yes game=flood");
            let e = ProcessInfo::run_with(&path, Duration::from_secs(30)).unwrap_err();
            assert!(e.contains("more than"), "{e}");
            assert!(started.elapsed() < Duration::from_secs(10), "stopped early");
            // A child that outlives --info and keeps the pipe open must not make the caller wait for it.
            let path = script(&dir, "(sleep 4 &)\necho game=leak");
            let e = ProcessInfo::run_with(&path, Duration::from_secs(30)).unwrap_err();
            assert!(e.contains("left its output open"), "{e}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
