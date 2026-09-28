# Custom visible clients and custom simulations

Two different needs, decided by who owns the rules:

| You need | You own | Start from |
|---|---|---|
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
`Sim::step(&Input)`, `drain_events()`, a state hash), `src/main.rs` (the window), `tests/determinism.rs`,
and the same identity, icon, packaging and ship gate as the stock starter. Copy its loop:

```rust
input.begin_frame_with_keyboard(&mut shell, capture_cursor, focused, platform::keyboard());
let dt = input.frame_seconds();                       // wall-clock frame length, not get_frame_time()
acc.feed(held, edges, look);                          // device state in (InputAccumulator)
for _ in 0..stepper.advance(dt) {                     // whole 60 Hz ticks out (FixedStepper)
    let tick = acc.take_tick();                       // exactly one Input per tick, edges delivered once
    sim.step(&Input::from(tick));
    for event in sim.drain_events() { react(&event); }  // sound, particles, shake: an exhaustive match
}
draw(&sim, stepper.alpha());                          // camera = simulation pose + acc.pending_look()
```

The engine provides the parts every such game rewrites, each independent and optional:

| Module | Piece | Job |
|---|---|---|
| `devkit` (headless) | `FrameClock`, `FixedStepper`, `InputAccumulator` | frame timing, fixed ticks, one input per tick |
| | `Simulation`, `StateHasher`, `assert_deterministic`, `run_inputs` | the deterministic-state contract and its replay test |
| | `Playback`, `Timeline`, `CapturePlan`, `PerfReport`, `flag_value` | `--playback`, `--script`, `--capture`, `--perf` flags for an agent that cannot play |
| | `Rng`, `Juice`, `Pulse`, `Settings`, `Records`, `store_atomic` | seeded random numbers, screen feel, atomic never-fatal settings and high-score files |
| | `Snapshot`, `snapshot::{save_to_slot, load_from_slot, autosave, assert_resumes_exactly}` | F5/F9 save states: atomic files, backups, migrations, all-or-nothing loads, and their proof ([SAVE_STATE.md](SAVE_STATE.md)) |
| | `synth` | oscillators, filters, envelopes, WAV, ready-made effect presets, a music-loop helper |
| `kit` (`presentation`) | `View`, `Template`, `Batch`, `Tint` | camera; small meshes built once, batched into a few draw calls per frame |
| | `Look`, `Materials` | lit + fogged + glowing world material, alpha and additive effects, sky |
| | `Fx`, `hud`, `SoundBank`, `capture::save_frame` | particles and rings, scaled outlined text and panels, off-thread sound, screenshots |
| `controller` | `set_floor(None)`, `set_gravity`, `apply_impulse`, `Collider::overlaps_body` | voids and pits, other gravity, knockback and dashes, the body-overlap test |
| `mesh` | `Lighting`, `bake_with` | bake a static world with your own light (`Lighting::house()` is the stock look) |

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
