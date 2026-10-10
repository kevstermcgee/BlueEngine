# Identity Lab

To start or resume work, use `python3 tools/be2.py start "<task>" --project GAME_DIR`
in the engine checkout (`python` on Windows); `next`/`resume TASK_ID` refreshes the packet.

Read this file, src/lib.rs and game.project.json. For a rule change, update the tests in lib.rs.
main.rs is platform glue; use shared Scene/World composition; do not explore the legacy renderer. Engine docs/PORTABLE_GAMES.md is the platform/composition reference; docs/TWO_D.md covers primitives.

- GameLogic + Simulation own fixed 60 Hz integer rules. Input is Intent. Draw only reads state.
- Snapshot uses the engine frame/hash/migration contract on native targets. Bump VERSION and
  add a Migration for released state-layout changes; save every field that decides the future.
- The shared client owns viewport, devices, pause/restart, sound cues, save/load and settings.
- game.project.json deliberately selects presentation, targets and networking. Unsupported combinations fail.
- verification_input is a real public-input route, never mutate the game to pass verification.
  Add losing/collision/alternate-input tests too. Native scenarios exercise this same route.

Commands from this game:

```
cargo test --no-default-features
scripts/blue check                    # content/code/package integrity
scripts/blue ship --no-install        # native package + isolated smoke, no desktop access
scripts/blue ship                     # native desktop distribution
```

From the engine checkout, `python3 tools/be2.py check --game GAME_DIR --loop inner`
records focused test evidence without engine dependency tests. Use `--loop integration`
for native presentation compilation and `--loop shipping` for declared final targets.
New games default to Windows EXE delivery; game.project.json can explicitly declare native development targets.
For camera/input, custom themes, fully replaced interface layouts and fonts, read the
engine docs/GAME_PRESENTATION.md contract. Use `Game::interface` and `ui::Action` to
retain the shared lifecycle; draw notices and focus indicators in your own layout.
For named semantic sound bindings, read docs/AUDIO.md. Starter colors, HUD/panels,
particles and numeric sounds are optional prototype conventions, not game identity.
R/K/L/Esc/M/N stay shared actions. Native hosts should use `run_with_focus` and the generated platform hook.

Browser gameplay/WASM is retired. Do not install browser dependencies or add web targets.
Inspect native captures and controls; compiler/tests do not prove audible hardware playback.
