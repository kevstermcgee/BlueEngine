# ADR 0015: A custom-simulation road alongside GameDocument

Status: Accepted

## Context

`GameDocument` (ADR 0007) is a bounded, declarative ruleset: counters, interactables,
timers, trigger zones. It deliberately does not grow into scripting. A game with
enemies, projectiles, scoring, AI, waves or per-frame physics does not fit that
shape, and the only path for it was a hand-rolled window loop copied from the stock
client each time: frame timing (`macroquad::get_frame_time()` is bumpy under vsync),
fixed-step accumulation, one input per tick, capture/script/perf flags for an agent
that cannot play, screen-feel (shake, hit-stop, flash), save files, batching, HUD
text and sound playback were all rewritten per game. A real one built this way
(Bouncer, a ring-out arena shooter) reported nine such rewrites as the concrete
evidence for this decision.

## Decision

Add two independent, optional modules and a second scaffold template, rather than
extending `GameDocument`.

- `viewer::devkit` (headless, always compiled — no macroquad, no audio device, no
  wall clock inside it; `tools/check_headless.py` enforces this): `FrameClock` (the
  wall-clock frame length), `FixedStepper` (whole 60 Hz ticks, bounded 8-tick
  catch-up, render `alpha`), `InputAccumulator` (device frames in, exactly one
  input per tick out, edges delivered once), the `Simulation` trait plus
  `StateHasher`/`assert_deterministic`/`run_inputs` (the determinism contract and
  its replay test), `Playback`/`Timeline`/`CapturePlan`/`PerfReport` (the
  `--script`/`--capture`/`--perf` flags an agent needs since it cannot watch a
  window), seeded `Rng`, `Juice`/`Pulse` (screen feel), `Settings`/`Records`
  (atomic never-fatal files), `synth` (procedural sound effects and a music loop,
  entirely computed so an agent with no ears can still ship audio; see ADR 0016
  for `Snapshot`, added in the same pass as save states).
- `viewer::kit` (opt-in, presentation-only): `View`/`Template`/`Batch`/`Tint`
  (small meshes batched into few draw calls per frame), `Look`/`Materials` (lit,
  fogged, glowing world material with presets), `Fx`/`hud`/`SoundBank`, and
  `capture::save_frame`.
- `templates/custom-sim`: a second `new-game` template (`new-game NAME DIR
  ENGINE_PATH custom-sim`) — a pure, seeded simulation library plus a window
  binary built on `devkit` and `kit` — alongside the existing stock
  (`GameDocument`) template. `docs/GAME_QUICKSTART.md` states the decision rule
  between the two starters.

`Controller` gained an optional floor (`set_floor(None)` for voids and pits, in
place of an implicit y = 0 ground), settable gravity, and `apply_impulse` (a
decaying external push) so a custom simulation's knockback and arenas-with-no-floor
do not need to reimplement movement. `mesh::Lighting` (with `house()`, `sun()`,
`flat()`) makes the baked static-world light swappable instead of hard-coded.
`ClientInput` now carries its own `FrameClock`.

## Consequences

Every piece is independent and optional: a game can use `devkit` without `kit`
(a text-only or 2D game), or neither (the stock `GameDocument` path is unchanged).
`devkit` staying graphics-free means a custom simulation's rule tests, determinism
tests and save-state tests run with no window and no audio device, on Linux CI.
The kit and devkit carry no genre logic (no health, no weapons, no wave spawner):
they are the parts every such game rewrites, not a game.

## Rejected or deferred

- **Do not grow `GameDocument` into a scripting language.** It stays a bounded,
  auditable ruleset; a game whose rules do not fit it uses `custom-sim` instead of
  pushing the declarative format past its design.
- **No genre modules** (no `HealthKit`, `WeaponKit`, `EnemyKit`). The feedback
  asked only for the parts every such game rewrites; genre-specific behavior
  belongs in the game.
- **`Simulation`/kit are not mandatory** for a custom loop. A game may implement
  its own `main.rs` from scratch; `custom-sim` is a starting point, not a boundary
  the engine enforces.
- **No bundled art or music** beyond `synth`'s procedurally computed effects. The
  engine does not ship sample packs.
