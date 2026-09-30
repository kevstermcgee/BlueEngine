# ADR 0028: Three changes from the external study: replication priority, an injectable clock, span summaries

Status: Accepted

## Context

`docs/analysis/external-inspiration-study.md` read twelve open-source projects for ideas and measured BlueEngine against
each question. Three measurements showed a real problem with a small fix; this ADR records those three. The study
documents the rest (including what was rejected and why).

## Decisions

### 1. Replication spends its packet budget on what is near, and measures freshness

A 1,100-byte packet carries about six player records. Past that, who gets sent is decided by the budget rule, and the
fair rotating cursor treated a player 5 m away like one 150 m away: at 64 players the nearest 8 were 24.1 broadcasts
(1.2 s) stale against 29.3 for the farthest.

`ReplicationSender::prepare` now gives every changed record a priority that grows each broadcast it waits, by
`max(0.1, 1/(1+(d/10 m)^2))` for distance `d` from the observer, and resets when the record is sent. The tail of the
candidate list is sorted by it (stable, so ties keep the fair rotation). Unchanged: the owner's own record, the fair slot
and urgent records (removals, held props) keep their precedence, so nothing starves; a crowd that fits one packet is sent
everything every broadcast. Counters `records_sent`, `wait_sum`, `wait_max` and `mean_wait()` make freshness a number.
Result: nearest-8 staleness 64 players 24.1 -> 2.8, 128 players 52.3 -> 4.5, 256 players 100.6 -> 9.0, and the overall
mean roughly halved. Not adopted: a token-bucket byte cap, channel reliability modes, rollback.

### 2. The server's time comes from a `Clock`

Seven places in `DedicatedServer` read the wall clock for timeouts, last-seen, the reconnect reservation and the handshake
window, so those could only be tested by sleeping, and the 60-second reconnect window not at all. `net::Clock` is the real
clock or a manual one shared by its clones; `DedicatedServer::with_clock` uses it for those decisions and nothing else
(real-time pacing still reads the real clock). The handshake limiter starts its window on the first handshake, not at
construction, so it does not depend on when it was built. Eight tests that would have needed minutes of real waiting run
in 10 ms. A hard-coded "World is full (8 players)" refusal now reports the configured cap.

Not adopted: rewriting the server as a sans-IO state machine. `LoopNet` already gives the transport virtual time and the
session types already take `now`; the clock closes the remaining gap.

### 3. `viewer::spans`: a compact, optional span summary

Phase breakdowns had to be hand-instrumented three times in one day. `spans::span("name")` records into a per-thread
table; `spans::summary()` returns `name count total mean max`, biggest first. Off by default (3.3 ns per span), 58 ns when
on. About ten spans cover the phases of a world step, the Rapier/sync split and the server's poll, step and broadcast
stages; `be2-headless --profile` prints the summary with each status line. Tables are registered per thread and read by
`summary`, not merged in thread-local destructors: a test showed `std::thread::scope` can return before those run (75 of
100 worker spans counted), which is the path the parallel broadcast uses.

Not adopted: timelines, plots, a capture protocol, Tracy itself.

## Consequences

* Nearby players stay fresh in large crowds; `mean_wait()` exposes the cost of the packet budget directly.
* Timeout, reconnect and rate-limit behaviour can be asserted instantly and exactly, which unlocks simulated
  multiplayer fault tests (see the study's section 7).
* Where a tick goes is one flag away, and is assertable in tests.
* Replication record order within a packet changed (priority order); clients apply deltas by id, so nothing depends on it.
* Still open, in order of value (study sections 2, 5, 7): a compact encoding for player records (about six times as many per
  packet, a protocol change), reusing identical materials in `add_box`, and a seeded fault-sequence harness.
