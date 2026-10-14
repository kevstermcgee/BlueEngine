# Versioned custom-game foundation

This pass grows **custom Rust simulation** through a small external automation
workshop. It does not add production systems to stock GameDocument or portable
planar worlds. Run [WORKSHOP.md](WORKSHOP.md) for the public contracts and example.

## Starting point and reconciliation

The actual fetched `origin/main` was `cb19b58e5c952ba83f120d801a85d929307ed19f`.
The existing checkout was branch `couch-games`, revision `76743a0`, with unrelated
learning, performance, pixel-art and game-icon edits. It was preserved. Following
the user's selected base, this isolated `milestone/versioned-workshop` worktree
starts at authoring follow-up `0f5a3e2ef088161fe51cef330c06149dc6a91817`; it does not
merge that pending work into main. Prescribed start/context ran before targeted
navigation (`faff587aee4e`). REPORT and ADR 0051 describe earlier inputs/proposals.

The selected base already centralizes pinned project tooling with thin shims,
optional physics/networking/device dependencies, bounded discovery and ownership
checks. These are reused, not reimplemented. Pending custom replication `1807420`
and creative-identity `41676f8` were inspected as separate work and not merged.
Existing kit, devkit Lifecycle, fixed-step Simulation/StateHasher, Snapshot,
atomic SaveSlots, public NetGame/ClientView and cli::serve own infrastructure.

Baseline evidence at `0f5a3e2`: required 45-stage Linux gate passed, 556.259 seconds
wall / 678.743 seconds summed stages, plus Linux development packages and software
captures for offline Pocket Breaker and stock Signal Garden. That evidence is
scoped to that revision; it is not current main CI or Windows device evidence.

## One bounded change

The only new reusable mechanism is optional `runtime::resources::Inventory`:
bounded deterministic integer debit/credit transactions, validated deserialization
and sorted receipts. It has no recipes, clock, presentation or network policy.
The separate timed ability-cost/refund/cooldown fixture reuses it. No dependencies
were added; existing default capability selection is preserved.

`games/automation-workshop` owns its compiled definitions, production escrow,
commands, upgrades, deterministic machine ordering, inspection, presentation and
explicit compatibility policy. Definition IDs, instance IDs and display labels
are distinct. Game ID, release, state schema, content fingerprint and online wire
version are separate. Content edits do not require engine changes or Rust rebuilds.
Completed outputs and cancellation refunds remain pending when the store is full.

Migration uses the engine frame and a private legacy Snapshot adapter around the
same authority to validate the genuine v1 hash. It preserves frozen in-flight
rules, imports into current definitions, validates, writes a new engine slot and
only then activates. Originals and existing destination slots are preserved on
failure. Normal exact-content checks remain strict. Append-only v2 recipe/upgrade
additions over existing items have an explicit import; new items, retuning or
removal need another declared migration.

The online fixture/server uses the same rules and the existing custom netplay
adapter. Tests execute real local UDP and simulated loss/interruption. Full state
must fit the active transport's ceiling, at most 1,100 bytes. Two idle machines
measured 136 bytes; ten idle machines measured 336; 100 idle machines measured
2,586 and exceeded the budget. Real blocked work is larger: ten paid smelt jobs
measured 1,538 bytes and already exceeded it. These are encoder probes; the actual
online fixture starts with two machines.
These are probe sizes, not demonstrated player/world capacity. Local accepted
traffic is not remote delivery. Match timeout reclaim and late join are rejected.
The offline native console is not an online UI demonstration.

## Dependency-ordered roadmap

1. **Authoring/tooling prerequisites:** retain the selected pending shared-tooling
   and optional-dependency pass. Fixture: generated and migrated tiny game,
   intentional hook preservation, isolated resolved graphs, cold/warm receipts.
2. **This milestone:** atomic resources and a versioned custom workshop. Fixture:
   two machines/full store/cancel/recovery, real old save migration, independent
   ability cost, public-command tests and 10/100/1,000 probes.
3. **Required next release: player updates.** Depends on independent game/save/
   content/protocol identity and explicit migration. Inspected developer source
   upgrades (`tools/upgrade.py`, ADR 0032), operator server replacement
   (`tools/hub_deploy.py`, `deploy/hub/update.sh`) and installer/reinstall tooling
   (`templates/game_installer.py`) provide no reusable authenticated player-update
   contract. Companion export/publication (`scripts/publish_games.py`,
   `.github/workflows/publish-games.yml`) delivers releases; it is not an installed
   player's authenticated update service. Add one reusable service with game-owned UI for notes, download
   progress/cancellation, bandwidth limits and the player's safe application point.
   Use a maintained secure-update implementation with pinned trust roots,
   authenticated fresh metadata, hashes/sizes, rollback protection and key rotation.
   Stage immutable versions, apply after exit via a minimal bootstrap, keep saves
   outside releases, back up migrations and health-check before activation. Retain
   only a vetted save-compatible rollback; never silently give a newer save to an
   older binary or discard post-update progress. Reject unsafe/oversized archives.
   Verification does not replace safe installation or platform code signing.
   Content hot reload needs an explicit safe contract; rules switch at session
   boundaries and binaries restart. Keep signing/publishing credentials out of games.
   Pin multiplayer executable,
   content and protocol identities; drain/restart incompatible servers. Fixture:
   actual Windows two-version install/update/relaunch plus interruption, invalid
   signature, corruption, low disk, startup failure and incompatible-save cases.
   Local test keys only; production signing/publication/deployment and installation
   on user machines require separate authorization. No updater is added here.
4. **World persistence/regions:** depends on stable IDs and coherent transactions.
   Fixture: save-before-evict, bounded dirty queues, generator-version mismatch and
   cross-region transfer recovery. Distinguish rendered, replicated, simulated,
   sleeping and unloaded states; unseen production must continue under explicit
   tested semantics. Catch-up is game policy, not an implicit rendering shortcut.
5. **Custom-game replication:** build on consistent bounded snapshots and region
   ownership. Fixture: this workshop with relevant-state budgets, baseline/delta
   recovery, late join, durable timeout reclaim, hidden-state filtering and explicit
   overload. Do not claim stock-server interest/capacity/lifecycle features here.
6. **Optional mechanics/presentation:** extract only demonstrated shared mechanics;
   fixture: two different games using inventory/technology inspection, binding and
   localization hooks, distinct controls/art/audio, state-bound interaction tests.
7. **Long-session reliability:** checkpoint hashes, bounded memory, minimized replay
   failures, fault injection and property tests across declared platform/physics
   limits. Fixture: long workshop production/save/reconnect workload. Workers start
   with immutable independent inputs and deterministic application order; parallel
   authority requires profiling and proof.

The next **game-capability bottleneck** is custom netplay's complete-snapshot and
match-lifecycle boundary; the player-update milestone is the next **release gate**.
Neither is hidden behind a new server or speculative framework.

## Evidence and limits

Machine probes run real queued/paid jobs blocked by a full destination, with 51
measured fixed steps per size. The probe reports p50/p95/p99, requested System
allocator bytes/calls, Linux RSS, engine save/load cost and optional encoded
network size. It stops if the declared 2 MiB import/probe save budget or 16.667 ms
p99 tick budget is exceeded; 1,024 machines is a constructor bound, not a capacity
promise. Allocator metadata and GPU allocations are excluded. The fixture has no
authoritative randomness. No subjective fun or commercial-scale claim is made.

Use final receipts with their recorded revision/source digest. Warm and changed
checks are distinct from cold setup, integration and shipping. Linux Xvfb/software
captures prove packaged launch/rendering under that environment, not physical
audio, input, GPU or Windows runtime. Missing measurements are unavailable.

Whole-task authoring source/doc bytes are a reading proxy. Actual model-token
usage is reported only if the fresh Codex JSON stream provides valid usage; a
scripted modification test is separate from a genuine model completion. The
complete initial instructions and selected guides are bounded by discovery tests.

Main merges trigger the companion game publisher. Source push/stacked review is
permitted, but merging to main would publish games without the mission's separate
publication authorization. This milestone therefore remains reviewable on its
isolated source branch.

## Exact evidence ledger

Starting revision: `0f5a3e2ef088161fe51cef330c06149dc6a91817`. This ledger records
completed observations and planned final acceptance separately. Final clean-revision
receipts, exact commands, source identity, wall/summed-stage times and platform
results will be supplied in the PR/CI and local `.be2-work` evidence. This report
commit does not certify a currently running check or a later source revision.

| Check | Executed input scope | State/result |
| --- | --- | --- |
| Starting required Linux gate | Clean `0f5a3e2`; 45 stages | Executed/passed: 556.259 s wall, 678.743 s summed stages |
| Transactions, ability reuse, offline/online gameplay and migration | Recorded intermediate inputs: 10 resource tests/8,125 exhaustive cases; `1c310b8` plus working inputs: 25 offline / 32 online-feature tests | Executed/passed; focused game checks 14.553 / 4.223 s; not final-revision acceptance |
| Three 10/100/1,000-machine load ladders | `1c310b8` plus recorded inputs; paid blocked jobs, `itest` | Executed/passed declared offline budgets; not online capacity |
| Isolated dependency graphs | Clean `0f5a3e2` versus `d7314ec` recorded manifests/lock/source hashes | Executed/passed graph guards; graph resolution alone executes no fixture |
| Native MCP discovery | `d7314ec` recorded inputs; fresh launcher from `/tmp`, five queries | Executed/passed; [actual answers](perf/workshop/discovery-answers.json) |
| Native describe, capacity and retrieval floors | Budget-fix code committed as `7663498`; `physics,networking`, `itest` | Executed/passed 9 focused tests and four unchanged discovery datasets; 6,027 → 5,974 UTF-8 bytes, all signatures/feature names retained |
| Genuine model recipe/upgrade task | Frozen trial plus disclosed controller baseline synchronization | Executed/passed 25 offline tests; assisted baseline, not an untouched-base trial |
| Genuine content-only edit | Trial variant at `9edea6b`; current-content tests | Executed/passed 25 tests; zero rebuilt Cargo artifacts |
| Linux native package/input/save | `d7314ec` working inputs, matching package/content digests | Executed/passed isolated software launch, captures, synthetic X11 input and negative upgrade-cost check; shipping 33.588 s |
| Required Linux preflight, including external fixtures | 51-stage run began at `7663498`; game formatting, report and CI inputs subsequently changed | Executed/passed: 1,454.241 s wall, 1,586.535 s summed stages, 4,669 test executions; 633 built/3,156 fresh artifacts; not immutable final-revision evidence |
| Final required Linux gate, external fixtures and package | Final clean revision required after reporting/platform fixes | Planned; final receipts supplied in PR/CI and local `.be2-work` |
| Windows native package | Exact `945f21d`; six declared files, executable resources and two software captures | Executed/passed; `--no-install`; separate installer build was not part of this receipt |
| Final Windows engine/native/installer CI | Exact final source revision required; existing installer builder added to the required workshop job in `25f1508` | Planned/unverified until matching CI receipt; Linux does not establish Windows runtime |
| Desktop installation/shortcuts | Package command used `--no-install` | Skipped intentionally; package evidence retained |
| Cold setup; physical GPU/input/audio; internet/long sessions | Existing caches; no matching device/session experiments | Unverified, not zero; no crash-free or commercial-capacity claim |
| Secure two-version player update | Required next release milestone | Planned/deferred; no updater, credentials, publication or deployment added here |

Failed evidence remains retained. Full attempts at `69df8ad` failed engine
formatting and at `0906de7` failed describe's existing 6,000-byte assertion. A clean
`--changed` invocation without a base selected no work and is excluded from
completion evidence. Linux CI at `0906de7` rejected game `build.rs`
formatting; `c1904f3` still rejected determinism-test formatting, corrected in
`d429129`. Their failures do not become passes retroactively. Other recorded
failures include compile/API mismatches, discovery recall (floor preserved),
fixture ID/timestamp assumptions, missing packaged assets and an inverted camera
subsequently corrected and visually rechecked. The X11 harness initially focused
an unmapped window before repair. Default ALSA reported `Can't set rate` on both
native launches; the same package passed with a private plug-to-null sink. That
repeat isolates an environment failure; it is not an engine audio fix or audible
hardware evidence. See [original](perf/workshop/native-synthetic-input.json) and
[null-sink](perf/workshop/native-synthetic-input-plug-null.json) receipts.

Windows engine CI at `945f21d` exposed 15 UTF-8 decoding errors and nine short/long
path-alias assertions in test fixtures. The narrow correction uses explicit UTF-8
source/JSON reads and compares canonical paths, retaining all assertions. The
three focused Linux Python suites passed 56 tests; actual Windows re-execution
remains a required final gate. This changes test portability, not game behavior.

[Measurements](perf/workshop/measurements.json) retain exact commands, configurations,
intermediate scopes and failures; [graphs](perf/workshop/dependency-graphs.json)
retain manifests/locks/source hashes. Counts are distinct package names, not
compiled artifacts. Existing selected graphs are unchanged; new resources select
17 names, equal to pure rules, without physics, QUIC or devices. Inventory did not
exist at baseline: this is not equivalent-workload performance evidence.

The Intel N97 Linux host has four cores and 16 GiB installed RAM; rustc 1.98.1,
shared warm targets, two Cargo jobs, disabled incremental/cache wrapper, `itest`
assertions and allocation instrumentation apply. Across three 1,000-machine runs,
per-run p50 was 8.151–8.281 ms, p95 8.265–11.112 ms and p99 8.445–11.487 ms;
each run has 51 samples, so p99 is its maximum. Saves were 287,674 bytes,
save/load 37.976–38.307 / 20.299–20.643 ms; setup 8.189–8.262 s. Over 51 ticks,
requested allocations totalled 123,114,816 bytes/1,020,001 calls, peak requested
live bytes 754,733 and Linux RSS 11,784,192–11,829,248 bytes. Metadata/GPU memory
are excluded; these are not controlled long-session tails. Blocked ten-machine
packets already exceed the 1,100-byte online ceiling.

The earlier six-test netplay receipt (`d7e4188` plus recorded working inputs)
measured 49 actual drops/662 sent packets under configured 8% LoopNet loss,
two-tick latency and 0–1-tick jitter; 246 locally accepted snapshots/34,302 bytes
over 4.100 simulated authority seconds, or 8,366.3 locally accepted bytes/s across
two clients. A one-second polling interruption recovered full state; timeout
reconnect and new-peer late join were rejected. These are local acceptance and
fixture results, not delivery acknowledgements or general internet bandwidth.
Final network tests/results require their own matching clean-input receipt.

Warm no-change tests took 0.525/0.515 s with tests executing and no rebuilds.
A genuine CLI/identity source edit took 2.776 s; a separate authority fix/test
14.553 s. One genuine content edit took 0.107 s, then current-content tests
0.513 s with no rebuilds. Workloads differ; cold setup, universal speedups and
billed-cost savings are unmeasured. Integration and shipping are separate gates;
final wall/summed-stage totals and final package results must come from final
receipts, not inherited baseline/intermediate passes.

The [model trial](perf/workshop/authoring-trial.json) reports 718.942 s wall,
53 tool items, four failed verification attempts and 20.657 s across 20 instrumented
verification/metadata commands. Its 22 explicit reads exposed 129,895 UTF-8 bytes;
11,917 automatic-instruction bytes may overlap and are not added as unique reads.
Actual usage: 1,238,498 input tokens, including 1,159,680 cached, plus 22,130 output
tokens. Cached input is a subset. Read/command audits found no engine implementation
inspection. The model repaired game-test assumptions after controller base updates;
one assisted completion with no comparison cohort proves neither lower AI cost nor
completion probability. Source bytes are reading proxies, not model tokens.
