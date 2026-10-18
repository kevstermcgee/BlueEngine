# Bounded headless workshop playtesting

This development utility runs the real single-player `Workshop::step` authority in
isolated processes. It uses no renderer, physics, network or narrative runtime.
Build the source being tested first; never reuse a binary just because its name matches:

```sh
cargo build --locked --release --manifest-path games/automation-workshop/Cargo.toml --no-default-features --bin workshop
python3 tools/playtest.py --binary /PATH/TO/TARGET/release/workshop --out /tmp/new-playtest
```

Cargo prints the selected target path; shared `CARGO_TARGET_DIR` applies. Defaults:
8 seeds, 10,000 ticks each, concurrency 2 (maximum 4), per-process address space
128 MiB, 30 seconds per run and 120 seconds total. `--ticks`, `--seeds`,
`--concurrency`, `--memory-mib`, `--seconds`, `--total-seconds` configure bounds.
Address-space enforcement currently requires Linux. These are conservative
process limits, not whole-machine memory accounting. Budget exhaustion is a failed
run, never a gameplay pass. Use a new `--out`; old evidence is preserved.

Each run starts with a public-action production/recovery route: paid escrow,
full output destination, blocked cancellation refund, delivery freeing capacity,
wire-tools upgrade, wire production and delivery. Then seeded actions are chosen
by the real command validator on an isolated candidate. Every accepted action
uses numbered player envelopes; deliberate duplicate envelopes apply once.
An independent accounting vector includes inventory, finite sources, delivered
items and all escrow. Only completed recipes and paid upgrades may change it.
An active job with time remaining must progress unless cancellation is pending;
blocked output/refunds are deliberately exempt from a false progression-stall alarm.
No claim is made that arbitrary waiting players must finish a game.

Checkpoints use engine-owned Snapshot save/restore every 31 turns. The resumed
run is compared with uninterrupted authority on subsequent turns. Every turn also
checks state validity and duplicate-command equivalence. These are accounting and
continuation oracles over gameplay, not a second simulation.

`inputs.json` records revision, tracked working diff hash, complete source identity (including new files), binary hash, seeds and
budgets. Each seed directory records its command before launching, stderr, a full
action/checkpoint trace and result. Even timeout/OOM retains the seed command.
Results are sorted by seed regardless of worker completion order. Replay exactly:

```sh
/PATH/TO/TARGET/release/workshop replay-playtest /tmp/new-playtest/7/trace.json
```

The trace pins complete workshop content. Keep `inputs.json` and the original
binary/source with it: a trace is not a substitute for restoring the recorded
source. The CLI refuses excessive trace turns/actions. For an oracle self-test,
`--fault-fixture --seeds 7 --ticks 100` deliberately repeats a gather through the
real command path. It must fail and its recorded trace must reproduce the failure.
The fault option belongs only to this test harness, never gameplay.

This establishes bounded behavioral evidence. It does not certify controller feel,
visual/audio quality, physical devices, enjoyment, all possible action sequences,
or capacities beyond the tested workloads. Trace reduction is deferred; exact
single-run replay is available now.
