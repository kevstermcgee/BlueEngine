# BlueEngineSandbox validation

Windows verification, September 26, 2026. Based on BlueEngine commit
`16acc82ddc3f934748ff5cf7e1f7a5ac74a289e3`, plus the accompanying local sandbox changes.
The source archive includes uncommitted development changes; no upstream push was made.

## Automated checks

- `cargo fmt --check`: passed.
- `cargo test --locked`: passed (complete default-feature suite).
- `cargo test --locked --no-default-features`: passed (complete headless suite).
- Clippy, all targets, warnings denied: passed with and without default features.
- Library rustdoc, warnings denied: passed in both feature configurations.
- `tools/check_headless.py` and `tools/check_authoring.py`: passed.
- `scripts/validate_sandbox.py`: **90/90 passed**: all 78 specimen audits,
  eight file-backed map audits, and four new-map traversal routes.
- `scripts/build_sandbox_content.py --check`: passed; generation matches the catalogs.
- Extended asset catalog: three packs, 78 assets, no validation issues.
- `scripts/publish_games.py check`: passed; sandbox content is included in the
  existing managed games collection. The custom executable uses its own package script.
- Four new Rust integration tests passed: all registered content compiles with
  clear spawns; native apple geometry retains dynamic-prop eligibility and settles;
  both movement profiles advance consistently at 30/60/144 display rates;
  actual clearance tunnels distinguish Feta, crouched humans and standing humans.

## Graphical and live checks

Optimized builds rendered browser, first-person and actual shared pause-menu
captures for all four new maps and all four new character skins. Native prop and
new pine previews were also inspected. Character proportions and small-object
framing were corrected after visual review. Official branding is preserved.

Sampled capture runs were around **16.7 ms median**, with **16.8–17.0 ms p95** on
this Windows host. These are short static capture samples, not general hardware
performance guarantees. The shared stepper tests explicitly verify approximately
120 authoritative movement steps over two simulated seconds at multiple display rates.

Live Windows input verification covered:

- Enter opens walk testing; WASD and arrow presses move the real controller.
- Escape releases capture and opens the menu; attempted movement leaves the
  tick count frozen while paused (observed at tick 1864).
- Resume restores simulation; F toggles fullscreen and back with readable text.
- Controls menu layout, Tab browser navigation, character selection and Q
  third-person rendering of the wizard.
- Search starts empty, filters ordinary typed keys, and treats F as text while
  focused. A stale text-queue issue was found and fixed during testing.
- The Physics session button launches the stock client with the selected apple
  map and its character-selection screen. Live pickup/drop was not exercised;
  prop initialization/settling is covered by the native simulation test.

## Deliberate boundaries

The sandbox's walk mode is a static collision/scale workbench. Dynamic props are
tested through the explicitly launched stock client. Custom skins remain in the
sandbox; the stock client still offers Scientist/Feta. Cosmetic hat/staff/antenna
geometry can extend outside the human hull. No multiplayer sandbox, combat mode,
live geometry editor, custom character powers, or arbitrary audio/texture browser
is claimed. Map export saves authored geometry and spawn metadata, not a running
physics session. Only Windows was built and exercised graphically.

Generated content and traversal routes are committed-source candidates, while
temporary logs and screenshots remain under `.be2-work`. A Windows CI workflow
rebuilds, validates and packages the project for future changes.

## Character lineup update � 2026-09-26

Temporarily removed Starfall Wizard from browser selection and the --character CLI.
The five available characters are Scientist, Feta, Dusty Trails, Nova Visitor and
Bolt-7. Existing model indices are mapped explicitly so remaining appearances stay
correct. Formatting, four sandbox tests, sandbox Clippy, release build and games
publication validation passed. The packaged cowboy browser and pause-menu captures
were inspected after rebuilding.

## Creative sandbox update — 2026-09-26

Play/Create now offers nine maps and eight characters. Orbit Scout, Brass Diver
and Cedar Ranger join the five existing selectable characters; the wizard stays
hidden. V opens the 78-item asset palette. Placement supports camera aiming,
quarter-turn rotation, reach, elevation, grid snapping, deletion, undo and per-map
autosave. These are static placed props, not a dynamic multiplayer physics mode.
This section supersedes the earlier walk-only boundaries and five-character count.

Validation completed:
- Full be2 check passed: 212 default tests and 189 no-default tests, formatting,
  rustdoc, both Clippy configurations, headless and authoring guards.
- Seven sandbox tests include all 78 specimens at all four rotations, transformed
  geometry and collider validation, safe IDs, removal, save replacement and reload,
  and rejection of player/spawn intersections and invalid positions.
- Final sandbox Clippy and release build passed after the final input fixes.
- Application creative smoke placed three props, removed and restored one, switched
  character and map, and reloaded the persisted build successfully.
- Setup, placed world, asset palette and all three new character previews were
  visually inspected. Sample capture timing was 16.65 ms median / 17.08 ms p95;
  this short scripted sample is not a general performance guarantee.

Live keyboard/mouse verification for this update was unavailable because the UI
helper failed to initialize (failed to write kernel assets, OS error 3). Scripted
application captures and persistence checks ran successfully; they do not replace
physical input, cursor capture or fullscreen interaction testing.
