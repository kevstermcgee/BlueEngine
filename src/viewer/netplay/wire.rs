//! The netplay envelope: fixed messages around game-defined payloads. Every read is bounds-checked
//! ([`codec`](crate::viewer::net::codec)); a datagram that does not parse is dropped by whoever received it.
use super::NetGame;
use crate::viewer::net::codec::{Reader, WireError, WireResult, Writer};

pub const MAGIC: [u8; 2] = *b"NP";
/// Bump when the envelope layout changes; peers with another version are refused.
pub const PROTOCOL: u8 = 1;
/// The engine-wide ceiling; an active transport may impose a smaller per-peer budget.
pub const MAX_DATAGRAM: usize = crate::viewer::net::MAX_PACKET_BYTES;
/// Longest text field (join key, name).
pub const MAX_TEXT: usize = 32;
/// Inputs bundled into one datagram, newest last: redundancy that makes a lost datagram cost nothing.
pub const INPUT_BUNDLE: usize = 4;
/// Events one snapshot may carry.
pub const MAX_EVENTS: usize = 24;
/// A session token.
pub type Token = [u64; 2];

/// What a client says.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientMsg<I> {
    Hello {
        key: String,
        name: String,
        choice: u8,
        nonce: [u64; 2],
        fingerprint: u32,
    },
    Select {
        token: Token,
        choice: u8,
    },
    Ready {
        token: Token,
        ready: bool,
    },
    /// Newest inputs (oldest first) and the newest server tick this client has seen.
    Input {
        token: Token,
        frames: Vec<(u32, I)>,
        ack_tick: u32,
    },
    /// `rtt_ms` is the client's own latest round-trip measurement, for the server's statistics.
    Ping {
        token: Token,
        stamp: u32,
        rtt_ms: u16,
    },
    Leave {
        token: Token,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct LobbyEntry {
    pub slot: u8,
    pub choice: u8,
    pub ready: bool,
    pub name: String,
}

/// Who is in the lobby and when the match starts.
#[derive(Clone, Debug, PartialEq)]
pub struct LobbyState {
    /// 0 lobby, 1 in a match, 2 results.
    pub stage: u8,
    /// Seconds until the match starts, or 0 while waiting.
    pub seconds_left: u8,
    /// Participants in the next match (players plus the game's own AI).
    pub participants: u8,
    pub entries: Vec<LobbyEntry>,
}

/// One snapshot with what the server knows about its recipient.
#[derive(Clone, Debug, PartialEq)]
pub struct SnapshotMsg<S, E> {
    pub server_tick: u32,
    /// The newest input of yours the server has applied.
    pub applied_seq: u32,
    /// The participant you drive, or 255 if you are only watching.
    pub you: u8,
    pub snapshot: S,
    /// Events newer than the tick you last acknowledged, oldest first.
    pub events: Vec<(u32, E)>,
}

/// What the server says.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerMsg<S, E> {
    Welcome {
        token: Token,
        slot: u8,
        fingerprint: u32,
    },
    Rejected {
        reason: String,
    },
    Lobby(LobbyState),
    Snapshot(SnapshotMsg<S, E>),
    Pong {
        stamp: u32,
    },
}

fn open(bytes: &[u8]) -> WireResult<(u8, Reader<'_>)> {
    if bytes.len() < 4 || bytes.len() > MAX_DATAGRAM {
        return Err(WireError("wrong size"));
    }
    if bytes[..2] != MAGIC {
        return Err(WireError("not a netplay datagram"));
    }
    if bytes[2] != PROTOCOL {
        return Err(WireError("protocol version mismatch"));
    }
    let mut r = Reader::new(bytes);
    r.take(4)?;
    Ok((bytes[3], r))
}

fn start(kind: u8) -> Writer {
    let mut w = Writer::new();
    w.raw(&MAGIC);
    w.u8(PROTOCOL);
    w.u8(kind);
    w
}

pub fn encode_client<G: NetGame>(msg: &ClientMsg<G::Input>) -> Vec<u8> {
    match msg {
        ClientMsg::Hello {
            key,
            name,
            choice,
            nonce,
            fingerprint,
        } => {
            let mut w = start(1);
            w.text(key, MAX_TEXT);
            w.text(name, MAX_TEXT);
            w.u8(*choice);
            w.token(*nonce);
            w.u32(*fingerprint);
            w.finish()
        }
        ClientMsg::Select { token, choice } => {
            let mut w = start(2);
            w.token(*token);
            w.u8(*choice);
            w.finish()
        }
        ClientMsg::Ready { token, ready } => {
            let mut w = start(3);
            w.token(*token);
            w.bool(*ready);
            w.finish()
        }
        ClientMsg::Input {
            token,
            frames,
            ack_tick,
        } => {
            let mut w = start(4);
            w.token(*token);
            w.u32(*ack_tick);
            let n = frames.len().min(INPUT_BUNDLE);
            w.u8(n as u8);
            for (seq, input) in &frames[frames.len() - n..] {
                w.u32(*seq);
                G::write_input(input, &mut w);
            }
            w.finish()
        }
        ClientMsg::Ping {
            token,
            stamp,
            rtt_ms,
        } => {
            let mut w = start(5);
            w.token(*token);
            w.u32(*stamp);
            w.u16(*rtt_ms);
            w.finish()
        }
        ClientMsg::Leave { token } => {
            let mut w = start(6);
            w.token(*token);
            w.finish()
        }
    }
}

pub fn decode_client<G: NetGame>(bytes: &[u8]) -> WireResult<ClientMsg<G::Input>> {
    let (kind, mut r) = open(bytes)?;
    let msg = match kind {
        1 => ClientMsg::Hello {
            key: r.text(MAX_TEXT)?,
            name: r.text(MAX_TEXT)?,
            choice: r.u8()?,
            nonce: r.token()?,
            fingerprint: r.u32()?,
        },
        2 => ClientMsg::Select {
            token: r.token()?,
            choice: r.u8()?,
        },
        3 => ClientMsg::Ready {
            token: r.token()?,
            ready: r.bool()?,
        },
        4 => {
            let token = r.token()?;
            let ack_tick = r.u32()?;
            let n = r.u8()? as usize;
            if n > INPUT_BUNDLE {
                return Err(WireError("too many inputs"));
            }
            let mut frames = Vec::with_capacity(n);
            for _ in 0..n {
                let seq = r.u32()?;
                frames.push((seq, G::read_input(&mut r)?));
            }
            ClientMsg::Input {
                token,
                frames,
                ack_tick,
            }
        }
        5 => ClientMsg::Ping {
            token: r.token()?,
            stamp: r.u32()?,
            rtt_ms: r.u16()?,
        },
        6 => ClientMsg::Leave { token: r.token()? },
        _ => return Err(WireError("unknown client message")),
    };
    r.done()?;
    Ok(msg)
}

/// Encode a server message. A snapshot that would not fit a datagram sheds its oldest events first; if the
/// snapshot alone is too big the result is longer than [`MAX_DATAGRAM`] and the sender must not send it.
pub fn encode_server<G: NetGame>(msg: &ServerMsg<G::Snapshot, G::Event>) -> Vec<u8> {
    encode_server_with_limit::<G>(msg, MAX_DATAGRAM)
}

pub(crate) fn encode_server_with_limit<G: NetGame>(
    msg: &ServerMsg<G::Snapshot, G::Event>,
    limit: usize,
) -> Vec<u8> {
    match msg {
        ServerMsg::Welcome {
            token,
            slot,
            fingerprint,
        } => {
            let mut w = start(101);
            w.token(*token);
            w.u8(*slot);
            w.u32(*fingerprint);
            w.finish()
        }
        ServerMsg::Rejected { reason } => {
            let mut w = start(102);
            w.text(reason, MAX_TEXT);
            w.finish()
        }
        ServerMsg::Lobby(l) => {
            let mut w = start(103);
            w.u8(l.stage);
            w.u8(l.seconds_left);
            w.u8(l.participants);
            w.u8(l.entries.len().min(16) as u8);
            for e in l.entries.iter().take(16) {
                w.u8(e.slot);
                w.u8(e.choice);
                w.bool(e.ready);
                w.text(&e.name, MAX_TEXT);
            }
            w.finish()
        }
        ServerMsg::Snapshot(s) => {
            let mut w = start(104);
            w.u32(s.server_tick);
            w.u32(s.applied_seq);
            w.u8(s.you);
            let mut body = Writer::new();
            G::write_snapshot(&s.snapshot, &mut body);
            w.u16(body.len() as u16);
            w.raw(body.as_slice());
            // Serialize each payload once, even when events must be fitted to the budget.
            let first = s.events.len().saturating_sub(MAX_EVENTS);
            let encoded: Vec<Vec<u8>> = s.events[first..]
                .iter()
                .map(|(tick, e)| {
                    let mut event = Writer::new();
                    event.u32(*tick);
                    G::write_event(e, &mut event);
                    event.finish()
                })
                .collect();
            let mut size = w.len() + 1 + encoded.iter().map(Vec::len).sum::<usize>();
            let mut first = 0;
            while size > limit.min(MAX_DATAGRAM) && first < encoded.len() {
                size -= encoded[first].len();
                first += 1;
            }
            w.u8((encoded.len() - first) as u8);
            for event in &encoded[first..] {
                w.raw(event);
            }
            w.finish()
        }
        ServerMsg::Pong { stamp } => {
            let mut w = start(105);
            w.u32(*stamp);
            w.finish()
        }
    }
}

pub fn decode_server<G: NetGame>(bytes: &[u8]) -> WireResult<ServerMsg<G::Snapshot, G::Event>> {
    let (kind, mut r) = open(bytes)?;
    let msg = match kind {
        101 => ServerMsg::Welcome {
            token: r.token()?,
            slot: r.u8()?,
            fingerprint: r.u32()?,
        },
        102 => ServerMsg::Rejected {
            reason: r.text(MAX_TEXT)?,
        },
        103 => {
            let (stage, seconds_left, participants) = (r.u8()?, r.u8()?, r.u8()?);
            let n = r.u8()? as usize;
            if n > 16 || stage > 2 {
                return Err(WireError("bad lobby"));
            }
            let mut entries = Vec::with_capacity(n);
            for _ in 0..n {
                entries.push(LobbyEntry {
                    slot: r.u8()?,
                    choice: r.u8()?,
                    ready: r.bool()?,
                    name: r.text(MAX_TEXT)?,
                });
            }
            ServerMsg::Lobby(LobbyState {
                stage,
                seconds_left,
                participants,
                entries,
            })
        }
        104 => {
            let server_tick = r.u32()?;
            let applied_seq = r.u32()?;
            let you = r.u8()?;
            let len = r.u16()? as usize;
            let body = r.take(len)?;
            let mut body_reader = Reader::new(body);
            let snapshot = G::read_snapshot(&mut body_reader)?;
            body_reader.done()?;
            let n = r.u8()? as usize;
            if n > MAX_EVENTS {
                return Err(WireError("too many events"));
            }
            let mut events = Vec::with_capacity(n);
            for _ in 0..n {
                let tick = r.u32()?;
                events.push((tick, G::read_event(&mut r)?));
            }
            ServerMsg::Snapshot(SnapshotMsg {
                server_tick,
                applied_seq,
                you,
                snapshot,
                events,
            })
        }
        105 => ServerMsg::Pong { stamp: r.u32()? },
        _ => return Err(WireError("unknown server message")),
    };
    r.done()?;
    Ok(msg)
}
