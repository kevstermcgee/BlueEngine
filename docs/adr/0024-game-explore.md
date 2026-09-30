# ADR 0024: Explore a GameDocument's rule states instead of trusting validation

Status: Accepted

## Context

Ten minigames on the declarative `GameDocument` path passed `game-validate` and a smoke scenario each
(ADR 0023 has the background). Re-running them with full playthroughs showed that validation says nothing about
whether a game works: a countdown no rule read, a timer nobody listened to, a "laser" vault with no laser, a
reset that could never re-arm, and objectives armed one press early all validated cleanly. Finding each took a
hand-written scenario and reading the rules.

## Decision

`be2-tools game-explore GAME.json [--max-states=N]` (`viewer::game_explore`) searches the game's rule states
breadth-first and reports winnability, the shortest win and loss, and dead or stuck parts (see
`docs/GAME_QUICKSTART.md`).

* **The engine's own runtime takes every step.** `GameRuntime` gains a small model API (`model_events`,
  `model_apply`, `model_load`, `record_fired_rules`), and interacting and timer expiry are factored into
  `fire_target_rules` and `expire_timer`, which the real tick path uses too. The analysis therefore cannot
  disagree with the game about what a rule does.
* **Time and movement are abstracted**, not simulated: any running timer can expire, any enabled target can be
  pressed, and zones can be entered and left, in any order. This over-approximates the game, so "can be won" can
  overstate and every "never" conclusion is exact (unless the search is truncated, which is reported, and which
  downgrades those findings to notes).
* **The state key forgets what cannot matter.** A counter no condition reads is dropped. A counter compared
  only by `modulo` keeps its remainder (lcm of the moduli, at most 1,000,000). A counter that is only ever
  raised (or only lowered) keeps its value up to one past the largest (or smallest) constant compared with it.
  Each is exact for how the rules can change that counter. A randomized differential test (400 games, folded
  against unfolded) and mutation checks guard the claim.
* Errors exit 1; warnings and notes do not.

Not adopted:

* **Simulating time.** Exact timing would need the tick model and the map's geometry, and the state space would
  explode; the abstraction answers "is this game structurally sound", which is what validation lacked.
* **A win-probability or difficulty estimate.** It would need a model of player behaviour.
* **Emitting scenarios from the shortest win.** Turning events into walking inputs needs positions (a separate
  improvement to scenario authoring).

## Consequences

* Authors get a one-command answer to "did I wire this up?" and the shortest win as the first scenario to write.
* `GameRuntime` has a public model API; it is intended for tooling and tests, not gameplay code.
* A game with an unbounded counter that is genuinely read by an exact comparison in both directions (raised and
  lowered) is searched exactly and may truncate; the report says so.
