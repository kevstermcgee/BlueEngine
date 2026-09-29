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
