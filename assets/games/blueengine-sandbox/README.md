# BlueEngineSandbox

The engine's local content workbench: explore maps, inspect the complete registered
prop/prefab library, compare character designs, and reproduce experiments.

## Start

Unzip the Windows package and run **blueengine-sandbox.exe**. Keep its `assets`
folder beside the executable. Nothing is installed and no server is required.

From the BlueEngine source root:

```sh
cargo run --release --locked --bin blueengine-sandbox
```

Build the optional stock physics client alongside it:

```sh
cargo build --release --locked --bin blueengine-sandbox --bin be2 --bin be2-tools
```

## A repeatable experiment

1. Choose **Assets**, search by name/tag, and select a specimen. The preview uses
   the actual catalog geometry. Dimensions and source path are shown below it.
2. Drag to orbit, scroll to zoom, or enable automatic orbit. Toggle collision
   bounds to compare the rendered parts with the engine's actual movement bounds.
3. Press **Tab** to walk the current preview. WASD or arrows move, mouse looks,
   Space jumps, Shift sprints, and C/Ctrl crouches. Tab returns to the browser.
4. Q switches first/third person. Home restores spawn. B toggles collision bounds.
5. Choose **Play / Create** to pair a character with any map and build there.
   V opens the asset palette in creative play. The original Scientist and Feta
   skins share the same preview path as their stock engine counterparts.
6. **Export map** saves a new editable MapDocument in
   `.be2-work/sandbox-exports/`; source maps are never overwritten.

F/F11 toggles fullscreen. Escape opens the shared Resume / Controls / Quit menu
and pauses this local session. Losing focus releases the cursor. F3 displays
frame rate and fixed simulation tick count. Browser selection also supports arrows
and Enter. Click the search field before typing; F remains text while it has focus.

**Physics session** opens the sibling `be2` executable in a separate window with
the chosen map. E carries/drops eligible native props there. That stock client has
its own Scientist/Feta selection; the six cosmetic skins are available in the
sandbox client. Its pickup rules do not make every interior prefab or new kit prop
movable. Close the physics window when finished, then return to the sandbox.

## Maps

| Destination | Purpose |
| --- | --- |
| Asset Atrium | All 78 registered specimens on labeled stands/locations, with a clear blue central aisle |
| Calibration Range | Metric grid, 16 cm stairs, cover, and 0.42 / 1.18 / 1.90 m clearance tunnels |
| Cedar Courtyard | Outdoor circulation, benches, planters, trees and a pergola |
| Foundry Yard | Two loading sheds, crates/pallets, work lights, cover and sightlines |
| House, School Wing, Office, Convenience Store | Furnished starter maps with their expanded grounds |
| Blue Test Lab | The engine's native test fixture |

The starter copies add explicit spawn metadata to older documents; originals are
untouched. New maps and specimen rooms are ordinary MapDocument v1 files and can
also be loaded by the native client/headless tools.

## Characters

| Character | Design / movement |
| --- | --- |
| Scientist | Original human, 1.80 m collision hull |
| Feta | Original white rat, 0.30 m hull |
| Dusty Trails | Cowboy hat, bandana, sheriff badge, leather vest and boots |
| Nova Visitor | Green alien, large eyes, antennae and a survey backpack |
| Bolt-7 | Maintenance robot with cyan visor, amber panel and articulated limbs |
| Orbit Scout | Cream pressure suit, round helmet, blue visor and life-support pack |
| Brass Diver | Copper diving helmet, porthole and twin air tanks |
| Cedar Ranger | Green field jacket, canvas pack, broad hat and binoculars |

The six cosmetic characters use the **unchanged human controller**. Walking animates
limbs by distance; crouching changes the displayed stance. Hats, antennae and
accessories are cosmetic and can extend beyond the 1.80 m collision hull. There
are no custom powers, combat rules, or character-specific physics.

## Content authoring and validation

The 78 specimens comprise 17 native props, 53 interior prefabs, and eight new
game-local assets: crate, pallet, bollard, planter, pine, gantry, bench and work
light. `assets.json` registers the new kit with provenance and reusable scenes.
The browser is a generated snapshot of the catalog, not a recursive scan of every
audio/image/source file in the repository.

Run these from the engine root after changing the source catalogs:

```sh
python scripts/build_sandbox_content.py
python scripts/build_sandbox_content.py --check
python tools/assets.py --include assets/games/blueengine-sandbox/assets.json validate
python scripts/validate_sandbox.py
cargo test --locked --test sandbox
```

`build_sandbox_content.py` deterministically rewrites only this sandbox's generated
catalog, maps, routes, kit scenes and specimen previews. Edit the generator for
persistent changes; hand edits to generated JSON will be replaced on regeneration.
Custom character meshes are cached in `src/bin/sandbox/characters.rs`.

The walk/preview mode intentionally keeps props static and uses BlueEngine's
Controller and PlayerStepper at 60 Hz. It does not run hidden rigid bodies against
static visuals. The stock physics client is the explicit dynamic-prop test mode.

For deterministic graphical evidence:

```sh
target/release/blueengine-sandbox --map courtyard --capture .be2-work/courtyard
target/release/blueengine-sandbox --character cowboy --capture .be2-work/cowboy
target/release/blueengine-sandbox --asset sandbox/pine --capture .be2-work/pine
```

Capture writes browser, walking and real pause-menu screenshots, plus frame-time
percentiles. `--root DIRECTORY` locates a separate content tree. Captures do not
claim to verify physical input or gameplay. See `VALIDATION.md` in the package.

## Publishing / packaging

Content is already covered by the engine's `assets/games` publication entry.
The custom executable is built from BlueEngine, not the stock game-document release
launcher. `scripts/package_sandbox.py --output NEW_DIRECTORY` packages the optimized
Windows binaries and content; it refuses to replace an existing directory.

All new source/content is MIT under the engine LICENSE. The official BlueEngine
rat artwork is preserved. The embedded UI font retains its supplied license.


## Creative play

Choose **Play / Create**, select any of the nine maps and any of the eight
characters, then choose **Play selected world**. This loads that map's saved build
if present. The wizard remains retired. All skins except Feta use the human hull.

- **V** opens a searchable, paged palette of all 78 assets while in the map.
- Select an asset to close the palette and show a translucent placement preview.
- **Left click** places a copy. **Right click** cancels the preview.
- **R** rotates by 90 degrees. **Mouse wheel** changes reach (2–30 m).
- **Shift + wheel** adjusts elevation; **G** toggles a 25 cm placement grid.
- Aim at surfaces to stack objects, or use reach/elevation to build in open space.
- **E** copies an aimed catalog asset into the placement preview.
- **Delete** removes an aimed player-placed object within 4.5 m; original map
  scenery is protected. **Z** undoes up to 16 edits in the current session.
- **Q** changes perspective, **Home** returns to spawn, **Tab** opens the workbench,
  and **Escape** pauses. Palette text entry suppresses gameplay shortcuts.

Edits save automatically after each successful placement, removal or undo to
`.be2-work/sandbox-worlds/<map-id>.json` beside the content root. Switching maps,
characters or restarting restores those objects; the undo history is session-only.
The workbench's **Export map** also exports the edited map. Save files are written
before replacing the active world, preserving the prior save on validation/write
failure. A failed save reports an error instead of silently discarding work.

Objects are static, collidable props. This is local creative construction, with
no multiplayer building, terrain mining, flight, or movable-prop physics. Overlap
with scenery is allowed for creative building, but previews turn red at the player
or protected spawn area and those placements are rejected. Maps retain the native
5000-item and 8 MB limits, with at most 256 player-placed objects per world.

For isolated repeatable checks (use new output directories):

```sh
cargo test --locked --test sandbox
blueengine-sandbox --creative-smoke .be2-work/creative-check
blueengine-sandbox --map calibration --character astronaut --play --save-dir .be2-work/manual-worlds
```

The creative smoke path exercises real app placement, deletion, undo, character
switching and map reload, then captures setup, world preview, palette and pause
screens. It writes only its own output directory. It does not simulate physical
keyboard/mouse events.


## Xbox controller

The sandbox polls native controllers every frame, including menus and hot-plug.
The workbench header reports the connected device or initialization error.
- Left stick: move; right stick: look; A: jump; B: crouch; LS click: sprint.
- Y: open Play/Create from the workbench or the asset palette in a world.
- D-pad: move menu focus; A: choose; B: back; Start: pause/resume.
- LB/RB: scroll the workbench or page the asset palette.
- RT: place; LT: cancel preview; RB: rotate; View: toggle grid.
- D-pad up/down: placement distance; hold LB with up/down for height.
- D-pad left: undo; D-pad right: remove aimed placed object; X: copy aimed asset.
- RS click: first/third-person camera. Keyboard and mouse remain available.

`--sign-capture NEW_DIR` captures the atrium's Red apple label from nine camera
angles for regression inspection. Signs test against world depth but never write
depth, preventing overlapping transparent glyph tiles from erasing letter strokes.
