# ADR 0003: Opt-in arena-shooter primitives

## Status

Accepted.

## Context

BlueEngine's default character controller deliberately converges on approachable walk
and sprint speeds. Arena shooters need materially different semantics: acceleration,
ground friction, air steering, momentum-preserving jumps, held-jump chaining, vertical
launch routes, timed map control and projectile splash. Retrofitting these semantics
into the default controller would change existing games and blur its contract.

## Decision

`viewer::arena` is a rendering-independent, opt-in domain layer. It owns a validated
fixed-step `ArenaBody` with configurable grounded step-up, explicit `LaunchVolume`, `TimedPickup`, deterministic `Projectile`
and free-for-all `FragMatch`. It also declares the shared 90-degree arena camera default.
Games retain presentation, map layout, weapon identity, collision-query policy and
transport shells. The default controller and FPS/TDM foundation remain unchanged.

## Consequences

Arena games can share tested feel-critical math without destabilizing exploration games.
The state is serializable and fixed-tick friendly. Collision uses conservative capsule-
against-box movement; sophisticated ramps, step climbing, prediction reconciliation,
lag compensation and swept projectile callbacks remain explicit future extensions.
