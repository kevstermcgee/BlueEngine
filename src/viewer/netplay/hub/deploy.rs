//! What the updater (`deploy/hub/update.sh`) asks of the hub: check a candidate server before it replaces the
//! installed one (`be2-hub verify`), and read back what the running hub actually has for a game (`be2-hub status`).
//!
//! # Verify
//!
//! `deploy/hub/update.sh` builds a game's server somewhere private and asks this module whether the hub would
//! accept it. The rules are the ones the running hub applies when it loads a game ([`Registry::load`]: `--info`
//! parses, the settings schema is valid, the config's `public_set`/`user_set`/`client_settings` still resolve
//! against it, a `game=` that differs from the config id is a warning), run on the candidate path instead of the
//! installed one. Then, optionally, the candidate is started the way the hub would start its Public room, on a
//! loopback ephemeral port with its own report directory, and must print its first `STATUS` line (with the same
//! build id its `--info` promised) before a deadline; it is then stopped and reaped.
//!
//! What this does **not** prove: that the hub has a free pool port or process slot, or that real players can join.
//! `be2-hub status` (the hub's own view of the running Public room) covers the former after activation.
//!
//! # Status
//! A reload acknowledgement only says the hub re-read the game and asked for a replacement room. [`GameStatus`] is
//! the hub's answer to "what is running now": the build its registry holds, and whether the Public room's process
//! is up and has printed a fresh `STATUS` line (and with which build). [`GameStatus::readiness`] turns that, and
//! the build the updater expects, into one word the updater records.
use super::registry::{Config, InfoSource, Registry};
use super::rooms::PublicRoom;
use super::spawn::{RoomProcess, RoomSpec, RoomStatus, Spawner};
use crate::viewer::netplay::cli::Info;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long a candidate has to print its first `STATUS` line (it prints one a second).
pub const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// What to check besides `--info`.
#[derive(Clone, Debug, Default)]
pub struct VerifyOptions {
    /// Also start the candidate and wait this long for its first `STATUS` line.
    pub startup: Option<Duration>,
    /// Where the updater will install the candidate; the config's `server =` must be this very file, or the hub
    /// would never use the new build.
    pub installs_to: Option<PathBuf>,
}

/// What a successful check found.
#[derive(Clone, Debug)]
pub struct Report {
    pub info: Info,
    /// Registry warnings (for example the server's `game=` differs from the config id).
    pub warnings: Vec<String>,
    /// The first status of the isolated start, when one was requested.
    pub startup: Option<RoomStatus>,
}

impl Report {
    /// `key=value` lines for scripts (`deploy/hub/update.sh` reads `build=`, `game=`, `max_seats=`, `settings=`).
    pub fn to_text(&self) -> String {
        let mut t = format!(
            "game={}\nbuild={:08x}\nfingerprint={:08x}\nmax_seats={}\nsettings={}\n",
            self.info.game,
            self.info.build,
            self.info.fingerprint,
            self.info.max_seats,
            self.info.settings.len()
        );
        for w in &self.warnings {
            t.push_str(&format!("warning={w}\n"));
        }
        match &self.startup {
            Some(s) => t.push_str(&format!(
                "startup=ok players={} max={} build={:08x}\n",
                s.players, s.max, s.build
            )),
            None => t.push_str("startup=not-checked\n"),
        }
        t
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        // The destination may not exist yet (first install): compare the paths as written.
        _ => a == b,
    }
}

/// Apply the registry's rules to `candidate` as the server of `game` in `config`, and start it if asked.
pub fn verify_candidate(
    config: &Config,
    game: &str,
    candidate: &Path,
    src: &mut dyn InfoSource,
    spawner: &mut dyn Spawner,
    opts: &VerifyOptions,
) -> Result<Report, String> {
    let Some(g) = config.games.iter().find(|g| g.id == game) else {
        return Err(format!("{game} is not in the config"));
    };
    if let Some(dest) = &opts.installs_to {
        if !same_file(dest, &g.server) {
            return Err(format!(
                "the config runs {game} from {} but the updater installs to {}: the hub would never use the new build \
                 (make the config's server = path and the installed path the same file)",
                g.server.display(),
                dest.display()
            ));
        }
    }
    let mut candidate_game = g.clone();
    candidate_game.server = candidate.to_path_buf();
    let one = Config {
        hub: Default::default(),
        games: vec![candidate_game],
    };
    let (registry, problems) = Registry::load(&one, src);
    let Some(entry) = registry.get(game) else {
        return Err(problems.join("; "));
    };
    let mut report = Report {
        info: entry.info.clone(),
        warnings: problems,
        startup: None,
    };
    if let Some(limit) = opts.startup {
        let report_dir = scratch_dir()?;
        let spec = RoomSpec {
            game: game.to_string(),
            name: entry.config.public_name.clone(),
            port: 0,
            listen: "127.0.0.1:0".into(),
            server: candidate.to_path_buf(),
            settings: entry.public_settings.clone(),
            transport: entry.config.transport.clone(),
            auto_start: entry.config.auto_start,
            report_dir: Some(report_dir.clone()),
            public: true,
        };
        let outcome = start_and_wait(spawner, &spec, limit);
        let _ = std::fs::remove_dir_all(&report_dir);
        let status = outcome?;
        if status.build != entry.info.build {
            return Err(format!(
                "the candidate's STATUS line says build {:08x} but its --info says {:08x}",
                status.build, entry.info.build
            ));
        }
        report.startup = Some(status);
    }
    Ok(report)
}

/// Start one process, wait for its first status or its death or the deadline, and always stop and reap it.
fn start_and_wait(
    spawner: &mut dyn Spawner,
    spec: &RoomSpec,
    limit: Duration,
) -> Result<RoomStatus, String> {
    let mut process = spawner
        .spawn(spec)
        .map_err(|e| format!("the candidate could not be started: {e}"))?;
    let result = wait_for_status(process.as_mut(), limit);
    process.kill();
    result
}

fn wait_for_status(process: &mut dyn RoomProcess, limit: Duration) -> Result<RoomStatus, String> {
    let started = Instant::now();
    loop {
        if let Some(status) = process.status() {
            return Ok(status);
        }
        if process.exited() {
            // A last look: the status line may have been read just before the exit.
            return process.status().ok_or_else(|| {
                "the candidate exited before reporting its first STATUS line (its own output is above)"
                    .to_string()
            });
        }
        if started.elapsed() >= limit {
            return Err(format!(
                "the candidate printed no STATUS line within {} s",
                limit.as_secs().max(1)
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Whether a game's Public room exists in the hub.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicState {
    /// The game has no Public room (`public = off`): there is no process to observe.
    Off,
    /// It should have one and does not (not started yet, or waiting to restart).
    Missing,
    /// A process was started but has not reported a fresh `STATUS` line.
    Starting,
    /// The process reports a fresh `STATUS` line.
    Up,
}

impl PublicState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Missing => "missing",
            Self::Starting => "starting",
            Self::Up => "up",
        }
    }
}

/// What the hub currently holds for one game (the payload of a control `Status` reply, as `key=value` words).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameStatus {
    pub game: String,
    /// The build id of the server file the registry loaded.
    pub registry_build: u32,
    pub public: PublicState,
    pub public_port: Option<u16>,
    /// The build the running Public room's process itself reports.
    pub public_build: Option<u32>,
    /// Rooms of this game still draining after a reload.
    pub retired: usize,
    /// Listed (not retired) rooms of this game.
    pub rooms: usize,
}

/// The one word the updater records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Readiness {
    /// The Public room's process is up and reports the expected build.
    Ready,
    /// The registry holds the expected build, but the game has no Public room, so no running process can confirm it.
    RegistryOnly,
    /// The Public room was started and has not reported yet.
    Starting,
    /// The game should have a Public room and has none.
    Missing,
    /// The registry or the running room has a different build than expected.
    WrongBuild,
}

impl Readiness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::RegistryOnly => "registry-only",
            Self::Starting => "starting",
            Self::Missing => "missing",
            Self::WrongBuild => "wrong-build",
        }
    }

    /// Nothing more to wait for.
    pub fn settled(self) -> bool {
        matches!(self, Self::Ready | Self::RegistryOnly)
    }
}

impl GameStatus {
    pub fn to_text(&self) -> String {
        let mut t = format!(
            "game={} registry_build={:08x} public={}",
            self.game,
            self.registry_build,
            self.public.as_str()
        );
        if let Some(p) = self.public_port {
            t.push_str(&format!(" public_port={p}"));
        }
        if let Some(b) = self.public_build {
            t.push_str(&format!(" public_build={b:08x}"));
        }
        t.push_str(&format!(" retired={} rooms={}", self.retired, self.rooms));
        t
    }

    /// `None` unless every word is well formed.
    pub fn parse(text: &str) -> Option<Self> {
        let mut s = GameStatus {
            game: String::new(),
            registry_build: 0,
            public: PublicState::Off,
            public_port: None,
            public_build: None,
            retired: 0,
            rooms: 0,
        };
        let (mut have_game, mut have_build, mut have_public) = (false, false, false);
        for word in text.split_whitespace() {
            let (k, v) = word.split_once('=')?;
            match k {
                "game" => {
                    s.game = v.to_string();
                    have_game = true;
                }
                "registry_build" => {
                    s.registry_build = u32::from_str_radix(v, 16).ok()?;
                    have_build = true;
                }
                "public" => {
                    s.public = match v {
                        "off" => PublicState::Off,
                        "missing" => PublicState::Missing,
                        "starting" => PublicState::Starting,
                        "up" => PublicState::Up,
                        _ => return None,
                    };
                    have_public = true;
                }
                "public_port" => s.public_port = Some(v.parse().ok()?),
                "public_build" => s.public_build = Some(u32::from_str_radix(v, 16).ok()?),
                "retired" => s.retired = v.parse().ok()?,
                "rooms" => s.rooms = v.parse().ok()?,
                _ => {}
            }
        }
        (have_game && have_build && have_public).then_some(s)
    }

    /// From the room table's view of the game's Public room.
    pub fn of(
        game: &str,
        registry_build: u32,
        wants_public: bool,
        public: Option<PublicRoom>,
        retired: usize,
        rooms: usize,
    ) -> Self {
        let (state, port, build) = match (wants_public, public) {
            (false, _) => (PublicState::Off, None, None),
            (true, None) => (PublicState::Missing, None, None),
            (true, Some(r)) => match r.status {
                Some(s) => (PublicState::Up, Some(r.port), Some(s.build)),
                None => (PublicState::Starting, Some(r.port), None),
            },
        };
        GameStatus {
            game: game.to_string(),
            registry_build,
            public: state,
            public_port: port,
            public_build: build,
            retired,
            rooms,
        }
    }

    /// Is the game running the build the updater installed? `expect` is the candidate's `--info` build; without
    /// it the hub's own registry build is the reference.
    pub fn readiness(&self, expect: Option<u32>) -> Readiness {
        let want = expect.unwrap_or(self.registry_build);
        if self.registry_build != want {
            return Readiness::WrongBuild;
        }
        match self.public {
            PublicState::Off => Readiness::RegistryOnly,
            PublicState::Missing => Readiness::Missing,
            PublicState::Starting => Readiness::Starting,
            PublicState::Up if self.public_build == Some(want) => Readiness::Ready,
            PublicState::Up => Readiness::WrongBuild,
        }
    }
}

fn scratch_dir() -> Result<PathBuf, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("be2hub-verify-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::super::registry::{
        parse_config,
        tests::{fake_info, Scripted},
    };
    use super::super::rooms::tests::{status, Fake, World};
    use super::super::wire::RoomState;
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn config(extra: &str) -> Config {
        parse_config(
            &format!("[game kart]\nserver=/srv/kart-server\npublic=on\n{extra}\n"),
            Path::new("/etc/be2"),
        )
        .unwrap()
    }

    fn src(path: &str, info: Result<Info, String>) -> Scripted {
        let mut s = Scripted::default();
        s.put(path, 1, info);
        s
    }

    fn fake() -> (Fake, Rc<RefCell<World>>) {
        let w = Rc::new(RefCell::new(World::default()));
        (Fake(w.clone()), w)
    }

    /// A spawner whose process has already exited (a binary that crashes at startup).
    struct Crashing;
    struct Dead;
    impl Spawner for Crashing {
        fn spawn(&mut self, _: &RoomSpec) -> std::io::Result<Box<dyn RoomProcess>> {
            Ok(Box::new(Dead))
        }
    }
    impl RoomProcess for Dead {
        fn status(&mut self) -> Option<RoomStatus> {
            None
        }
        fn exited(&mut self) -> bool {
            true
        }
        fn kill(&mut self) {}
    }

    fn with_build(mut s: RoomStatus, info: &Info) -> RoomStatus {
        s.build = info.build;
        s
    }

    #[test]
    fn a_good_candidate_passes_the_registry_rules_and_starts() {
        let (mut spawner, w) = fake();
        let info = fake_info("kart", 7);
        w.borrow_mut()
            .status
            .insert(0, with_build(status(0, RoomState::Lobby), &info));
        let mut s = src("/new/kart", Ok(info.clone()));
        let opts = VerifyOptions {
            startup: Some(Duration::from_secs(1)),
            installs_to: None,
        };
        let r = verify_candidate(
            &config(""),
            "kart",
            Path::new("/new/kart"),
            &mut s,
            &mut spawner,
            &opts,
        )
        .unwrap();
        assert_eq!(r.info.build, info.build);
        assert!(r.warnings.is_empty());
        assert!(r.to_text().contains(&format!("build={:08x}", info.build)));
        assert!(r.to_text().contains("startup=ok"));
        let spawned = w.borrow().spawned.clone();
        assert_eq!(spawned.len(), 1);
        assert_eq!(
            spawned[0].server,
            PathBuf::from("/new/kart"),
            "the candidate, not the installed file"
        );
        assert_eq!(spawned[0].listen, "127.0.0.1:0", "loopback, ephemeral");
        assert!(spawned[0]
            .report_dir
            .as_ref()
            .unwrap()
            .starts_with(std::env::temp_dir()));
        assert_eq!(
            w.borrow().killed,
            vec![0],
            "stopped again, nothing left running"
        );
    }

    #[test]
    fn invalid_information_is_refused_with_the_registry_rules_and_nothing_is_started() {
        let (mut spawner, w) = fake();
        let opts = VerifyOptions {
            startup: Some(Duration::from_secs(1)),
            installs_to: None,
        };
        let v = |cfg: &Config, game: &str, info: Result<Info, String>, spawner: &mut Fake| {
            verify_candidate(
                cfg,
                game,
                Path::new("/new/kart"),
                &mut src("/new/kart", info),
                spawner,
                &opts,
            )
        };
        // --info fails or is garbage.
        let e = v(
            &config(""),
            "kart",
            Err("--info exited with status: 1".into()),
            &mut spawner,
        )
        .unwrap_err();
        assert!(e.contains("exited"), "{e}");
        // The config names a setting the candidate no longer has.
        let e = v(
            &config("user_set=wat=3"),
            "kart",
            Ok(fake_info("kart", 7)),
            &mut spawner,
        )
        .unwrap_err();
        assert!(e.contains("wat") && e.contains("does not have"), "{e}");
        // A value outside the candidate's range.
        let e = v(
            &config("public_set=kills=9999"),
            "kart",
            Ok(fake_info("kart", 7)),
            &mut spawner,
        )
        .unwrap_err();
        assert!(e.contains("public_set"), "{e}");
        // A game the config does not carry.
        let e = v(
            &config(""),
            "ghost",
            Ok(fake_info("ghost", 7)),
            &mut spawner,
        )
        .unwrap_err();
        assert!(e.contains("not in the config"), "{e}");
        assert!(
            w.borrow().spawned.is_empty(),
            "no process was started for an invalid candidate"
        );
    }

    #[test]
    fn a_game_name_that_differs_from_the_config_id_is_a_warning_as_in_the_hub() {
        let (mut spawner, _) = fake();
        let r = verify_candidate(
            &config(""),
            "kart",
            Path::new("/new/kart"),
            &mut src("/new/kart", Ok(fake_info("kart-renamed", 7))),
            &mut spawner,
            &VerifyOptions::default(),
        )
        .unwrap();
        assert_eq!(r.warnings.len(), 1);
        assert!(r.warnings[0].contains("kart-renamed") && r.to_text().contains("warning="));
        assert!(r.to_text().contains("startup=not-checked"));
    }

    #[test]
    fn the_installed_path_must_be_the_one_the_config_runs() {
        let (mut spawner, _) = fake();
        let mut s = src("/new/kart", Ok(fake_info("kart", 7)));
        let wrong = VerifyOptions {
            startup: None,
            installs_to: Some("/home/me/blueengine/kart-server".into()),
        };
        let e = verify_candidate(
            &config(""),
            "kart",
            Path::new("/new/kart"),
            &mut s,
            &mut spawner,
            &wrong,
        )
        .unwrap_err();
        assert!(
            e.contains("never use the new build") && e.contains("/srv/kart-server"),
            "{e}"
        );
        let right = VerifyOptions {
            startup: None,
            installs_to: Some("/srv/kart-server".into()),
        };
        assert!(verify_candidate(
            &config(""),
            "kart",
            Path::new("/new/kart"),
            &mut s,
            &mut spawner,
            &right
        )
        .is_ok());
    }

    #[test]
    fn a_candidate_that_dies_stays_silent_or_lies_about_its_build_is_refused_and_reaped() {
        let opts = VerifyOptions {
            startup: Some(Duration::from_millis(300)),
            installs_to: None,
        };
        let info = fake_info("kart", 7);
        // Spawn fails (not executable).
        let (mut spawner, w) = fake();
        w.borrow_mut().fail_spawn = true;
        let e = verify_candidate(
            &config(""),
            "kart",
            Path::new("/new/kart"),
            &mut src("/new/kart", Ok(info.clone())),
            &mut spawner,
            &opts,
        )
        .unwrap_err();
        assert!(e.contains("could not be started"), "{e}");
        // Exits at once.
        let e = verify_candidate(
            &config(""),
            "kart",
            Path::new("/new/kart"),
            &mut src("/new/kart", Ok(info.clone())),
            &mut Crashing,
            &opts,
        )
        .unwrap_err();
        assert!(e.contains("exited before"), "{e}");
        // Alive but never reports: bounded by the deadline.
        let (mut spawner, w) = fake();
        let started = Instant::now();
        let e = verify_candidate(
            &config(""),
            "kart",
            Path::new("/new/kart"),
            &mut src("/new/kart", Ok(info.clone())),
            &mut spawner,
            &opts,
        )
        .unwrap_err();
        assert!(e.contains("no STATUS line"), "{e}");
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(w.borrow().killed, vec![0], "reaped after the deadline");
        // Reports a different build than --info promised.
        let (mut spawner, w) = fake();
        w.borrow_mut().status.insert(0, status(0, RoomState::Lobby)); // build 0
        let e = verify_candidate(
            &config(""),
            "kart",
            Path::new("/new/kart"),
            &mut src("/new/kart", Ok(info)),
            &mut spawner,
            &opts,
        )
        .unwrap_err();
        assert!(e.contains("STATUS line says build"), "{e}");
        assert_eq!(w.borrow().killed, vec![0]);
    }

    #[test]
    fn game_status_round_trips_and_readiness_compares_with_the_expected_build() {
        let st = GameStatus::of(
            "kart",
            0xAB,
            true,
            Some(PublicRoom {
                port: 4105,
                status: Some(with_build(
                    status(1, RoomState::Lobby),
                    &fake_info("kart", 0xAB),
                )),
            }),
            2,
            3,
        );
        let text = st.to_text();
        assert!(text.len() < 200, "{text}");
        assert_eq!(GameStatus::parse(&text), Some(st.clone()));
        assert_eq!(st.public, PublicState::Up);
        // The room prints the build of --info (fold of the fingerprint); the registry holds 0xAB: they differ here.
        assert_eq!(st.readiness(Some(0xAB)), Readiness::WrongBuild);
        let ok = GameStatus {
            registry_build: 7,
            public_build: Some(7),
            ..st.clone()
        };
        assert_eq!(ok.readiness(Some(7)), Readiness::Ready);
        assert_eq!(ok.readiness(None), Readiness::Ready);
        assert_eq!(
            ok.readiness(Some(8)),
            Readiness::WrongBuild,
            "the hub holds another build"
        );
        let starting = GameStatus::of(
            "kart",
            7,
            true,
            Some(PublicRoom {
                port: 1,
                status: None,
            }),
            0,
            1,
        );
        assert_eq!(starting.readiness(Some(7)), Readiness::Starting);
        assert!(!starting.readiness(Some(7)).settled());
        let missing = GameStatus::of("kart", 7, true, None, 0, 0);
        assert_eq!(missing.readiness(Some(7)), Readiness::Missing);
        let off = GameStatus::of("kart", 7, false, None, 0, 0);
        assert_eq!(off.readiness(Some(7)), Readiness::RegistryOnly);
        assert!(
            off.readiness(Some(7)).settled(),
            "nothing more can be observed"
        );
        for bad in [
            "",
            "game=x",
            "game=x registry_build=zz public=up",
            "game=x registry_build=00000001 public=maybe",
            "garbage",
        ] {
            assert!(GameStatus::parse(bad).is_none(), "{bad:?}");
        }
    }
}
