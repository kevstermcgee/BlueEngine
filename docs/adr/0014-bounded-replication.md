# ADR 0014: Bounded acknowledged partial-world replication

Status: Accepted

## Context

A legal eight-player snapshot already exceeds the unchanged 1100-byte JSON packet
ceiling. The server discarded encoding/send errors while putting the complete world
in a 60-entry history. A full QUIC facade queue reported successful submission after
dropping the packet. Deterministic regressions reproduced both failures before the
implementation changed.

## Decision

Protocol 7 uses the existing JSON snapshot/delta codec for bounded partial worlds.
Keep one immutable packet in flight per peer, and commit only its represented state
on an exact, session-bound acknowledgement after local acceptance. Retransmit at the
existing 20 Hz opportunities. Reserve a rotating dirty record; prioritize the owner,
removals and held props in remaining space. Alternate owner/fair priority where only
one record fits. Rotate peers and the independent full GameState lane under pressure.

New/resynchronized peers start with a partial snapshot, then accumulate deltas.
No periodic full-world keyframe can erase incremental progress. Session tokens bind
world/game state and resync requests; tick floors coalesce duplicate resync requests
while allowing round resets to supersede an initial, unacknowledged keyframe. Hello
retries preserve existing authority and replication. Queue acceptance, rejection,
worker submission/drop and application acknowledgement have separate counters.

Implementation: [replication.rs](../../src/viewer/net/replication.rs),
[transport.rs](../../src/viewer/net/transport.rs), and
[server.rs](../../src/viewer/server.rs). Limits and measurements live in the existing
[hosting guide](../HOSTING.md).

## Consequences

Memory is bounded by one world plus one packet per peer and bounded transport queues.
Settled supported state converges when updates and acknowledgements keep getting
through. Stop-and-wait trades throughput/latency at high RTT for simple, exact recovery
semantics. A partial world intentionally contains records from different authority
ticks; GameState remains independently repeated. Individually oversized records and
invalid configurations fail explicitly. No packet-ceiling increase, binary codec,
renderer dependency, or second gameplay loop is introduced.
