//! The room table: which room servers exist, on which ports, and when they are closed.
//!
//! One shared pool of UDP ports serves every game. Ports are handed out round-robin, so a port that was just
//! freed is the last one to be reused: a client holding a stale list that joins `host:port` after the room
//! closed finds nothing there rather than another game's room.
//!
//! # Lifetime of a room
//! * A **Public** room (per game, if the registry says `public = on`) is permanent: if its server dies the hub
//!   starts it again after a short pause, on a fresh port.
//! * A **player-made** room closes after it has been empty for [`ManagerConfig::empty_timeout_ms`] (120 s), and
//!   after a shorter [`ManagerConfig::never_joined_timeout_ms`] (45 s from creation) when nobody ever joined: a
//!   room made by a prank or a script does not sit on a slot for two minutes.
//! * A room whose server ended unexpectedly is dropped from the list at once.
//!
//! # Reload semantics
//! [`RoomManager::retire_game`] is what `be2-hub reload <game>` (and a changed server binary) does: every room of
//! that game is **retired**. A retired room is no longer listed and is not counted against the game's room cap,
//! so players cannot join it any more, but its server keeps running with the build it started with until it is
//! empty (it then closes at once) or [`ManagerConfig::retire_grace_ms`] has passed. Players inside a match are
//! therefore never thrown out by an update; the next list shows fresh rooms, the Public room is started again
//! on a new port straight away, and only that game is affected.
use super::log;
use super::registry::{GameEntry, Registry};
use super::spawn::{RoomSpec, RoomStatus, Spawner};
use super::wire::{sanitize_name, ErrorCode, RoomInfo, RoomState};
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct ManagerConfig {
    /// Rooms use `pool_start .. pool_start + pool_size`.
    pub pool_start: u16,
    pub pool_size: u16,
    /// The IP the room servers bind (the port is added).
    pub bind_ip: String,
    /// Match reports go to `<report_dir>/<game>/port-<n>`.
    pub report_dir: Option<PathBuf>,
    /// A player-made room that has had nobody in it this long is shut down.
    pub empty_timeout_ms: u64,
    /// A player-made room nobody ever joined is shut down this long after it was made.
    pub never_joined_timeout_ms: u64,
    /// Wait this long before restarting a dead Public room.
    pub public_restart_ms: u64,
    /// A retired room that still has players is closed after this long anyway.
    pub retire_grace_ms: u64,
    /// Room servers at once, all games and retired rooms together.
    pub max_processes: usize,
    /// Rooms one creator IP may have open at once.
    pub max_rooms_per_ip: usize,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            pool_start: 4101,
            pool_size: 16,
            bind_ip: "0.0.0.0".into(),
            report_dir: None,
            empty_timeout_ms: 120_000,
            never_joined_timeout_ms: 45_000,
            public_restart_ms: 2_000,
            retire_grace_ms: 30 * 60_000,
            max_processes: 16,
            max_rooms_per_ip: 2,
        }
    }
}

/// Why a room could not be created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HubError {
    pub code: ErrorCode,
    pub text: String,
}

impl HubError {
    pub fn new(code: ErrorCode, text: impl Into<String>) -> Self {
        Self {
            code,
            text: text.into(),
        }
    }
}

struct Room {
    game: String,
    name: String,
    port: u16,
    public: bool,
    process: Box<dyn RoomProcess>,
    status: Option<RoomStatus>,
    /// The capacity to list until the server reports its own.
    capacity_hint: u8,
    created_ms: u64,
    /// A player has been seen in the room at least once.
    ever_joined: bool,
    /// Since when the room has been seen empty (a new room counts as empty from its creation).
    empty_since_ms: Option<u64>,
    creator: Option<IpAddr>,
    retired_at_ms: Option<u64>,
}

use super::spawn::RoomProcess;

/// A game's live Public room as the room table sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicRoom {
    pub port: u16,
    /// The latest fresh status its process printed; `None` until it has printed one (or if it stopped).
    pub status: Option<RoomStatus>,
}

/// Why a Public room could not be started.
enum StartFail {
    /// No process slot or pool port: waiting (or closing something) may fix it.
    Capacity(String),
    /// The server file would not run.
    Spawn(String),
}

impl StartFail {
    fn describe(&self, game: &str) -> String {
        match self {
            Self::Capacity(why) => format!("No Public room could be started for {game}: {why}"),
            Self::Spawn(why) => format!("Could not start the Public room of {game}: {why}"),
        }
    }
}

impl Room {
    fn info(&self) -> RoomInfo {
        let (players, capacity, state) = match self.status {
            Some(s) => (s.players, s.max, s.state),
            None => (0, self.capacity_hint, RoomState::Lobby),
        };
        RoomInfo {
            name: self.name.clone(),
            players,
            capacity,
            state,
            port: self.port,
            public: self.public,
        }
    }
}

/// Owns the room table, the port pool and the child servers.
pub struct RoomManager {
    cfg: ManagerConfig,
    spawner: Box<dyn Spawner>,
    rooms: Vec<Room>,
    /// Where to look next in the pool.
    cursor: usize,
    /// Per game: not before this time may its Public room be started again.
    next_public_try_ms: HashMap<String, u64>,
}

impl RoomManager {
    pub fn new(cfg: ManagerConfig, spawner: Box<dyn Spawner>) -> Self {
        Self {
            cfg,
            spawner,
            rooms: Vec::new(),
            cursor: 0,
            next_public_try_ms: HashMap::new(),
        }
    }

    pub fn config(&self) -> &ManagerConfig {
        &self.cfg
    }

    /// Room servers running now, retired ones included.
    pub fn processes(&self) -> usize {
        self.rooms.len()
    }

    /// The next free port of the pool, round-robin; `None` when every port is in use or taken by another program.
    fn alloc_port(&mut self) -> Option<u16> {
        let size = self.cfg.pool_size as usize;
        for step in 0..size {
            let idx = (self.cursor + step) % size;
            let port = self.cfg.pool_start + idx as u16;
            if self.rooms.iter().any(|r| r.port == port) {
                continue;
            }
            if !self.spawner.port_free(&self.cfg.bind_ip, port) {
                continue;
            }
            self.cursor = (idx + 1) % size;
            return Some(port);
        }
        None
    }

    fn spec(
        &self,
        game: &GameEntry,
        name: &str,
        port: u16,
        public: bool,
        settings: Vec<(u8, u32)>,
    ) -> RoomSpec {
        let g = game.id();
        RoomSpec {
            game: g.to_string(),
            name: name.to_string(),
            port,
            listen: format!("{}:{port}", self.cfg.bind_ip),
            server: game.config.server.clone(),
            settings,
            transport: game.config.transport.clone(),
            auto_start: game.config.auto_start,
            // One directory per game and port so two servers never append to the same matches.jsonl.
            report_dir: self
                .cfg
                .report_dir
                .as_ref()
                .map(|d| d.join(g).join(format!("port-{port}"))),
            public,
        }
    }

    /// Housekeeping: read statuses, drop dead, long-empty, never-joined and drained rooms, (re)start Public rooms.
    /// Call a few times a second.
    pub fn tick(&mut self, now_ms: u64, registry: &Registry) {
        // Rooms whose server ended.
        let mut i = 0;
        while i < self.rooms.len() {
            if self.rooms[i].process.exited() {
                let r = self.rooms.remove(i);
                log(&format!(
                    "Room \"{}\" ({}, port {}) ended unexpectedly; removing it",
                    r.name, r.game, r.port
                ));
                if r.public && r.retired_at_ms.is_none() {
                    self.next_public_try_ms
                        .insert(r.game.clone(), now_ms + self.cfg.public_restart_ms);
                }
                continue;
            }
            i += 1;
        }
        // Statuses, and the rooms that have outlived their purpose.
        let cfg = self.cfg.clone();
        let mut i = 0;
        while i < self.rooms.len() {
            let room = &mut self.rooms[i];
            room.status = room.process.status();
            if room.status.is_some_and(|s| s.players > 0) {
                room.ever_joined = true;
                room.empty_since_ms = None;
            } else {
                room.empty_since_ms.get_or_insert(now_ms);
            }
            let why = if let Some(since) = room.retired_at_ms {
                if room.status.is_some_and(|s| s.players == 0) {
                    Some("retired and empty".to_string())
                } else if now_ms.saturating_sub(since) >= cfg.retire_grace_ms {
                    Some(format!("retired {} s ago", cfg.retire_grace_ms / 1000))
                } else {
                    None
                }
            } else if room.public {
                None
            } else if !room.ever_joined
                && now_ms.saturating_sub(room.created_ms) >= cfg.never_joined_timeout_ms
            {
                Some(format!(
                    "nobody joined in {} s",
                    cfg.never_joined_timeout_ms / 1000
                ))
            } else if room
                .empty_since_ms
                .is_some_and(|t| now_ms.saturating_sub(t) >= cfg.empty_timeout_ms)
            {
                Some(format!("empty for {} s", cfg.empty_timeout_ms / 1000))
            } else {
                None
            };
            if let Some(why) = why {
                log(&format!(
                    "Closing room \"{}\" ({}, port {}): {why}",
                    room.name, room.game, room.port
                ));
                room.process.kill();
                self.rooms.remove(i);
                continue;
            }
            i += 1;
        }
        self.ensure_public_rooms(now_ms, registry);
    }

    fn ensure_public_rooms(&mut self, now_ms: u64, registry: &Registry) {
        for game in registry.games() {
            if !game.config.public {
                continue;
            }
            let id = game.id();
            if self
                .rooms
                .iter()
                .any(|r| r.game == id && r.public && r.retired_at_ms.is_none())
            {
                continue;
            }
            if now_ms < self.next_public_try_ms.get(id).copied().unwrap_or(0) {
                continue;
            }
            match self.start_public(game, now_ms) {
                Ok(port) => log(&format!(
                    "Public room \"{}\" of {id} is up on port {port}",
                    game.config.public_name
                )),
                Err(e) => {
                    let retry = self.cfg.public_restart_ms.max(5_000);
                    log(&format!("{}; will retry", e.describe(id)));
                    self.next_public_try_ms
                        .insert(id.to_string(), now_ms + retry);
                }
            }
        }
    }

    /// Start `game`'s Public room on a free pool port. Does not look at the rooms the game already has.
    fn start_public(&mut self, game: &GameEntry, now_ms: u64) -> Result<u16, StartFail> {
        let id = game.id();
        if self.rooms.len() >= self.cfg.max_processes {
            return Err(StartFail::Capacity(format!(
                "all {} room processes are in use",
                self.cfg.max_processes
            )));
        }
        let Some(port) = self.alloc_port() else {
            return Err(StartFail::Capacity(
                "no pool port is free (all are in use or held by another program)".into(),
            ));
        };
        let name = game.config.public_name.clone();
        let spec = self.spec(game, &name, port, true, game.public_settings.clone());
        let process = self
            .spawner
            .spawn(&spec)
            .map_err(|e| StartFail::Spawn(e.to_string()))?;
        self.rooms.push(Room {
            game: id.to_string(),
            name,
            port,
            public: true,
            process,
            status: None,
            capacity_hint: game.info.max_seats.min(255) as u8,
            created_ms: now_ms,
            ever_joined: false,
            empty_since_ms: Some(now_ms),
            creator: None,
            retired_at_ms: None,
        });
        Ok(port)
    }

    /// Every listed room of `game` (retired ones are not), the Public room first.
    pub fn rooms_of(&self, game: &str) -> Vec<RoomInfo> {
        let mut live: Vec<&Room> = self
            .rooms
            .iter()
            .filter(|r| r.game == game && r.retired_at_ms.is_none())
            .collect();
        live.sort_by_key(|r| !r.public);
        live.into_iter().map(Room::info).collect()
    }

    /// The game's live (not retired) Public room: its port and the fresh status its process reported, if any.
    pub fn public_room(&self, game: &str) -> Option<PublicRoom> {
        self.rooms
            .iter()
            .find(|r| r.game == game && r.public && r.retired_at_ms.is_none())
            .map(|r| PublicRoom {
                port: r.port,
                status: r.status,
            })
    }

    /// Rooms of `game` that are being drained after a reload.
    pub fn retired_of(&self, game: &str) -> usize {
        self.rooms
            .iter()
            .filter(|r| r.game == game && r.retired_at_ms.is_some())
            .count()
    }

    /// Make a player's room. `settings` must already be validated against the game's schema
    /// ([`GameEntry::room_settings`]). Fails on a bad or taken name, a cap, or a server that will not start.
    pub fn create(
        &mut self,
        game: &GameEntry,
        raw_name: &str,
        settings: Vec<(u8, u32)>,
        creator: IpAddr,
        now_ms: u64,
    ) -> Result<RoomInfo, HubError> {
        let id = game.id();
        let name =
            sanitize_name(raw_name).map_err(|e| HubError::new(ErrorCode::BadName, e.text()))?;
        let lower = name.to_lowercase();
        let live = |r: &&Room| r.retired_at_ms.is_none();
        if lower == game.config.public_name.to_lowercase()
            || self
                .rooms
                .iter()
                .filter(live)
                .any(|r| r.game == id && r.name.to_lowercase() == lower)
        {
            return Err(HubError::new(
                ErrorCode::NameTaken,
                "A room with that name already exists.",
            ));
        }
        let own = self
            .rooms
            .iter()
            .filter(live)
            .filter(|r| r.creator == Some(creator))
            .count();
        if own >= self.cfg.max_rooms_per_ip {
            return Err(HubError::new(
                ErrorCode::Full,
                "You already have rooms open. Close one (leave it empty) or join it.",
            ));
        }
        let of_game = self
            .rooms
            .iter()
            .filter(live)
            .filter(|r| r.game == id && !r.public)
            .count();
        if of_game >= game.config.max_rooms {
            return Err(HubError::new(
                ErrorCode::Full,
                "All rooms are in use right now. Join one, or try again in a few minutes.",
            ));
        }
        if self.rooms.len() >= self.cfg.max_processes {
            return Err(HubError::new(
                ErrorCode::Full,
                "The server is busy with other games. Try again in a few minutes.",
            ));
        }
        let Some(port) = self.alloc_port() else {
            return Err(HubError::new(
                ErrorCode::Full,
                "All rooms are in use right now. Join one, or try again in a few minutes.",
            ));
        };
        let spec = self.spec(game, &name, port, false, settings);
        let process = self.spawner.spawn(&spec).map_err(|e| {
            log(&format!(
                "Could not start a {id} server for room \"{name}\": {e}"
            ));
            HubError::new(
                ErrorCode::Unavailable,
                "The server could not start the room. Try again shortly.",
            )
        })?;
        let room = Room {
            game: id.to_string(),
            name,
            port,
            public: false,
            process,
            status: None,
            capacity_hint: game.info.max_seats.min(255) as u8,
            created_ms: now_ms,
            ever_joined: false,
            empty_since_ms: Some(now_ms),
            creator: Some(creator),
            retired_at_ms: None,
        };
        let info = room.info();
        self.rooms.push(room);
        Ok(info)
    }

    /// Retire every room of `game` (see the module docs). Returns how many were retired.
    pub fn retire_game(&mut self, game: &str, now_ms: u64) -> usize {
        self.retire_except(game, now_ms, None)
    }

    fn retire_except(&mut self, game: &str, now_ms: u64, keep_port: Option<u16>) -> usize {
        let mut n = 0;
        for r in self
            .rooms
            .iter_mut()
            .filter(|r| r.game == game && r.retired_at_ms.is_none() && Some(r.port) != keep_port)
        {
            r.retired_at_ms = Some(now_ms);
            n += 1;
        }
        self.next_public_try_ms.remove(game);
        n
    }

    /// What a reload does for a game whose server was replaced: start the new build's Public room **first**, then
    /// retire the rooms that were there, so a replacement that cannot start (no free port or process slot, a
    /// binary that will not run) leaves the game exactly as it was and says why. Returns how many rooms were
    /// retired. One exception keeps updates possible on a full pool: when the only thing in the way is capacity and
    /// the game's current Public room is empty (retiring it closes it at once), the old order is used.
    pub fn replace_game(&mut self, game: &GameEntry, now_ms: u64) -> Result<usize, String> {
        let id = game.id();
        if !game.config.public {
            return Ok(self.retire_game(id, now_ms));
        }
        match self.start_public(game, now_ms) {
            Ok(port) => {
                log(&format!(
                    "Public room \"{}\" of {id} is up on port {port} (new build)",
                    game.config.public_name
                ));
                Ok(self.retire_except(id, now_ms, Some(port)))
            }
            Err(StartFail::Capacity(why))
                if self
                    .public_room(id)
                    .is_some_and(|r| r.status.is_some_and(|s| s.players == 0)) =>
            {
                log(&format!(
                    "No room for a second {id} Public room ({why}): retiring the empty one first"
                ));
                Ok(self.retire_game(id, now_ms))
            }
            Err(e) => Err(format!(
                "{}; nothing was changed, the old rooms keep running",
                e.describe(id)
            )),
        }
    }

    /// Kill every child server.
    pub fn shutdown(&mut self) {
        for r in &mut self.rooms {
            r.process.kill();
        }
        self.rooms.clear();
    }
}

impl Drop for RoomManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::registry::tests::{entry, game_config};
    use super::super::registry::{GameEntry, Registry};
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::io;
    use std::rc::Rc;

    #[derive(Default)]
    pub struct World {
        pub status: BTreeMap<u16, RoomStatus>,
        pub dead: Vec<u16>,
        pub spawned: Vec<RoomSpec>,
        pub killed: Vec<u16>,
        pub fail_spawn: bool,
        pub busy_ports: Vec<u16>,
    }
    pub struct Fake(pub Rc<RefCell<World>>);
    pub struct FakeProc(u16, Rc<RefCell<World>>);
    impl Spawner for Fake {
        fn spawn(&mut self, spec: &RoomSpec) -> io::Result<Box<dyn RoomProcess>> {
            let mut w = self.0.borrow_mut();
            if w.fail_spawn {
                return Err(io::Error::other("no binary"));
            }
            w.dead.retain(|p| *p != spec.port);
            w.spawned.push(spec.clone());
            Ok(Box::new(FakeProc(spec.port, self.0.clone())))
        }
        fn port_free(&mut self, _ip: &str, port: u16) -> bool {
            !self.0.borrow().busy_ports.contains(&port)
        }
    }
    impl RoomProcess for FakeProc {
        fn status(&mut self) -> Option<RoomStatus> {
            self.1.borrow().status.get(&self.0).copied()
        }
        fn exited(&mut self) -> bool {
            self.1.borrow().dead.contains(&self.0)
        }
        fn kill(&mut self) {
            self.1.borrow_mut().killed.push(self.0);
        }
    }

    pub fn status(players: u8, state: RoomState) -> RoomStatus {
        RoomStatus {
            players,
            max: 12,
            state,
            build: 0,
        }
    }

    fn registry(games: &[GameEntry]) -> Registry {
        let mut r = Registry::new();
        for g in games {
            r.add(g.clone());
        }
        r
    }

    fn ip(n: u8) -> IpAddr {
        IpAddr::from([10, 0, 0, n])
    }

    fn cfg() -> ManagerConfig {
        ManagerConfig {
            pool_start: 5000,
            pool_size: 6,
            max_processes: 6,
            ..Default::default()
        }
    }

    pub fn manager(cfg: ManagerConfig) -> (RoomManager, Rc<RefCell<World>>) {
        let w = Rc::new(RefCell::new(World::default()));
        (RoomManager::new(cfg, Box::new(Fake(w.clone()))), w)
    }

    fn names(m: &RoomManager, game: &str) -> Vec<String> {
        m.rooms_of(game).into_iter().map(|r| r.name).collect()
    }

    #[test]
    fn each_game_gets_its_public_room_first_on_pool_ports() {
        let (mut m, w) = manager(cfg());
        let reg = registry(&[entry("deadfall", 1), entry("kart", 2)]);
        m.tick(0, &reg);
        assert_eq!(names(&m, "deadfall"), ["Public"]);
        assert_eq!(names(&m, "kart"), ["Public"]);
        let spawned = w.borrow().spawned.clone();
        assert_eq!(
            spawned.iter().map(|s| s.port).collect::<Vec<_>>(),
            [5000, 5001]
        );
        assert!(spawned
            .iter()
            .all(|s| s.public && s.listen == format!("0.0.0.0:{}", s.port)));
        // Public settings and the game's own server and report directory.
        assert_eq!(spawned[0].settings, vec![(1, 1), (2, 40), (3, 1)]);
        assert_eq!(spawned[0].server, PathBuf::from("/srv/deadfall-server"));
        let m2 = ManagerConfig {
            report_dir: Some("/r".into()),
            ..cfg()
        };
        let (mut m2, w2) = manager(m2);
        m2.tick(0, &reg);
        assert_eq!(
            w2.borrow().spawned[1].report_dir,
            Some("/r/kart/port-5001".into())
        );
        // A game with public = off has none.
        let mut quiet = entry("quiet", 3);
        quiet.config.public = false;
        let (mut m3, _) = manager(cfg());
        m3.tick(0, &registry(&[quiet]));
        assert!(m3.rooms_of("quiet").is_empty());
    }

    #[test]
    fn rooms_are_per_game_and_names_are_unique_per_game_only() {
        let (mut m, _) = manager(cfg());
        let d = entry("deadfall", 1);
        let k = entry("kart", 2);
        let reg = registry(&[d.clone(), k.clone()]);
        m.tick(0, &reg);
        assert!(m.create(&d, "Arena", vec![], ip(1), 0).is_ok());
        assert_eq!(
            m.create(&d, "arena", vec![], ip(2), 0).unwrap_err().code,
            ErrorCode::NameTaken
        );
        assert_eq!(
            m.create(&d, "PUBLIC", vec![], ip(2), 0).unwrap_err().code,
            ErrorCode::NameTaken
        );
        assert!(
            m.create(&k, "Arena", vec![], ip(2), 0).is_ok(),
            "another game may reuse the name"
        );
        assert_eq!(
            m.create(&d, "<x>", vec![], ip(3), 0).unwrap_err().code,
            ErrorCode::BadName
        );
        assert_eq!(names(&m, "deadfall"), ["Public", "Arena"]);
        assert_eq!(names(&m, "kart"), ["Public", "Arena"]);
        assert!(m.rooms_of("nothing").is_empty());
    }

    #[test]
    fn ports_are_handed_out_round_robin_and_a_freed_port_is_the_last_reused() {
        let (mut m, w) = manager(ManagerConfig {
            max_processes: 6,
            ..cfg()
        });
        let d = GameEntry::new(
            GameConfig2::quiet("deadfall"),
            super::super::registry::tests::fake_info("deadfall", 1),
            None,
        )
        .unwrap();
        let reg = registry(std::slice::from_ref(&d));
        let port_of = |m: &mut RoomManager, name: &str, ip_n: u8, now: u64| {
            m.create(&d, name, vec![], ip(ip_n), now).unwrap().port
        };
        assert_eq!(port_of(&mut m, "a", 1, 0), 5000);
        assert_eq!(port_of(&mut m, "b", 2, 0), 5001);
        // a's room is closed (the server dies), then another is made: it does not get 5000 back.
        w.borrow_mut().dead.push(5000);
        m.tick(1, &reg);
        assert_eq!(port_of(&mut m, "c", 1, 2), 5002);
        assert_eq!(port_of(&mut m, "d", 3, 2), 5003);
        assert_eq!(port_of(&mut m, "e", 4, 2), 5004);
        assert_eq!(port_of(&mut m, "f", 5, 2), 5005);
        // Only now, having gone all the way round, is 5000 used again.
        w.borrow_mut().dead.push(5001);
        m.tick(3, &reg);
        assert_eq!(port_of(&mut m, "g", 6, 4), 5000);
        assert_eq!(port_of(&mut m, "h", 7, 4), 5001);
    }

    /// A game config with no Public room, for tests that count rooms.
    struct GameConfig2;
    impl GameConfig2 {
        fn quiet(id: &str) -> super::super::registry::GameConfig {
            let mut c = game_config(id);
            c.public = false;
            c.max_rooms = 24;
            c
        }
    }

    #[test]
    fn a_port_taken_by_another_program_is_skipped() {
        let (mut m, w) = manager(cfg());
        w.borrow_mut().busy_ports = vec![5000, 5001];
        let d = entry("deadfall", 1);
        m.tick(0, &registry(&[d]));
        assert_eq!(m.rooms_of("deadfall")[0].port, 5002);
    }

    #[test]
    fn caps_per_game_per_creator_and_per_hub_hold() {
        let mut cfg = cfg();
        cfg.max_processes = 4;
        let (mut m, _) = manager(cfg);
        let d = entry("deadfall", 1); // max_rooms 4, public on
        let reg = registry(std::slice::from_ref(&d));
        m.tick(0, &reg); // the Public room: 1 process
                         // Per creator IP: two rooms, then Full.
        assert!(m.create(&d, "a", vec![], ip(1), 0).is_ok());
        assert!(m.create(&d, "b", vec![], ip(1), 0).is_ok());
        let e = m.create(&d, "c", vec![], ip(1), 0).unwrap_err();
        assert_eq!(e.code, ErrorCode::Full);
        assert!(e.text.contains("already have"), "{e:?}");
        // The whole hub: Public + a + b = 3 processes of 4; one more fits, then it is full.
        assert!(m.create(&d, "d", vec![], ip(2), 0).is_ok());
        let e = m.create(&d, "e", vec![], ip(3), 0).unwrap_err();
        assert_eq!((e.code, m.processes()), (ErrorCode::Full, 4));
        assert!(e.text.contains("busy"), "{e:?}");
        // The per-game cap, when the hub has room: a game limited to 1 player room.
        let (mut m, _) = manager(cfg_big());
        let mut one = game_config("one");
        one.max_rooms = 1;
        one.public = false;
        let one = GameEntry::new(
            one,
            super::super::registry::tests::fake_info("one", 1),
            None,
        )
        .unwrap();
        assert!(m.create(&one, "a", vec![], ip(1), 0).is_ok());
        let e = m.create(&one, "b", vec![], ip(2), 0).unwrap_err();
        assert!(
            e.code == ErrorCode::Full && e.text.contains("in use"),
            "{e:?}"
        );
        // The cap counts per game: another game is not affected.
        let two = entry("two", 2);
        assert!(m.create(&two, "a", vec![], ip(2), 0).is_ok());
    }

    fn cfg_big() -> ManagerConfig {
        ManagerConfig {
            pool_size: 32,
            max_processes: 32,
            max_rooms_per_ip: 8,
            ..cfg()
        }
    }

    #[test]
    fn a_creators_slot_comes_back_when_the_room_closes() {
        let (mut m, w) = manager(cfg_big_ip(1));
        let d = GameEntry::new(
            GameConfig2::quiet("d"),
            super::super::registry::tests::fake_info("d", 1),
            None,
        )
        .unwrap();
        let reg = registry(std::slice::from_ref(&d));
        assert!(m.create(&d, "a", vec![], ip(1), 0).is_ok());
        assert!(m.create(&d, "b", vec![], ip(1), 0).is_err());
        w.borrow_mut().dead.push(5000);
        m.tick(10, &reg);
        assert!(m.create(&d, "b", vec![], ip(1), 11).is_ok());
    }

    fn cfg_big_ip(n: usize) -> ManagerConfig {
        ManagerConfig {
            max_rooms_per_ip: n,
            ..cfg()
        }
    }

    #[test]
    fn counts_and_states_come_from_the_servers_and_empty_rooms_are_reaped() {
        let (mut m, w) = manager(cfg());
        let d = entry("deadfall", 1);
        let reg = registry(std::slice::from_ref(&d));
        m.tick(0, &reg);
        let made = m.create(&d, "Mine", vec![], ip(1), 0).unwrap();
        assert_eq!(
            (made.players, made.capacity, made.state),
            (0, 12, RoomState::Lobby)
        );
        w.borrow_mut()
            .status
            .insert(made.port, status(5, RoomState::Playing));
        m.tick(1000, &reg);
        let r = m.rooms_of("deadfall");
        assert_eq!((r[1].players, r[1].state), (5, RoomState::Playing));
        // Everyone leaves: still listed until the empty timeout.
        w.borrow_mut()
            .status
            .insert(made.port, status(0, RoomState::Lobby));
        m.tick(2000, &reg);
        m.tick(2000 + 119_000, &reg);
        assert_eq!(names(&m, "deadfall"), ["Public", "Mine"]);
        m.tick(2000 + 120_000, &reg);
        assert_eq!(names(&m, "deadfall"), ["Public"]);
        assert!(w.borrow().killed.contains(&made.port));
        // A stale status (the server hangs) reads as empty, not as the last count.
        let (mut m, w) = manager(cfg());
        let made = m.create(&d, "Mine", vec![], ip(1), 0).unwrap();
        w.borrow_mut()
            .status
            .insert(made.port, status(4, RoomState::Lobby));
        m.tick(1000, &reg);
        w.borrow_mut().status.clear();
        m.tick(2000, &reg);
        assert_eq!(
            m.rooms_of("deadfall")
                .iter()
                .find(|r| r.name == "Mine")
                .unwrap()
                .players,
            0
        );
    }

    #[test]
    fn a_room_nobody_ever_joins_closes_sooner_than_one_that_emptied() {
        let (mut m, w) = manager(cfg());
        let d = entry("deadfall", 1);
        let reg = registry(std::slice::from_ref(&d));
        let never = m.create(&d, "Never", vec![], ip(1), 0).unwrap();
        let once = m.create(&d, "Once", vec![], ip(2), 0).unwrap();
        w.borrow_mut()
            .status
            .insert(once.port, status(1, RoomState::Lobby));
        m.tick(1000, &reg);
        w.borrow_mut()
            .status
            .insert(once.port, status(0, RoomState::Lobby));
        m.tick(2000, &reg);
        m.tick(44_999, &reg);
        assert_eq!(names(&m, "deadfall"), ["Public", "Never", "Once"]);
        m.tick(45_000, &reg);
        assert_eq!(
            names(&m, "deadfall"),
            ["Public", "Once"],
            "never joined: closed at 45 s"
        );
        assert!(w.borrow().killed.contains(&never.port));
        m.tick(2000 + 119_999, &reg);
        assert_eq!(names(&m, "deadfall"), ["Public", "Once"]);
        m.tick(2000 + 120_000, &reg);
        assert_eq!(
            names(&m, "deadfall"),
            ["Public"],
            "emptied: closed 120 s after it was last occupied"
        );
    }

    #[test]
    fn a_dead_public_room_is_restarted_on_a_new_port_after_a_pause_and_a_dead_user_room_is_not() {
        let (mut m, w) = manager(cfg());
        let d = entry("deadfall", 1);
        let reg = registry(std::slice::from_ref(&d));
        m.tick(0, &reg);
        let first = m.rooms_of("deadfall")[0].port;
        let user = m.create(&d, "Mine", vec![], ip(1), 0).unwrap();
        w.borrow_mut().dead.push(first);
        m.tick(1000, &reg);
        assert_eq!(names(&m, "deadfall"), ["Mine"], "gone, restart pending");
        m.tick(2999, &reg);
        assert_eq!(names(&m, "deadfall"), ["Mine"]);
        m.tick(3000, &reg);
        let rooms = m.rooms_of("deadfall");
        assert_eq!(rooms[0].name, "Public");
        assert_ne!(rooms[0].port, first, "a fresh port, not the one just freed");
        w.borrow_mut().dead.push(user.port);
        m.tick(4000, &reg);
        assert_eq!(
            names(&m, "deadfall"),
            ["Public"],
            "a user room is not restarted"
        );
        // Spawn failure: retried later, nothing listed meanwhile.
        w.borrow_mut().fail_spawn = true;
        let pub_port = m.rooms_of("deadfall")[0].port;
        w.borrow_mut().dead.push(pub_port);
        m.tick(5000, &reg);
        m.tick(8000, &reg);
        assert!(m.rooms_of("deadfall").is_empty());
        w.borrow_mut().fail_spawn = false;
        m.tick(13_000, &reg);
        assert_eq!(names(&m, "deadfall"), ["Public"]);
    }

    #[test]
    fn a_failed_spawn_is_reported_without_leaving_a_room() {
        let (mut m, w) = manager(cfg());
        let d = entry("deadfall", 1);
        w.borrow_mut().fail_spawn = true;
        let e = m.create(&d, "Mine", vec![], ip(1), 0).unwrap_err();
        assert_eq!(e.code, ErrorCode::Unavailable);
        assert_eq!(m.processes(), 0);
        w.borrow_mut().fail_spawn = false;
        assert!(
            m.create(&d, "Mine", vec![], ip(1), 0).is_ok(),
            "the name is not burned"
        );
    }

    #[test]
    fn retiring_a_game_hides_its_rooms_keeps_players_in_them_and_replaces_the_public_room() {
        let (mut m, w) = manager(cfg());
        let d = entry("deadfall", 1);
        let k = entry("kart", 2);
        let reg = registry(&[d.clone(), k.clone()]);
        m.tick(0, &reg);
        let busy = m.create(&d, "Busy", vec![], ip(1), 0).unwrap();
        let idle = m.create(&d, "Idle", vec![], ip(2), 0).unwrap();
        let other = m.create(&k, "Other", vec![], ip(3), 0).unwrap();
        let old_public = m.rooms_of("deadfall")[0].port;
        w.borrow_mut()
            .status
            .insert(busy.port, status(2, RoomState::Playing));
        w.borrow_mut()
            .status
            .insert(old_public, status(3, RoomState::Lobby));
        w.borrow_mut()
            .status
            .insert(idle.port, status(0, RoomState::Lobby));
        w.borrow_mut()
            .status
            .insert(other.port, status(1, RoomState::Lobby));
        m.tick(1000, &reg);
        assert_eq!(m.retire_game("deadfall", 1000), 3);
        assert!(
            m.rooms_of("deadfall").is_empty(),
            "retired rooms are not listed"
        );
        assert_eq!(m.retired_of("deadfall"), 3);
        assert_eq!(
            names(&m, "kart"),
            ["Public", "Other"],
            "another game is untouched"
        );
        m.tick(1500, &reg);
        // The empty one closed at once; the two with players keep running; a new Public room is up.
        assert_eq!(m.retired_of("deadfall"), 2);
        assert!(w.borrow().killed.contains(&idle.port));
        assert!(
            !w.borrow().killed.contains(&busy.port) && !w.borrow().killed.contains(&old_public)
        );
        let rooms = m.rooms_of("deadfall");
        assert_eq!(rooms.len(), 1);
        assert!(rooms[0].public && rooms[0].port != old_public);
        // The name of a retired room is free again, and the retired room does not count against the caps.
        assert!(m.create(&d, "Busy", vec![], ip(1), 2000).is_ok());
        // When the players leave, the old rooms close.
        w.borrow_mut()
            .status
            .insert(busy.port, status(0, RoomState::Lobby));
        m.tick(3000, &reg);
        assert_eq!(m.retired_of("deadfall"), 1);
        // The grace period closes what is left even with players inside.
        m.tick(1000 + 30 * 60_000, &reg);
        assert_eq!(m.retired_of("deadfall"), 0);
        assert!(w.borrow().killed.contains(&old_public));
    }

    fn ports_of(m: &RoomManager, game: &str) -> Vec<u16> {
        m.rooms_of(game).into_iter().map(|r| r.port).collect()
    }

    #[test]
    fn replacing_a_game_starts_the_new_public_room_first_and_changes_only_that_game() {
        let (mut m, w) = manager(cfg());
        let a = entry("kart", 2);
        let b_old = entry("deadfall", 1);
        let b_new = entry("deadfall", 9);
        let reg = registry(&[a.clone(), b_old.clone()]);
        m.tick(0, &reg);
        let a_busy = m.create(&a, "A busy", vec![], ip(1), 0).unwrap();
        let b_busy = m.create(&b_old, "B busy", vec![], ip(2), 0).unwrap();
        let (a_public, b_public) = (m.rooms_of("kart")[0].port, m.rooms_of("deadfall")[0].port);
        for (port, players) in [
            (a_busy.port, 2),
            (b_busy.port, 3),
            (a_public, 1),
            (b_public, 0),
        ] {
            w.borrow_mut()
                .status
                .insert(port, status(players, RoomState::Playing));
        }
        m.tick(1000, &reg);
        let a_before = (ports_of(&m, "kart"), names(&m, "kart"));

        // A candidate whose server will not even start: nothing about B (or A) changes.
        w.borrow_mut().fail_spawn = true;
        let e = m.replace_game(&b_new, 2000).unwrap_err();
        assert!(e.contains("nothing was changed"), "{e}");
        w.borrow_mut().fail_spawn = false;
        assert_eq!(m.retired_of("deadfall"), 0, "B still has its usable rooms");
        assert_eq!(names(&m, "deadfall"), ["Public", "B busy"]);

        // A good one: B's new Public room is already running when the old rooms are retired.
        let n = m.replace_game(&b_new, 3000).unwrap();
        assert_eq!(n, 2, "B's old Public room and B's busy room");
        let mut reg = reg;
        reg.add(b_new.clone());
        m.tick(3500, &reg);
        let b_rooms = m.rooms_of("deadfall");
        assert_eq!(b_rooms.len(), 1);
        assert!(b_rooms[0].public && b_rooms[0].port != b_public);
        assert_eq!(
            m.retired_of("deadfall"),
            1,
            "the empty old Public room closed at once; the occupied room drains"
        );
        assert!(w.borrow().killed.contains(&b_public));
        assert!(
            !w.borrow().killed.contains(&b_busy.port),
            "players inside are not thrown out"
        );
        // A was not touched in any way.
        assert_eq!((ports_of(&m, "kart"), names(&m, "kart")), a_before);
        assert_eq!(m.retired_of("kart"), 0);
        assert!(
            !w.borrow().killed.contains(&a_busy.port) && !w.borrow().killed.contains(&a_public)
        );
        // The occupied old B room follows the documented cap: closed when the grace period ends, not before.
        m.tick(3500 + 29 * 60_000, &reg);
        assert!(!w.borrow().killed.contains(&b_busy.port));
        m.tick(3000 + 30 * 60_000, &reg);
        assert!(w.borrow().killed.contains(&b_busy.port));
        assert!(
            !w.borrow().killed.contains(&a_busy.port),
            "and A's occupied room is still running"
        );
    }

    #[test]
    fn a_full_pool_or_a_busy_port_makes_replacing_fail_honestly_unless_the_old_public_room_is_empty(
    ) {
        let mut c = cfg();
        c.pool_size = 2;
        c.max_processes = 2;
        let (mut m, w) = manager(c);
        let a = entry("kart", 2);
        let b_new = entry("deadfall", 9);
        let mut reg = registry(&[a, entry("deadfall", 1)]);
        m.tick(0, &reg);
        assert_eq!(m.processes(), 2, "both Public rooms use the whole pool");
        let b_public = m.rooms_of("deadfall")[0].port;
        w.borrow_mut()
            .status
            .insert(b_public, status(2, RoomState::Playing));
        m.tick(1000, &reg);
        let e = m.replace_game(&b_new, 2000).unwrap_err();
        assert!(
            e.contains("room processes are in use") && e.contains("nothing was changed"),
            "{e}"
        );
        assert_eq!(m.retired_of("deadfall"), 0);
        assert_eq!(m.processes(), 2);

        // The old Public room is empty: retiring it frees the slot, so an update is still possible on a full pool.
        w.borrow_mut()
            .status
            .insert(b_public, status(0, RoomState::Lobby));
        m.tick(2000, &reg);
        let n = m.replace_game(&b_new, 3000).unwrap();
        assert_eq!(n, 1);
        reg.add(b_new.clone());
        m.tick(3000, &reg);
        let rooms = m.rooms_of("deadfall");
        assert!(
            rooms.len() == 1 && rooms[0].public,
            "a new Public room replaced the empty one: {rooms:?}"
        );
        assert_eq!(
            names(&m, "kart"),
            ["Public"],
            "the other game's room never moved"
        );

        // A process slot is free but every pool port is held by another program: the message says so.
        let mut c = cfg();
        c.pool_size = 2;
        let (mut m, w) = manager(c);
        m.tick(0, &registry(&[entry("deadfall", 1)]));
        w.borrow_mut().busy_ports = vec![5001];
        let e = m.replace_game(&b_new, 10).unwrap_err();
        assert!(e.contains("no pool port is free"), "{e}");
        assert_eq!(names(&m, "deadfall"), ["Public"]);
    }

    #[test]
    fn the_public_room_reports_a_fresh_status_only_once_its_process_has_printed_one() {
        let (mut m, w) = manager(cfg());
        let d = entry("deadfall", 1);
        let reg = registry(std::slice::from_ref(&d));
        assert!(m.public_room("deadfall").is_none());
        m.tick(0, &reg);
        let p = m.public_room("deadfall").unwrap();
        assert!(p.status.is_none(), "started, nothing printed yet");
        let mut s = status(0, RoomState::Lobby);
        s.build = d.info.build;
        w.borrow_mut().status.insert(p.port, s);
        m.tick(500, &reg);
        assert_eq!(
            m.public_room("deadfall").unwrap().status.unwrap().build,
            d.info.build
        );
        // A status the server stopped printing is not trusted (the real process drops it after a few seconds).
        w.borrow_mut().status.clear();
        m.tick(900, &reg);
        assert!(m.public_room("deadfall").unwrap().status.is_none());
        m.retire_game("deadfall", 1000);
        assert!(
            m.public_room("deadfall").is_none(),
            "a retired room is not the Public room"
        );
    }

    #[test]
    fn shutdown_and_drop_kill_every_child() {
        let (mut m, w) = manager(cfg());
        let d = entry("deadfall", 1);
        m.tick(0, &registry(std::slice::from_ref(&d)));
        let user = m.create(&d, "Mine", vec![], ip(1), 0).unwrap();
        m.retire_game("deadfall", 0);
        let ports: Vec<u16> = vec![5000, user.port];
        m.shutdown();
        let mut killed = w.borrow().killed.clone();
        killed.sort();
        assert_eq!(killed, ports, "retired rooms die too");
        assert_eq!(m.processes(), 0);
        let (m2, w2) = manager(cfg());
        {
            let mut m2 = m2;
            m2.create(&d, "x", vec![], ip(1), 0).unwrap();
        }
        assert_eq!(
            w2.borrow().killed.len(),
            1,
            "dropping the manager kills the children"
        );
    }
}
