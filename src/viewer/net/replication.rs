//! Packet-budgeted, acknowledged partial-world replication (protocol 7).
//!
//! One immutable update is in flight per peer. Only its exact acknowledgement
//! advances the baseline. Retry is bounded to one world packet per scheduling
//! opportunity; queued bytes are not evidence of delivery. A rotating dirty-key
//! cursor reserves progress for cold entities before prioritizing the owner.
use super::{
    DatagramTransport, DeltaSnapshot, Packet, PlayerNetState, PropNetState, SendOutcome,
    WorldSnapshot, MAX_PACKET_BYTES,
};
use std::{collections::BTreeMap, net::SocketAddr};

pub const MAX_REPLICATED_ENTITIES: usize = 1024;
pub const MAX_ENTITY_ID_BYTES: usize = 128;

#[derive(Clone, Debug, Default)]
pub struct ReplicationCounters {
    pub accepted_packets: u64,
    pub accepted_bytes: u64,
    pub backpressured: u64,
    pub send_errors: u64,
    pub retries: u64,
    pub acknowledged: u64,
    pub ignored_acks: u64,
    pub resyncs: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Player(u64),
    Prop(String),
}
#[derive(Clone, Debug)]
enum Record {
    Player(PlayerNetState),
    Prop(PropNetState),
    RemovePlayer(u64),
    RemoveProp(String),
}
impl Record {
    fn key(&self) -> Key {
        match self {
            Self::Player(p) => Key::Player(p.id),
            Self::Prop(p) => Key::Prop(p.id.clone()),
            Self::RemovePlayer(id) => Key::Player(*id),
            Self::RemoveProp(id) => Key::Prop(id.clone()),
        }
    }
    fn add(&self, delta: &mut DeltaSnapshot) {
        match self {
            Self::Player(p) => delta.changed_players.push(p.clone()),
            Self::Prop(p) => delta.changed_props.push(p.clone()),
            Self::RemovePlayer(id) => delta.removed_players.push(*id),
            Self::RemoveProp(id) => delta.removed_props.push(id.clone()),
        }
    }
}
#[derive(Clone, Debug)]
struct Pending {
    packet: Packet,
    bytes: Vec<u8>,
    accepted: bool,
    attempts: u64,
}
/// Bounded per-peer state: one acknowledged world and one packet, never a queue of worlds.
#[derive(Clone, Debug, Default)]
pub struct ReplicationSender {
    session: Option<super::SessionToken>,
    baseline: Option<WorldSnapshot>,
    pending: Option<Pending>,
    cursor: Option<Key>,
    issued: u64,
    pub counters: ReplicationCounters,
}
impl ReplicationSender {
    pub fn for_session(session: super::SessionToken) -> Self {
        Self {
            session: Some(session),
            ..Self::default()
        }
    }
    pub fn baseline(&self) -> Option<&WorldSnapshot> {
        self.baseline.as_ref()
    }
    pub fn pending_bytes(&self) -> usize {
        self.pending.as_ref().map_or(0, |p| p.bytes.len())
    }
    /// Serialized retained payload, useful for capacity measurements (not allocator overhead).
    pub fn retained_payload_bytes(&self) -> usize {
        self.pending_bytes()
            + self
                .baseline
                .as_ref()
                .map_or(0, |b| serde_json::to_vec(b).map_or(0, |v| v.len()))
    }
    pub fn acknowledge(&mut self, tick: u64) -> bool {
        let Some(p) = &self.pending else {
            return false;
        };
        let target = match &p.packet {
            Packet::Snapshot(s) => s.tick,
            Packet::Delta(d) => d.target_tick,
            _ => unreachable!(),
        };
        if tick != target || !p.accepted {
            if tick != 0 {
                self.counters.ignored_acks += 1;
            }
            return false;
        }
        let p = self.pending.take().unwrap();
        self.baseline = Some(match p.packet {
            Packet::Snapshot(s) => s,
            Packet::Delta(d) => d.apply_to(self.baseline.as_ref().unwrap()),
            _ => unreachable!(),
        });
        self.counters.acknowledged += 1;
        true
    }
    /// A receiver that lost its baseline requests a new partial keyframe. Sequence
    /// numbers remain monotonic, so old packets/acks cannot undo the reset.
    pub fn resynchronize(&mut self) {
        self.baseline = None;
        self.pending = None;
        self.cursor = None;
        self.counters.resyncs += 1;
    }
    /// Coalesce duplicate requests only when an independently applicable newer
    /// keyframe is already pending. A round reset may invalidate an older one.
    pub fn resynchronize_after(&mut self, after_tick: u64) {
        // An acknowledgement newer than the request proves that the receiver
        // has already recovered. Reordered/duplicated requests cannot erase it.
        if self.baseline.as_ref().is_some_and(|s| s.tick > after_tick) {
            return;
        }
        if self.baseline.is_none()
            && self
                .pending
                .as_ref()
                .is_some_and(|p| matches!(&p.packet, Packet::Snapshot(s) if s.tick > after_tick))
        {
            return;
        }
        self.resynchronize();
    }
    pub fn send<T: DatagramTransport>(
        &mut self,
        transport: &T,
        peer: SocketAddr,
        desired: &WorldSnapshot,
        owner: u64,
    ) -> crate::Result<()> {
        let limit = transport.payload_limit(peer).min(MAX_PACKET_BYTES);
        validate_world(desired, limit)?;
        // A path MTU reduction invalidates the pending packet, not the acknowledged baseline.
        if self.pending.as_ref().is_some_and(|p| p.bytes.len() > limit) {
            self.pending = None;
        }
        if self.pending.is_none() {
            self.prepare(desired, owner, limit)?;
        }
        let pending = self.pending.as_mut().unwrap();
        if pending.attempts > 0 {
            self.counters.retries += 1;
        }
        pending.attempts += 1;
        match transport.try_send(peer, &pending.bytes) {
            Ok(SendOutcome::Accepted { bytes }) => {
                pending.accepted = true;
                self.counters.accepted_packets += 1;
                self.counters.accepted_bytes += bytes as u64;
            }
            Ok(SendOutcome::Backpressured) => self.counters.backpressured += 1,
            Err(e) => {
                self.counters.send_errors += 1;
                return Err(e);
            }
        }
        Ok(())
    }
    fn prepare(&mut self, desired: &WorldSnapshot, owner: u64, limit: usize) -> crate::Result<()> {
        let empty = WorldSnapshot::default();
        let base = self.baseline.as_ref().unwrap_or(&empty);
        let old_players: BTreeMap<_, _> = base.players.iter().map(|p| (p.id, p)).collect();
        let old_props: BTreeMap<_, _> = base.props.iter().map(|p| (&p.id, p)).collect();
        let players: BTreeMap<_, _> = desired.players.iter().map(|p| (p.id, p)).collect();
        let props: BTreeMap<_, _> = desired.props.iter().map(|p| (&p.id, p)).collect();
        let mut dirty = BTreeMap::new();
        for p in &desired.players {
            if old_players.get(&p.id).copied() != Some(p) {
                let r = Record::Player(p.clone());
                dirty.insert(r.key(), r);
            }
        }
        for p in &desired.props {
            if old_props.get(&p.id).copied() != Some(p) {
                let r = Record::Prop(p.clone());
                dirty.insert(r.key(), r);
            }
        }
        for p in &base.players {
            if !players.contains_key(&p.id) {
                let r = Record::RemovePlayer(p.id);
                dirty.insert(r.key(), r);
            }
        }
        for p in &base.props {
            if !props.contains_key(&p.id) {
                let r = Record::RemoveProp(p.id.clone());
                dirty.insert(r.key(), r);
            }
        }
        let mut keys: Vec<_> = dirty.keys().cloned().collect();
        if let Some(cursor) = &self.cursor {
            let offset = keys.partition_point(|k| k <= cursor);
            keys.rotate_left(offset);
        }
        // One rotating record is reserved. At very small payload limits the
        // owner and fair record may not fit together: alternate who goes first.
        let fair = keys.first().cloned();
        let owner_key = Key::Player(owner);
        let mut order = Vec::with_capacity(keys.len());
        if self.counters.acknowledged % 2 == 1 && dirty.contains_key(&owner_key) {
            order.push(owner_key.clone());
        }
        if let Some(key) = &fair {
            if !order.contains(key) {
                order.push(key.clone());
            }
        }
        if dirty.contains_key(&owner_key) && !order.contains(&owner_key) {
            order.push(owner_key);
        }
        // Removals and held objects are urgent, without taking the fair slot.
        for key in &keys {
            if (matches!(&dirty[key], Record::RemovePlayer(_) | Record::RemoveProp(_))
                || matches!(&dirty[key], Record::Prop(p) if p.held_by.is_some()))
                && !order.contains(key)
            {
                order.push(key.clone());
            }
        }
        order.extend(keys);
        let mut scheduled = std::collections::BTreeSet::new();
        order.retain(|key| scheduled.insert(key.clone()));
        let tick = desired.tick.max(
            self.issued
                .checked_add(1)
                .ok_or("Replication sequence exhausted; reconnect server")?,
        );
        let mut delta = DeltaSnapshot {
            session: self.session,
            base_tick: base.tick,
            target_tick: tick,
            ack_client_tick: base.ack_client_tick,
            changed_players: vec![],
            changed_props: vec![],
            removed_players: vec![],
            removed_props: vec![],
        };
        for key in order {
            let record = &dirty[&key];
            if matches!(record, Record::RemovePlayer(_) | Record::RemoveProp(_)) {
                let mut alone = DeltaSnapshot {
                    session: self.session,
                    base_tick: u64::MAX,
                    target_tick: u64::MAX,
                    ack_client_tick: u64::MAX,
                    changed_players: vec![],
                    changed_props: vec![],
                    removed_players: vec![],
                    removed_props: vec![],
                };
                record.add(&mut alone);
                if encode_update(&alone, false)?.1.len() > limit {
                    return Err(format!("Removal record {key:?} exceeds active transport limit {limit}; reconnect with a sufficient payload budget").into());
                }
            }
            let mut candidate = delta.clone();
            record.add(&mut candidate);
            if matches!(record, Record::Player(p) if p.id == owner) {
                candidate.ack_client_tick = desired.ack_client_tick;
            }
            let count = base.players.len()
                + base.props.len()
                + candidate
                    .changed_players
                    .iter()
                    .filter(|p| !old_players.contains_key(&p.id))
                    .count()
                + candidate
                    .changed_props
                    .iter()
                    .filter(|p| !old_props.contains_key(&p.id))
                    .count()
                - candidate.removed_players.len()
                - candidate.removed_props.len();
            if count <= MAX_REPLICATED_ENTITIES
                && encode_update(&candidate, self.baseline.is_none())?.1.len() <= limit
            {
                if fair.as_ref() == Some(&key) {
                    self.cursor = Some(key);
                }
                delta = candidate;
            }
        }
        let (packet, bytes) = encode_update(&delta, self.baseline.is_none())?;
        if bytes.len() > limit {
            return Err(format!(
                "Replication envelope needs {} bytes; transport budget is {limit}",
                bytes.len()
            )
            .into());
        }
        self.issued = tick;
        self.pending = Some(Pending {
            packet,
            bytes,
            accepted: false,
            attempts: 0,
        });
        Ok(())
    }
}
fn encode_update(delta: &DeltaSnapshot, initial: bool) -> crate::Result<(Packet, Vec<u8>)> {
    let packet = if initial {
        Packet::Snapshot(delta.apply_to(&WorldSnapshot::default()))
    } else {
        Packet::Delta(delta.clone())
    };
    let bytes = serde_json::to_vec(&packet)?;
    Ok((packet, bytes))
}
/// Reject unsupported individual records before starting a partial world. Reserve
/// worst-case sequence/ack digits so a supported record stays supported over time.
pub fn validate_world(world: &WorldSnapshot, limit: usize) -> crate::Result<()> {
    if world.players.len() + world.props.len() > MAX_REPLICATED_ENTITIES {
        return Err(format!("Replication supports at most {MAX_REPLICATED_ENTITIES} relevant entities per peer; reduce relevance or entity count").into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for record in world
        .players
        .iter()
        .cloned()
        .map(Record::Player)
        .chain(world.props.iter().cloned().map(Record::Prop))
    {
        let key = record.key();
        let finite_v = |v: crate::math::V| v.0.is_finite() && v.1.is_finite() && v.2.is_finite();
        let finite = match &record {
            Record::Player(p) => {
                finite_v(p.position)
                    && p.yaw.is_finite()
                    && p.pitch.is_finite()
                    && p.vertical_vel.is_finite()
            }
            Record::Prop(p) => {
                finite_v(p.position)
                    && finite_v(p.linear_velocity)
                    && finite_v(p.angular_velocity)
                    && p.rotation.iter().all(|n| n.is_finite())
            }
            _ => true,
        };
        if !finite {
            return Err(format!("Replication record {key:?} has a non-finite number; fix the authoritative transform or velocity").into());
        }
        if !seen.insert(key.clone()) {
            return Err(format!("Duplicate replication entity {key:?}").into());
        }
        if matches!(&key, Key::Prop(id) if id.len() > MAX_ENTITY_ID_BYTES) {
            return Err(format!(
                "Replication entity ID exceeds {MAX_ENTITY_ID_BYTES} UTF-8 bytes; shorten {key:?}"
            )
            .into());
        }
        let mut delta = DeltaSnapshot {
            session: Some([u64::MAX; 2]),
            base_tick: u64::MAX,
            target_tick: u64::MAX,
            ack_client_tick: u64::MAX,
            changed_players: vec![],
            changed_props: vec![],
            removed_players: vec![],
            removed_props: vec![],
        };
        record.add(&mut delta);
        let bytes = encode_update(&delta, false)?.1.len();
        if bytes > limit {
            return Err(format!("Replication record {key:?} requires {bytes} bytes including envelope; active transport allows {limit}. Shorten IDs or use a transport with sufficient payload").into());
        }
    }
    Ok(())
}
/// Apply exactly one represented update. Duplicate/older updates are harmless;
/// a future delta with a missing baseline requests explicit resynchronization.
pub fn receive_update(
    baseline: &mut Option<WorldSnapshot>,
    packet: Packet,
) -> Result<Option<WorldSnapshot>, MissingBaseline> {
    let tick = match &packet {
        Packet::Snapshot(s) => s.tick,
        Packet::Delta(d) => d.target_tick,
        _ => return Ok(None),
    };
    if baseline.as_ref().is_some_and(|b| tick <= b.tick) {
        return Ok(None);
    }
    let next = match packet {
        Packet::Snapshot(s) => s,
        Packet::Delta(d) => {
            let base = baseline
                .as_ref()
                .filter(|b| b.tick == d.base_tick && b.session == d.session)
                .ok_or(MissingBaseline)?;
            d.apply_to(base)
        }
        _ => unreachable!(),
    };
    if next.players.len() + next.props.len() > MAX_REPLICATED_ENTITIES {
        return Err(MissingBaseline);
    }
    *baseline = Some(next.clone());
    Ok(Some(next))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissingBaseline;
