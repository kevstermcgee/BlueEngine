# Native 2D presentation

Use `python3 tools/be2.py start "Create a 2D game with projectiles" --kind new-game
--target windows`. Execute the packet's explicit two-d scaffold command. Read the
project AGENTS.md, src/lib.rs and game.project.json; do not inspect the legacy renderer.
Windows EXE distribution and native Linux/macOS development are separate targets.
No browser target is supported; see docs/BROWSER_WORKFLOW.md for migration.

Edit requirements before implementing. GameLogic owns typed game-specific rules,
including enemies, projectiles, scoring and timers. Stock GameDocument is a separate
3D authoring path; combining declarative stock authoring with 2D requires deliberate
engineering. Portable multiplayer is not implemented; custom NetGame/ClientView
presentation is available without inventing another network stack.

Examples: games/lantern-run, games/pocket-breaker, games/orchard-watch; 3D composition
and hybrid presentation are described in docs/PORTABLE_GAMES.md.

## Game surface (`vesper3d::two_d`)

- `Simulation` + `GameLogic`: fixed 60 Hz integer rules, `Intent { x, y, pointer, action }`, outcome,
  public-input verification route and one meaningful real-device probe. All state stays rendering-free.
- `Snapshot`: same checksummed BE2SAVE frame, exact continuation, version and migration contract as 3D.
- `Rect::{overlaps,contains,slide}`: half-open integer bounds, swept axis movement that cannot tunnel;
  `Body::step(gravity,walls)` supplies small kinematic physics; `Trigger::enter` detects entrances.
- `draw::Scene`: `rect`, `circle`, `text`, `sprite`, `sprite_png`; stable integer painter layers.
  Texture transforms have position, scale and rotation. `sprite` accepts atlas source rectangles.
  Embedded PNGs decode once in the renderer. Invalid/duplicate asset IDs fail explicitly.
- `draw::Animation::new(frames,ticks_per_frame)?.frame(tick)`; `Particles` are bounded, presentation-only.
- `Viewport::fit` letterboxes a logical canvas; pointer coordinates outside it are `None`. Rendering
  can offset by a camera `Point`. The shared client uses an 800×450 logical canvas.
- `draw::Game::draw` reads your state; `client::run` owns timing/input, pause/restart, sound,
  settings, saves, error notices and verification evidence. Main only supplies identity/icon configuration.

Rules/tests live in lib.rs. Presentation is its `#[cfg(feature="client")]` implementation. Every sound
cue is 0 pickup, 1 damage, 2 success; shared generated WAVs load asynchronously. New genres may need
a custom sound bank later. This milestone intentionally has no editor or genre framework.

The shared client overlays the title at logical (24,30), controls at (24,428)/(24,448), and
transient save/error notices at (24,395), on layers 100/101 of the 800×450 canvas.
Reserve those areas in your HUD; avoid drawing another title or controls over them.
It also owns start/pause/win/loss panels at (145,135), size 510×180, layers 200/201.
Supply game-specific text through `Game::menu_status()`; avoid a duplicate terminal panel
under that overlay. `Game::show_hud()` controls title/control labels, while a custom client
remains available for a different presentation. Shared keys are R restart, K save, L load,
Esc pause and M sound; these do not need game-owned key handlers.
Inspect contrast, glyphs and the native captures before final
packaging. The built-in bitmap font covers ordinary ASCII; custom text/art remains
available through the drawing APIs and custom clients.
The starter already uses `runtime::assert_deterministic`, `snapshot::assert_resumes_exactly`,
`two_d::verify` for a public-input route, collision and loss assertions. Extend them
with timer, pickup, locked-exit and restart cases. Native packaged-game smoke checks the client; inspect pause, storage, audio and input separately.


## Native checks

From the engine use `be2.py check --game GAME --loop inner`, then integration and
shipping. From the game use `python scripts/ship.py ship --no-install` on Windows
for a complete package and isolated launch smoke; plain ship requests installation.
The shared native client owns storage/audio/input. Retain the engine's Snapshot
migration and deterministic continuation tests. A compile pass does not prove
rendered behavior, physical controller input or audible playback.

Button utilities can opt into `GameLogic::pointer_target_only_on_press()` so keyboard/controller
actions use focus and actual click targets survive frames without simulation ticks. Default input
retains continuous pointer aiming. Override `restart()` to keep a persistent library; its default
creates a fresh `Self::new(7)`, preserving ordinary game restart behavior.
