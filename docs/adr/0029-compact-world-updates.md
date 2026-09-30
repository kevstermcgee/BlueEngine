# ADR 0029: World updates in a compact binary form (protocol 8)

Status: Accepted. Supersedes the JSON snapshot/delta codec of ADR 0014 (its bounded partial-world replication, budgets
and acknowledgement rules are unchanged).

## Context

The freshness measurement behind ADR 0028 showed that once a crowd exceeds one packet, how stale a player's state is
depends on how many records fit in 1,100 bytes. As JSON a player record is about 172 bytes and a prop about 183, so a
packet carried six players. ADR 0028's priority ordering made the nearest players fresh; the packet size is what limits
how many others can be.

## Decision

`Packet::Snapshot` and `Packet::Delta` are sent as a compact binary form (`net::worldwire`); every other packet stays
JSON. A leading `0xB1` byte marks a binary world update (JSON always begins `{` or `"`), so `Packet::decode` tells them
apart and everything that already went through `Packet::encode`/`decode` needed no change. `PROTOCOL_VERSION` is 8; a
protocol 7 peer is refused at the handshake.

* **Exact, not lossy.** Floats are their 32-bit patterns and decode bit-for-bit, because the server decides what to
  resend by comparing exact states; a quantised encoding would make a record look changed forever (or require the
  server to quantise its own state first, a separate and larger change). Only values that are exactly `+0.0` are
  omitted (a flag), so `-0.0` round-trips too.
* **Small where it is free.** Ids, counts and ticks are variable-length integers (new canonical `varint`/`signed`
  in the codec, which refuses non-minimal and overlong forms). A record's tick is an offset from the packet's tick, so
  it is one byte. Flags carry the booleans, the character kind and "has room". Zero `pitch`, zero vertical velocity and
  zero prop velocities are omitted.
* **Sizes** (measured): a walking player 19 to 28 bytes against 171 to 174 as JSON (6 to 9 times smaller); a prop 39 to
  51 against 183 to 184. The delta envelope is 32 bytes typically (53 worst case). A packet carries about 45 to 50
  players instead of 6.
* **Replication sizing follows.** A delta still writes every list, so adding a record costs exactly its encoded length
  (the count is one byte below 128 records); the fast arithmetic path is now also trusted for accepted records, not just
  to reject, and the owner's record, initial snapshots and `with_exact_sizing` still measure the whole delta. The
  differential test from ADR 0027 holds both to the same packets. The entity count is tracked incrementally and
  `DeltaSnapshot::apply_to` indexes players instead of scanning.
* **Hostile input.** Decoding is bounds-checked; floats must be finite; list counts are capped by
  `MAX_REPLICATED_ENTITIES` and by what the remaining bytes could physically hold (no allocation from a claimed count);
  reserved flag bits, unknown kinds, bad session flags and trailing bytes are refused.

## Evidence

Mean staleness in broadcasts (50 ms), `examples/net_freshness_bench.rs`; nearest 8 / farthest 8 in brackets:

| Players | JSON, fair cursor (before ADR 0028) | JSON + priority | binary + priority |
|---|---|---|---|
| 32 | 12.9 (11.7 / 13.5) | 7.5 (1.8 / 14.5) | **0.0** (0.0 / 0.0) |
| 64 | 28.3 (24.1 / 29.3) | 15.1 (2.8 / 21.2) | **0.1** (0.0 / 0.3) |
| 128 | 60.2 (52.3 / 61.3) | 28.1 (4.5 / 30.9) | **0.8** (0.0 / 1.0) |
| 256 | 115.7 (100.6 / 123.5) | 59.5 (9.0 / 73.5) | **2.3** (0.1 / 2.5) |

The two changes depend on each other: with the compact encoding but *without* priority ordering the nearest 8 players
of a 140-player crowd were 26.7 broadcasts stale (a mutation check of the priority test). Checked by `tests/world_wire.rs`
(11 tests: exact round trips of 6,000 random updates including `-0.0`, subnormals, `u64::MAX`; every truncation of a valid
update refused; 24,000 hostile inputs without a panic; the size claims), the existing 640 tests, and a real-UDP run of 48
clients through `be2-headless` (7,632 deltas, 48 snapshots, no resyncs, identical at 1 and 4 server threads).

**What did not improve: CPU.** A broadcast costs about the same as before (128 players on one thread: 13.5 ms, against
12.7 ms with JSON sizing), because each packet now carries about five times as many records and `prepare`'s per-record
bookkeeping (the changed-record maps, ordering, validation) is the floor, not serialization. So each broadcast delivers
roughly 4.6 times as much state for about the same CPU. Four network threads still divide it (128 players: 5.1 ms).

## Not adopted

* **Quantised positions and angles.** Would shrink a player to about 12 bytes but breaks the exact-comparison contract
  unless the server also quantises its authoritative state; a separate decision.
* **Per-field deltas against the baseline.** Sending only the changed fields of a record would shrink packets further, at
  the cost of a much more complex delta and ack model.
* **Binary client input.** The server spends about 0.5 ms per tick decoding JSON inputs from 56 clients (`--profile`); a
  worthwhile follow-up with the same framing, but outside "world updates".
* **Version negotiation / serving both protocols.** The handshake already refuses a mismatch with a clear message;
  two wire formats in one server would double the test surface for little gain at this stage.

## Consequences

* Protocol 7 clients and servers no longer interoperate; rebuild both peers.
* Crowds of several dozen players are kept fresh within one broadcast, and 256 players within a few.
* A packet's size no longer tells you its record count by eye; tools that printed world updates as JSON should decode
  them with `Packet::decode` (and `serde_json::to_value` to print).
* The remaining per-broadcast cost is the per-record bookkeeping in `ReplicationSender::prepare`; making it independent of
  crowd size is spatial interest management, the next lever.
