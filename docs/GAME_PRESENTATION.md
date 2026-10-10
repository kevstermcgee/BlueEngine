# Game presentation contract

Every playable BlueEngine game should meet this baseline before delivery:

- F toggles fullscreen in place; F11 may be an alias. Never restart the renderer or match.
- Escape opens a compact Resume / Controls / Quit menu. Opening it releases the mouse
  and suppresses movement, fire, reload, weapon selection and look. Resume clicks must
  not fire. An online match continues; label that explicitly. Losing focus releases capture.
- Choose a HUD that serves the game. First-person combat prototypes default to a small crosshair, health, ammo/weapon and match objective/score.
  Put controls in the menu, secondary stats on Tab and diagnostics on F3. No permanent
  connection banner after joining and no persistent performance, FOV or control text.
- Give floors, walls, cover and landmarks different value/material roles. Use coherent
  location colors and directional markings. Do not rely on hue alone to identify teams
  or pickups; retain names, shapes, labels or silhouettes. Inspect actual first-person
  views and menus, including small windows, rather than only a showcase camera.
- Cache static geometry and material lookup at load. Honor primitive transforms and
  shapes; substituting boxes for cylinders creates misleading cover and collision.
- Simulate at fixed 60 Hz, independent of display rate. Interpolate remote snapshots;
  keep local look immediate. Bound catch-up/extrapolation and reject stale snapshots.
- Ship optimized builds. Measure frame-time percentiles and authoritative tick rate;
  joining a server is not a movement test. Exercise both WASD and arrows, menu input
  gating, cursor recapture and repeated fullscreen toggles.

## Shared implementation

Stock `GameDocument` games may opt into `presentation` in `game.json`:

```json
{"presentation": {
  "objective": "Open shutter, calibrate three times, then transmit",
  "success": "Observation transmitted! E / R to play again",
  "failure": "Battery depleted. E / R to restart",
  "counters": {
    "battery": {"label": "Battery", "format": "clock"},
    "calibration": {"label": "Calibration", "units": "/ 3"},
    "internal": {"visible": false}
  },
  "palette": {"accent": [1, 0.74, 0.3, 1]},
  "hud": {"scale": 1, "margin": 16, "width": 880, "crosshair": true}
}}
```

Counter keys must name declared counters (including `internal` in this example). `number` is the default format; `clock` interprets the authoritative
integer as seconds and displays minutes:seconds, retaining negative signs. Formatting never changes timers,
counter values or win/loss rules. Hidden counters remain authoritative and saved/replicated.
Palette slots are `background`, `panel`, `text`, `accent`, `success`, `failure`, each finite RGBA in 0..1.
HUD scale is 0.75..1.5, margin 8..48 pixels and optional width 240..900 pixels, clamped to the window;
text is fitted with an ellipsis. Wording is 1..200 printable ASCII bytes, labels <=48, units <=12 (or empty).
Include restart controls in customized outcome wording if desired.

Omitting `presentation` preserves stock defaults and legacy serialization; existing JSON games need no migration.
Rust code constructing `GameDocument` directly must add `presentation: None` (or `Some(config)`).
The renderer reads the same `GameState` as the headless runtime. See the
[observatory fixture](../assets/games/observatory/README.md) for complete configuration and captured evidence.

Enable the `presentation` feature and use `viewer::game_client::{window_config,
GameShell, movement_axes, static_meshes}`. Call `begin_frame` before reading gameplay
input and gate every action with `playing()` if the game captures the mouse (first person), or with
`accepting_input()` if it does not (`playing()` is false for ever without a captured mouse). `ClientInput::movement` and
`stick_look` already use `accepting_input()`; `mouse_look` needs the capture. Call `menu` after drawing the scene/HUD;
its return value requests quit. Native focus/input queries belong in the executable.
`viewer::presentation::PoseStream` rejects reordered samples and interpolates remote
actors with a 50 ms delay. Its bounded local extrapolation is a visual aid, **not** full
input prediction/reconciliation. High-latency games still need acknowledged input replay.

Use `HeadlessWorld::with_static_room` when the client shows a static map and gameplay
does not replicate movable props. `try_with_room` deliberately extracts catalog props
into rigid bodies and incurs physics work. Never simulate invisible prop motion against
a client that continues drawing the original authored positions.

`examples/custom_client.rs` demonstrates the shared shell. BlueDM and Riftwake in the
companion games repository consume the same shell, cached shading and pose stream.

## Text, weapons and collision-safe presentation

Use `game_text::{initialize, draw_text, measure_text}` for shipped HUDs and menus.
The embedded, licensed Liberation Sans raster atlas never grows or replaces a GPU
texture during a draw batch. Test both light and dark text after multiple fullscreen
transitions; a headless screenshot alone cannot verify live input or resizing.
`game_text::sign_mesh` uses that same immutable atlas for world-space signage.

`game_visuals::SurfaceRenderer` caches material state and adds inexpensive world-space
surface detail; use restrained material palettes, believable structural supports,
doors/windows, signage and architectural landmarks. Its draw method restores the
normal material before drawing signs and HUD text. `WeaponPresentation` supplies
cached beveled gun models, gloved hands, immediate recoil, flash, sound, tracers and
impact marks. Effects are cosmetic: server ammo/damage remain authoritative. Games
must respect semi-auto versus automatic firing and suppress effects in menus.

Use `PoseStream::collision_safe_position` with the actual game collision hull for
local snapshot extrapolation. Never use unconstrained extrapolation for a player
camera: even a 50 ms prediction can cross a wall or ledge. This is bounded presentation,
not full input prediction/reconciliation. Test floor, wall and stair approaches.

Windows executables should choose one source for keyboard edges. OR-ing asynchronous
native edges with delayed window-event edges can toggle the same menu twice. Keep OS
queries in the executable and pass a single callback to GameShell.

Local applications that actually freeze their simulation use `GameShell::local_menu` instead of `menu`; it preserves the shared controls and displays an accurate local-pause caption.

A game that owns its simulation (`new-game ... custom-sim`) is its own single authority: a pure, fixed-step,
rendering-free library that the window only reads, so presentation still never duplicates gameplay authority;
the rule above concerns presenting `GameSession` state and does not apply to it. Its window still follows this
contract (`GameShell` menus, cached static geometry, fixed simulation independent of frame rate) and uses the
frame clock from `ClientInput::frame_seconds` instead of `get_frame_time()`.

A shipped game names and marks its window: `game_client::window_config_with_icon(title, icon_from_rgba(..))`
with the title from `assets/identity.json`, the same text as its desktop shortcut. Plain `window_config` leaves
miniquad's logo as the window icon, which is fine for a prototype and wrong for a game that ships.

Shared standalone-game infrastructure: [gameplay kit](SHARED_GAMEPLAY.md). Generated games call `playable::run_game_with_options` for shared local/online gameplay. `MapPlayer` remains a static viewer; custom presentation can use the graphics-free `GameSession`.

## Camera and interaction

`be2.py start` preserves explicit camera requirements in `workflow.requested.cameras`.
First-person selects **custom-sim**, whose existing sample uses `View::first_person`,
`ClientInput::look_delta_with`, captured mouse/right-stick look and camera-relative movement.
Declarative first-person uses **stock**. Explicit incompatible starters are retained with a
`camera_implementation_required` gap. A gap is required engineering, never a fulfilled request.
`three-d` is a fixed oblique view of planar rules; it has no first-person controller.

| Intended view | Existing building blocks and input convention |
|---|---|
| First-person | `custom-sim` main; `devkit::{FpsCamera, MouseLook}`, `kit::View::first_person`; capture while playing, release in menus, aim from the eye |
| Third-person follow/orbit | `World::new(eye, target)` or `kit::View`; `Game::drag_look()` supplies accumulated `Intent.look`; `camera::sweep_boom` keeps the boom outside walls; choose camera-relative or world-relative movement deliberately |
| Fixed cinematic | `World::new` with game-owned shot selection; retain world-space interaction targets when changing shots; `three-d` already has a fixed view |
| Top-down | `two-d` logical Scene coordinates and `Viewport::pointer`; cursor targets and keyboard/controller focus can coexist |
| Orthographic/isometric | `world.camera.projection = macroquad::camera::Projection::Orthographics`; `fovy` is the visible world height; world depth/collision remains the game's choice |
| Side-scrolling | Game-owned drawing offset following the player, bounded by the level; transform pointer positions back to game coordinates; `Scene::draw(view, camera, renderer)` is available to custom clients |
| Custom | Own `World.camera` or `Scene::render_view`; reuse `GameShell`/`Lifecycle`, fixed input accumulation and saves; test camera changes and menu gating |

For example, a fixed orthographic view needs no new controller framework:

```rust,ignore
let mut world = World::new([8., 10., 8.], [0., 0., 0.]);
world.camera.projection = macroquad::camera::Projection::Orthographics;
world.camera.fovy = 12.; // world height, not radians in orthographic mode
scene.world(0, Rect::new(0, 0, 800, 450), world);
```

Following, orbit, side-scrolling and custom views require game-owned behavior; the
coordinator reports that implementation step when the sample does not supply it.
A change to perspective must include appropriate look, movement, picking and focus tests.

## Portable interface ownership

`two_d::client::run` retains prototype overlays. Three levels use the same client:

1. Keep `Game::interface` and `Theme` defaults for a prototype.
2. Override `Game::theme()` for background/panel/text/accent colors, a named font,
   panel bounds, padding, heading/body sizes and an optional pulse period. Override
   `interface` to add artwork through Scene sprites/geometry or motion via `UiFrame.elapsed`.
3. Replace `Game::interface(&self, scene, frame) -> ui::Layout` entirely. Draw any
   layout and register its logical hit rectangles with `layout.button(rect, ui::Action)`.

`UiFrame` exposes `screen` (Start/Playing/Paused/Won/Lost), focus, pointer, selected
navigation index, elapsed presentation seconds, canvas size, sound/music toggles,
notices, `menu_status()` and font metrics. Register buttons in keyboard/controller
navigation order; arrows/stick/D-pad move focus, Enter/Space/South activates, and
clicks hit rectangles. Draw the focus indicator using `frame.selected`.
Available actions are Start, Resume, TogglePause, Restart, Save, Load, ToggleSound,
ToggleMusic and Quit. R/K/L/Esc/M/N remain shared shortcuts. A playing HUD may expose
buttons too. The game owns appearance; the client executes every action, storage
error notice and restart and consumes that frame's gameplay input. Outcome screens
freeze gameplay. Focus loss pauses; regaining focus needs Resume. Pending input and
look are cleared at transitions. `run_with_focus::<G>(platform::focused)` uses a native
host's foreground callback; the new starter includes the Windows host hook. Legacy
`run` retains its old default callback, so existing hosts should adopt the focus hook.
Unattended captures/scripts use a focused baseline; the `focus:0@FROM-TO` script cue exercises focus loss without depending on a CI foreground window. No UI action or animation becomes part of authoritative simulation.

```rust,ignore
fn interface(&self, scene: &mut Scene, frame: &UiFrame) -> ui::Layout {
    let mut layout = ui::Layout::default();
    if frame.screen == ui::Screen::Paused {
        scene.text_with_font(210, "Continue observations", Point::new(65, 180),
                             24., paper_ink, "notebook");
        layout.button(Rect::new(55, 150, 290, 45), ui::Action::Resume);
    }
    // Draw your own notices, settings state and other screens as needed.
    layout
}
```

Fully replaced interfaces own notice placement too; keep save/load errors visible.
`show_hud()` still controls the default labels; `cue_particles()` disables prototype
bursts independently of sound. All overlays use an 800×450 logical canvas, letterboxed
by the shared viewport; layout positions and hit tests scale together.

## Portable fonts

Declare runtime assets with `Game::fonts() -> &'static [draw::FontAsset]`, for example
`&[FontAsset { id: "notebook", file: "assets/fonts/Notebook.ttf" }]`.
`Scene::text_with_font(layer, text, baseline, size, color, "notebook")` selects it.
`frame.fonts.measure(text, Some("notebook"), size)?` returns logical width/height/baseline
metrics using the exact draw font and scale. Use `None` for the prototype font.
Fonts load once; invalid files, duplicate IDs and undeclared font names fail explicitly.
All queued glyphs are measured before any draw batch; the raster size stays 64px while
window scaling changes, preventing atlas replacement from invalidating earlier text.
TTF/OTF support and glyph coverage depend on the bundled font; ship its license and
verify the actual characters, contrast and small-window layout. This is text measurement,
not complex-script shaping or an independent UI framework.
Put fonts under `assets/fonts` and add `"package": ["assets"]` to `assets/identity.json`
for runtime assets (the embedded-only starter does not need this declaration).
The normal package manifest then includes fonts, licenses and nested audio files.
Resolve an asset root with `devkit::runtime_assets` in native glue when launching from
another directory, as [Identity Lab](../examples/identity-lab/README.md) demonstrates.

Identity Lab shares one switch-puzzle simulation between a notebook index, a relay
instrument and a toy arcade. Its [presentation module](../examples/identity-lab/src/presentation.rs)
is a working example of full custom layouts, fonts, motion and identical actions;
its named audio bindings use the [audio contract](AUDIO.md#portable-shared-client).

The shared client accepts `--script` through the existing Timeline parser for reproducible
presentation inspection: `start`, `resume`, `pause`, `restart`, `save`, `load`, `sound`,
`music`, `quit`, `left/right/up/down`, `action`, `pointer:X/Y`, `click:X/Y`, `focus:0/1`.
For example `--script "start@2,pause@14,resume@20" --capture NEW_DIR --frames 1,10,16,22`.
Coordinates are logical; clicks use the renderer's registered hit regions and game device
mapping, then the same fixed-step intentions. Scripts/captures disable progress autosave
and use deterministic frame seconds, but save/load actions use normal Snapshot storage.
Set `BLUEENGINE_DATA_DIR` to a scratch directory for unattended storage tests.
