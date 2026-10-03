//! The front door: abuse limits and the datagram handler in front of the room table, and the UDP loop.
//!
//! [`Hub::handle`] takes one datagram and the time and returns the reply, if any; it has no socket and no clock
//! of its own, so tests drive it with fakes. [`serve`] is the loop around a real socket.
//!
//! Three protocols share the one port, told apart by the first four bytes: `BEHB` (the hub protocol, [`wire`]),
//! `DFHB` (the Deadfall hub protocol of already-shipped clients, [`legacy`]) and `BECT` (loopback control).
//! Anything else gets silence.
use super::deploy::GameStatus;
use super::legacy::{self, Mode};
use super::limits::{Bucket, Limits, RateLimiter};
use super::registry::{Change, Config, GameEntry, InfoSource, Registry};
use super::rooms::{HubError, ManagerConfig, RoomManager};
use super::spawn::Spawner;
use super::wire::{
    max_reply_len, min_request_len, Control, ControlReply, ErrorCode, Reply, Request, RoomInfo,
    CONTROL_MAGIC, MAGIC, MAX_REPLY, MAX_REQUEST,
};
use super::{log, wire};
use std::collections::hash_map::RandomState;
use std::collections::VecDeque;
use std::hash::BuildHasher;
use std::io;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// A cookie is good for this long (it is valid in its own window and the next).
pub const COOKIE_WINDOW_MS: u64 = 60_000;
/// How often the server files are checked for a new build.
pub const BINARY_CHECK_MS: u64 = 10_000;

/// Everything a hub needs besides its registry and spawner.
#[derive(Clone, Debug)]
pub struct HubOptions {
    pub manager: ManagerConfig,
    pub limits: Limits,
    pub legacy: Mode,
    /// The config file, re-read for `reload <game>`.
    pub config_path: Option<PathBuf>,
}

impl Default for HubOptions {
    fn default() -> Self {
        Self {
            manager: ManagerConfig::default(),
            limits: Limits::default(),
            legacy: Mode::Serve,
            config_path: None,
        }
    }
}

/// The datagram handler. No sockets and no clock of its own.
pub struct Hub {
    registry: Registry,
    manager: RoomManager,
    info_source: Box<dyn InfoSource>,
    per_source: RateLimiter,
    create_limit: RateLimiter,
    legacy_create: Bucket,
    global: Bucket,
    limits: Limits,
    legacy: Mode,
    config_path: Option<PathBuf>,
    /// The cookie key: random per process, never sent anywhere.
    cookie_key: RandomState,
    /// Recent creates by (source, nonce): a client that repeats a create because the reply was lost gets the same
    /// room back instead of "name taken".
    recent_creates: VecDeque<(IpAddr, u32, String, String)>,
    last_binary_check_ms: u64,
    /// The last reload (source, nonce) and its encoded reply: `be2-hub` resends a request it has had no answer to
    /// yet, and a slow reload must not run twice (and retire the room it just started). Status is a read; it is
    /// always answered afresh.
    last_control: Option<(SocketAddr, u32, Vec<u8>)>,
}

impl Hub {
    pub fn new(
        opts: HubOptions,
        registry: Registry,
        spawner: Box<dyn Spawner>,
        info_source: Box<dyn InfoSource>,
    ) -> Self {
        let limits = opts.limits;
        let mut manager_cfg = opts.manager;
        manager_cfg.max_processes = manager_cfg.max_processes.min(limits.max_processes.max(1));
        manager_cfg.max_rooms_per_ip = limits.max_rooms_per_ip;
        let mut hub = Self {
            registry,
            manager: RoomManager::new(manager_cfg, spawner),
            info_source,
            per_source: RateLimiter::new(limits.burst, limits.per_sec, limits.max_sources),
            create_limit: RateLimiter::new(
                limits.create_burst,
                limits.create_per_sec,
                limits.max_sources,
            ),
            legacy_create: Bucket::new(limits.legacy_create_burst),
            global: Bucket::new(limits.global_burst),
            limits,
            legacy: opts.legacy,
            config_path: opts.config_path,
            cookie_key: RandomState::new(),
            recent_creates: VecDeque::new(),
            last_binary_check_ms: 0,
            last_control: None,
        };
        hub.manager.tick(0, &hub.registry);
        hub
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn manager(&self) -> &RoomManager {
        &self.manager
    }

    /// Housekeeping: the room table, and now and then a look at the servers' files. Call a few times a second.
    pub fn tick(&mut self, now_ms: u64) {
        self.manager.tick(now_ms, &self.registry);
        if now_ms.saturating_sub(self.last_binary_check_ms) >= BINARY_CHECK_MS {
            self.last_binary_check_ms = now_ms;
            for change in self.registry.poll_changes(self.info_source.as_mut()) {
                match change {
                    Change::Updated(game) => {
                        // New Public room first, as `reload` does; if it cannot start, retire the old rooms anyway
                        // (the registry already holds the new build, so listing the old rooms would mislead).
                        let n = match self.registry.get(&game).cloned() {
                            Some(entry) => self
                                .manager
                                .replace_game(&entry, now_ms)
                                .unwrap_or_else(|why| {
                                    log(&format!("{why}; retiring the old rooms instead"));
                                    self.manager.retire_game(&game, now_ms)
                                }),
                            None => 0,
                        };
                        log(&format!(
                            "The {game} server changed on disk: retired {n} room(s); new rooms use the new build"
                        ));
                    }
                    Change::Failed(game, e) => log(&format!(
                        "The {game} server changed on disk but its --info failed ({e}); keeping the old build"
                    )),
                }
                // Start the replacement Public room now rather than at the next tick.
                self.manager.tick(now_ms, &self.registry);
            }
        }
    }

    pub fn shutdown(&mut self) {
        self.manager.shutdown();
    }

    /// The token a Create from `ip` must carry: a keyed hash of the address and a coarse time window.
    fn cookie(&self, ip: IpAddr, window: u64) -> u64 {
        self.cookie_key.hash_one((ip, window))
    }

    fn cookie_ok(&self, ip: IpAddr, token: u64, now_ms: u64) -> bool {
        let w = now_ms / COOKIE_WINDOW_MS;
        token == self.cookie(ip, w) || (w > 0 && token == self.cookie(ip, w - 1))
    }

    /// Answer one datagram, or `None` to stay silent (garbage, unpadded requests, and anyone over their rate:
    /// silence costs the hub nothing and gives a flooder nothing).
    pub fn handle(&mut self, src: SocketAddr, data: &[u8], now_ms: u64) -> Option<Vec<u8>> {
        if data.len() > MAX_REQUEST || data.len() < 4 {
            return None;
        }
        let magic = [data[0], data[1], data[2], data[3]];
        match magic {
            MAGIC => self.handle_behb(src, data, now_ms),
            legacy::MAGIC => self.handle_legacy(src, data, now_ms),
            CONTROL_MAGIC => self.handle_control(src, data, now_ms),
            _ => None,
        }
    }

    fn admit(&mut self, src: SocketAddr, now_ms: u64) -> bool {
        let g = &self.limits;
        self.global
            .take(now_ms, g.global_burst, g.global_per_sec, 1.)
            && self.per_source.allow(src.ip(), now_ms)
    }

    fn rooms_reply(&self, game: &str, skip: u8) -> Reply {
        let all = self.manager.rooms_of(game);
        let mut size = wire::REPLY_HEADER + 1 + game.len() + 3;
        let mut rooms = Vec::new();
        for r in all.iter().skip(skip as usize) {
            size += Reply::room_len(r);
            if size > MAX_REPLY {
                break;
            }
            rooms.push(r.clone());
        }
        Reply::Rooms {
            game: game.to_string(),
            skip,
            total: all.len().min(255) as u8,
            rooms,
        }
    }

    // ---- BEHB --------------------------------------------------------------------------------------------------------

    fn handle_behb(&mut self, src: SocketAddr, data: &[u8], now_ms: u64) -> Option<Vec<u8>> {
        let (nonce, request) = Request::decode(data)?;
        if data.len() < min_request_len(&request) {
            return None;
        }
        if !self.admit(src, now_ms) {
            return None;
        }
        let max_len = max_reply_len(&request);
        let (reply, build) = self.answer(src, nonce, request, now_ms);
        let bytes = reply.encode_within(nonce, build, max_len);
        debug_assert!(bytes.len() <= max_len, "reply {} bytes", bytes.len());
        Some(bytes)
    }

    fn answer(
        &mut self,
        src: SocketAddr,
        nonce: u32,
        request: Request,
        now_ms: u64,
    ) -> (Reply, u32) {
        let unknown = || {
            (
                Reply::Error {
                    code: ErrorCode::UnknownGame,
                    text: "Unknown game.".into(),
                },
                0,
            )
        };
        match request {
            Request::Cookie => (
                Reply::Cookie {
                    token: self.cookie(src.ip(), now_ms / COOKIE_WINDOW_MS),
                },
                0,
            ),
            Request::Ping { game } => match self.registry.get(&game) {
                Some(g) => (Reply::Pong, g.info.build),
                None => unknown(),
            },
            Request::List { game, skip } => match self.registry.get(&game) {
                Some(g) => (self.rooms_reply(&game, skip), g.info.build),
                None => unknown(),
            },
            Request::Create {
                game,
                name,
                settings,
                cookie,
            } => {
                let Some(entry) = self.registry.get(&game) else {
                    return unknown();
                };
                let build = entry.info.build;
                let err = |code, text: &str| {
                    (
                        Reply::Error {
                            code,
                            text: text.into(),
                        },
                        build,
                    )
                };
                if !self.cookie_ok(src.ip(), cookie, now_ms) {
                    return err(ErrorCode::BadCookie, "Ask again: the request expired.");
                }
                if let Some(room) = self.repeated_create(src.ip(), nonce, &game) {
                    return (Reply::Created { room }, build);
                }
                if !self.create_limit.allow(src.ip(), now_ms) {
                    return err(
                        ErrorCode::RateLimited,
                        "Too many rooms made too fast. Wait a little.",
                    );
                }
                let entry = entry.clone();
                let settings = match entry.room_settings(&settings) {
                    Ok(s) => s,
                    Err(e) => return err(ErrorCode::BadSetting, &e.0),
                };
                match self.make_room(&entry, &name, settings, src.ip(), nonce, now_ms) {
                    Ok(room) => (Reply::Created { room }, build),
                    Err(e) => (
                        Reply::Error {
                            code: e.code,
                            text: e.text,
                        },
                        build,
                    ),
                }
            }
        }
    }

    /// The room an earlier create from this source with this nonce made, if it still exists.
    fn repeated_create(&self, ip: IpAddr, nonce: u32, game: &str) -> Option<RoomInfo> {
        let (_, _, g, made) = self
            .recent_creates
            .iter()
            .find(|(i, n, g, _)| *i == ip && *n == nonce && g == game)?;
        self.manager
            .rooms_of(g)
            .into_iter()
            .find(|r| &r.name == made)
    }

    fn make_room(
        &mut self,
        entry: &GameEntry,
        name: &str,
        settings: Vec<(u8, u32)>,
        ip: IpAddr,
        nonce: u32,
        now_ms: u64,
    ) -> Result<RoomInfo, HubError> {
        let room = self.manager.create(entry, name, settings, ip, now_ms)?;
        log(&format!(
            "{ip} created {} room \"{}\" on port {}",
            entry.id(),
            room.name,
            room.port
        ));
        self.recent_creates
            .push_back((ip, nonce, entry.id().to_string(), room.name.clone()));
        if self.recent_creates.len() > 64 {
            self.recent_creates.pop_front();
        }
        Ok(room)
    }

    // ---- DFHB --------------------------------------------------------------------------------------------------------

    fn handle_legacy(&mut self, src: SocketAddr, data: &[u8], now_ms: u64) -> Option<Vec<u8>> {
        let (nonce, request) = legacy::Request::decode(data)?;
        if data.len() < legacy::min_request_len(&request) {
            return None;
        }
        // The old protocol's build is the game's raw fingerprint; without a Deadfall registered there is nothing
        // an old client could be told.
        let fingerprint = self.registry.get(legacy::GAME)?.info.fingerprint;
        if !self.admit(src, now_ms) {
            return None;
        }
        let max_len = legacy::max_reply_len(&request);
        let (reply, build) = if self.legacy == Mode::Refuse {
            let build = legacy::refusing_build(fingerprint);
            match request {
                legacy::Request::Ping => (legacy::Reply::Pong, build),
                legacy::Request::List { skip } => (
                    legacy::Reply::Rooms {
                        skip,
                        total: 0,
                        rooms: Vec::new(),
                    },
                    build,
                ),
                legacy::Request::Create { .. } => (
                    legacy::Reply::Error {
                        code: ErrorCode::BadRequest,
                        text: "Update the game.".into(),
                    },
                    build,
                ),
            }
        } else {
            (
                self.legacy_answer(src, nonce, request, now_ms)?,
                fingerprint,
            )
        };
        let bytes = reply.encode(nonce, build);
        debug_assert!(bytes.len() <= max_len, "reply {} bytes", bytes.len());
        Some(bytes)
    }

    fn legacy_answer(
        &mut self,
        src: SocketAddr,
        nonce: u32,
        request: legacy::Request,
        now_ms: u64,
    ) -> Option<legacy::Reply> {
        Some(match request {
            legacy::Request::Ping => legacy::Reply::Pong,
            legacy::Request::List { skip } => {
                legacy::rooms_reply(&self.manager.rooms_of(legacy::GAME), skip)
            }
            legacy::Request::Create { name, bots, kills } => {
                if let Some(room) = self.repeated_create(src.ip(), nonce, legacy::GAME) {
                    return Some(legacy::Reply::Created { room });
                }
                // No cookie on this protocol: the source may be forged, so creating is cut off globally as well.
                let l = &self.limits;
                if !self.legacy_create.take(
                    now_ms,
                    l.legacy_create_burst,
                    l.legacy_create_per_sec,
                    1.,
                ) || !self.create_limit.allow(src.ip(), now_ms)
                {
                    return None;
                }
                let entry = self.registry.get(legacy::GAME)?.clone();
                let settings = entry
                    .room_settings(&legacy::map_create_settings(entry.schema(), bots, kills))
                    .ok()?;
                match self.make_room(&entry, &name, settings, src.ip(), nonce, now_ms) {
                    Ok(room) => legacy::Reply::Created { room },
                    Err(e) => legacy::Reply::Error {
                        code: e.code,
                        text: e.text,
                    },
                }
            }
        })
    }

    // ---- control -----------------------------------------------------------------------------------------------------

    fn handle_control(&mut self, src: SocketAddr, data: &[u8], now_ms: u64) -> Option<Vec<u8>> {
        if !src.ip().is_loopback() {
            return None;
        }
        let (nonce, control) = Control::decode(data)?;
        let is_reload = matches!(control, Control::Reload { .. });
        if let Some((from, seen, reply)) = self.last_control.as_ref().filter(|_| is_reload) {
            if *from == src && *seen == nonce {
                return Some(reply.clone()); // a resend of a reload already done
            }
        }
        let reply = match control {
            Control::Reload { game } => {
                let reply = match self.reload(&game, now_ms) {
                    Ok(text) => ControlReply { ok: true, text },
                    Err(text) => ControlReply { ok: false, text },
                };
                log(&format!("reload {game}: {}", reply.text));
                reply
            }
            Control::Status { game } => match self.game_status(&game) {
                Ok(status) => ControlReply {
                    ok: true,
                    text: status.to_text(),
                },
                Err(text) => ControlReply { ok: false, text },
            },
        };
        let bytes = reply.encode(nonce);
        if is_reload {
            self.last_control = Some((src, nonce, bytes.clone()));
        }
        Some(bytes)
    }

    /// `be2-hub status <game>`: what the hub holds for a game right now. Changes nothing.
    pub fn game_status(&self, game: &str) -> Result<GameStatus, String> {
        let entry = self
            .registry
            .get(game)
            .ok_or_else(|| format!("{game} is not loaded in the running hub"))?;
        Ok(GameStatus::of(
            game,
            entry.info.build,
            entry.config.public,
            self.manager.public_room(game),
            self.manager.retired_of(game),
            self.manager.rooms_of(game).len(),
        ))
    }

    /// `be2-hub reload <game>`: re-read the config file for this game, run its server's `--info` again, and
    /// retire its rooms (see [`rooms`](super::rooms)). A broken new build or config changes nothing.
    pub fn reload(&mut self, game: &str, now_ms: u64) -> Result<String, String> {
        let text = match &self.config_path {
            Some(path) => std::fs::read_to_string(path)
                .map_err(|e| format!("cannot read {}: {e}", path.display()))?,
            None => String::new(),
        };
        let base = self
            .config_path
            .as_deref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        let config: Config = if self.config_path.is_some() {
            super::registry::parse_config(&text, &base).map_err(|e| format!("config: {e}"))?
        } else {
            Config::default()
        };
        match config.games.iter().find(|g| g.id == game) {
            Some(g) => {
                let one = Config {
                    hub: Default::default(),
                    games: vec![g.clone()],
                };
                let (fresh, problems) = Registry::load(&one, self.info_source.as_mut());
                let Some(entry) = fresh.get(game).cloned() else {
                    return Err(problems.join("; "));
                };
                // The replacement Public room starts before anything is retired: if it cannot, nothing changes.
                let n = self.manager.replace_game(&entry, now_ms)?;
                let build = entry.info.build;
                self.registry.add(entry);
                self.manager.tick(now_ms, &self.registry);
                let public = match self.manager.public_room(game) {
                    Some(r) => format!("Public room on port {}, no status yet", r.port),
                    None if self.registry.get(game).is_some_and(|g| g.config.public) => {
                        "Public room not running yet".to_string()
                    }
                    None => "no Public room".to_string(),
                };
                Ok(format!(
                    "{game} reloaded (build {build:08x}): {n} room(s) retired; {public}. Not proof of readiness: ask `be2-hub status {game}`"
                ))
            }
            None if self.registry.get(game).is_some() => {
                self.registry.remove(game);
                let n = self.manager.retire_game(game, now_ms);
                Ok(format!(
                    "{game} removed from the config: {n} room(s) retired"
                ))
            }
            None => Err(format!("{game} is not in the config")),
        }
    }
}

/// The hub's UDP loop: answer datagrams on `socket` and tick the hub until `stop` is set, then kill every child.
pub fn serve(socket: &UdpSocket, hub: &mut Hub, stop: &AtomicBool) -> io::Result<()> {
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;
    let start = Instant::now();
    let mut buf = [0u8; 2048];
    let mut last_tick = 0u64;
    while !stop.load(Ordering::Relaxed) {
        let now_ms = start.elapsed().as_millis() as u64;
        match socket.recv_from(&mut buf) {
            Ok((n, src)) => {
                if let Some(reply) = hub.handle(src, &buf[..n], now_ms) {
                    let _ = socket.send_to(&reply, src);
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                        | io::ErrorKind::ConnectionReset
                ) => {}
            Err(e) => {
                hub.shutdown();
                return Err(e);
            }
        }
        if now_ms.saturating_sub(last_tick) >= 250 {
            last_tick = now_ms;
            hub.tick(now_ms);
        }
    }
    log("Shutting down: stopping every room");
    hub.shutdown();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::registry::tests::{entry, fake_info, game_config, Scripted};
    use super::super::rooms::tests::{status, Fake, World};
    use super::super::wire::RoomState;
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn src(n: u8) -> SocketAddr {
        SocketAddr::from(([10, 0, 0, n], 5000))
    }

    fn registry() -> Registry {
        let mut r = Registry::new();
        r.add(entry("deadfall", 0x1111_2222));
        r.add(entry("kart", 0x3333_4444));
        r
    }

    fn opts() -> HubOptions {
        HubOptions {
            manager: ManagerConfig {
                pool_start: 6000,
                pool_size: 12,
                max_processes: 12,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn hub_with(opts: HubOptions) -> (Hub, Rc<RefCell<World>>) {
        let w = Rc::new(RefCell::new(World::default()));
        let h = Hub::new(
            opts,
            registry(),
            Box::new(Fake(w.clone())),
            Box::new(Scripted::default()),
        );
        (h, w)
    }

    fn hub() -> (Hub, Rc<RefCell<World>>) {
        hub_with(opts())
    }

    fn big() -> HubOptions {
        let mut o = opts();
        o.limits = Limits {
            burst: 1e6,
            create_burst: 1e6,
            max_rooms_per_ip: 100,
            max_processes: 100,
            ..Default::default()
        };
        o.manager.pool_size = 64;
        o.manager.max_processes = 100;
        o
    }

    /// Every ask is a new request with its own nonce (a repeated nonce is a retry, which the hub answers differently).
    fn next_nonce() -> u32 {
        static NONCE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1000);
        NONCE.fetch_add(1, Ordering::Relaxed)
    }

    fn ask(h: &mut Hub, from: u8, r: &Request, now: u64) -> Option<wire::ReplyPacket> {
        let bytes = h.handle(src(from), &r.encode(next_nonce()), now)?;
        assert!(
            bytes.len() <= max_reply_len(r),
            "{} bytes for {r:?}",
            bytes.len()
        );
        Some(Reply::decode(&bytes).expect("a reply we can decode"))
    }

    fn cookie(h: &mut Hub, from: u8, now: u64) -> u64 {
        match ask(h, from, &Request::Cookie, now).unwrap().reply {
            Reply::Cookie { token } => token,
            other => panic!("{other:?}"),
        }
    }

    fn create(game: &str, name: &str, cookie: u64) -> Request {
        Request::Create {
            game: game.into(),
            name: name.into(),
            settings: vec![],
            cookie,
        }
    }

    fn list(game: &str) -> Request {
        Request::List {
            game: game.into(),
            skip: 0,
        }
    }

    fn created(h: &mut Hub, from: u8, game: &str, name: &str, now: u64) -> Reply {
        let c = cookie(h, from, now);
        ask(h, from, &create(game, name, c), now).unwrap().reply
    }

    #[test]
    fn the_hub_lists_creates_pings_and_stamps_replies_with_the_games_build() {
        let (mut h, _) = hub();
        let build = h.registry().get("kart").unwrap().info.build;
        assert_eq!(build, crate::viewer::netplay::cli::fold_build(0x3333_4444));
        let p = ask(
            &mut h,
            1,
            &Request::Ping {
                game: "kart".into(),
            },
            0,
        )
        .unwrap();
        assert_eq!((p.reply, p.build), (Reply::Pong, build));
        let p = ask(&mut h, 1, &list("kart"), 0).unwrap();
        let Reply::Rooms {
            game,
            total,
            rooms,
            skip,
        } = p.reply
        else {
            panic!()
        };
        assert_eq!((game.as_str(), total, skip, p.build), ("kart", 1, 0, build));
        assert!(rooms[0].public && rooms[0].name == "Public");
        let Reply::Created { room } = created(&mut h, 1, "kart", "Mine", 1) else {
            panic!()
        };
        assert_eq!(room.name, "Mine");
        let Reply::Rooms { total, .. } = ask(&mut h, 2, &list("kart"), 2).unwrap().reply else {
            panic!()
        };
        assert_eq!(total, 2);
        let Reply::Rooms { total, .. } = ask(&mut h, 2, &list("deadfall"), 2).unwrap().reply else {
            panic!()
        };
        assert_eq!(total, 1, "rooms are per game");
    }

    #[test]
    fn an_unknown_game_gets_a_short_error_never_more_than_it_sent() {
        let (mut h, _) = hub();
        for r in [
            Request::Ping {
                game: "nope".into(),
            },
            list("nope"),
            create("nope", "x", 0),
        ] {
            let bytes = h.handle(src(1), &r.encode(1), 0).unwrap();
            assert!(bytes.len() <= max_reply_len(&r) && bytes.len() < r.encode(1).len());
            let p = Reply::decode(&bytes).unwrap();
            assert!(
                matches!(
                    p.reply,
                    Reply::Error {
                        code: ErrorCode::UnknownGame,
                        ..
                    }
                ),
                "{r:?}"
            );
            assert_eq!(p.build, 0);
        }
    }

    #[test]
    fn a_create_needs_a_cookie_that_belongs_to_the_senders_address() {
        let (mut h, w) = hub();
        let good = cookie(&mut h, 1, 0);
        // No cookie, a made-up one, and somebody else's: all refused, and no process started.
        for (from, token) in [(1, 0), (1, good ^ 1), (2, good)] {
            let p = ask(&mut h, from, &create("kart", "Spoof", token), 1).unwrap();
            assert!(
                matches!(
                    p.reply,
                    Reply::Error {
                        code: ErrorCode::BadCookie,
                        ..
                    }
                ),
                "{from} {token}"
            );
        }
        assert!(
            w.borrow().spawned.iter().all(|s| s.public),
            "nothing but the Public rooms"
        );
        assert!(matches!(
            ask(&mut h, 1, &create("kart", "Real", good), 1)
                .unwrap()
                .reply,
            Reply::Created { .. }
        ));
        // A cookie is good for its window and the next, then expires.
        let (mut h, _) = hub();
        let c = cookie(&mut h, 1, 10_000);
        let in_next = ask(&mut h, 1, &create("kart", "A", c), COOKIE_WINDOW_MS + 1).unwrap();
        assert!(
            matches!(in_next.reply, Reply::Created { .. }),
            "{in_next:?}"
        );
        let late = ask(&mut h, 1, &create("kart", "B", c), 2 * COOKIE_WINDOW_MS + 1).unwrap();
        assert!(matches!(
            late.reply,
            Reply::Error {
                code: ErrorCode::BadCookie,
                ..
            }
        ));
        // Cookies differ per address and per window, and each hub has its own key.
        assert_ne!(cookie(&mut h, 1, 0), cookie(&mut h, 2, 0));
        assert_ne!(cookie(&mut h, 1, 0), cookie(&mut h, 1, COOKIE_WINDOW_MS));
        let (mut other, _) = hub();
        assert_ne!(cookie(&mut h, 1, 0), cookie(&mut other, 1, 0));
    }

    #[test]
    fn creating_applies_validated_settings_and_refuses_the_rest() {
        let (mut h, w) = hub_with(big());
        let c = cookie(&mut h, 1, 0);
        let with = |settings: Vec<(u8, u32)>, name: &str| Request::Create {
            game: "deadfall".into(),
            name: name.into(),
            settings,
            cookie: c,
        };
        let p = ask(&mut h, 1, &with(vec![(1, 1), (2, 25)], "Ok"), 0).unwrap();
        assert!(matches!(p.reply, Reply::Created { .. }));
        let spawned = w.borrow().spawned.last().unwrap().clone();
        assert_eq!(spawned.settings, vec![(1, 1), (2, 25), (3, 1)]);
        assert_eq!(spawned.auto_start, Some(30));
        for (i, bad) in [
            vec![(9, 1)],
            vec![(2, 0)],
            vec![(2, 501)],
            vec![(1, 5)],
            vec![(2, 5), (2, 6)],
        ]
        .into_iter()
        .enumerate()
        {
            let p = ask(&mut h, 1, &with(bad.clone(), &format!("Bad{i}")), 0).unwrap();
            assert!(
                matches!(
                    p.reply,
                    Reply::Error {
                        code: ErrorCode::BadSetting,
                        ..
                    }
                ),
                "{bad:?}"
            );
        }
        let before = w.borrow().spawned.len();
        let _ = ask(&mut h, 1, &with(vec![(9, 1)], "Again"), 0);
        assert_eq!(
            w.borrow().spawned.len(),
            before,
            "refused settings never reach a process"
        );
        // Defaults from the registry apply when the player chooses nothing.
        let p = ask(&mut h, 1, &with(vec![], "Plain"), 0).unwrap();
        assert!(matches!(p.reply, Reply::Created { .. }));
        assert_eq!(
            w.borrow().spawned.last().unwrap().settings,
            vec![(1, 0), (2, 30), (3, 1)]
        );
    }

    #[test]
    fn a_repeated_create_with_the_same_nonce_returns_the_same_room_instead_of_name_taken() {
        let (mut h, _) = hub();
        let c = cookie(&mut h, 1, 0);
        let req = create("kart", "Same", c);
        let a = h.handle(src(1), &req.encode(77), 0).unwrap();
        let b = h.handle(src(1), &req.encode(77), 1).unwrap();
        let (Reply::Created { room: ra }, Reply::Created { room: rb }) = (
            Reply::decode(&a).unwrap().reply,
            Reply::decode(&b).unwrap().reply,
        ) else {
            panic!()
        };
        assert_eq!(ra, rb);
        // A different nonce is a different request.
        let c2 = h.handle(src(1), &req.encode(78), 2).unwrap();
        assert!(matches!(
            Reply::decode(&c2).unwrap().reply,
            Reply::Error {
                code: ErrorCode::NameTaken,
                ..
            }
        ));
        // The same nonce for another game is not a repeat.
        let other = create("deadfall", "Same", c);
        let d = h.handle(src(1), &other.encode(77), 3).unwrap();
        assert!(matches!(
            Reply::decode(&d).unwrap().reply,
            Reply::Created { .. }
        ));
    }

    #[test]
    fn garbage_short_unpadded_and_oversized_datagrams_get_no_reply() {
        let (mut h, _) = hub();
        let fine = list("kart").encode(1);
        assert!(h.handle(src(1), &fine, 0).is_some());
        assert!(h.handle(src(1), b"", 0).is_none());
        assert!(h.handle(src(1), b"hi", 0).is_none());
        assert!(h.handle(src(1), b"GET / HTTP/1.1\r\n\r\n", 0).is_none());
        assert!(h.handle(src(1), &[0u8; 100], 0).is_none());
        assert!(
            h.handle(src(1), &fine[..199], 0).is_none(),
            "a list shorter than its minimum"
        );
        assert!(h
            .handle(src(1), &Request::Cookie.encode(1)[..31], 0)
            .is_none());
        let mut big = fine.clone();
        big.resize(MAX_REQUEST + 1, 0);
        assert!(h.handle(src(1), &big, 0).is_none(), "oversized");
        let mut bad_version = fine.clone();
        bad_version[4] = 9;
        assert!(h.handle(src(1), &bad_version, 0).is_none());
        let mut bad_kind = fine.clone();
        bad_kind[5] = 77;
        assert!(h.handle(src(1), &bad_kind, 0).is_none());
        let mut bad_game = fine.clone();
        bad_game[10] = 200;
        assert!(h.handle(src(1), &bad_game, 0).is_none());
        let mut create = create("kart", "abcdef", 5).encode(1);
        create.truncate(40);
        assert!(h.handle(src(1), &create, 0).is_none());
        // A control datagram from a non-loopback address is silence too.
        assert!(h
            .handle(
                src(1),
                &Control::Reload {
                    game: "kart".into()
                }
                .encode(1),
                0
            )
            .is_none());
        // And a reply-shaped datagram (a reflection attempt) is not a request.
        let reply = Reply::Pong.encode(1, 1);
        assert!(h.handle(src(1), &reply, 0).is_none());
    }

    #[test]
    fn a_flood_gets_its_burst_then_silence_and_other_sources_are_unaffected() {
        let (mut h, _) = hub();
        let answered = (0..100)
            .filter(|_| ask(&mut h, 1, &list("kart"), 0).is_some())
            .count();
        assert_eq!(answered, 10, "the burst, then silence");
        assert!(ask(&mut h, 2, &list("kart"), 0).is_some());
        assert!(ask(&mut h, 1, &list("kart"), 1000).is_some(), "it recovers");
        // Unknown games and cookie requests cost the same bucket.
        let (mut h, _) = hub();
        let answered = (0..100)
            .filter(|_| ask(&mut h, 1, &Request::Cookie, 0).is_some())
            .count();
        assert_eq!(answered, 10);
    }

    #[test]
    fn creating_rooms_has_its_own_slow_limit_and_a_global_limit_caps_everyone() {
        let mut o = big();
        o.limits.burst = 1000.;
        o.limits.create_burst = 3.;
        let (mut h, _) = hub_with(o);
        let name = |i: u32| format!("R{i}");
        for i in 0..3 {
            assert!(matches!(
                created(&mut h, 1, "kart", &name(i), i as u64),
                Reply::Created { .. }
            ));
        }
        let c = cookie(&mut h, 1, 3);
        let p = ask(&mut h, 1, &create("kart", "R9", c), 3).unwrap();
        assert!(
            matches!(
                p.reply,
                Reply::Error {
                    code: ErrorCode::RateLimited,
                    ..
                }
            ),
            "a fourth create in a few seconds is refused with a reason (the source is proven real)"
        );
        assert!(
            ask(
                &mut h,
                1,
                &Request::Ping {
                    game: "kart".into()
                },
                3
            )
            .is_some(),
            "but listing still works"
        );
        let (mut h, _) = hub();
        let answered = (0..2000u32)
            .filter(|i| {
                h.handle(
                    SocketAddr::from(([10, (i >> 8) as u8, *i as u8, 1], 1)),
                    &Request::Cookie.encode(1),
                    0,
                )
                .is_some()
            })
            .count();
        assert_eq!(
            answered, 300,
            "spoofed sources cannot make the hub send more than the global burst"
        );
    }

    #[test]
    fn the_per_creator_cap_applies_across_games() {
        let (mut h, _) = hub_with(opts());
        assert!(matches!(
            created(&mut h, 1, "kart", "One", 0),
            Reply::Created { .. }
        ));
        assert!(matches!(
            created(&mut h, 1, "deadfall", "Two", 1),
            Reply::Created { .. }
        ));
        let Reply::Error { code, .. } = created(&mut h, 1, "kart", "Three", 2) else {
            panic!()
        };
        assert_eq!(code, ErrorCode::Full);
        assert!(
            matches!(
                created(&mut h, 2, "kart", "Other", 3),
                Reply::Created { .. }
            ),
            "another address"
        );
    }

    #[test]
    fn a_long_room_list_is_truncated_to_one_datagram_and_can_be_paged() {
        let (mut h, _) = hub_with(big());
        let mut roomy = game_config("kart");
        roomy.max_rooms = 24;
        h.registry
            .add(GameEntry::new(roomy, fake_info("kart", 0x3333_4444), None).unwrap());
        for i in 0..24u64 {
            let name = format!("{i:02}{}", "字".repeat(wire::MAX_NAME_CHARS - 2));
            let from = (i % 200 + 1) as u8;
            assert!(
                matches!(
                    created(&mut h, from, "kart", &name, i),
                    Reply::Created { .. }
                ),
                "{i}"
            );
        }
        let first = h.handle(src(250), &list("kart").encode(1), 0).unwrap();
        assert!(first.len() <= MAX_REPLY, "{} bytes", first.len());
        let Reply::Rooms { total, rooms, .. } = Reply::decode(&first).unwrap().reply else {
            panic!()
        };
        assert_eq!(total as usize, 25);
        assert!(
            rooms.len() < total as usize,
            "the first page is a truncation"
        );
        let second = h
            .handle(
                src(250),
                &Request::List {
                    game: "kart".into(),
                    skip: rooms.len() as u8,
                }
                .encode(2),
                0,
            )
            .unwrap();
        let Reply::Rooms { rooms: more, .. } = Reply::decode(&second).unwrap().reply else {
            panic!()
        };
        assert_eq!(
            rooms.len() + more.len(),
            total as usize,
            "two pages cover everything"
        );
    }

    // ---- the legacy DFHB adapter, end to end through Hub::handle ----------------------------------------------------

    fn legacy_ask(
        h: &mut Hub,
        from: u8,
        r: &legacy::Request,
        nonce: u32,
        now: u64,
    ) -> Option<legacy::ReplyPacket> {
        let bytes = h.handle(src(from), &r.encode(nonce), now)?;
        assert!(bytes.len() <= legacy::max_reply_len(r));
        Some(legacy::Reply::decode(&bytes).expect("a v1 reply"))
    }

    #[test]
    fn legacy_clients_see_deadfalls_rooms_with_the_raw_fingerprint_as_the_build() {
        let (mut h, w) = hub();
        let p = legacy_ask(&mut h, 1, &legacy::Request::Ping, 5, 0).unwrap();
        assert_eq!(
            (p.nonce, p.build, p.reply),
            (5, 0x1111_2222, legacy::Reply::Pong)
        );
        let p = legacy_ask(&mut h, 1, &legacy::Request::List { skip: 0 }, 6, 0).unwrap();
        assert_eq!(
            p.build, 0x1111_2222,
            "the raw fingerprint, not the combined build"
        );
        let legacy::Reply::Rooms { total, rooms, .. } = p.reply else {
            panic!()
        };
        assert_eq!(total, 1);
        assert!(rooms[0].public && rooms[0].capacity == 12);
        // Create maps bots and kills onto the schema.
        let req = legacy::Request::Create {
            name: "Old school".into(),
            bots: true,
            kills: 25,
        };
        let p = legacy_ask(&mut h, 1, &req, 7, 0).unwrap();
        let legacy::Reply::Created { room } = p.reply else {
            panic!("{p:?}")
        };
        assert_eq!(room.name, "Old school");
        assert_eq!(
            w.borrow().spawned.last().unwrap().settings,
            vec![(1, 1), (2, 25), (3, 1)]
        );
        // Rooms made through the old door are listed through the new one, and the other way round.
        let Reply::Rooms { total, .. } = ask(&mut h, 2, &list("deadfall"), 1).unwrap().reply else {
            panic!()
        };
        assert_eq!(total, 2);
        assert!(matches!(
            created(&mut h, 3, "deadfall", "New school", 2),
            Reply::Created { .. }
        ));
        let p = legacy_ask(&mut h, 2, &legacy::Request::List { skip: 0 }, 8, 3).unwrap();
        let legacy::Reply::Rooms { total, .. } = p.reply else {
            panic!()
        };
        assert_eq!(total, 3);
        // A repeated create (the reply was lost) returns the same room; errors keep their old codes.
        let again = legacy_ask(&mut h, 1, &req, 7, 4).unwrap();
        assert!(
            matches!(again.reply, legacy::Reply::Created { room: r } if r.name == "Old school")
        );
        let dup = legacy::Request::Create {
            name: "old SCHOOL".into(),
            bots: false,
            kills: 0,
        };
        let p = legacy_ask(&mut h, 1, &dup, 9, 5).unwrap();
        assert!(matches!(
            p.reply,
            legacy::Reply::Error {
                code: ErrorCode::NameTaken,
                ..
            }
        ));
        let bad = legacy::Request::Create {
            name: "<b>".into(),
            bots: false,
            kills: 0,
        };
        assert!(matches!(
            legacy_ask(&mut h, 4, &bad, 10, 6).unwrap().reply,
            legacy::Reply::Error {
                code: ErrorCode::BadName,
                ..
            }
        ));
    }

    #[test]
    fn legacy_creates_are_cut_off_globally_because_they_carry_no_cookie() {
        let (mut h, _) = hub_with(big());
        let mut made = 0;
        for i in 0..40u8 {
            let req = legacy::Request::Create {
                name: format!("Spoof {i}"),
                bots: false,
                kills: 0,
            };
            if legacy_ask(&mut h, i + 1, &req, i as u32, 0).is_some() {
                made += 1;
            }
        }
        assert_eq!(
            made, 3,
            "the global burst for the old protocol, however many sources"
        );
        // Listing is unaffected.
        assert!(legacy_ask(&mut h, 99, &legacy::Request::List { skip: 0 }, 1, 0).is_some());
    }

    #[test]
    fn legacy_refuse_mode_answers_well_formed_v1_replies_with_a_wrong_build() {
        let mut o = opts();
        o.legacy = Mode::Refuse;
        let (mut h, w) = hub_with(o);
        let wrong = 0x1111_2222 ^ 1;
        let p = legacy_ask(&mut h, 1, &legacy::Request::Ping, 1, 0).unwrap();
        assert_eq!((p.build, p.reply), (wrong, legacy::Reply::Pong));
        let p = legacy_ask(&mut h, 1, &legacy::Request::List { skip: 0 }, 2, 0).unwrap();
        assert_eq!(p.build, wrong);
        assert!(matches!(p.reply, legacy::Reply::Rooms { total: 0, .. }));
        let before = w.borrow().spawned.len();
        let p = legacy_ask(
            &mut h,
            1,
            &legacy::Request::Create {
                name: "x".into(),
                bots: false,
                kills: 0,
            },
            3,
            0,
        )
        .unwrap();
        assert!(matches!(p.reply, legacy::Reply::Error { .. }));
        assert_eq!(w.borrow().spawned.len(), before);
        // BEHB is unaffected by the legacy switch.
        assert!(ask(&mut h, 2, &list("deadfall"), 0).is_some());
    }

    #[test]
    fn without_a_deadfall_registered_old_clients_get_silence() {
        let w = Rc::new(RefCell::new(World::default()));
        let mut r = Registry::new();
        r.add(entry("kart", 1));
        let mut h = Hub::new(opts(), r, Box::new(Fake(w)), Box::new(Scripted::default()));
        assert!(h
            .handle(src(1), &legacy::Request::Ping.encode(1), 0)
            .is_none());
        assert!(ask(&mut h, 1, &list("kart"), 0).is_some());
    }

    // ---- reload and binary changes --------------------------------------------------------------------------------------

    fn config_file(tag: &str, text: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("be2hub-serve-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hub.conf");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn reload_rereads_one_game_retires_its_rooms_and_leaves_the_others_alone() {
        let path = config_file("reload", "[game kart]\nserver=/srv/kart-server\npublic=on\nmax_rooms=4\n[game deadfall]\nserver=/srv/deadfall-server\npublic=on\n");
        let w = Rc::new(RefCell::new(World::default()));
        let mut src_info = Scripted::default();
        src_info.put("/srv/kart-server", 1, Ok(fake_info("kart", 0xAAAA)));
        src_info.put("/srv/deadfall-server", 1, Ok(fake_info("deadfall", 0xBBBB)));
        let cfg = super::super::registry::parse_config(
            &std::fs::read_to_string(&path).unwrap(),
            path.parent().unwrap(),
        )
        .unwrap();
        let (reg, problems) = Registry::load(&cfg, &mut src_info);
        assert!(problems.is_empty(), "{problems:?}");
        let mut o = opts();
        o.config_path = Some(path.clone());
        let mut h = Hub::new(o, reg, Box::new(Fake(w.clone())), Box::new(src_info));
        let Reply::Created { room } = created(&mut h, 1, "kart", "Mine", 0) else {
            panic!()
        };
        w.borrow_mut()
            .status
            .insert(room.port, status(2, RoomState::Playing));
        h.tick(1000);
        let kart_public = h.manager().rooms_of("kart")[0].port;
        // The game is rebuilt: the box's --info answers a new fingerprint.
        let new_build = crate::viewer::netplay::cli::fold_build(0xCCCC);
        let mut s2 = Scripted::default();
        s2.put("/srv/kart-server", 2, Ok(fake_info("kart", 0xCCCC)));
        s2.put("/srv/deadfall-server", 1, Ok(fake_info("deadfall", 0xBBBB)));
        h.info_source = Box::new(s2);
        let text = h.reload("kart", 2000).unwrap();
        assert!(
            text.contains("3 room(s) retired") || text.contains("2 room(s) retired"),
            "{text}"
        );
        assert_eq!(h.registry().get("kart").unwrap().info.build, new_build);
        let p = ask(&mut h, 2, &list("kart"), 2000).unwrap();
        assert_eq!(p.build, new_build, "new lists carry the new build");
        let Reply::Rooms { rooms, .. } = p.reply else {
            panic!()
        };
        assert_eq!(
            rooms.len(),
            1,
            "a fresh Public room only: the old ones are retired"
        );
        assert_ne!(rooms[0].port, kart_public);
        assert!(h.manager().retired_of("kart") >= 1);
        assert_eq!(h.manager().retired_of("deadfall"), 0);
        assert_eq!(
            h.registry().get("deadfall").unwrap().info.fingerprint,
            0xBBBB
        );
        // A game the config does not have, and a broken new build.
        assert!(h
            .reload("nope", 3000)
            .unwrap_err()
            .contains("not in the config"));
        let mut s3 = Scripted::default();
        s3.put(
            "/srv/kart-server",
            3,
            Err("--info exited with status: 1".into()),
        );
        h.info_source = Box::new(s3);
        let e = h.reload("kart", 4000).unwrap_err();
        assert!(e.contains("exited"), "{e}");
        assert_eq!(
            h.registry().get("kart").unwrap().info.fingerprint,
            0xCCCC,
            "a broken build changes nothing"
        );
        // Removing a game from the file retires its rooms and forgets it.
        std::fs::write(&path, "[game kart]\nserver=/srv/kart-server\npublic=on\n").unwrap();
        let text = h.reload("deadfall", 5000).unwrap();
        assert!(text.contains("removed"), "{text}");
        assert!(h.registry().get("deadfall").is_none());
        assert!(h.manager().rooms_of("deadfall").is_empty());
        // A config with an error changes nothing.
        std::fs::write(&path, "[game kart]\nbogus=1\n").unwrap();
        assert!(h
            .reload("kart", 6000)
            .unwrap_err()
            .contains("config: line 2"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn the_control_datagram_works_from_loopback_only() {
        let path = config_file(
            "control",
            "[game kart]\nserver=/srv/kart-server\npublic=on\n",
        );
        let w = Rc::new(RefCell::new(World::default()));
        let mut src_info = Scripted::default();
        src_info.put("/srv/kart-server", 1, Ok(fake_info("kart", 1)));
        let cfg = super::super::registry::parse_config(
            &std::fs::read_to_string(&path).unwrap(),
            path.parent().unwrap(),
        )
        .unwrap();
        let (reg, _) = Registry::load(&cfg, &mut src_info);
        let mut o = opts();
        o.config_path = Some(path.clone());
        let mut h = Hub::new(o, reg, Box::new(Fake(w)), Box::new(src_info));
        let ctl = Control::Reload {
            game: "kart".into(),
        };
        assert!(
            h.handle(src(1), &ctl.encode(5), 0).is_none(),
            "not from the network"
        );
        let lo = SocketAddr::from(([127, 0, 0, 1], 40000));
        let reply = h.handle(lo, &ctl.encode(5), 0).unwrap();
        let (nonce, r) = ControlReply::decode(&reply).unwrap();
        assert!(nonce == 5 && r.ok, "{r:?}");
        let bad = h
            .handle(lo, &Control::Reload { game: "zzz".into() }.encode(6), 0)
            .unwrap();
        assert!(!ControlReply::decode(&bad).unwrap().1.ok);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    fn ask_status(h: &mut Hub, game: &str) -> Result<GameStatus, String> {
        let lo = SocketAddr::from(([127, 0, 0, 1], 40000));
        let reply = h
            .handle(lo, &Control::Status { game: game.into() }.encode(3), 0)
            .expect("loopback gets an answer");
        let (nonce, r) = ControlReply::decode(&reply).unwrap();
        assert_eq!(nonce, 3);
        if r.ok {
            Ok(GameStatus::parse(&r.text).expect("a well-formed status"))
        } else {
            Err(r.text)
        }
    }

    #[test]
    fn status_says_what_is_running_and_readiness_needs_the_new_builds_own_status_line() {
        use super::super::deploy::Readiness;
        let path = config_file(
            "status",
            "[game kart]\nserver=/srv/kart-server\npublic=on\n[game other]\nserver=/srv/other-server\npublic=on\n",
        );
        let w = Rc::new(RefCell::new(World::default()));
        let mut s1 = Scripted::default();
        s1.put("/srv/kart-server", 1, Ok(fake_info("kart", 0xAAAA)));
        s1.put("/srv/other-server", 1, Ok(fake_info("other", 0xBBBB)));
        let cfg = super::super::registry::parse_config(
            &std::fs::read_to_string(&path).unwrap(),
            path.parent().unwrap(),
        )
        .unwrap();
        let (reg, problems) = Registry::load(&cfg, &mut s1);
        assert!(problems.is_empty(), "{problems:?}");
        let mut o = opts();
        o.config_path = Some(path.clone());
        let mut h = Hub::new(o, reg, Box::new(Fake(w.clone())), Box::new(s1));
        let old = crate::viewer::netplay::cli::fold_build(0xAAAA);
        let new = crate::viewer::netplay::cli::fold_build(0xCCCC);
        let public_of = |h: &Hub, g: &str| h.manager().public_room(g).unwrap().port;
        let say = |w: &Rc<RefCell<World>>, port: u16, build: u32, players: u8| {
            let mut st = status(players, RoomState::Lobby);
            st.build = build;
            w.borrow_mut().status.insert(port, st);
        };

        // Started, silent: not ready. Printing the old build: ready for the old build, wrong for the new one.
        let st = ask_status(&mut h, "kart").unwrap();
        assert_eq!(st.readiness(None), Readiness::Starting);
        let (kart_old, other_port) = (public_of(&h, "kart"), public_of(&h, "other"));
        say(&w, kart_old, old, 1); // somebody is in it
        say(&w, other_port, fake_info("other", 0xBBBB).build, 0);
        h.tick(500);
        let st = ask_status(&mut h, "kart").unwrap();
        assert_eq!(st.readiness(Some(old)), Readiness::Ready);
        assert_eq!(st.readiness(Some(new)), Readiness::WrongBuild);

        // The server file is replaced and reloaded: the registry has the new build at once, but the new Public room is
        // only "starting" until its own process prints a status line, and a line with the old build does not count.
        let mut s2 = Scripted::default();
        s2.put("/srv/kart-server", 2, Ok(fake_info("kart", 0xCCCC)));
        s2.put("/srv/other-server", 1, Ok(fake_info("other", 0xBBBB)));
        h.info_source = Box::new(s2);
        let text = h.reload("kart", 1000).unwrap();
        assert!(
            text.contains(&format!("(build {new:08x})")) && text.contains("Not proof of readiness"),
            "{text}"
        );
        assert!(
            text.len() < 200,
            "fits a control reply: {} bytes",
            text.len()
        );
        let st = ask_status(&mut h, "kart").unwrap();
        assert_eq!((st.registry_build, st.retired), (new, 1));
        assert_eq!(st.readiness(Some(new)), Readiness::Starting);
        let kart_new = public_of(&h, "kart");
        assert_ne!(kart_new, kart_old);
        say(&w, kart_new, old, 0);
        h.tick(1500);
        assert_eq!(
            ask_status(&mut h, "kart").unwrap().readiness(Some(new)),
            Readiness::WrongBuild
        );
        say(&w, kart_new, new, 0);
        h.tick(2000);
        assert_eq!(
            ask_status(&mut h, "kart").unwrap().readiness(Some(new)),
            Readiness::Ready
        );
        // The other game never changed: same build, same Public room, nothing retired.
        let other = ask_status(&mut h, "other").unwrap();
        assert_eq!(
            (other.registry_build, other.public_port, other.retired),
            (fake_info("other", 0xBBBB).build, Some(other_port), 0)
        );

        // Unknown games and non-loopback senders.
        assert!(ask_status(&mut h, "ghost")
            .unwrap_err()
            .contains("not loaded"));
        assert!(h
            .handle(
                src(1),
                &Control::Status {
                    game: "kart".into()
                }
                .encode(4),
                0
            )
            .is_none());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_reload_that_cannot_start_its_new_public_room_changes_nothing_and_says_so() {
        let path = config_file(
            "noreplace",
            "[game kart]\nserver=/srv/kart-server\npublic=on\n",
        );
        let w = Rc::new(RefCell::new(World::default()));
        let mut s1 = Scripted::default();
        s1.put("/srv/kart-server", 1, Ok(fake_info("kart", 0xAAAA)));
        let cfg = super::super::registry::parse_config(
            &std::fs::read_to_string(&path).unwrap(),
            path.parent().unwrap(),
        )
        .unwrap();
        let (reg, _) = Registry::load(&cfg, &mut s1);
        let mut o = opts();
        o.config_path = Some(path.clone());
        let mut h = Hub::new(o, reg, Box::new(Fake(w.clone())), Box::new(s1));
        let before = h.manager().public_room("kart").unwrap().port;
        let mut s2 = Scripted::default();
        s2.put("/srv/kart-server", 2, Ok(fake_info("kart", 0xCCCC)));
        h.info_source = Box::new(s2);
        w.borrow_mut().fail_spawn = true;
        let e = h.reload("kart", 1000).unwrap_err();
        assert!(e.contains("nothing was changed"), "{e}");
        assert_eq!(
            h.registry().get("kart").unwrap().info.fingerprint,
            0xAAAA,
            "the registry still holds the build whose rooms are running"
        );
        assert_eq!(h.manager().public_room("kart").unwrap().port, before);
        assert_eq!(h.manager().retired_of("kart"), 0);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_binary_replaced_on_a_full_pool_still_retires_the_old_rooms_instead_of_listing_a_stale_build(
    ) {
        let w = Rc::new(RefCell::new(World::default()));
        let mut s = Scripted::default();
        s.put("/srv/deadfall-server", 1, Ok(fake_info("deadfall", 1)));
        s.put("/srv/kart-server", 1, Ok(fake_info("kart", 2)));
        let cfg = Config {
            hub: Default::default(),
            games: vec![game_config("deadfall"), game_config("kart")],
        };
        let (reg, _) = Registry::load(&cfg, &mut s);
        let mut o = opts();
        o.manager.pool_size = 2;
        o.manager.max_processes = 2;
        let mut h = Hub::new(o, reg, Box::new(Fake(w.clone())), Box::new(s));
        let old = h.manager().rooms_of("deadfall")[0].port;
        w.borrow_mut()
            .status
            .insert(old, status(2, RoomState::Playing));
        h.tick(1000);
        let mut s2 = Scripted::default();
        s2.put("/srv/deadfall-server", 2, Ok(fake_info("deadfall", 99)));
        s2.put("/srv/kart-server", 1, Ok(fake_info("kart", 2)));
        h.info_source = Box::new(s2);
        h.tick(BINARY_CHECK_MS);
        h.tick(2 * BINARY_CHECK_MS);
        // No second process fits, so the new Public room cannot start first: the old occupied room is retired (its
        // players keep playing) and the other game is untouched.
        assert_eq!(h.manager().retired_of("deadfall"), 1);
        assert!(h.manager().rooms_of("deadfall").is_empty());
        assert_eq!(h.manager().rooms_of("kart").len(), 1);
        assert_eq!(h.manager().retired_of("kart"), 0);
    }

    #[test]
    fn a_resent_control_request_is_answered_again_without_running_twice() {
        let path = config_file(
            "resend",
            "[game kart]\nserver=/srv/kart-server\npublic=on\n",
        );
        let w = Rc::new(RefCell::new(World::default()));
        let mut src_info = Scripted::default();
        src_info.put("/srv/kart-server", 1, Ok(fake_info("kart", 1)));
        let cfg = super::super::registry::parse_config(
            &std::fs::read_to_string(&path).unwrap(),
            path.parent().unwrap(),
        )
        .unwrap();
        let (reg, _) = Registry::load(&cfg, &mut src_info);
        let mut o = opts();
        o.config_path = Some(path.clone());
        let mut h = Hub::new(o, reg, Box::new(Fake(w)), Box::new(src_info));
        let lo = SocketAddr::from(([127, 0, 0, 1], 40000));
        let ctl = Control::Reload {
            game: "kart".into(),
        }
        .encode(5);
        let first = h.handle(lo, &ctl, 0).unwrap();
        let room = h.manager().public_room("kart").unwrap().port;
        let again = h.handle(lo, &ctl, 10).unwrap();
        assert_eq!(first, again, "the same answer");
        assert_eq!(
            h.manager().public_room("kart").unwrap().port,
            room,
            "the room the first reload started was not retired by the resend"
        );
        assert_eq!(
            h.manager().retired_of("kart"),
            1,
            "one reload happened, not two"
        );
        // A new request (another nonce, or another process) is a new reload.
        let next = Control::Reload {
            game: "kart".into(),
        }
        .encode(6);
        h.handle(lo, &next, 20).unwrap();
        assert_ne!(h.manager().public_room("kart").unwrap().port, room);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_changed_server_binary_retires_that_games_rooms_by_itself() {
        let w = Rc::new(RefCell::new(World::default()));
        let mut s = Scripted::default();
        s.put("/srv/deadfall-server", 1, Ok(fake_info("deadfall", 1)));
        s.put("/srv/kart-server", 1, Ok(fake_info("kart", 2)));
        let cfg = Config {
            hub: Default::default(),
            games: vec![game_config("deadfall"), game_config("kart")],
        };
        let (reg, _) = Registry::load(&cfg, &mut s);
        let mut h = Hub::new(opts(), reg, Box::new(Fake(w)), Box::new(s));
        let old = h.manager().rooms_of("deadfall")[0].port;
        // Replace the deadfall file: seen at one check, settled at the next, then applied.
        let mut s2 = Scripted::default();
        s2.put("/srv/deadfall-server", 2, Ok(fake_info("deadfall", 99)));
        s2.put("/srv/kart-server", 1, Ok(fake_info("kart", 2)));
        h.info_source = Box::new(s2);
        h.tick(BINARY_CHECK_MS);
        assert_eq!(h.registry().get("deadfall").unwrap().info.fingerprint, 1);
        h.tick(2 * BINARY_CHECK_MS);
        assert_eq!(h.registry().get("deadfall").unwrap().info.fingerprint, 99);
        assert_eq!(h.registry().get("kart").unwrap().info.fingerprint, 2);
        let rooms = h.manager().rooms_of("deadfall");
        assert_eq!(rooms.len(), 1);
        assert_ne!(
            rooms[0].port, old,
            "the Public room was started again with the new binary"
        );
    }

    #[test]
    fn the_cookie_and_hub_state_survive_nothing_a_client_can_see() {
        // A hub never reveals paths: no reply contains the server path or the config's directory.
        let (mut h, _) = hub();
        let r = h.handle(src(1), &list("kart").encode(1), 0).unwrap();
        assert!(!String::from_utf8_lossy(&r).contains("/srv"));
    }
}
