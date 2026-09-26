# BlueEngine

<img src="assets/branding/blueengine.png" alt="BlueEngine official logo" width="128" height="128">

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
cargo run --locked --no-default-features --bin be2-headless -- --server 127.0.0.1:4000 --auth-key "LONG_RANDOM_SECRET"
cargo run --locked --bin be2 -- --connect 127.0.0.1:4000 --auth-key "LONG_RANDOM_SECRET"
cargo run --locked --no-default-features --bin be2-tools -- describe
cargo run --locked --no-default-features --bin be2-tools -- export-lab lab.json
cargo run --locked --bin be2 -- --map lab.json
```

Use the same map on both peers. Protocol 3 rejects different initial content.
Output files must be new. `export-house` and assets/maps/starters retain reference maps.
WASD/arrows move, mouse looks, Space jumps, Ctrl/C crouches, E carries/drops,
left click uses the demo tool, scroll selects tools, Q changes perspective, Esc pauses.
Scientist/Feta remain demo profiles. GameDocument v1 adds configurable movement and simple interaction objectives; see [game quickstart](docs/GAME_QUICKSTART.md).

## Current capabilities and limits

- Shared 60 Hz player simulation; graphics/audio-free headless build; Rapier props.
- Dedicated UDP server, client prediction, interpolation, spatial interest,
  acknowledged deltas/keyframe recovery and authoritative prop ownership/combat.
- Validated MapDocument authoring, stable semantic IDs, transactional edits,
  catalog assets, bounded discovery and route/capture tools.
- SceneBuilder/prelude for static boxes and catalog props; ID-based impulse/position APIs.

Networking is development-grade JSON/UDP with a 1400-byte packet limit. Supplying
`--auth-key` on both peers enables HMAC-SHA256 challenge-response, server-issued
session tokens, authenticated datagrams and replay protection. It does not encrypt
payloads, hide metadata or provide session migration. Large snapshots can exceed the
packet limit; bounded encoding is not snapshot chunking. Map fingerprints detect
accidental mismatch, not hostile forgery. Map v1 does not encode custom game rules,
multiplayer spawn profiles or arbitrary dynamic meshes. See the
[hosting guide](docs/HOSTING.md) for the runnable server's exact security boundary.

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

Try the data-driven objective demo with `Launch Three Switches.cmd` (after a release build), or `be2 --game assets/games/three-switches/game.json`. See [game quickstart](docs/GAME_QUICKSTART.md) and [implementation evidence and limits](docs/GAME_REFINEMENT.md).
