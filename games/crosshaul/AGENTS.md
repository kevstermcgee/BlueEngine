# Working on Crosshaul

Start with the engine's absolute `tools/be2.py start "TASK" --project GAME_DIR`.
Read the selected public contract; use `docs/CUSTOM_SIM_CHEATSHEET.md` for custom-sim
APIs. Work only in this game unless the task explicitly authorizes engine work.

- `src/lib.rs` is the rendering-free fixed 60 Hz authority. `Intent` is the only
  gameplay command. Coordinates are integer cells; preserve coordinate inventories.
- `preview` owns both swept-box checks. Cubes are 0.72 cells wide and baffles 0.96.
  The renderer uses these same extents. Test both routes, destination occupancy,
  stationary cargo, the moving partner, and the first obstruction.
- Legal exchanges spend one allowance. Rejections and no-ops are free. Zero
  allowance with undocked cargo fails; undo and restart always remain available.
- The history, selected pair, axis, tick and synchronized motion all belong to the
  exact `devkit::Snapshot`. Do not add separate save files or presentation authority.
- `src/main.rs` uses shared `ClientInput`, `GameShell`, `Lifecycle` and kit drawing.
  The pointer is uncaptured: gate controls on `shell.accepting_input()`.
  Orbit changes presentation only. Never reinterpret world axes by camera angle.
- `assets/chambers.json` owns all eight chambers. Later chambers need height and
  depth changes. Chamber three requires moving an already docked carrier.
- Update reference solutions and `assets/verification_input.json` together when
  changing levels. Verification drives public selection/axis/exchange/next inputs.
- `assets/verification_script.txt` is the matching native cue run. `H` hints use the
  same route authority as the player and never execute moves.

Run `cargo test --no-default-features`, then the standalone game's inner/shipping
checks through `be2.py check --game GAME_DIR --loop ...`. Do not run dependency
engine suites for game-only rules. `python3 scripts/check.py --skip-ship` checks the
native project; `cargo clippy --all-targets -- -D warnings` checks source hygiene.

Windows x64 EXE installers are the delivery target; Linux is verification only.
Never install shortcuts or publish unless requested. On Windows use
`python scripts/ship.py ship --no-install`, then the documented installer gates.
Keep the canonical generated icon/resource/package wiring.

Inspect native captures and real mouse/keyboard controls when presentation changes.
Use `assets/native-review.json` as the criteria. An unavailable display, skipped
smoke, or Linux package cannot certify Windows delivery or physical audio. Record
observed friction honestly; the requesting supervisor owns feedback/learning entries.
