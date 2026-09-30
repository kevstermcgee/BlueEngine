# Behavioral testing without a custom harness

The native `sim` command drives the real public input boundary at deterministic 60 Hz and emits
structured assertion outcomes. Scenario paths are portable: `game_path` is resolved
relative to the scenario file, not the current shell directory.

```sh
cargo run --locked --no-default-features --bin be2-tools -- \
  sim assets/games/three-switches/scenario.json trace.json
cargo run --locked --no-default-features --bin be2-tools -- replay-trace trace.json
```

A scenario declares players, timestamped held movement/look values and press-edge
`jump`/`interact` commands. Assertions can inspect a player position, a named counter,
completion, and a target's independent eligibility and visibility state. A failed
assertion produces JSON with `ok: false`, returns a nonzero exit status, and still
runs the remaining ticks so the report retains useful evidence.

Use `sim` for external-project acceptance tests before writing Rust helpers. Use the
public `scenario::evaluate_scenario` API when a Rust test needs the same structured
report in-process. Replay traces detect divergence in deterministic engine state; they
do not replace assertions about intended game behavior.

## Writing a scenario as intent

Hand-timed movement (`right: 1.0` for 38 ticks, then stop) is fragile. Two inputs say what you mean instead:

```json
[
  {"tick": 1, "player": 1, "walk_to": [-1.0, 3.0]},
  {"tick": 1, "player": 1, "face": "button-b", "interact": true}
]
```

* `walk_to: [x, z]` walks the player to that point (metres) and stops within about 0.15 m. Later inputs for the same
  player are held until it has arrived and settled, so the press above happens after the walk, not during it. A walk
  that never arrives (a wall in the way) is reported as a failure after 1800 ticks instead of hanging. Without `face`
  the player keeps the heading it has.
* `face: "entity-id"` looks at the centre of that entity from where the player's eyes are now. Interaction needs
  line of sight within 2.5 m, so stand in range and `face` the target rather than working out a yaw.

`sim` also reports where each player ended up (`players`, eye position), so a walk can be checked without guessing.
`tick` stays the earliest tick an input may run; inputs without `walk_to`/`face` behave exactly as they always did.

## Getting the first scenario for free

`game-explore` finds the shortest way to win a game (see the game quickstart). With `--scenario` it also plays that
win and writes it as a scenario:

```sh
be2-tools game-explore game.json --scenario=win.json
be2-tools sim win.json
```

It walks to each target, faces it and presses it, and where the rules need time to pass (a switch that only works
in one phase of a cycle) it waits until pressing does what the plan expects. The scenario is run through the same
runner as `sim` before it is reported as verified, and an existing file is never overwritten. Paths that go through
trigger zones are not generated yet. Treat the file as the starting point: add the assertions and the failing and
out-of-order cases yourself.
