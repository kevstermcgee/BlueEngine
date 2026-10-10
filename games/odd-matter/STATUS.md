# Odd Matter status

Implemented: six genuine 3D vaults; integer overlap parity; public Intent movement
and rail selection; crushing with unlimited undo/retry; win progression; exact
engine Snapshot saves including undo; native 3D geometry; cutaway and retained
obstruction outlines; move forecasts; compass; cross-sections; field notes;
authoritative event chimes; shared pause/audio menu; own canonical generated icon.

Verified on Linux:
- 11 behavioral rule tests and identity/icon checks, with and without client features.
- Integration formatting and native-client compilation.
- Strict Clippy across all game targets with warnings denied.
- Full Linux repository verification: all 40 stages of `be2.py check --changed --serial` passed.
- A 108-command public-input native route through all six exits, with captures
  inspected from every chamber and the final victory state.
- Real X11 keyboard/mouse control test: preview, cutaway, crush/undo, save/retry/load,
  exit progression, orbit, zoom and shared pause/quit. Evidence in
  `.blue-check/review-controls-final/controls.json` and its inspected PNGs,
  using the final packaged executable.
- Shipping gate: release package integrity, own icon and isolated declared-file
  packaged-game smoke. No shortcut installation. Fonts are embedded; their license
  and player instructions are declared package files.

Canonical native scripts were compared against a newly generated custom-sim
scaffold. The local check.py tool-discovery patch was removed; check.py, ship.py,
dev.py and both blue launchers now match the fresh scaffold exactly. project.py
matches the engine template. The icon was regenerated with fresh canonical map
tooling. Set BE2_TOOLS to its itest executable for native project checks; see README.

Remaining for the supervisor: Windows x64 EXE installer generation, executable
resources and packaged runtime verification on Windows. Physical Windows devices,
speaker playback, unfamiliar-player fun and worldwide originality are unverified.
The Linux package is verification evidence, not the game's distribution target.
See README.md for controls and repeatable checks. Genuine engine friction is
returned separately to the supervisor; no feedback or learning ledger was written.
