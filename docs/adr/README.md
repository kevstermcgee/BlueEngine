# Architecture decisions

These records document decisions visible in the current implementation, retrospectively recorded on 2026-09-23. Current networking is implemented; records describe the decision at the time and later superseding changes.

- [0001: Shared simulation and optional presentation](0001-shared-simulation.md)
- [0002: Explicit static map documents](0002-static-map-documents.md)

For a new decision, add the next numbered file with Status, Context, Decision and Consequences. Explain the tradeoff and link the affected code. Supersede old records explicitly instead of rewriting their rationale. Routine implementation details belong in rustdoc or the feature index.

- [0003: Runtime loose-prop rigid bodies](0003-loose-prop-physics.md)

- [0004: One authoring contract and compatibility names](0004-authoring-and-compatibility.md)
- [0005: Native discovery and evidence](0005-native-discovery.md)
- [0006: Content compatibility handshake](0006-content-handshake.md)

- [0007: Bounded GameDocument v1](0007-game-documents.md)
- [0008: One authoritative path over explicit transport profiles](0008-transport-profiles.md)
- [0009: Explicit playable content lifecycle](0009-playable-content-lifecycle.md)
- [0010: Reusable rendering-independent FPS domain](0010-reusable-fps-domain.md)

- [0011: Optional native controller input](0011-native-controllers.md)

- [0012: Shared gameplay kit](0012-shared-gameplay-kit.md)
- [0013: Demand-loaded context and conservative validation scopes](0013-scoped-change-workflow.md)
- [0014: Bounded acknowledged partial-world replication](0014-bounded-replication.md)

- [0015: A custom-simulation road alongside GameDocument](0015-custom-simulation-road.md)
- [0016: Native save states](0016-native-save-states.md)
- [0017: Every game ships its own identity, verified by a gate](0017-game-identity-and-ship-gate.md)
- [0018: The save contract a physics-backed simulation can keep](0018-physics-save-contract.md)
- [0019: One lifecycle for custom-simulation windows, and a save policy declared per game](0019-lifecycle-and-save-policy.md)
- [0020: One mouse-look convention, checkable scale, and a dev loop that survives a running game](0020-look-scale-and-dev-loop.md)
- [0021: Stick look in the shared convention, discrete menu steps, and a graceful exit](0021-stick-look-menu-steps-and-exit.md)
- [0022: A multiplayer kit for custom simulations, and the fixes Spooky Kart called for](0022-custom-sim-multiplayer-kit.md)
- [0023: A lost outcome and richer rule conditions](0023-fail-outcome-and-rich-conditions.md)
- [0024: Explore a GameDocument's rule states instead of trusting validation](0024-game-explore.md)
- [0025: Scenarios that say what they mean, and a generated first scenario](0025-scenario-intents-and-generated-scenarios.md)
- [0026: Movers carry the players standing on them](0026-movers-carry-players.md)
- [0027: A crowd-sized server: capacity, cheaper replication, and parallel preparation](0027-crowd-sized-server.md)
- [0028: Three changes from the external study: replication priority, an injectable clock, span summaries](0028-study-replication-priority-clock-spans.md)
- [0029: World updates in a compact binary form (protocol 8)](0029-compact-world-updates.md)
- [0030: Graceful shutdown on signals, without a new dependency](0030-graceful-shutdown-signals.md)
- [0031: `be2-ctl`, a small manager for servers on one machine](0031-be2-ctl-server-manager.md)
- [0032: A version-aware upgrade workflow for external games](0032-game-upgrade-workflow.md)
- [0033: Generated ambient music, and audio settings that survive a relaunch](0033-ambient-music-and-persistent-audio-settings.md)
- [0034: Level geometry and waypoint pathing promoted from Dead Air](0034-level-geometry-and-waypoint-pathing-from-dead-air.md)
- [0035: A testable, loud native key reader, and one restart convention](0035-native-key-reader-and-restart-convention.md)
- [0036: Shadows for kit games: contact blobs and one shadow map, behind one setting](0036-shadows-for-kit-games.md)
- [0037: One hub, one name, every BlueEngine online game](0037-shared-multi-game-hub.md)
- [0038: The engine learns from the games built on it](0038-engine-learns-from-development.md)
- [0039: Shared 2D presentation and verified static browser artifacts](0039-two-d-browser-artifacts.md)
