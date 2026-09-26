# 0010: Reusable rendering-independent FPS domain

## Status

Accepted on 2026-09-26.

## Context

BlueEngine's first combat code coupled one pistol and one wrench to the stock viewer.
The multiplayer template then implemented a second, game-local hitscan loop. Building
BlueDM exposed the cost of that split: a new FPS needed to reinvent weapon validation,
fixed-tick cadence, reloads, fire modes, spread, ADS, teams, health, scoring and
respawns before it could test its actual game design.

## Decision

`viewer::fps` is a rendering- and transport-independent domain module. It provides a
validated data-driven weapon catalog, a ten-firearm starter armory, deterministic
fixed-tick weapon state, reproducible shot intents, smooth frame-rate-independent ADS,
four presentation-neutral operative archetypes, and an authoritative team-deathmatch
state machine. Games still own maps, animation/meshes, networking messages, hit-shape
policy and HUD/presentation.

The older `viewer::weapons` pistol/wrench demo remains compatible. It is not the API
for new multiplayer FPS projects.

## Consequences

- Future shooters can share balance data and combat invariants without adopting the
  stock renderer or wire protocol.
- Server and client can reproduce shot cosmetics from the same sequence while the
  server remains authoritative for damage.
- Hit registration geometry and internet service hardening remain explicit game/host
  responsibilities rather than being hidden inside a general weapon definition.
- BlueDM is the first consumer and provides an end-to-end usage example.
