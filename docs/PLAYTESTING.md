# Bounded authoritative playtesting

`tools/playtest.py` runs game-owned adapters around the real rendering-free
single-player authorities. Workshop drives `Workshop::step` with public numbered
commands; Afterpull drives `Simulation::step` with public `Intent` actions.
The shared utility owns processes, budgets, source/binary identity, retained
checkpoints, replay and serial/parallel comparison. Rules and invariants stay in
the games; neither adapter implements a second production or movement resolver.

Build the source being tested first; never reuse a binary because its name matches:

```sh
cargo build --locked --release --manifest-path games/automation-workshop/Cargo.toml --no-default-features --bin workshop
cargo build --locked --release --manifest-path games/afterpull/Cargo.toml --no-default-features --bin afterpull-playtest
python3 tools/playtest.py --binary /PATH/TO/TARGET/release/workshop --compare-serial --out /tmp/new-workshop-playtest
python3 tools/playtest.py --binary /PATH/TO/TARGET/release/afterpull-playtest --compare-serial --out /tmp/new-afterpull-playtest
```

Cargo prints the selected target path; shared `CARGO_TARGET_DIR` applies. Defaults:
8 seeds, 10,000 ticks each, concurrency 2 (maximum 4), per-process address space
128 MiB, 30 seconds per run and 120 seconds total. `--ticks`, `--seeds`,
`--concurrency`, `--memory-mib`, `--seconds`, `--total-seconds` configure bounds.
Address-space enforcement currently requires Linux. These are process limits,
not whole-machine memory accounting. Budget exhaustion is a failed run. Use a new
`--out`; old evidence is preserved. The serial comparison shares the total budget
and compares hashes, actions/trace identity, failures and game-owned coverage.

## Workshop

Each run starts with a public-action production/recovery route: paid escrow,
full destination, blocked cancellation refund, delivery freeing capacity,
wire-tools upgrade, wire production and delivery. Seeded actions use the real
command validator on an isolated candidate. Deliberate duplicate envelopes must
apply once. An independent accounting vector includes inventory, finite sources,
deliveries and escrow. Only completed recipes and paid upgrades may change it.

The direct progression oracle follows the documented scheduler: each step visits
one head job per machine in stable instance-ID order. A noncancelled job loses
one remaining tick, and a ready job (including a previously completed head) must
leave the queue when its complete output fits. A pending cancellation must leave
when its entire escrow refund fits. A successor waits until the next step.
Transaction preview determines fit against the command-boundary inventory and
actual earlier commits, using the production transaction validator. Blocked work
must retain its paid escrow and report the exact final-inventory blockage cause.
Delivery must release fitting work; blocked work is not exempt from progress once
capacity becomes available. This does not require an arbitrarily idle player to win.

Longer runs alternate repeated blockage/recovery routes every 512 ticks. Snapshot
save/restore occurs every 31 turns, and compatible label-only content migration
every 997 turns. Migrated progress, previously paid work, original save bytes and
existing destination slots are checked; exact restore still rejects different
content. Subsequent state hashes are compared with uninterrupted authority.
Original and migrated framed saves remain in evidence. Test-only stall-completion
and stall-refund fixtures reverse a real commit while preserving accounting and
prove that the direct oracle rejects it. The CLI `--fault-fixture` retains the
separate duplicate-command detector fixture.

## Afterpull

Afterpull has different rules: simultaneous departure pulls, shielding, jams,
pits and six rooms. Its adapter checks crate/player occupancy and count, dock/pit
status, moves against the authority's immutable preview, exact whole-turn undo
including counters/history, retry, clear-room transitions and final completion.
Routes use the public solution/preview/movement/undo/retry/advance commands,
including actual pit loss, movement while terminal and recovery. Seeded route
variation is followed by repeated final-room failure/undo/retry/completion cycles.
Snapshot reloads every 31 turns must agree with uninterrupted authority.

`--fault-fixture` deliberately leaves an undo counter unrestored. The whole-turn
oracle must fail and replay the same failure. This is a detector self-test, not a
naturally discovered gameplay defect. Afterpull has no released content-migration
contract to exercise; Workshop supplies that coverage.

## Evidence and replay

`inputs.json` records the commit, tracked diff hash, complete source identity
(including new files), binary hash, seeds and budgets. Each seed directory records
its command before launching, stderr, full action/checkpoint trace and result.
Timeout/OOM retains the seed command and captured output. Checkpoint directories
retain original `.be2save` bytes and migrated bytes when applicable. Results are
sorted by seed regardless of completion order. `comparison.json` retains serial
results. Keep the original binary/source and `inputs.json`; traces do not restore
source or certify that an arbitrary binary was freshly built.

Replay through the same process limits and into a fresh evidence directory:

```sh
python3 tools/playtest.py --binary /PATH/TO/ADAPTER --replay-trace /tmp/prior-playtest/7/trace.json --out /tmp/new-replay
```

The adapters also accept `replay-playtest TRACE.json` directly. Traces pin game
content; excessive files, turns and commands are rejected. For either CLI detector
self-test, use `--fault-fixture --seeds 7 --ticks 100`: failure is expected, and the
retained trace must reproduce it. Fault options belong only to developer adapters.
Exact replay is supported; automatic trace reduction is deferred.

This establishes bounded behavioral evidence. It does not certify rendering,
networking, controller feel, visual clarity, physical input, audio backend health,
audible output, enjoyment, all action sequences or untested capacities. Canonical
engine tests include both adapters' invariant tests and shared-runner tests;
native presentation/package and platform/device reviews remain separate gates.
