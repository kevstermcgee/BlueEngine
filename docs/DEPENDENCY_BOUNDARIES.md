# Selecting engine dependencies

Default `vesper3d` and the existing `client` feature retain the stock native client,
rigid-body simulation and encrypted multiplayer. Games with narrower needs use
`default-features = false` and opt into what they execute:

| Consumer | Engine features | Public path |
| --- | --- | --- |
| Pure game rules, saves, integer 2D collision | none | `runtime::{Simulation, Snapshot}`, `two_d::{Rect, Body}` |
| Offline 2D or planar hybrid game | `two-d` or `portable`; optionally `gamepad` | `two_d::client`, `portable::draw` |
| Spatial rigid-body rules | `physics` | `SceneBuilder::world`, `HeadlessWorld`, `vesper3d::rapier` |
| Custom native multiplayer authority | `networking` | `viewer::netplay::{NetGame, ClientView}`, `netplay::cli::serve` |
| Stock native headless server | `physics,networking` | `be2-headless`, `DedicatedServer` |
| Custom native client without stock rigid bodies or QUIC | `native-client` | `viewer::kit`, `ClientInput`, custom window and controls |
| Schema generation / validation | `schema-generation` / `schema-validation` | existing `python3 tools/be2.py schemas --write/--check` |

`physics` keeps the existing Rapier pin and `enhanced-determinism` setting. Shared
math, simulation, snapshot and save payload types have one definition; this change
does not introduce another simulation or save format. Pure rules and static map
authoring do not compile Rapier. The schema coordinator uses an internal native
`be2-schema` binary so generating schemas does not select stock-world or transport
dependencies; `be2-tools schema-generate` remains available in the full authoring tool.

`networking` selects QUIC, Rustls and Tokio. Lightweight protocol data, authenticated
development UDP and in-memory test transport remain available without it. Selecting
`TransportProfile::Production` without `networking` returns an actionable error;
it never falls back to plaintext UDP. Custom netplay does not inherit stock-server
interest management, partial world replication or demonstrated capacity; see
[NETPLAY.md](NETPLAY.md) for its actual limits.

For an existing `default-features = false` consumer, add `features = ["physics"]`
if its Rust code constructs `HeadlessWorld`, calls `SceneBuilder::world` or uses
`GameSession`/Rapier. Add `networking` if it constructs the production transport or
uses QUIC-specific types. Existing `client` consumers need no manifest change.
Pure custom-sim and 2D generated projects continue to test their rules without these
features. The custom-sim client selects `native-client`, which supplies existing
presentation, devices and Windows input support without stock authority or QUIC. The stock scaffold selects `physics` for its headless rules path.

Engine commands that need broad authoring or the stock server now declare their
features explicitly:

```sh
cargo build --locked --profile itest --no-default-features --features physics,networking --bin be2-tools
cargo run --locked --no-default-features --features physics --example prototype
cargo run --locked --no-default-features --features networking --bin be2-toy-server -- --info
```

Iteration chooses the owning feature flags and reports what it excluded. Integration
and shipping still execute default-client and stock-headless coverage plus narrow
boundary checks. Guarded Cargo tests declare their required features instead of
silently failing to compile or enabling unrelated dependencies for every consumer.
`python3 tools/check_headless.py` resolves normal-edge graphs per configuration and
the canonical full check also runs small external public-API fixtures. A lockfile's
membership is not evidence of a dependency being selected.

The measured resolved graphs, exact commands and toolchain are in
[docs/perf/dependency-boundaries/graphs.json](perf/dependency-boundaries/graphs.json).
Package counts describe selected package/version nodes, not compile time, runtime
performance, AI tokens or billed cost. The default client's graph is unchanged.
