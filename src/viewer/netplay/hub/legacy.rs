//! The `DFHB` version 1 protocol of the Deadfall hub, answered byte for byte as the old hub did.
//!
//! Deadfall clients already shipped speak this to `deadfall-kevin.duckdns.org:4100`. They compare the `build`
//! they are told to *their own raw* `netgame::fingerprint()` (not the combined build of [`build_id`](crate::viewer::netplay::cli::build_id)), send `Create{bots, kills, name}` with no cookie, and join
//! `hub_host:room.port` over raw UDP. A reply in any other magic or version is silently dropped by them and shows
//! "Can't reach the Deadfall servers", so the hub keeps answering exactly this and maps it to the game id
//! [`GAME`]. The layout below must never change; `tests` at the bottom check it against an independent copy of
//! the old encoder and decoder.
//!
//! ```text
//! request: "DFHB" version(1) kind(1) nonce(u32) payload...   zero-padded up to the kind's minimum length
//! reply:   "DFHB" version(1) kind(1) nonce(u32) build(u32) payload...
//!   1 List   skip(u8)                          min 200, reply <= 1000
//!   2 Create bots(u8) kills(u16) name(string)  min 96,  reply <= 128
//!   3 Ping                                     min 32,  reply <= 32
//!   0x81 Rooms skip total count room*    0x82 Created room    0x83 Error code(u8) text(string<=80)    0x84 Pong
//!   room = name(string<=96) players capacity state(0 lobby, 1 playing) port(u16) public
//! ```
//!
//! # Policy
//! `legacy = serve` (the default) answers with the Deadfall game's rooms and its raw fingerprint. `legacy =
//! refuse` still answers a well-formed v1 reply, but with a build that is deliberately wrong (the fingerprint
//! xor 1), so an old client shows its own "update the game" message instead of "can't reach": the way to retire
//! the protocol once everyone has updated. A `Create` carries no cookie, so it is weaker than a BEHB one; the hub
//! compensates with its own global create bucket for this protocol ([`Limits`](super::limits::Limits)).
use super::wire::{Cur, ErrorCode, Put, RoomInfo, MAX_REPLY};

pub const MAGIC: [u8; 4] = *b"DFHB";
pub const VERSION: u8 = 1;
/// The game id every legacy request is mapped to.
pub const GAME: &str = "deadfall";

const REQ_LIST: u8 = 1;
const REQ_CREATE: u8 = 2;
const REQ_PING: u8 = 3;
const REP_ROOMS: u8 = 0x81;
const REP_CREATED: u8 = 0x82;
const REP_ERROR: u8 = 0x83;
const REP_PONG: u8 = 0x84;
const REQUEST_HEADER: usize = 10;
pub(super) const REPLY_HEADER: usize = 14;
const MAX_ERROR_TEXT: usize = 80;

/// What to do with `DFHB` datagrams.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Answer as the old hub did.
    Serve,
    /// Answer with a well-formed reply whose build is wrong, so old clients show their update message.
    Refuse,
}

impl std::str::FromStr for Mode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "serve" => Ok(Mode::Serve),
            "refuse" => Ok(Mode::Refuse),
            _ => Err(format!("legacy must be serve or refuse, not {s:?}")),
        }
    }
}

/// A request, as an old client sends it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    List {
        skip: u8,
    },
    Create {
        name: String,
        bots: bool,
        kills: u16,
    },
    Ping,
}

/// A reply, as the old hub sent it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    Rooms {
        skip: u8,
        total: u8,
        rooms: Vec<RoomInfo>,
    },
    Created {
        room: RoomInfo,
    },
    Error {
        code: ErrorCode,
        text: String,
    },
    Pong,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyPacket {
    pub nonce: u32,
    pub build: u32,
    pub reply: Reply,
}

/// The shortest datagram answered for a request kind (clients pad up to it).
pub fn min_request_len(request: &Request) -> usize {
    match request {
        Request::Ping => 32,
        Request::Create { .. } => 96,
        Request::List { .. } => 200,
    }
}

/// The longest reply sent to a request kind.
pub fn max_reply_len(request: &Request) -> usize {
    match request {
        Request::Ping => 32,
        Request::Create { .. } => 128,
        Request::List { .. } => MAX_REPLY,
    }
}

/// The old protocol only has codes 1..=6; the newer ones read as the nearest old one.
pub fn old_code(code: ErrorCode) -> ErrorCode {
    match code {
        ErrorCode::UnknownGame | ErrorCode::Unavailable => ErrorCode::Unavailable,
        ErrorCode::BadCookie | ErrorCode::BadSetting => ErrorCode::BadRequest,
        other => other,
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
            Request::Ping => header(REQ_PING, nonce),
        };
        match self {
            Request::List { skip } => p.u8(*skip),
            Request::Create { name, bots, kills } => {
                p.u8(*bots as u8);
                p.u16(*kills);
                p.str(name, 96);
            }
            Request::Ping => {}
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
            REQ_LIST => Request::List { skip: c.u8()? },
            REQ_CREATE => Request::Create {
                bots: c.u8()? != 0,
                kills: c.u16()?,
                name: c.str()?,
            },
            REQ_PING => Request::Ping,
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
        };
        p.u32(build);
        match self {
            Reply::Rooms { skip, total, rooms } => {
                p.u8(*skip);
                p.u8(*total);
                p.u8(rooms.len().min(255) as u8);
                for r in rooms {
                    p.room_legacy(r);
                }
            }
            Reply::Created { room } => p.room_legacy(room),
            Reply::Error { code, text } => {
                p.u8(old_code(*code) as u8);
                p.str(text, MAX_ERROR_TEXT);
            }
            Reply::Pong => {}
        }
        p.0
    }

    pub fn decode(data: &[u8]) -> Option<ReplyPacket> {
        if data.len() < REPLY_HEADER {
            return None;
        }
        let (kind, nonce, mut c) = check_header(data)?;
        let build = c.u32()?;
        let reply = match kind {
            REP_ROOMS => {
                let (skip, total, n) = (c.u8()?, c.u8()?, c.u8()? as usize);
                let mut rooms = Vec::with_capacity(n.min(32));
                for _ in 0..n {
                    rooms.push(c.room_legacy()?);
                }
                Reply::Rooms { skip, total, rooms }
            }
            REP_CREATED => Reply::Created {
                room: c.room_legacy()?,
            },
            REP_ERROR => Reply::Error {
                code: ErrorCode::from_u8(c.u8()?),
                text: c.str()?,
            },
            REP_PONG => Reply::Pong,
            _ => return None,
        };
        Some(ReplyPacket {
            nonce,
            build,
            reply,
        })
    }
}

/// The settings of a legacy `Create`, looked up by name in the game's schema: `bots` (set only when asked for) and
/// `kills` (when above 0, clamped into the schema's range). A name the schema does not have is ignored.
pub fn map_create_settings(
    schema: &[crate::viewer::netplay::cli::SettingDef],
    bots: bool,
    kills: u16,
) -> Vec<(u8, u32)> {
    let mut out = Vec::new();
    if bots {
        if let Some(d) = schema.iter().find(|d| d.name == "bots") {
            out.push((d.id, 1u32.clamp(d.min, d.max)));
        }
    }
    if kills > 0 {
        if let Some(d) = schema.iter().find(|d| d.name == "kills") {
            out.push((d.id, (kills as u32).clamp(d.min, d.max)));
        }
    }
    out
}

/// The build a refusing hub reports: the raw fingerprint, deliberately wrong.
pub fn refusing_build(fingerprint: u32) -> u32 {
    fingerprint ^ 1
}

/// Rooms from number `skip` on, as many as fit one datagram (the old hub's paging).
pub fn rooms_reply(all: &[RoomInfo], skip: u8) -> Reply {
    let mut size = REPLY_HEADER + 3;
    let mut rooms = Vec::new();
    for r in all.iter().skip(skip as usize) {
        size += Reply::room_len(r);
        if size > MAX_REPLY {
            break;
        }
        rooms.push(r.clone());
    }
    Reply::Rooms {
        skip,
        total: all.len().min(255) as u8,
        rooms,
    }
}

#[cfg(test)]
mod tests {
    use super::super::wire::RoomState;
    use super::*;

    // ---- an independent copy of the encoder the SHIPPED Deadfall client uses (games/deadfall/src/hub.rs at
    // blueengine-8796c421be5f), written out by hand byte by byte, so a change to the code above that breaks old
    // clients fails here even if encode and decode still agree with each other.

    fn old_client_request(kind: u8, nonce: u32, payload: &[u8], min: usize) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"DFHB");
        v.push(1);
        v.push(kind);
        v.extend_from_slice(&nonce.to_le_bytes());
        v.extend_from_slice(payload);
        if v.len() < min {
            v.resize(min, 0);
        }
        v
    }

    fn old_ping(nonce: u32) -> Vec<u8> {
        old_client_request(3, nonce, &[], 32)
    }
    fn old_list(nonce: u32, skip: u8) -> Vec<u8> {
        old_client_request(1, nonce, &[skip], 200)
    }
    fn old_create(nonce: u32, name: &str, bots: bool, kills: u16) -> Vec<u8> {
        let mut payload = vec![bots as u8];
        payload.extend_from_slice(&kills.to_le_bytes());
        payload.push(name.len() as u8);
        payload.extend_from_slice(name.as_bytes());
        old_client_request(2, nonce, &payload, 96)
    }

    /// An independent copy of the shipped client's reply decoder.
    #[derive(Debug, PartialEq)]
    enum OldReply {
        Rooms {
            skip: u8,
            total: u8,
            rooms: Vec<(String, u8, u8, u8, u16, u8)>,
        },
        Created((String, u8, u8, u8, u16, u8)),
        Error(u8, String),
        Pong,
    }

    fn old_decode(data: &[u8]) -> Option<(u32, u32, OldReply)> {
        if data.len() < 14 || &data[..4] != b"DFHB" || data[4] != 1 {
            return None;
        }
        let kind = data[5];
        let nonce = u32::from_le_bytes(data[6..10].try_into().ok()?);
        let build = u32::from_le_bytes(data[10..14].try_into().ok()?);
        let mut at = 14;
        let take = |at: &mut usize, n: usize| -> Option<&[u8]> {
            let s = data.get(*at..*at + n)?;
            *at += n;
            Some(s)
        };
        let string = |at: &mut usize| -> Option<String> {
            let n = take(at, 1)?[0] as usize;
            String::from_utf8(take(at, n)?.to_vec()).ok()
        };
        let room = |at: &mut usize| -> Option<(String, u8, u8, u8, u16, u8)> {
            let name = string(at)?;
            let b = take(at, 6)?;
            Some((
                name,
                b[0],
                b[1],
                b[2],
                u16::from_le_bytes([b[3], b[4]]),
                b[5],
            ))
        };
        let reply = match kind {
            0x81 => {
                let h = take(&mut at, 3)?;
                let (skip, total, n) = (h[0], h[1], h[2]);
                let mut rooms = Vec::new();
                for _ in 0..n {
                    rooms.push(room(&mut at)?);
                }
                OldReply::Rooms { skip, total, rooms }
            }
            0x82 => OldReply::Created(room(&mut at)?),
            0x83 => {
                let code = take(&mut at, 1)?[0];
                OldReply::Error(code, string(&mut at)?)
            }
            0x84 => OldReply::Pong,
            _ => return None,
        };
        Some((nonce, build, reply))
    }

    fn info(name: &str, players: u8, port: u16, public: bool) -> RoomInfo {
        RoomInfo {
            name: name.into(),
            players,
            capacity: 12,
            state: if players > 5 {
                RoomState::Playing
            } else {
                RoomState::Lobby
            },
            port,
            public,
            transport: crate::viewer::net::TransportProfile::Development,
            requires_key: false,
        }
    }

    #[test]
    fn requests_from_the_old_client_decode_here() {
        assert_eq!(
            Request::decode(&old_ping(0xA1B2_C3D4)),
            Some((0xA1B2_C3D4, Request::Ping))
        );
        assert_eq!(
            Request::decode(&old_list(5, 9)),
            Some((5, Request::List { skip: 9 }))
        );
        assert_eq!(
            Request::decode(&old_create(6, "Kevin's room", true, 25)),
            Some((
                6,
                Request::Create {
                    name: "Kevin's room".into(),
                    bots: true,
                    kills: 25
                }
            ))
        );
        assert_eq!(
            Request::decode(&old_create(7, "No bots", false, 0)),
            Some((
                7,
                Request::Create {
                    name: "No bots".into(),
                    bots: false,
                    kills: 0
                }
            ))
        );
    }

    #[test]
    fn requests_encoded_here_are_byte_identical_to_the_old_client() {
        assert_eq!(Request::Ping.encode(0xA1B2_C3D4), old_ping(0xA1B2_C3D4));
        assert_eq!(Request::List { skip: 9 }.encode(5), old_list(5, 9));
        assert_eq!(
            Request::Create {
                name: "Kevin's room".into(),
                bots: true,
                kills: 25
            }
            .encode(6),
            old_create(6, "Kevin's room", true, 25)
        );
        // The size classes of the old protocol.
        assert_eq!(old_ping(1).len(), 32);
        assert_eq!(old_list(1, 0).len(), 200);
        assert_eq!(old_create(1, "x", false, 0).len(), 96);
    }

    #[test]
    fn replies_decode_with_the_old_clients_decoder_and_have_its_exact_bytes() {
        let rooms = Reply::Rooms {
            skip: 0,
            total: 2,
            rooms: vec![
                info("Public", 2, 4101, true),
                info("Kevin's room", 7, 4102, false),
            ],
        };
        let bytes = rooms.encode(0x1122_3344, 0xCAFE_F00D);
        // Spelled out: header, build, skip total count, then the rooms.
        let mut want = b"DFHB\x01\x81".to_vec();
        want.extend_from_slice(&0x1122_3344u32.to_le_bytes());
        want.extend_from_slice(&0xCAFE_F00Du32.to_le_bytes());
        want.extend_from_slice(&[0, 2, 2]);
        want.extend_from_slice(b"\x06Public\x02\x0c\x00");
        want.extend_from_slice(&4101u16.to_le_bytes());
        want.push(1);
        want.extend_from_slice(b"\x0cKevin's room\x07\x0c\x01");
        want.extend_from_slice(&4102u16.to_le_bytes());
        want.push(0);
        assert_eq!(bytes, want);
        assert_eq!(
            old_decode(&bytes),
            Some((
                0x1122_3344,
                0xCAFE_F00D,
                OldReply::Rooms {
                    skip: 0,
                    total: 2,
                    rooms: vec![
                        ("Public".into(), 2, 12, 0, 4101, 1),
                        ("Kevin's room".into(), 7, 12, 1, 4102, 0)
                    ]
                }
            ))
        );
        assert_eq!(
            old_decode(
                &Reply::Created {
                    room: info("New", 0, 4103, false)
                }
                .encode(1, 2)
            ),
            Some((1, 2, OldReply::Created(("New".into(), 0, 12, 0, 4103, 0))))
        );
        assert_eq!(
            old_decode(&Reply::Pong.encode(3, 4)),
            Some((3, 4, OldReply::Pong))
        );
        assert_eq!(Reply::Pong.encode(3, 4).len(), 14);
        assert_eq!(
            old_decode(
                &Reply::Error {
                    code: ErrorCode::NameTaken,
                    text: "taken".into()
                }
                .encode(8, 9)
            ),
            Some((8, 9, OldReply::Error(2, "taken".into())))
        );
    }

    #[test]
    fn new_error_codes_read_as_old_ones_on_the_old_wire() {
        for (new, old) in [
            (ErrorCode::UnknownGame, 5),
            (ErrorCode::BadCookie, 6),
            (ErrorCode::BadSetting, 6),
            (ErrorCode::Full, 3),
            (ErrorCode::RateLimited, 4),
            (ErrorCode::BadName, 1),
        ] {
            let bytes = Reply::Error {
                code: new,
                text: "x".into(),
            }
            .encode(1, 1);
            assert_eq!(
                old_decode(&bytes),
                Some((1, 1, OldReply::Error(old, "x".into()))),
                "{new:?}"
            );
        }
    }

    #[test]
    fn replies_stay_inside_the_old_size_classes() {
        let many: Vec<RoomInfo> = (0..40)
            .map(|i| info(&"字".repeat(24), 1, 4100 + i, false))
            .collect();
        let Reply::Rooms { rooms, total, .. } = rooms_reply(&many, 0) else {
            panic!()
        };
        assert_eq!(total, 40);
        assert!(rooms.len() < 40);
        let bytes = Reply::Rooms {
            skip: 0,
            total,
            rooms,
        }
        .encode(1, 1);
        assert!(bytes.len() <= max_reply_len(&Request::List { skip: 0 }));
        let Reply::Rooms {
            skip,
            rooms: second,
            ..
        } = rooms_reply(&many, 10)
        else {
            panic!()
        };
        assert_eq!(skip, 10);
        assert_eq!(second[0], many[10]);
    }

    #[test]
    fn garbage_decodes_to_nothing() {
        assert!(Request::decode(b"").is_none());
        let mut ok = old_list(1, 0);
        ok[4] = 2;
        assert!(Request::decode(&ok).is_none(), "another version");
        assert!(Request::decode(&old_ping(1)[..9]).is_none());
        let mut create = old_create(1, "abcdef", false, 0);
        create.truncate(REQUEST_HEADER + 6);
        assert!(
            Request::decode(&create).is_none(),
            "a name that runs off the end"
        );
        let mut bad_utf8 = old_create(1, "abcd", false, 0);
        bad_utf8[REQUEST_HEADER + 4] = 0xFF;
        assert!(Request::decode(&bad_utf8).is_none());
        let mut new_magic = old_ping(1);
        new_magic[..4].copy_from_slice(b"BEHB");
        assert!(Request::decode(&new_magic).is_none());
    }

    #[test]
    fn creates_map_onto_the_schema_by_name() {
        use crate::viewer::netplay::cli::SettingDef;
        let schema = vec![
            SettingDef::parse_field("1:bots:bots:bool:0:1:0").unwrap(),
            SettingDef::parse_field("2:kills:kills:int:1:500:40").unwrap(),
            SettingDef::parse_field("3:skill:skill:int:0:2:1").unwrap(),
        ];
        assert_eq!(map_create_settings(&schema, false, 0), vec![]);
        assert_eq!(map_create_settings(&schema, true, 0), vec![(1, 1)]);
        assert_eq!(map_create_settings(&schema, false, 25), vec![(2, 25)]);
        assert_eq!(
            map_create_settings(&schema, true, 9999),
            vec![(1, 1), (2, 500)]
        );
        // A game without those settings simply ignores them.
        assert_eq!(map_create_settings(&schema[2..], true, 25), vec![]);
        assert_eq!(map_create_settings(&[], true, 25), vec![]);
    }

    #[test]
    fn mode_parses() {
        assert_eq!("serve".parse(), Ok(Mode::Serve));
        assert_eq!("refuse".parse(), Ok(Mode::Refuse));
        assert!("maybe".parse::<Mode>().is_err());
        assert_eq!(refusing_build(0x1234), 0x1235);
        assert_eq!(refusing_build(refusing_build(7)), 7);
    }
}
