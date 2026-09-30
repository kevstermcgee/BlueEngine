# Custom visible clients and custom simulations

Two different needs, decided by who owns the rules:

| You need | You own | Start from |
|---|---|---|
| a first window, camera, meshes and sound in one file, to copy | presentation and a toy loop | `examples/minimal_game.rs` (`cargo run --example minimal_game`) |
| your own window, renderer, HUD or input over **the engine's** simulation and `GameDocument` rules | presentation | `examples/custom_client.rs` (below) |
| **your own rules**: enemies, projectiles, scoring, AI, per-frame physics | the simulation *and* the window loop | `be2-tools new-game NAME DIR ENGINE_PATH custom-sim` ([A custom simulation](#a-custom-simulation)) |

## Custom presentation over the stock simulation

Use a custom client when a game needs presentation or device handling beyond the stock
`be2 --game` client. BlueEngine owns validated scene data, deterministic 60 Hz
simulation, collision, game rules, and authoritative networking. The application owns
its window, renderer, input polling, HUD, and the translation from device state into
`Movement` and interaction requests.

Start from `examples/custom_client.rs`:

```sh
cargo run --locked --example custom_client
```

For an external project in a sibling directory, use the supported renderer version
explicitly; applications and BlueEngine may then exchange Macroquad types without a
second-version type mismatch:

```toml
[dependencies]
vesper3d = { package = "be2", path = "../BlueEngine", default-features = false, features = ["client"] }
macroquad = { version = "=0.4.14", default-features = false, features = ["audio"] }
```

Enable `client` for the shared visible client; use no default features for a rendering-free host. Submit held movement every display frame, preserve jump and
interaction as press edges, and advance `HeadlessWorld::step` in fixed 1/60-second
increments. Cap catch-up work after long frames. Read copied public state such as
`player`, `prop_position`, and `GameRuntime::state`; do not mutate results to make
tests pass.

The example uses the shared MapPlayer static renderer, input, camera, characters and menus. Use `be2 --game` when document-driven presentation
is sufficient, including replicated target visibility and movers.

Playable game presentation follows [the shared presentation contract](GAME_PRESENTATION.md). Use the `presentation` feature for `GameShell` and cached static rendering.

## A custom simulation

`GameDocument` deliberately has no scripting. When a game's rules are code, the code is the authority:
a pure library, fixed 60 Hz, seeded, no window, no sound device, no wall clock (the same contract the
engine asks of its own `HeadlessWorld`). The window only *reads* it and reacts to its events, so there
is still exactly one authoritative loop (the presentation contract forbids a *second* one in the stock
client, which presents `GameSession` state; it does not apply to a game that owns its only authority).

`be2-tools new-game NAME DIR ENGINE_PATH custom-sim` generates the shape: `src/lib.rs` (rules:
`Sim::step(&Input)`, `drain_events()`, a state hash, a save policy), `src/main.rs` (the window),
`tests/determinism.rs`, and the same identity, icon, packaging and ship gate as the stock starter. The
window's lifecycle (run flags, whole ticks with one input each, F5/F9, `--load`, capture and perf evidence)
is `devkit::Lifecycle`; `main.rs` keeps the drawing and the mapping from devices and cues to its held state:

```rust
let mut life = Lifecycle::<Held>::start_or_exit(&args, &CUES); // --seed/--script/--capture/--load/--perf/--mute
life.load_flag_or_exit(&mut sim);                              // --load SLOT_OR_FILE, before the first frame
loop {
    input.begin_frame_with_keyboard(&mut shell, capture_cursor, focused, platform::keyboard());
    let dt = life.begin_frame(input.frame_seconds());          // wall clock for a person; one tick unattended
    let (held, edges, look) = match life.script() {            // cues (--script) or devices in
        Some(s) => (Held { forward: s.axis("fwd", "back"), .. }, edges(s.starts("jump")), [s.value("look", 0), s.value("look", 1)]),
        None => devices(&input, &shell, dt),
    };
    life.feed(held, edges, look);
    let (save, load) = life.save_load_requested(input.pressed(KeyCode::F5), input.pressed(KeyCode::F9));
    if save { show(life.quick_save(&sim, "Quick save")); }     // a Notice: title, detail, colour, ok
    if load { show(life.quick_load(&mut sim)); }               // pending input and leftover time are dropped
    for _ in 0..life.ticks(dt, juice.time_scale(None), !shell.paused) { // whole 60 Hz ticks
        let tick = life.take_tick();                           // exactly one Input per tick, edges once
        sim.step(&Input::from(tick));
        for event in sim.drain_events() { react(&event); }     // sound, particles, shake: an exhaustive match
    }
    draw(&sim, life.pending_look());                           // camera = pose + look no tick consumed yet
    if let Some(path) = life.capture_path() {                  // --capture: the caller writes the file
        life.captured(&path, kit::capture::save_frame(&path).map_err(|e| e.to_string()));
    }
    if life.end_frame(dt) { break; }                           // the capture plan is finished
    next_frame().await;
}
if let Some(report) = life.report() { println!("{report}"); } // --perf
```

The engine provides the parts every such game rewrites, each independent and optional:

| Module | Piece | Job |
|---|---|---|
| `devkit` (headless) | `Lifecycle`, `Options`, `Notice` | the whole window lifecycle composed from the rows below: flags, ticks, F5/F9, `--load`, capture and perf evidence |
| | `FrameClock`, `FixedStepper`, `InputAccumulator` | frame timing, fixed ticks, one input per tick |
| | `Simulation`, `StateHasher`, `assert_deterministic`, `run_inputs` | the deterministic-state contract and its replay test |
| | `Playback`, `Timeline`, `CapturePlan`, `PerfReport`, `flag_value` | `--playback`, `--script`, `--capture`, `--perf` flags for an agent that cannot play |
| | `Rng`, `Juice`, `Pulse`, `Settings`, `Records`, `store_atomic` | seeded random numbers, screen feel, atomic never-fatal settings and high-score files |
| | `Snapshot`, `snapshot::{save_to_slot, load_from_slot, autosave, assert_resumes_exactly}` | F5/F9 save states: atomic files, backups, migrations, all-or-nothing loads, and their proof ([SAVE_STATE.md](SAVE_STATE.md)) |
| | `SavePolicy`, `snapshot::assert_resumes_as_promised`, `snapshot::{assert_loads_replay_identically, assert_resumes_within}`, `Simulation::hash_parts` | the save promise a game declares in one place (`Exact`, or `PhysicsContinuation` for a rigid-body world), the proof that follows it, and a load error that names the forgotten field |
| `simulation` | `HeadlessWorld` (via `SceneBuilder`, `without_spawn()` for a props-only world) | rigid-body props as the physics authority inside your own rules (below) |
| | `synth` | oscillators, filters, envelopes, WAV, ready-made effect presets, a music-loop helper |
| `kit` (`presentation`) | `View`, `Template`, `Batch`, `Tint` | camera; small meshes built once, batched into a few draw calls per frame |
| | `Look`, `Materials` | lit + fogged + glowing world material, alpha and additive effects, sky |
| | `Fx`, `hud`, `SoundBank`, `capture::save_frame` | particles and rings, scaled outlined text and panels, off-thread sound, screenshots |
| `controller` | `set_floor(None)`, `set_gravity`, `apply_impulse`, `Collider::overlaps_body` | voids and pits, other gravity, knockback and dashes, the body-overlap test |
| `mesh` | `Lighting`, `bake_with` | bake a static world with your own light (`Lighting::house()` is the stock look) |

**Rigid-body props in your own rules.** A custom `Sim` may embed the engine's `HeadlessWorld` as its physics
authority: build it with `SceneBuilder` (`without_spawn()` when the player is not a physical body in it, or
`spawn` and `join` when it is), keep it in `Sim`, step it once per tick from `Sim::step`, read props by ID
(`prop_position`, `prop_linear_velocity`, `prop_mass`, `throw`, `set_prop_floor(None)` for a pit), hash it
with `checksum()`, and put `save_state()` in your `Snapshot::State` (`restore_state` puts it back,
all-or-nothing). It is graphics-free, fixed-step and deterministic, so it belongs in the library, not the
window. The `HeadlessWorld` docs show exactly that `Sim` as a compiled example, and `tests/physics_saves.rs`
is one with twenty rolling props under the save-contract helpers. Its saves meet the physics contract in
[SAVE_STATE.md](SAVE_STATE.md#which-contract-a-physics-game-can-meet), not the exact one.

Traps met while building a real game on this (see the generated `AGENTS.md`): `Controller` floors at
y = 0 unless `set_floor(None)`; the third argument of `begin_frame` decides cursor capture (pass `false`
on menu and game-over screens); `ClientInput::look_delta` adds mouse and stick (`mouse_look` and
`stick_look` are separate when you want your own stick sensitivity); an `eprintln!` inside the frame
loop can make the next frame slow and cascade, so collect diagnostics and print at exit.

Verify without watching: `--capture DIR --frames 30,120` (a new directory; the loop also prints the
real pixel size of every capture), `--script "fwd:0-200,jump@60"` (the human input path), `--seed N`,
`--perf`. `python tools/contact_sheet.py sheet.png --dir DIR` folds many captures into one image and flags
blank frames; `python tools/audio_report.py FILE.wav` reports clipping, clicks, DC offset and spectrum
(numbers are the honest ceiling: nobody can listen).

See [the shared gameplay kit](SHARED_GAMEPLAY.md) for the playable starter, public creative APIs, feature gates and runtime boundaries.

## Local lights and planar mirrors

`kit::PointLight::new(position, radius, rgb, intensity)` validates finite positive radius and
nonnegative RGB/intensity. Call `Materials::set_point_lights(&lights)` **after** `set_scene`,
for each camera pass. At most four unshadowed lights use Lambert diffuse with squared
finite-radius falloff. Unused slots are cleared; oversized lists fail without changing uniforms.
`set_scene` clears all lights so existing clients keep the same look. This is additive diffuse
lighting, not PBR or shadow mapping.

`MirrorPlane::new(center, right, up, size)` validates a rectangular aperture with perpendicular
axes; `right × up` points to the viewer side. `PlanarMirror::new(plane, (width,height))` requires
GL initialization and owns its target, depth buffer, camera adapter and upright UVs. Resolution
is bounded at 2048 per axis. `mirror.camera(eye, far)` returns None behind/within 2 cm of the plane
or when the far range is too short. Otherwise set that camera, clear, render all desired world
geometry **excluding mirror surfaces**, and set lighting with `camera.eye`. Restore the main
camera, render the main scene, then call `mirror.draw_surface()` only if the reflection ran.
The plane-aligned near clip excludes geometry behind the mirror; the off-axis frustum follows
the aperture and gives sideways-motion parallax. Render targets have fixed world orientation,
so sky rendering needs its own direction handling. There is no recursive, rough, refractive or
shadowed reflection. Each visible mirror costs one additional world render.

The separate Physics Lab game exercises these APIs with
fire flicker, burning sample lights, a moving blue light and a vertical walk-up mirror. Its
thermal/fluid model remains game-owned, approximate and documented, rather than an engine
continuum-fluid solver.
