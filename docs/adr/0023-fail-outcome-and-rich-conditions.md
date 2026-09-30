# ADR 0023: A lost outcome and richer rule conditions in GameDocument

Status: Accepted

## Context

Ten minigames were built on the declarative `GameDocument` path (`docs/feedback/2026-09-30-ten-minigames-feedback.md`,
games in `BlueEngineGames`). Their feedback agreed on two gaps, and re-running them with full playthrough
scenarios confirmed both. Bomb Defusal's countdown reached -3 and the game still let the player win; Floor is Lava's
`lava_level` and Turret Trench's `cycle_phase` were counters no rule ever read. A rule could only test
`counter == n`, and the only way to end a match was `complete`, so no timer could ever lose the game and no rule
could say "at least" or "every other tick".

## Decision

* `{"action": "fail"}` sets `GameState::failed`. `GameState::finished()` (won or lost) replaces
  `completed` at every point that stops the world accepting events: interactions, timers, trigger zones, the
  restart action, the HUD prompts. Restarting a lost match works exactly like restarting a won one.
* `failed` is serialized only while true. A game that never fails produces the same JSON, save files,
  packets and `HeadlessWorld::checksum` as before this change, so the protocol version and every recorded
  checksum stay valid. A client built before this change rejects a packet that carries `failed` (the state
  type denies unknown fields), which is the right outcome for an unplayable mismatch.
* `Condition` is one struct with optional fields, validated to exactly one form: a leaf (`counter` plus
  `equals`, `not_equals`, `less_than`, `greater_than`, `at_most`, `at_least`, optional `modulo`) or a compound
  (`all`, `any`, `not`). The existing `{counter, equals}` document is a valid leaf. Conditions compile to a small
  tree of indices; evaluation allocates nothing.
* Bounds: depth 4, 16 parts per condition, values within the counter magnitude, `modulo >= 1`. Modulo uses
  `rem_euclid`, so a negative counter yields a remainder in `0..modulo`.
* Scenario files gain a `failed_equals` assertion next to `completed_equals`.

Not adopted:

* **A `reason` on `fail`.** Nothing consumes it, and a string in the state would have to be replicated and hashed.
  A game that needs to distinguish losses can set a counter first.
* **Comparing two counters or `abs(a - b)`.** Requested for a balance-scale puzzle; it would need a second
  counter reference per leaf. Revisit if a second game needs it.
* **A tagged-enum condition.** It reads better but gives poor errors under `deny_unknown_fields` and would break
  the original document form.
* **Raising `actions_per_rule`.** Compound conditions remove most of the daisy-chained counters that motivated it.

## Consequences

* Timer-driven games can lose: a fuse, a rising hazard, a turret window.
* A rule with `all`/`any` replaces several chained helper counters, which also shrinks the 64-rule budget pressure.
* `GameState` gains a field, so code that builds it with a struct literal (rather than `..Default`) must add it.
* Every consumer that asks "is the game over" should call `finished()`, not read `completed`.
