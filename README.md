# Blue Engine Antigravity (BEA) / Blue Engine 2 (BE2) — 0.2.0

A native Rust client and headless simulation engine foundation. Forked from Blue Engine 2 with playable environments (Suburban House, School Wing, Corporate Office, Convenience Store, Studio Sandbox), full prop physics, and completed Scientist weapon loadout (Wrench & Black Pistol).

## Credits & Attribution

- **Project Lead & Architecture Direction**: Kevin Ward ([@kevstermcgee](https://github.com/kevstermcgee))
- **Engine Development, Map Authoring & Simulation Systems**: OpenAI Codex
- **Weapon System, Antigravity Fork, Integration & Launchers**: Google DeepMind Antigravity

## Play

Launch via `launch_bea.bat`, `BEA.bat`, or run `bin/BEA.exe` (or `bin/BE2.exe`), then choose **The Scientist** or **Feta / Lab rat**. No installation or server is needed.

- **WASD / Arrows**: Move
- **Mouse**: Look around
- **Shift**: Sprint (Scientist)
- **Space**: Jump
- **Ctrl / C**: Crouch
- **Mouse Scroll**: Switch weapon (Scientist: Wrench <-> Black Pistol)
- **Left-Click**: Attack / Fire equipped weapon
- **E**: Pick up / drop nearby loose props
- **Q**: Switch between first-person and third-person camera
- **Esc / Tab**: Pause menu / controls
- **F / F11**: Toggle fullscreen

## Blue Test Lab (Default Development Map)

Bare launches (`cargo run --bin be2` or `bin/BEA.exe`) open the **Blue Test Lab** (`MapId::TestLab`), the primary test bed for engine architecture and validation:

- **Main Arena**: High ceiling, dual multiplayer spawn pads (`SPAWN_PLAYER_1`, `SPAWN_PLAYER_2`), weapon target practice wall, and interactive terminal.
- **Physics Lab (East Wing)**: Rapier dynamic rigid-body test stacks (crates and cereal boxes) for verifying momentum transfer, impulse response, and sleep/wake cycles.
- **Locomotion Lab (West Wing)**: Walkable staircases (standard 0.15 m risers), 15° and 30° ramps, elevated observation ledges, and low-clearance crawlspace (0.70 m ceiling) accessible to Feta / Lab Rat.
- **Room / Portal Graph**: Three connected zones with doorway portals validating spatial culling and network interest management.

## Reference / Legacy Content

The earlier procedural and furnished maps are preserved as reference content and asset catalogs:
- **Suburban House**: Two-story house with backyard (`--house` or `--map assets/maps/starters/house.json`).
- **School Wing, Corporate Office, Convenience Store**: Available via `--map assets/maps/starters/<name>.json`.
- **Studio Sandbox**: Minimal lighting and prop stage (`--studio`).

## Changes

- Softer baked shadows, roughness-aware highlights and restrained metal reflections.
- Fixed 60 Hz movement with interpolated display poses and bounded catch-up after stalls.
- Preserved between-frame action taps and reduced held-tool screen coverage.
- Removed persistent branding, control panels by default, and redundant hit text.
- Shared static vertices and reusable character/tool buffers reduce rendering work and allocations.
- Four new props: cereal box, chair, table and apple, with stable IDs and collision bounds.
- A true headless Cargo build with no graphics/window dependencies.

The active props use plain matte colours and bounded low-poly meshes. Five design accessories add framed sunset and botanical prints, a terracotta oval sculpture, leafy ceramic vase and catchall bowl. Examples are placed in the living room, kitchen and bedroom. Earlier prop assets are retained in `assets/legacy-props`.

## Reusable props

The props are in the house and in `assets/props/*.json`. Reuse them through `vesper3d::viewer::props::scene(PropKind)`, or export them to a new folder:

```sh
cargo run --no-default-features --example export_props -- my-props
```

Exports refuse to replace existing files. JSON uses the inherited scene format documented in AI_REFERENCE.md. Loose catalog props now have runtime rigid-body physics; disguises and possession belong to the upcoming Prop Hunt game layer.

## Build

```sh
cargo run --release --locked --bin be2
cargo build --release --locked --no-default-features --bin be2-headless
cargo run --release --locked --no-default-features --bin be2-headless -- --ticks 60000
cargo run --release --locked --no-default-features --bin be2-headless -- --ticks 600 --realtime
```

The second command is the intended build configuration on Debian/Ubuntu. Linux compilation and VPS performance have not been tested locally. CI includes Linux checks. The headless executable runs a bounded local two-player simulation; it does not open a network port.

`bin/be2-headless.exe` is the Windows headless build. `bin/vesper3d.exe` retains the original offline rendering CLI. The library remains named `vesper3d` for compatibility.

## Validation

```sh
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked --no-default-features
cargo clippy --all-targets --locked --no-default-features -- -D warnings
```

House screenshots: `BE2.exe --capture-house DIR`. The original studio remains available with `--studio`; include that flag for the legacy scripted captures below.

Visual smoke modes: `BE2.exe --capture DIR`, `--capture-props DIR`, `--capture-character DIR`, `--capture-interactions DIR`, `--capture-motion DIR`, and `--capture-wrench DIR`. Use a new directory; capture filenames are replaced inside the explicitly supplied directory.

See VALIDATION.md for measured results and limitations; BE2_ARCHITECTURE.md for the implementation and PulseNet plan. The original prototype documentation is retained in BLUE_V1_README.md and BLUE_V1_VALIDATION.md.

## Multiplayer Networking Architecture

Blue Engine V2 provides a modular, server-authoritative multiplayer pipeline:
- **UDP Transport (`UdpTransport`)**: Non-blocking UDP socket wrapper for low-latency datagram communication.
- **Server-Authoritative Simulation (`HeadlessWorld`)**: Continuous 60 Hz fixed-timestep physics and player simulation with 64-bit deterministic state checksums.
- **Client Prediction & Reconciliation (`PredictionBuffer`)**: Zero-latency local movement with authoritative server rewind and replay upon drift.
- **Snapshot Interpolation (`InterpolationBuffer`)**: Smooth remote entity rendering with configurable jitter buffer delay.
- **Spatial Interest Management (`snapshot_for_player`)**: Room/portal graph based relevance filtering so clients only receive deltas for entities in their own or adjacent rooms.
- **Delta Snapshots (`DeltaSnapshot`)**: Transmits only state changes and removals between simulation ticks.
- **Validated 1-Server + 2-Clients Tests**: End-to-end integration verified both in simulated network environments and over real loopback UDP sockets (`tests/multiplayer_transport.rs`).

## Agent editing toolkit

Start with `python tools/be2.py doctor`, then `python tools/be2.py map help`. The repository includes a native headless map editor, JSON edit transactions, spatial queries, collision floor plans, route/sightline checks, reusable prop placement, full validation, capture automation and release packaging with hashes. Both runtimes accept `--map FILE` for edited maps; the normal desktop launch still uses the built-in house.

See [tools/README.md](tools/README.md) for the complete workflow and [tools/FEATURES.json](tools/FEATURES.json) for the feature-to-source index. Distributed Windows builds include bin/be2-tools.exe for native editing without Python or a compiler.

## Playable characters

Every interactive map launch asks you to choose Feta or The Scientist. Feta is a white rat with red eyes, pink ears/paws, whiskers and an animated tail. His body is 0.30 m high with a 0.16 m collision radius and 0.22 m eye height. Normal movement is 5.6 m/s, matching the Scientist sprint; Shift does not boost it further. Crouching lowers his body to 0.20 m and speed to 2.8 m/s. He starts in third person; Q switches perspectives. He has no wrench. The 1.80 m Scientist wears a white lab coat and eyeglasses and retains normal movement and wrench controls. Reset position preserves your character.

Visual QA: `--studio --capture-character DIR --feta` renders Feta; omit `--feta` for the Scientist. Character selection is local; prop replication and multiplayer role rules remain future work.

## Picking up and dropping props

Aim at a loose prop within 2 metres and press **E** to carry it; press **E** again to drop it. Both Feta and The Scientist use the same controls. A contextual label identifies the target or carried item. The Scientist puts the wrench away while carrying. Dropped props fall, rotate, bounce slightly, settle with friction and can knock other loose props over. Objects stay solid while carried and can be blocked by walls. Escape freezes physics; Reset position releases the carried object first.

This applies to freestanding catalog props in the built-in and shipped JSON maps, including boxes, fruit, chairs, small tables, vases and tabletop decorations. Wall art, built-in furnishings and architecture remain fixed. Custom geometry using unrelated materials remains static. Changes to object positions last for the current session; relaunch restores the map. This does not add breakage, inventory, throwing controls or multiplayer ownership.

Physics visual smoke: `BE2.exe --studio --capture-physics NEW_DIR` (add `--feta` for the rat). The report records final prop positions alongside the rendered sequence.
