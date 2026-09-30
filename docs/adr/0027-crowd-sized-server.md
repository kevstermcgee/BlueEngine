# ADR 0027: A crowd-sized server: capacity, cheaper replication, and parallel preparation

Status: Accepted

## Context

The load measurements in `docs/perf` stopped at 8 clients (4.7% of a core), the hard-coded cap, and concluded the
server had large headroom. A benchmark that admits many synthetic clients to a real `DedicatedServer` showed
otherwise once clients acknowledged packets like live connections: a snapshot broadcast took 7.4 ms at 64 players and
29.8 ms at 128, longer than the 16.7 ms tick it runs inside. The cost is quadratic, because each peer's update walks
the changed records of every other relevant player.

Where the time went (measured, not assumed): building the snapshot was small (5 us per peer at 256 players);
validating it about 15%; and about 75% was the fit loop in `ReplicationSender::prepare`, which decided whether each
candidate record fitted the packet by serializing the entire delta built so far, including after the packet was full.

## Decision

* **Capacity is configuration.** `DedicatedServer::with_max_players(n)` (1..=1024, default 8) raises the world's join
  cap, the session registry and the handshake rate together; `be2-headless` gets `--max-players`. Existing servers
  behave as before, including the refusal text.
* **Size candidates by arithmetic.** A delta always serializes all four lists, so adding a record costs exactly its own
  serialized length plus a comma when the list is non-empty. A candidate that cannot fit is skipped without
  re-serializing the delta. The owner's record (whose acknowledgement digits can change the envelope) and initial
  snapshots are still measured exactly. A differential test runs the new and the original method side by side over
  thousands of packets, with varied budgets, loss and resyncs, and requires byte-identical output.
* **Split `send` into `stage` and `transmit`.** `stage` (validate, invalidate a stale packet, prepare) depends only on
  the sender and the snapshot and is the expensive half; `transmit` is the cheap submit. `send` is their composition,
  so every existing caller is unchanged.
* **Prepare peers in parallel, send on one thread.** `with_network_threads(n)` (`--network-threads`) runs `stage` for
  peers across `std::thread::scope` workers reading the shared `&HeadlessWorld`, each writing only its own session.
  Transmission stays on the calling thread in the existing fairness-rotated order, so transports need not be `Sync`
  (the QUIC transport holds a non-`Sync` receiver) and the send order and first-error behaviour are unchanged. Fewer
  than `PARALLEL_MIN_PEERS` (32) peers are never split: measured, it saves under 0.3 ms there and costs CPU.

Result (Intel N97, 4 cores, microseconds per broadcast): 128 players 29,825 -> 12,755 (sizing) -> 4,161 (4 threads);
256 players 120,587 -> 51,392 -> 16,302. See `docs/perf/README.md` for the full table and caveats.

Not adopted:

* **A persistent worker pool or a channel-based I/O thread.** The parallel stage is a pure function of shared
  read-only state; scoped threads need no queues, no lifetimes beyond the call and no unsafe code, and start in
  microseconds against a 50 ms broadcast period. Moving socket reads to their own thread would not help: the receive
  side is cheap at these rates and UDP reads are non-blocking.
* **Parallel simulation.** The authoritative world stays single-threaded: determinism is a core contract.
* **Threads by default.** Below about 32 peers they cost more than they save.

## Consequences

* A server can host a hundred or more players on four cores within the tick budget; the practical single-thread limit
  moves from about 100 players to about 250 with threads, on this hardware.
* Replication output depends on the digit width of the session token (it is written into every packet), so tests that
  compare two servers packet for packet must pin the tokens, as `tests/parallel_broadcast.rs` does.
* Still open, in order of value: spatial interest management (makes the per-peer cost independent of crowd size),
  batching the per-peer game-state packet, multi-threading the QUIC endpoint, and measuring real-socket cost.
