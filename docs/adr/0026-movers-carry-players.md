# ADR 0026: Movers carry the players standing on them

Status: Accepted

## Context

A game `mover` translates a box collider between a closed and an open position. It worked as a door, but a probe
showed the obvious second use failing: put a player on a box that rises 2 m and the box moves up through them;
they end on the floor. Nothing in the engine carried a body with the platform it stood on. The controller docs
named `set_physics_state` as "the supported way to apply a moving platform", but no engine code did it, so every
game wanting a lift would have written it by hand. The Floor is Lava feedback asked for exactly this ("movers can
push players upward rather than clipping through them").

## Decision

* `GameRuntime::step_movers` returns the motion of each mover that moved this tick (`MoverMotion { from, to }`);
  callers that own no players ignore it.
* `Controller::ride(from, to)` moves a grounded body standing on the top of the old box (feet within 5 cm of its
  top, inside its footprint plus the body radius) by the box's translation, in any direction, and zeroes its
  vertical velocity. `HeadlessWorld::step` applies it to every player, and the local client applies it to its own
  controller, in both its offline loop and when a server snapshot moves a mover.
* Nothing else is done for a box that slides or rises into a body. A first version pushed such bodies out through
  the leading face; a mutation check showed ordinary collision already does it, so the branch was deleted rather than
  kept as untested code. Only the case collision cannot handle, keeping a body on top of a box that moves away from
  under it, needed new behaviour.

Not adopted:

* **Crushing.** A box descending onto a body is left to collision.
* **Carrying bodies that are not standing on top** (riding along a wall, hanging on the side).
* **Rotation and looping paths.** Movers stay a two-state translation; those remain a separate request.
* **Carrying dynamic props** (`prop_physics`). Loose props are a different system.

## Consequences

* A mover can be a lift, an elevator or a moving platform, and the carried player stays on it.
* `step_movers` changes its return type; existing callers compile unchanged because the value can be ignored.
* A networked custom client that steps movers itself must call `ride` on its own controller; otherwise it predicts
  the player sinking and is corrected by the server. The stock client does. Other peers' bodies are the server's
  business (it applies `ride` to every player).
