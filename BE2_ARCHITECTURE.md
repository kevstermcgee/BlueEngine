# BE2 architecture

BE2 is a local fork of Blue Engine's 3858ca0 history plus its latest working-tree wrench, character and camera additions, copied on 2026-09-23. The original folder is unchanged. The inherited library name `vesper3d` and MIT license are preserved.

## Simulation and presentation

`viewer/simulation.rs` owns the rendering-free 60 Hz movement path. `PlayerStepper` accumulates real frame time, preserves a jump edge until a tick consumes it, and caps catch-up at eight ticks. Excess time during stalls is deliberately discarded. Rendering interpolates the two most recent poses; mouse look uses the current angles. Pause/reset clears interpolation debt. This adds at most one simulation tick of positional latency, avoiding visible 60 Hz steps on faster displays.

`HeadlessWorld` provides match-local room/collision data, join/leave, validated movement intent, fixed ticks and read-only player state, with an eight-player ceiling. Movement and collision code is shared with the client. This is a simulation foundation, not a multiplayer game: players currently share a spawn, do not collide with each other, and server-side combat/interactions are not yet implemented.

## Builds

- `client`: optional Macroquad graphics and native Windows window/input support.
- `offline`: optional inherited PNG/movie renderer and Ctrl+C handler.
- Default: both, retaining the existing tools.
- `--no-default-features --bin be2-headless`: no window, graphics backend, PNG renderer, FFmpeg or socket dependency. Serde/JSON and the shared rendering-free Rapier physics module remain.

The headless executable is a finite local benchmark with optional real-time pacing. It does not listen on a port or claim to be an online game server.

## Graphics and cost

Static lighting uses four samples per emitter for softer shadows. Identical shading samples are cached per primitive, then exact vertex entries are shared within batches; normals, colours and material tags remain distinct. Roughness controls specular width, with a restrained metal edge reflection. The 4x MSAA setting is retained. No expensive per-frame shadow pass is added.

Wrench geometry and character buffers reuse capacity each frame. The first-person wrench is smaller and lower to expose more of the scene. Branding panels and hit-label text are removed from gameplay; help defaults off, H toggles it, F3 remains opt-in. The crosshair, contextual pickup/drop hint and wrench hit feedback remain. The client no longer dispatches legacy inspection actions or draws object information cards/prompts. E now carries/drops loose props for both characters. Right-click selection and R replication remain future hider work.

The held wrench uses a continuous beveled open-end profile with a 15-degree head angle and tapered steel shank, shared by first- and third-person rendering. Its private transparent render target explicitly includes a depth attachment at creation and resize so hidden tool/hand surfaces cannot overdraw the visible faces. Geometry is constructed only at startup; poses reuse existing buffers.

## Props

`viewer/props.rs` contains seventeen typed definitions, stable IDs, bounds, room placements and `scene(PropKind)`. `assets/props` contains their standalone JSON scenes. Coordinates are metres, Y-up, origin at the floor. They use existing primitive/material types, so there is no scene-schema change. Cereal box, chair, table and apple have conservative collision boxes and retain semantic metadata and work with wrench ray hits. Disguise selection and prop possession are not implemented.

## PulseNet next phase

The prior design uses Quinn transport, reliable control streams, input/snapshot datagrams, bounded queues, a 60 Hz authoritative simulation and 20 Hz snapshots. Its source archive was referenced in the earlier conversation but was not available in this workspace; this change does not invent or vendor a substitute.

The adapter should map authenticated PlayerId to HeadlessWorld IDs, validate/decode bounded input, reject duplicate/out-of-order sequences, expire stale movement on disconnect/timeout, run `step()` only on a simulation worker, and encode player-visible snapshots within PulseNet's payload limit. Add authoritative wrench/prop/round rules before enabling public matches. Client prediction, snapshot interpolation/reconciliation and a two-client integration test follow. Keep offline play available without a connection.

Simple-prop revision: tag 3 selects one quad per box face and a 12-by-6 apple sphere. Static directional face colours replace detailed shadow bakes on these props; their shader skips specular highlights. The cereal band is a solid section of the carton, not an overlaid label. New objects have new IDs; earlier JSON props remain under assets/legacy-props with their original IDs. Crystal recolouring is restricted to tag 2, so it cannot tint simple props.

## House and contact audio

MapId selects House by default for both client and HeadlessWorld, with Studio preserved for regression work. House instances use the matte low-poly tag throughout. The controller can step onto obstacles up to 22 cm while grounded if standing clearance is available; the staircase uses 10 cm risers. Automated routes cover each upstairs room and the backyard.

The optional client feature includes Macroquad audio. ImpactAudio embeds the original short WAV and plays once when the wrench's confirmed hit count changes. No sound is played for a miss; playback is quiet and non-looping. Audio initialization failure leaves the client usable without sound. No audio types or dependencies enter the headless build. Linux client builds require ALSA development libraries; CI installs them.

## Agent authoring boundary

viewer/authoring.rs owns the strict MapDocument v1 contract, static validation, runtime construction and transactional edits. It depends only on the existing scene/simulation modules and Serde. be2-tools supplies JSON CLI inspection, diagnostics, patches and exports with exclusive output creation. Client --map and headless --map share the same loader. The ordinary built-in house remains the default. Entity IDs, labels and room names now use owned String values to support repeated map loading without leaked allocations.

tools/be2.py orchestrates tools, checks, isolated release builds, bounded captures and packages through argument arrays without shell evaluation. tools/FEATURES.json and tools/README.md are the discovery entry points for agents. New gameplay subsystems remain Rust code changes; authoring patches do not rewrite code or dependencies.

Landscaping uses reusable procedural builders in viewer/landscaping.rs: tapered branching broadleaf trees, layered conifer canopies, irregular multi-tone shrub clusters, and low-cost petal/stem/leaf flower shapes. These remain static supported primitives, so map export and headless loading continue to work. Decorative flowers have no collision; shrubs preserve the established core bounds and trees use simple trunk bounds.

The shared client starts with a 90-degree world field of view for all maps and both perspectives. The pause-menu slider remains adjustable; the separate held-tool lens is unchanged.

## Character profiles

`CharacterKind` in controller.rs defines Scientist and Feta. `Controller::for_character` creates a grounded spawn with the correct body and eye height; horizontal, ceiling and ground collisions use the profile radius. Fixed stepping and presentation interpolation remain shared. Feta is 0.30 m tall (0.20 crouched), radius 0.16 m, eye offset 0.08 m below the body top, and moves at the human sprint speed without Shift. The client gates gameplay behind a per-launch character choice, suppresses Feta's wrench input/rendering, and starts him in third person with a shorter camera boom. Reset preserves the selected profile. The headless default remains Scientist; there is no network role protocol. Procedural meshes live in bin/character/mod.rs; Scientist sleeves also share the white coat palette with wrench_view.

## Loose-prop physics

`viewer/prop_physics.rs` owns a rendering-independent Rapier 0.26.1 world. The client initializes it once from the room. Freestanding semantic bounds containing catalog-material primitives (`prop-` / `decor-`) become compound dynamic bodies; small bounds claim nodes before larger furniture, and thin wall art / large architecture stay fixed. Existing v1 map files require no rewrite. Materials outside this convention stay static. Runtime extraction keeps IDs and the original compiled document; the static BVH and baked GPU mesh omit claimed nodes. Rendered primitives are cached per body and transformed in `bin/prop_view/mod.rs`, without whole-map mesh rebuilding.

Gravity, contacts, angular motion, friction, modest restitution, sleeping and CCD run at 120 Hz, capped at 16 ticks per frame. Each primitive contributes a convex collision proxy; sphere/cylinder/cone proxies are sampled convex hulls, and boxes retain their oriented shape. Static visible geometry supplies collision surfaces; the movement world ground at y=0 is retained. `Room::hit` chooses the closest static/dynamic geometry for focus, camera and wrench hits. Dynamic BVHs and semantic/player-collision bounds track poses, eliminating old-position ghost hits. Held props are excluded from player/camera queries but remain rigid bodies against scenery and other props.

E uses the closest visible surface within 2 m. Carrying drives a capped velocity servo toward a point in front of the player; it does not teleport the object. Releasing restores gravity and bounded carry velocity. Pause stops time, reset drops first, and wrench input/rendering is suppressed while carrying. Physics state is session-local. The standalone `HeadlessWorld` movement benchmark is unchanged; the reusable `PropPhysics` module is tested without graphics, but there is no authoritative multi-player ownership protocol.


## Furniture clearance

Room construction refines legacy table/desk/chair/bench/workbench collision envelopes into contained visible component AABBs. Furniture is identified by the final word of its semantic label (space or hyphen separated); unrelated architecture and single solid plinths retain their authored proxies. Entity bounds and IDs are unchanged. This applies to procedural and JSON rooms and both character profiles; Feta retains his normal size and movement speed. Real legs, stretchers, seats and tops remain solid, including ceilings for jumping.

PropPhysics removes both whole-object and component proxies when extracting movable furniture, then supplies its transformed compound-part bounds each step. Moving or dropping a table therefore does not leave fixed invisible legs behind. Quarter-turn furniture, fixed desks, native chairs/tables, Scientist exclusion, solid legs/undersides and desk passages in all four shipped maps are covered by tests/furniture_clearance.rs and the physics regression suite. Arbitrary-angle component AABBs remain conservative.
