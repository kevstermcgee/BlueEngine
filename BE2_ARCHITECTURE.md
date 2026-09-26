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

`client` gates Macroquad graphics/audio, native gamepads and Windows window/input support.
The optional `gamepad` feature also works without graphics: `viewer/gamepad.rs`
polls gilrs devices and produces frame snapshots; the executable merges those
with keyboard/mouse before the existing fixed-step and network input paths. See
[controller contract](docs/CONTROLLERS.md) and [ADR 0011](docs/adr/0011-native-controllers.md). `offline`
gates the inherited output renderer. `--no-default-features` includes neither:
Serde, Rapier and networking remain. `be2-headless --server` runs the selected
datagram profile; without that flag it runs bounded local simulation/benchmark ticks.

## Authoring and prototype API

MapDocument v1 contains static primitive geometry, colliders, semantic entities and
an optional room/portal graph. Both runtimes load it through `--map`. Blue Test Lab
is the built-in default; native `export-lab` and `export-house` produce editable
snapshots. Vesper scene assets alone do not include playable collision/entities.

SceneBuilder batches existing validated edits and requires an explicit standalone spawn.
`box_body` includes static collision and a semantic entity; `structural_box` creates
visual collision without a reachability target. Catalog props use existing rigid-body extraction. `build` returns MapDocument;
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
profiles/actions. New FPS games use the separate rendering-independent `viewer::fps`
domain for weapon catalogs/state, smooth ADS and team-deathmatch rules; games retain
their presentation, hit-shape policy and transport messages. A general arbitrary
behavior document/controller/action boundary remains future work.

## Networking

DedicatedServer (`viewer/server.rs`) is generic over `DatagramTransport`; packet
encoding/decoding sits above the concrete socket. Both UDP and QUIC therefore run the
same 60 Hz authority and 20 Hz snapshot path. Input sequence numbers reject duplicates and
out-of-order commands. Socket ownership gates disconnects. Reconnect reservations
are address-bound and expire after 60 seconds. With `--auth-key`, the server uses
HMAC-SHA256 challenge-response with random nonces/salts, issues session tokens, and
authenticates later datagrams with replay protection. The session registry is the
authoritative owner of each token and timeout; registration, reconnect, timeout and
disconnect cleanup update both token and peer indexes atomically. Credential entropy
failures reject the connection, and keyed disconnects require the exact token.
Without that flag the raw-UDP session remains unauthenticated. Neither mode encrypts
payloads.

Clients acknowledge snapshot ticks. The server computes deltas against acknowledged
history; clients reject mismatched baselines and request keyframes. Periodic full
keyframes provide another recovery path. Replication includes prop orientation,
velocities, sleeping state and holder. Combat resolves nearer static geometry before
applying prop impulses. Constants in weapons.rs/wrench.rs define ranges/cooldowns.
The reusable FPS domain is deliberately above this stock packet path; BlueDM shows a
game-owned protocol that consumes the shared combat rules without coupling them to a
specific renderer or transport profile.

Protocol 5 requires the initial content fingerprint, captured before physics
extraction from scene, collision, semantic data and the spatial graph. HashMap/HashSet
contents are canonicalized. Mismatched content and full servers are rejected before
session allocation. The client displays the rejection. Old clients must rebuild.
The fingerprint is non-cryptographic FNV-1a for accidental mismatch detection only.

JSON encoding/decoding enforces an 1100-byte cross-transport datagram ceiling; it does not chunk oversized
snapshots. Malformed/oversized datagrams are dropped without terminating the server;
receive work is bounded per call and server poll. Compact binary serialization,
reconnect-token migration and content negotiation remain planned. Executables expose
raw UDP as `--transport development` and pinned-certificate QUIC/TLS 1.3 as
`--transport production`; production servers require `BLUE_TLS_KEY_FILE`. Optional
HMAC authentication supplies client identity on either profile. Room overlap resolves
by minimum stable ID.
Tests cover two-client UDP and separate server processes, loss/reordering, contention,
combat occlusion, content mismatch, capacity, malformed packets and a QUIC handshake
through the authoritative server.

## Measurement and remaining limits

`be2-tools bench`, `inspect-performance`, `validate-budget` and `replay-test` provide
current diagnostics. `be2-tools bench` and `tests/benchmarks.rs` enforce the same
checked-in absolute budgets for simulation steps, snapshots, deltas and room lookup;
budget failures return nonzero. These are service ceilings suitable for heterogeneous
CI, not hardware-normalized baselines or allocation guards. Replay-test compares two
in-memory runs and quantized checksums do not prove cross-platform bitwise determinism.
See [refinement report](docs/REFINEMENT.md).

No arbitrary gameplay scripting, account service or required MCP
adapter is implemented. [ADRs](docs/adr/README.md) record settled boundaries.

## Data-driven prototypes

GameDocument v1 (`viewer/game.rs`) compiles validated rules to indices, separate from
MapDocument geometry. `viewer/profile.rs` supplies movement dimensions and speeds.
Local play and HeadlessWorld share ordered interaction transitions. Protocol 5 adds
game semantics to the content fingerprint and repeats bounded full GameState snapshots
independently of movement deltas. Interactable visibility is replicated separately
from eligibility and never changes collision; the stock client bakes those semantic
boxes into independently drawable batches. See docs/GAME_QUICKSTART.md and ADR 0007.

`viewer/scenario.rs` is the reusable behavioral-driver boundary. Timestamped public
inputs drive real fixed ticks, while structured outcomes inspect positions, counters,
completion, eligibility and visibility. Scenario-relative game paths keep committed
tests portable; replay traces remain a determinism diagnostic rather than a behavior
oracle.

`HeadlessWorld::change_map` and `change_game` construct replacement content before
committing it, preserve the authoritative tick, and respawn existing player IDs.
Game state and props reset. Network hosts must coordinate the new protocol-5 content
hash with clients before transitioning; the world API does not distribute assets.

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

Standalone client presentation lives in `viewer/game_client.rs` behind `presentation`; `viewer/presentation.rs` is rendering-free bounded pose smoothing. `HeadlessWorld::with_static_room` preserves authored static collision without catalog rigid-body extraction. See docs/GAME_PRESENTATION.md.
