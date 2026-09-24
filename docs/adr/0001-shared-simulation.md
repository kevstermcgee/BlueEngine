# 0001: Shared simulation and optional presentation

Status: Accepted; retrospective description of the current code.

## Context

The local viewer and a future authoritative server need the same movement and collision behavior. A headless build must not need a graphics context or audio device. There is currently no transport implementation to abstract.

## Decision

Share the concrete `Controller`, `Movement`, `PlayerStepper` and `HeadlessWorld` APIs in [simulation.rs](../../src/viewer/simulation.rs) and [controller.rs](../../src/viewer/controller.rs). Keep client graphics behind the `client` feature and offline output behind `offline`. The inherited library name remains `vesper3d`.

Simulation runs at 60 Hz. Client presentation interpolates poses; the client stepper caps catch-up at eight ticks and discards excess stall time. Headless callers explicitly schedule each tick. Keep future PulseNet transport separate from rendering and map it onto the simulation boundary.

## Consequences

The two runtimes reuse physics without introducing a trait for a single implementation. Public rustdoc and lifecycle tests define the present contract. Add a narrow trait when an actual adapter or alternate implementation needs one; do not predefine authentication, packet or snapshot contracts before integrating transport.

Interpolation adds up to one tick of positional latency. A local fixed-step parity test is not proof of cross-platform lockstep determinism. The headless executable is a finite benchmark, not a listening server. It has no authentication, input expiry, network sequence validation, player-player collision or authoritative game rules.
