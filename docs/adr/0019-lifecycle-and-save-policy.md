# ADR 0019: One lifecycle for custom-simulation windows, and a save policy declared per game

Status: Accepted

## Context

The `custom-sim` starter's `main.rs` composed the devkit pieces by hand: parsing
`--seed`, `--script`, `--capture`, `--load`, `--save-dir`, `--perf` and `--mute`,
choosing a fixed tick for unattended runs, feeding the input accumulator from
either the script or the devices, detecting `save@N`/`load@N` cues or F5/F9,
writing quick-save and quick-load banners, resetting the stepper and accumulator
after a load, the tick loop, the capture schedule with its JSON evidence line,
the perf report, and exit status 2 with a printed reason for a bad flag. About a
quarter of the file, copied into every generated game and drifting from the next
one the moment a flag changed. Three games built on the starter (ADR 0018) each
carried that copy, and one of them found `assert_resumes_exactly` could not pass
because its simulation embedded a rigid-body world; the fix landed the weaker
helpers but left the choice between them implicit in whichever the game's test
happened to call.

## Decision

- **`devkit::Lifecycle<H>`** owns the window lifecycle around a simulation, built
  only from pieces devkit already had (`CapturePlan`, `Timeline`, `FixedStepper`,
  `InputAccumulator`, `PerfReport`, `SaveSlots`, `snapshot`): `start` parses the
  flags into `Options` and creates the capture directory; `load_flag` resumes
  `--load`; `begin_frame` picks the wall clock or one fixed tick; `script` exposes
  the frame's cues (`ScriptFrame::{axis, held, starts, value}`) so the device
  mapping is a few lines; `feed`, `ticks`, `take_tick`, `pending_look` are the
  one-input-per-tick loop; `save_load_requested`, `quick_save` and `quick_load`
  return a `Notice` for a banner and drop pending input and leftover time after a
  load; `capture_path`, `captured` and `end_frame` produce the screenshot and
  frame-time evidence and say when to exit; `report` is the `--perf` text. It is
  graphics-free: the caller writes the screenshot with `kit::capture::save_frame`.
  `main.rs` keeps the rendering, the sound reactions and the two-arm match that
  maps cues or devices to its own `Held`.
- **`SavePolicy`** (`Exact` or `PhysicsContinuation`) is declared once per game as
  `Snapshot::POLICY`, written into every save, shown by `be2-tools save-info FILE`, and
  proved by `snapshot::assert_resumes_as_promised`, which the starter's save test
  calls. Declaring the policy is what changes what the test demands; the specific
  helpers stay available for a game that also promises a drift bound.

## Consequences

A generated game's lifecycle behaves the same under `--script`, `--capture`,
`--load` and `--perf` as every other one, and a fix to that behaviour lands in
the engine instead of in each copy. The starter's `main.rs` is shorter and what
is left is the game. The physics-save policy is a visible, reviewable line rather
than a test choice: a reviewer sees `PhysicsContinuation` and knows the game
embeds a rigid-body world, and the documentation says the policy is a promise
about physics only, never a way to hide a forgotten field.

## Rejected or deferred

- **A full game runner that also owns the window and drawing**: the presentation
  contract keeps the window the game's; `Lifecycle` stops at the edge of graphics.
- **A drift tolerance inside the policy**: a bound needs the game's own distance
  measure, so it stays an explicit `assert_resumes_within` call.
- **Refusing a save whose recorded policy differs from the game's**: the policy
  is a promise about resuming, not a compatibility key; the content fingerprint
  and payload version already decide what loads.
