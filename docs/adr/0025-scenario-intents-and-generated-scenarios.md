# ADR 0025: Scenarios that say what they mean, and a generated first scenario

Status: Accepted

## Context

Thirty scenarios for the ten minigames (see ADR 0023, 0024) were written with a hand-built planner: walk speed
measured in metres per tick, a fixed yaw, and a ten-tick settle after every hop. `sim` reported no positions, so
even checking a walk meant recovering coordinates from assertion errors. Meanwhile `game-explore` could name the
shortest win but not turn it into a test.

## Decision

* `TimedInput` gains `walk_to: [x, z]` and `face: "entity"`. An `InputDriver` in `viewer::scenario` applies them,
  and both the assertion runner and the replay verifier use it, so they cannot disagree. A walk steers toward the
  point (eased over the last half metre) and finishes within 0.12 m; later inputs for that player are held until it
  has arrived and settled for 8 ticks; a walk that has not arrived after 1800 ticks is a reported problem.
* Inputs that use neither are applied exactly as before, and a test compares them with the old loop's checksum.
* `sim` prints the final eye position and yaw of each player.
* `game-explore --scenario=OUT.json` (`viewer::game_scenario`) records a scenario that plays the shortest win. It
  does not replay the abstract plan blindly: before each press it asks the model what pressing would do to the state
  the game is really in and presses once the same rules fire, waiting otherwise. It tries several ways of standing
  in front of each target, and only reports a scenario after the ordinary runner has passed it.

Not adopted:

* **Exact state matching against the plan.** Timers run while the player walks (a relic decays), so the real state
  drifts from the model's; the rules a press fires are the part that has to match.
* **Zone paths.** Walking into a zone and out again needs occupancy tracking that the generator does not model yet.
* **Emitting failing and out-of-order scenarios.** The generator writes the win; the cases that matter for a
  particular game are the author's to choose.

## Consequences

* A scenario is a few lines of intent, and a new game gets its first passing scenario from one command.
* `TimedInput` and `Assertion` gain fields and now skip default values when written, so generated files stay small;
  older files parse unchanged.
* The generated scenario depends on where targets are placed only through line of sight; a target hidden behind
  geometry from every tried side makes generation report why it could not.
