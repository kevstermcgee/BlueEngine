# BE2 architecture

BE2 is a local fork of Blue Engine's 3858ca0 history plus its latest working-tree wrench, character and camera additions, copied on 2026-09-23. The original folder is unchanged. The inherited library name `vesper3d` and MIT license are preserved.

## Simulation and presentation

`viewer/simulation.rs` owns the rendering-free 60 Hz movement path. `PlayerStepper` accumulates real frame time, preserves a jump edge until a tick consumes it, and caps catch-up at eight ticks. Excess time during stalls is deliberately discarded. Rendering interpolates the two most recent poses; mouse look uses the current angles. Pause/reset clears interpolation debt. This adds at most one simulation tick of positional latency, avoiding visible 60 Hz steps on faster displays.

`HeadlessWorld` provides match-local room/collision data, join/leave, validated movement intent, fixed ticks and read-only player state, with an eight-player ceiling. Movement and collision code is shared with the client. This is a simulation foundation, not a multiplayer game: players currently share a spawn, do not collide with each other, and server-side combat/interactions are not yet implemented.

## Builds

- `client`: optional Macroquad graphics and native Windows window/input support.
- `offline`: optional inherited PNG/movie renderer and Ctrl+C handler.
- Default: both, retaining the existing tools.
- `--no-default-features --bin be2-headless`: no window, graphics backend, PNG renderer, FFmpeg or socket dependency. Only Serde/JSON and their transitive dependencies remain.

The headless executable is a finite local benchmark with optional real-time pacing. It does not listen on a port or claim to be an online game server.

## Graphics and cost

Static lighting uses four samples per emitter for softer shadows. Identical shading samples are cached per primitive, then exact vertex entries are shared within batches; normals, colours and material tags remain distinct. Roughness controls specular width, with a restrained metal edge reflection. The 4x MSAA setting is retained. No expensive per-frame shadow pass is added.

Wrench geometry and character buffers reuse capacity each frame. The first-person wrench is smaller and lower to expose more of the scene. Branding panels and hit-label text are removed from gameplay; help defaults off, H toggles it, F3 remains opt-in. Crosshair, nearby action prompts and explicitly requested information cards remain.

## Props

`viewer/props.rs` contains four typed definitions, stable IDs, bounds, room placements and `scene(PropKind)`. `assets/props` contains their standalone JSON scenes. Coordinates are metres, Y-up, origin at the floor. They use existing primitive/material types, so there is no scene-schema change. Crate, barrel, stool and toolbox have conservative collision boxes and work with inspection/wrench ray hits. Disguise selection and prop possession are not implemented.

## PulseNet next phase

The prior design uses Quinn transport, reliable control streams, input/snapshot datagrams, bounded queues, a 60 Hz authoritative simulation and 20 Hz snapshots. Its source archive was referenced in the earlier conversation but was not available in this workspace; this change does not invent or vendor a substitute.

The adapter should map authenticated PlayerId to HeadlessWorld IDs, validate/decode bounded input, reject duplicate/out-of-order sequences, expire stale movement on disconnect/timeout, run `step()` only on a simulation worker, and encode player-visible snapshots within PulseNet's payload limit. Add authoritative wrench/prop/round rules before enabling public matches. Client prediction, snapshot interpolation/reconciliation and a two-client integration test follow. Keep offline play available without a connection.
