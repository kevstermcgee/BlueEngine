# BlueEngine

An AI-first Rust 3D engine foundation for building small, testable prototypes.
Continues the complete BlueEngineAntigravity history. The primary development
fixture is **Blue Test Lab**; furnished legacy maps remain reusable reference assets.

## Start here

- Content authors: `python tools/author.py describe`, then [tools/AUTHORING.md](tools/AUTHORING.md).
- Native discovery: `be2-tools describe`, `be2-tools search multiplayer`, `be2-tools catalog`.
- Rust prototypes: [30-line quickstart](docs/AI_QUICKSTART.md), `cargo run --locked --no-default-features --example prototype`.
- Engine maintenance: [AGENTS.md](AGENTS.md), [current architecture](BE2_ARCHITECTURE.md), [decisions](docs/adr/README.md).

Cargo package/binaries remain `be2`; the library remains `vesper3d` for compatibility.

## Run and author

```sh
cargo run --locked --bin be2
cargo run --locked --no-default-features --bin be2-headless -- --server 127.0.0.1:4000
cargo run --locked --bin be2 -- --connect 127.0.0.1:4000
cargo run --locked --no-default-features --bin be2-tools -- describe
cargo run --locked --no-default-features --bin be2-tools -- export-lab lab.json
cargo run --locked --bin be2 -- --map lab.json
```

Use the same map on both peers. Protocol 2 rejects different initial content.
Output files must be new. `export-house` and assets/maps/starters retain reference maps.
WASD/arrows move, mouse looks, Space jumps, Ctrl/C crouches, E carries/drops,
left click uses the demo tool, scroll selects tools, Q changes perspective, Esc pauses.
Scientist/Feta are current demo profiles, not a generic gameplay document system.

## Current capabilities and limits

- Shared 60 Hz player simulation; graphics/audio-free headless build; Rapier props.
- Dedicated UDP server, client prediction, interpolation, spatial interest,
  acknowledged deltas/keyframe recovery and authoritative prop ownership/combat.
- Validated MapDocument authoring, stable semantic IDs, transactional edits,
  catalog assets, bounded discovery and route/capture tools.
- SceneBuilder/prelude for static boxes and catalog props; ID-based impulse/position APIs.

Networking is development-grade JSON/UDP with a 1400-byte packet limit. There is no
cryptographic authentication, encryption, binary codec or session-token migration.
Large snapshots can exceed that limit; bounded encoding is not snapshot chunking.
Map fingerprints detect accidental mismatch, not hostile forgery. Map v1 does not
encode custom game rules, multiplayer spawn profiles or arbitrary dynamic meshes.

## Validation

`python tools/be2.py check` runs formatting, rustdoc, tests and Clippy in both feature
configurations. CI runs on Linux and Windows. Focused suites cover prototype APIs,
native capability evidence, content handshakes, malformed packets and multiplayer
including a separate server process. See [the refinement report](docs/REFINEMENT.md)
for this pass's measured results and remaining work.

## History and attribution

Kevin Ward directed the project; OpenAI Codex contributed engine/tooling development;
Google DeepMind Antigravity contributed the earlier fork, integrations and weapons.
Original authorship, Git history and MIT license are preserved. BlueEngine continues
that work with OpenAI Codex. Historical Vesper/Blue v1 documentation remains for
asset/offline-renderer compatibility; current runtime guidance is linked above.
