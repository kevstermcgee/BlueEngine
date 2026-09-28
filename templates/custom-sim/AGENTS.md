# {{title}} - AI Agent Guide

A game with its own simulation on BlueEngine (path dependency `vesper3d`, see Cargo.toml). Read this
file and `src/lib.rs`. Missing engine capability is engine work, not permission to invent an API: in the
engine checkout run `python tools/be2.py context "<need>"`, and read docs/CUSTOM_CLIENT.md (the kit and
the loop) and docs/SHARED_GAMEPLAY.md ("Custom loops").

## Architecture (do not break)
- `src/lib.rs` is the game: pure, seeded (`devkit::Rng`), fixed 60 Hz `Sim::step(&Input)`. No window,
  sound device or wall clock. It reaches the window only through `Event`s (`drain_events`).
- `src/main.rs` is the window: `ClientInput` + `GameShell`, `InputAccumulator` -> one `Input` per tick,
  `FixedStepper`, `kit` renderer/HUD/sound. Frame length is `input.frame_seconds()`, never macroquad's
  `get_frame_time()` (bumpy under vsync). Capture/script/perf flags are already wired.
- `Controller` has an implicit floor at y = 0 unless `set_floor(None)` (this starter uses none: the
  platform's collider is the only ground). Knockback is `apply_impulse`. Cannot see or hear the game?
  Use `--capture`, `--script`, `--perf` below, and numbers.

## Save states
- F5 / F9 save and load the `quick` slot (`devkit::snapshot`): `impl Snapshot for Sim` in `src/lib.rs` lists
  everything that decides the future. **Every new field of `Sim` goes into `SimState`**, or a save loses
  it; `a_save_from_any_tick_resumes_exactly...` fails at the first tick that reads what was forgotten.
  Changing `SimState` after release: bump `VERSION` and add a `Migration` so old saves still load.
- Loads are all-or-nothing and saves are atomic with a backup; never write your own save file code.

## Checks
- `cargo test` (rules + determinism, headless). Add a test for every rule you add.
- Iterate with `python scripts/check.py --skip-ship`; `--content-only` needs no Cargo.
- Look at it: `target/debug/{{name}} --capture out --frames 30,120 --seed 3`, then
  `python <engine>/tools/contact_sheet.py sheet.png --dir out` (one image instead of many).
  `--script "fwd:0-200,jump@60"` drives the human input path; `--perf` prints frame-time percentiles.

## Definition of done (every game made with BlueEngine)
1. `python scripts/check.py` passes. It ends with the ship gate, so it fails until step 2 is done.
2. The game ships as a package with its own icon and desktop shortcut: fill in `assets/identity.json`
   (real title, tagline, controls), regenerate the icon if the title changed
   (`be2-tools icon TITLE assets --replace`, add a number for another design), then run
   `scripts/blue ship`. It builds a release
   package in `dist/`, creates the shortcut named after the game, and verifies target, icon
   uniqueness among the desktop's shortcuts, window title and icon. Never point a shortcut at `target/`.
3. You looked at real frames of the shipped exe and exercised the controls; say what you did not verify.
