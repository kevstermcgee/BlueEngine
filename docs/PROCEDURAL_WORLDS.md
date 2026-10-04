# Streaming procedural worlds

For an endless custom simulation, use `viewer::devkit::procedural`. These helpers are
graphics-free and work in headless tests. The engine supplies identity, coordinates,
bounded streaming, seamless noise and a repeating clock; the game chooses plants,
terrain, collision and presentation. Runnable example: `assets/games/leo`.

```rust
use vesper3d::viewer::devkit::procedural::{ChunkCache, ChunkId, WorldPoint, DayCycle};
let size = 32.;
let mut point = WorldPoint::new(ChunkId::default(), [-0.1, 16.], size)?;
let mut chunks = ChunkCache::new(3)?; // square radius: at most 49 chunks
chunks.update(point.chunk, |id| Ok::<_, String>(id.rng(7, 42)))?;
let local = point.relative(point.chunk, size)?;
let noon = DayCycle::new(43_200)?.at(21_600);
# Ok::<(), String>(())
```

`ChunkId` uses signed 64-bit integer coordinates. Derive each chunk's random stream
from `id.rng(world_seed, salt)`, independently of visiting order. Never advance one
global RNG as chunks arrive: revisiting or loading would then change the landscape.
`ChunkCache<T>::update` generates missing chunks before committing changes, retains
only its square radius, and returns added/removed IDs. A failed generator or coordinate
overflow keeps the previous cache. Stationary updates do not generate or allocate.
Radius is 0..8 (maximum 289 chunks); gameplay must choose a radius covering movement
and collision queries. Build render caches on changes, rather than rebuilding plants
every frame. Invalidate derived caches when a save replaces the world seed.

Keep a `WorldPoint { chunk, local }`, with local metres in `[0, size)`, and a nearby
integer render/collision origin. Normalize bounded movement with `WorldPoint::new`;
on crossing a chunk boundary, subtract the same small origin shift from previous and
current render poses. `relative(origin, size)` converts only nearby integer differences
(maximum 64 chunks per axis) into floats. Do not cast the global coordinate to `f32`.
Chunk size is finite 1..4096 metres. Integer exhaustion returns a clear error: this
is practically endless streaming, not an assertion that finite integers are infinite.

`field(seed, point, size, scale)` is smooth value noise, continuous across chunk seams,
including negative and very distant coordinates. Scale is 1..64 chunks. It describes
distribution; it is not a heightfield physics or navigation system.

`DayCycle::new(ticks_per_day)?.at(authoritative_tick)` returns completed days and a
normalized phase: midnight 0, sunrise .25, noon .5, sunset .75. Advance and save the
tick in the shared fixed-step simulation. Derive lighting and named audio layer levels
from this state; do not run a separate wall-clock day timer in the renderer.

Save authoritative seed, chunk/local movement state and tick through `Snapshot`.
Regenerate chunks on restore. Leo demonstrates origin rebasing, tree collision,
an exact save continuation, day-count menu, imported nature loops and an authored
score. Its fields are flat with decorative plant geometry; it adds no biome editor,
terrain tessellator, asynchronous chunk worker, network protocol or world database.

Verification: `cargo test --profile itest --no-default-features --test procedural_world`,
then Leo's `cargo test --no-default-features`. Development captures use the remote
Linux CI virtual display; nothing needs to launch on a busy desktop.
