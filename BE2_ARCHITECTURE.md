# BlueEngine architecture

Current runtime contract. The repository is BlueEngine; Cargo package/executables
remain `be2` and the compatibility library is `vesper3d`. Historical background:
[ADR 0004](docs/adr/0004-authoring-and-compatibility.md).

## Shared simulation and presentation

`viewer/simulation.rs` owns HeadlessWorld and the rendering-free 60 Hz movement path.
Controller/Movement are shared by both hosts. PlayerStepper retains jump edges,
interpolates display poses and caps frame catch-up at eight ticks. HeadlessWorld
holds up to eight players; input persists until replaced and jump is consumed once.
DedicatedServer adds sequencing and six-tick (100 ms) stale-input neutralization.
`new` and `try_with_room` report physics initialization errors. Legacy `with_room`
retains its historical best-effort physics behavior for compatibility.

`client` gates Macroquad graphics/audio and Windows window/input support. `offline`
gates the inherited output renderer. `--no-default-features` includes neither:
Serde, Rapier and standard-library UDP remain. `be2-headless --server` binds a UDP
socket; without that flag it runs bounded local simulation/benchmark ticks.

## Authoring and prototype API

MapDocument v1 contains static primitive geometry, colliders, semantic entities and
an optional room/portal graph. Both runtimes load it through `--map`. Blue Test Lab
is the built-in default; native `export-lab` and `export-house` produce editable
snapshots. Vesper scene assets alone do not include playable collision/entities.

SceneBuilder batches existing validated edits. Boxes include static collision;
catalog props use existing rigid-body extraction. `build` returns MapDocument;
`world` constructs a fallible HeadlessWorld. `prelude` exposes the minimal shared
surface. Stable-ID `impulse` and copied `prop_position` avoid retaining body borrows.
See [API audit](docs/API_AUDIT.md) and [quickstart](docs/AI_QUICKSTART.md).

The native command registry drives parser arity, help and describe. Search reads
embedded tools/FEATURES.json, returns at most ten records and does not read source.
Python authoring remains a bounded wrapper; its describe also queries native
capabilities. Evidence tests check registry agreement, file/test references and
actual workflows. Curated prose still requires review.

## Physics and demo boundaries

PropPhysics extracts catalog-material primitives inside small semantic bounds into
Rapier compound bodies. Architectural materials, large furniture and thin wall art
stay fixed. All eligible bodies are allocated during initialization: lifecycle
promotion/sleep is not a lazy-allocation architecture. Physics uses 120 Hz steps;
HeadlessWorld advances it from the 60 Hz authoritative tick.

Body poses update collision, semantic bounds and dynamic geometry. Per-player
ownership tables reject contention and release held props on disconnect. Clients
reconcile held state from authoritative snapshots. Static rendering is baked once;
prop meshes reuse transforms. Scientist/Feta and wrench/pistol remain demo-coupled
profiles/actions. A general behavior document/controller/action boundary is future work.

## Networking

DedicatedServer (`viewer/server.rs`) and Packet/UdpTransport (`viewer/net.rs`) provide
60 Hz authority and 20 Hz snapshots. Input sequence numbers reject duplicates and
out-of-order commands. Socket ownership gates disconnects. Reconnect reservations
are address-bound and expire after 60 seconds. With `--auth-key`, the server uses
HMAC-SHA256 challenge-response with random nonces/salts, issues session tokens, and
authenticates later datagrams with replay protection. Without that flag the raw-UDP
session remains unauthenticated. Neither mode encrypts payloads.

Clients acknowledge snapshot ticks. The server computes deltas against acknowledged
history; clients reject mismatched baselines and request keyframes. Periodic full
keyframes provide another recovery path. Replication includes prop orientation,
velocities, sleeping state and holder. Combat resolves nearer static geometry before
applying prop impulses. Constants in weapons.rs/wrench.rs define ranges/cooldowns.

Protocol 3 requires the initial content fingerprint, captured before physics
extraction from scene, collision, semantic data and the spatial graph. HashMap/HashSet
contents are canonicalized. Mismatched content and full servers are rejected before
session allocation. The client displays the rejection. Old clients must rebuild.
The fingerprint is non-cryptographic FNV-1a for accidental mismatch detection only.

JSON encoding/decoding enforces a 1400-byte datagram ceiling; it does not chunk oversized
snapshots. Malformed/oversized datagrams are dropped without terminating the server;
receive work is bounded per call and server poll. Compact binary serialization,
encrypted executable transport, reconnect-token migration and content negotiation
remain planned. `viewer/net/quic.rs` implements a QUIC/TLS datagram transport behind
the transport trait, but the current client and dedicated-server binaries use
`UdpTransport`. Room overlap resolves by minimum stable ID.
Tests cover two-client UDP and separate server processes, loss/reordering, contention,
combat occlusion, content mismatch, capacity and malformed packets.

## Measurement and remaining limits

`be2-tools bench`, `inspect-performance`, `validate-budget` and `replay-test` provide
current diagnostics. Replay-test compares two in-memory runs; it is not a durable
versioned trace format. Quantized checksums help diagnose divergence but do not prove
cross-platform bitwise determinism. There is no calibrated benchmark baseline or
allocation regression guard yet. See [refinement report](docs/REFINEMENT.md).

No arbitrary gameplay scripting, account service or required MCP
adapter is implemented. [ADRs](docs/adr/README.md) record settled boundaries.

## Data-driven prototypes

GameDocument v1 (`viewer/game.rs`) compiles validated rules to indices, separate from
MapDocument geometry. `viewer/profile.rs` supplies movement dimensions and speeds.
Local play and HeadlessWorld share ordered interaction transitions. Protocol 3 adds
game semantics to the content fingerprint and repeats bounded full GameState snapshots
independently of movement deltas. See docs/GAME_QUICKSTART.md and ADR 0007.

Player overlap recovery handles props dropped/moved into a character before movement.
It chooses the nearest clear horizontal candidate within four metres, preserves feet
height, and rejects paths crossing previously non-overlapping colliders. If no safe
candidate exists, it keeps the current pose rather than crossing a wall. This runs in
the shared Controller for local play, client prediction and server simulation.

Third-person CameraRig is presentation-only. Feta uses a low centered boom and 7 cm
clearance; look pitch changes view direction without swinging the boom into furniture.
Obstructions retract immediately, and release uses exponential distance smoothing
without delaying player-follow or look. Render and local aim use the same rig policy.
Collision is rechecked for the interpolated pose; first-person remains unchanged.
