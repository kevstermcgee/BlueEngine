# Rift Delver - AI Agent Guide

To start or resume work, use `python3 tools/be2.py start "<task>" --project GAME_DIR`
in the engine checkout (`python` on Windows); `next`/`resume TASK_ID` refreshes the packet.

A game with its own simulation on BlueEngine (path dependency `vesper3d`, see Cargo.toml). Read this
file and `src/lib.rs`. Missing engine capability is engine work, not permission to invent an API: in the
engine checkout run `python tools/be2.py context "<need>"`, and read docs/CUSTOM_CLIENT.md (the kit and
the loop) and docs/SHARED_GAMEPLAY.md ("Custom loops").

## Architecture (do not break)
- `src/lib.rs` is the game: pure, seeded (`devkit::Rng`), fixed 60 Hz `Sim::step(&Input)`. No window,
  sound device or wall clock. It reaches the window only through `Event`s (`drain_events`).
- `src/main.rs` is the window: `ClientInput` + `GameShell` for devices, `devkit::Lifecycle` for the run
  flags, one `Input` per fixed tick, quick save/load and capture/perf evidence, `kit` renderer/HUD/sound.
  Frame length is `input.frame_seconds()` through `life.begin_frame`, never macroquad's `get_frame_time()`
  (bumpy under vsync). Add a device or cue: extend `Held`, the vocabulary passed to `Lifecycle::start`
  and the two arms of the "devices in" match; nothing else in the loop changes.
- `Controller` has an implicit floor at y = 0 unless `set_floor(None)` (this starter uses none: the
  platform's collider is the only ground). Knockback is `apply_impulse`. Cannot see or hear the game?
  Use `--capture`, `--script`, `--perf` below, and numbers.
- **Music is optional, not required.** `main.rs`'s `HAS_MUSIC` turns the generated ambient background
  track (`synth::ambient_spec_for`, keyed to `assets/identity.json`'s title/tagline) on or off; the
  Settings screen adapts either way. Turn it off if music would fight a gameplay mechanic this game
  depends on (precise or diegetic audio, rhythm timing, a soundtrack the game is itself about) or just
  does not suit the feel — do not force it in because the starter ships with it on. Sound effects
  (`SOUNDS`/`render`) are unaffected by this flag either way.
- For authored audio, run the engine's `be2-tools audio describe` and read `docs/AUDIO.md`.
  Named JSON projects support presets/variants, imported PCM16, stereo note scores and adaptive layers.
  Render once before shipping; `kit::AudioBank::load` reads the bundle off-thread, and audio data edits
  require rerendering rather than a Rust rebuild. Poll each frame, inspect `state()`/`errors()`, and
  play named cues on confirmed events. Measure loops with `audio_report.py --loop`; native preview
  is `examples/audio_preview.rs`. Numeric checks do not establish perceived quality or hardware audibility.

## Look, scale, iteration
- Mouse look has one convention (`devkit::look`): hand right turns right, hand up looks up. Feed
  `MouseLook::look(px_right, px_down)` from `game_input::mouse_pixels()` into `FpsCamera` (or the same
  `[right, down]` delta into `Controller::look(dx, dy, 1.0, false)`). Never read `mouse_delta_position()`
  yourself (previous-minus-current: both signs flipped); never hand-write a yaw/pitch formula.
- Gamepad: `input.look_delta_with(&shell, dt)` already adds mouse and right stick in that convention;
  menus use `input.menu_step()` / `menu_select()` / `menu_back()` (flick + repeat built in). Quit with
  `game_client::request_exit()` and break on `exit_requested()`; never `process::exit`.
- Unit = metre. Check generated geometry without a window: `Bounds::of(points)?.expect_longest("shell",
  0.05..=0.30)` fails with the scale to apply; `kit::gizmo::human_scale`/`bbox` show it in a frame.
- Geometry bugs a screenshot hides (z-fighting kerbs, a road border folded at a tight bend, inside-out triangles) are
  caught headless: build the `Template` in a test and `kit::lint::assert_clean(&t, "name")` (needs the `client` feature);
  `Template::offset_strip` makes road/kerb ribbons that cannot fold, `View::camera_checked` warns about a far/near
  ratio over 3000. See docs/CUSTOM_SIM_CHEATSHEET.md in the engine checkout.
- Rebuild with `scripts/blue dev` (closes a still-open copy first: a running .exe cannot be relinked on
  Windows) and use `scripts/blue dev -- --capture out --frames 30`; a release build is for `ship`.

## Save states
- F5 / F9 save and load the `quick` slot (`devkit::snapshot`, wired by `devkit::Lifecycle` in `main.rs`):
  `impl Snapshot for Sim` in `src/lib.rs` lists everything that decides the future. **Every new field of
  `Sim` goes into `SimState`**, or a save loses it; `a_save_from_any_tick_resumes_as_the_game_promises...`
  fails at the first tick that reads what was forgotten, and the error names the `hash_parts` piece.
  Changing `SimState` after release: bump `VERSION` and add a `Migration` so old saves still load.
- `Sim::POLICY` is the save promise, in one place: `SavePolicy::Exact` (this starter) or
  `SavePolicy::PhysicsContinuation` for a `Sim` that embeds a rigid-body world (`HeadlessWorld`), whose
  resumed run is a pure function of the file but not bit-identical (docs/SAVE_STATE.md). The test proves
  whichever is declared; never weaken the policy to make a test pass for a non-physics field.
- Loads are all-or-nothing and saves are atomic with a backup; never write your own save file code.

## Checks
Record development friction in the engine with `python tools/learn.py record --game rift-delver --area AREA
--tokens N --note "..." --keywords "future,query,terms"` (add `--trap` for a silent failure).
Verify the lesson appears in `python tools/be2.py context "representative future query" --compact`.
Without keywords a record is archive-only. Do not copy session logs or credentials.

- `cargo test` (rules + determinism, headless). Add a test for every rule you add.
- Iterate with `python scripts/check.py --skip-ship`; `--content-only` needs no Cargo.
- Look at it: `target/debug/rift-delver --capture out --frames 30,120 --seed 3`, then
  `python <engine>/tools/contact_sheet.py sheet.png --dir out` (one image instead of many).
  `--script "fwd:0-200,jump@60"` drives the human input path; `--perf` prints frame-time percentiles.

## Definition of done (every game made with BlueEngine)
1. `python scripts/check.py` passes. It ends with the ship gate, so it fails until step 2 is done.
2. The game ships as a package with its own icon: fill in `assets/identity.json`
   (real title, tagline, controls), regenerate the icon if the title changed
   (`be2-tools icon TITLE assets --replace`, add a number for another design), then run
   `scripts/blue ship --no-install`. It builds a release package in `dist/`, verifies assets,
   executable resources and an isolated packaged-game smoke without desktop access.
   `scripts/blue ship` additionally creates a shortcut and checks its target and icon;
   on Windows it checks the launched window. Icon similarity is optional advisory only.
   Never point a shortcut at `target/`.
3. You looked at real frames of the shipped exe and exercised the controls; say what you did not verify.
