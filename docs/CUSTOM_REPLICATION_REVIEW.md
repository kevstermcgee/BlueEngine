# Review of the existing custom replication branch

Review decision: no confirmed blocker was found within the branch's documented
trusted game-callback contract. The branch remains separate; this review does not
authorize merging or establish final integration with the maintenance changes.

Reviewed `improve/custom-replication` at clean commit
`18074204392737f8a03a907d0c97ebc9ffd30405` in
`/home/kevin/BlueEngine-replication-pass`. Its scoped engine identity was
`d9f12071098db6abe8eab673fe0366e0bd335fdbe47b9ae386910e6015adaef1`
(1,433 files), unchanged before and after the fresh checks. Review followed that
checkout's AGENTS/start/context workflow, ADR 0050, the public NETPLAY contract,
snapshot/event channels, client/server/wire implementation, and measurement
fixtures. No branch source, merge, deployment, or service change was made.

## What the implementation preserves

- `NetGame::SNAPSHOT_CHUNKS` defaults to false and requires `RELIABLE_EVENTS`.
  Opt-in changes the handshake fingerprint, requiring matching rebuilt clients
  and servers. Legacy encoding retains its separate path and regression coverage.
- Authority remains the game-owned fixed-step `NetGame`; snapshots, codecs,
  durable rules, views, controls, cameras, HUD and audio remain game-owned.
  Complete-state replacement expresses removals and semantic ID reuse.
- Envelopes are capped at 8,192 encoded bytes, 16 parts, and the active transport's
  datagram limit. Reassembly retains at most eight unfinished snapshots for one
  second. Layout, part length and digest validation precede application. The
  reassembly byte limit excludes metadata and game-owned decoded objects.
- Session token and peer ownership checks isolate reconnects; match epochs and
  tick floors reject stale/reordered state. A snapshot is applied only after full
  decoding and matching its envelope tick. A valid authenticated new-epoch piece
  or failure can initialize the epoch before complete state: this is documented
  lifecycle behavior, needed when state is blocked but events can progress.
  Malformed new-epoch event payloads validate before resetting the old view.
- Remote event acknowledgements require the current owned session/epoch and a
  sequence no greater than the sender's last sequence. The client validates and
  orders events before acknowledgement. Local queue acceptance is a submission
  statistic, never remote delivery. State has no remote ACK or retained baseline.
  Chunk-mode ready effects are capped at 4,096; undrained overflow and remote gaps
  have separate counters.
- Transport/authentication/encryption/queue implementation files were unchanged
  relative to the branch's baseline. Existing QUIC/TLS remains the encrypted
  runtime path; the raw UDP fixture does not demonstrate encryption or transfer
  every stock-server feature to custom netplay.

## Fresh and historical evidence

Fresh Linux checks at the reviewed commit used Rust 1.98.1, `itest`,
`CARGO_INCREMENTAL=0`, `CARGO_PROFILE_ITEST_DEBUG=0`, `CARGO_BUILD_JOBS=2`,
`CARGO_TARGET_DIR=/mnt/blueengine-usb/authoring-efficiency-target`, and
`TMPDIR=/mnt/blueengine-usb/authoring-efficiency-tmp`. This branch predates the
maintenance branch's optional physics/networking features; its supported
`--no-default-features` configuration was used directly.

```sh
cargo test --locked --profile itest --no-default-features --lib viewer::netplay:: -- --test-threads=1
cargo test --locked --profile itest --no-default-features --test netplay --test netplay_state -- --test-threads=1
cargo clippy --locked --profile itest --no-default-features --lib -- -D warnings
```

Results: 135 netplay library tests, 22 netplay integration tests and 11 state
integration tests passed; Clippy passed. The state suite reports one ignored
subprocess helper, which its passing UDP parent test explicitly launches and
checks. Harness durations were 13.17, 0.76 and 8.85 seconds respectively. These
are warm focused acceptance observations, not a speed comparison. Exact commands,
source identity and logs are retained in the reviewed checkout's ignored
`.be2-work/review-acceptance.json` and `review-netplay-*.log` files.

The prior canonical local 40-command receipt is
`.be2-work/check-20261013T123020152830Z-ypjamo5u/report.json`, copied unchanged to
`.be2-work/replication-final-verification.json`: all 40 commands passed, with
4,089 reported test executions across modes and 4,587.184 seconds elapsed. Its
before/after engine identity matches the reviewed clean commit exactly. The
receipt does **not record a Git revision**, so no missing revision is inferred.
This is matching scoped-source local evidence, not current CI or proof that the
maintenance and replication branches integrate. The older authoring packet names
dirty revision `c02080422871f8ebc53b07deed62b9dee29a805a`; it is not the final
passing receipt.

The recorded measurement source hashes for the six snapshot/client/server/wire
fixture files match this review. The documented baseline is `579b90868e7e20005f874d2689e35a5f8a23e559`;
network-specific replay starts at `bfbda246ea4921f2102c9b0e6e788df5fe172133` after unrelated
camera work. Budgets precede improved results, except the explicitly labeled later
180-tick recovery deadline. The paired legacy wire replay and deterministic
transport simulator cover 96 actors, four clients, loss, delay, removal/ID reuse,
stale epochs, bounded queues and eventual complete recovery. A bounded UDP process
smoke adds actual datagram execution. They do not prove production QUIC throughput,
real-time deployment capacity, or configured player capacity. Whole-state delivery
uses more bandwidth/CPU than legacy's failed delivery; no efficiency gain is claimed.

## Limits and integration conditions

The encoded/reassembly limits are not a hard total-allocation limit. `write_snapshot`
writes into an ordinary growable writer before the size check, and custom
snapshot/decoded-view objects can allocate separately. `snapshot_with_budget` is
cooperative trusted Rust code; a panic, OOM or callback that never returns is not
contained. Oversize failure notices/statistics work when that callback returns.
The public guide/report state this limitation; pathological serializer isolation
is an evidence gap, not a demonstrated guarantee.

There is no paced state cursor. If persistent backpressure accepts fewer pieces
per cadence than a complete keyframe needs, complete state can remain unavailable
while independently acknowledged events continue. Recovery requires sufficient
submission capacity and a later complete keyframe before expiry. Bounded state
pacing is the next evidenced protocol improvement; deltas, stock-server interest
management, partial replication and production capacity remain outside this pass.

A pre-existing authoring discrepancy deserves a separate bounded follow-up:
`ServerConfig.participants` describes players plus AI, but match start clamps it
to seat capacity (`server.rs`). The 96-actor fixture owns those actors inside its
game rules and uses four participants; it does not verify 96 configured participants.
This clamp is also present at baseline `579b908`, rather than introduced here.

Before any separately authorized merge, resolve the maintenance branch's feature
gates/planner ownership against these changed modules and fixtures, then run its
complete required Linux and Windows integration closure at the actual merged
revision. Re-run custom public API/legacy-wire, stock-server and transport checks;
preserve native release/package gates where shipping inputs change. Fresh Windows
execution, current remote CI, shipping validation, physical input/audio/visual
behavior and sustained production QUIC load were not verified by this review.
