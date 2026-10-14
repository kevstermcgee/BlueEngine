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
failure. Normal exact-content checks remain strict. Append-only v2 additions have
an explicit import; retuning/removal needs another declared migration.

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
   content/protocol identity and explicit migration. Audit found developer source
   upgrades and operator server replacement, but no reusable authenticated player
   updater. Use a maintained secure-update implementation with pinned trust roots,
   authenticated fresh metadata, hashes/sizes, rollback protection and key rotation.
   Stage immutable versions, apply after exit via a minimal bootstrap, keep saves
   outside releases, back up migrations, health-check before activation and retain
   only a vetted save-compatible rollback. Fixture: actual Windows two-version
   install/update/relaunch plus interruption, invalid signature, corruption, unsafe
   archives, low disk, startup failure and incompatible-save cases. Local test keys
   only; production publication/signing/deployment requires separate authorization.
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
