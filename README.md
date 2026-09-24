# Blue Engine 2 (BE2) — 0.2.0

A native Rust client and headless simulation foundation for our Prop Hunt project. Forked from the latest Blue Engine working code, with a Scientist seeker, Feta lab rat, wrench, first/third-person camera and Git history.

## Play

Open `bin/BE2.exe`, then choose **Feta / Lab rat** or **The Scientist / Seeker**. No installation or server is needed. Windows x64 is the locally tested build.

WASD/arrows move; mouse looks; Shift sprints; Space jumps; Ctrl/C crouches; Q switches camera; left-click swings the wrench; Escape/Tab pauses. H enables optional help; F3 enables diagnostics. The default view shows the crosshair, wrench hit feedback and a contextual E pickup/drop hint. The seeker has no object inspection menu. E picks up or drops a nearby loose prop for either character. Right-click and R remain available for future disguise controls. F/F11 toggles fullscreen.

## House map

The default client and headless world now use the two-story suburban house: living room, kitchen/dining area, two bedrooms, bathroom, walkable stairs, and a fenced backyard with patio furniture. The existing desktop shortcut launches this updated map. Walk up the stairs normally; jumping is not required.

All house geometry uses simple matte shapes. The cereal box, apple, chair and table share the reusable prop definitions. A quiet, short impact sound plays when the wrench makes contact; missed swings stay silent.

The cleanup pass closes the roof gables and wall gaps, completes the stair railing, and adds 12 outdoor planting beds. The garden privacy screen and a pocket behind the living-room sofa provide cover while keeping main routes open. Furniture follows room layouts: a sofa facing the wall-mounted TV, fridge beside the counters, inward-facing dining chairs, and beds/storage against walls.

Landscaping includes branching broadleaf trees, layered pines, clustered shrubs and 48 small flowers in cream, pink and gold. Plant geometry stays static and matte, with shared builders available for future maps.

Four furnished maps are playable through the named desktop shortcuts or `--map assets/maps/starters/NAME.json`: house, school-wing, office and convenience-store. Each has been expanded to roughly double its previous floor/yard area, with 269 additional loose physics props in total. See assets/maps/starters/README.md for the additions and validation. A bare executable launch retains the original procedural house.

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

## Next: Prop Hunt and PulseNet

This release improves the engine and adds props. It does not yet implement online multiplayer, prop disguises, competitive hunter/hider rules, rounds or scoring. The next phase connects the discussed PulseNet utility to the headless world, adds authoritative game rules, and validates one server with two clients. Offline play remains available.

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
