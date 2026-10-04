# Shared gameplay and presentation

New games should depend on the engine, not copy sandbox source. The sandbox is a
consumer of the modules below; its catalog, layout and creative-mode policy stay
application-specific.

| Capability | Engine API | Features |
|---|---|---|
| Authored local/online game | `playable::run_game_with_options`, `GameOptions` | client |
| Stock named cues/adaptive music/settings | `GameDocument::presentation.audio`, `stock_audio::{StockAudio, AudioCursor}` | client playback; configuration/observation headless |
| Graphics-free local/online session | `game_session::{GameSession, GameInput}` | none |
| Static viewer (no rules or dynamic props) | `local_client::run_map`, `MapPlayer` | client |
| Keyboard/mouse + native controllers | `game_input::ClientInput` | client |
| Pause/fullscreen/focus input snapshots | `game_client::GameShell`, `ShellActions` | presentation |
| Buttons, fitted text, scoped controller focus | `game_ui` | presentation |
| All eight character appearances | `character_skins::Avatar`, `IDS`, `NAMES` | presentation |
| Original Scientist/Feta and held tools | `character`, `wrench_view` | presentation |
| Text and depth-safe world signs | `game_text`, `game_visuals::SurfaceRenderer` | presentation |
| Asset extraction, rotated collision/geometry, aimed previews | `creative::{extract, place, placement_position}` | none |
| Protected removal, bounded undo, save filenames and atomic saves | `creative::{remove, History, save_path, save}` | none |
| Fixed-step motion and safe first/third-person camera | `simulation::PlayerStepper`, `camera::CameraRig` | none |
| Frame clock, fixed stepper, input accumulator, replay/capture/perf helpers, seeded RNG, saves, screen feel, synthesised audio | `devkit::{FrameClock, FixedStepper, InputAccumulator, Simulation, Playback, CapturePlan, Rng, Juice, Settings, synth}` | none |
| Batched dynamic meshes, materials and lighting, effects, HUD helpers, sound bank | `kit::{Template, Batch, Look, Materials, Fx, hud, SoundBank}` | presentation |
| Bake a static world with your own light | `mesh::{Lighting, bake_with}` | presentation |
| Title, tagline, controls and packaging manifest of a shipped game | `identity::Identity` | none |
| Voids, gravity, knockback impulses, body-overlap test | `controller::Controller::{set_floor, set_gravity, apply_impulse}`, `Collider::overlaps_body` | none |

All paths are under `vesper3d::viewer`. The compatibility crate name remains
`vesper3d`; the Cargo package name is `be2`.

## Start a game

Run `be2-tools new-game my-game ../my-game` from a checkout named BlueEngine.
The generated Cargo dependency points to `../BlueEngine`; adjust it for another
layout, or supply the optional third argument `ENGINE_PATH` to `new-game`. A fourth
argument picks the starter: `stock` (this section) or `custom-sim` (a game that owns
its simulation; see [custom clients](CUSTOM_CLIENT.md)). Games whose rules need
enemies, projectiles, scoring, AI or per-frame physics want `custom-sim`.
Every generated game carries an identity, an icon set and a ship gate: see the
"Definition of done" in [the game quickstart](GAME_QUICKSTART.md).

```sh
be2-tools new-game my-game ../my-game
cargo run --release --manifest-path ../my-game/Cargo.toml
cargo test --manifest-path ../my-game/Cargo.toml --no-default-features
```

The generated executable loads `game.json` and calls
`playable::run_game_with_options(game, options, platform::focused).await`. It opens no socket
unless `--connect ADDR` or `--server [ADDR]` is requested. The starter includes an
interactive blue terminal, a carryable dynamic apple, and a courtyard trigger.
WASD/arrows move; E/X interacts or carries. Once complete, E/X restarts the game.
The generated headless test exercises movement, physics, completion, pause and reset.
Adapt that test when changing the authored objective.

To play online, start `be2-headless --game ../my-game/game.json --server 127.0.0.1:4000`,
then run the generated executable with `--connect 127.0.0.1:4000`. Both peers need
identical content. `--transport production` and `--auth-key` use the same existing
QUIC/authentication path as the stock client; see HOSTING.md. Protocol 7 adds round
identity and actual mover positions; both peers must rebuild.

Local authority is HeadlessWorld, including rules, timers, triggers and prop physics.
Online clients never step those systems: GameRuntime accepts server state and mover
positions; the existing PredictionBuffer and InterpolationBuffer handle movement
and prop presentation. Local pause freezes the world. Online menus neutralize local
input while the server keeps running. Any joined player may restart a completed
round. Restart preserves IDs and the monotonic tick, but resets players, props,
ownership, objectives, timers and movers. Old-round sequenced input is rejected.

`GameOptions` exposes connection/profile, cosmetics, perspective and verification
options. `--character astronaut --third-person` changes presentation only; the
GameDocument still owns the movement profile. Native foreground/key-state queries remain in
the generated executable. `options.keyboard = platform::keyboard()` supplies the
Windows adapter to shared ClientInput; the library owns key-edge tracking and
focus gating. `run_game_with_focus` remains the simpler window-event input entry. Non-Windows focus relies on GameShell minimization events.

`local_client::run_map` and `MapPlayer` remain lightweight static viewers. They do
not load game documents, execute rules or simulate props. `examples/custom_client.rs`
illustrates that intentionally narrower viewer, not the generated gameplay path.

## Custom loops

Own one `ClientInput` and one `GameShell` in your application. Each frame call
`input.begin_frame(&mut shell, capture_cursor, focused)`, including while paused.
`capture_cursor` is whether the game wants the mouse captured while unpaused and
focused: `true` while playing, `false` on title, menu and game-over screens so the
cursor is released (an online session passes "connected").
Use `input.movement(&shell)` and `input.look_delta(&shell)` only for gameplay
(`mouse_look` and `stick_look(&shell, seconds)` are the two halves when you want your
own stick sensitivity). The adapter gates these when capture/focus/pause prevents play.
`focused` is a
host-provided foreground signal; the shell additionally handles minimization.
`begin_frame` also starts the engine's frame clock: read the frame length from
`input.frame_seconds()`, the wall-clock interval between frame starts. macroquad's
`get_frame_time()` is stamped after the GL flush and reads 33 ms then 1 ms when one
frame stalls although frames were presented evenly, which a fixed-step accumulator
turns into a doubled step and a repeated frame. `devkit::FixedStepper` turns frame
lengths into whole 60 Hz ticks and `devkit::InputAccumulator` delivers exactly one
input per tick (edges once, look immediately). A game with its own rules builds on
these: [custom clients](CUSTOM_CLIENT.md#a-custom-simulation).
Native OS focus queries remain in the executable. Do not place device backends in
thread-local storage: Windows destroys worker threads before TLS cleanup.

For modal screens, call `poll(focused)` once, route the frame's buttons to the
modal first, then pass `shell_actions(paused, keyboard_edges)` to
`GameShell::begin_frame_with_actions`. This avoids duplicate edges. Controllers
are positional: A confirms, B backs out, Start pauses and D-pad changes focus.
Keep keyboard text entry separate from gameplay shortcuts.

Call `game_ui::begin_navigation(scope, input.navigation())`, draw enabled buttons,
then `end_navigation()`. Use a distinct nonzero scope per screen; zero disables
underlying controls during pause. Buttons consume a confirmation once. The UI
navigation storage supports one immediate-mode UI per render thread/window.

`Avatar::new(id)` requires an active graphics context. Scientist, Feta, cowboy,
alien, robot, astronaut, diver and ranger are available. `kind()` supplies the
matching collision profile. Cosmetic hats/backpacks do not change the hull.

## Creative editing

Use `extract(document, root_entity_id)` to isolate a static asset. Root-prefixed
geometry and colliders are normalized to a single `specimen` asset. Animated
transforms are rejected. `specimen(document)` is the gallery convenience wrapper.
`place` generates unique `creative-N` names, rotates visuals and collision together,
protects the player and default spawn, and validates the resulting map. Its limit
is 256 placed objects, within existing document limits. `creative-` is a reserved
ownership namespace; authored base entities must not use it.

Use `placement_position` for ray hit, support extent, height offset and grid preview.
`remove` only removes owned creative objects. `History` keeps the last 16 document
snapshots: remember after a successful edit/save, peek before restoring, and pop
only after the restore succeeds. Hosts must reject restores intersecting players.
`save_path` rejects path traversal in map IDs. `save` validates and writes a complete
replacement (maximum 8 MB) before renaming; failed writes preserve the old save.
The caller owns the save directory and runtime application of the edited map.

The sandbox still owns its specific asset list, map picker, palette, autosave policy
and export/stock-client launch actions. Those are product choices, not mandatory
engine behavior. Reuse the shipped asset/map data with the normal authoring tools.

## Verification

Game-only edits use the generated `python scripts/check.py`; `--content-only` is the
no-Cargo iteration path. Setup and guarantees are in [the change workflow](CHANGE_WORKFLOW.md).
The full engine suite below is for changes to the shared engine implementation.

`python tools/be2.py check` covers default/headless tests, docs and Clippy.
`tests/shared_gameplay.rs` exercises public placement and scaffold contracts;
`tests/sandbox.rs` now exercises the public creative API across all 78 assets.
Build a generated project as an external Cargo consumer too. Both runners accept
`--capture NEW_DIR` for world/pause screenshots; sandbox `--sign-capture NEW_DIR`
and `--creative-smoke NEW_DIR` cover the regression scenes. Captures do not replace
physical input, cursor capture and fullscreen testing.

The gameplay runner also accepts `--playback INPUTS.json --capture NEW_DIRECTORY`.
INPUTS is an array of `GameInput` frames (one per 60 Hz tick; omitted fields default).
It drives the same local session and renderer, saves actual world/win/reset/menu
images and a `run.json` trace, then exits. Playback is local-only and explicitly
bypasses device/focus gating; interactive controls still require a live playtest.

Local games save and load: F5 writes the `quick` slot, F9 loads it, `--save-dir DIR` moves the folder
(default: `saves` next to the executable) and `--load SLOT_OR_FILE` resumes at start. Online clients refuse
(the server owns the world; `be2-headless --autosave` saves it). The contract, failure modes and versioning
are in [SAVE_STATE.md](SAVE_STATE.md); `be2-tools save-info FILE [GAME_JSON]` inspects a save.
World replication progresses through bounded partial updates under the existing
1100-byte ceiling. See HOSTING.md for entity, record and acknowledgement limits.
