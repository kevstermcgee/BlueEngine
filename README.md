# BlueEngine

<img src="assets/branding/blueengine.png" alt="BlueEngine official logo" width="128" height="128">

An AI-first Rust 3D engine foundation for building small, testable prototypes.

Curated games, prototypes, test content, and demos are copied automatically to
[BlueEngineGames](https://github.com/kevstermcgee/BlueEngineGames). See
[`docs/GAMES_PUBLISHING.md`](docs/GAMES_PUBLISHING.md) to validate an export or add
new content to the publication manifest. Ready-to-play Windows packages are on the
[latest BlueEngineGames release](https://github.com/kevstermcgee/BlueEngineGames/releases/latest).
Continues the complete BlueEngineAntigravity history. The primary development
fixture is **Blue Test Lab**; furnished legacy maps remain reusable reference assets.

## Start here

- Content authors: `python tools/author.py describe`, then [tools/AUTHORING.md](tools/AUTHORING.md).
- Asset discovery: `python tools/assets.py search "desk lamp"`; see the [asset library contract](assets/README.md).
- Native discovery: `be2-tools describe`, `be2-tools search multiplayer`, `be2-tools catalog`.
- Rust prototypes: [compact quickstart](docs/AI_QUICKSTART.md), `cargo run --locked --no-default-features --example prototype`.
- Custom presentation: [visible-client boundary](docs/CUSTOM_CLIENT.md), `cargo run --locked --example custom_client`.
- Behavioral tests: [scripted headless scenarios](docs/BEHAVIORAL_TESTING.md).
- Engine maintenance: [AGENTS.md](AGENTS.md), [current architecture](BE2_ARCHITECTURE.md), [decisions](docs/adr/README.md).

Cargo package/binaries remain `be2`; the library remains `vesper3d` for compatibility.

## Run and author

```sh
cargo run --locked --bin be2
cargo run --locked --no-default-features --bin be2-headless -- --server 127.0.0.1:4000 --transport development
cargo run --locked --bin be2 -- --connect 127.0.0.1:4000 --transport development
cargo run --locked --no-default-features --bin be2-headless -- --server 127.0.0.1:4000 --auth-key "LONG_RANDOM_SECRET"
cargo run --locked --bin be2 -- --connect 127.0.0.1:4000 --auth-key "LONG_RANDOM_SECRET"
cargo run --locked --no-default-features --bin be2-tools -- describe
cargo run --locked --no-default-features --bin be2-tools -- export-lab lab.json
cargo run --locked --bin be2 -- --map lab.json
```

Use the same map/game on both peers. Protocol 5 rejects different initial content.
Output files must be new. `export-house` and assets/maps/starters retain reference maps.
WASD/arrows move, mouse looks, Space jumps, Ctrl/C crouches, E carries/drops,
left click uses the demo tool, scroll selects tools, Q changes perspective, Esc pauses.
Scientist/Feta remain demo profiles. GameDocument v1 adds configurable movement and simple interaction objectives; see [game quickstart](docs/GAME_QUICKSTART.md).

## Current capabilities and limits

- Shared 60 Hz player simulation; graphics/audio-free headless build; Rapier props.
- Transport-agnostic authoritative server, client prediction, interpolation, spatial interest,
  acknowledged deltas/keyframe recovery and authoritative prop ownership/combat.
- Validated MapDocument authoring, stable semantic IDs, transactional edits,
  a structured and extensible asset catalog, bounded discovery and route/capture tools.
- SceneBuilder/prelude for static boxes and catalog props; ID-based impulse/position APIs.

Networking uses one JSON packet path with an 1100-byte cross-transport ceiling.
`--transport development` (the compatibility default) is raw, unencrypted UDP for
local work and impairment testing. `--transport production` uses QUIC datagrams over
TLS 1.3 and pins the server certificate selected by `BLUE_TLS_CERT_FILE` (or the
bundled default); the server loads its PKCS#8 key from `BLUE_TLS_KEY_FILE`. Supplying `--auth-key` on both peers additionally enables
client HMAC challenge-response, session tokens and replay protection. Neither profile
hides traffic metadata or provides session migration. Large snapshots can exceed the
packet limit; bounded encoding is not snapshot chunking. Map fingerprints detect
accidental mismatch, not hostile forgery. See the
[hosting guide](docs/HOSTING.md) for the runnable server's exact security boundary.

## Validation

`python tools/be2.py check` runs formatting, rustdoc, tests and Clippy in both feature
configurations. CI runs on Linux and Windows. Focused suites cover prototype APIs,
native capability evidence, content handshakes, malformed packets and multiplayer
including a separate server process and a QUIC authoritative handshake. Headless
microbenchmarks enforce checked-in absolute regression budgets in tests and in
`be2-tools bench`. See [the refinement report](docs/REFINEMENT.md) for context.

## History and attribution

Kevin Ward directed the project; OpenAI Codex contributed engine/tooling development;
Google DeepMind Antigravity contributed the earlier fork, integrations and weapons.
Original authorship, Git history and MIT license are preserved. BlueEngine continues
that work with OpenAI Codex. Historical Vesper/Blue v1 documentation remains for
asset/offline-renderer compatibility; current runtime guidance is linked above.

Try the data-driven objective demo with `Launch Three Switches.cmd` (after a release build), or `be2 --game assets/games/three-switches/game.json`. See [game quickstart](docs/GAME_QUICKSTART.md) and [implementation evidence and limits](docs/GAME_REFINEMENT.md).
