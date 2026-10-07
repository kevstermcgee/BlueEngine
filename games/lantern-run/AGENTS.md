# Lantern Run

Public distribution is native Windows x64 EXE only. Do not publish browser builds.

Read this file, src/lib.rs and game.project.json. For a rule change, update the tests in lib.rs.
main.rs is platform glue; use shared Scene/World composition; do not explore the legacy renderer. Engine docs/PORTABLE_GAMES.md is the platform/composition reference; docs/TWO_D.md covers primitives.

- GameLogic + Simulation own fixed 60 Hz integer rules. Input is Intent. Draw only reads state.
- Snapshot uses the engine frame/hash/migration contract on native targets. Bump VERSION and
  add a Migration for released state-layout changes; save every field that decides the future.
- The shared client owns viewport, devices, pause/restart, sound cues, save/load and settings.
- game.project.json deliberately selects presentation, targets and networking. Unsupported combinations fail.
- verification_input is a real public-input route, never mutate the game to pass verification.
  Add losing/collision/alternate-input tests too. Browser verification compares this route's native hash.

Commands from this game:

```
cargo test --no-default-features
python scripts/ship.py ship           # on Windows: EXE, shortcut, isolated capture smoke
python scripts/check.py               # full game verification including shipping
```

Web builds require rustup target add wasm32-unknown-unknown, Node, ws and Chromium.
Publishing to a directory produces a deployment-ready library, not an external URL.
Browser gameplay is retired. Inspect native captures/controls; audio counters do not prove heard sound.

For package-only delivery use `python scripts/ship.py ship --no-install`; it keeps the
isolated smoke and never accesses the desktop. Plain `ship` additionally installs and
verifies this game's shortcut. Other applications' icon similarity cannot fail shipping.
