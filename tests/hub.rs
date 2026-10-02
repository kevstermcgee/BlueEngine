//! The hub over real loopback UDP with real server processes: a hub (in a thread, and as the `be2-hub` binary),
//! the `be2-toy-server` binary as every game's server, a `HubClient` asking for rooms, and two real netplay clients
//! joining the room it made. Loopback only; ports come from 43000-44999 and are checked free before use, and the
//! live Deadfall hub's ports (4100-4107) are never touched.
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use vesper3d::viewer::net::{client_transport, TransportProfile};
use vesper3d::viewer::netplay::cli::{build_id, Info, Status, StatusStage};
use vesper3d::viewer::netplay::hub::legacy;
use vesper3d::viewer::netplay::hub::registry::Registry;
use vesper3d::viewer::netplay::hub::spawn::{RoomProcess, RoomSpec, RoomStatus, Spawner};
use vesper3d::viewer::netplay::hub::wire::{
    Control, ControlReply, ErrorCode, Reply, Request, RoomInfo, RoomState,
};
use vesper3d::viewer::netplay::hub::{
    self, build_matches, local_build, parse_config, room_addr, Hub, HubClient, HubEvent,
    HubOptions, Limits, ManagerConfig, ProcessInfo, ProcessSpawner,
};
use vesper3d::viewer::netplay::toy::{ToyGame, ToyInput};
use vesper3d::viewer::netplay::{ClientConfig, ClientState, NetClient, NetGame};

const TOY_SERVER: &str = env!("CARGO_BIN_EXE_be2-toy-server");
const HUB_BIN: &str = env!("CARGO_BIN_EXE_be2-hub");

// ---- helpers --------------------------------------------------------------------------------------------------------

/// A fresh run of `n + 1` consecutive loopback UDP ports that are free right now (the hub's, then the pool).
/// Tests in one process get disjoint runs; the bounded search skips ports something else holds.
fn free_ports(n: u16) -> u16 {
    static NEXT: AtomicU16 = AtomicU16::new(0);
    // Mix the clock into the start so two test processes started together (several suites on one machine) rarely
    // pick the same run: the pid alone gave only 40 distinct starts.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let seed = ((std::process::id().wrapping_mul(2_654_435_761) ^ nanos) % 1_900) as u16;
    for _ in 0..200 {
        let step = NEXT.fetch_add(1, Ordering::SeqCst);
        let base = 43_000 + (seed + step * 24) % 1_900;
        let held: Vec<_> = (0..=n)
            .filter_map(|i| UdpSocket::bind(("127.0.0.1", base + i)).ok())
            .collect();
        if held.len() == n as usize + 1 {
            return base;
        }
    }
    panic!("no free run of {} loopback ports in 43000-44999", n + 1);
}

struct TempDir(PathBuf);
impl TempDir {
    fn new(tag: &str) -> TempDir {
        static N: AtomicU16 = AtomicU16::new(0);
        let dir = std::env::temp_dir().join(format!(
            "be2hub-it-{}-{tag}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A config with the toy server registered as `toy-footrace` (and optionally as `deadfall`, for the legacy protocol).
fn config_text(
    base: u16,
    pool: u16,
    dir: &Path,
    extra_game_lines: &str,
    legacy_deadfall: bool,
) -> String {
    let mut t = format!(
        "[hub]\nlisten = 127.0.0.1:{base}\npool_start = {}\npool_size = {pool}\nreport_dir = {}\n\
         rate_burst = 1000\nrate_per_sec = 1000\nmax_rooms_per_ip = 10\n\n\
         [game toy-footrace]\nserver = {TOY_SERVER}\npublic = on\nauto_start = 0\nmax_rooms = 3\n{extra_game_lines}\n",
        base + 1,
        dir.join("reports").display()
    );
    if legacy_deadfall {
        t.push_str(&format!(
            "\n[game deadfall]\nserver = {TOY_SERVER}\npublic = on\nauto_start = 0\nmax_rooms = 3\nuser_set = ai-speed=100\n"
        ));
    }
    t
}

struct TestHub {
    addr: SocketAddr,
    pool: std::ops::Range<u16>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl TestHub {
    /// Run a hub on `127.0.0.1:base` from `config` text (written to `dir/hub.conf`), with the given spawner.
    fn start(
        dir: &Path,
        base: u16,
        pool: u16,
        config: &str,
        limits: Limits,
        legacy_mode: legacy::Mode,
        spawner: impl FnOnce() -> Box<dyn Spawner> + Send + 'static,
    ) -> TestHub {
        let conf_path = dir.join("hub.conf");
        std::fs::write(&conf_path, config).unwrap();
        let socket = UdpSocket::bind(("127.0.0.1", base)).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let report_dir = dir.join("reports");
        let thread = std::thread::spawn(move || {
            let text = std::fs::read_to_string(&conf_path).unwrap();
            let cfg = parse_config(&text, conf_path.parent().unwrap()).unwrap();
            let mut info = ProcessInfo::new();
            let (registry, problems) = Registry::load(&cfg, &mut info);
            assert!(
                problems.iter().all(|p| p.starts_with("warning")),
                "{problems:?}"
            );
            let opts = HubOptions {
                manager: ManagerConfig {
                    pool_start: base + 1,
                    pool_size: pool,
                    bind_ip: "127.0.0.1".into(),
                    report_dir: Some(report_dir),
                    max_processes: pool as usize,
                    public_restart_ms: 300,
                    // Short enough for a test, long enough for two clients to join a new room.
                    never_joined_timeout_ms: 4_000,
                    ..Default::default()
                },
                limits,
                legacy: legacy_mode,
                config_path: Some(conf_path),
            };
            let mut hub = Hub::new(opts, registry, spawner(), Box::new(info));
            hub::serve(&socket, &mut hub, &flag).unwrap();
        });
        TestHub {
            addr: ([127, 0, 0, 1], base).into(),
            pool: base + 1..base + 1 + pool,
            stop,
            thread: Some(thread),
        }
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
    }
}

impl Drop for TestHub {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Limits loose enough for a test that polls the hub many times a second.
fn relaxed() -> Limits {
    Limits {
        burst: 1000.,
        per_sec: 1000.,
        create_burst: 100.,
        max_rooms_per_ip: 10,
        ..Default::default()
    }
}

fn real_spawner() -> Box<dyn Spawner> {
    Box::new(ProcessSpawner)
}

fn wait_event(c: &mut HubClient) -> HubEvent {
    let t = Instant::now();
    loop {
        if let Some(e) = c.poll() {
            return e;
        }
        assert!(t.elapsed() < Duration::from_secs(10), "no hub event");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn list(c: &mut HubClient) -> Vec<RoomInfo> {
    c.request_list();
    match wait_event(c) {
        HubEvent::Rooms { build, rooms } => {
            assert!(
                build_matches::<ToyGame>(build),
                "the hub reports this game's build id"
            );
            rooms
        }
        other => panic!("expected rooms, got {other:?}"),
    }
}

/// Poll `check` until it holds or `secs` pass.
fn eventually(secs: u64, mut check: impl FnMut() -> bool) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(secs) {
        if check() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn port_is_free(port: u16) -> bool {
    UdpSocket::bind(("127.0.0.1", port)).is_ok()
}

fn client(
    server: SocketAddr,
    name: &str,
    choice: u8,
) -> NetClient<ToyGame, impl vesper3d::viewer::net::DatagramTransport> {
    NetClient::<ToyGame, _>::new(
        client_transport(TransportProfile::Development, server).unwrap(),
        server,
        ClientConfig {
            name: name.into(),
            key: String::new(),
            choice,
        },
    )
    .unwrap()
}

/// Poll `clients` until `done` holds; fail with their states after `secs`.
fn drive<T: vesper3d::viewer::net::DatagramTransport>(
    clients: &mut [NetClient<ToyGame, T>],
    secs: u64,
    mut done: impl FnMut(&[NetClient<ToyGame, T>]) -> bool,
) -> bool {
    let start = Instant::now();
    loop {
        let now = start.elapsed().as_secs_f64();
        for c in clients.iter_mut() {
            c.poll(now);
            if *c.state() == ClientState::Playing {
                c.tick(ToyInput { throttle: 1 });
            }
            c.frame(now, 1. / 60.);
        }
        if done(clients) {
            return true;
        }
        if start.elapsed() >= Duration::from_secs(secs) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn states<T: vesper3d::viewer::net::DatagramTransport>(
    clients: &[NetClient<ToyGame, T>],
) -> Vec<ClientState> {
    clients.iter().map(|c| c.state().clone()).collect()
}

// ---- a spawner that runs real servers but can be told to crash one ---------------------------------------------------

#[derive(Clone, Default)]
struct Crash(Arc<Mutex<Vec<u16>>>);
struct Crashable {
    inner: Box<dyn RoomProcess>,
    port: u16,
    crash: Crash,
}
struct CrashSpawner {
    inner: ProcessSpawner,
    crash: Crash,
    spawned: Arc<Mutex<Vec<u16>>>,
}
impl Spawner for CrashSpawner {
    fn spawn(&mut self, spec: &RoomSpec) -> io::Result<Box<dyn RoomProcess>> {
        self.spawned.lock().unwrap().push(spec.port);
        Ok(Box::new(Crashable {
            inner: self.inner.spawn(spec)?,
            port: spec.port,
            crash: self.crash.clone(),
        }))
    }
}
impl RoomProcess for Crashable {
    fn status(&mut self) -> Option<RoomStatus> {
        self.inner.status()
    }
    fn exited(&mut self) -> bool {
        let mut c = self.crash.0.lock().unwrap();
        if let Some(i) = c.iter().position(|p| *p == self.port) {
            c.remove(i);
            self.inner.kill(); // SIGKILL: the server dies without a word
        }
        drop(c);
        self.inner.exited()
    }
    fn kill(&mut self) {
        self.inner.kill();
    }
}

/// A spawner with no processes at all (for tests of the front door only).
struct Pretend;
struct PretendProc;
impl Spawner for Pretend {
    fn spawn(&mut self, _: &RoomSpec) -> io::Result<Box<dyn RoomProcess>> {
        Ok(Box::new(PretendProc))
    }
}
impl RoomProcess for PretendProc {
    fn status(&mut self) -> Option<RoomStatus> {
        None
    }
    fn exited(&mut self) -> bool {
        false
    }
    fn kill(&mut self) {}
}

// ---- the game server binary ---------------------------------------------------------------------------------------------

#[test]
fn the_server_binary_describes_itself_with_info() {
    let out = Command::new(TOY_SERVER).arg("--info").output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.starts_with("game=toy-footrace\nfingerprint="),
        "{text}"
    );
    let info = Info::parse(&text).unwrap();
    assert_eq!(info, Info::of::<ToyGame>());
    assert_eq!(info.fingerprint, ToyGame::fingerprint());
    assert_eq!(info.build, build_id::<ToyGame>());
    assert_eq!(info.build, local_build::<ToyGame>());
    assert_eq!((info.max_seats, info.tick_hz), (8, 60));
    assert_eq!(
        text.lines()
            .filter(|l| l.starts_with("setting="))
            .collect::<Vec<_>>(),
        [
            "setting=1:ai-speed:ai-speed:int:25:400:100",
            "setting=2:bots:bots:bool:0:1:1"
        ]
    );
}

#[test]
fn the_server_binary_refuses_bad_flags_and_settings_with_a_message_and_a_failing_exit() {
    for args in [
        &["--bogus"][..],
        &["--set", "1=9999"],
        &["--set", "9=1"],
        &["--set", "1"],
        &["--ai-speed"],
        &["--transport", "pigeon"],
    ] {
        let out = Command::new(TOY_SERVER)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(!out.status.success(), "{args:?}");
        assert!(!out.stderr.is_empty(), "{args:?} explained nothing");
    }
    let help = Command::new(TOY_SERVER).arg("--help").output().unwrap();
    assert!(
        help.status.success() && String::from_utf8_lossy(&help.stdout).contains("--status-lines")
    );
}

#[test]
fn the_server_prints_status_lines_and_exits_when_its_parent_goes_away() {
    use std::io::{BufRead, BufReader};
    let port = free_ports(0);
    let mut child = Command::new(TOY_SERVER)
        .args([
            "--listen",
            &format!("127.0.0.1:{port}"),
            "--status-lines",
            "--exit-on-stdin-eof",
            "--seats",
            "5",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let status = loop {
        let line = lines.next().expect("the server printed a status").unwrap();
        if let Some(s) = Status::parse_line(&line) {
            break s;
        }
    };
    assert_eq!(
        status,
        Status {
            game: "toy-footrace".into(),
            players: 0,
            max: 5,
            stage: StatusStage::Lobby,
            build: build_id::<ToyGame>()
        }
    );
    drop(child.stdin.take()); // what happens when the hub dies
    assert!(
        eventually(10, || child.try_wait().unwrap().is_some()),
        "the server quit when its stdin closed"
    );
}

// ---- the hub with real rooms --------------------------------------------------------------------------------------------

#[test]
fn a_room_made_through_the_hub_takes_two_real_players_with_the_settings_chosen_and_reports_them() {
    let (base, pool) = (free_ports(4), 4);
    let dir = TempDir::new("room");
    let mut hub = TestHub::start(
        dir.path(),
        base,
        pool,
        &config_text(base, pool, dir.path(), "", false),
        relaxed(),
        legacy::Mode::Serve,
        real_spawner,
    );
    let mut hc = HubClient::new(hub.addr, "toy-footrace").unwrap();

    let rooms = list(&mut hc);
    assert_eq!(rooms.len(), 1, "{rooms:?}");
    assert!(rooms[0].public && rooms[0].name == "Public" && hub.pool.contains(&rooms[0].port));
    assert_eq!(rooms[0].capacity, 8);

    // Another game id is not hosted here: a short, honest error.
    let mut other = HubClient::new(hub.addr, "no-such-game").unwrap();
    other.request_list();
    assert!(matches!(
        wait_event(&mut other),
        HubEvent::Error {
            code: ErrorCode::UnknownGame,
            ..
        }
    ));

    // A room with bots off and a fixed seat count: the settings go to the process as validated typed values.
    hc.request_create_with("Test room", &[(2, 0)]);
    let HubEvent::Created { room, build } = wait_event(&mut hc) else {
        panic!("create failed")
    };
    assert!(build_matches::<ToyGame>(build));
    assert_eq!((room.name.as_str(), room.public), ("Test room", false));
    assert!(hub.pool.contains(&room.port) && room.port != rooms[0].port);
    let listed = list(&mut hc);
    let entry = listed
        .iter()
        .find(|r| r.name == "Test room")
        .expect("the new room is listed");
    assert_eq!(
        (entry.port, entry.players, entry.state),
        (room.port, 0, RoomState::Lobby)
    );

    // Two real clients join the room's port (the hub's host, the room's port).
    let server = room_addr(hub.addr, room.port);
    let mut clients = vec![client(server, "P0", 0), client(server, "P1", 1)];
    assert!(
        drive(&mut clients, 20, |c| c
            .iter()
            .all(|x| *x.state() == ClientState::Lobby)),
        "both clients reached the lobby: {:?}",
        states(&clients)
    );

    // The server prints a status line a second; the hub lists the count.
    let mut seen = 0;
    let found = eventually(15, || {
        drive(&mut clients, 0, |_| true);
        seen = list(&mut hc)
            .iter()
            .find(|r| r.name == "Test room")
            .map_or(0, |r| r.players);
        seen == 2
    });
    assert!(found, "the room reports players=2, saw {seen}");
    // Both ready up: the match has exactly the two players because bots=0 reached the server.
    for c in clients.iter_mut() {
        c.ready(true);
    }
    assert!(
        drive(&mut clients, 25, |c| c
            .iter()
            .all(|x| x.view().snapshot.is_some())),
        "the match started: {:?}",
        states(&clients)
    );
    let participants = clients[0].view().snapshot.as_ref().unwrap().position.len();
    assert_eq!(participants, 2, "bots=0: no computer runners");
    assert!(eventually(10, || {
        drive(&mut clients, 0, |_| true);
        list(&mut hc)
            .iter()
            .any(|r| r.name == "Test room" && r.state == RoomState::Playing && r.players == 2)
    }));
    // The Public room (default settings: bots on) was not touched by that choice.
    // Reports go to a directory per game and port.
    assert!(dir
        .path()
        .join("reports")
        .join("toy-footrace")
        .join(format!("port-{}", room.port))
        .is_dir());

    // Stopping the hub takes every room server with it: the ports are free again.
    hub.stop();
    drop(clients);
    for port in hub.pool.clone() {
        assert!(
            eventually(10, || port_is_free(port)),
            "port {port} still held by an orphan"
        );
    }
}

#[test]
fn a_crashed_public_room_is_restarted_on_a_fresh_port() {
    let (base, pool) = (free_ports(3), 3);
    let dir = TempDir::new("crash");
    let crash = Crash::default();
    let spawned = Arc::new(Mutex::new(Vec::new()));
    let (c2, s2) = (crash.clone(), spawned.clone());
    let hub = TestHub::start(
        dir.path(),
        base,
        pool,
        &config_text(base, pool, dir.path(), "", false),
        relaxed(),
        legacy::Mode::Serve,
        move || {
            Box::new(CrashSpawner {
                inner: ProcessSpawner,
                crash: c2,
                spawned: s2,
            })
        },
    );
    let mut hc = HubClient::new(hub.addr, "toy-footrace").unwrap();
    let first = list(&mut hc)[0].port;
    crash.0.lock().unwrap().push(first);
    assert!(
        eventually(10, || spawned.lock().unwrap().len() >= 2),
        "the hub started Public again"
    );
    let again = spawned.lock().unwrap()[1];
    assert_ne!(again, first, "round robin: not the port just freed");
    assert!(
        eventually(10, || list(&mut hc)
            .iter()
            .any(|r| r.public && r.port == again)),
        "and lists it"
    );
    assert!(
        eventually(10, || port_is_free(first)),
        "the dead server's port is free"
    );
}

#[test]
fn a_room_nobody_joins_closes_and_its_port_is_freed_while_public_stays() {
    let (base, pool) = (free_ports(4), 4);
    let dir = TempDir::new("ghost");
    let hub = TestHub::start(
        dir.path(),
        base,
        pool,
        &config_text(base, pool, dir.path(), "", false),
        relaxed(),
        legacy::Mode::Serve,
        real_spawner,
    );
    let mut hc = HubClient::new(hub.addr, "toy-footrace").unwrap();
    hc.request_create("Ghost");
    let HubEvent::Created { room, .. } = wait_event(&mut hc) else {
        panic!()
    };
    assert!(list(&mut hc).iter().any(|r| r.name == "Ghost"));
    assert!(!port_is_free(room.port), "a live room holds its port");
    assert!(
        eventually(15, || list(&mut hc).iter().all(|r| r.name != "Ghost")),
        "never joined: closed (4 s in this test; 45 s by default)"
    );
    assert!(
        eventually(10, || port_is_free(room.port)),
        "and its server is gone"
    );
    assert!(
        list(&mut hc).iter().any(|r| r.public),
        "the Public room stays"
    );
}

#[test]
fn the_front_door_over_real_sockets_floods_get_a_burst_then_silence_and_garbage_is_ignored() {
    let (base, pool) = (free_ports(2), 2);
    let dir = TempDir::new("flood");
    let hub = TestHub::start(
        dir.path(),
        base,
        pool,
        &config_text(base, pool, dir.path(), "", false),
        Limits::default(),
        legacy::Mode::Serve,
        || Box::new(Pretend),
    );
    let flooder = UdpSocket::bind("127.0.0.1:0").unwrap();
    flooder
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let packet = Request::List {
        game: "toy-footrace".into(),
        skip: 0,
    }
    .encode(1);
    for _ in 0..60 {
        flooder.send_to(&packet, hub.addr).unwrap();
    }
    std::thread::sleep(Duration::from_millis(300));
    let mut buf = [0u8; 2048];
    let mut answered = 0;
    while flooder.recv_from(&mut buf).is_ok() {
        answered += 1;
    }
    assert!(
        (8..=14).contains(&answered),
        "about the burst of 10 (plus a refill or two): {answered}"
    );
    // A different source address is a different bucket.
    if let Ok(other) = UdpSocket::bind("127.0.0.2:0") {
        other
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        other.send_to(&Request::Cookie.encode(2), hub.addr).unwrap();
        let (n, _) = other
            .recv_from(&mut buf)
            .expect("another address is still served");
        assert!(matches!(
            Reply::decode(&buf[..n]).unwrap().reply,
            Reply::Cookie { .. }
        ));
    }
    // Rates recover.
    std::thread::sleep(Duration::from_millis(1200));
    flooder
        .send_to(
            &Request::Ping {
                game: "toy-footrace".into(),
            }
            .encode(3),
            hub.addr,
        )
        .unwrap();
    flooder
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    assert!(
        flooder.recv_from(&mut buf).is_ok(),
        "answered again after a pause"
    );
    // Garbage from a fresh address gets no reply and does not hurt the hub.
    let s = UdpSocket::bind("127.0.0.1:0").unwrap();
    s.set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    for junk in [
        &b"GET / HTTP/1.1\r\n\r\n"[..],
        &[0u8; 300][..],
        b"DFHB",
        b"BEHB\x01\x01",
        &[0xFF; 1500][..],
    ] {
        s.send_to(junk, hub.addr).unwrap();
        assert!(
            s.recv_from(&mut buf).is_err(),
            "no reply to {} junk bytes",
            junk.len()
        );
    }
    s.send_to(&Request::Cookie.encode(9), hub.addr).unwrap();
    assert!(s.recv_from(&mut buf).is_ok());
}

#[test]
fn a_spoofed_source_cannot_create_a_room_without_the_cookie_its_address_was_given() {
    let (base, pool) = (free_ports(2), 2);
    let dir = TempDir::new("cookie");
    let hub = TestHub::start(
        dir.path(),
        base,
        pool,
        &config_text(base, pool, dir.path(), "", false),
        relaxed(),
        legacy::Mode::Serve,
        || Box::new(Pretend),
    );
    let a = UdpSocket::bind("127.0.0.1:0").unwrap();
    a.set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    let ask = |req: &Request, nonce: u32| -> Reply {
        a.send_to(&req.encode(nonce), hub.addr).unwrap();
        let mut buf = [0u8; 2048];
        let (n, _) = a.recv_from(&mut buf).unwrap();
        Reply::decode(&buf[..n]).unwrap().reply
    };
    let make = |cookie: u64| Request::Create {
        game: "toy-footrace".into(),
        name: "Mine".into(),
        settings: vec![],
        cookie,
    };
    assert!(matches!(
        ask(&make(0), 1),
        Reply::Error {
            code: ErrorCode::BadCookie,
            ..
        }
    ));
    let Reply::Cookie { token } = ask(&Request::Cookie, 2) else {
        panic!()
    };
    assert!(matches!(
        ask(&make(token ^ 1), 3),
        Reply::Error {
            code: ErrorCode::BadCookie,
            ..
        }
    ));
    assert!(matches!(ask(&make(token), 4), Reply::Created { .. }));
}

// ---- legacy DFHB over a real socket, and reload, through the be2-hub binary ----------------------------------------------

struct HubProcess {
    child: Child,
    base: u16,
    pool: u16,
}

impl HubProcess {
    fn start(
        dir: &Path,
        base: u16,
        pool: u16,
        legacy_deadfall: bool,
        extra_args: &[&str],
    ) -> HubProcess {
        let conf = dir.join("hub.conf");
        std::fs::write(&conf, config_text(base, pool, dir, "", legacy_deadfall)).unwrap();
        let mut cmd = Command::new(HUB_BIN);
        cmd.arg("--config")
            .arg(&conf)
            .args(extra_args)
            .stdin(Stdio::null());
        let child = cmd.spawn().unwrap();
        let mut p = HubProcess { child, base, pool };
        // Wait until it answers.
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        sock.set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let mut buf = [0u8; 256];
        let up = eventually(20, || {
            let _ = sock.send_to(&Request::Cookie.encode(1), ("127.0.0.1", base));
            sock.recv_from(&mut buf).is_ok() || p.child.try_wait().unwrap().is_some()
        });
        assert!(
            up && p.child.try_wait().unwrap().is_none(),
            "be2-hub did not start"
        );
        p
    }
    fn addr(&self) -> SocketAddr {
        ([127, 0, 0, 1], self.base).into()
    }
    fn ports(&self) -> Vec<u16> {
        (self.base + 1..=self.base + self.pool).collect()
    }
}

impl Drop for HubProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn legacy_ask(
    sock: &UdpSocket,
    hub: SocketAddr,
    r: &legacy::Request,
    nonce: u32,
) -> legacy::ReplyPacket {
    sock.send_to(&r.encode(nonce), hub).unwrap();
    let mut buf = [0u8; 2048];
    let (n, _) = sock.recv_from(&mut buf).expect("a legacy reply");
    legacy::Reply::decode(&buf[..n]).expect("a v1 reply")
}

#[test]
fn the_hub_binary_answers_both_protocols_reloads_one_game_and_leaves_no_orphans() {
    let (base, pool) = (free_ports(6), 6);
    let dir = TempDir::new("binary");
    let hubp = HubProcess::start(dir.path(), base, pool, true, &[]);
    let conf = dir.path().join("hub.conf");

    // `ports` prints the hub port then the pool, from the same config.
    let ports = Command::new(HUB_BIN)
        .arg("ports")
        .arg("--config")
        .arg(&conf)
        .output()
        .unwrap();
    assert!(ports.status.success());
    let printed: Vec<u16> = String::from_utf8(ports.stdout)
        .unwrap()
        .lines()
        .map(|l| l.parse().unwrap())
        .collect();
    assert_eq!(
        printed,
        std::iter::once(base)
            .chain(hubp.ports())
            .collect::<Vec<_>>()
    );

    // BEHB: list, create with the cookie flow, ping.
    let mut hc = HubClient::new(hubp.addr(), "toy-footrace").unwrap();
    assert_eq!(list(&mut hc).len(), 1);
    hc.request_create("Binary room");
    let HubEvent::Created { room, .. } = wait_event(&mut hc) else {
        panic!("create failed")
    };
    assert!(hubp.ports().contains(&room.port));

    // DFHB (an old Deadfall client): the same hub answers in the old format, with the raw fingerprint as the build.
    let old = UdpSocket::bind("127.0.0.1:0").unwrap();
    old.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let p = legacy_ask(&old, hubp.addr(), &legacy::Request::Ping, 11);
    assert_eq!(
        (p.nonce, p.build, p.reply),
        (11, ToyGame::fingerprint(), legacy::Reply::Pong)
    );
    let p = legacy_ask(&old, hubp.addr(), &legacy::Request::List { skip: 0 }, 12);
    assert_eq!(p.build, ToyGame::fingerprint());
    let legacy::Reply::Rooms { total, rooms, .. } = p.reply else {
        panic!()
    };
    assert_eq!(
        total, 1,
        "deadfall's own Public room, not the toy-footrace rooms"
    );
    assert!(rooms[0].public);
    let p = legacy_ask(
        &old,
        hubp.addr(),
        &legacy::Request::Create {
            name: "Old client room".into(),
            bots: false,
            kills: 20,
        },
        13,
    );
    assert!(
        matches!(p.reply, legacy::Reply::Created { ref room } if room.name == "Old client room"),
        "{p:?}"
    );

    // Reload one game: its rooms are retired (unlisted), the other game's are untouched, new rooms come up.
    let before = list(&mut hc);
    let out = Command::new(HUB_BIN)
        .args(["reload", "toy-footrace", "--config"])
        .arg(&conf)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("reloaded"));
    assert!(
        eventually(10, || {
            let now = list(&mut hc);
            now.len() == 1 && now[0].public && before.iter().all(|r| r.port != now[0].port)
        }),
        "a fresh Public room and no retired rooms"
    );
    let legacy::Reply::Rooms { total, .. } =
        legacy_ask(&old, hubp.addr(), &legacy::Request::List { skip: 0 }, 14).reply
    else {
        panic!()
    };
    assert_eq!(
        total, 2,
        "deadfall's rooms were not touched by reloading another game"
    );
    let bad = Command::new(HUB_BIN)
        .args(["reload", "nonexistent", "--config"])
        .arg(&conf)
        .output()
        .unwrap();
    assert!(
        !bad.status.success() && String::from_utf8_lossy(&bad.stderr).contains("not in the config")
    );
    // The retired room had nobody in it, so its server is gone and its port is free again.
    assert!(
        eventually(10, || port_is_free(room.port)),
        "the retired empty room closed"
    );

    // SIGKILL the hub (no chance to clean up): every server it started notices and exits.
    let ports = hubp.ports();
    drop(hubp);
    for port in ports {
        assert!(
            eventually(15, || port_is_free(port)),
            "port {port} still held by an orphan"
        );
    }
}

#[test]
fn a_retired_room_keeps_its_players_until_they_leave() {
    let (base, pool) = (free_ports(4), 4);
    let dir = TempDir::new("retire");
    let hubp = HubProcess::start(dir.path(), base, pool, false, &[]);
    let conf = dir.path().join("hub.conf");
    let mut hc = HubClient::new(hubp.addr(), "toy-footrace").unwrap();
    let old_public = list(&mut hc)[0].clone();
    let server = room_addr(hubp.addr(), old_public.port);
    let mut clients = vec![client(server, "Stayer", 0)];
    assert!(
        drive(&mut clients, 20, |c| *c[0].state() == ClientState::Lobby),
        "{:?}",
        states(&clients)
    );
    assert!(eventually(10, || {
        drive(&mut clients, 0, |_| true);
        list(&mut hc).iter().any(|r| r.public && r.players == 1)
    }));
    let out = Command::new(HUB_BIN)
        .args(["reload", "toy-footrace", "--config"])
        .arg(&conf)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The list shows only the new Public room; the player is still connected to the old one.
    assert!(eventually(10, || {
        let now = list(&mut hc);
        now.len() == 1 && now[0].port != old_public.port
    }));
    drive(&mut clients, 3, |_| false);
    assert_eq!(
        *clients[0].state(),
        ClientState::Lobby,
        "the player was not thrown out"
    );
    assert!(
        !port_is_free(old_public.port),
        "the retired room is still running"
    );
    // The player leaves: the retired room closes at once.
    clients[0].leave();
    drive(&mut clients, 1, |_| false);
    drop(clients);
    assert!(
        eventually(15, || port_is_free(old_public.port)),
        "the retired room closed once it was empty"
    );
}

#[test]
fn the_hub_binary_refuses_to_start_on_a_taken_port_and_skips_games_that_cannot_run() {
    let base = free_ports(2);
    let dir = TempDir::new("refuse");
    let _held = UdpSocket::bind(("127.0.0.1", base)).unwrap();
    // `free_ports` released the hub port; hold it again and try to start there.
    let conf = dir.path().join("hub.conf");
    std::fs::write(&conf, config_text(base, 2, dir.path(), "", false)).unwrap();
    let out = Command::new(HUB_BIN)
        .arg("--config")
        .arg(&conf)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("cannot listen") && err.contains(&base.to_string()),
        "{err}"
    );
    drop(_held);

    // A missing server binary and a server whose --info fails are skipped; the good game still runs.
    let base = free_ports(2);
    let conf = dir.path().join("two.conf");
    std::fs::write(
        &conf,
        format!(
            "[hub]\nlisten = 127.0.0.1:{base}\npool_size = 2\nreport_dir = {}\n\n\
             [game toy-footrace]\nserver = {TOY_SERVER}\npublic = on\nauto_start = 0\n\n\
             [game ghost]\nserver = {}/does-not-exist\n\n[game broken]\nserver = /bin/false\n",
            dir.path().join("r").display(),
            dir.path().display()
        ),
    )
    .unwrap();
    let mut child = Command::new(HUB_BIN)
        .arg("--config")
        .arg(&conf)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
    sock.set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let mut buf = [0u8; 2048];
    let mut ok = false;
    for _ in 0..100 {
        let _ = sock.send_to(
            &Request::Ping {
                game: "toy-footrace".into(),
            }
            .encode(1),
            ("127.0.0.1", base),
        );
        if let Ok((n, _)) = sock.recv_from(&mut buf) {
            ok = matches!(Reply::decode(&buf[..n]).map(|p| p.reply), Some(Reply::Pong));
            break;
        }
    }
    assert!(ok, "the good game is served");
    sock.send_to(
        &Request::Ping {
            game: "ghost".into(),
        }
        .encode(2),
        ("127.0.0.1", base),
    )
    .unwrap();
    let (n, _) = sock.recv_from(&mut buf).unwrap();
    assert!(matches!(
        Reply::decode(&buf[..n]).unwrap().reply,
        Reply::Error {
            code: ErrorCode::UnknownGame,
            ..
        }
    ));
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn the_control_datagram_is_ignored_unless_it_comes_from_loopback_and_names_a_game() {
    let (base, pool) = (free_ports(2), 2);
    let dir = TempDir::new("ctl");
    let hubp = HubProcess::start(dir.path(), base, pool, false, &[]);
    let s = UdpSocket::bind("127.0.0.1:0").unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut buf = [0u8; 512];
    s.send_to(
        &Control::Reload {
            game: "toy-footrace".into(),
        }
        .encode(7),
        hubp.addr(),
    )
    .unwrap();
    let (n, _) = s.recv_from(&mut buf).unwrap();
    let (nonce, reply) = ControlReply::decode(&buf[..n]).unwrap();
    assert!(nonce == 7 && reply.ok, "{reply:?}");
    s.send_to(b"BECT\x01\x01\0\0\0\0\x03A!B", hubp.addr())
        .unwrap();
    s.set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    assert!(
        s.recv_from(&mut buf).is_err(),
        "a malformed control datagram gets silence"
    );
}
