# Shared gameplay and presentation

New games should depend on the engine, not copy sandbox source. The sandbox is a
consumer of the modules below; its catalog, layout and creative-mode policy stay
application-specific.

| Capability | Engine API | Features |
|---|---|---|
| Playable static-map starter | `local_client::run_map`, `MapPlayer` | client |
| Keyboard/mouse + native controllers | `game_input::ClientInput` | client |
| Pause/fullscreen/focus input snapshots | `game_client::GameShell`, `ShellActions` | presentation |
| Buttons, fitted text, scoped controller focus | `game_ui` | presentation |
| All eight character appearances | `character_skins::Avatar`, `IDS`, `NAMES` | presentation |
| Original Scientist/Feta and held tools | `character`, `wrench_view` | presentation |
| Text and depth-safe world signs | `game_text`, `game_visuals::SurfaceRenderer` | presentation |
| Asset extraction, rotated collision/geometry, aimed previews | `creative::{extract, place, placement_position}` | none |
| Protected removal, bounded undo, save filenames and atomic saves | `creative::{remove, History, save_path, save}` | none |
| Fixed-step motion and safe first/third-person camera | `simulation::PlayerStepper`, `camera::CameraRig` | none |

All paths are under `vesper3d::viewer`. The compatibility crate name remains
`vesper3d`; the Cargo package name is `be2`.

## Start a game

Run `be2-tools new-game my-game ../my-game` from a checkout named BlueEngine.
The generated Cargo dependency points to `../BlueEngine`; adjust it for another
layout, or supply the optional third argument `ENGINE_PATH` to `new-game`. `cargo run --release --manifest-path ../my-game/Cargo.toml` opens a playable
map, not a console-only loader. The starter uses the shared window configuration,
static renderer, collision, camera, Xbox controls, avatar and pause menu. Add
`-- --character astronaut --third-person` to select a character/view.

`examples/custom_client.rs` is the small in-repository version. External Cargo:

```toml
vesper3d = { package = "be2", path = "../BlueEngine", default-features = false, features = ["client"] }
macroquad = { version = "=0.4.14", default-features = false, features = ["audio"] }
```

The generated executable includes the supplied native foreground adapter on Windows
and calls `run_map_with_focus`; native OS queries remain application-owned.
Non-Windows focus currently relies on GameShell minimization events.

The starter intentionally uses static map collision. It does not simulate invisible
moving props. Its valid `game.json` can be launched by the stock `be2 --game` client
for declarative rules, movers, dynamic physics and authoritative gameplay. Adding
rules to that file does not make the static `run_map` loop execute them. For custom
rules, use the existing HeadlessWorld/GameRuntime contracts and own the game loop.

## Custom loops

Own one `ClientInput` and one `GameShell` in your application. Each frame call
`input.begin_frame(&mut shell, connected, focused)`, including while paused.
Use `input.movement(&shell)` and `input.look_delta(&shell)` only for gameplay.
The adapter gates these when capture/focus/pause prevents play. `focused` is a
host-provided foreground signal; the shell additionally handles minimization.
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

`python tools/be2.py check` covers default/headless tests, docs and Clippy.
`tests/shared_gameplay.rs` exercises public placement and scaffold contracts;
`tests/sandbox.rs` now exercises the public creative API across all 78 assets.
Build a generated project as an external Cargo consumer too. `run_map` accepts
`--capture NEW_DIR` for world/pause screenshots; sandbox `--sign-capture NEW_DIR`
and `--creative-smoke NEW_DIR` cover the regression scenes. Captures do not replace
physical input, cursor capture and fullscreen testing.
