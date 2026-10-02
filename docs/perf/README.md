# Performance data

`metrics.jsonl` is append-only: one JSON row per measurement, never edited. Rows carry commit,
dirty flag, CPU, cores, rustc, linker and sccache state, so compare only rows whose `metric`,
`kind`, `profile`, `cpu` and `linker` match. Record with `python tools/perf.py record`, read with
`python tools/perf.py report`. Pass `--note` saying what changed since the last run.

Fields: `metric` (`build_headless`, `test_build`, `test_run_total`, `test_suite:<name>`,
`sim_mean_tick`), `kind` (`cold`, `incremental_edit`, `warm`, `runtime_2_players`), `profile`,
`value`, `unit`. Rows with `source: manual-...` were measured by hand, not by the script.

## Profiles (opt-in; default dev/release and CI are unchanged)

- `--profile fast`: release without LTO, 16 codegen units, incremental. For iterating and for
  running the headless server on this machine.
- `--profile itest`: dev with optimized dependencies and line-tables-only debuginfo. For tests.
- `--release`: single-unit thin LTO. For shipping only.

## Baseline (Intel N97, 4 cores, 15 GB, rustc 1.98.1, default linker, no sccache)

| Step | Before | After |
|---|---|---|
| headless build after a code edit | 63 s (`--release`) | 15 s (`fast`) |
| `physics_saves` + `save_state` suites | 306 s (`dev`) | 12 s (`itest`; one-time 118 s compile) |
| full `cargo test --no-default-features` | 402 s (`dev`) | 51 s (`itest`, mold + sccache) |
| cold headless build | 82 s (`fast`) | 38 s (`fast`, warm sccache) |
| cold test build | 118 s (`itest`) | 48 s (`itest`, warm sccache) |

Findings behind the numbers: the whole engine is one crate, so `--release` recompiles about 52k
lines as a single serial LTO unit; debug rapier/parry make physics tests 10-25x slower; each of
the 27 test binaries links the full engine. Cold builds spend about 60 s in rapier, parry,
nalgebra, rustls, quinn and ring.

Full `python tools/be2.py check` (all gates, both feature modes): 1,068 s of commands on `dev`, 376 s on
`itest`, which is now its default (`--profile dev` opts out; CI runs cargo directly and is unchanged).

## Server load (loopback, development transport, house map, `--profile fast`)

`python tools/perf.py record --suite server` (about 2 minutes) runs `examples/server_load.rs` against
a real `be2-headless` process. Synthetic clients only walk and jump; no combat, no moving props.

| Clients | Server CPU (% of one core) | Tick mean / max | Per-client bandwidth | Largest packet |
|---|---|---|---|---|
| 1 | 2.2 | 290 / 405 us | 7.6 KB/s | 396 B |
| 2 | 2.5 | 328 / 610 us | 11.1 KB/s | 587 B |
| 4 | 3.1 | 424 / 841 us | 18.4 KB/s | 972 B |
| 8 | 4.7 | 700 / 1569 us | 20.4 KB/s | 1100 B |

Server RSS stays about 6 MB. World updates arrive every 50 ms (20 Hz) with a worst gap of 67 ms and no
resyncs. A 9th client is refused ("Server is full (8 players)"): the cap is hard-coded at
`src/viewer/server.rs:230`, so 16 or 32 players needs an engine change, not just a faster machine.
Headroom is large on this CPU, but the largest packet already uses 1100 of the 1400-byte limit at 8 idle
players. Not covered: real network loss/latency, the production QUIC/TLS transport, props in motion, combat.

## Broadcast cost at scale (`examples/net_broadcast_bench.rs`)

`cargo run --profile fast --example net_broadcast_bench -- 512` admits N synthetic clients to a real
`DedicatedServer` on a transport that only counts (no sockets), moves them, and times one snapshot broadcast (what
the server thread spends preparing and handing off every peer's update, 20 times a second). Every client
acknowledges each packet, as a live connection does; an earlier version of the benchmark did not, so each peer just
re-sent one packet and the numbers looked far better than reality. Intel N97, 4 cores, `fast` profile; microseconds
per broadcast:

| Players | Original | Arithmetic sizing | + 2 threads | + 4 threads | Budget: one 60 Hz tick is 16,667 us |
|---|---|---|---|---|---|
| 16 | 492 | 277 | - | - | |
| 32 | 1,878 | 886 | 651 | 598 | |
| 64 | 7,421 | 3,240 | 1,879 | 1,787 | original already 45% of a tick |
| 128 | 29,825 | 12,755 | 6,681 | 4,161 | original: **1.8 ticks** |
| 256 | 120,587 | 51,392 | 26,731 | 16,302 | original: 7 ticks |
| 512 | 489,066 | 211,633 | 109,150 | 67,236 | |

What this shows and does not:

* The original code could not hold 60 Hz past about 100 players: a broadcast ran inside the tick, and its cost per
  peer grows with the number of other players, so the total is quadratic. Nothing in the earlier 8-client
  measurements (4.7% of a core) could reveal that.
* About half of the saving is not threads: replication measured every candidate record by serializing the whole
  delta, including after the packet was full. Sizing by arithmetic (exact: a delta always writes all four lists)
  is 2.3x faster at 256 players on one thread with byte-identical output. Threads add about 3x on 4 cores on top.
* Below about 32 peers threads do not pay (they cost more total CPU than they save); `PARALLEL_MIN_PEERS` keeps
  small servers single-threaded. A real-UDP run of 40 clients against `be2-headless --max-players 48` showed the
  same packet counts and no resyncs at 1 and 4 threads, with 4 threads using more CPU at that size.
* The cost is still quadratic. The remaining per-peer work (validating the snapshot, ordering the changed records,
  measuring each candidate) all grows with the number of relevant players; spatial interest management that limits
  what each peer is considered for is the next step and would make it linear.
* Not measured: real socket and QUIC/TLS cost (the QUIC endpoint still runs on one async thread), props in motion,
  and receive-side cost.
## Freshness and where a tick goes

`cargo run --profile fast --example net_freshness_bench` measures, for one watching client, how many broadcasts (50 ms)
ago each other player was last sent, for the 8 nearest and the 8 farthest, as the crowd grows. A packet carries about
six player records, so past 8 players freshness is decided by how the budget is spent. Before distance-weighted priority
(ADR 0028), nearest / farthest at 32 players were 11.7 / 13.5 broadcasts, at 64 players 24.1 / 29.3; after, 1.8 / 14.5 and
2.8 / 21.2 (128 players: 52.3 -> 4.5 nearest; 256 players: 100.6 -> 9.0). `ReplicationCounters::mean_wait()` reports the same
thing from a live sender.

**With the compact binary world updates (ADR 0029, protocol 8)** the same benchmark reads, mean (nearest 8 / farthest 8):
32 players 0.0, 64 players 0.1 (0.0 / 0.3), 128 players 0.8 (0.0 / 1.0), 256 players 2.3 (0.1 / 2.5), because a packet now
carries about 45 to 50 players instead of 6. Broadcast CPU is about unchanged (128 players: 13.5 ms on one thread, 5.1 ms
on four), so each broadcast delivers about 4.6 times as much state for the same cost.


`be2-headless --server ... --profile` prints a span table with each status line (name, count, total ms, mean us, max us,
biggest first). A 56-client server on 4 network threads reported: per-broadcast preparation 3.3 ms (183 us per peer),
receive 0.5 ms per tick, the whole world step 0.4 ms, Rapier 78 us per sub-step. Span cost: 3.3 ns off, 58 ns on.

Idle physics (512 sleeping props, tick 515 to 537 us): Rapier's step is 449 us (87%); BlueEngine's per-prop sync and
lifecycle loops together are under 9 us. Skipping Rapier's step while no body is active measured -85% (526 -> 78 us); not
adopted, see `docs/analysis/external-inspiration-study.md` sections 3 and 6.

Test suite, warm build, 4 cores: `cargo test` 71.7 s (52% of the CPU used), `cargo nextest run` 48.2 s (plus `cargo test
--doc` 3.8 s), 28% faster overall; nextest is optional.

## Publishing (BlueEngineGames, GitHub Actions, windows-latest)

"Build Windows releases" took 15-26 minutes on each of the last five runs (19m, 26m, 15m, 16m, 20m).
Reading `.github/workflows/releases.yml` and `package_native_releases.ps1` in that repo: there is no Rust
cache; the engine is built `--release` from scratch; then each of 6 native games is built `--release`
with its own `target/`, so the physics/math/rendering dependencies compile 7 times; and every game is
rebuilt on every run even when only one changed. Baseline step times (run 36606293975): engine build 243 s, native games 865 s.

Result of the fix on branch `faster-windows-release` of BlueEngineGames (shared target dir + cache):
1,151 s -> 799 s cold cache (native games 508 s) -> 436 s warm cache (engine 159 s, native games 238 s).
The remaining cost is compiling the engine crate itself once per profile group; skipping unchanged
games would be the next step.

## Spooky Kart (a game built on the engine, `~/SpookyKart`)

`python tools/perf.py record --suite kart` runs the game's own load test (`tools/load_test.py` in that repo, or
`$BLUE_KART_DIR`): its real server over real UDP loopback with 1, 4 and 8 bot clients, one full two-lap race each
(about 7 minutes). Metrics are `kart_*`; per-character results are `kart_character_mean_place` and
`kart_character_win_rate` (rows carry a `character` field). Baseline, dev build, N97:

| Clients | Server CPU | Server tick mean / max | RAM | Per client down / up | RTT | Prediction corrections (max error) |
|---|---|---|---|---|---|---|
| 1 | 1.4% of a core | 132 / 517 us | 3.7 MB | 18.0 / 2.2 KB/s | 17 ms | 2 (0.20 m) |
| 4 | 1.6% | 149 / 566 us | 3.8 MB | 18.2 / 2.2 KB/s | 17 ms | 229 (1.11 m) |
| 8 | 1.9% | 173 / 507 us | 3.7 MB | 18.1 / 2.2 KB/s | 17 ms | 1,328 (0.88 m) |

After porting the game onto the engine's `viewer::netplay` kit the same test gave server CPU 1.4 / 1.7 / 2.0%,
tick mean 127 / 162 / 191 us, 18.4-18.5 KB/s down and 2.2 KB/s up per client, and 22 / 487 / 1,358 prediction
corrections (largest error 0.27 / 0.86 / 0.87 m): the kit costs nothing measurable. (An earlier run of the ported game
recorded zero corrections; that was the view reset discarding its counters, fixed in the client. Only the
`kart_client_corrections`, `kart_client_snaps` and `kart_client_max_error` rows of that one run are wrong, and the
run after the fix supersedes them.)

No hard prediction snaps at any level; corrections rise with players because clients do not predict kart-to-kart
bumps. What this game taught us about the engine, with evidence, is in `~/SpookyKart/docs/ENGINE_LESSONS.md`.

## Open ideas, in expected order of value

1. Done on this machine: mold and sccache via `~/.cargo/config.toml` (not the repo, so Windows CI is
   unchanged). Edit-and-rebuild stayed 15 s: it is dominated by compiling the engine crate itself.
   Optional: cargo-nextest for test scheduling.
2. Tiered `be2.py check` (`cargo check`, then focused tests, then full).
3. Split the engine crate so an edit rebuilds less (simulation, net, presentation).
4. Fewer test binaries or a shared test crate to cut links.

## Runtime props and replication (2026-09-29)

Starting source: `bba11ce23548fb89164fe6e3e06c4af1cd68c290`. All three review leads were
confirmed at that checkout. The obsolete full-snapshot overflow problem was already
resolved by ADR 0014; bounded partial replication and its acknowledgement rules remain.
Raw before/after percentiles, sample counts, environment and limitations are in
[`runtime-2026-09-29.json`](runtime-2026-09-29.json).

Actual authority is `HeadlessWorld::step` in `viewer/simulation.rs`, called by
`DedicatedServer::step` and the local `GameSession` scheduler. The shared playable
runner uses that session; online sessions predict local movement and receive authority.
Stock capture fixtures and game-owned custom simulations/protocols are separate paths.

The patch caches prop-to-lifecycle indices, validates public registry identities in
linear time, and rebuilds mappings after mutation. Restore preserves the same IDs or
triggers revalidation; transactional map replacement constructs fresh mappings. Position
generation thresholds, promotion and three-second rest/demotion rules are unchanged.
Every prop still receives lifecycle bookkeeping, avoiding a new event/state machine.
Snapshot construction now uses the existing prop ID index instead of linear searches.

Physics caches each prop's transformed instances and collision bounds. Pose/ownership
changes refresh that prop; unchanged query worlds are retained even with awake bodies.
Final sleeping poses are detected. Explicit corrections/restores conservatively refresh
the caches, and body-to-collider propagation precedes bounds computation. Held props
remain excluded from query/collision, while semantic bounds still follow them. Dynamic
query BVHs still rebuild when the represented geometry changes; no new spatial structure.
The caches retain extra geometry in exchange for less recurring work.

Replication borrows dirty records, packs in place with rollback for rejected records,
and counts JSON bytes without creating candidate packets/byte buffers. Validation counts
the shared worst-case envelope once and serializes borrowed records; all input validation
still runs on retries. Final packets are encoded and checked against the active ceiling.
Pending packet immutability, exact acknowledgements, fair cursor and session rules remain.

Selected results below are microseconds, median / p95, eight players. Each step/snapshot
row has 600 samples; each send row has 1,200 sends. These are optimized `fast` builds
(opt-level 3, no LTO, 16 codegen units), without default features, on Linux/glibc 2.41,
Intel N97 (4 cores, 15.4 GiB), rustc 1.98.1, mold. CPU scaling was enabled and unpinned.
Before/after timing runs were serial; allocation runs were separate.

| Path | Props | Before | After |
|---|---:|---:|---:|
| Idle world step | 32 | 51.04 / 52.49 | 47.53 / 48.77 |
| One moving world step | 32 | 95.29 / 98.75 | 64.33 / 66.03 |
| Idle world step | 128 | 220.12 / 225.56 | 154.26 / 159.45 |
| One moving world step | 128 | 379.13 / 394.86 | 202.47 / 210.55 |
| Idle world step | 512 | 1875.74 / 2005.92 | 657.81 / 719.76 |
| One moving world step | 512 | 2801.80 / 2940.62 | 1021.69 / 1130.50 |
| Many active world step | 512 | 8230.70 / 8396.96 | 7018.10 / 7183.03 |
| All disturbed then resettled step | 512 | 1875.73 / 2073.86 | 658.01 / 712.42 |
| Snapshot preparation | 512 | 618.33 / 646.88 | 51.51 / 53.00 |
| One-peer send, zero RTT | 512 | 2589.36 / 2725.39 | 1664.43 / 1758.47 |
| One-peer send, 200 ms RTT (includes retries) | 512 | 541.92 / 2610.94 | 268.69 / 1717.88 |

The small idle workload was already inexpensive. Larger idle/mostly settled workloads
benefit most; many-active physics remains the dominant step cost. A world step is not
the complete dedicated-server tick: multiply per-peer packing only after measuring
the actual relevance sets and aggregate broadcast path for a specific game.

The whole-process glibc probe counted 186,117,755 allocator calls before and 51,344,391
after (72.4% fewer), including fixture setup, measurement buffers and receiver decode/apply.
Cumulative requested bytes fell from 43,035,459,210 to 6,875,774,660. These are allocator
requests, including reallocations, not successful-allocation attribution or retained heap.
An empty `Vec::new()` does not contribute a call. Peak RSS was 30,596 / 30,168 KiB;
that small difference does not establish a memory improvement for the engine caches.
Serialized retained per-peer payload stayed identical (up to about 119.6 KB in this run),
excluding allocation overhead and transport queues. One baseline and pending packet remain.

Reproduce from the changed checkout, keeping baseline and changed targets separate:

```bash
mkdir -p /tmp/be2-runtime-baseline
git archive bba11ce23548fb89164fe6e3e06c4af1cd68c290 | tar -x -C /tmp/be2-runtime-baseline
cp examples/runtime_perf.rs /tmp/be2-runtime-baseline/examples/runtime_perf.rs
cargo build --locked --profile fast --no-default-features --example runtime_perf --manifest-path /tmp/be2-runtime-baseline/Cargo.toml --target-dir /tmp/be2-runtime-baseline-target
cargo build --locked --profile fast --no-default-features --example runtime_perf
/tmp/be2-runtime-baseline-target/fast/examples/runtime_perf > /tmp/be2-before.txt
target/fast/examples/runtime_perf > /tmp/be2-after.txt
gcc -shared -fPIC -O2 tools/perf_alloc.c -o /tmp/be2-alloc.so
LD_PRELOAD=/tmp/be2-alloc.so /tmp/be2-runtime-baseline-target/fast/examples/runtime_perf > /tmp/be2-before-alloc.txt
LD_PRELOAD=/tmp/be2-alloc.so target/fast/examples/runtime_perf > /tmp/be2-after-alloc.txt
```

The fixture uses two/eight players, 32/128/512 separated apple props, 600 settling ticks,
600 ticks per idle/one/many/resettled scenario, and periodic identical impulses. All props
are explicitly included in the subsequent replication workload; ordinary settled props
demote out of stock snapshots. This stresses continuous relevance-wide churn rather than
claiming that every settled object is normally transmitted. Real sender packing/retries,
receiver application and acknowledgements run for 60 simulated seconds per RTT at 20 Hz.
Age samples use the last 30 seconds, encode the source time in each prop position, and
measure each received prop independently; packet tick is not used as a freshness proxy.

### Freshness and the separately scoped next protocol

The CPU patch leaves entity age exactly unchanged. With two players and 32 continuously
changing props, median / p95 age is 700 / 1500 ms at zero RTT, 750 / 1550 ms at 50 ms RTT,
1500 / 3050 ms at 100 ms RTT and 3050 / 6150 ms at 200 ms RTT. Both bounded record packing
and stop-and-wait matter: zero RTT is already limited by the fair rotation/payload budget.
At eight players/512 props, received-state median age reaches tens of seconds. High-RTT
large worlds have incomplete initial coverage after 30 seconds, so their reported age
distribution excludes missing props and understates the overall freshness problem.
These results do not establish adequate multiplayer freshness for large continuously
moving worlds, even though simulation alone fits the fixed tick budget here.

Proposed separate protocol-8 scope: keep acknowledged persistent entity/ownership/removal
updates and recovery, but move transient poses onto a bounded, coalesced latest-state lane.
Each pose needs session, content/round, stable ID/incarnation and authoritative source tick;
receivers must ignore old poses, deleted incarnations and other sessions. Persistent
relevance entry/keyframes must bootstrap a pose and identity; poses arriving before identity
must not create entities. Removal/reentry/resync must invalidate the appropriate pose floor.
Keep exact persistent acknowledgements and baseline recovery independent of transient delivery.
Reserve fair progress and owner priority across world, pose and GameState lanes under
backpressure; respect each active payload limit. Bound coalesced state to the supported
entity/peer limits and discard it on reconnect. Validate loss, reordered poses/removals,
same-ID recreation, resync and queue rejection before considering rollout. Version this
incompatible separation. Merely sending more deltas or switching codecs is insufficient,
and a pose lane cannot overcome the payload throughput floor by itself.

Nearer CPU priorities are aggregate broadcast measurements for real game relevance,
validated immutable snapshot reuse across peers/retries without weakening public `send`
validation, and query-BVH refitting if remaining rebuild cost warrants it. Lazy body
activation, broader threading and new physics/rendering stacks are not justified here.

### Validation

`python3 tools/be2.py check --changed` passed all ten gates: formatting, rustdoc,
tests and warnings-as-errors clippy in default/headless feature configurations,
headless/authoring checks and Python suites. It reported 1,375 test executions
(550 default Rust, 475 headless Rust, 48 authoring, 302 Python; repeated configurations
are not unique tests). Focused regressions cover registry reorder/removal/reinsertion,
restore, map replacement, sleeping corrections with semantic/collision/ray queries,
unchanged query retention, exact escaped-JSON budgeting and validated immutable retries.
Existing suites exercise pickup/drop, save-state replay, player collision, real sockets,
QUIC, bounded relevance replacement, loss, reorder, reconnect and backpressure.
`python3 scripts/publish_games.py check` also passed (254 files).

The final check used `TMPDIR=/home/kevin/BlueEngine/.be2-work/validation-tmp` because
the machine's `/tmp` tmpfs filled and caused doctest linker bus errors. Repeating with
space available passed; no budgets or tests were relaxed. Windows CI, manual graphical
play, real WAN latency loads and aggregate multi-peer timings for these prop fixtures
were not run. `tools/perf.py record --suite sim --profile fast` recorded the current
headless diagnostic separately (22.429 us mean); it is not the matched prop benchmark.

## Shadow tiers (`examples/shadow_demo.rs`, ADR 0036)

The harness (`perf.py record`) measures builds, tests and the headless simulation, not rendering, so shadows are
measured with the demo itself under Mesa llvmpipe on xvfb (`--capture DIR --frames 1 --exit-after 300 --perf`,
1280x720, 4 cores, `fast` profile; rows `shadow_demo_*`, `source: manual-shadow-demo-2026-10-02`). The scene is small
(a ground quad, a wall, a cube, a pillar and three moving actors), so these are **relative** costs of the tier, in
software GL where fill rate dominates:

| Tier | 300 frames, wall time | Own CPU work per frame (p50) |
|---|---|---|
| Off | 7.5 s | 0.27 ms |
| Simple | 7.7 s (+3%) | 0.41 ms |
| Full (2048 map) | 12.6 s (+68%, about 17 ms more per frame) | 11.8 ms |

Simple is a few hundred extra vertices and one extra draw call. Full pays for a second pass over the casters and for
clearing and filling a 2048x2048 colour plus depth target; under llvmpipe that is raster time, which a GPU does in
a small fraction. Not measured: any real GPU, the Intel N97's own GPU, Windows, a scene with thousands of casters,
the effect of the 9-tap lookup on a real shader core. Treat the Full figure as an upper bound on this machine's
software path, not as a prediction for players; the first run on real hardware should add rows here.

