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

Rules/tests live in lib.rs. Presentation is its `#[cfg(feature="client")]` implementation.
For changing camera, replacing the full interface, theme/font assets and action hit regions,
read the authoritative [presentation contract](GAME_PRESENTATION.md#portable-interface-ownership).
The shared client preserves pause/restart, input gating, settings, saves and focus through
`Game::interface`; a different appearance does not need a different client.
For game-owned semantic sound events, checked banks and optional numeric fallbacks,
read [shared-client audio](AUDIO.md#portable-shared-client). Numeric cues 0/1/2 remain supported.
Default title/control/notice positions and menu bounds remain prototype defaults, and
`show_hud()` still hides default labels. `interface` replaces every overlay, including notices.
[Identity Lab](../examples/identity-lab/README.md) demonstrates three different interfaces
and audio identities over identical rules. Inspect captures and controls before packaging.
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
