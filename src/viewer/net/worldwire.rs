//! The compact binary form of world updates (`Packet::Snapshot` and `Packet::Delta`), protocol 8.
//!
//! These two packets are almost all of the server's traffic, and as JSON a player record is about 172 bytes, so a
//! 1,100-byte packet carries six of them. Here a walking player is 19 to 28 bytes (a prop 39 to 51) and a packet carries
//! about 45 to 50 players. Every other packet stays JSON; a leading [`MARK`] byte tells them apart (JSON always starts with `{` or `"`).
//!
//! **Exactness.** Floats are written as their 32-bit patterns and decode to bit-identical values, because the server
//! decides what to resend by comparing exact states; a lossy encoding would make a record look changed forever. The only
//! omissions are values that are exactly `+0.0` (flagged) and fields that are absent.
//!
//! **Layout** (all integers are [`Writer::varint`] unless noted; ticks inside records are a zigzag offset from the
//! packet's own tick, so they are one or two bytes):
//!
//! ```text
//! snapshot: MARK 1  session?  tick  ack  players  props
//! delta:    MARK 2  session?  base_tick  target_tick  ack  changed_players  changed_props  removed_players  removed_props
//! session?  0            or   1 + 16 bytes
//! list      count, then that many records
//! player    id  tick_offset  flags  x y z yaw (f32 each)  [pitch]  [vertical_vel]  [room_id]
//!           flags: 1 grounded, 2 crouched, 4 Feta, 8 has room, 16 pitch != 0, 32 vertical_vel != 0; 64 and 128 must be 0
//! prop      id(len + utf-8)  flags  x y z  rot(4)  [linear(3)]  [angular(3)]  [held_by]  generation
//!           flags: 1 sleeping, 2 held, 4 linear != 0, 8 angular != 0; 16..128 must be 0
//! removed   player id, or prop id as text
//! ```
//!
//! **Hostile input.** Decoding never trusts the sender: every read is bounds-checked, floats must be finite, list
//! counts are capped by [`MAX_REPLICATED_ENTITIES`] and by what the datagram could physically hold, reserved bits
//! must be zero, and trailing bytes are refused. A malformed update is an error, never a panic or an allocation bomb.
use super::{
    codec::{Reader, WireError, WireResult, Writer},
    replication::MAX_REPLICATED_ENTITIES,
    DeltaSnapshot, Packet, PlayerNetState, PropNetState, WorldSnapshot, MAX_PACKET_BYTES,
};
use crate::{
    math::V,
    viewer::{controller::CharacterKind, lifecycle::Generation, spatial::RoomId},
};

/// First byte of a binary world update. No JSON document starts with it.
pub const MARK: u8 = 0xB1;
const SNAPSHOT: u8 = 1;
const DELTA: u8 = 2;
/// Longest prop id the wire carries; replication refuses longer ones earlier with a clear message.
pub const MAX_ID_BYTES: usize = 255;

/// Where encoded bytes go: a real buffer, or a counter that only measures.
pub trait Out {
    fn u8(&mut self, v: u8);
    fn raw(&mut self, bytes: &[u8]);
    fn f32(&mut self, v: f32) {
        self.raw(&v.to_le_bytes());
    }
    fn varint(&mut self, mut v: u64) {
        while v >= 0x80 {
            self.u8((v as u8 & 0x7F) | 0x80);
            v >>= 7;
        }
        self.u8(v as u8);
    }
    fn signed(&mut self, v: i64) {
        self.varint(((v << 1) ^ (v >> 63)) as u64);
    }
    fn text(&mut self, s: &str) {
        self.varint(s.len() as u64);
        self.raw(s.as_bytes());
    }
}

impl Out for Writer {
    fn u8(&mut self, v: u8) {
        Writer::u8(self, v);
    }
    fn raw(&mut self, bytes: &[u8]) {
        Writer::raw(self, bytes);
    }
}

/// Counts instead of storing, so sizes can be measured without allocating.
#[derive(Default)]
pub struct Count(pub usize);
impl Out for Count {
    fn u8(&mut self, _: u8) {
        self.0 += 1;
    }
    fn raw(&mut self, bytes: &[u8]) {
        self.0 += bytes.len();
    }
}

fn is_zero(v: f32) -> bool {
    v.to_bits() == 0
}
fn vec3<O: Out>(o: &mut O, v: V) {
    o.f32(v.0);
    o.f32(v.1);
    o.f32(v.2);
}
fn is_zero_vec(v: V) -> bool {
    is_zero(v.0) && is_zero(v.1) && is_zero(v.2)
}
/// Offset of a record's tick from the packet's, wrapping so every `u64` pair round-trips.
fn tick_offset(packet_tick: u64, record_tick: u64) -> i64 {
    packet_tick.wrapping_sub(record_tick) as i64
}

pub fn player<O: Out>(o: &mut O, p: &PlayerNetState, packet_tick: u64) {
    o.varint(p.id);
    o.signed(tick_offset(packet_tick, p.tick));
    let flags = u8::from(p.grounded)
        | u8::from(p.crouched) << 1
        | u8::from(p.character_kind == CharacterKind::Feta) << 2
        | u8::from(p.room_id.is_some()) << 3
        | u8::from(!is_zero(p.pitch)) << 4
        | u8::from(!is_zero(p.vertical_vel)) << 5;
    o.u8(flags);
    vec3(o, p.position);
    o.f32(p.yaw);
    if !is_zero(p.pitch) {
        o.f32(p.pitch);
    }
    if !is_zero(p.vertical_vel) {
        o.f32(p.vertical_vel);
    }
    if let Some(room) = p.room_id {
        o.varint(u64::from(room.0));
    }
}

pub fn prop<O: Out>(o: &mut O, p: &PropNetState) {
    o.text(&p.id);
    let flags = u8::from(p.sleeping)
        | u8::from(p.held_by.is_some()) << 1
        | u8::from(!is_zero_vec(p.linear_velocity)) << 2
        | u8::from(!is_zero_vec(p.angular_velocity)) << 3;
    o.u8(flags);
    vec3(o, p.position);
    p.rotation.iter().for_each(|q| o.f32(*q));
    if !is_zero_vec(p.linear_velocity) {
        vec3(o, p.linear_velocity);
    }
    if !is_zero_vec(p.angular_velocity) {
        vec3(o, p.angular_velocity);
    }
    if let Some(holder) = p.held_by {
        o.varint(holder);
    }
    o.varint(p.generation.0);
}

fn session<O: Out>(o: &mut O, s: Option<[u64; 2]>) {
    match s {
        Some(t) => {
            o.u8(1);
            o.raw(&t[0].to_le_bytes());
            o.raw(&t[1].to_le_bytes());
        }
        None => o.u8(0),
    }
}

pub fn snapshot_into<O: Out>(o: &mut O, s: &WorldSnapshot) {
    o.u8(MARK);
    o.u8(SNAPSHOT);
    session(o, s.session);
    o.varint(s.tick);
    o.varint(s.ack_client_tick);
    o.varint(s.players.len() as u64);
    s.players.iter().for_each(|p| player(o, p, s.tick));
    o.varint(s.props.len() as u64);
    s.props.iter().for_each(|p| prop(o, p));
}

pub fn delta_into<O: Out>(o: &mut O, d: &DeltaSnapshot) {
    o.u8(MARK);
    o.u8(DELTA);
    session(o, d.session);
    o.varint(d.base_tick);
    o.varint(d.target_tick);
    o.varint(d.ack_client_tick);
    o.varint(d.changed_players.len() as u64);
    d.changed_players
        .iter()
        .for_each(|p| player(o, p, d.target_tick));
    o.varint(d.changed_props.len() as u64);
    d.changed_props.iter().for_each(|p| prop(o, p));
    o.varint(d.removed_players.len() as u64);
    d.removed_players.iter().for_each(|id| o.varint(*id));
    o.varint(d.removed_props.len() as u64);
    d.removed_props.iter().for_each(|id| o.text(id));
}

pub fn encode_snapshot(s: &WorldSnapshot) -> Vec<u8> {
    let mut w = Writer::new();
    snapshot_into(&mut w, s);
    w.finish()
}
pub fn encode_delta(d: &DeltaSnapshot) -> Vec<u8> {
    let mut w = Writer::new();
    delta_into(&mut w, d);
    w.finish()
}
pub fn snapshot_len(s: &WorldSnapshot) -> usize {
    let mut c = Count::default();
    snapshot_into(&mut c, s);
    c.0
}
pub fn delta_len(d: &DeltaSnapshot) -> usize {
    let mut c = Count::default();
    delta_into(&mut c, d);
    c.0
}
pub fn player_len(p: &PlayerNetState, packet_tick: u64) -> usize {
    let mut c = Count::default();
    player(&mut c, p, packet_tick);
    c.0
}
pub fn prop_len(p: &PropNetState) -> usize {
    let mut c = Count::default();
    prop(&mut c, p);
    c.0
}
pub fn text_len(id: &str) -> usize {
    let mut c = Count::default();
    c.text(id);
    c.0
}
pub fn varint_len(v: u64) -> usize {
    let mut c = Count::default();
    c.varint(v);
    c.0
}

// ---------------------------------------------------------------- decoding

fn read_vec3(r: &mut Reader) -> WireResult<V> {
    Ok(V(r.f32()?, r.f32()?, r.f32()?))
}

/// The most records a list could hold in what is left of the datagram, given the smallest a record can be.
fn list_len(r: &mut Reader, min_record: usize) -> WireResult<usize> {
    let n = r.varint_max(MAX_REPLICATED_ENTITIES as u64)? as usize;
    if n.saturating_mul(min_record) > r.remaining() {
        return Err(WireError("list longer than the datagram"));
    }
    Ok(n)
}

fn read_player(r: &mut Reader, packet_tick: u64) -> WireResult<PlayerNetState> {
    let id = r.varint()?;
    let tick = packet_tick.wrapping_sub(r.signed()? as u64);
    let flags = r.u8()?;
    if flags & 0xC0 != 0 {
        return Err(WireError("reserved player flag set"));
    }
    let position = read_vec3(r)?;
    let yaw = r.f32()?;
    let pitch = if flags & 16 != 0 { r.f32()? } else { 0.0 };
    let vertical_vel = if flags & 32 != 0 { r.f32()? } else { 0.0 };
    let room_id = if flags & 8 != 0 {
        let room = r.varint_max(u64::from(u32::MAX))?;
        Some(RoomId(room as u32))
    } else {
        None
    };
    Ok(PlayerNetState {
        id,
        tick,
        position,
        yaw,
        pitch,
        vertical_vel,
        grounded: flags & 1 != 0,
        crouched: flags & 2 != 0,
        character_kind: if flags & 4 != 0 {
            CharacterKind::Feta
        } else {
            CharacterKind::Scientist
        },
        room_id,
    })
}

fn read_text(r: &mut Reader) -> WireResult<String> {
    let n = r.varint_max(MAX_ID_BYTES as u64)? as usize;
    String::from_utf8(r.take(n)?.to_vec()).map_err(|_| WireError("text is not utf-8"))
}

fn read_prop(r: &mut Reader) -> WireResult<PropNetState> {
    let id = read_text(r)?;
    let flags = r.u8()?;
    if flags & 0xF0 != 0 {
        return Err(WireError("reserved prop flag set"));
    }
    let position = read_vec3(r)?;
    let rotation = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
    let linear_velocity = if flags & 4 != 0 {
        read_vec3(r)?
    } else {
        V(0., 0., 0.)
    };
    let angular_velocity = if flags & 8 != 0 {
        read_vec3(r)?
    } else {
        V(0., 0., 0.)
    };
    let held_by = if flags & 2 != 0 {
        Some(r.varint()?)
    } else {
        None
    };
    let generation = Generation(r.varint()?);
    Ok(PropNetState {
        id,
        position,
        rotation,
        linear_velocity,
        angular_velocity,
        sleeping: flags & 1 != 0,
        held_by,
        generation,
    })
}

fn read_session(r: &mut Reader) -> WireResult<Option<[u64; 2]>> {
    match r.u8()? {
        0 => Ok(None),
        1 => Ok(Some(r.token()?)),
        _ => Err(WireError("bad session flag")),
    }
}

/// Smallest possible encoded sizes, used to bound list counts before anything is allocated.
const MIN_PLAYER: usize = 1 + 1 + 1 + 16;
const MIN_PROP: usize = 1 + 1 + 28;

/// Decode a datagram that starts with [`MARK`]. Anything else is the caller's (JSON) business.
pub fn decode(bytes: &[u8]) -> WireResult<Packet> {
    if bytes.len() > MAX_PACKET_BYTES {
        return Err(WireError("datagram too large"));
    }
    let mut r = Reader::new(bytes);
    if r.u8()? != MARK {
        return Err(WireError("not a binary world update"));
    }
    let packet = match r.u8()? {
        SNAPSHOT => {
            let session = read_session(&mut r)?;
            let tick = r.varint()?;
            let ack_client_tick = r.varint()?;
            let n = list_len(&mut r, MIN_PLAYER)?;
            let players = (0..n)
                .map(|_| read_player(&mut r, tick))
                .collect::<WireResult<Vec<_>>>()?;
            let n = list_len(&mut r, MIN_PROP)?;
            let props = (0..n)
                .map(|_| read_prop(&mut r))
                .collect::<WireResult<Vec<_>>>()?;
            Packet::Snapshot(WorldSnapshot {
                session,
                tick,
                ack_client_tick,
                players,
                props,
            })
        }
        DELTA => {
            let session = read_session(&mut r)?;
            let base_tick = r.varint()?;
            let target_tick = r.varint()?;
            let ack_client_tick = r.varint()?;
            let n = list_len(&mut r, MIN_PLAYER)?;
            let changed_players = (0..n)
                .map(|_| read_player(&mut r, target_tick))
                .collect::<WireResult<Vec<_>>>()?;
            let n = list_len(&mut r, MIN_PROP)?;
            let changed_props = (0..n)
                .map(|_| read_prop(&mut r))
                .collect::<WireResult<Vec<_>>>()?;
            let n = list_len(&mut r, 1)?;
            let removed_players = (0..n).map(|_| r.varint()).collect::<WireResult<Vec<_>>>()?;
            let n = list_len(&mut r, 1)?;
            let removed_props = (0..n)
                .map(|_| read_text(&mut r))
                .collect::<WireResult<Vec<_>>>()?;
            Packet::Delta(DeltaSnapshot {
                session,
                base_tick,
                target_tick,
                ack_client_tick,
                changed_players,
                changed_props,
                removed_players,
                removed_props,
            })
        }
        _ => return Err(WireError("unknown world update kind")),
    };
    r.done()?;
    Ok(packet)
}
