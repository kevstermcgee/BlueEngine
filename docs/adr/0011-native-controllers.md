# 0011: Optional native controller input

Status: Accepted

## Context

Macroquad's keyboard/mouse path does not supply native gamepad input. The shared
simulation and network protocol already accept analog Movement and look angles.

## Decision

Use gilrs behind an optional `gamepad` feature enabled by `client`. Keep the safe
Rust adapter in viewer/gamepad.rs independent of rendering. Poll events once per
frame and expose normalized snapshots, explicit device selection and button edges.
The stock executable owns focus/menu policy and merges input before simulation.

## Consequences

No new unsafe code, simulation schema or protocol is necessary. Custom clients can
use native devices without enabling presentation. Linux graphical CI gains libudev
development headers; ordinary headless builds retain no gamepad dependency.
Hardware compatibility depends on native drivers/mappings and requires physical
acceptance testing. Rumble and persistent bindings can be added separately.
