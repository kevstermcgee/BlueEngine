# 0009: Explicit playable content lifecycle

Status: accepted (2026-09-26). Extends 0002 and supersedes the capacity and protocol
details of 0007.

## Context

Map analysis and standalone simulation implicitly used the Test Lab spawn at
`(0, 0, 4.6)`. Static boxes always created semantic entities, so ceilings and walls
became false reachability targets. GameDocument v1 masks limited rules and targets to
16, and replacing a map required rebuilding several independently owned runtime
subsystems.

## Decision

MapDocument may carry an explicit `default_spawn`; standalone builders require one,
while GameDocument spawn points remain authoritative for games. Analysis without a
map spawn or caller-supplied start fails instead of guessing. `box_body` retains its
semantic entity for compatibility and `structural_box` creates only geometry and
collision. Reach tests use the closest entity-bound point from a reachable player eye.

HeadlessWorld owns transactional `change_map` and `change_game` operations. Replacement
content is fully built before commit, the tick is preserved, and existing player IDs
respawn with neutral input. Game/prop state resets. Network hosts remain responsible
for coordinating content availability and hashes before a transition.

GameDocument remains bounded, but protocol 5 widens replicated masks to 64 bits and
permits 32 counters and 64 rules/targets/zones/movers/timers. Worst-case GameState
encoding must remain below the shared packet ceiling; warnings do not replace hard
validation limits.

## Consequences

Standalone SceneBuilder callers must call `spawn`. Legacy map documents still parse,
but lint, reach, audit, route, and standalone player joins require an explicit spawn.
Structural architecture no longer pollutes semantic reachability or lifecycle tables.
Content transitions are safe for local/headless ownership but do not yet implement a
network preload/acknowledgement protocol or asset transfer.

Evidence: `tests/prototype.rs`, `tests/game_documents.rs`, and
`tests/content_handshake.rs` cover spawn enforcement, structural boxes, rollback,
player respawn, widened packet budgets, and content fingerprints.
