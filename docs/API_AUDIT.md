# Prototype API audit

Before this pass, a Rust prototype needed imports across scene, geometry, controller,
room, interaction and simulation; material and node construction; matching collider
and entity records; compilation; physics extraction; and separate input/step calls.
A visual client additionally initializes Macroquad, bakes static meshes, creates
per-prop render buffers, polls devices, accumulates fixed ticks and draws poses.

SceneBuilder now batches AddBox/AddProp edits and validates once at build. A static
box gets geometry, collision and a semantic entity together. A catalog prop gets its
existing reusable geometry and physics extraction contract. `build` returns the
ordinary serializable MapDocument; `world` additionally initializes headless physics
and propagates errors. Client/headless `--map` use that same document. The compact
example is headless input-driven movement; it is not a window/device-input wrapper.

The most common borrowing friction was finding an index in `world.prop_physics`
while needing mutable access to that physics/world. `impulse(id, vector)` performs
that lookup internally; `prop_position(id)` returns a copied vector. IDs are stable
semantic strings, never retained references or exposed Rapier handles. Existing
low-level APIs remain available. Static/missing IDs fail explicitly. These lookups
are linear and appropriate for prototype-sized scenes; benchmark before indexing.

No unsafe code, lifetime parameters, reference-counted interior mutability or new
traits are needed for this layer. A full event loop builder, arbitrary dynamic
meshes, runtime spawning/despawning, custom controller profiles and data-driven
game rules are deliberately not claimed. They need explicit ownership, validation
and replication contracts rather than a convenience wrapper over demo behavior.
