# 0007: Bounded GameDocument v1

Status: accepted (2026-09-24). Extends 0002 and supersedes protocol 2 in 0006.

## Context
Static MapDocument geometry cannot express even a switches-and-exit prototype.
AI authors need validated data and discoverable commands without repeated source edits.

## Decision
Keep MapDocument unchanged. A separate GameDocument owns one movement profile,
spawn points, counters, declared interaction targets and bounded ordered rules.
Compile references into indices before simulation. Use the same GameRuntime in local
play and HeadlessWorld; the server verifies range/occlusion and handles interaction
edges in stable player order. Conditions observe previous actions, once flags are
match-wide, and completion stops subsequent interactions. No recursive dispatch.

Protocol 3 fingerprints canonical game semantics plus initial map content (excluding
the map file location). Game state is a separate full snapshot at 20 Hz with monotonically
accepted ticks. At maximum configured state it fits 256 bytes, so existing movement
snapshot budgets remain independent. Repeated state recovers dropped packets; joins
receive current counters, enabled/once masks and completion without replaying events.
The fingerprint detects accidental incompatibility, not adversarial modification.

## Consequences
Native describe/schema/example/validate make a source-free three-switches prototype
possible. Validation confines map references, checks spawn clearance and rejects
unknown fields/references before starting physics. Controller profiles retain legacy
Scientist/Feta behavior. Static targets must match box geometry/collision/entity bounds.
Enabled means interaction eligibility, never physical door movement or visibility.
Weapons remain demo code and are disabled in game mode. Timers, multiple profiles,
triggers beyond E, durable event replay and arbitrary scripts are deferred.

Evidence: tests/game_documents.rs covers shared deterministic transitions, real UDP
clients, forged state, incompatibility, occlusion, profiles and native authoring.
