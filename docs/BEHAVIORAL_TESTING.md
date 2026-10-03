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
* `route: true` on a `walk_to` input uses A* waypoints around the world's **current** colliders and the active
  controller profile. It replans every 30 executed steps, waiting when a moving gate still blocks the route,
  within the same 1800-step walk timeout. The default remains direct steering. A route candidate is not proof
  of physical completion: the shared simulation must actually arrive and satisfy the assertions.
* `wait_ticks: 1` on each repeated press holds later inputs until another simulation step. Different timestamps
  alone are insufficient: several overdue presses can become due together after a blocking walk, and authority
  intentionally coalesces interaction intentions in one tick. `wait_ticks` accepts 0..60000 executed steps.

`sim` also reports where each player ended up (`players`, eye position), so a walk can be checked without guessing.
`tick` stays the earliest tick an input may run; inputs without `walk_to`/`face` behave exactly as they always did.

* `restart: true` restarts the match the way a player pressing the action button does once it is won or lost: everything
  resets (counters, timers, movers, `once` rules) and players respawn. Restarting a match that is still running is
  reported as a problem and fails the scenario, so it cannot pass by restarting at the wrong moment.

## Getting the first scenario for free

`game-explore` finds the shortest way to win a game (see the game quickstart). With `--scenario` it also plays that
win and writes it as a scenario:

```sh
be2-tools game-explore game.json --scenario=win.json
be2-tools sim win.json
```

It routes to each target using current collision, faces it and presses it, and where the rules need time to pass (a switch that only works
in one phase of a cycle) it waits until pressing does what the plan expects. The scenario is run through the same
runner as `sim` before it is reported as verified, and an existing file is never overwritten. Paths that go through
trigger zones are not generated yet. The file is named, and the command reports it, as **one winning route only**:
it shows the rules can be won once in the order the planner found, and says nothing about losing, restarting,
retrying after a mistake, pressing in the wrong order or timer variation. Treat it as the starting point: add the
assertions and the failing and out-of-order cases yourself.

The [observatory fixture](../assets/games/observatory/README.md) demonstrates an initially closed shutter,
a turn around a baffle, three calibration presses, battery failure, restart and a successful run.
Its never-opening-shutter variant is abstractly winnable but reports `scenario.verified:false`, with no written
winning scenario. The command's `ok` describes abstract exploration; inspect the separate physical result.

To inspect real stock frames without converting intents into hand-timed movement:

```sh
be2 --game assets/games/observatory/content/game.json --scenario assets/games/observatory/content/loss-restart-win.json --capture NEW_DIRECTORY
```

This verifies the scenario before rendering, then executes the same driver against local `GameSession` authority.
It requires one player (id 1), default spawn, matching game content, and no `--connect`, `--server`, `--load`
or `--playback`. Captures include world, loss, restart, win and menu frames when those transitions occur.
It demonstrates rendered scripted behavior; it does not establish live keyboard/controller or network behavior.

## Gate reachability in content checks

Static `lint MAP` and `reach MAP` describe initial map collision. For an authored gate, pass a verified physical
scenario as evidence instead of exempting movers:

```sh
be2-tools lint map.json --game=game.json --scenario=win.json
```

Game-aware lint runs the shared simulation for the exact game/map contents, using authored spawns (scenario
spawn overrides are refused). It resolves an initial `unreachable` error only when that individual enabled
target was observed within authoritative interaction reach and line of sight. The finding remains as
`initial-unreachable-now-reached` information, naming the demonstrated scenario. Other targets, inaccessible
movers and failed/mismatched scenarios remain errors or unresolved. This proves a selected run, not every state.
`sim` exposes `reachable_targets` for the same evidence; this reads authority without changing rules.

Generated `scripts/check.py --scenario tests/win.json --scenario tests/loss-restart-win.json` uses the first
chosen scenario for game-aware lint when the GameDocument has movers, then executes all scenarios normally.
Without a scenario it keeps static lint. Put a route demonstrating required targets first; a passing loss-only
scenario does not establish their reachability. Existing games need no change unless opting into this evidence
path; copy the updated engine template/check script (or use the engine's supported upgrade workflow).

## Four different claims

Keep these apart when you say a game "works":

1. **Valid document** (`game-validate`): the JSON is well formed and every reference resolves.
2. **Reachable in the abstract** (`game-explore`): some sequence of rule events wins, and no rule is dead. It models rule
   state, not walking, line of sight or exact timing.
3. **Playable once** (a generated or hand-written scenario that wins): that route, through the real runtime.
4. **Robust** (scenarios for the paths players actually take): losing, restarting and winning again, wrong order, retrying
   after a mistake, repeated timer cycles and the once-per-match rule.

`tests/game_failure_paths.rs` is a worked example of the fourth kind: an ordered three-button puzzle with a 600-tick
deadline, played through `evaluate_scenario` (the same runner as `sim`). It loses on the timer and shows the match
freezes; restarts and wins again (the `once` bonus is available again because a restart is a new match); presses the exit
early and out of order; retries after a mistake (the `once` bonus is not given twice in one match); and counts repeating
timer cycles. Its last two tests are mutation checks: the same assertions must fail against a game that lacks the
deadline or reset rules, so they cannot pass for the wrong reason. It does not explore time or physical states
exhaustively; each scenario is one chosen path.
