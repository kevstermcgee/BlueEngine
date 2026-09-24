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

# Source-free authoring toolkit validation (2026-09-23)

Added tools/author.py, tools/authoring.json and tools/AUTHORING.md. No engine Rust files were read or changed for this addition. Tested against the existing Windows packaged binaries. `python -m unittest discover -s tools -p test_author.py -v` passed all four tests: discovery/JSON errors, every asset and recipe through native compilation, failed-transaction preservation, and routes/persistent failure reports. The seven asset/recipe fixtures all compiled and audited successfully.

A garden_planter instance passed the thirteen-waypoint upstairs route and seven-waypoint garden route. The custom map loaded in the packaged headless runtime for sixty ticks. Two graphical verification runs each produced all twelve captures and a render report. Eleven hashes matched between runs; blue-engine-12.png (menu) differed and was correctly marked review_required. Inspected garden and menu images; the existing blue-engine-11.png camera faces the fence and does not provide useful general scene coverage. The inherited camera tour is not a complete visual-quality oracle. No input behavior or engine visuals changed; the Rust engine suite was not rerun for this Python/data/docs-only addition. Linux and macOS were not tested.
# Forged wrench visual revision (2026-09-23)

Replaced the rectangular three-block head with an angled open-end profile, parallel gripping faces, curved shoulders and beveled edges. The handle is tapered forged steel. First- and third-person views share the mesh. Enabled an actual depth attachment on the first-person render target (including after resizing); the previous default target had none, allowing rear surfaces to paint over front surfaces. Swing timing and hit logic were not changed.

Validated the final release build with all five required checks: fmt, default tests/Clippy and no-default-feature tests/Clippy. Persistent successful report: `.be2-work/check-20260923T161817724162Z/report.json`. Generated 12 house captures, 4 wrench captures and 8 character captures; inspected the held wrench, impact pose, third-person portrait and actual menu. Wrench capture recorded one hit and one audio-play event; audio audibility was not manually retested. Updated `bin/BE2.exe` to the verified release binary, preserving the preceding binary at `.be2-work/BE2-before-wrench.exe`. Windows tested only; hardware input unchanged and not retested.

## Design accessories — 2026-09-23

Added five reusable native props: framed sunset and botanical prints, terracotta oval sculpture, leafy ceramic vase, and ceramic catchall bowl. Six instances decorate the default house. Standalone scenes are in assets/props; catalogue discovery, patch schema and native placement support all nine prop kinds.

Windows validation: cargo fmt --check, cargo test --locked, cargo clippy --all-targets --locked -- -D warnings, plus test and Clippy with --no-default-features passed. Persistent report: .be2-work/check-20260923T163512286192Z/report.json. Accessory geometry is tested against its collision/inspection bounds. Existing house traversal tests passed. All four Python authoring integration tests passed, including instantiation/audit of every asset and recipe and upstairs/garden routes.

Inspected final living room, kitchen, bedroom and pause menu captures. Corrected initial art placement in a doorway and over a window. Final house capture: 35,100 triangles, 41,904 vertices, 12 batches at 1024x697; this is a capture smoke check, not a performance benchmark. Updated bin/BE2.exe, bin/be2-tools.exe and bin/be2-headless.exe; previous binaries retained in .be2-work/before-accessory-binaries. Existing unrelated working-tree edits preserved. No multiplayer or cross-platform validation claimed.

## Decor library expansion — 2026-09-23

Added eight library-only assets: table lamp, stacked books, candle trio, potted cactus, daisy vase, tall vase, mantel clock and woven-style basket. Default house placements are unchanged. Seventeen total catalogue entries are available through the native CLI and author.py. Standalone scenes and dimensions are documented in assets/props/DECOR_LIBRARY.md. Props remain static; lights/candles are unlit and clock hands do not animate.

All five required Rust checks passed (.be2-work/check-20260923T163924186995Z/report.json), including conservative bounds and matte mesh budget tests. An initial unrelated output replacement test returned Windows Access Denied; a complete rerun passed. All four packaged authoring tests passed, including placement/audit of all seventeen assets and routes. Inspected rendered previews of all eight additions and the actual pause-menu capture. Updated client, tools and headless binaries; backups are in .be2-work/before-library-binaries. Windows only; existing working-tree edits retained.


## Lived-in map revision

School, office and market furnishing upgraded without Rust changes. Three native audits passed with zero duplicate collision boxes; eleven routes passed. Three negative exit routes were blocked as intended and native rays hit closed-door geometry. All 23 interior prefab bounds and rotated native placements passed, with occupied-ID and overwrite failure preservation checks. Inspected client frames, pause menu and detail renders; 600-tick headless smokes passed. Reports: assets/maps/starters/lived-in-validation.json. Previous map files preserved in .be2-work/lived-in/before-publish.

House shortcut snapshot: nine added household props; native audit, upstairs/garden routes, 600-tick headless smoke and inspected living/kitchen captures passed. All four authoring integration tests also passed. Desktop links verified for House, School Wing, Office and Convenience Store.

## Distinct furnished maps, September 23

four clean native audits, sixteen successful controller routes, three deliberately blocked exterior routes; 41 successful rotated template placement audits; all 18 new template bounds checked; shelf/basket panels checked for non-overlapping volumes. All four maps passed 600-tick headless smoke checks. Targeted interior renders reviewed, plus native market client captures. The native basket now uses five abutting panels with a regression test; full tools/be2.py check and build all passed. Still images and geometry checks do not establish every possible live-camera view or exhaustive traversal.

Previous maps/library/binaries: .be2-work/distinct-maps/before-publish. Reports, captures and scratch outputs: .be2-work/distinct-maps/revision-4. Map audits/routes: redesign-validation.json.

## Context and simulation contract documentation � 2026-09-23

Expanded AGENTS.md and added its CLAUDE.md import, two retrospective ADRs,
a glossary and an eight-option context review. Documented the public simulation
API and added two lifecycle integration tests plus one runnable rustdoc example.
Library rustdoc with warnings denied now runs in both configurations in the
local check runner and CI. No runtime behavior or packaged binaries changed.

Windows: all seven tools/be2.py check steps passed, including both test and Clippy
configurations and both rustdoc builds. Report:
.be2-work/check-20260923T184546111275Z/report.json. Both new tests and the doctest
passed in both feature configurations. Documentation links, feature-index paths,
Python runner syntax and git diff whitespace checks passed. Existing uncommitted
work was preserved. CI configuration was updated but remote CI was not run.


Bedroom decor update: the large bedroom now has a reading loveseat, low table with books/mug, rug, floor lamp and dresser with a cactus/keepsake bowl. The smaller bedroom has a bedside cabinet/lamp, coastal print, rug and laundry basket. All original components preserved. Native audit (no duplicate boxes), four routes including new house.route-4.json, collision-overlap check and 600-tick headless smoke passed. Reviewed both bedroom renders and native client bedroom/menu captures. Updated the house JSON used by the map shortcut; procedural default and binaries unchanged. Backup/review: .be2-work/bedroom-decor; persistent report: assets/maps/starters/bedroom-validation.json.


## Seeker object controls (2026-09-23)

Removed the client E activation, object cards, focus/action prompts, right-click dismissal and obsolete help/menu labels. Left-click wrench timing, contact feedback and impact audio are preserved. E/right-click selection and R replication remain future hider work. Legacy semantic actions remain library/map metadata; the client does not dispatch them.

`python tools/be2.py check` passed all seven checks (format, library rustdoc in both configurations, tests and strict all-target Clippy in both configurations); report: `.be2-work/check-20260923T193007098764Z/report.json`. The default suite passed 88 tests including rustdoc. Final release build, formatting and diff checks passed after correcting the pause-menu label. Inspected final live house menu and studio wrench captures; scripted contact reported one hit and one audio playback with no card. Live W and Up movement, Enter start and E/no UI were checked. No multiplayer/hider behavior is implemented or claimed. Updated bin/BE2.exe and bin/BE2-decor.exe, preserving map arguments; desktop host verification confirms all four BE2 map shortcuts resolve to BE2-decor.exe.


## Feta and Scientist (2026-09-23)

Added profile-aware body/eye height, cylinder radius, stance and speed; Feta uses human sprint speed for normal movement. Regression tests verify equal travel distance and low-obstacle traversal versus human collision. Full seven-stage `python tools/be2.py check` passed after final code changes: `.be2-work/check-20260923T201405611178Z/report.json`. Release client build and git diff whitespace check passed. Existing unrelated working-tree changes remain intact.

Inspected both procedural character portraits and final Feta menu (`.be2-work/feta-final`); Scientist portrait is in `.be2-work/scientist-qa`. Live Windows checks exercised Feta launch selection, W and Up movement, Escape pause and Enter resume. Installed BE2-decor launcher displayed selection and Scientist choice entered first-person play with the white sleeve. Both bin/BE2.exe and bin/BE2-decor.exe were replaced by the final release binary. No multiplayer, disguise behavior or other-platform validation is claimed.


## E pickup/drop and loose-prop physics (2026-09-23)

Added Rapier 0.26.1 compound rigid bodies and reusable dynamic mesh transforms. Six physics regressions cover falling/settling and absent static ghosts, momentum transfer/toppling, pickup/carry/drop for both characters, occluded selection, carrying against a thin wall, frame-rate equivalence/pause, and built-in/all four shipped-map extraction. Final full seven-stage checks passed: `.be2-work/check-20260923T203812286997Z/report.json`. Metadata confirms no dependency declares an MSRV above Rust 1.87 after locking ordered-float 5.0.0; the compiler used locally was the installed toolchain, not a separate Rust 1.87 test.

Rendered Scientist and Feta physics sequences in `.be2-work/physics-scientist` and `.be2-work/physics-feta`; inspected held and settled frames. Feta's occluded cereal ray correctly selected the table instead, lifting it and displacing its contents. Final installed release smoke is `.be2-work/physics-final`. Both launch binaries match SHA256 5858D03672F8C29387FEE147793C1B61583ACB2EB78FAE567A2322DA440D0851.

Used a local two-level test fixture for live Windows controls: Scientist E pickup hides the wrench, E drop restores it; Feta third-person E pickup and drop both work. Exercised S and Down carrying movement, Escape pause and Enter resume, and inspected each pause menu. Closed a duplicate hidden test process after it interfered with Windows key polling; the single visible instance then passed. Existing unrelated working changes are preserved. Physics positions are session-local, custom non-catalog geometry stays fixed, and no network ownership or other-platform execution is claimed.

## Four-map expansion (2026-09-23)

All four launcher JSON maps expanded to approximately 2x gross floor/yard area. Added 269 runtime-recognized loose physics props. Four native audits, 46 controller routes (18 retained, 25 extension, three reopened entrances), and 1,200-tick headless loads per map passed. The rendering-independent validation example passed settling plus pickup/drop as both characters on all maps. Full seven-stage tools/be2.py check passed; log .be2-work/check-20260923T205924113475Z/report.json. Targeted offline renders and native menu captures reviewed. No gameplay engine or shipped binary changes in this pass. Map hashes and results: assets/maps/starters/expansion-validation.json. Original maps and detailed captures/logs retained under .be2-work/map-expansion.

## Feta furniture clearance (2026-09-23)

Procedural and JSON room loading now refines table/desk/chair/bench collision envelopes into visible-part bounds. Tests cover Feta standing passage, Scientist exclusion, quarter-turn furniture, fixed/dynamic cases, solid legs and undersides, all four shipped map desks, and moved furniture without ghost proxies. Full seven-stage check passed: .be2-work/check-20260923T211414339288Z/report.json. Client/tools/headless release builds installed in bin, including BE2.exe and BE2-decor.exe; hashes/backups: .be2-work/feta-clearance. Four audits and all 46 shipped routes passed with packaged tools. School/Feta client capture and menu reviewed. Movement mappings and character dimensions are unchanged; no manual keyboard-input retest in this collision-only pass.
