# ADR 0036: Shadows for kit games: contact blobs and one shadow map, behind one setting

Status: Accepted

## Context

Games built on `kit::Look`/`Materials` felt flat: hemisphere ambient, one Lambert key light, rim and fog, but
nothing ever shadowed anything. The player-facing ask was "shadows as an option", and the engine-facing
constraint was "logical, reliable, elegant, not heavy". The renderer is macroquad 0.4.14 on miniquad 0.4.8
(OpenGL, GLSL 100, no web target), and a read of those sources fixed what is and is not possible:

- **Polygon offset is dead.** `PipelineParams::depth_write_offset` exists but nothing reads it. Bias has to be done
  in the shader.
- **A depth test needs depth writes.** `apply_pipeline` calls `glDisable(GL_DEPTH_TEST)` when `depth_write` is
  false, so `fx_alpha` and `fx_add` are never depth-tested (a shadow blob drawn with them shows through walls and
  over the car that casts it). A depth-tested translucent material must write depth.
- **There is no depth texture.** `render_target_ex` always makes an RGBA8 colour texture; its depth attachment
  is internal and not readable. A shadow map must be colour, with the depth packed into the bytes.
- **`sample_count: 1` (the default) blits after every draw call.** The resolve copy of the whole target runs per
  draw. `sample_count: 0` has no resolve and no blit; `depth: true` still gives hidden-surface removal.
- **Render targets default to `Linear` filtering,** which blends the bytes of a packed value into garbage. The map
  must be `Nearest`.
- **Samplers**: `MaterialParams::textures` declares extra samplers (units start after `Texture` and
  `_ScreenTexture`); an unset one binds a 1x1 white texture. GLSL 100 `sampler2D` defaults to `lowp`: declare it
  `highp`. There is no `dFdx` without an extension, so slope-scaled bias is not available.
- Fragment uniforms were already at the GLES2 minimum of 16 vectors. A `mat4` and a `vec4` exceed it; desktop
  GL (the only target) is fine.
- There are no retained GPU meshes: every pass re-uploads its geometry, so a shadow pass roughly doubles the
  geometry upload. Headless software GL runs at about 16 frames per second.

## Decision

One setting, `devkit::ShadowQuality { Off, Simple, Full }`, default `Simple`, stored in `Settings.shadow_quality`
with a serde default (old files load; an unknown word reads as the default instead of discarding the file),
cycled in Esc > Settings (`GameShell::local_menu_with_options`, only for games that call it) and set per run with
`--shadows off|simple|full`. One helper, `kit::Shadows`, owns the rest; at `Off` every call is a no-op.

**Simple: contact blobs, no extra pass.** `Materials::decal` is alpha-blended, `depth_test: LessOrEqual`,
`depth_write: true`, and its vertex shader subtracts `DECAL_DEPTH_BIAS * w` from clip-space z. The bias is
8 steps of a 24-bit depth buffer in NDC, so in world units it is a fixed multiple (about 8x) of
`lint::depth_resolution` at every distance: the decal wins against a coplanar road at 5 m and at 700 m (near 0.3,
far 700) and is still only millimetres at close range. `Batch::blob(center, radius, strength)` builds a soft disc
from the existing template primitives (lint-clean). `Shadows::blob(pos, radius)` asks a ground-height closure
(default flat at 0) where the ground is and fades and widens the blob with height. Draw order: static world,
decals, dynamic actors. Because decals are depth-tested, walls hide blobs and actors cover their own.

**Full: one directional shadow map.** `ShadowMap::new(resolution)` is an RGBA8 target (`sample_count: 0`,
`depth: true`, `Nearest`). The `caster` material writes the light-space depth packed into three bytes
(`fract(d * (1, 255, 65025))` minus the carry; about 24 bits). `ShadowMap::camera(&look, focus, half_extent, depth)`
fits an orthographic box along `-Look::key_direction` around the focus, **snapped to whole texels in light space**
so a moving focus never shimmers. The world fragment shader samples the map through a declared `ShadowMap` sampler
and a `LightVP` mat4, uses **normal-offset bias** (the receiver is pushed along its normal by 1.5 texels) plus a
1-texel constant depth bias, filters 3x3 with constant loop bounds, fades the shadow out over the outer 15% of the
box, and applies the result to the key light's term only (the four point lights stay unshadowed). A zero `Shadow`
strength (the state before any `set_shadow`) skips the lookup; an unset sampler is white, which unpacks to "farther
than anything", so the safe default is no shadow. Games draw the same batches into the pass
(`Shadows::cast(|| ...)`), so there is no second copy of scene code. Casters are whatever the game draws there:
fx, glass and a first-person viewmodel are left out by not drawing them.

Pure Rust mirrors of the GLSL (`pack_depth`, `unpack_depth`, `fit_light_box`, `shadow_factor`) carry the unit tests:
round-trip error, monotonic packing, box centring and light direction, texel-snapping stability, bias sign and
slope acne, behind-the-far-plane and outside-the-box are lit, and a drift test keeps `WORLD_UNIFORMS` and the shader
in step (a `Mat4` arm; the sampler is a texture, not a uniform, and is checked separately for `highp`).

## Consequences

- Games that never call `Shadows` render as before (the shadow uniforms start at zero).
- `Materials` gains a public `decal` field; `MenuOutcome` gains `cycle_shadows`; `Settings` gains `shadow_quality`
  (struct-literal constructors of these need `..Default::default()`; none exist in the engine's games).
- `Full` costs a second pass over the casters and a 2048x2048 RGBA8 target (16 MB plus its depth). Numbers per tier
  are in `docs/perf/README.md`; real GPUs and Windows were not available to measure.
- Not solved, on purpose: the key light is the only shadowed light; there are no cascades (one box follows the
  focus, so a large open world sees crisp shadows near the player and none beyond the box); thin casters under 2
  texels can vanish; blobs are flat discs (steep slopes need Full); translucent casters cast as if opaque or not at all.

## Rejected or deferred

- Polygon offset or a depth texture: not available in miniquad 0.4.8; patching it for one feature was not worth a fork.
- Cascaded shadow maps, variance/PCSS soft shadows: heavier than "simple and reliable", and the box-follows-focus
  design covers the games that exist.
- Raising `fx_alpha` to be depth-tested: it would change every existing game's look (glows would vanish behind walls).
  `decal` is the opt-in path.
- Shadowing the point lights: four more passes for a lighting term most games use for small effects.
