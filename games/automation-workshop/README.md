# Automation Workshop

A small native custom-Rust game: two independent machines, a shared twelve-unit
store, ore/coal sources, plates/gears, and a speed upgrade. Select a machine and
recipe, queue work, deliver items to free a full store, or cancel and recover its
escrow. All actions use the same fixed-step authority as the headless tests and
optional native network adapter.

```text
cargo run --locked --bin automation-workshop
cargo run --locked --no-default-features --bin workshop -- play
cargo run --locked --no-default-features --bin workshop -- probe
python scripts/check.py
python scripts/ship.py ship --no-install
```

Windows x64 installers are the delivery path; Linux packages/captures are development
verification. Save files default to the game's stable per-user data directory;
`--save-dir PATH` selects an explicit directory. F5/F9 save/load through the engine.

Rules/content are `assets/workshop.json`, validated and indexed at startup. Adding
a recipe/upgrade changes game content, not the engine. Read `AGENTS.md` and
`../../docs/WORKSHOP.md` for contracts, exact checks, explicit save-copy migration,
resource budgets and the current online limit. The native console is offline;
the headless online fixture/server demonstrate shared authority, brief interruption
recovery and rejection of timed-out-seat reclaim/late join. Large-world replication
is a separate milestone.

The view, controls and palette belong to this game. This reference deliberately
has no soundtrack; native audio remains available through the optional client.
