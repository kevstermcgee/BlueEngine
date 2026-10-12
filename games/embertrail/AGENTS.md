# Embertrail

To start or resume work, use `python3 tools/be2.py start "<task>" --project GAME_DIR`
in the engine checkout (`python` on Windows); `next`/`resume TASK_ID` refreshes the packet.

Read this file, src/lib.rs and game.project.json. For a rule change, update the tests in lib.rs.
main.rs is platform glue; use shared Scene/World composition; do not explore the legacy renderer. Engine docs/PORTABLE_GAMES.md is the platform/composition reference; docs/TWO_D.md covers primitives.

- GameLogic + Simulation own fixed 60 Hz integer rules. Input is Intent. Draw only reads state.
- Snapshot uses the engine frame/hash/migration contract on native. Bump VERSION and
  add a Migration for released state-layout changes; save every field that decides the future.
- The shared client owns viewport, devices, pause/restart, sound cues, save/load and settings.
- game.project.json deliberately selects presentation, targets and networking. Unsupported combinations fail.
- verification_input is a real public-input route, never mutate the game to pass verification.
  Add losing/collision/alternate-input tests too. Verification uses this route's native hash.

Commands from this game:

```
cargo test --locked
python scripts/ship.py ship            # native Windows distribution, shortcut and graphical smoke
# Compile scripts/installer.iss with Inno Setup 6, then test on Windows:
powershell -File scripts/test-installer.ps1
```

From the engine checkout, `python3 tools/be2.py check --game GAME_DIR --loop inner`
records focused test evidence without engine dependency tests. Use `--loop integration`
for native presentation compilation and `--loop shipping` for declared final targets.
The shared client already draws title/controls at (24,30)/(24,428)
and notices at (24,395); reserve those HUD areas (logical 800×450).
It owns start/pause/win/loss panels in (145,135,510,180); customize `menu_status()`
instead of drawing an overlapping terminal panel. R restarts, K saves, L loads,
Esc pauses and M toggles sound. Use a custom client when the game needs different UI.

Publish only a Windows x64 installer EXE after the Windows workflow passes. Preserve player saves and settings outside the install directory. Never claim compilation proves graphical playback or that audio counters prove a listener heard sound.
