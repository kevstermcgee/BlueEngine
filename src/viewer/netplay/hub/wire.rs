//! The hub's datagram protocol, `BEHB` version 1, and the loopback control protocol `BECT`.
//!
//! One UDP datagram each way, little endian. A request is padded with zeros up to a minimum length for its kind
//! and a reply is capped per kind, so the hub can never be used to amplify traffic by more than a small factor
//! ([`min_request_len`], [`max_reply_len`]); anything it does not understand gets silence.
//!
//! ```text
//! request: "BEHB" version(1) kind(1) nonce(u32) payload...        zero-padded to the kind's minimum length
//! reply:   "BEHB" version(1) kind(1) nonce(u32) build(u32) payload...
//!
//! game id  = len(u8) bytes                    1..=24 of a-z 0-9 -
//! string   = len(u8) utf8 bytes
//! room     = name(string<=96) players(u8) capacity(u8) state(u8: 0 lobby, 1 match) port(u16) public(u8)
//!
//! request kinds                                   min length
//!   1 List    game skip(u8)                          200
//!   2 Create  game nsettings(u8<=8) {id(u8) value(u32)}* cookie(u64) name(string)     96
//!   3 Ping    game                                    32
//!   4 Cookie  (nothing)                               32
//! reply kinds                                     max length
//!   0x81 Rooms   game skip(u8) total(u8) count(u8) room*                (List: 1000)
//!   0x82 Created room                                                   (Create: 128)
//!   0x83 Error   code(u8) text(string<=80)                              (the request's cap)
//!   0x84 Pong                                                           (Ping: 32)
//!   0x85 Cookie  token(u64)                                             (Cookie: 32)
//! ```
//! `build` is the game's build id ([`cli::build_id`](crate::viewer::netplay::cli::build_id)) as the hub learned it
//! from the game's server; it is 0 in replies that do not concern a known game. The nonce is echoed so a client
//! can match a reply to its request.
//!
//! **Cookie.** A source address can be forged on UDP, so a Create is honoured only when it carries the token a
//! `Cookie` request returned to that same address a short while before: whoever forges a source cannot see the
//! reply and cannot make a room in somebody else's name. The token is a keyed hash of the source IP and a coarse
//! time window; the key never leaves the hub.
//!
//! **Control (`BECT`).** `"BECT" version(1) kind(1) nonce(u32) game` with kind 1 = reload and kind 2 = status; the
//! reply is `"BECT" version kind|0x80 nonce ok(u8) text(string)`. The hub honours it only from a loopback address.
//! Reload's `ok` means "the hub re-read the game and asked for a replacement room", not that the room is ready;
//! status's text is what the hub holds right now (see [`deploy::GameStatus`](super::deploy::GameStatus)).
//! Hubs older than status ignore kind 2 (silence), which `be2-hub status` reports as "no answer".
use std::fmt;

pub const MAGIC: [u8; 4] = *b"BEHB";
pub const VERSION: u8 = 1;
/// The default hub port.
pub const DEFAULT_PORT: u16 = 4100;
/// Every reply fits one safe datagram.
pub const MAX_REPLY: usize = 1000;
/// Requests longer than this are ignored.
pub const MAX_REQUEST: usize = 512;
/// A room name is at most this many characters.
pub const MAX_NAME_CHARS: usize = 24;
/// A Create carries at most this many settings.
pub const MAX_SETTINGS: usize = 8;
/// The longest game id.
pub const MAX_GAME_ID: usize = 24;

pub const CONTROL_MAGIC: [u8; 4] = *b"BECT";

const REQ_LIST: u8 = 1;
const REQ_CREATE: u8 = 2;
const REQ_PING: u8 = 3;
const REQ_COOKIE: u8 = 4;
const REP_ROOMS: u8 = 0x81;
const REP_CREATED: u8 = 0x82;
const REP_ERROR: u8 = 0x83;
const REP_PONG: u8 = 0x84;
const REP_COOKIE: u8 = 0x85;
pub(super) const REQUEST_HEADER: usize = 10;
pub(super) const REPLY_HEADER: usize = 14;
pub(super) const MAX_ERROR_TEXT: usize = 80;

/// A request, as a client sends it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// The room list of a game, starting at room number `skip` (a reply holds as many rooms as fit one datagram).
    List { game: String, skip: u8 },
    /// Make a room. `settings` are `(setting id, value)` pairs from the game's schema; `cookie` comes from a
    /// [`Request::Cookie`] answered to this same address.
    Create {
        game: String,
        name: String,
        settings: Vec<(u8, u32)>,
        cookie: u64,
    },
    /// Is the hub there, and what build is this game?
    Ping { game: String },
    /// Ask for the token a Create must carry.
    Cookie,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomState {
    Lobby,
    Playing,
}

/// One room as listed to clients.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomInfo {
    pub name: String,
    pub players: u8,
    pub capacity: u8,
    pub state: RoomState,
    /// The UDP port of the room's game server, on the hub's host.
    pub port: u16,
    /// The permanent Public room of the game.
    pub public: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    BadName = 1,
    NameTaken = 2,
    /// A room cap is reached (the game's, the hub's, or the sender's own).
    Full = 3,
    RateLimited = 4,
    /// The hub could not start a server process.
    Unavailable = 5,
    BadRequest = 6,
    /// The hub does not carry this game.
    UnknownGame = 7,
    /// The Create's cookie was missing, wrong or too old: ask for a new one.
    BadCookie = 8,
    /// A setting id is unknown or its value out of range.
    BadSetting = 9,
}

impl ErrorCode {
    pub fn from_u8(v: u8) -> ErrorCode {
        match v {
            1 => ErrorCode::BadName,
            2 => ErrorCode::NameTaken,
            3 => ErrorCode::Full,
            4 => ErrorCode::RateLimited,
            5 => ErrorCode::Unavailable,
            7 => ErrorCode::UnknownGame,
            8 => ErrorCode::BadCookie,
            9 => ErrorCode::BadSetting,
            _ => ErrorCode::BadRequest,
        }
    }
}

/// A reply, as the hub sends it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// `rooms` are numbers `skip..skip + rooms.len()` of `total`.
    Rooms {
        game: String,
        skip: u8,
        total: u8,
        rooms: Vec<RoomInfo>,
    },
    Created {
        room: RoomInfo,
    },
    Cookie {
        token: u64,
    },
    Error {
        code: ErrorCode,
        text: String,
    },
    Pong,
}

/// A decoded reply with its envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyPacket {
    pub nonce: u32,
    /// The game's build id as the hub knows it (0 when the reply does not concern a known game).
    pub build: u32,
    pub reply: Reply,
}

/// The shortest datagram the hub answers for a request (clients pad up to it).
pub fn min_request_len(request: &Request) -> usize {
    match request {
        Request::Ping { .. } | Request::Cookie => 32,
        Request::Create { .. } => 96,
        Request::List { .. } => 200,
    }
}

/// The longest reply the hub sends to a request: at most about five times the (padded) request.
pub fn max_reply_len(request: &Request) -> usize {
    match request {
        Request::Ping { .. } | Request::Cookie => 32,
        Request::Create { .. } => 128,
        Request::List { .. } => MAX_REPLY,
    }
}

/// Is this a valid game id: 1..=24 of `a-z`, `0-9`, `-`, not starting with `-`.
pub fn game_id_ok(id: &str) -> bool {
    crate::viewer::netplay::cli::game_id_ok(id)
}

pub(super) struct Put(pub Vec<u8>);
impl Put {
    pub fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn str(&mut self, s: &str, max: usize) {
        let mut end = s.len().min(max).min(255);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        self.u8(end as u8);
        self.0.extend_from_slice(&s.as_bytes()[..end]);
    }
    pub fn room(&mut self, r: &RoomInfo) {
        self.str(&r.name, 96);
        self.u8(r.players);
        self.u8(r.capacity);
        self.u8(matches!(r.state, RoomState::Playing) as u8);
        self.u16(r.port);
        self.u8(r.public as u8);
    }
}

pub(super) struct Cur<'a>(pub &'a [u8]);
impl Cur<'_> {
    pub fn take(&mut self, n: usize) -> Option<&[u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }
    pub fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    pub fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn u64(&mut self) -> Option<u64> {
        self.take(8).map(|b| {
            let mut a = [0u8; 8];
            a.copy_from_slice(b);
            u64::from_le_bytes(a)
        })
    }
    pub fn str(&mut self) -> Option<String> {
        let n = self.u8()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
    /// A game id: a string that is also a valid id.
    pub fn game(&mut self) -> Option<String> {
        self.str().filter(|g| game_id_ok(g))
    }
    pub fn room(&mut self) -> Option<RoomInfo> {
        Some(RoomInfo {
            name: self.str()?,
            players: self.u8()?,
            capacity: self.u8()?,
            state: if self.u8()? == 0 {
                RoomState::Lobby
            } else {
                RoomState::Playing
            },
            port: self.u16()?,
            public: self.u8()? != 0,
        })
    }
}

fn header(kind: u8, nonce: u32) -> Put {
    let mut p = Put(Vec::with_capacity(256));
    p.0.extend_from_slice(&MAGIC);
    p.u8(VERSION);
    p.u8(kind);
    p.u32(nonce);
    p
}

fn check_header(data: &[u8]) -> Option<(u8, u32, Cur<'_>)> {
    if data.len() < REQUEST_HEADER || data[..4] != MAGIC || data[4] != VERSION {
        return None;
    }
    let nonce = u32::from_le_bytes([data[6], data[7], data[8], data[9]]);
    Some((data[5], nonce, Cur(&data[REQUEST_HEADER..])))
}

impl Request {
    /// The datagram for this request, padded to its minimum length.
    pub fn encode(&self, nonce: u32) -> Vec<u8> {
        let mut p = match self {
            Request::List { .. } => header(REQ_LIST, nonce),
            Request::Create { .. } => header(REQ_CREATE, nonce),
            Request::Ping { .. } => header(REQ_PING, nonce),
            Request::Cookie => header(REQ_COOKIE, nonce),
        };
        match self {
            Request::List { game, skip } => {
                p.str(game, MAX_GAME_ID);
                p.u8(*skip);
            }
            Request::Create {
                game,
                name,
                settings,
                cookie,
            } => {
                p.str(game, MAX_GAME_ID);
                let n = settings.len().min(MAX_SETTINGS);
                p.u8(n as u8);
                for (id, value) in &settings[..n] {
                    p.u8(*id);
                    p.u32(*value);
                }
                p.u64(*cookie);
                p.str(name, 96);
            }
            Request::Ping { game } => p.str(game, MAX_GAME_ID),
            Request::Cookie => {}
        }
        let min = min_request_len(self);
        if p.0.len() < min {
            p.0.resize(min, 0);
        }
        p.0
    }

    /// `None` for anything that is not a well-formed request of this version (trailing padding is ignored).
    pub fn decode(data: &[u8]) -> Option<(u32, Request)> {
        let (kind, nonce, mut c) = check_header(data)?;
        let request = match kind {
            REQ_LIST => Request::List {
                game: c.game()?,
                skip: c.u8()?,
            },
            REQ_CREATE => {
                let game = c.game()?;
                let n = c.u8()? as usize;
                if n > MAX_SETTINGS {
                    return None;
                }
                let mut settings = Vec::with_capacity(n);
                for _ in 0..n {
                    settings.push((c.u8()?, c.u32()?));
                }
                Request::Create {
                    game,
                    settings,
                    cookie: c.u64()?,
                    name: c.str()?,
                }
            }
            REQ_PING => Request::Ping { game: c.game()? },
            REQ_COOKIE => Request::Cookie,
            _ => return None,
        };
        Some((nonce, request))
    }
}

impl Reply {
    /// Encoded size of one listed room.
    pub fn room_len(r: &RoomInfo) -> usize {
        1 + r.name.len().min(96) + 6
    }

    pub fn encode(&self, nonce: u32, build: u32) -> Vec<u8> {
        let mut p = match self {
            Reply::Rooms { .. } => header(REP_ROOMS, nonce),
            Reply::Created { .. } => header(REP_CREATED, nonce),
            Reply::Error { .. } => header(REP_ERROR, nonce),
            Reply::Pong => header(REP_PONG, nonce),
            Reply::Cookie { .. } => header(REP_COOKIE, nonce),
        };
        p.u32(build);
        match self {
            Reply::Rooms {
                game,
                skip,
                total,
                rooms,
            } => {
                p.str(game, MAX_GAME_ID);
                p.u8(*skip);
                p.u8(*total);
                p.u8(rooms.len().min(255) as u8);
                for r in rooms {
                    p.room(r);
                }
            }
            Reply::Created { room } => p.room(room),
            Reply::Error { code, text } => {
                p.u8(*code as u8);
                p.str(text, MAX_ERROR_TEXT);
            }
            Reply::Pong => {}
            Reply::Cookie { token } => p.u64(*token),
        }
        p.0
    }

    /// [`Reply::encode`], but an error's text is shortened until the datagram is at most `max_len` bytes (an error
    /// answering a Ping must stay as small as the Ping).
    pub fn encode_within(&self, nonce: u32, build: u32, max_len: usize) -> Vec<u8> {
        if let Reply::Error { code, text } = self {
            let mut p = header(REP_ERROR, nonce);
            p.u32(build);
            p.u8(*code as u8);
            p.str(
                text,
                max_len.saturating_sub(REPLY_HEADER + 2).min(MAX_ERROR_TEXT),
            );
            return p.0;
        }
        self.encode(nonce, build)
    }

    pub fn decode(data: &[u8]) -> Option<ReplyPacket> {
        if data.len() < REPLY_HEADER {
            return None;
        }
        let (kind, nonce, mut c) = check_header(data)?;
        let build = c.u32()?;
        let reply = match kind {
            REP_ROOMS => {
                let game = c.game()?;
                let (skip, total, n) = (c.u8()?, c.u8()?, c.u8()? as usize);
                let mut rooms = Vec::with_capacity(n.min(32));
                for _ in 0..n {
                    rooms.push(c.room()?);
                }
                Reply::Rooms {
                    game,
                    skip,
                    total,
                    rooms,
                }
            }
            REP_CREATED => Reply::Created { room: c.room()? },
            REP_ERROR => Reply::Error {
                code: ErrorCode::from_u8(c.u8()?),
                text: c.str()?,
            },
            REP_PONG => Reply::Pong,
            REP_COOKIE => Reply::Cookie { token: c.u64()? },
            _ => return None,
        };
        Some(ReplyPacket {
            nonce,
            build,
            reply,
        })
    }
}

// ---- control (loopback only) ----------------------------------------------------------------------------------------

/// A request to the running hub from the machine it runs on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Control {
    /// Re-read the registry for this game and retire its rooms (they close once empty; new rooms use the new
    /// build). See the module docs of [`rooms`](super::rooms) for the exact semantics.
    Reload { game: String },
    /// Report what the hub holds for this game: the registry's build, and the Public room's process and status.
    /// Changes nothing.
    Status { game: String },
}

/// The hub's answer to a [`Control`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlReply {
    pub ok: bool,
    pub text: String,
}

impl Control {
    pub fn encode(&self, nonce: u32) -> Vec<u8> {
        let (kind, game) = match self {
            Control::Reload { game } => (1, game),
            Control::Status { game } => (2, game),
        };
        let mut p = Put(Vec::new());
        p.0.extend_from_slice(&CONTROL_MAGIC);
        p.u8(VERSION);
        p.u8(kind);
        p.u32(nonce);
        p.str(game, MAX_GAME_ID);
        p.0
    }

    pub fn decode(data: &[u8]) -> Option<(u32, Control)> {
        if data.len() < REQUEST_HEADER || data[..4] != CONTROL_MAGIC || data[4] != VERSION {
            return None;
        }
        let nonce = u32::from_le_bytes([data[6], data[7], data[8], data[9]]);
        let mut c = Cur(&data[REQUEST_HEADER..]);
        match data[5] {
            1 => Some((nonce, Control::Reload { game: c.game()? })),
            2 => Some((nonce, Control::Status { game: c.game()? })),
            _ => None,
        }
    }
}

impl ControlReply {
    pub fn encode(&self, nonce: u32) -> Vec<u8> {
        let mut p = Put(Vec::new());
        p.0.extend_from_slice(&CONTROL_MAGIC);
        p.u8(VERSION);
        p.u8(0x81);
        p.u32(nonce);
        p.u8(self.ok as u8);
        p.str(&self.text, 200);
        p.0
    }

    pub fn decode(data: &[u8]) -> Option<(u32, ControlReply)> {
        if data.len() < REQUEST_HEADER
            || data[..4] != CONTROL_MAGIC
            || data[4] != VERSION
            || data[5] != 0x81
        {
            return None;
        }
        let nonce = u32::from_le_bytes([data[6], data[7], data[8], data[9]]);
        let mut c = Cur(&data[REQUEST_HEADER..]);
        Some((
            nonce,
            ControlReply {
                ok: c.u8()? != 0,
                text: c.str()?,
            },
        ))
    }
}

// ---- room names -----------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameError {
    Empty,
    TooLong,
    /// Only letters, digits, spaces, apostrophes and hyphens are allowed.
    BadCharacter,
}

impl NameError {
    pub fn text(self) -> &'static str {
        match self {
            NameError::Empty => "Give the room a name.",
            NameError::TooLong => "That name is too long (24 characters at most).",
            NameError::BadCharacter => {
                "Use letters, numbers, spaces, apostrophes and hyphens only."
            }
        }
    }
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str((*self).text())
    }
}

/// Clean a room name: trim, collapse runs of spaces, and accept only Unicode letters, digits, spaces, apostrophes and
/// hyphens (so no control characters, no look-alike tricks, nothing that needs escaping in a log). At most
/// [`MAX_NAME_CHARS`] characters and at least one letter or digit.
pub fn sanitize_name(raw: &str) -> Result<String, NameError> {
    let mut out = String::new();
    let mut last_space = false;
    for ch in raw.trim_matches(' ').chars() {
        if ch == ' ' {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
            continue;
        }
        last_space = false;
        if !(ch.is_alphanumeric() || ch == '\'' || ch == '-') {
            return Err(NameError::BadCharacter);
        }
        out.push(ch);
    }
    if !out.chars().any(char::is_alphanumeric) {
        return Err(if out.is_empty() && raw.chars().all(char::is_whitespace) {
            NameError::Empty
        } else {
            NameError::BadCharacter
        });
    }
    if out.chars().count() > MAX_NAME_CHARS {
        return Err(NameError::TooLong);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(name: &str, port: u16) -> RoomInfo {
        RoomInfo {
            name: name.into(),
            players: 3,
            capacity: 12,
            state: RoomState::Playing,
            port,
            public: false,
        }
    }

    fn create() -> Request {
        Request::Create {
            game: "spooky-kart".into(),
            name: "Kevin's place".into(),
            settings: vec![(1, 25), (2, 1)],
            cookie: 0x0123_4567_89ab_cdef,
        }
    }

    #[test]
    fn requests_round_trip_and_stay_in_their_size_classes() {
        for r in [
            Request::Ping {
                game: "deadfall".into(),
            },
            Request::Cookie,
            Request::List {
                game: "a".into(),
                skip: 3,
            },
            create(),
            Request::Create {
                game: "g".into(),
                name: "字".repeat(24),
                settings: vec![(255, u32::MAX); MAX_SETTINGS],
                cookie: u64::MAX,
            },
        ] {
            let bytes = r.encode(0xDEAD_BEEF);
            assert!(
                bytes.len() >= min_request_len(&r) && bytes.len() <= MAX_REQUEST,
                "{r:?}"
            );
            assert_eq!(Request::decode(&bytes), Some((0xDEAD_BEEF, r)));
        }
    }

    #[test]
    fn the_request_layout_is_the_documented_one() {
        let bytes = Request::List {
            game: "kart".into(),
            skip: 7,
        }
        .encode(0x0403_0201);
        assert_eq!(&bytes[..10], b"BEHB\x01\x01\x01\x02\x03\x04");
        assert_eq!(&bytes[10..16], b"\x04kart\x07");
        assert!(bytes[16..].iter().all(|b| *b == 0) && bytes.len() == 200);
        let bytes = create().encode(1);
        assert_eq!(&bytes[10..24], b"\x0bspooky-kart\x02\x01");
        // settings (1,25) (2,1), cookie, name
        assert_eq!(&bytes[24..28], &[25, 0, 0, 0]);
        assert_eq!(&bytes[28..33], &[2, 1, 0, 0, 0]);
        assert_eq!(&bytes[33..41], &0x0123_4567_89ab_cdefu64.to_le_bytes());
        assert_eq!(&bytes[41..42], &[13]);
        assert_eq!(&bytes[42..55], b"Kevin's place");
    }

    #[test]
    fn replies_round_trip_and_stay_in_their_size_classes() {
        let rooms = Reply::Rooms {
            game: "deadfall".into(),
            skip: 1,
            total: 2,
            rooms: vec![room("Ünïcode room", 4102), room("b", 4103)],
        };
        for (r, request) in [
            (Reply::Pong, Request::Cookie),
            (Reply::Cookie { token: u64::MAX }, Request::Cookie),
            (
                Reply::Created {
                    room: room(&"字".repeat(24), 4102),
                },
                create(),
            ),
            (
                Reply::Error {
                    code: ErrorCode::UnknownGame,
                    text: "x".repeat(300),
                },
                create(),
            ),
            (
                rooms,
                Request::List {
                    game: "g".into(),
                    skip: 0,
                },
            ),
        ] {
            let bytes = r.encode_within(7, 0x1234_5678, max_reply_len(&request));
            assert!(bytes.len() <= max_reply_len(&request), "{r:?}");
            let p = Reply::decode(&bytes).unwrap();
            assert_eq!((p.nonce, p.build), (7, 0x1234_5678));
            match (&r, &p.reply) {
                (Reply::Error { .. }, Reply::Error { code, text }) => {
                    assert!(*code == ErrorCode::UnknownGame && text.len() == MAX_ERROR_TEXT)
                }
                _ => assert_eq!(p.reply, r),
            }
        }
    }

    #[test]
    fn an_error_answering_a_small_request_is_no_bigger_than_it() {
        let pong_class = max_reply_len(&Request::Cookie);
        let bytes = Reply::Error {
            code: ErrorCode::UnknownGame,
            text: "That game is not hosted here, sorry about that.".into(),
        }
        .encode_within(1, 0, pong_class);
        assert!(bytes.len() <= pong_class, "{} bytes", bytes.len());
        let Reply::Error { code, text } = Reply::decode(&bytes).unwrap().reply else {
            panic!()
        };
        assert_eq!(code, ErrorCode::UnknownGame);
        assert!(text.starts_with("That game"));
    }

    #[test]
    fn garbage_and_truncated_datagrams_decode_to_nothing() {
        assert!(Request::decode(b"").is_none());
        assert!(Request::decode(b"hello world, this is not a hub request").is_none());
        let mut ok = Request::List {
            game: "g".into(),
            skip: 0,
        }
        .encode(1);
        ok[4] = VERSION + 1;
        assert!(
            Request::decode(&ok).is_none(),
            "another version is not understood"
        );
        let mut ok = Request::Cookie.encode(1);
        ok[5] = 99;
        assert!(Request::decode(&ok).is_none(), "an unknown kind");
        assert!(Request::decode(&Request::Cookie.encode(1)[..9]).is_none());
        // A game id that is not one, or runs off the end.
        for bad in ["", "Upper", "has space", "-lead", "a_b", &"x".repeat(25)] {
            let mut p = header(REQ_PING, 1);
            p.str(bad, 255);
            p.0.resize(32, 0);
            assert!(Request::decode(&p.0).is_none(), "game id {bad:?}");
        }
        let mut p = header(REQ_PING, 1);
        p.u8(9);
        p.0.extend_from_slice(b"abc");
        assert!(
            Request::decode(&p.0).is_none(),
            "a game id that runs off the end"
        );
        // Create: too many settings, truncated settings, a name that runs off the end, bad UTF-8.
        let good = create().encode(1);
        let mut many = good.clone();
        many[10 + 12] = (MAX_SETTINGS + 1) as u8;
        assert!(Request::decode(&many).is_none());
        assert!(
            Request::decode(&good[..30]).is_none(),
            "cut inside the settings"
        );
        assert!(
            Request::decode(&good[..45]).is_none(),
            "a name that runs off the end"
        );
        let mut bad_utf8 = good.clone();
        bad_utf8[42] = 0xFF;
        assert!(Request::decode(&bad_utf8).is_none());
        assert!(Reply::decode(&Reply::Pong.encode(1, 1)[..10]).is_none());
        let mut rooms = Reply::Rooms {
            game: "g".into(),
            skip: 0,
            total: 1,
            rooms: vec![],
        }
        .encode(1, 1);
        *rooms.last_mut().unwrap() = 9;
        assert!(
            Reply::decode(&rooms).is_none(),
            "a count with no rooms behind it"
        );
    }

    #[test]
    fn a_dfhb_datagram_is_not_a_behb_request() {
        let mut legacy = Request::Cookie.encode(1);
        legacy[..4].copy_from_slice(b"DFHB");
        assert!(Request::decode(&legacy).is_none());
    }

    #[test]
    fn control_datagrams_round_trip() {
        let c = Control::Reload {
            game: "deadfall".into(),
        };
        assert_eq!(Control::decode(&c.encode(9)), Some((9, c)));
        let st = Control::Status {
            game: "deadfall".into(),
        };
        assert_eq!(Control::decode(&st.encode(10)), Some((10, st.clone())));
        assert_eq!(
            st.encode(10)[5],
            2,
            "status is its own kind: reload's bytes are unchanged"
        );
        let r = ControlReply {
            ok: true,
            text: "retired 2 rooms".into(),
        };
        assert_eq!(ControlReply::decode(&r.encode(9)), Some((9, r)));
        assert!(Control::decode(&Request::Cookie.encode(1)).is_none());
        assert!(
            Control::decode(b"BECT\x01\x02\0\0\0\0").is_none(),
            "no game"
        );
        assert!(
            Control::decode(b"BECT\x01\x03\0\0\0\0\x03abc").is_none(),
            "unknown kind"
        );
        assert!(
            Control::decode(b"BECT\x01\x01\0\0\0\0\x03A!B").is_none(),
            "an invalid game id"
        );
    }

    #[test]
    fn names_are_cleaned_and_bad_ones_refused() {
        assert_eq!(
            sanitize_name("  Kevin's   Room-2 ").as_deref(),
            Ok("Kevin's Room-2")
        );
        assert_eq!(sanitize_name("Zażółć łódź").as_deref(), Ok("Zażółć łódź"));
        assert_eq!(sanitize_name(""), Err(NameError::Empty));
        assert_eq!(sanitize_name("    "), Err(NameError::Empty));
        assert_eq!(sanitize_name("---"), Err(NameError::BadCharacter));
        assert_eq!(sanitize_name("a\nb"), Err(NameError::BadCharacter));
        assert_eq!(sanitize_name("tab\there"), Err(NameError::BadCharacter));
        assert_eq!(sanitize_name("bell\u{7}"), Err(NameError::BadCharacter));
        assert_eq!(
            sanitize_name("rtl\u{202e}evil"),
            Err(NameError::BadCharacter)
        );
        assert_eq!(sanitize_name("<script>"), Err(NameError::BadCharacter));
        assert_eq!(sanitize_name("%s%n"), Err(NameError::BadCharacter));
        assert_eq!(
            sanitize_name(&"a".repeat(MAX_NAME_CHARS)).map(|s| s.len()),
            Ok(MAX_NAME_CHARS)
        );
        assert_eq!(
            sanitize_name(&"a".repeat(MAX_NAME_CHARS + 1)),
            Err(NameError::TooLong)
        );
        let wide = "字".repeat(MAX_NAME_CHARS);
        assert_eq!(sanitize_name(&wide).as_deref(), Ok(wide.as_str()));
        assert!(!NameError::TooLong.to_string().is_empty());
    }
}
