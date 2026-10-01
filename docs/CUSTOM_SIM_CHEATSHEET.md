# Custom-sim cheat sheet: the game-facing API on one page

For an agent building a game that owns its rules (`be2-tools new-game NAME DIR ENGINE custom-sim`). Everything is
under `vesper3d::viewer`. Signatures were checked against the source; when in doubt read the module's rustdoc
(`cargo doc --open`), not the whole repository.

## The loop (`devkit`)

```rust
let mut life = Lifecycle::<Held>::start_or_exit(&args, &CUES);   // flags: --capture --script --seed --load --perf
let seed = life.seed();   let unattended = life.options.unattended();   life.options.silent();
loop {
    let dt = life.begin_frame(input.frame_seconds());            // fixed dt when unattended
    // devices in, or from the script:
    life.feed(held, edge_bits, look_delta);                      // one frame of held state; edges are bit flags
    if let Some(s) = life.script() { s.axis("fwd","back"); s.held("drift"); s.starts("perk"); s.value("look",0) }
    for _ in 0..life.ticks(dt, time_scale, playing) { let tick = life.take_tick(); tick.held; tick.pressed(BIT); tick.look; sim.step(..) }
    life.save_load_requested(f5, f9); life.quick_save(&sim, "label"); life.quick_load(&mut sim); life.load_flag_or_exit(&mut sim);
    life.restart_seed(); life.reset_input(); life.time(); life.pending_look();
    if let Some(path) = life.capture_path() { life.captured(&path, kit::capture::save_frame(&path).map_err(|e| e.to_string())) }
    if life.end_frame(dt) { break }
}
```

The simulation implements `Simulation { type Input; fn step; fn state_hash; fn hash_parts }` (hash with
`StateHasher::new().u64(..).f32(..).bool(..).finish()`) and `Snapshot { KIND, POLICY, State, capture, restore, save_tick }`.
Test with `assert_deterministic(|| Sim::new(7), &inputs)`, `run_inputs`, `snapshot::assert_resumes_as_promised`.
Random numbers: `Rng::new(seed)`: `range(lo,hi)`, `below(n)`, `chance(p)`, `sign()`, `pick(&items)`, `shuffle(&mut v)`.

## Devices (`game_input::ClientInput`, `game_client::GameShell`, `gamepad`)

`input.begin_frame_with_keyboard(&mut shell, capture_cursor, focused, platform::keyboard())`,
`input.pressed(KeyCode::X)` (edge), `input.down(KeyCode::X)` (held). The native Windows reader covers all letters,
digits, F-keys, modifiers, navigation and common punctuation (`game_input::KEY_TABLE`); a `KeyCode` outside that
table reads as never pressed on Windows. `input.movement(&shell)` (WASD/arrows/stick →
forward, right, sprint, jump edge), `input.look_delta_with(&shell, dt)`, `input.menu_step()` (`MenuStep { up, down, left,
right }`), `input.menu_select()`, `input.menu_back()`, `input.gamepad()` → `GamepadFrame { left_stick, right_stick,
triggers: [left, right], down(Button), pressed(Button) }`. `gilrs` buttons are positional: `Button::South` = A,
`West` = X, `LeftTrigger`/`RightTrigger` are the bumpers, `LeftTrigger2`/`RightTrigger2` the analog triggers.
`shell.paused`, `shell.playing()` (mouse captured: first person), `shell.accepting_input()` (menu closed: any game),
`shell.local_menu(title, &controls)` (returns true to quit), or `shell.local_menu_with_audio(title, &controls,
AudioMenu { music_on, sfx_on, has_music })` for the same menu with a Settings screen (music/sound toggles, a
"Save music" download) built in; returns `MenuOutcome { quit, toggle_music, toggle_sfx, download_music }`.

## Yaw and space (`devkit::path`)

Yaw 0 faces -Z, positive yaw turns towards +X: `forward(yaw) = (sin, -cos)`, `right(yaw) = (cos, sin)`, `yaw_of(v)`,
`wrap_angle(a)`. `ClosedPath::from_control_points(&[(x,z)], subdivisions)`: a smooth loop with `length()`, `point_at(s)`,
`tangent_at(s)`, `offset_point(s, lateral)`, `nearest(pos, hint, window)` / `nearest_global(pos)` → `PathPoint { index, s,
lateral, center, tangent }`, `arc_delta(from, to)`. `math::V(x, y, z)` has `dot`, `cross`, `length`, `norm`, `lerp`.
An indoor level built in code instead of an authored map: `devkit::{wall_along_x, wall_along_z}(fixed, a, b, height,
thickness, gaps)` cut doorways out of a straight wall (`gaps` is a list of `(start, end)` ranges to leave open);
`devkit::WaypointGraph::new(vec![Waypoint { pos, edges }, ..])` then gives an NPC `.nearest(pos)` and `.path(from, to)`
(breadth-first) through the rooms those walls make — a different shape from `ClosedPath`'s single loop.

## Drawing (`kit`, needs the `presentation` feature)

Build once: `let mut t = Template::new();` then `t.box_(center, half, rgb, glow)`, `t.ball(center, radii, rgb, glow, segs, rings)`,
`t.cone(base, r_bottom, r_top, height, rgb, glow, sides)`, `t.cylinder(base, r, height, rgb, glow, sides)`,
`t.ring(center, r_in, r_out, rgb, glow, sides)`, `t.disc(..)`, `t.tube(..)`, `t.quad_facing([4 corners], normal, rgb, glow)`,
`t.sky_dome(radius, |elevation| rgb, segs, rings)`, `t.transformed(Mat4)`, `t.append(&other)`, `t.split()`, `t.to_meshes()`.
`rgb` is `[f32; 3]`, `glow` 0..1 (self-illumination). A template over 9,000 vertices is split for you now; over 65,535
it panics: use several. Per frame: `let mut batch = Batch::new(); batch.clear(); batch.add(&template, Mat4, Tint::NONE)`
(`Tint::alpha(a)`, `Tint::flash(x)`), then `gl_use_material(&materials.world); batch.draw();` (`materials.fx_alpha` and
`fx_add` for translucent and additive batches). For a scene's unchanging static meshes (built once with `to_meshes()`,
not a per-frame `Batch`), `materials.draw_static(&meshes)` binds `world` and draws them; it exists because the manual
two-step version is easy to get backwards, which compiles and renders nothing with no error. `Materials::load()`,
`Look::night()` (fields: `ambient_sky`, `fog_color`,
`fog_density`, `key_color`, `rim_color`), `materials.set_scene(&look, eye, time, pulse)`, `View::first_person(eye, yaw,
pitch)` (`.fov`, `.roll`, `.project(p, w, h)`), `Fx::new(seed)` (`sparks`, `dust`, `ring`, `fireball`, `beam`, `popup`,
`banner`, `confetti_fountain`, `update(dt)`, `draw(..)`), `hud::{text_outlined, text_centered, text_right, panel, bar,
wrap, col, ui_scale, overlay, draw_popups, draw_banners}`, `Juice { shake, kick, stop, flash, camera_shake() }`.

Local illumination: `PointLight::new(position, radius, rgb, intensity)?`; call
`materials.set_point_lights(&lights)?` after each `set_scene`. Maximum four unshadowed lights;
`set_scene` clears them so existing games keep their appearance.

Mirrors: `MirrorPlane::new(center, right, up, size)?` (`right × up` faces the viewer),
`PlanarMirror::new(plane, (768,384))?` after GL initialization. Each frame, if
`mirror.camera(eye, far)` returns a camera: set it, clear, render the world excluding mirrors,
using `camera.eye` for lighting; restore the main camera and render the world, then
`mirror.draw_surface()`. Target depth, parallax, clipping behind the plane and UV orientation
are engine-owned. No recursion/rough reflection; one extra world render per visible mirror.

## Sound (`devkit::synth`, `kit::SoundBank`)

Presets: Click, Select, Back, Blip, Coin, Pickup, PowerUp, Jump, Land, Hit, Thump, Explosion, Zap, Shoot, Whoosh, Error,
Warning, Success, GameOver, Footstep, PowerDown, Hurt, Chime. Render them off-thread with `Rendered { sfx, stems }`, play
with `sounds.play(index, volume)`.

## Online (`net`, `netplay`)

See `docs/NETPLAY.md`. `net::codec::{Writer, Reader}` for layouts, `net::loopback::LoopNet` for tests,
`net::{server_transport, client_transport}` for UDP or QUIC/TLS.

## Seeing and testing without a display

`python tools/xcapture.py GAME_BINARY --frames 30,300 -- --character ghost` runs the game on a virtual display and
prints the screenshot paths (`docs/HEADLESS_CAPTURE.md`). Read the PNGs with your image viewer. Software rendering
is slow; capture a few frames, not a whole match.

## Traps this list exists to avoid

- `cargo test --locked` fails on the very first command of a new game: run `cargo build` once (it settles Cargo.lock).
- The `quad` winding and the 9,000-vertex limit no longer fail silently, but use `quad_facing` anyway.
- **`shell.playing()` needs a captured mouse.** A game with no mouse look (`capture_cursor = false`) gets `false`
  forever, and gating input on it drops every key while the menus, which read keys directly, still work. Use
  `shell.accepting_input()` (or `!shell.paused`). Scripted and autopilot runs bypass this gate, so tests and
  captures will not reveal it: run the real window once and press a key (Spooky Kart shipped with this bug). The engine's own
  `ClientInput::movement`/`stick_look` use `accepting_input()` (unit-tested with the mouse uncaptured); only `mouse_look` needs capture.
  A scripted-input pass is not evidence that real keys reach your game.
- `Lifecycle::feed` takes edge *bit flags* (`u32`), not a bool; `held` and `starts` are different cue queries.
- Drawing a scene's static meshes with `draw_mesh` before `gl_use_material(&materials.world)` compiles, runs and
  renders nothing, with no error. Use `materials.draw_static(&meshes)` (built for exactly this; Dead Air's own
  headless captures were the only thing that caught the manual version getting it backwards).
- Facing something (an NPC toward the player, a spawn toward a doorway) is `devkit::path::yaw_of(direction)`; do
  not hand-derive `atan2` for it, even though the formula is short enough to look safe to re-derive. For a spawn
  that should face a specific landmark, `yaw_facing(spawn, landmark)` is one call with no sign to get backwards —
  two real games have shipped a spawn yaw of `0.` or `PI` chosen by guessing, and guessed wrong both times.
- A game's `scripts/blue` needs `python3` or `python`; `scripts/check.py` finds a built `be2-tools` in the engine
  checkout by itself (build it with `cargo build --profile fast --no-default-features --bin be2-tools`).
- Build and test fast: `cargo build --profile fast`, `cargo test --profile itest` (see `docs/perf`).
