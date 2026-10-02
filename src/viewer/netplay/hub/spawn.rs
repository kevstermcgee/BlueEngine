//! Starting and watching one room's server process.
//!
//! Every room is its own process on its own UDP port: match settings are process-global in the games, and a
//! crash or a stuck room must never take another room down. The hub starts the game's server (the path comes from
//! the registry, never from the wire) with a command line built only from validated typed values, holds the
//! child's stdin open (the server quits when it closes, so a hub that is killed leaves no orphans), and reads the
//! `STATUS` lines the server prints once a second.
//!
//! The processes sit behind two small traits ([`Spawner`], [`RoomProcess`]) so the room logic in
//! [`rooms`](super::rooms) is tested with fakes and the real thing is tested with real servers.
use super::wire::RoomState;
use crate::viewer::netplay::cli::{Status, StatusStage};
use std::io;
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A status older than this is not trusted (the server prints one a second).
pub const STATUS_FRESH: Duration = Duration::from_secs(5);

/// What a room's server reports about itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoomStatus {
    pub players: u8,
    /// Participants per match: the room's capacity.
    pub max: u8,
    pub state: RoomState,
    /// The game's build id as the server computed it.
    pub build: u32,
}

impl RoomStatus {
    /// From a parsed `STATUS` line. The results screen counts as the lobby: the next match is already forming.
    pub fn from_status(s: &Status) -> RoomStatus {
        RoomStatus {
            players: s.players,
            max: s.max,
            state: match s.stage {
                StatusStage::Match => RoomState::Playing,
                StatusStage::Lobby | StatusStage::Results => RoomState::Lobby,
            },
            build: s.build,
        }
    }
}

/// Everything a [`Spawner`] needs to start one room's server.
#[derive(Clone, Debug)]
pub struct RoomSpec {
    pub game: String,
    pub name: String,
    pub port: u16,
    /// The address the server binds, e.g. `0.0.0.0:4102`.
    pub listen: String,
    pub server: PathBuf,
    /// Every setting of the game and its value (`--set id=value` each), from the validated schema.
    pub settings: Vec<(u8, u32)>,
    /// `development` or `production`.
    pub transport: String,
    pub auto_start: Option<u32>,
    pub report_dir: Option<PathBuf>,
    pub public: bool,
}

/// The command-line arguments for a room: only values the hub itself validated (numbers, a port, a path of its own).
pub fn args(spec: &RoomSpec) -> Vec<String> {
    let mut a = vec![
        "--listen".to_string(),
        spec.listen.clone(),
        "--transport".into(),
        spec.transport.clone(),
        "--status-lines".into(),
        "--exit-on-stdin-eof".into(),
    ];
    if let Some(s) = spec.auto_start {
        a.extend(["--auto-start".to_string(), s.to_string()]);
    }
    for (id, value) in &spec.settings {
        a.extend(["--set".to_string(), format!("{id}={value}")]);
    }
    if let Some(dir) = &spec.report_dir {
        a.extend(["--report-dir".to_string(), dir.display().to_string()]);
    }
    a
}

/// A running room server, as the manager sees it.
pub trait RoomProcess {
    /// The latest status the server reported, if it is recent.
    fn status(&mut self) -> Option<RoomStatus>;
    /// The process has ended.
    fn exited(&mut self) -> bool;
    /// End the process and wait for it.
    fn kill(&mut self);
}

/// Starts room servers. The real one runs the game's executable; tests use a fake.
pub trait Spawner {
    fn spawn(&mut self, spec: &RoomSpec) -> io::Result<Box<dyn RoomProcess>>;
    /// Is nothing else using this UDP port on `ip`? (A port taken by another program is skipped, not handed out.)
    fn port_free(&mut self, ip: &str, port: u16) -> bool {
        let _ = (ip, port);
        true
    }
}

struct ChildRoom {
    child: Child,
    /// The newest status line and when it arrived.
    status: Arc<Mutex<Option<(RoomStatus, Instant)>>>,
}

impl RoomProcess for ChildRoom {
    fn status(&mut self) -> Option<RoomStatus> {
        let s = self.status.lock().ok()?;
        s.filter(|(_, at)| at.elapsed() < STATUS_FRESH)
            .map(|(s, _)| s)
    }
    fn exited(&mut self) -> bool {
        !matches!(self.child.try_wait(), Ok(None))
    }
    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ChildRoom {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Runs the registered executable for each room.
#[derive(Default)]
pub struct ProcessSpawner;

impl Spawner for ProcessSpawner {
    fn spawn(&mut self, spec: &RoomSpec) -> io::Result<Box<dyn RoomProcess>> {
        if let Some(dir) = &spec.report_dir {
            // The server creates it too, but a hub that cannot write there should say so now, not per match.
            std::fs::create_dir_all(dir)?;
        }
        let mut child = Command::new(&spec.server)
            .args(args(spec))
            // The hub holds stdin open; the server exits when it closes, so a hub that is killed leaves no orphans.
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", spec.server.display())))?;
        let status = Arc::new(Mutex::new(None));
        if let Some(out) = child.stdout.take() {
            let shared = status.clone();
            std::thread::spawn(move || {
                use std::io::{BufRead, BufReader};
                // Always drain the pipe (a full pipe would stall the server); keep only the status lines.
                for line in BufReader::new(out).lines() {
                    let Ok(line) = line else { break };
                    if let Some(s) = Status::parse_line(&line) {
                        if let Ok(mut g) = shared.lock() {
                            *g = Some((RoomStatus::from_status(&s), Instant::now()));
                        }
                    }
                }
            });
        }
        Ok(Box::new(ChildRoom { child, status }))
    }

    fn port_free(&mut self, ip: &str, port: u16) -> bool {
        match ip.parse::<IpAddr>() {
            Ok(ip) => std::net::UdpSocket::bind((ip, port)).is_ok(),
            Err(_) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> RoomSpec {
        RoomSpec {
            game: "deadfall".into(),
            name: "Mine".into(),
            port: 4102,
            listen: "0.0.0.0:4102".into(),
            server: "/srv/deadfall-server".into(),
            settings: vec![(1, 1), (2, 25)],
            transport: "development".into(),
            auto_start: Some(30),
            report_dir: Some("/var/reports/deadfall/port-4102".into()),
            public: false,
        }
    }

    #[test]
    fn server_arguments_carry_the_validated_settings_and_nothing_typed_by_a_player() {
        assert_eq!(
            args(&spec()),
            [
                "--listen",
                "0.0.0.0:4102",
                "--transport",
                "development",
                "--status-lines",
                "--exit-on-stdin-eof",
                "--auto-start",
                "30",
                "--set",
                "1=1",
                "--set",
                "2=25",
                "--report-dir",
                "/var/reports/deadfall/port-4102"
            ]
        );
        let bare = RoomSpec {
            settings: vec![],
            auto_start: None,
            report_dir: None,
            ..spec()
        };
        assert_eq!(args(&bare).len(), 6);
        // The room's name (player text) is not an argument at all.
        let named = RoomSpec {
            name: "--listen 1.2.3.4:5; rm -rf".into(),
            ..spec()
        };
        assert!(args(&named).iter().all(|a| !a.contains("rm -rf")));
    }

    #[test]
    fn status_maps_stages_to_room_states() {
        let line = |stage: StatusStage| Status {
            game: "g".into(),
            players: 3,
            max: 8,
            stage,
            build: 7,
        };
        assert_eq!(
            RoomStatus::from_status(&line(StatusStage::Lobby)).state,
            RoomState::Lobby
        );
        assert_eq!(
            RoomStatus::from_status(&line(StatusStage::Results)).state,
            RoomState::Lobby
        );
        let m = RoomStatus::from_status(&line(StatusStage::Match));
        assert_eq!(
            (m.state, m.players, m.max, m.build),
            (RoomState::Playing, 3, 8, 7)
        );
    }

    #[test]
    fn the_real_spawner_skips_ports_in_use() {
        let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = held.local_addr().unwrap().port();
        let mut s = ProcessSpawner;
        assert!(!s.port_free("127.0.0.1", port));
        drop(held);
        assert!(s.port_free("127.0.0.1", port));
    }
}
