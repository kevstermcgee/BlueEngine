# ADR 0020: One mouse-look convention, checkable scale, and a dev loop that survives a running game

Status: Accepted

## Context

Feedback from building *Eventide Sands* on the custom-simulation road (2026-09-29) named the largest
recurring costs of an agent making a first-person game:

- **Mouse look took several rounds of a human testing it.** An agent cannot feel a mouse, and the
  repository held contradictory formulas: `ClientInput::mouse_look`, `blue-engine` and the sandbox
  agreed with each other, `templates/multiplayer-game` had the vertical axis inverted, and none of them
  said why. The cause is that three conventions meet: operating systems report +x right, +y down;
  macroquad's `mouse_delta_position()` is *previous minus current* in half-screens (so +x is left and
  +y is up); engine angles are yaw 0 = -Z, positive yaw turns towards +X, pitch up-positive.
- **Scale mistakes were only visible to a person.** A generated shell came out metres across; nothing
  failed and no tool said "this is 3 m long".
- **A rebuild failed with `Access is denied (os error 5)`** whenever the previous run of the game was
  still open, and each `scripts/blue play` was a 90-150 s release build.
- The remaining findings were already true or lived elsewhere: `docs/AI_QUICKSTART.md` and
  `examples/custom_client.rs` existed but no window example showed the kit end to end; the launcher
  overwrote its local catalog (a separate repository, fixed there).

## Decision

- **`devkit::look`** states the convention once, in graphics-free code that unit tests pin against
  `Controller::look`: a look delta is `[right, down]` radians; the hand moving right turns right, up
  looks up; `MouseLook` converts pixels (`invert_y`, sensitivity clamped to `SENSITIVITY_RANGE`,
  non-finite input ignored); `FpsCamera` applies a delta with the engine's pitch limit `PITCH_LIMIT`
  (1.5 rad, about 86 degrees; the controller and arena body already clamp there, and a different camera
  limit would let camera and simulation disagree). The recommendation of 89 degrees was not adopted for
  that reason: raising the simulation clamp changes recorded runs and saves.
- **`game_input::mouse_pixels()`** is the only place that reads `mouse_delta_position()` for new code
  and undoes its sign and unit quirks (`pixels_from_macroquad_delta` is the tested pure half). The
  multiplayer template's inverted pitch is corrected. The stock client's feel (2.5 rad per half-screen,
  resolution-relative) is unchanged.
- **`devkit::Bounds`** (`of`, `size`, `longest`, `describe`, `expect_longest`) with human-scale
  constants and `describe_length` ("about 0.5 x a person"), so a headless agent gets "conch: 3.00 x ...
  expected 0.05..0.30 m: scale by about 0.10" instead of a screenshot review. **`kit::gizmo`** draws
  `bbox`, `template_bbox`, `axes` and a 1.75 m `human_scale` silhouette for the same purpose by eye.
- **`examples/minimal_game.rs`**, about 100 lines: window, `FpsCamera` + `mouse_pixels`, meshes,
  collectable orbs, procedural coin sound, HUD, `--shot` for an agent. It is what to copy before the
  full `custom-sim` starter.
- **`scripts/dev.py`** (`scripts/blue dev`, and `play`) closes a running copy of this project's own
  binary before cargo runs, matched by full path so nothing else is touched. No new Cargo profile: the
  starter already builds the game at opt-level 2 and dependencies at 3, so only the game relinks per
  edit; the cost was using `--release` to iterate. A dynamic-library split was not adopted: Rust has no
  stable ABI, so it adds fragility for a saving `dev` already gets.

## Consequences

- New custom games get the same look direction without deriving it. A game that hand-rolls its own
  formula is still possible; `AGENTS.md` of the starter says not to.
- `PITCH_LIMIT` is a public constant other code may rely on.
- The launcher fix (merge local discovery into the remote catalog) is in `BlueEngineGames`,
  `launcher/EngineGamesLauncher.cs`, not here.
