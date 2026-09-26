# BE2 agent editing toolkit

For source-free content authoring, start with `python tools/author.py describe` and [AUTHORING.md](AUTHORING.md). This compact JSON interface uses packaged binaries and adds bounded discovery, asset IDs, parameterized recipes and persistent regression reports without Cargo or engine source reads. The development runner below remains available for engine maintenance/builds.

Start with native `be2-tools describe` and `be2-tools search TEXT`; `export-lab NEW.json` exports the default Test Lab. Engine maintainers also read AGENTS.md and BE2_ARCHITECTURE.md. This toolkit lives with the repository, uses no AI service, and works for humans, Codex or other agents. The native editor is Rust; the workflow runner uses Python 3.10+ and only its standard library. No plugin installation is needed.

## First five minutes

Run from the repository root:

```sh
python tools/be2.py doctor
python tools/be2.py features
python tools/be2.py map help
```

Read tools/FEATURES.json to locate a feature's implementation, dependencies and checks. Inspect `git status --short` before edits; preserve existing work. `doctor` reports tools without installing or changing anything. Rust 1.87+ is required, along with rustfmt and Clippy. FFmpeg is needed for the full offline test suite. Linux graphical builds require ALSA development headers (`libasound2-dev`), plus `libudev-dev` and `pkg-config` for native gamepads; this revision was tested on Windows only.

The runner respects CARGO_HOME and CARGO_TARGET_DIR. It isolates client, headless and tooling release outputs so packages cannot accidentally include a graphics-enabled headless executable. Missing dependencies and failed checks return nonzero; commands do not continue past failures. Check logs go in ignored `.be2-work/check-*/`.

## Command inventory

| Command | Purpose |
|---|---|
| `python tools/be2.py doctor` | Read-only local toolchain and Git report |
| `python tools/be2.py features` | Machine-readable feature/file/check index |
| `python tools/be2.py check` | Formatting, both test configurations, both Clippy configurations; persistent logs and JSON result |
| `python tools/be2.py build client` | Locked release client and offline renderer |
| `python tools/be2.py build headless` | Locked release server simulation with default features disabled |
| `python tools/be2.py build tools` | Native editor without graphics/audio |
| `python tools/be2.py build all` | All three isolated builds |
| `python tools/be2.py capture NEW_DIR --map MAP.json` | Twelve house camera captures and rendered menu; checks completion |
| `python tools/be2.py package NEW.zip` | Builds a source + binary package with SHA256 manifest and Git provenance |
| `python tools/be2.py map ...` | Builds and invokes `be2-tools` with the arguments below |

`package` includes current Git-tracked working files plus newly built binaries. Add intended new source files to Git first. It records dirty status; it does not imply tests ran or commit your edits. Run `check` before packaging. It never updates the user's desktop shortcut or installed binaries automatically.

A distributed Windows copy also includes `bin/be2-tools.exe`, which needs neither Python nor Cargo. `be2-tools help` lists the native commands. Native operations return JSON reports/errors and nonzero exit codes on failure. `help` is plain text; inspect prints a full JSON document. Export commands write files.

## A complete map-edit session

Use a fresh directory (create `edits` first). These commands do not modify the built-in house:

```sh
python tools/be2.py map export-house edits/house.json
python tools/be2.py map audit edits/house.json
python tools/be2.py map floorplan edits/house.json edits/house.svg
python tools/be2.py map apply edits/house.json tools/examples/add-garden-props.json edits/garden.json
python tools/be2.py map diff edits/house.json edits/garden.json
python tools/be2.py map route edits/garden.json tools/examples/upstairs.route.json
python tools/be2.py map route edits/garden.json tools/examples/garden.route.json
python tools/be2.py map ray edits/garden.json 0,1.68,-6.3 4.4,0.98,-11.4
python tools/be2.py capture edits/captures --map edits/garden.json
```

Run a packaged map with `bin/BE2.exe --map edits/garden.json` or `bin/be2-headless.exe --map edits/garden.json --ticks 600`. Source runs can use `cargo run --locked --bin be2 -- --map edits/garden.json`. Without `--map`, the Blue Test Lab is the default. An edited JSON file becomes active only when explicitly selected. Keep finished maps in an appropriate versioned assets/maps directory if adopting them; do not assume an export changes the default.

`export-house` freezes the current procedural house, all its collision proxies and semantic entities into a versioned document. A snapshot's IDs stay stable while edited. Regenerating from changed Rust code can renumber `room-N` and `collider-N`; rebase patches against a new export deliberately. Newly added objects use your explicit IDs.

## Find, add, move and remove things

- `inspect MAP` dumps all visual nodes, materials, colliders and entities.
- `near MAP X,Y,Z RADIUS` returns IDs whose bounds overlap a sphere around that point. This is a candidate selection, not proof all returned parts belong to one object.
- `select MAP ID` selects exact IDs and their slash-prefixed children. It is useful for tool-added multi-part props, such as `garden-apple/0` and `/1`.
- `catalog` lists reusable prop kinds and supported operations.
- `export-scene MAP OUT` exports only the visual Vesper scene for the offline renderer. Collision/entities are intentionally absent from that format.
- `diff BEFORE AFTER` lists added/removed/changed nodes, colliders, entities and materials by ID.

Patch files are JSON arrays. Every field is explicit and unknown fields are rejected. `tools/patch.schema.json` describes their structure. Examples:

```json
[
  {"op":"add_box","id":"cover-cabinet","label":"Cabinet","center":[2,1,-8],"half_extents":[0.6,1,0.4],"color":[0.45,0.25,0.12],"structural":false},
  {"op":"add_prop","id":"spare-chair","label":"Chair","kind":"chair","origin":[-2,0,-9]}
]
```

`add_box` creates a matte box and matching collision proxy. It also creates an inspect entity unless `structural:true`; use structural boxes for floors, walls and ceilings that should not be reachability targets. Sizes are **half extents**, positions are metres, Y is up, RGB values are linear 0..1. `add_prop` reuses cereal, chair, table, apple, framed-art, framed-botanical, sculpture, vase-plant or bowl geometry; origin is the bottom of the prop. Both reject occupied IDs. Use `select` to enumerate the resulting components.

```json
[
  {"op":"translate","nodes":["cover-cabinet"],"colliders":["cover-cabinet"],"entities":["cover-cabinet"],"delta":[1,0,0]},
  {"op":"remove","nodes":["spare-chair/0","spare-chair/1","spare-chair/2","spare-chair/3","spare-chair/4","spare-chair/5"],"colliders":["spare-chair"],"entities":["spare-chair"]}
]
```

Use the actual lists returned by `select`; do not assume a future prop still has six nodes. For built-in geometry, visual, collision and semantic IDs are independent: use `near`, `inspect` and the floor plan to find all relevant pieces. Removing only a visual node leaves its collision proxy in place. Explicit lists permit legitimate visual-only details and multiple collision parts, so the editor does not guess ownership or silently delete nearby objects.

`apply` clones the source document, applies the complete batch, validates it and compiles the resulting geometry before creating the destination. A failed operation saves nothing and leaves the source untouched. **All output paths must be new**; existing files are never overwritten. Roll back by returning to the previous document or reversing the patch; there is no hidden undo database. Empty/unknown selections are errors. Unused materials are retained to avoid accidental shared-material removal.

## Validation and its limits

Map schema v1 is for portable, static, matte primitive maps. It accepts unparented box/sphere/cylinder/cone nodes with fixed positive scales; no animation, repeats, imported meshes, external audio or custom monitor/crystal actions. The legacy studio stays available via `--studio` and is not exported by this schema. Runtime entity text is owned, so loading maps does not leak strings or need unsafe code.

`audit` validates schema/version/IDs/transforms/bounds, requires and verifies `default_spawn`, compiles the map, reports counts and flags exact duplicate collision boxes. Duplicate boxes are advisory; overlap alone does not mean an error. It cannot prove the absence of every visual gap, floating object, trapped region or gameplay imbalance.

`route` uses the real controller at 60 Hz, starting at the map's explicit `default_spawn`. A route is a JSON array of `{x,z,feet,crouch?}`. Each straight waypoint leg has a 60-second simulation budget. It is a reachability regression, **not a pathfinder**: insert waypoints around obstacles. It returns failure at the first blocked waypoint. The supplied upstairs and garden routes exercise important house circulation.

`ray` tests actual render geometry between two points, not just physics proxies. It reports the first blocking surface and node ID. Use it for line of sight and cover checks. A clear ray does not imply a player can fit through a gap.

`floorplan` emits a lightweight SVG showing collision slices at 0.6 m and 3.8 m. Hover boxes for IDs; entity labels identify landmarks. The current view is sized for the house plot and the two current floor heights; it is not an automatic floor-discovery system. Tall/low objects may appear only in one slice. Inspect actual captures after map changes.

`capture` uses the current house camera tour. A future school/office/store will need its own capture positions in the client. Inspect PNGs and the menu; successful file generation is not visual approval. Hardware input, audio audibility and multiplayer balancing still require appropriate playtesting.

## Procedural generation and spatial analysis

- `lint MAP.json` audits spatial connectivity, identifying floating props, stairs to nowhere, unreachable rooms, and perimeter leaks.
- `reach MAP.json` evaluates topological reachability between all declared rooms and spawns.
- `walk-auto MAP.json FROM_X,Z TO_X,Z` plans an A* route between coordinates and verifies it with real physics simulation.
- `walk-explain MAP.json FROM_X,Z TO_X,Z OUT.svg` executes an auto-planned walk test and exports an SVG diagram of the route and any blocking obstacles.
- `blueprint-example OUT.json` writes a starter JSON blueprint specification for procedural map compilation.
- `build SPEC.json OUT.json` compiles a blueprint specification of rooms, corridors, and doorways into a full, lint-clean map.
- `scatter MAP.json KIND COUNT X1,Z1,X2,Z2 SEED OUT.json` procedurally places props within a 2D bounding rectangle with collision clearance.
- `line MAP.json KIND COUNT X1,Z1,X2,Z2 OUT.json` places props spaced evenly along a line segment.

## Simulation, tracing, and multiplayer verification

- `sim SCENARIO.json [TRACE.json]` runs an authoritative headless simulation scenario and writes an execution trace.
- `replay-trace TRACE.json [GAME.json]` replays a simulation trace tick-by-tick and verifies deterministic state checksums; game scenarios retain their resolved game path, and the optional argument overrides it.
- `replay-test` runs an automated 120-tick determinism and state checkpoint verification test.
- `net-test` runs deterministic multiplayer client-side prediction and server reconciliation under artificial network latency and packet loss.
- `net-proxy LISTEN UPSTREAM [PRESET]` runs a live UDP network impairment proxy with Gilbert-Elliott burst loss, Gaussian latency/jitter (Box-Muller), packet duplication, and standard bad network presets (`bad-wifi`, `mobile-3g`, `satellite`, `congested-bursty`).
- `bench` measures simulation steps, snapshot creation, delta compression, and room graph lookups; it returns nonzero when a checked-in regression budget is exceeded.
- `inspect-performance` measures memory, allocation, and tick budgets.
- `validate-budget` validates engine performance budgets and reports any violations.
- `new-game NAME DIRECTORY` scaffolds a complete game project with starter map and rule definitions.
- `ui-check` audits UI layout geometry and text fit across aspect ratios.
- `mcp` runs the Model Context Protocol (MCP) JSON-RPC stdio server exposing engine tools directly to AI agents.
- `doc-check [ROOT]` runs automated documentation drift protection, validating that all CLI commands, signatures, links, embedded JSON blocks, and feature files remain synchronized.

## Code feature workflow

For gameplay, graphics, audio, networking or new reusable prop types, use `FEATURES.json` to find the relevant source. The JSON map editor does not rewrite Rust or remove engine dependencies.

1. Identify shared simulation versus optional client code before changing a feature.
2. Make a narrow source change and preserve existing semantic IDs and feature boundaries.
3. Run focused tests for that behavior. Run the required complete `check` suite before delivery.
4. For visuals, capture and inspect the changed area and menu. For controls, exercise both key layouts, cursor capture and camera modes.
5. Update this guide and the feature index when entry points or contracts change. Record tested platforms and remaining limits in VALIDATION.md.
6. Build and package. Keep useful source, routes and patches tracked; keep scratch exports and logs out of the final asset set.

To remove a feature, inspect all references with `rg`, remove its runtime path and assets intentionally, then update callers, tests, feature flags and documentation together. Never remove a dependency solely because it appears unused in one binary: the offline renderer and headless build share this crate. For networking, preserve the rendering-free, transport-agnostic authoritative boundary and the explicit development/production profiles.

Accessory origins are at their bottom center. Framed prints are 1.10 m wide by 0.80 m tall and face +Z; place the back against a wall facing that direction. All accessory kinds create inspection entities and conservative collision bounds. Place tabletop pieces on an existing surface. Catalogue aliases: framed_art_1, framed_botanical_1, sculpture_1, vase_plant_1, bowl_1.

The catalogue also supports table-lamp, book-stack, candle-trio, potted-cactus, flower-vase, tall-vase, mantel-clock and woven-basket. See assets/props/DECOR_LIBRARY.md for dimensions and catalogue IDs. Lamps/candles are unlit static props and clock hands are fixed.

## Interior furnishing prefabs

Use `python tools/assets.py search TEXT` to discover both native props and all 53 data-only interior templates through one metadata API. Place interior templates with `python tools/place_interior.py MAP ASSET OUTPUT --id ID --at=X,Y,Z --yaw 90`. The native audit validates the result before a new file is created. This separate helper supports quarter turns and conservative inspection/collision bounds; these are not extra native add_prop kinds. See assets/README.md and assets/props/interiors/README.md.

## Engine API reference

Use the existing FEATURES.json index before opening source. Generate browsable
library contracts with `cargo doc --locked --no-deps --lib --open` (add
`--no-default-features` for the headless surface). `check` also builds library
rustdoc with warnings denied in both feature configurations; cargo test runs its
examples. This does not enforce documentation on every public item.

Read [the decision index](../docs/adr/README.md) for boundary tradeoffs and
[the glossary](../docs/GLOSSARY.md) for domain terms. The public simulation lifecycle
is exercised by tests/simulation_flow.rs. It is local simulation, not a network handshake.

Character engine edits: `CharacterKind` / `Controller::for_character` in controller.rs own body profiles. Capture both skins with `--studio --capture-character NEW_DIR` and a separate run adding `--feta`. Interactive launches always ask for a character; capture-only flags bypass selection for deterministic QA.

## Runtime loose props

`prop_physics.rs` is the engine entry point for E pickup/drop and rigid bodies. Existing native catalog placement remains the authoring path. The client recognizes freestanding semantic groups with `prop-`/`decor-` materials; custom geometry and wall art stay fixed. Data-only authoring does not tune masses or author scripts. Validate new catalog geometry with headless physics tests plus `--studio --capture-physics DIR` and a second capture with `--feta`. Runtime positions are not saved into the static source map.


Furniture clearance: runtime room loading replaces matching table/desk/chair/bench/workbench semantic envelopes with contained visible-part collision bounds. Use a furniture noun as the final label word (e.g. Student desk or Dining table). Keep the full entity envelope for selection/ownership. Solid pedestals remain solid. Existing shipped maps need no data rewrite. Tests: tests/furniture_clearance.rs; moved furniture ghost-proxy regression in prop_physics.rs.

Playable game presentation follows [the shared presentation contract](../docs/GAME_PRESENTATION.md). Use the `presentation` feature for `GameShell` and cached static rendering.

The same presentation feature exposes `game_text` for stable UI/signage, `game_visuals` for cached materials/weapons, and collision-safe local pose smoothing. See the contract for rendering order and authoritative-versus-cosmetic responsibilities.

## Sandbox workbench

`cargo run --release --locked --bin blueengine-sandbox` opens the catalog browser, character studio and map test client. Regenerate content with `python scripts/build_sandbox_content.py`, then run `python scripts/validate_sandbox.py` and the sandbox Rust integration suite. See [the workbench guide](../assets/games/blueengine-sandbox/README.md).

Sandbox creative play: Play / Create pairs any map with a character. V opens the
asset palette, click places a static prop, R rotates, wheel changes reach,
Shift+wheel changes height, G toggles grid, Delete removes placed props, Z undoes.
Per-map saves are in `.be2-work/sandbox-worlds`; `--creative-smoke NEW_DIR` runs
isolated placement/save/reload checks and captures. See the sandbox README.

## Native controllers

The optional `gamepad` feature exposes `viewer::gamepad::Gamepads` independently
of graphics; `client` enables it by default. Poll once per frame, including menus,
with the host's focus status. Device I/O never enters the fixed-step simulation.
See [bindings, API and platform requirements](../docs/CONTROLLERS.md). Controller
regressions run with `cargo test --locked --lib gamepad`; the normal headless build
must continue to pass `python tools/check_headless.py` without gilrs.


Sandbox Xbox input uses the same native backend for movement, look, menus and
creative placement; the header reports device status. `--sign-capture NEW_DIR`
checks moving-view sign rendering. SurfaceRenderer's sign pipeline reads world
depth without writing it, so transparent glyph rectangles cannot erase neighbors.
