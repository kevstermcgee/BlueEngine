//! The game's side of the hub: ask which rooms exist, make one, work out where the hub lives, and the rules of a
//! Play Online screen. Std only and no window, so the decisions have unit tests and a game only draws.
//!
//! Three layers, use as much as you need:
//!
//! * [`HubClient`]: one non-blocking request at a time to one hub for one game (list, create with the
//!   source-address cookie round trip, paging, retries, timeout). Poll it once a frame.
//! * [`Online`]: the Play Online state machine on top of it: resolves the hub's name off the UI thread, refreshes
//!   the list, owns the Create Room dialog, says when to join ([`Action::Join`]), and explains failures.
//!   [`order_rooms`], [`room_status`], [`JoinWait`], [`scroll_to`] and the message functions are its rules, public so a
//!   game with its own screens can use them directly.
//! * [`default_hub`] and [`local_build`]: which hub to talk to and which build to compare.
//!
//! ```ignore
//! let hub = hub::default_hub(cli_connect_arg, settings.last_hub.as_deref());
//! let build = hub::local_build::<MyGame>();
//! let mut online = Online::new(&hub.address, MyGame::NAME, build, now);
//! // every frame:
//! if let Some(Action::Join { addr, room }) = online.update(now) { /* NetClient to addr */ }
//! // draw online.view / online.rooms / online.dialog; call online.join_selected(), online.open_dialog(&player), ...
//! ```
use super::wire::{
    sanitize_name, ErrorCode, NameError, Reply, Request, RoomInfo, RoomState, DEFAULT_PORT,
};
use super::DEFAULT_HUB;
use crate::viewer::devkit::{resolve_ipv4, AddressError, ServerChoice, ServerOrigin, ServerSource};
use crate::viewer::netplay::{cli, ConnectFailure, NetGame};
use std::io;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The override file, beside the executable: the first line is the hub address (`host` or `host:port`), an
/// optional second line a join key (unused by hubs).
pub const OVERRIDE_FILE: &str = "server.txt";
/// Resend an unanswered request this often.
pub const RETRY_EVERY: Duration = Duration::from_millis(800);
/// Give up on a request after this long.
pub const TIMEOUT: Duration = Duration::from_millis(2500);
/// A long room list is fetched in at most this many datagrams.
const MAX_PAGES: u8 = 4;
/// Ask the hub for a fresh room list this often while the screen is open.
pub const REFRESH_EVERY: f64 = 4.;
/// Try a refused-because-a-match-is-running join again this often.
pub const JOIN_RETRY_EVERY: f64 = 3.;
/// Stop waiting for a match to end after this long (a long match is a few minutes).
pub const JOIN_GIVE_UP: f64 = 20. * 60.;

pub use super::wire::MAX_NAME_CHARS;

// ---- which hub, which build -----------------------------------------------------------------------------------------

/// The hub to use: a command-line value, else `server.txt` beside the executable, else the address the player used
/// last time, else [`DEFAULT_HUB`].
pub fn default_hub(cli_arg: Option<&str>, last_used: Option<&str>) -> ServerChoice {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    default_hub_in(&dir, cli_arg, last_used)
}

/// [`default_hub`] for an explicit executable directory, so tests are deterministic.
pub fn default_hub_in(dir: &Path, cli_arg: Option<&str>, last_used: Option<&str>) -> ServerChoice {
    ServerChoice::first_of(
        dir,
        &[
            ServerSource::CliArg(cli_arg),
            ServerSource::FileBesideExe(OVERRIDE_FILE),
            ServerSource::LastUsed(last_used),
            ServerSource::Builtin(DEFAULT_HUB),
        ],
    )
    .unwrap_or(ServerChoice {
        address: DEFAULT_HUB.to_string(),
        join_key: None,
        origin: ServerOrigin::Builtin,
    })
}

/// This game's build id: what its server reports, and what a hub reports for the game
/// ([`cli::build_id`]). A hub whose build differs would be refused by its rooms anyway.
pub fn local_build<G: NetGame>() -> u32 {
    cli::build_id::<G>()
}

/// Does a hub's build id match this game's? A mismatch means the rooms would refuse us.
pub fn build_matches<G: NetGame>(hub_build: u32) -> bool {
    hub_build == local_build::<G>()
}

/// The address of a room's game server: the hub's host, the room's port.
pub fn room_addr(hub_addr: SocketAddr, port: u16) -> SocketAddr {
    SocketAddr::new(hub_addr.ip(), port)
}

/// Discover a room with [`HubClient`] / [`Online`], then connect through its advertised transport.
/// Production TLS uses the locally provisioned pin (`BLUE_TLS_CERT_FILE`); discovery cannot change it.
/// The admission key is supplied by the player/deployment, never fetched over the hub's UDP channel.
pub fn connect_room<G: NetGame>(
    hub_addr: SocketAddr,
    room: &RoomInfo,
    cfg: crate::viewer::netplay::ClientConfig,
) -> crate::Result<crate::viewer::netplay::NetClient<G, crate::viewer::net::AnyTransport>> {
    let pin = if room.transport == crate::viewer::net::TransportProfile::Production {
        crate::viewer::net::quic::trusted_certificate()?
    } else {
        Vec::new()
    };
    connect_room_with_pin(hub_addr, room, cfg, &pin)
}

/// As [`connect_room`] with a certificate provisioned by the caller, never one supplied by discovery.
pub fn connect_room_with_pin<G: NetGame>(
    hub_addr: SocketAddr,
    room: &RoomInfo,
    cfg: crate::viewer::netplay::ClientConfig,
    pin: &[u8],
) -> crate::Result<crate::viewer::netplay::NetClient<G, crate::viewer::net::AnyTransport>> {
    use crate::viewer::net::{client_transport, TransportProfile};
    if room.requires_key && cfg.key.is_empty() {
        return Err("This room requires an admission key; obtain it from the host".into());
    }
    if room.transport == TransportProfile::Development && !hub_addr.ip().is_loopback() {
        return Err("Remote development UDP requires an explicit custom client; use a production room for Internet play".into());
    }
    if room.transport == TransportProfile::Production && !room.requires_key {
        return Err("Production room lacks admission metadata; refresh or update the game".into());
    }
    let addr = room_addr(hub_addr, room.port);
    let transport = match room.transport {
        TransportProfile::Development => client_transport(room.transport, addr)?,
        TransportProfile::Production => crate::viewer::net::AnyTransport::new(
            crate::viewer::net::quic::SecureSocket::client(addr, pin.to_vec())?,
        ),
    };
    crate::viewer::netplay::NetClient::new(transport, addr, cfg)
}

/// What to tell the player when the hub's build is not this game's.
pub fn update_message(game_title: &str) -> String {
    format!(
        "This server runs a different version of {game_title}. Update the game, then try again."
    )
}

/// The build id as shown in a corner of a menu.
pub fn build_label(build: u32) -> String {
    format!("build {build:08x}")
}

// ---- the hub client -------------------------------------------------------------------------------------------------

/// The result of a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HubEvent {
    /// The room list, Public first. `build` is the game's build id on the hub: compare it with [`local_build`].
    Rooms { build: u32, rooms: Vec<RoomInfo> },
    /// The room was made; join [`room_addr`]`(hub, room.port)`.
    Created { build: u32, room: RoomInfo },
    /// The hub refused: show the sentence for `code` ([`hub_error_message`]); `text` is the hub's own, short.
    Error {
        build: u32,
        code: ErrorCode,
        text: String,
    },
    /// No answer after the retries: the hub is down or unreachable.
    Timeout,
}

#[derive(Clone, Debug)]
struct CreateIntent {
    name: String,
    settings: Vec<(u8, u32)>,
}

struct Pending {
    /// What is in flight right now (a Create starts as a Cookie request).
    request: Request,
    create: Option<CreateIntent>,
    nonce: u32,
    started: Instant,
    last_sent: Instant,
    collected: Vec<RoomInfo>,
    pages: u8,
    cookie_retries: u8,
}

/// One request at a time to one hub for one game; poll it every frame.
pub struct HubClient {
    socket: UdpSocket,
    hub: SocketAddr,
    game: String,
    pending: Option<Pending>,
    retry: Duration,
    timeout: Duration,
    seq: u32,
}

impl HubClient {
    /// A client for `game` on the hub at `hub` (IPv4). Opens a UDP socket; nothing is sent until a request is made.
    pub fn new(hub: SocketAddr, game: &str) -> io::Result<Self> {
        Self::with_timing(hub, game, RETRY_EVERY, TIMEOUT)
    }

    /// [`HubClient::new`] with other retry and timeout times (tests).
    pub fn with_timing(
        hub: SocketAddr,
        game: &str,
        retry: Duration,
        timeout: Duration,
    ) -> io::Result<Self> {
        if !hub.is_ipv4() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the hub must be an IPv4 address",
            ));
        }
        if !super::wire::game_id_ok(game) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a game id is 1-24 of a-z, 0-9 and -",
            ));
        }
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_nonblocking(true)?;
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(1, |d| d.subsec_nanos());
        Ok(Self {
            socket,
            hub,
            game: game.to_string(),
            pending: None,
            retry,
            timeout,
            seq: seed ^ std::process::id().rotate_left(16),
        })
    }

    pub fn hub_addr(&self) -> SocketAddr {
        self.hub
    }

    pub fn game(&self) -> &str {
        &self.game
    }

    /// A request is in flight.
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }

    /// Ask for the room list. Replaces any request in flight.
    pub fn request_list(&mut self) {
        let request = Request::List {
            game: self.game.clone(),
            skip: 0,
        };
        self.start(request, None);
    }

    /// Ask the hub to make a room called `name` (clean it with [`sanitize_name`] first to show errors as the player
    /// types; the hub checks again). Replaces any request in flight.
    pub fn request_create(&mut self, name: &str) {
        self.request_create_with(name, &[]);
    }

    /// [`HubClient::request_create`] with settings: `(setting id, value)` pairs from the game's
    /// [`NetGame::settings`] (at most 8; the hub validates them).
    pub fn request_create_with(&mut self, name: &str, settings: &[(u8, u32)]) {
        let intent = CreateIntent {
            name: name.to_string(),
            settings: settings.to_vec(),
        };
        // A Create carries a cookie the hub gave to this address a moment ago: ask for one first.
        self.start(Request::Cookie, Some(intent));
    }

    /// Forget any request in flight (a late reply is ignored).
    pub fn cancel(&mut self) {
        self.pending = None;
    }

    fn start(&mut self, request: Request, create: Option<CreateIntent>) {
        // A xorshift step: nonces are for matching replies to requests, not secrecy.
        let mut x = self.seq | 1;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seq = x;
        let now = Instant::now();
        let p = Pending {
            request,
            create,
            nonce: x,
            started: now,
            last_sent: now,
            collected: Vec::new(),
            pages: 1,
            cookie_retries: 0,
        };
        self.send(&p.request, p.nonce);
        self.pending = Some(p);
    }

    fn send(&self, request: &Request, nonce: u32) {
        let _ = self.socket.send_to(&request.encode(nonce), self.hub);
    }

    /// Check for the answer (never blocks). Returns each request's result once.
    pub fn poll(&mut self) -> Option<HubEvent> {
        let mut buf = [0u8; 2048];
        while let Ok((n, from)) = self.socket.recv_from(&mut buf) {
            let Some(p) = self.pending.as_mut() else {
                continue;
            };
            if from != self.hub {
                continue;
            }
            let Some(packet) = Reply::decode(&buf[..n]) else {
                continue;
            };
            if packet.nonce != p.nonce {
                continue;
            }
            let build = packet.build;
            match (&p.request, packet.reply) {
                (Request::Cookie, Reply::Cookie { token }) => {
                    let Some(intent) = p.create.clone() else {
                        continue;
                    };
                    let request = Request::Create {
                        game: self.game.clone(),
                        name: intent.name,
                        settings: intent.settings,
                        cookie: token,
                    };
                    p.last_sent = Instant::now();
                    let _ = self.socket.send_to(&request.encode(p.nonce), self.hub);
                    p.request = request;
                }
                (
                    Request::Create { .. },
                    Reply::Error {
                        code: ErrorCode::BadCookie,
                        ..
                    },
                ) if p.cookie_retries < 1 => {
                    // The cookie expired on the way (a slow connection, a long pause): get a fresh one, once.
                    p.cookie_retries += 1;
                    p.request = Request::Cookie;
                    p.last_sent = Instant::now();
                    let _ = self
                        .socket
                        .send_to(&Request::Cookie.encode(p.nonce), self.hub);
                }
                (_, Reply::Error { code, text }) => {
                    self.pending = None;
                    return Some(HubEvent::Error { build, code, text });
                }
                (
                    Request::List { .. },
                    Reply::Rooms {
                        skip,
                        total,
                        rooms,
                        game,
                    },
                ) => {
                    if game != self.game || skip as usize != p.collected.len() {
                        continue; // another game's list, or a duplicate page
                    }
                    let got = rooms.len();
                    p.collected.extend(rooms);
                    if got > 0 && p.collected.len() < total as usize && p.pages < MAX_PAGES {
                        p.pages += 1;
                        p.last_sent = Instant::now();
                        let next = Request::List {
                            game: self.game.clone(),
                            skip: p.collected.len().min(255) as u8,
                        };
                        let _ = self.socket.send_to(&next.encode(p.nonce), self.hub);
                        continue;
                    }
                    let rooms = std::mem::take(&mut p.collected);
                    self.pending = None;
                    return Some(HubEvent::Rooms { build, rooms });
                }
                (Request::Create { .. }, Reply::Created { room }) => {
                    self.pending = None;
                    return Some(HubEvent::Created { build, room });
                }
                _ => {}
            }
        }
        let p = self.pending.as_mut()?;
        if p.started.elapsed() >= self.timeout {
            self.pending = None;
            return Some(HubEvent::Timeout);
        }
        if p.last_sent.elapsed() >= self.retry {
            p.last_sent = Instant::now();
            // Resend what is in flight (a Create is safe to repeat: the hub recognises the nonce).
            let request = match &p.request {
                Request::List { game, .. } => Request::List {
                    game: game.clone(),
                    skip: p.collected.len().min(255) as u8,
                },
                other => other.clone(),
            };
            let _ = self.socket.send_to(&request.encode(p.nonce), self.hub);
        }
        None
    }

    #[cfg(test)]
    fn local_port(&self) -> u16 {
        self.socket.local_addr().map(|a| a.port()).unwrap_or(0)
    }

    #[cfg(test)]
    fn nonce(&self) -> Option<u32> {
        self.pending.as_ref().map(|p| p.nonce)
    }
}

// ---- pure rules -----------------------------------------------------------------------------------------------------

/// The list as the player sees it: the Public room first, then the busiest rooms, ties by name.
pub fn order_rooms(mut rooms: Vec<RoomInfo>) -> Vec<RoomInfo> {
    rooms.sort_by(|a, b| {
        b.public
            .cmp(&a.public)
            .then(b.players.cmp(&a.players))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    rooms
}

/// The right-hand side of a room's row: "3/12  Lobby", "5/12  In match", "12/12  Full".
pub fn room_status(room: &RoomInfo) -> String {
    let state = if room.players >= room.capacity && room.capacity > 0 {
        "Full"
    } else {
        match room.state {
            RoomState::Lobby => "Lobby",
            RoomState::Playing => "In match",
        }
    };
    format!("{}/{}  {state}", room.players, room.capacity)
}

/// The room name offered in the Create Room box: "PLAYER's room", cleaned so the hub accepts it.
pub fn default_room_name(player: &str) -> String {
    let who: String = player
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '\'')
        .collect();
    let who = who.trim();
    let name = if who.chars().any(char::is_alphanumeric) {
        format!("{who}'s room")
    } else {
        "My room".to_string()
    };
    match sanitize_name(&name) {
        Ok(n) => n,
        Err(_) => "My room".to_string(),
    }
}

/// A friendly sentence for something the hub refused.
pub fn hub_error_message(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::NameTaken => {
            "A room with that name already exists. Pick another name, or join it from the list."
        }
        ErrorCode::Full => {
            "All the rooms are in use right now. Join one from the list, or try again in a few minutes."
        }
        ErrorCode::RateLimited => "Too many requests too fast. Wait a few seconds and try again.",
        ErrorCode::BadName => {
            "That name will not work. Use letters, numbers, spaces, apostrophes and hyphens."
        }
        ErrorCode::Unavailable => {
            "The server could not start a new room right now. Try again in a moment."
        }
        ErrorCode::BadRequest => "The server did not understand that. Update the game and try again.",
        ErrorCode::UnknownGame => {
            "This server does not host this game right now. Try again later, or pick another server."
        }
        ErrorCode::BadCookie => "The server did not accept the request in time. Try again.",
        ErrorCode::BadSetting => {
            "The server does not accept those room settings. Update the game and try again."
        }
    }
}

/// A friendly sentence for a room name that failed the local check.
pub fn name_error_message(e: NameError) -> &'static str {
    e.text()
}

/// How long the engine client waits before it gives up, for the message (`CONNECT_TIMEOUT` in the engine client).
pub const CONNECT_WAIT_SECONDS: u32 = 8;

/// The words for a failed connection to a room's game server, from the engine's own [`ConnectFailure`]. `room`
/// is true when the address came from the room list, where the player never typed it and port forwarding is not
/// their problem. A [`ConnectFailure::MatchInProgress`] is normally handled with a [`JoinWait`] before this is shown.
pub fn connect_failure_message(failure: &ConnectFailure, room: bool) -> String {
    match failure {
        ConnectFailure::Unreachable { addr, waited_secs } if room => format!(
            "No reply from the room at {addr} after {waited_secs} s. The room may have just closed, or the server is \
             having trouble. Go back and refresh the list, or try again."
        ),
        other => other.hint(),
    }
}

/// Should a join that failed this way be tried again after a wait?
pub fn should_retry_join(failure: &ConnectFailure) -> bool {
    matches!(failure, ConnectFailure::MatchInProgress)
}

/// What to do about a join that waits for a running match to end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RetryStep {
    /// Not yet.
    Wait,
    /// Ask the lobby again now.
    TryNow,
    /// Waited long enough.
    GiveUp,
}

/// The retry schedule for "A match is running".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JoinWait {
    pub since: f64,
    pub next_at: f64,
    pub attempts: u32,
}

impl JoinWait {
    pub fn new(now: f64) -> JoinWait {
        JoinWait {
            since: now,
            next_at: now + JOIN_RETRY_EVERY,
            attempts: 0,
        }
    }

    pub fn step(&self, now: f64) -> RetryStep {
        if now - self.since >= JOIN_GIVE_UP {
            RetryStep::GiveUp
        } else if now >= self.next_at {
            RetryStep::TryNow
        } else {
            RetryStep::Wait
        }
    }

    /// An attempt was made at `now` and was turned away again: schedule the next one.
    pub fn attempted(&mut self, now: f64) {
        self.attempts += 1;
        self.next_at = now + JOIN_RETRY_EVERY;
    }
}

/// Keep the highlighted row inside the visible window. `sel` is the selected room's index in the whole list.
pub fn scroll_to(sel: usize, offset: usize, visible: usize, total: usize) -> usize {
    let max_offset = total.saturating_sub(visible);
    let offset = if sel < offset {
        sel
    } else if sel >= offset + visible {
        sel + 1 - visible
    } else {
        offset
    };
    offset.min(max_offset)
}

// ---- the state machine ----------------------------------------------------------------------------------------------

/// What the room list screen is showing.
#[derive(Clone, Debug, PartialEq)]
pub enum Pane {
    /// Waiting for the first answer.
    Looking,
    Rooms,
    /// The hub does not answer (or its name did not resolve).
    Offline,
    /// The hub answered with a refusal to the list request.
    Problem(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum DialogPhase {
    Editing,
    Creating,
}

/// The Create Room box.
#[derive(Clone, Debug)]
pub struct Dialog {
    pub name: String,
    pub phase: DialogPhase,
    pub error: Option<String>,
}

/// What the screen should do next.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Connect to this room's game server.
    Join { addr: SocketAddr, room: RoomInfo },
}

/// The Play Online screen without the window.
pub struct Online {
    spec: String,
    game: String,
    local_build: u32,
    resolving: Option<Receiver<Result<SocketAddr, AddressError>>>,
    client: Option<HubClient>,
    pub view: Pane,
    /// Why the hub is unreachable, for a dim detail line.
    pub detail: Option<String>,
    pub rooms: Vec<RoomInfo>,
    /// The game's build id on the hub, once it has answered.
    pub build: Option<u32>,
    next_refresh: f64,
    pub dialog: Option<Dialog>,
    /// Settings sent with the next Create ([`Online::set_create_settings`]).
    create_settings: Vec<(u8, u32)>,
    /// The highlighted room, by name (the list can reorder under it).
    pub selected: Option<String>,
    pub offset: usize,
}

fn spawn_resolve(spec: &str) -> Receiver<Result<SocketAddr, AddressError>> {
    let (tx, rx) = mpsc::channel();
    let spec = spec.to_string();
    // If the thread cannot start the sender is dropped and the receiver reports a disconnect: the screen shows offline.
    let _ = std::thread::Builder::new()
        .name("hub-dns".into())
        .spawn(move || {
            let _ = tx.send(resolve_ipv4(&spec, DEFAULT_PORT));
        });
    rx
}

impl Online {
    /// Open the screen for `game` on the hub at `spec` (`host` or `host:port`): resolves it off the calling thread.
    /// `local_build` is this game's build ([`local_build`]); a game with a "pretend to be another build" switch
    /// passes that instead.
    pub fn new(spec: &str, game: &str, local_build: u32, now: f64) -> Online {
        Online {
            spec: spec.to_string(),
            game: game.to_string(),
            local_build,
            resolving: Some(spawn_resolve(spec)),
            client: None,
            view: Pane::Looking,
            detail: None,
            rooms: Vec::new(),
            build: None,
            next_refresh: now,
            dialog: None,
            create_settings: Vec::new(),
            selected: None,
            offset: 0,
        }
    }

    /// [`Online::new`] for a hub whose address is already known (tests).
    pub fn at(hub: SocketAddr, game: &str, local_build: u32, now: f64) -> Online {
        let mut o = Online::new(&hub.to_string(), game, local_build, now);
        o.resolving = None;
        o.attach(hub, now);
        o
    }

    pub fn hub_spec(&self) -> &str {
        &self.spec
    }

    pub fn game(&self) -> &str {
        &self.game
    }

    /// The hub's address once it has resolved.
    pub fn hub_addr(&self) -> Option<SocketAddr> {
        self.client.as_ref().map(HubClient::hub_addr)
    }

    /// The `(setting id, value)` pairs the next Create sends (the game's room options, from its own settings screen).
    pub fn set_create_settings(&mut self, settings: &[(u8, u32)]) {
        self.create_settings = settings.to_vec();
    }

    /// The game's build on the hub is not this game's: rooms would turn us away.
    pub fn mismatch(&self) -> bool {
        self.build.is_some_and(|b| b != self.local_build)
    }

    /// Joining and creating are allowed.
    pub fn can_join(&self) -> bool {
        self.view == Pane::Rooms && !self.mismatch()
    }

    /// Nothing is happening that a Retry should wait for.
    pub fn is_busy(&self) -> bool {
        self.resolving.is_some() || self.client.as_ref().is_some_and(HubClient::busy)
    }

    fn attach(&mut self, addr: SocketAddr, now: f64) {
        match HubClient::new(addr, &self.game) {
            Ok(mut c) => {
                c.request_list();
                self.client = Some(c);
            }
            Err(e) => {
                self.detail = Some(e.to_string());
                self.view = Pane::Offline;
                self.next_refresh = now + REFRESH_EVERY;
            }
        }
    }

    /// Ask again now (the Refresh and Retry buttons). Looks the hub up again if it never resolved.
    pub fn refresh(&mut self, now: f64) {
        if self.resolving.is_some() {
            return;
        }
        match self.client.as_mut() {
            None => {
                self.resolving = Some(spawn_resolve(&self.spec));
                self.view = Pane::Looking;
            }
            Some(c) => {
                if self
                    .dialog
                    .as_ref()
                    .is_some_and(|d| d.phase == DialogPhase::Creating)
                {
                    return;
                }
                c.request_list();
                if self.view == Pane::Offline || matches!(self.view, Pane::Problem(_)) {
                    self.view = Pane::Looking;
                }
            }
        }
        self.next_refresh = now + REFRESH_EVERY;
    }

    pub fn selected_room(&self) -> Option<&RoomInfo> {
        self.selected
            .as_ref()
            .and_then(|n| self.rooms.iter().find(|r| &r.name == n))
            .or(self.rooms.first())
    }

    /// Join the highlighted room (the Join button).
    pub fn join_selected(&self) -> Option<Action> {
        if !self.can_join() {
            return None;
        }
        let hub = self.client.as_ref()?.hub_addr();
        let room = self.selected_room()?.clone();
        Some(Action::Join {
            addr: room_addr(hub, room.port),
            room,
        })
    }

    /// Join a room by its place in the list (Enter or a click on its row).
    pub fn join_index(&self, index: usize) -> Option<Action> {
        if !self.can_join() {
            return None;
        }
        let hub = self.client.as_ref()?.hub_addr();
        let room = self.rooms.get(index)?.clone();
        Some(Action::Join {
            addr: room_addr(hub, room.port),
            room,
        })
    }

    pub fn open_dialog(&mut self, player: &str) {
        if self.can_join() {
            self.dialog = Some(Dialog {
                name: default_room_name(player),
                phase: DialogPhase::Editing,
                error: None,
            });
        }
    }

    /// The Cancel button of the dialog (also abandons a request in flight).
    pub fn close_dialog(&mut self) {
        if self
            .dialog
            .take()
            .is_some_and(|d| d.phase == DialogPhase::Creating)
        {
            if let Some(c) = self.client.as_mut() {
                c.cancel();
            }
            self.view = Pane::Looking;
            self.next_refresh = 0.;
        }
    }

    /// The Create button: check the name, ask the hub. The answer comes through [`Online::update`].
    pub fn create(&mut self) {
        let allowed = self.view == Pane::Rooms && !self.mismatch();
        let Some(d) = self.dialog.as_mut() else {
            return;
        };
        if d.phase != DialogPhase::Editing || !allowed {
            return;
        }
        match sanitize_name(&d.name) {
            Err(e) => d.error = Some(name_error_message(e).to_string()),
            Ok(name) => {
                d.name = name.clone();
                d.error = None;
                if let Some(c) = self.client.as_mut() {
                    c.request_create_with(&name, &self.create_settings);
                    d.phase = DialogPhase::Creating;
                }
            }
        }
    }

    /// Once a frame: collect the DNS answer and the hub's replies, refresh on schedule.
    pub fn update(&mut self, now: f64) -> Option<Action> {
        if let Some(rx) = &self.resolving {
            match rx.try_recv() {
                Ok(Ok(addr)) => {
                    self.resolving = None;
                    self.attach(addr, now);
                }
                Ok(Err(e)) => {
                    self.resolving = None;
                    self.detail = Some(e.to_string());
                    self.view = Pane::Offline;
                    self.next_refresh = now + REFRESH_EVERY;
                }
                Err(TryRecvError::Disconnected) => {
                    self.resolving = None;
                    self.detail = Some("The hub's name could not be looked up.".into());
                    self.view = Pane::Offline;
                    self.next_refresh = now + REFRESH_EVERY;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        let event = self.client.as_mut().and_then(HubClient::poll);
        let mut action = None;
        if let Some(event) = event {
            match event {
                HubEvent::Rooms { build, rooms } => {
                    self.build = Some(build);
                    self.rooms = order_rooms(rooms);
                    self.view = Pane::Rooms;
                    self.detail = None;
                    self.next_refresh = now + REFRESH_EVERY;
                }
                HubEvent::Created { build, room } => {
                    self.build = Some(build);
                    self.dialog = None;
                    if let Some(c) = self.client.as_ref() {
                        action = Some(Action::Join {
                            addr: room_addr(c.hub_addr(), room.port),
                            room,
                        });
                    }
                }
                HubEvent::Error { build, code, .. } => {
                    // Build 0 means the reply did not concern a known game: keep what we knew.
                    if build != 0 {
                        self.build = Some(build);
                    }
                    let text = hub_error_message(code).to_string();
                    match self.dialog.as_mut() {
                        Some(d) => {
                            d.phase = DialogPhase::Editing;
                            d.error = Some(text);
                            self.next_refresh = now + REFRESH_EVERY;
                        }
                        None => {
                            if self.rooms.is_empty() {
                                self.view = Pane::Problem(text);
                            }
                            self.next_refresh = now + REFRESH_EVERY;
                        }
                    }
                }
                HubEvent::Timeout => {
                    self.next_refresh = now + REFRESH_EVERY;
                    match self.dialog.as_mut() {
                        Some(d) if d.phase == DialogPhase::Creating => {
                            d.phase = DialogPhase::Editing;
                            d.error = Some(
                                "The server did not answer. Check your connection and try again."
                                    .into(),
                            );
                        }
                        _ => {
                            self.view = Pane::Offline;
                            self.rooms.clear();
                            self.detail = None;
                        }
                    }
                }
            }
        }
        if let Some(c) = self.client.as_mut() {
            if !c.busy() && now >= self.next_refresh {
                c.request_list();
                self.next_refresh = now + REFRESH_EVERY;
            }
        } else if self.resolving.is_none() && self.view == Pane::Offline && now >= self.next_refresh
        {
            // The name never resolved: look again now and then, so coming back online fixes the screen by itself.
            self.resolving = Some(spawn_resolve(&self.spec));
            self.next_refresh = now + REFRESH_EVERY;
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::netplay::toy::ToyGame;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    const BUILD: u32 = 0xABCD;

    fn room(name: &str, port: u16, public: bool) -> RoomInfo {
        RoomInfo {
            name: name.into(),
            players: 1,
            capacity: 12,
            state: RoomState::Lobby,
            port,
            public,
            transport: crate::viewer::net::TransportProfile::Development,
            requires_key: false,
        }
    }

    /// A UDP socket on loopback that answers each datagram with `script(request_index, request, nonce)` replies.
    fn fake_hub(
        script: impl Fn(usize, Request, u32) -> Vec<Reply> + Send + 'static,
    ) -> (SocketAddr, Arc<AtomicUsize>) {
        fake_hub_with_build(BUILD, script)
    }

    fn fake_hub_with_build(
        build: u32,
        script: impl Fn(usize, Request, u32) -> Vec<Reply> + Send + 'static,
    ) -> (SocketAddr, Arc<AtomicUsize>) {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = socket.local_addr().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let seen = Arc::new(AtomicUsize::new(0));
        let count = seen.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            let mut idle = 0;
            while idle < 100 {
                match socket.recv_from(&mut buf) {
                    Ok((n, from)) => {
                        idle = 0;
                        let i = count.fetch_add(1, Ordering::SeqCst);
                        if let Some((nonce, req)) = Request::decode(&buf[..n]) {
                            for r in script(i, req, nonce) {
                                socket.send_to(&r.encode(nonce, build), from).unwrap();
                            }
                        }
                    }
                    Err(_) => idle += 1,
                }
            }
        });
        (addr, seen)
    }

    fn wait(c: &mut HubClient) -> HubEvent {
        let t = Instant::now();
        loop {
            if let Some(e) = c.poll() {
                return e;
            }
            assert!(t.elapsed() < Duration::from_secs(5), "no event");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    // ---- hub choice and builds ----------------------------------------------------------------------------------------

    #[test]
    fn the_hub_choice_chain_is_cli_then_server_txt_then_last_used_then_the_builtin() {
        let dir = std::env::temp_dir().join(format!("be2-hub-client-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pick = |cli, last| default_hub_in(&dir, cli, last);
        let c = pick(None, None);
        assert_eq!(
            (c.address.as_str(), c.origin),
            (DEFAULT_HUB, ServerOrigin::Builtin)
        );
        assert_eq!(
            pick(None, Some("last.example:4100")).origin,
            ServerOrigin::LastUsed
        );
        let write = |s: &str| std::fs::write(dir.join(OVERRIDE_FILE), s).unwrap();
        write("  192.168.1.20:4100  \r\nsecond line\n");
        let c = pick(None, Some("last.example"));
        assert_eq!(
            (c.address.as_str(), c.origin),
            ("192.168.1.20:4100", ServerOrigin::File)
        );
        write("\u{feff}play.example.com\n");
        assert_eq!(pick(None, None).address, "play.example.com");
        let c = pick(Some("cli.example:5000"), Some("last.example"));
        assert_eq!(
            (c.address.as_str(), c.origin),
            ("cli.example:5000", ServerOrigin::CliArg)
        );
        write("\n\n");
        assert_eq!(
            pick(None, None).address,
            DEFAULT_HUB,
            "an empty file falls back"
        );
        let _ = std::fs::remove_dir_all(&dir);
        // And the real one (beside the test binary) always yields something.
        assert!(!default_hub(None, None).address.is_empty());
    }

    #[test]
    fn the_build_is_the_games_build_id_and_a_mismatch_is_detected() {
        assert_eq!(local_build::<ToyGame>(), cli::build_id::<ToyGame>());
        assert!(build_matches::<ToyGame>(local_build::<ToyGame>()));
        assert!(!build_matches::<ToyGame>(local_build::<ToyGame>() ^ 1));
        assert_eq!(build_label(0xAB), "build 000000ab");
        assert!(update_message("Spooky Kart").contains("Spooky Kart"));
        let hub: SocketAddr = "203.0.113.5:4100".parse().unwrap();
        assert_eq!(room_addr(hub, 4103), "203.0.113.5:4103".parse().unwrap());
    }

    // ---- the client ---------------------------------------------------------------------------------------------------

    #[test]
    fn a_list_request_gets_this_games_rooms_and_the_build() {
        let (addr, _) = fake_hub(|_, req, _| {
            assert!(matches!(&req, Request::List { game, skip: 0 } if game == "kart"));
            vec![Reply::Rooms {
                game: "kart".into(),
                skip: 0,
                total: 2,
                rooms: vec![room("Public", 4101, true), room("Mine", 4102, false)],
            }]
        });
        let mut c = HubClient::new(addr, "kart").unwrap();
        assert!(c.poll().is_none() && !c.busy());
        c.request_list();
        assert!(c.busy());
        let HubEvent::Rooms { build, rooms } = wait(&mut c) else {
            panic!()
        };
        assert_eq!((build, rooms.len(), rooms[1].port), (BUILD, 2, 4102));
        assert!(!c.busy());
        assert!(c.poll().is_none(), "an event is delivered once");
    }

    #[test]
    fn a_list_for_another_game_is_ignored() {
        let (addr, _) = fake_hub(|_, _, _| {
            vec![Reply::Rooms {
                game: "other".into(),
                skip: 0,
                total: 0,
                rooms: vec![],
            }]
        });
        let mut c = HubClient::with_timing(
            addr,
            "kart",
            Duration::from_millis(50),
            Duration::from_millis(300),
        )
        .unwrap();
        c.request_list();
        assert_eq!(wait(&mut c), HubEvent::Timeout);
    }

    #[test]
    fn a_truncated_list_is_fetched_page_by_page_and_merged() {
        let (addr, seen) = fake_hub(|_, req, _| {
            let Request::List { skip, .. } = req else {
                panic!()
            };
            let all: Vec<_> = (0..5)
                .map(|i| room(&format!("R{i}"), 4101 + i, false))
                .collect();
            let page = all.iter().skip(skip as usize).take(2).cloned().collect();
            vec![Reply::Rooms {
                game: "g".into(),
                skip,
                total: 5,
                rooms: page,
            }]
        });
        let mut c = HubClient::new(addr, "g").unwrap();
        c.request_list();
        let HubEvent::Rooms { rooms, .. } = wait(&mut c) else {
            panic!()
        };
        assert_eq!(
            rooms.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["R0", "R1", "R2", "R3", "R4"]
        );
        assert_eq!(seen.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn create_fetches_a_cookie_first_then_sends_it_with_the_settings() {
        let (addr, seen) = fake_hub(|_, req, _| match req {
            Request::Cookie => vec![Reply::Cookie { token: 0xC00C1E }],
            Request::Create {
                name,
                settings,
                cookie,
                game,
            } => {
                assert_eq!((game.as_str(), cookie), ("kart", 0xC00C1E));
                assert_eq!(settings, vec![(1, 25)]);
                if name == "Taken" {
                    vec![Reply::Error {
                        code: ErrorCode::NameTaken,
                        text: "nope".into(),
                    }]
                } else {
                    vec![Reply::Created {
                        room: room(&name, 4105, false),
                    }]
                }
            }
            _ => vec![],
        });
        let mut c = HubClient::new(addr, "kart").unwrap();
        c.request_create_with("Taken", &[(1, 25)]);
        assert!(
            matches!(wait(&mut c), HubEvent::Error { code: ErrorCode::NameTaken, ref text, .. } if text == "nope")
        );
        c.request_create_with("Fresh", &[(1, 25)]);
        let HubEvent::Created { room, build } = wait(&mut c) else {
            panic!()
        };
        assert_eq!(
            (room.name.as_str(), room.port, build),
            ("Fresh", 4105, BUILD)
        );
        assert_eq!(seen.load(Ordering::SeqCst), 4, "cookie + create, twice");
    }

    #[test]
    fn an_expired_cookie_is_replaced_once_automatically_and_not_forever() {
        let (addr, seen) = fake_hub(|i, req, _| match req {
            Request::Cookie => vec![Reply::Cookie { token: i as u64 }],
            Request::Create { name, cookie, .. } => {
                if cookie < 2 {
                    vec![Reply::Error {
                        code: ErrorCode::BadCookie,
                        text: "old".into(),
                    }]
                } else {
                    vec![Reply::Created {
                        room: room(&name, 4105, false),
                    }]
                }
            }
            _ => vec![],
        });
        let mut c = HubClient::new(addr, "kart").unwrap();
        c.request_create("Fresh");
        assert!(
            matches!(wait(&mut c), HubEvent::Created { .. }),
            "the second cookie is accepted"
        );
        assert_eq!(seen.load(Ordering::SeqCst), 4);
        // A hub that always says BadCookie: one retry, then the error is shown.
        let (addr, seen) = fake_hub(|_, req, _| match req {
            Request::Cookie => vec![Reply::Cookie { token: 1 }],
            _ => vec![Reply::Error {
                code: ErrorCode::BadCookie,
                text: "old".into(),
            }],
        });
        let mut c = HubClient::new(addr, "kart").unwrap();
        c.request_create("Fresh");
        assert!(matches!(
            wait(&mut c),
            HubEvent::Error {
                code: ErrorCode::BadCookie,
                ..
            }
        ));
        assert_eq!(seen.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn replies_from_elsewhere_or_with_another_nonce_do_not_count() {
        let (addr, _) = fake_hub(|_, _, _| vec![]);
        let mut c = HubClient::with_timing(
            addr,
            "g",
            Duration::from_millis(50),
            Duration::from_millis(300),
        )
        .unwrap();
        c.request_list();
        let stranger = UdpSocket::bind("127.0.0.1:0").unwrap();
        let target: SocketAddr = format!("127.0.0.1:{}", c.local_port()).parse().unwrap();
        let reply = Reply::Rooms {
            game: "g".into(),
            skip: 0,
            total: 0,
            rooms: vec![],
        };
        stranger
            .send_to(&reply.encode(c.nonce().unwrap(), 1), target)
            .unwrap();
        assert_eq!(
            wait(&mut c),
            HubEvent::Timeout,
            "a datagram from another address is ignored"
        );
    }

    #[test]
    fn silence_retries_then_times_out_and_the_client_can_be_reused() {
        let (addr, seen) = fake_hub(|_, _, _| vec![]);
        let mut c = HubClient::with_timing(
            addr,
            "g",
            Duration::from_millis(100),
            Duration::from_millis(450),
        )
        .unwrap();
        c.request_list();
        assert_eq!(wait(&mut c), HubEvent::Timeout);
        assert!(
            seen.load(Ordering::SeqCst) >= 3,
            "retried: {}",
            seen.load(Ordering::SeqCst)
        );
        assert!(!c.busy());
        c.request_list();
        assert!(c.busy());
        c.cancel();
        assert!(!c.busy());
    }

    #[test]
    fn the_client_refuses_ipv6_hubs_and_bad_game_ids() {
        assert!(HubClient::new("[::1]:4100".parse().unwrap(), "g").is_err());
        assert!(HubClient::new("127.0.0.1:4100".parse().unwrap(), "Not A Game").is_err());
    }

    // ---- pure rules ---------------------------------------------------------------------------------------------------

    fn rr(name: &str, players: u8, public: bool) -> RoomInfo {
        RoomInfo {
            name: name.into(),
            players,
            capacity: 12,
            state: RoomState::Lobby,
            port: 4100,
            public,
            transport: crate::viewer::net::TransportProfile::Development,
            requires_key: false,
        }
    }

    #[test]
    fn rooms_are_ordered_public_first_then_busiest_then_by_name() {
        let list = order_rooms(vec![
            rr("zed", 1, false),
            rr("Alpha", 3, false),
            rr("beta", 3, false),
            rr("Public", 0, true),
            rr("Mid", 5, false),
        ]);
        let names: Vec<_> = list.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Public", "Mid", "Alpha", "beta", "zed"]);
        let list = order_rooms(vec![rr("Big", 11, false), rr("Public", 0, true)]);
        assert_eq!(list[0].name, "Public");
        assert!(order_rooms(vec![]).is_empty());
    }

    #[test]
    fn room_rows_say_players_and_state() {
        let mut r = rr("x", 3, false);
        assert_eq!(room_status(&r), "3/12  Lobby");
        r.state = RoomState::Playing;
        assert_eq!(room_status(&r), "3/12  In match");
        r.players = 12;
        assert_eq!(room_status(&r), "12/12  Full");
    }

    #[test]
    fn the_default_room_name_is_always_acceptable_to_the_hub() {
        assert_eq!(default_room_name("Kevin"), "Kevin's room");
        assert_eq!(default_room_name("  "), "My room");
        assert_eq!(default_room_name("!!!"), "My room");
        assert_eq!(default_room_name("kevin_99"), "kevin99's room");
        for who in [
            "Kevin",
            "A very long player name",
            "字字字字",
            "x-y",
            "O'Neil",
            "",
        ] {
            let n = default_room_name(who);
            assert_eq!(sanitize_name(&n).as_deref(), Ok(n.as_str()), "{who}");
            assert!(n.chars().count() <= MAX_NAME_CHARS);
        }
    }

    #[test]
    fn every_hub_error_has_a_friendly_sentence() {
        use ErrorCode::*;
        for code in [
            BadName,
            NameTaken,
            Full,
            RateLimited,
            Unavailable,
            BadRequest,
            UnknownGame,
            BadCookie,
            BadSetting,
        ] {
            let m = hub_error_message(code);
            assert!(m.ends_with('.') && m.len() > 20, "{code:?}: {m}");
            assert!(!m.to_lowercase().contains("turned you away"));
        }
        assert!(hub_error_message(NameTaken).contains("already exists"));
        assert!(hub_error_message(Full).contains("in use"));
        assert!(hub_error_message(RateLimited).contains("Wait"));
        assert!(hub_error_message(Unavailable).contains("could not start"));
        assert!(hub_error_message(BadName).contains("letters"));
        assert!(hub_error_message(UnknownGame).contains("does not host"));
    }

    #[test]
    fn connection_failures_read_honestly() {
        let addr: SocketAddr = "1.2.3.4:4101".parse().unwrap();
        let none = ConnectFailure::Unreachable {
            addr,
            waited_secs: 8,
        };
        let listed = connect_failure_message(&none, true);
        assert!(
            listed.contains("1.2.3.4:4101")
                && listed.contains("8 s")
                && !listed.contains("forwarded"),
            "{listed}"
        );
        let typed = connect_failure_message(&none, false);
        assert!(
            typed.contains("forwarded") && !typed.contains("turned you away"),
            "{typed}"
        );
        let refused = connect_failure_message(&ConnectFailure::classify("Banned"), true);
        assert!(refused.contains("turned you away: Banned"), "{refused}");
        assert!(
            connect_failure_message(&ConnectFailure::VersionMismatch, true)
                .contains("different version")
        );
        assert!(should_retry_join(&ConnectFailure::MatchInProgress));
        for f in [
            ConnectFailure::Full,
            ConnectFailure::WrongKey,
            ConnectFailure::VersionMismatch,
            none,
        ] {
            assert!(!should_retry_join(&f));
        }
    }

    #[test]
    fn a_running_match_is_retried_every_three_seconds_until_the_give_up_time() {
        let mut w = JoinWait::new(100.);
        assert_eq!(w.step(100.), RetryStep::Wait);
        assert_eq!(w.step(102.9), RetryStep::Wait);
        assert_eq!(w.step(103.), RetryStep::TryNow);
        w.attempted(103.2);
        assert_eq!(
            (w.attempts, w.step(105.), w.step(106.2)),
            (1, RetryStep::Wait, RetryStep::TryNow)
        );
        assert_eq!(w.step(100. + JOIN_GIVE_UP), RetryStep::GiveUp);
        assert_eq!(
            w.step(100. + JOIN_GIVE_UP - 1.),
            RetryStep::TryNow,
            "an overdue attempt is not postponed"
        );
    }

    #[test]
    fn the_list_scrolls_to_keep_the_highlight_in_view() {
        assert_eq!(scroll_to(0, 0, 5, 3), 0);
        assert_eq!(scroll_to(5, 0, 5, 10), 1);
        assert_eq!(scroll_to(9, 1, 5, 10), 5);
        assert_eq!(scroll_to(2, 5, 5, 10), 2);
        assert_eq!(scroll_to(4, 2, 5, 10), 2);
        assert_eq!(scroll_to(0, 7, 5, 6), 0);
    }

    // ---- the state machine --------------------------------------------------------------------------------------------

    fn run_until(o: &mut Online, mut done: impl FnMut(&Online) -> bool) -> Option<Action> {
        let start = Instant::now();
        let mut action = None;
        while !done(o) {
            assert!(
                start.elapsed() < Duration::from_secs(6),
                "timed out in state {:?}",
                o.view
            );
            let now = start.elapsed().as_secs_f64();
            action = o.update(now).or(action);
            std::thread::sleep(Duration::from_millis(10));
        }
        action
    }

    #[test]
    fn listing_creating_and_joining_through_a_hub() {
        let (hub, _) = fake_hub(|_, req, _| match req {
            Request::List { game, .. } => vec![Reply::Rooms {
                game,
                skip: 0,
                total: 2,
                rooms: vec![room("Mine", 4102, false), room("Public", 4101, true)],
            }],
            Request::Cookie => vec![Reply::Cookie { token: 5 }],
            Request::Create { name, settings, .. } if name == "Taken" => {
                assert_eq!(settings, vec![(2, 1)]);
                vec![Reply::Error {
                    code: ErrorCode::NameTaken,
                    text: "raw hub text".into(),
                }]
            }
            Request::Create { name, .. } => vec![Reply::Created {
                room: room(&name, 4105, false),
            }],
            _ => vec![],
        });
        let mut o = Online::at(hub, "kart", BUILD, 0.);
        assert_eq!((o.view.clone(), o.game()), (Pane::Looking, "kart"));
        run_until(&mut o, |o| o.view == Pane::Rooms);
        assert_eq!(o.rooms[0].name, "Public", "public first");
        assert!(o.can_join() && o.hub_addr() == Some(hub));
        assert_eq!(o.selected_room().unwrap().name, "Public");
        let Some(Action::Join { addr, room }) = o.join_selected() else {
            panic!()
        };
        assert_eq!((addr, room.public), (SocketAddr::new(hub.ip(), 4101), true));
        let Some(Action::Join { addr, .. }) = o.join_index(1) else {
            panic!()
        };
        assert_eq!(addr.port(), 4102);
        // A bad name is caught locally; a taken one comes back as a friendly sentence and the dialog stays.
        o.open_dialog("Kevin");
        assert_eq!(o.dialog.as_ref().unwrap().name, "Kevin's room");
        o.dialog.as_mut().unwrap().name = "***".into();
        o.create();
        assert!(o
            .dialog
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .contains("letters"));
        o.set_create_settings(&[(2, 1)]);
        o.dialog.as_mut().unwrap().name = "Taken".into();
        o.create();
        assert_eq!(o.dialog.as_ref().unwrap().phase, DialogPhase::Creating);
        run_until(&mut o, |o| {
            o.dialog
                .as_ref()
                .is_some_and(|d| d.phase == DialogPhase::Editing)
        });
        assert!(o
            .dialog
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .contains("already exists"));
        // A good one is made and joined at once.
        o.set_create_settings(&[]);
        o.dialog.as_mut().unwrap().name = "  Fresh   room ".into();
        o.create();
        let action = run_until(&mut o, |o| o.dialog.is_none());
        let Some(Action::Join { addr, room }) = action else {
            panic!("no join: {action:?}")
        };
        assert_eq!((room.name.as_str(), addr.port()), ("Fresh room", 4105));
    }

    #[test]
    fn a_silent_hub_is_offline_and_a_different_build_blocks_joining() {
        let (silent, _) = fake_hub(|_, _, _| vec![]);
        let mut o = Online::at(silent, "kart", BUILD, 0.);
        run_until(&mut o, |o| o.view == Pane::Offline);
        assert!(!o.can_join() && o.join_selected().is_none() && o.rooms.is_empty());
        o.open_dialog("Kevin");
        assert!(o.dialog.is_none(), "no Create Room while offline");

        // A hub that reports another build for the game: the list shows, but Join and Create are off.
        let (other, _) = fake_hub_with_build(BUILD ^ 0xFFFF, |_, req, _| match req {
            Request::List { game, .. } => vec![Reply::Rooms {
                game,
                skip: 0,
                total: 1,
                rooms: vec![room("Public", 4101, true)],
            }],
            _ => vec![],
        });
        let mut o = Online::at(other, "kart", BUILD, 0.);
        run_until(&mut o, |o| o.view == Pane::Rooms);
        assert!(o.mismatch() && !o.can_join());
        assert!(o.join_selected().is_none() && o.join_index(0).is_none());
        o.open_dialog("Kevin");
        assert!(o.dialog.is_none());
    }

    #[test]
    fn a_hub_that_does_not_host_the_game_says_so_and_keeps_the_known_build() {
        let (hub, _) = fake_hub_with_build(0, |_, _, _| {
            vec![Reply::Error {
                code: ErrorCode::UnknownGame,
                text: "Unknown game.".into(),
            }]
        });
        let mut o = Online::at(hub, "kart", BUILD, 0.);
        run_until(&mut o, |o| matches!(o.view, Pane::Problem(_)));
        let Pane::Problem(text) = &o.view else {
            panic!()
        };
        assert!(text.contains("does not host"), "{text}");
        assert!(o.build.is_none() && !o.mismatch(), "build 0 is not a build");
    }

    #[test]
    fn an_unresolvable_hub_name_is_offline_not_a_hang() {
        let mut o = Online::new("nonexistent.invalid", "kart", BUILD, 0.);
        assert_eq!(o.view, Pane::Looking);
        run_until(&mut o, |o| o.view == Pane::Offline);
        assert!(o
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("nonexistent.invalid")));
        let mut o = Online::new("bad host name", "kart", BUILD, 0.);
        run_until(&mut o, |o| o.view == Pane::Offline);
    }
}
