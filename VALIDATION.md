# Interior placement revision � 2026-09-23

Rearranged the living room around a sofa facing a TV mounted on the solid partition, with a low coffee table and wall-adjacent console. Moved the fridge into the counter run, faced its doors into the kitchen, and removed its duplicate collider. Dining chairs now face the table from opposite sides. Moved the master bed headboard to the rear wall, placed the wardrobe beside it on a solid wall section, and added a bedside cabinet. The smaller bedroom uses a narrower single bed with a usable side aisle. Removed the detached hall cupboard, misplaced kitchen island, floating sideboard and unnecessary bedroom divider.

Preserved semantic IDs for moved furniture and updated visual, collision and entity bounds together. Indoor cover now uses a tested crouched pocket behind the sofa; outdoor cover is unchanged. Expanded route checks verify access beside the small bed and in front of the wardrobe, plus a new living-room/kitchen working-aisle route. Existing stair, doorway, backyard and cover-occlusion checks pass.

83 default tests and 60 no-default-feature tests pass, as do formatting and both Clippy configurations. One earlier run encountered a Windows access-denied error in the unchanged offline-output replacement test; the full suite passed on retry. Client, headless and authoring release builds are updated. Inspected kitchen, both bedrooms, living-room views and the actual menu. Hardware input, physical audio and Linux runtime were not retested; input handling and audio were unchanged. The desktop shortcut continues to target bin/BE2.exe.

---

# Landscaping revision � 2026-09-23

Replaced two pole-and-sphere trees with tapered branching broadleaf trees and irregular seven-cluster crowns. Added two layered pine trees. Rebuilt all twelve shrubs from six varied leaf clusters with small stems; preserved shrub collision cores. Added 48 cream/pink/gold flowers across bed edges and four entrance/patio planting patches. All geometry is static matte primitives, without textures, transparency, per-frame animation or new dependencies.

The house circulation, upstairs access, fence containment and hiding-pocket route/occlusion tests pass. The full 82 default and 59 no-default-feature tests, formatting and both Clippy configurations pass. Authoring round-trip tests cover the new cone/cylinder/sphere/box assets. Client, headless and editor release builds are included.

Inspected exterior, backyard, side-yard vegetation and actual menu captures. Whole-house render: 29,316 triangles, 36,987 shared vertices, 10 batches at 1024x697. The short capture averaged 16.940 ms/frame; it is a smoke check, not a sustained GPU benchmark or a claim to have eliminated hardware stutter. House instance budget is now 1,400 to accommodate static flower parts. Native controls, physical audio and Linux runtime were not retested for this visual-only change.

The existing desktop shortcut targets the updated bin/BE2.exe. Shared landscaping builders and the feature index are documented for later maps.

---

# Agent authoring toolkit � 2026-09-23

Added a rendering-free be2-tools executable and tools/be2.py workflow entry point. Both the graphical client and headless simulation load validated static map documents through --map. Default play still uses the procedural house; demonstration edits do not modify it. Runtime entity/name strings are owned rather than leaked static allocations.

82 default tests and 59 no-default-feature tests pass, including five authoring integration tests covering map round-trip/build, synchronized added components, move/removal, invalid versions/fields/IDs, failure preservation, exclusive file creation, successful CLI patches, spatial selection, controller routes and custom-map headless execution. Formatting and both all-target Clippy configurations pass with warnings denied. The workflow runner's check command completed and saved individual logs plus a JSON report.

Built client/offline, headless and tools releases in feature-isolated target directories. Exercised doctor, feature index, native help/catalog, export, audit, apply, diff, select, near, ray, floorplan and route commands. Exported the house, applied the sample planter/apple patch, and completed upstairs and garden controller routes. A sightline query hit the expected garden screen. Audit reports the inherited duplicate fridge collision proxy as advisory rather than silently modifying it.

The edited document loaded in the headless runner for 600 ticks and the graphical client for twelve capture views. Inspected exterior, added planter/apple and actual menu. The sample edit rendered 10,092 triangles in four batches. No new graphic asset or gameplay feature was added to the default house. Capture automation completed; the generated file set and report were checked. Native input and physical audio were not retested because their handling did not change.

Limits: map v1 covers static matte primitive scenes and inspect actions. Original generated component IDs are stable within a snapshot, not across regenerated exports from modified Rust. Visual/collision/entity components have explicit independent selections; ownership is not inferred. Routes are waypoint regressions, not pathfinding; the floor-plan slices and capture tour target the current house. Windows x64 is locally verified; Linux runtime is not. Packaging uses tracked working files, reports dirty status, and includes a SHA256 manifest; running it does not substitute for tests.

---

# House cleanup and hiding cover � 2026-09-23

Fixed open roof gables, gaps above upstairs walls, incomplete doorway headers, unsupported stair posts and headboard collision coverage. The bathroom now uses recessed tub/basin shapes and a recognizable toilet; the wardrobe and new cabinets have doors/handles. The tub base and rim abut without coplanar overlap.

Added twelve simple faceted shrubs with planting beds, a framed L-shaped garden screen, a hall cabinet, living-room sideboard, kitchen island and bedroom divider. The main entrances, stairway and upstairs doorways remain traversable. Cover has predictable collision cores; shrub leaves are opaque static meshes, with no alpha textures or animation.

77 default tests and 54 no-default-feature tests pass. Formatting and both all-target Clippy configurations pass with warnings denied. Client and headless release builds pass. New regressions check gable closure, wall tops, reachable indoor/outdoor cover pockets, a garden escape route and actual world-ray occlusion from entrances. Existing stair and every-room traversal checks pass.

Expanded --capture-house to twelve views and inspected the house exterior, living room, kitchen, stairs, both bedrooms, bathroom, hall, side yard, backyard, cover viewpoint and actual menu across the revision. Final geometry: 9,924 triangles, 16,578 shared vertices, 4 batches. The short capture runs are smoke checks, not multiplayer balance tests or proof that all display stutter is resolved. Hardware-controlled play and Linux runtime were not retested.

Updated client/headless binaries and previews/house-cover are included. The existing desktop shortcut targets the updated client. Future Prop Hunt disguises and multiplayer rules remain outside this map pass.

---

# House and wrench sound revision — 2026-09-23

75 default tests and 52 no-default-feature tests pass. Formatting and Clippy (all targets, warnings denied) pass in both feature configurations. Client and headless release builds pass.

New traversal tests verify walking up and down the stairs without jumping, entry into every upstairs room, the back doorway and fence containment. Spawn clearance, entity ID uniqueness and a geometry instance budget are checked. Collision step-up is limited to 22 cm and requires ground contact and headroom.

Seven release captures were inspected: exterior, living room, kitchen, stairs, bedroom, backyard and menu. House scene: 6,276 triangles, 11,787 vertices, 3 batches. A short 960x600 capture averaged 17.363 ms per frame; this is a smoke run, not a sustained performance benchmark or verification of the user's display stutter.

The scripted studio wrench capture reported one confirmed hit and one audio playback call. The generated PCM asset and hit-only trigger were checked; speaker audibility on the user's hardware was not verified. Headless dependency inspection confirms Serde/JSON only, with no audio or graphics dependencies.

House headless smoke: two players, 60,000 ticks in 327.844 ms on this Windows host. This is not a VPS capacity estimate. Linux runtime and hardware keyboard/mouse traversal were not retested for this revision.

Updated bin/BE2.exe is the target of the existing desktop shortcut. Selected house captures are in previews/house. Only the house is implemented from the four-map roadmap; multiplayer and Prop Hunt rules remain future work.

---

# Simple-prop revision � 2026-09-23

Replaced the active crate/barrel/stool/toolbox set with a cereal box, chair, table and apple. Old JSON assets retain their IDs in assets/legacy-props. The new props have new IDs.

Validation: 71 default-build tests and 48 headless tests pass; formatting and both Clippy configurations pass. Default and no-default-feature headless release builds pass. The new geometry-budget test checks all four simple meshes, matte tags and index validity. Both room entity/collision checks and standalone scene compilation pass.

The active props total 324 triangles, versus 1,164 for the earlier set (72% reduction). Full room: 50,996 triangles, 37,879 shared vertices, 17 batches. Static face colours and no view-dependent highlights are used for these props. Thin decorative overlaps were removed; these were a plausible flicker source, not a confirmed diagnosis of the user's observed stutter.

Rendered and visually inspected overview, cereal/apple close-up, chair close-up and the actual pause menu. No obvious surface overlap artifacts appear in these stills. Motion on the user's display has not been verified, so do not claim all gameplay stutter is resolved. No controls or movement logic changed in this revision.

The rebuilt client was copied to bin/BE2.exe and its SHA256 matched the build output. The existing desktop shortcut continues to target that file. The headless binary and standalone exported prop scenes were updated as well.

---

# BE2 validation — 2026-09-23

## Passed locally on Windows x64

- `cargo fmt --check`.
- `cargo test --locked`: 70 tests (55 library, 2 client input, 13 offline CLI).
- `cargo clippy --all-targets --locked -- -D warnings`.
- Default release build, including client and inherited offline CLI.
- `cargo test --locked --no-default-features`: 48 tests.
- `cargo clippy --all-targets --locked --no-default-features -- -D warnings`.
- Release headless build with default features disabled.
- Headless dependency tree contains Serde/JSON only; no Macroquad, Miniquad, image/PNG, Ctrl+C/window library or networking transport.
- Two-player headless benchmark: 60,000 ticks in 33.692 ms (0.562 microseconds per tick) on this host. This exercises movement, stance, jumping and room collision, without networking or game rules. It is not a VPS capacity estimate.
- Paced runner: 60 ticks in 1000.202 ms. Headless Windows executable: 355,840 bytes.
- Prop exporter produced four standalone scenes; each compiled through the engine in a test. Stable prop entities have matching collision bounds, and the player spawn is clear.
- Release room, props and character capture modes completed. PNGs were inspected, including the actual rendered pause menu, clean HUD, softer scene lighting, and reduced first-person wrench coverage. Selected images are in previews/.

## Smoothness and correctness

Movement tests cover 30/60/144/240 Hz presentation rates, bounded long-frame catch-up, a jump queued across a sub-tick frame, reset interpolation, matching headless/client motion (floating-point tolerance), invalid input, maximum player count, existing wall/ceiling collision and jump/crouch behavior. Both keyboard layouts and between-frame press/release edges have automated tests.

Native UI automation successfully clicked Enter the room. Keyboard injection did not provide reliable observable results in the test session; live WASD/arrows, mouse capture/release and Escape/Q should receive a human playtest. The regression fix preserves event-subscriber press edges instead of relying on a brief native-poll sample. Do not interpret unit tests as a completed hardware-input playtest.

## Performance observations

The final static room, including four added props, has 51,836 triangles and 38,918 shared vertices. The equivalent unshared triangle stream has 155,508 entries: about 75% fewer vertex entries. The baked shade cache avoids recomputing identical vertices. Tool buffers reuse their meshes and the character reuses geometry capacity.

An initial 1024x697 capture measured original startup at 0.657 s versus BE2 at 0.366 s. Mean frame times were 17.338 versus 17.366 ms, near display/presentation pacing; this does not demonstrate an FPS gain. The final hidden-window 960x600 smoke measured startup at 0.305 s and mean frame time at 17.029 ms. Different viewports and short, vsync-paced captures are not controlled GPU benchmarks.

## Remaining scope

Linux compilation, actual Debian/Ubuntu VPS memory/CPU usage, GPU/driver coverage and sustained playtesting are not verified here. CI contains Linux and Windows checks but was not executed remotely.

PulseNet transport integration, authentication/session lifecycle, packet sequencing, snapshots, prediction/reconciliation, server combat and interactions, player collision, disguises, prop possession, rounds and scoring are not part of this release. Props are static inspectable/hittable assets. The headless executable is a local simulation/benchmark, not an online listener.

