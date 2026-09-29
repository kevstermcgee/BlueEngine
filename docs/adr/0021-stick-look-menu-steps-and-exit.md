# ADR 0021: Stick look in the shared convention, discrete menu steps, and a graceful exit

Status: Accepted

## Context

A second round of *Eventide Sands* feedback (controller support) found three more places where an
agent that cannot hold a controller writes something plausible that a human then finds wrong or heavy:

- **Stick look felt mirrored.** `GamepadFrame::look_delta` was already correct (`[right, down]`: stick
  right turns right, stick up looks up), but its doc said only "positive stick Y looks up" and never
  named the delta convention, so a game whose own camera used a mirrored forward vector
  (`(-sin yaw, .., -cos yaw)`) applied it with the wrong handedness.
- **Menus need events, not axes.** Turning a stick into "move down one row" takes a threshold,
  hysteresis, an initial repeat delay, a repeat rate and edge tracking. Each game rewrote it, and the
  stock `ClientInput::navigation()` only used D-pad edges.
- **No way to quit cleanly.** The only exit a menu button had was `std::process::exit`, skipping
  destructors and any network goodbye.

## Decision

- `devkit::stick_look` / `FpsCamera::turn_stick` express the stick in the ADR 0020 vocabulary;
  `GamepadFrame::look_delta` now *is* `stick_look`, and tests pin that stick right turns towards +X and
  stick up looks up. The module documentation says what to check when a stick feels mirrored
  (use `FpsCamera::forward`, not a hand-written vector). `STICK_RADIANS_PER_SECOND` (2.5) is unchanged.
- `devkit::MenuNav` / `MenuStep` (no device dependency): flick at 0.55, release below 0.35, repeat after
  0.40 s then every 0.12 s, the D-pad wins over the stick, the dominant axis wins on a diagonal.
  `GamepadFrame::{menu_step, menu_select, menu_back, dpad}` bind it to a pad (South confirms, East goes
  back; whichever stick is pushed further steers). `ClientInput` runs one `MenuNav` during `poll`, so
  `navigation()` and the paused-shell actions now step with a flicked stick and repeat, and
  `ClientInput::{menu_step, menu_select, menu_back}` serve custom menus and sliders. Mouse-hover versus
  pad-focus highlighting is unchanged (both may show; a click or an accept acts on its own target).
- `game_client::{request_exit, exit_requested}`: a flag the loop checks at the top of a frame and
  breaks on, so `main` returns normally. `examples/minimal_game.rs` shows it (Q).

Not adopted: a single crate shared by the engine workspace and the games repository through Cargo
workspace membership. The duplicate the feedback describes is a repository-layout problem, not an
engine API: a game has one source of truth, either it lives in the engine and `games-publish.json`
copies it into `BlueEngineGames`, or it lives in `BlueEngineGames` and is listed under `preserve` with no
copy in the engine. Keeping both edited by hand is the failure to avoid. Making the engine a
Cargo workspace over `games/*` would tie the engine's lock file and CI to every game.

## Consequences

- A stick that "feels wrong" is now a camera-vector bug with a named remedy, not an engine ambiguity.
- The shared menus gain stick navigation with repeat; a game that hand-rolled its own keeps working.
- `exit_requested` is process-wide state, like the other window-loop helpers in `game_client`.
