//! Optional bounded event stream. State and events have independent budgets and acknowledgements.
use super::wire::Token;
use super::NetGame;
use crate::viewer::net::codec::{Reader, WireError, WireResult, Writer};
use std::collections::{BTreeMap, VecDeque};

pub(super) const PROTOCOL: &str = "NEV2";
const MAGIC: &[u8] = PROTOCOL.as_bytes();
pub(super) const STATE_HEADER: usize = 29;
const DATA_HEADER: usize = 47;
const MAX_EVENTS: usize = 4096;
const MAX_BYTES: usize = 1024 * 1024;
const MAX_BATCH: usize = 128;

pub(super) fn recognizes(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}
fn header(kind: u8, token: Token, epoch: u64) -> Writer {
    let mut w = Writer::new();
    w.raw(MAGIC);
    w.u8(kind);
    w.token(token);
    w.u64(epoch);
    w
}
pub(super) fn state(token: Token, epoch: u64, body: &[u8]) -> Vec<u8> {
    let mut w = header(1, token, epoch);
    w.raw(body);
    w.finish()
}
pub(super) fn lobby(token: Token, epoch: u64, body: &[u8]) -> Vec<u8> {
    let mut w = header(5, token, epoch);
    w.raw(body);
    w.finish()
}
pub(super) fn ack(token: Token, epoch: u64, sequence: u64) -> Vec<u8> {
    let mut w = header(3, token, epoch);
    w.u64(sequence);
    w.finish()
}
fn gap(token: Token, epoch: u64, base: u64, sequence: u64) -> Vec<u8> {
    let mut w = header(4, token, epoch);
    w.u64(base);
    w.u64(sequence);
    w.finish()
}
pub(super) enum Frame<'a> {
    Lobby {
        token: Token,
        epoch: u64,
        body: &'a [u8],
    },
    State {
        token: Token,
        epoch: u64,
        body: &'a [u8],
    },
    Data {
        token: Token,
        epoch: u64,
        base: u64,
        first: u64,
        entries: Vec<(u32, &'a [u8])>,
    },
    Gap {
        token: Token,
        epoch: u64,
        base: u64,
        sequence: u64,
    },
    Ack {
        token: Token,
        epoch: u64,
        sequence: u64,
    },
}
pub(super) fn decode(bytes: &[u8]) -> WireResult<Frame<'_>> {
    if bytes.len() > super::wire::MAX_DATAGRAM {
        return Err(WireError("event frame too large"));
    }
    let mut r = Reader::new(bytes);
    if r.take(4)? != MAGIC {
        return Err(WireError("event frame magic"));
    }
    let kind = r.u8()?;
    let token = r.token()?;
    let epoch = r.u64()?;
    let frame = match kind {
        5 => Frame::Lobby {
            token,
            epoch,
            body: r.rest(),
        },
        1 => Frame::State {
            token,
            epoch,
            body: r.rest(),
        },
        2 => {
            let base = r.u64()?;
            let first = r.u64()?;
            let count = r.u16()? as usize;
            if base == 0
                || first < base
                || count == 0
                || count > MAX_BATCH
                || first.checked_add(count as u64).is_none()
            {
                return Err(WireError("event sequence range"));
            }
            let mut entries = Vec::with_capacity(count);
            for _ in 0..count {
                let tick = r.u32()?;
                let len = r.u16()? as usize;
                entries.push((tick, r.take(len)?));
            }
            Frame::Data {
                token,
                epoch,
                base,
                first,
                entries,
            }
        }
        4 => {
            let base = r.u64()?;
            let sequence = r.u64()?;
            if base == 0 || sequence < base || sequence == u64::MAX {
                return Err(WireError("event gap range"));
            }
            Frame::Gap {
                token,
                epoch,
                base,
                sequence,
            }
        }
        3 => Frame::Ack {
            token,
            epoch,
            sequence: r.u64()?,
        },
        _ => return Err(WireError("event frame kind")),
    };
    r.done()?;
    Ok(frame)
}
struct Entry {
    sequence: u64,
    tick: u32,
    bytes: Vec<u8>,
    oversized: bool,
}
#[derive(Default)]
pub(super) struct Sender {
    entries: VecDeque<Entry>,
    next: u64,
    bytes: usize,
    oversized: u64,
}
impl Sender {
    pub fn reset(&mut self) {
        self.entries.clear();
        self.next = 1;
        self.bytes = 0;
        self.oversized = 0;
    }
    pub fn last(&self) -> u64 {
        self.next.saturating_sub(1)
    }
    pub fn push<G: NetGame>(&mut self, tick: u32, event: &G::Event) {
        let mut w = Writer::new();
        G::write_event(event, &mut w);
        let bytes = w.finish();
        // An impossible event is represented by a sequence gap, never allowed to starve state.
        let sequence = self.next;
        self.next += 1;
        let oversized = bytes.len() + DATA_HEADER + 6 > super::wire::MAX_DATAGRAM;
        let bytes = if oversized {
            self.oversized += 1;
            Vec::new()
        } else {
            bytes
        };
        self.bytes += bytes.len();
        self.entries.push_back(Entry {
            sequence,
            tick,
            bytes,
            oversized,
        });
        while self.entries.len() > MAX_EVENTS
            || self.bytes > MAX_BYTES
            || self
                .entries
                .front()
                .is_some_and(|e| tick.saturating_sub(e.tick) as u64 > G::TICK_HZ.saturating_mul(10))
        {
            self.pop_front();
        }
    }
    fn pop_front(&mut self) {
        if let Some(e) = self.entries.pop_front() {
            self.bytes -= e.bytes.len();
        }
    }
    pub fn stats(&self) -> super::server::ServerEventStats {
        super::server::ServerEventStats {
            emitted: self.last(),
            retained: self.entries.len(),
            retained_bytes: self.bytes,
            oversized: self.oversized,
        }
    }
    pub fn packets(
        &self,
        token: Token,
        epoch: u64,
        acknowledged: u64,
        limit: usize,
        cursor: &mut u64,
    ) -> Vec<Vec<u8>> {
        let limit = limit.min(super::wire::MAX_DATAGRAM);
        let resume = *cursor;
        let mut remaining = self.entries.iter().peekable();
        while remaining.peek().is_some_and(|e| e.sequence <= acknowledged) {
            remaining.next();
        }
        let mut packets = Vec::new();
        let base = self.entries.front().map_or(self.next, |e| e.sequence);
        // Four packets per snapshot bounds bandwidth and avoids a catch-up flood.
        for packet_index in 0..4 {
            // Always retry the oldest missing batch, then pipeline newer batches. Repeating only
            // the first four makes throughput depend on ACK round-trip time instead of bandwidth.
            if packet_index == 1 && resume < self.next {
                while remaining.peek().is_some_and(|e| e.sequence < resume) {
                    remaining.next();
                }
            }
            let Some(first) = remaining.peek() else {
                break;
            };
            if first.oversized || DATA_HEADER + 6 + first.bytes.len() > limit {
                let packet = gap(token, epoch, base, first.sequence);
                if packet.len() > limit {
                    break;
                }
                packets.push(packet);
                *cursor = first.sequence + 1;
                remaining.next();
                continue;
            }
            let mut w = header(2, token, epoch);
            w.u64(base);
            w.u64(first.sequence);
            let mut payload = Writer::new();
            let mut count = 0;
            let mut expected = first.sequence;
            while let Some(e) = remaining.peek() {
                if e.oversized
                    || e.sequence != expected
                    || count == MAX_BATCH
                    || DATA_HEADER + payload.len() + 6 + e.bytes.len() > limit
                {
                    break;
                }
                payload.u32(e.tick);
                payload.u16(e.bytes.len() as u16);
                payload.raw(&e.bytes);
                count += 1;
                expected += 1;
                remaining.next();
            }
            if count == 0 {
                break;
            }
            w.u16(count as u16);
            w.raw(payload.as_slice());
            packets.push(w.finish());
            *cursor = expected;
        }
        // Remember reaching the tail even if more events arrive before the next send. Otherwise
        // a moving tail keeps the cursor on new data forever and only one old hole gets retried.
        if remaining.peek().is_none() {
            *cursor = 0;
        }
        packets
    }
}
#[derive(Default)]
pub(super) struct Receiver<E> {
    pub epoch: u64,
    pub acknowledged: u64,
    pub gaps: u64,
    pending: BTreeMap<u64, (u32, E)>,
}
impl<E> Receiver<E> {
    pub fn new() -> Self {
        Self {
            epoch: 0,
            acknowledged: 0,
            gaps: 0,
            pending: BTreeMap::new(),
        }
    }
    pub fn reset(&mut self, epoch: u64) {
        self.epoch = epoch;
        self.acknowledged = 0;
        self.pending.clear();
    }
    fn advance_base(&mut self, base: u64) {
        if base > self.acknowledged.saturating_add(1) {
            self.gaps += base - self.acknowledged - 1;
            self.acknowledged = base - 1;
            self.pending.retain(|seq, _| *seq >= base);
        }
    }
    fn drain_pending(&mut self) -> Vec<(u32, E)> {
        let mut events = Vec::new();
        while let Some(e) = self.pending.remove(&(self.acknowledged + 1)) {
            self.acknowledged += 1;
            events.push(e);
        }
        events
    }
    pub fn skip(&mut self, base: u64, sequence: u64) -> Vec<(u32, E)> {
        // Only the authenticated retention floor can certify earlier events as unavailable.
        self.advance_base(base);
        // A reordered gap cannot jump past earlier deliverable events.
        if sequence == self.acknowledged.saturating_add(1) {
            self.acknowledged = sequence;
            self.gaps += 1;
        }
        self.drain_pending()
    }
    pub fn receive<G: NetGame<Event = E>>(
        &mut self,
        base: u64,
        first: u64,
        entries: &[(u32, &[u8])],
    ) -> WireResult<Vec<(u32, E)>> {
        // Validate the complete frame before changing acknowledgement or delivering any event.
        let mut decoded = Vec::with_capacity(entries.len());
        for (tick, bytes) in entries {
            let mut r = Reader::new(bytes);
            let e = G::read_event(&mut r)?;
            r.done()?;
            decoded.push((*tick, e));
        }
        self.advance_base(base);
        for (i, e) in decoded.into_iter().enumerate() {
            let seq = first + i as u64;
            if seq > self.acknowledged && seq - self.acknowledged <= MAX_EVENTS as u64 {
                self.pending.entry(seq).or_insert(e);
            }
        }
        Ok(self.drain_pending())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::netplay::toy::ToyGame;
    #[test]
    fn malformed_and_truncated_frames_fail_without_partial_delivery() {
        let good = ack([4, 5], 3, 77);
        for end in 0..good.len() {
            assert!(decode(&good[..end]).is_err());
        }
        let mut extra = good.clone();
        extra.push(0);
        assert!(decode(&extra).is_err());
        let mut huge = vec![0; super::super::wire::MAX_DATAGRAM + 1];
        huge[..4].copy_from_slice(MAGIC);
        assert!(decode(&huge).is_err());
        for byte in 0..=255 {
            let _ = decode(&[byte; 64]);
        }
    }
    #[test]
    fn future_sequences_cannot_allocate_an_unbounded_reorder_buffer() {
        let mut receiver = Receiver::new();
        receiver.reset(1);
        let empty: Vec<(u32, &[u8])> = Vec::new();
        let _ = receiver.receive::<ToyGame>(1, u64::MAX - 1, &empty);
        assert!(receiver.pending.is_empty());
        assert_eq!(receiver.acknowledged, 0);
    }
    #[test]
    fn reordered_gap_preserves_earlier_events_and_drains_buffered_successors() {
        let mut receiver = Receiver::new();
        receiver.reset(1);
        let entries = [(42, &[0][..])];
        assert!(receiver
            .receive::<ToyGame>(1, 2, &entries)
            .unwrap()
            .is_empty());
        assert!(receiver.skip(1, 3).is_empty());
        assert_eq!(receiver.acknowledged, 0);
        assert_eq!(receiver.gaps, 0);
        let delivered = receiver.skip(1, 1);
        assert_eq!(delivered, vec![(42, super::super::toy::ToyEvent::Started)]);
        assert_eq!(receiver.acknowledged, 2);
        assert_eq!(receiver.gaps, 1);
        assert!(receiver.skip(1, 1).is_empty());
        assert_eq!(receiver.gaps, 1);
    }
    #[test]
    fn malformed_retention_frames_cannot_change_acknowledgement() {
        let good = gap([1, 2], 3, 2, 5);
        for end in 0..good.len() {
            assert!(decode(&good[..end]).is_err());
        }
        let mut extra = good;
        extra.push(0);
        assert!(decode(&extra).is_err());
        for (base, sequence) in [(0, 1), (2, 1), (1, u64::MAX)] {
            assert!(decode(&gap([1, 2], 3, base, sequence)).is_err());
        }
        let mut receiver = Receiver::new();
        receiver.reset(1);
        let entries = [(42, &[0][..]), (42, &[255][..])];
        assert!(receiver.receive::<ToyGame>(5, 5, &entries).is_err());
        assert_eq!(receiver.acknowledged, 0);
        assert_eq!(receiver.gaps, 0);
        assert!(receiver.pending.is_empty());
    }
    #[test]
    fn every_generated_packet_respects_the_negotiated_limit() {
        let mut sender = Sender::default();
        sender.reset();
        for sequence in 1..=100 {
            sender.entries.push_back(Entry {
                sequence,
                tick: 12,
                bytes: vec![3; 64],
                oversized: false,
            });
        }
        for limit in [120, 600, 1100, 1200] {
            for packet in sender.packets([1, 2], 5, 0, limit, &mut 0) {
                assert!(packet.len() <= limit.min(super::super::wire::MAX_DATAGRAM));
                assert!(decode(&packet).is_ok());
            }
        }
    }

    #[test]
    fn moving_tail_still_retries_lost_batches_before_acknowledgement() {
        let mut sender = Sender::default();
        sender.reset();
        for sequence in 1..=60 {
            sender.entries.push_back(Entry {
                sequence,
                tick: 1,
                bytes: vec![0],
                oversized: false,
            });
        }
        sender.next = 61;
        let mut cursor = 0;
        let _lost = sender.packets([1, 2], 1, 0, 120, &mut cursor);
        for sequence in 61..=70 {
            sender.entries.push_back(Entry {
                sequence,
                tick: 2,
                bytes: vec![0],
                oversized: false,
            });
        }
        sender.next = 71;
        let _also_lost = sender.packets([1, 2], 1, 0, 120, &mut cursor);
        for sequence in 71..=80 {
            sender.entries.push_back(Entry {
                sequence,
                tick: 3,
                bytes: vec![0],
                oversized: false,
            });
        }
        sender.next = 81;
        let retry = sender.packets([1, 2], 1, 0, 120, &mut cursor);
        assert!(retry.len() <= 4);
        assert!(retry.iter().any(|packet| matches!(decode(packet).unwrap(), Frame::Data { first, entries, .. } if first <= 20 && first + entries.len() as u64 > 20)), "new emissions cannot starve retries of a lost earlier batch");
    }

    #[test]
    fn eviction_then_four_transport_oversized_events_recovers_without_new_events() {
        for limit in [120, 600, 1100] {
            for reordered in [false, true] {
                let mut sender = Sender::default();
                sender.reset();
                // Sequence 1 has already been evicted; 2-5 are retained but cannot fit this peer.
                for sequence in 2..=6 {
                    sender.entries.push_back(Entry {
                        sequence,
                        tick: 42,
                        bytes: if sequence == 6 {
                            vec![0]
                        } else {
                            vec![0; limit]
                        },
                        oversized: false,
                    });
                }
                sender.next = 7;
                let mut receiver = Receiver::new();
                receiver.reset(1);
                let mut delivered = Vec::new();
                let mut cursor = 0;
                for _ in 0..8 {
                    let mut packets =
                        sender.packets([1, 2], 1, receiver.acknowledged, limit, &mut cursor);
                    assert!(packets.len() <= 4);
                    if reordered {
                        packets.reverse();
                    }
                    for packet in packets {
                        assert!(packet.len() <= limit);
                        match decode(&packet).unwrap() {
                            Frame::Gap { base, sequence, .. } => {
                                delivered.extend(receiver.skip(base, sequence))
                            }
                            Frame::Data {
                                base,
                                first,
                                entries,
                                ..
                            } => {
                                delivered.extend(
                                    receiver.receive::<ToyGame>(base, first, &entries).unwrap(),
                                );
                            }
                            _ => panic!("unexpected event packet"),
                        }
                    }
                }
                assert_eq!(delivered.len(), 1, "limit={limit}, reordered={reordered}");
                assert_eq!(delivered[0].0, 42);
                assert_eq!(receiver.acknowledged, 6);
                assert_eq!(receiver.gaps, 5);
            }
        }
    }
}
