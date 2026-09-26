# Game presentation contract

Every playable BlueEngine game should meet this baseline before delivery:

- F toggles fullscreen in place; F11 may be an alias. Never restart the renderer or match.
- Escape opens a compact Resume / Controls / Quit menu. Opening it releases the mouse
  and suppresses movement, fire, reload, weapon selection and look. Resume clicks must
  not fire. An online match continues; label that explicitly. Losing focus releases capture.
- HUD defaults to a small crosshair, health, ammo/weapon and match objective/score.
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

Enable the `presentation` feature and use `viewer::game_client::{window_config,
GameShell, movement_axes, static_meshes}`. Call `begin_frame` before reading gameplay
input and gate every action with `playing()`. Call `menu` after drawing the scene/HUD;
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
