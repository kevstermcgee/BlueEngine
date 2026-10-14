# Working on Automation Workshop

Read this file and `docs/WORKSHOP.md` in the pinned engine checkout. Begin with
`python3 tools/be2.py start "TASK" --kind
change-game --project games/automation-workshop --compact`; use an absolute project
path when invoking it from another directory.

This is an external custom Rust simulation. `src/lib.rs` owns fixed 60 Hz commands,
queues, escrow, recipes, upgrades and engine-owned Snapshot persistence. `src/main.rs`
owns the native console, uncaptured cursor, input, camera and silent presentation.
`src/net.rs` adapts the same rules to existing NetGame/ClientView; the server uses
netplay::cli::serve. Do not move game balance or content into engine internals.

Edit `assets/workshop.json` for recipes/upgrades/tuning. Stable namespaced IDs are
separate from labels and machine instance IDs. Validate and index content once;
never parse it every tick. Duplicate IDs/unresolved references must fail with source
locations. Keep frozen `assets/content-v1.json` and released migrations immutable.
Use semantic content-edit for copy/validate/commit edits.

Inventory mutation crosses `runtime::resources::Inventory::transact`. Queueing debits
into saved escrow; completion/refund clears it only after a successful full commit.
A blocked destination or cancellation retains all work and reports its cause.
Render callbacks may submit player commands; they never mutate authoritative state.
Test public inputs and actual outcomes; privileged debug mutation cannot certify play.

From the engine root:

```text
cargo run --locked --manifest-path games/automation-workshop/Cargo.toml --no-default-features --bin workshop -- validate
cargo test --locked --manifest-path games/automation-workshop/Cargo.toml --no-default-features
python tools/be2.py check --game games/automation-workshop --loop inner
```

F5/F9 and script save/load use Lifecycle/Snapshot; never create an ad hoc save format.
Per-user saves use blueengine-workshop; explicit --save-dir wins.
Schema/content migration must operate on a copy, preserve progress and retain originals
on failure. Resume command serials after load. Keep exact-content checks.

Native controls: 1/2 machine, Left/Right recipe, Space queue, Backspace cancel,
Tab next upgrade, U apply, O/C gather ore/coal, D deliver plate, Down next inventory
item, mouse select/buttons, F5/F9 saves, Esc menu. Capture --script uses the same
player-command path: smelt, gear, ore, coal, full, deliver, recovery, cancel, upgrade,
queue, machine, recipe, upgrade_next, item. save/load are engine cues. Silence is intentional; custom audio uses AudioBank/SoundBank.

Iteration does not certify shipping. Keep shared pinned tooling shims; package
with scripts/ship.py ship --no-install. Windows x64 installer/native
launch and physical controls/audio require their actual platform/device gates.
Record friction with tools/learn.py and verify retrieval; token telemetry may be
unavailable. Never publish or deploy from this fixture task.
