# ADR 0034: Level geometry and waypoint pathing promoted from Dead Air

Status: Accepted

## Context

Dead Air (a flashlight-and-pistol horror game on the custom-sim road) hand-built an indoor station from
boxes in code rather than authoring a `MapDocument`, the way Spooky Kart and the physics mini-games
already build their own scenes. Two pieces of that work turned out to have zero game-specific content:
cutting doorway gaps out of straight walls, and a small branching graph with shortest-path lookup for
the monster's patrol/investigate/hunt AI to move through the rooms those walls make. The engine's
existing `pathing.rs` (A*, `plan_route`/`execute_walk`) is a different thing: it verifies an *authored*
`MapDocument` is walkable at map-authoring time, not real-time AI navigation over a hand-built
custom-sim level — so this was a genuine gap, the same way `physics.rs` was identified as a promotion
candidate after three physics mini-games each copied it by hand.

A second, smaller lesson cost real time: drawing a scene's static meshes with `draw_mesh` before calling
`gl_use_material(&materials.world)` compiles, runs, and renders an empty scene — no error, no warning.
It was found only by inspecting a headless `--capture` and seeing a flat, geometry-free frame. A third:
`devkit::path::yaw_of(direction)` already does exactly what Dead Air's own code hand-derived
(`direction.0.atan2(-direction.2)`) to face its monster toward the player — a pure discoverability gap.

## Decision

- **`devkit::level`** (`wall_along_x`, `wall_along_z`): a wall run with a list of `(start, end)` gaps to
  leave open as doorways, returning the solid `Collider` segments. Ported from Dead Air's `layout.rs`
  with its existing tests; height and thickness are now parameters instead of module constants, since
  different games want different ones.
- **`devkit::waypoints::WaypointGraph`**: `Waypoint { pos, edges }` nodes, `Vec`-backed (not a fixed-size
  const array like Dead Air's own, so any game can build one from authored data or code), with
  `nearest(pos)` and breadth-first `path(from, to)`. A different shape from the existing
  `path::ClosedPath` (a single continuous loop — a track, a patrol lap): this one branches, for a
  building's rooms and corridors rather than a racetrack.
- **`Materials::draw_static(&meshes)`**: binds `world` and draws every mesh in one call, so the two-step
  version cannot be called in the wrong order. Purely additive; the manual
  `gl_use_material`/`draw_mesh` pattern still works. `templates/custom-sim/src/main.rs` now uses it for
  the starter's own static platform, so every new game demonstrates the safe pattern by default.
- **Docs**: both traps (the material-binding order, and `yaw_of` over a hand-derived `atan2`) added to
  `docs/CUSTOM_CLIENT.md`'s and `docs/CUSTOM_SIM_CHEATSHEET.md`'s existing "traps met building a real
  game" lists — the convention that already exists for exactly this kind of lesson — plus the new
  `devkit` pieces added to both docs' API tables and `tools/FEATURES.json`.
- **Dead Air itself** was switched to call the new `devkit::level`/`devkit::waypoints` instead of its own
  copies (same room coordinates, same 12-node graph, same `PATROL_ROUTE`, unchanged behavior), which is
  both the real validation that the extracted API is usable from an actual game and keeps it from
  drifting out of sync with the pattern it originated.

## Consequences

A custom-sim game that builds its own indoor level — not just Dead Air — now has a tested, generic
starting point for walls-with-doorways and lightweight NPC pathing, instead of needing to hand-roll BFS
over a graph again. The material-binding trap and the `yaw_of` pointer are now where every custom-sim
game's `AGENTS.md`-reading agent will see them before hitting the same bug or re-deriving the same
formula.

## Rejected or deferred

- **A "Collider list to visual boxes" helper** (turning wall colliders directly into matching `Template`
  boxes): Dead Air's version of this was about six lines (iterate colliders, call `box_` with the
  center/half-extents). Not enough real complexity to justify new API surface; inlining it remains the
  right size for this.
- **A migration-registry entry** (`tools/upgrade_migrations.json`) guiding existing games to adopt this:
  unlike the audio-settings migration, there is no reliable way to grep-detect "this game hand-rolled a
  waypoint graph," and a low-confidence entry would weaken a registry whose value depends on its entries
  being reliable hints, not noise. A game that wants this can read this ADR.
- **Generalizing `pathing.rs`'s A* to also serve custom-sim games directly**: it is built around
  `MapDocument` and real 60 Hz `Controller` simulation for *verification*, a different cost and purpose
  than a monster's per-tick AI decision; conflating the two was not attempted here.
