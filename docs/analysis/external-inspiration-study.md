# External inspiration study

Ten groups of open-source projects were read for ideas that could make BlueEngine better at being BlueEngine: easy for
an AI agent to understand and change, hard to misuse, cheap to build and test, efficient, reliable in multiplayer,
debuggable and deterministic. Nothing here is a plan to copy a project or add a dependency.

**Method.** External projects were shallow-cloned at the commits below and read by source (not built or run, except
where a section says so). BlueEngine was measured directly, at `origin/main` `49978b9` plus the branch that carries
this document. "Measured" below means a number produced on the development machine (Intel N97, 4 cores, 15 GiB,
Linux, rustc 1.98.1, `fast`/`itest` profiles), reproducible from the named example or command. Where a claim about an
external project comes from reading its code, the file is cited.

**Outcome.** Three changes were implemented, each because a measurement showed the problem in BlueEngine:

| # | Change | Evidence | Section |
|---|---|---|---|
| 1 | Distance-weighted replication priority + a freshness metric | nearest players were as stale as the farthest (24.1 vs 29.3 broadcasts at 64 players) | 2 |
| 2 | An injectable clock for the server's timeouts and windows | 7 wall-clock reads; the 60 s reconnect window could not be tested | 4 |
| 3 | A compact span summary (`--profile`) | phase breakdowns had to be hand-instrumented three times in one day | 9 |

Everything else is recorded with a recommendation and its evidence; several are **ALREADY SOLVED** or **NOT USEFUL**
for BlueEngine, which is a legitimate result.

| Project | Recommendation |
|---|---|
| raylib | ALREADY SOLVED (11 of 12 capabilities resolve); KEEP IN MIND one gap |
| Lightyear | **IMPLEMENT** (done: priority accumulation, freshness metric, then the compact binary encoding) |
| Flecs | KEEP IN MIND (the measured cost is not in BlueEngine's bookkeeping) |
| Quinn | **IMPLEMENT** (done: injectable clock); NOT USEFUL: a full sans-IO rewrite |
| Godot | EXPERIMENT (material dedupe, skip default fields) |
| Jolt | KEEP IN MIND (skip the idle Rapier step; measured -85%, needs wake tracking) |
| FoundationDB + Proptest | EXPERIMENT (seeded fault harness + small shrinker); NOT USEFUL: the `proptest` dependency |
| PettingZoo | EXPERIMENT (design sketched, not built) |
| Tracy | **IMPLEMENT** (done: spans and summary); NOT USEFUL: Tracy itself |
| nextest / sccache | nextest: EXPERIMENT (measured -28%, optional); sccache: ALREADY SOLVED locally, EXPERIMENT for CI |

Pinned revisions:

| Project | Commit | Date |
|---|---|---|
| raylib | `6ecf21f700642c797dd0f2f3d4b1f2c91b71711d` | 2026-09-28 |
| Lightyear | `320f825bbeeab62715f0158d07d03320fb1c9cd0` (priority code is in `bevy_replicon` v0.44.1 `608e24a14734377ff1d6edad24c5718395ad5605`, 2026-09-14) | 2026-09-21 |
| Flecs | `9e874bca2c05b60612cf5b671988adefc0785820` | 2026-09-12 |
| Quinn | `f2b3d32c059122e72370b55e091801d19aed676c` | 2026-09-30 |
| Godot | `2490bf30ec229ef3eeda24befb9d80d2226c8d29` | 2026-09-30 |
| Jolt Physics | `5830c342b90fa087f118aa0f086d541a4950b1d9` | 2026-09-28 |
| FoundationDB | `462c6832baad86efcfb395c4aa795139367a0f53` | 2026-09-30 |
| Proptest | `0d1cc6623fcff0f127a95000a93f277d6867ca05` (proptest 1.11.0) | 2026-09-26 |
| PettingZoo | `9a5287db8ece6e5b7dbfeb081784ca585a14f266` | 2026-09-28 |
| Tracy | `6dae06535c0c8af1e5ff7604a9cd29145fa8258c` | 2026-09-29 |
| cargo-nextest | `48d1d5d35cc5ad623cd86c7981a873d2dd71b89e` (0.9.146 installed and measured) | 2026-09-30 |
| sccache | `54b6f72a5d3583e8ccb8b83d44e5d41400d8039c` | 2026-09-28 |

---

## 1. raylib: API discoverability and example-driven design

1. **Studied.** `examples/` (222 examples), `examples/examples_list.txt`, `examples_template.c`, `tools/rlparser/`,
   `tools/rexm/`, `src/raylib.h`, `.github/workflows/parse_api.yml` and `build_examples_linux.yml`.
2. **Revision.** `6ecf21f7` (above).
3. **Idea.** A flat, sectioned header with a one-line comment per function; a parser that emits a machine-readable API
   (`raylib_api.json`) checked in and regenerated in CI (drift fails the build); examples named
   `<category>_<topic>`, each self-contained and under 300 lines, listed in one registry file, all compiled in CI.
   Missing on purpose: no generated link from an API function to its example (raylib users grep).
4. **BlueEngine today.** `python tools/be2.py context QUERY` returns a bounded packet (feature id, `read_first`
   files, contract, tests); `tools/FEATURES.json` indexes features; `docs/CUSTOM_SIM_CHEATSHEET.md` (101 lines) lists
   the game-facing custom-simulation API; `docs/GAME_QUICKSTART.md` covers declarative games; `examples/` has
   `minimal_game.rs` and others.
5. **Does BlueEngine have the problem?** Mostly no. Twelve capabilities an AI-built game commonly needs were each tried
   with two or three natural phrasings and checked for whether a returned `read_first` file contains the API:
   11 of 12 resolve (create player, spawn object, apply impulse, detect interaction, create objective, switch camera,
   add weapon, start a match, reconnect, save/load, query nearby). **Gap:** "create checkpoint" returns `save_state`
   and `closed_path`, never the declarative answer (a counter plus an `on_enter` trigger zone).
6. **Evidence.** The audit above (a short script over `be2.py context`). Cost that remains: `context` answers with a
   feature and 3 files (about 1.3 KB), and the file holding `Controller::apply_impulse` is 1,291 lines. The
   capability-to-*signature* step is still a read.
7. **Smallest adaptation.** Add the missing checkpoint entry to the feature index. A "capability card" list
   (`capability | signature | example | test`) verified by a drift test, in raylib's `parse_api` style, would cut the
   read but costs maintenance; do it only if the audit's gap grows.
8. **Complexity cost.** Index entry: trivial. Card list plus drift test: small, ongoing.
9. **AI benefit.** Index entry: fixes one of twelve lookups. Cards: replaces a 1,300-line read with a 300-byte answer.
10. **Runtime benefit.** None.
11. **Recommendation: ALREADY SOLVED** (discovery works); **KEEP IN MIND** the checkpoint entry and cards.

## 2. Lightyear: replication, prediction, interpolation and bandwidth prioritization

1. **Studied.** Lightyear's transport priority manager (`crates/transport/transport/src/packet/priority_manager.rs`),
   channels, prediction (`crates/replication/prediction/src/rollback.rs`), interpolation timeline, input buffer; and,
   because Lightyear now delegates replication to `bevy_replicon`, replicon's `src/server.rs` and
   `src/shared/replication/client_ticks.rs`.
2. **Revision.** Lightyear `320f825b`, replicon `608e24a1`.
3. **Idea.** Two stacked mechanisms. Replicon gates each entity per client with
   `send iff base_priority * (server_tick - last_acked_tick) >= 1` and `changed since last ack`: priority
   *accumulates* while a change goes unacknowledged and resets on acknowledgement, so low-priority entities always
   eventually send. Lightyear separately fills MTU-sized packets from a priority-sorted candidate list under a
   token-bucket byte budget. Freshness is ack-based (`ConfirmHistory`). Prediction keeps a history of confirmed and
   predicted values and replays the schedule on mismatch; interpolation is a separate timeline trailing remote time
   by send interval plus jitter.
4. **BlueEngine today.** Authoritative server, acknowledged bounded partial-world replication (ADR 0014): one baseline
   and one pending packet per peer, a fair rotating cursor so every changed record is eventually sent, JSON records.
   Client prediction and reconciliation exist; interest is room-based.
5. **Does BlueEngine have the problem?** **Yes, and severely once a crowd exceeds one packet.** A 1,100-byte packet
   carries about six player records (about 180 bytes of JSON each). Everything else waits, and the wait did not depend
   on distance. `examples/net_freshness_bench.rs` measures, for one watching client, how many broadcasts (50 ms) ago
   each other player was last sent.
6. **Evidence (measured).** Before any change:

   | Players | Mean | Nearest 8 | Farthest 8 |
   |---|---|---|---|
   | 8 | 1.0 | 1.0 | 1.0 |
   | 16 | 4.7 | 4.4 | 5.1 |
   | 32 | 12.9 | 11.7 | 13.5 |
   | 64 | 28.3 | 24.1 | 29.3 |
   | 128 | 60.2 | 52.3 | 61.3 |
   | 256 | 115.7 | 100.6 | 123.5 |

   A 32-player game showed the nearest players' state 0.6 s old on average; a 64-player game 1.2 s. The stock
   8-player game was unaffected, which is why it had not been noticed.
7. **Smallest adaptation (implemented).** Every changed record accumulates priority each broadcast it waits,
   `max(0.1, 1/(1+(d/10 m)^2))` per broadcast for distance `d` from the observer, and resets when it is sent. The tail
   of the candidate list is sorted by accumulated priority (stable, so ties keep the fair rotation). The owner's own
   record, the fair slot and urgent records (removals, held props) keep their precedence. Counters
   `records_sent`, `wait_sum`, `wait_max` (and `mean_wait()`) measure freshness directly.
   After:

   | Players | Mean | Nearest 8 | Farthest 8 |
   |---|---|---|---|
   | 8 | 0.5 | 0.5 | 0.5 |
   | 32 | 7.5 | **1.8** | 14.5 |
   | 64 | 15.1 | **2.8** | 21.2 |
   | 128 | 28.1 | **4.5** | 30.9 |
   | 256 | 59.5 | **9.0** | 73.5 |

   The nearest players are 6 to 12 times fresher, and the overall mean roughly halved. Starvation-freedom is
   guaranteed by the fair slot *and* by accumulation (a test asserts no player goes unsent for more than 70
   broadcasts in a 48-player crowd). Not adopted: Lightyear's token-bucket byte cap (BlueEngine's budget is the packet
   size, not a rate), channel reliability modes, and rollback.
8. **Complexity cost.** About 70 lines in `ReplicationSender::prepare`, per-sender state of one small map.
9. **AI benefit.** `mean_wait()` makes "is replication keeping up?" a number an agent can read.
10. **Runtime benefit.** Nearby state is 6 to 12 times fresher at 32+ players; nothing changes for crowds that fit one
    packet (a test asserts every record is delivered every broadcast for 4 players).
11. **Recommendation: IMPLEMENT (done).** The follow-up this section flagged, a compact binary encoding for world updates,
    was then implemented (ADR 0029, protocol 8): a walking player is 19 to 28 bytes against about 172 as JSON, a packet carries
    about 45 to 50 players instead of 6, and mean staleness falls to 0.1 broadcasts at 64 players and 2.3 at 256 (from
    28.3 and 115.7 before any change). Priority and encoding depend on each other: with the encoding but without the
    priority ordering the nearest 8 of a 140-player crowd were 26.7 broadcasts stale. CPU per broadcast did not fall
    (each packet carries about five times as many records), so the gain is delivered state, not server time.

## 3. Flecs: cached queries, change tracking and avoiding rediscovery

1. **Studied.** `src/query/cache/` (cache, iteration, change detection, matching), `src/storage/table.c` and
   `table.h` (`dirty_state`), `src/entity.c`, `src/observer.c`, `src/observable.c`.
2. **Revision.** `9e874bca`.
3. **Idea.** A cached query stores matching *tables*, not entities, and only rebuilds when a table is created or
   deleted (`match_count`). Each table carries an integer counter per column plus a structure counter, bumped on write;
   a query keeps a snapshot and `ecs_query_changed` compares counters, so an unchanged world costs O(matched tables)
   and never walks entities. Observers fire from the structural operation itself, with a per-table flag fast path.
   Granularity is the archetype table, not the entity; mutable iteration marks everything it touched dirty unless told
   not to.
4. **BlueEngine today.** A `HeadlessWorld` with a prop list, a lifecycle registry with per-prop index caches (added
   2026-09-29), cached pose and collision bounds, dirty-record replication.
5. **Does BlueEngine exhibit "unchanged world = almost no work"?** The existing `runtime_perf` example already
   measures settled, one-changed, many-changed and resettled worlds, and a recent patch optimized it. What remains:
   an idle world with 512 settled props costs 526 to 658 microseconds per tick (1 to 1.3 us per prop).
6. **Evidence (measured, `viewer::spans`).** Where the idle tick goes at 512 props: Rapier's `pipeline.step` **449 us
   (87%)**; BlueEngine's per-prop sync 3.4 us and lifecycle loop 5.2 us **(under 2% together)**; the rest of the step
   about 58 us. So the obvious Flecs-shaped fix (iterate only changed props) would save almost nothing: the retained
   information is already retained, and what scales with the world is inside Rapier.
7. **Smallest adaptation.** Skip Rapier's step entirely while it reports no active dynamic bodies. As an experiment
   (`islands.active_dynamic_bodies().is_empty()` gate) the idle step fell from 526 to 78 us at 512 props (-85%) and
   from 1,121 to 143 us at 900 props. That is about 450 us, 2.7% of a 16.7 ms tick, and only for worlds with hundreds
   of props.
8. **Complexity cost.** Low to write, but high in risk: a body can be woken from about a dozen places in
   `prop_physics.rs` (impulses, teleports, pickup servo, restores), and Rapier's own modification tracking is
   `pub(crate)`. A missed path silently freezes physics.
9. **AI benefit.** None directly.
10. **Runtime benefit.** Up to 85% of the idle step in prop-heavy worlds; negligible for typical games.
11. **Recommendation: KEEP IN MIND.** Revisit only for a game that genuinely holds hundreds of settled props, and then
    with a single "wake" choke point rather than a flag at each site.

## 4. Quinn: deterministic protocol architecture

1. **Studied.** `quinn-proto/src/tests/util.rs` (the `Pair` harness), `tests/mod.rs`, `endpoint.rs`,
   `connection/mod.rs`, `lib.rs`; the `quinn` and `quinn-udp` split by grep.
2. **Revision.** `f2b3d32c`.
3. **Idea.** `quinn-proto` is a sans-IO state machine: no sockets, no async, no clock reads. Time is an argument
   (`handle_timeout(now)`, `poll_transmit(now, ..)`), outputs are polled (`poll_transmit`, `poll_timeout`, `poll`).
   Tests use a `Pair` of endpoints with two queues and a virtual clock that jumps to the next wakeup
   (`min(poll_timeout, next delivery)`); loss is `queue.clear()`, reordering is `delay_outbound`/`finish_delay`. No test
   sleeps. The harness is about 300 lines; the protocol core is about 6,000.
4. **BlueEngine today.** `LoopNet` (in-memory datagram network with virtual time advanced per tick, delay, jitter,
   loss, reordering) already gives the transport virtual time, and the `netplay` kit's tests use it. `DedicatedServer`
   and the session registry take `now: Instant` as a parameter at most decision points.
5. **Does BlueEngine have the problem?** **Yes, in `DedicatedServer`.** Seven places in `server.rs` read the wall clock
   for timeouts, last-seen times, the reconnect reservation and the handshake rate window, so a test could not move
   time. 19 test sites sleep for real time (1 to 250 ms). The 60-second reconnect reservation could not be tested at
   all, and no test touched it.
6. **Evidence (measured).** `grep` over `server.rs`, `session.rs`, `netplay/*`, `game_session.rs`; `grep -rn sleep
   tests/`; a search of `tests/` for `recent_disconnects` found nothing.
7. **Smallest adaptation (implemented).** A `Clock` (real, or manual and shared by clones) and
   `DedicatedServer::with_clock`. The seven timeout and window reads use it; real-time pacing in `run_realtime` keeps
   the real clock. The handshake limiter now starts its window on the first handshake (a manual clock starts at a
   different instant than the real one, and "advance one second" landed a hair short). Eight tests now run in 10
   milliseconds in total: exact timeout boundaries, activity refreshing a session, duplicate frames not counting as
   activity, the reconnect reservation at 59 s and 61 s, another address being unable to claim a reserved player, and
   the handshake window. Each test was confirmed to fail when its behaviour is broken. A small existing bug was fixed on
   the way: a hard-coded "World is full (8 players)" refusal ignored a raised cap.
8. **Complexity cost.** About 40 lines plus 7 call sites.
9. **AI benefit.** Reconnect, timeout and rate-limit behaviour can be asserted by an agent without real waits or a
   second process.
10. **Runtime benefit.** None.
11. **Recommendation: IMPLEMENT (done).** **NOT USEFUL:** restructuring the server as a full sans-IO state machine. The
    clock and `LoopNet` already give the determinism that matters; the rest would be a rewrite for style.

## 5. Godot: reusable resources and scene instancing

1. **Studied.** `core/io/resource.h/.cpp`, `core/io/resource_loader.cpp`, `scene/resources/packed_scene.h/.cpp`.
2. **Revision.** `2490bf30`.
3. **Idea.** A resource is shared by default (cached by path, reference counted; `local_to_scene` or `duplicate()`
   opts out). A `PackedScene` stores a pool of deduplicated names and values and a flat node array of integer indices;
   instancing rebuilds the node tree, and a property is saved only if it differs from its default (looked up through the
   enclosing scene states). Overrides are `(node, property, value)` triples against the shared definition.
4. **BlueEngine today.** Maps are JSON documents of nodes, materials, colliders and entities. `add_prop` and recipes
   expand into ordinary components ("no live prefab links"); `add_box` creates one material per box
   (`edit-<id>`).
5. **If 100 chairs exist?** Shared in principle (mesh, material, collider definition); per-instance (transform,
   state). In BlueEngine all of it is stored per instance.
6. **Evidence (measured on `convenience-store.json`, 2.8 MB).** 163 materials, only 78 distinct by value (52% are
   duplicates); but materials are 20 KB of the file. Nodes are 1.29 MB for 2,649 nodes (about 485 bytes each), and
   about **58% of node text is fields equal to the default in 90% or more of nodes** (`motion`, `repeat`, `ease`,
   `tube`, null fields). The scene format caps maps at 4,096 nodes and **1,024 materials**; because every `add_box`
   makes its own material, a map stops accepting boxes at about 984 regardless of colour. (At the size ceiling the slowest
   analyses, `lint` and `reach`, take about 150 ms, so nothing here is a speed problem.)
7. **Smallest adaptation.** (a) Reuse an existing identical material in `add_box` instead of minting `edit-<id>`:
   removes the 1,024-material ceiling for repeated colours. (b) Skip default-valued node fields on write, Godot's delta
   rule: roughly halves map files with no semantic change.
8. **Complexity cost.** (a) about 15 lines plus checking that nothing depends on the `edit-` names. (b) `serde`
   attributes on the node type, plus checking that golden exports and content hashes do not depend on the verbose form.
9. **AI benefit.** (a) An AI building a level from many boxes stops hitting a cryptic "scene exceeds node/light/material
   limits (4096/16/1024)" at about 984 boxes. (b) Smaller diffs and less to read when a map is opened.
10. **Runtime benefit.** Smaller files parse faster, but loading is already tens of milliseconds at most; negligible.
11. **Recommendation: EXPERIMENT.** (a) is the more valuable and is a good next change. Do not build shared prefab
    definitions with override deltas: no measurement shows a problem that (a) and (b) do not fix more cheaply.

## 6. Jolt Physics: sleeping, activation and static-world efficiency

1. **Studied.** `Body/Body.h`, `BodyManager.cpp`, `BodyActivationListener.h`, `MotionProperties.h/.inl`,
   `PhysicsSystem.cpp`, `Collision/BroadPhase/BroadPhaseLayer.h`, `BroadPhaseQuadTree.cpp`.
2. **Revision.** `5830c342`.
3. **Idea.** `BodyManager` keeps a dense array of active bodies; sleeping bodies are simply not in it, so every stage
   that iterates it pays nothing for them. Activation and deactivation are pushed to the integrator through
   `BodyActivationListener`. Whole islands sleep together after a timer; deactivation zeroes velocity. Static and
   moving bodies live in separate broad-phase trees. Rapier already provides the dense active list
   (`IslandManager::active_dynamic_bodies`), sleeping and islands; it has no activation callback.
4. **BlueEngine today.** Props wrap Rapier bodies; every tick a loop over *all* props syncs position, velocity and the
   lifecycle rest timer; replication and the lifecycle registry treat sleeping props through their own tiers.
5. **Does BlueEngine do unnecessary engine-level work for sleeping bodies?** A little, but not where expected.
6. **Evidence (measured, `viewer::spans`, 512 sleeping props, idle tick 515 to 537 us).** Rapier step 449 us (87%),
   BlueEngine's per-prop sync 3.4 us, lifecycle loop 5.2 us. So the audit of "transform synchronization, replication,
   gameplay queries, collision bookkeeping, rendering preparation" finds BlueEngine's own work already cheap (under 2%),
   consistent with the 2026-09-29 patch; the cost is stepping Rapier with sleeping bodies (about 0.9 us each).
7. **Smallest adaptation.** As in section 3: skip the step while nothing is active; 526 to 78 us measured as an
   upper bound.
8. **Complexity cost / risk.** See section 3: about a dozen wake sites, silent failure mode.
9. **AI benefit.** None.
10. **Runtime benefit.** Up to -85% of an idle tick with hundreds of props; under 3% of the tick budget.
11. **Recommendation: KEEP IN MIND.** The settled-world benchmark already exists (`examples/runtime_perf.rs`); the new
    `viewer::spans` splits its cost without hand instrumentation.

## 7. FoundationDB and Proptest: deterministic failure reproduction

1. **Studied.** FoundationDB: `flow/` (`DeterministicRandom`, `Buggify.h`, `network.cpp`), `fdbrpc/sim2.cpp`,
   `fdbserver/workloads/MachineAttrition.cpp`, `tests/fast/*.toml`, `contrib/TestHarness2/.../joshua.py`.
   Proptest: `strategy/traits.rs` (`ValueTree`), `test_runner/`, `failure_persistence/file.rs`,
   `proptest-state-machine/src/{strategy,test_runner}.rs`.
2. **Revisions.** FoundationDB `462c6832`; Proptest `0d1cc662` (1.11.0).
3. **Idea.** One seed drives a single PRNG that every simulated decision draws from; the network and clock are swapped
   for simulated ones that jump virtual time; faults are injected at I/O boundaries, and `BUGGIFY` activates a random
   quarter of fault sites per run. A failure reproduces from `(test file, seed, buggify flag, git revision)`, and an
   "unseed" (next random draw) printed at the end detects nondeterminism. Proptest generates inputs from a seed, shrinks
   failures by `simplify`/`complicate`, persists failing seeds, and its state-machine crate generates and shrinks
   *sequences of actions* (deleting actions and replaying them through preconditions so a shrunk sequence never
   contains an illegal step).
4. **BlueEngine today.** Seeded, deterministic simulation with checksums (`replay-test`, `sim`, `devkit::rng`);
   `LoopNet` with seeded loss and delay; the network tests in `tests/netplay.rs` and `tests/multiplayer_transport.rs`;
   scenario files record inputs. No fault-sequence recorder and no shrinker; failures do not print a reproduction line.
5. **Does BlueEngine have the problem?** Partly. Determinism exists; what is missing is a *generator of action and
   fault sequences plus a minimiser and a reproduction line*. Wall-clock timeouts were the blocker to simulating
   reconnects and timeouts; section 4 removes that for the server.
6. **Evidence.** Two hand-written seeded comparison tests this study wrote (`replication_sizing`: fast vs exact sizing
   over thousands of packets; `parallel_broadcast`: identical output at any thread count) found and explained one
   genuine nondeterminism source (session tokens of varying digit width change packet size): the approach works, and
   each cost an afternoon's scaffolding. The `proptest` dependency (with `proptest-state-machine`) adds 32 crates
   (a clean dev build about 9 s with crates cached, from the study's probe).
7. **Smallest adaptation.** About 100 lines: a seeded generator of valid actions (`join`, `pickup`, `drop packets`,
   `reconnect`, `advance time`) over `DedicatedServer` + `LoopNet` + the manual clock, an invariant check after each
   step, a greedy delta-debugging shrinker that deletes chunks and keeps any deletion that still fails and still
   replays legally, and a one-line reproduction (`seed`, revision, content hash, action list). Not `proptest`.
8. **Complexity cost.** Small for the harness; the invariants are the real work.
9. **AI benefit.** High for multiplayer correctness: an agent gets a minimal failing sequence, not a flaky failure.
10. **Runtime benefit.** None.
11. **Recommendation: EXPERIMENT**, building on the clock from section 4. **NOT USEFUL:** FoundationDB's simulator and the
    `proptest` dependency for this purpose.

## 8. PettingZoo: an AI-playable game interface

1. **Studied.** `pettingzoo/utils/env.py` (`ParallelEnv`, `AECEnv`), `utils/conversions.py`, `test/api_test.py`,
   `parallel_test.py`, `seed_test.py`, `state_test.py`, one example environment.
2. **Revision.** `9a5287db`.
3. **Idea.** A tiny contract: `reset(seed) -> (obs, info)`, `step(actions) -> (obs, rewards, terminations, truncations,
   infos)` keyed by agent, `agents` / `possible_agents`, per-agent spaces, optional `state()`. Termination (game ended)
   and truncation (cap hit) are separate booleans. An `api_test` validator checks key-set equality, spaces containing
   observations and seed determinism. Observation versus privileged state is *encouraged* (`state()` is for centralised
   training) but not enforced.
4. **BlueEngine today.** `HeadlessWorld::step`, `snapshot_for_player` (an interest-filtered view, which is exactly
   an observation), scenario files run by the `sim` command (open-loop, scripted), bots in individual games. No closed-loop
   `reset`/`step`/observation interface.
5. **Does BlueEngine have the problem?** There is no way for an agent to *react* to a game through a stable
   interface; scenarios cannot. This is a capability gap, not a performance problem.
6. **Evidence.** Search for `reset`/`observe`/`reward`/`terminated` in `src/viewer` finds none (the hits are
   `game_explore`'s unrelated `truncated`).
7. **Smallest adaptation (sketched, not built).** A `HeadlessEnv` over `HeadlessWorld`: `reset(seed) -> Observation`;
   `step(actions) -> StepOut { observation, reward, terminated, truncated }`; `Observation` is the player's
   `snapshot_for_player` plus their own controller, a *different type* from a separate privileged `WorldState` so the
   compiler, not a test, keeps hidden information hidden; reward derived from `complete`/`fail` for declarative games,
   supplied by the game otherwise; seed explicit and required; a `validate` function mirroring `api_test`. It reuses the
   real simulation, never a second rule implementation.
8. **Complexity cost.** About 250 lines plus tests.
9. **AI benefit.** High for bots, automated playtesting and learning agents.
10. **Runtime benefit.** None.
11. **Recommendation: EXPERIMENT.** Not built here: the observation, action and reward conventions should be defined
    with the first real consumer (the "Gauntlet" use case) so the interface is shaped by use, not guessed.

## 9. Tracy: profiler visibility

1. **Studied.** `public/tracy/Tracy.hpp`, `public/client/TracyScoped.hpp`, `csvexport/src/csvexport.cpp`,
   `capture/src/capture.cpp`, `manual/tracy.tex` (overhead, on-demand, plots, worker API).
2. **Revision.** `6dae0653`.
3. **Idea.** `ZoneScoped` spans with static source locations, `FrameMark`, `TracyPlot`; every macro compiles to nothing
   when disabled; enabled cost is about 2.25 ns per zone via a lock-free thread-local queue and `rdtsc`. A separate
   `capture` + `csvexport` pipeline turns a recording into `name, total, count, mean, min, max` rows. No Rust binding
   is in the repo (a third-party crate wraps the C API).
4. **BlueEngine today.** `viewer::metrics` has a tick-time mean and budgets (`PerformanceBudget`,
   `inspect-performance`), and `HeadlessWorld::last_physics_time_us` for the physics section. No per-phase breakdown.
5. **Does BlueEngine have the problem?** Yes, demonstrably: in one working day the same phase breakdown was hand-built
   three times (replication phases, the idle-tick split, the prepare sections) by temporarily editing engine code.
6. **Evidence (measured).** Span cost: 3.3 ns disabled and 58 ns enabled (20 million iterations). A
   live 56-client server with `--profile` reported, without any editing: per-broadcast preparation 3.3 ms
   (`server.broadcast.stage`, 183 us per peer in `replication.stage`), receiving 0.5 ms per tick (`server.poll`), the
   whole world step only 0.4 ms (`world.step`), Rapier 78 us per sub-step.
7. **Smallest adaptation (implemented).** `viewer::spans`: `span("name")` RAII guard, `summary()` returning
   `name count total mean max` biggest first (text or JSON), off by default. Every thread registers a table that
   `summary` reads, including finished threads (see below); ten spans are placed on the phases of a world step, the
   Rapier/sync split, and the server's poll, step and the two broadcast stages. `be2-headless --profile` prints the
   summary with each 5-second status line and resets it. The first design merged per-thread tables in thread-local
   destructors; a test showed `std::thread::scope` can return before those run (75 of 100 worker spans counted), which
   is exactly the path the parallel broadcast uses, so the design registers tables instead. Stressed 60 times with no
   failures. Not adopted: timelines, plots, GPU zones, a capture protocol.
8. **Complexity cost.** About 200 lines, one new module, ten call sites.
9. **AI benefit.** One small table replaces ad-hoc instrumentation; it is also assertable in a test.
10. **Runtime benefit.** None directly; about 0.4 us per tick when on (0.1%), about 30 ns when off.
11. **Recommendation: IMPLEMENT (done).** **NOT USEFUL:** adopting Tracy itself; its strengths (timelines, GUI) are not
    what an agent consumes.

## 10. cargo-nextest and sccache: development efficiency

1. **Studied.** nextest: `site/src/docs/design/*`, `configuration/*`, `features/*`, `ci-features/*`,
   `benchmarks/index.md`. sccache: `docs/Rust.md`, `Caching.md`, `Local.md`, `GHA.md`, `Configuration.md`,
   `src/compiler/rust.rs`. nextest 0.9.146 was also installed and run.
2. **Revisions.** nextest `48d1d5d3`; sccache `54b6f72a`.
3. **Idea.** nextest runs each test in its own process and schedules tests globally across binaries, where `cargo
   test` runs binaries one after another. sccache caches rustc invocations by content and arguments; it returns "not
   cacheable" for incremental compilation, binaries and proc-macros.
4. **BlueEngine today.** `cargo test --profile itest` (opt-level 1 dev); 33 test binaries; sccache and mold already in
   use locally (recorded in `docs/perf`: cold test build 118 s to 48 s with a warm cache); CI builds everything cold.
5. **Does BlueEngine exhibit the problem?** Test time: yes. `cargo test` used 128 CPU-seconds in 61.6 s on 4 cores
   (52% utilisation): half the machine is idle while binaries run in sequence.
6. **Evidence (measured).** Warm build, same tree, 4 cores:

   | | Wall | Notes |
   |---|---|---|
   | `cargo test` | 71.7 s | 638 tests (incl. 26 doc tests) |
   | `cargo nextest run` | **48.2 s** | 612 tests, no doc tests |
   | `cargo nextest run -j3` | 57.3 s | leaves a core free |
   | `cargo test --doc` | 3.8 s | what nextest omits |

   So nextest plus doc tests is about 52 s against 71.7 s: **28% faster, about 20 s per full run**. The project's gate
   runs tests in two feature modes. sccache: its own documentation gives no numbers; locally it is already adopted; its
   value for CI would be the unmeasured part (CI compiles every dependency from scratch on two operating systems, runs
   of 17 to 35 minutes).
7. **Smallest adaptation.** `be2.py check` uses nextest when installed and `cargo test` otherwise (`cargo test --doc`
   alongside). For CI, test `mozilla-actions/sccache-action` with `CARGO_INCREMENTAL=0` and compare a cold and a warm run.
   Do not turn off incremental compilation locally for sccache.
8. **Complexity cost.** nextest: one optional tool (about 10 minutes to compile the first time on this machine).
   sccache in CI: a workflow change.
9. **AI benefit.** Faster iteration per check; nextest's per-test output is also easier to read.
10. **Runtime benefit.** None.
11. **Recommendation: nextest EXPERIMENT** (measured gain, optional adoption). **sccache: ALREADY SOLVED locally;
    EXPERIMENT for CI.**

---

## What was not done, and why

* No framework or dependency was added.
* Skipping the idle Rapier step, material dedupe, default-field skipping, a compact record encoding, the fault harness
  and the agent interface are recorded above with evidence and cost; none was both clearly worth it and safe enough to
  do alongside the three changes.
* The external projects were read, not run (except nextest), so statements about their behaviour are from their source and
  documentation.

## Reproducing the measurements

```sh
cargo run --profile fast --example net_freshness_bench        # section 2
cargo run --profile fast --example net_broadcast_bench -- 512 # ADR 0027 (broadcast cost)
be2-headless --server 127.0.0.1:4000 --realtime --profile     # sections 3, 6 and 9: where a tick goes
cargo test --profile itest --test virtual_time                # section 4
cargo nextest run --cargo-profile itest                       # section 10 (after cargo install cargo-nextest)
```
