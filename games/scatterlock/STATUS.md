# Scatterlock implementation status

The six-chamber native 3D game is implemented: fixed-point position scaling, simultaneous swept collision, matching endpoint docking, solid docked cargo, pulse allowance exhaustion, undo/restart, progression and final completion. Rendering-free authority takes Intent input at 60 Hz. Engine Snapshot captures cargo, allowance, selection, chamber and undo history. Windows x64 EXE installers are the delivery target; Linux is the verification host.

This review preserved the rules and improved the default camera angle, corner-post visibility, travel-path thickness, numbered berth labels and on-screen explanation. The BlueEngine Games opening is centered text-only. Fresh canonical map tooling generated a comparison custom-sim scaffold, refreshed the tooling revision and regenerated the game icon. The comparison scaffold was removed without replacing authored game rules.

Maintenance corrected two remaining presentation issues. Co-located B/D origins in chambers four and six now have separate world badges and matching click targets; their authoritative coordinates are unchanged. Cargo geometry and number/LOCKED labels now use the same interpolated position throughout the 0.55-second slide. Client regressions cover B/D clicks at three UI scales and interpolation through a complete two-pulse delivery, including missing animation history after a restore. These are separate from the headless rule/control tests below. The native review checklist now explicitly requests both overlapping-origin clicks and mid-slide label inspection.

`wiki.json` contains the public overview, concrete rules, exact controls and design rationale. Four unaltered 1280x800 native screenshots in `assets/wiki/` show the opening puzzle, an elevated obstruction, the delivery-order detour and campaign completion. Each gallery image was inspected. No story or characters were added.

Fresh evidence against the current engine:

- All 17 headless rule, control and identity tests pass, including unique scaling, moving/static/docked collisions, rejection, recoverable loss, determinism, complete public-input campaign solutions and exact Snapshot continuation.
- `evidence/current-native-review/` records real X11 keyboard and mouse events: number keys, world-badge/sidebar clicks, loss, undo, restart, solve, F5/F9, orbit, zoom and pause. The final log confirms two secured cubes and zero allowance remaining. Every control capture was inspected.
- `evidence/campaign-current/` records the full native campaign using public script cues; its final log confirms chamber six, all three cargo docked and zero allowance remaining. All ten captures were inspected.
- `evidence/current-opening/00-opening.png` confirms centered BlueEngine Games text without an image.

Run `python3 scripts/check.py` for the project checks and package validation. Run `python3 scripts/ship.py ship --no-install` on a native display (or under Xvfb for Linux software verification) for a release package and isolated packaged-game smoke. The package stamp and `.blue-check/` reports contain exact results; prior verification is not a substitute. Do not edit source while packaging, because the shipping gate binds source before and after the build.

Windows executable resources, installer prepare/build/test and physical controls, hardware rendering and audible sound remain supervisor/native-review gates. No engine source was changed. No shortcuts were installed and nothing was published. Observed friction: the capture helper can lose Xvfb startup diagnostics, and the audio backend may initialize ALSA during muted runs. Direct logged Xvfb runs and child-only null ALSA configuration supplied current software evidence. The supervisor owns feedback and learning-ledger entries.
