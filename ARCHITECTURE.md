# Architecture

The pipeline is `JSON -> validate -> compile geometry -> evaluate time -> build BVH -> render rows -> finish frame -> encode -> commit`.

`scene.rs` owns the versioned serde contract, resource limits, tracks, hierarchy, procedural motion and deterministic repeat offsets. Unknown fields are rejected. A track is either one vector or a list of time/vector pairs. Frame counts snap representational noise at exact boundaries before rounding up. The scene graph is validated before recursive evaluation. Scale bounds also account conservatively for the entire parent chain.

`math.rs` contains a small vector/affine transform implementation, inverse-transpose normals, rays, robust parallel-slab bounds and deterministic hashing. Scale is positive; nested nonuniform transforms can produce shear and are handled by the full affine inverse. Object-space ray directions are deliberately not normalized so intersection distance remains in world-ray units.

`geometry.rs` compiles declarative primitives, reusable models and bounded OBJ files into parts. Robot joints pivot around explicit shoulder/hip points. Torus vertices have smooth normals; imported faces and crystals use geometric normals. Every frame converts the evaluated parts into instances and builds a median-split BVH with four primitives per leaf. A fixed traversal stack avoids per-ray allocations. The expansion budget and balanced split limit tree depth well below its 64 entries. Repeats duplicate a node's geometry only; child nodes are not multiplied.

`render.rs` uses scoped worker threads with disjoint mutable image slices. Threads share immutable scene/BVH data. Samples derive from pixel and sample indices, so the RGB image is reproducible across thread counts on the same target build. Identical floating-point results across different architectures or compiler versions are not promised. The camera can be evaluated at arbitrary times. No accumulated simulation state is needed.

Shading combines direct GGX highlights, Fresnel metal response, diffuse shading, ambient approximation, sampled local occlusion and bounded sharp reflections. Shadow rays early-exit. Fog is applied along each traced segment. Bloom uses a bounded separable kernel, followed by filmic tone mapping and the sRGB transfer function. Maximum ray distance is 5000 world units. Area-light and AO sample counts are deliberately small in draft mode; shadows can show noise at low settings.

`output.rs` streams one RGB frame at a time into FFmpeg through a pipe. Arguments are passed directly to the process API. Diagnostic output goes to a temporary file to avoid stderr pipe deadlock; error reports read a bounded excerpt. RAII owns the child process and temporary files. PNG follows the same pending-output/commit pattern. Memory does not grow with film duration.

`main.rs` provides the AI-facing CLI. Successful summaries go to stdout and progress/errors to stderr as JSON. The binary embeds the AI reference. Validation includes asset resolution and mesh compilation, so broken mesh paths and invalid indices fail before expensive rendering. Audio decoding remains FFmpeg's responsibility and can fail when encoding starts.

## Extension seams

- Add a primitive by implementing bounds and local-space intersection, then a `Shape` and prefab mapping.
- Add a renderer backend behind the frame API; keep deterministic scene-time evaluation shared.
- A persistent worker pool, BVH refitting and cached world instances are the next speed improvements; the current implementation rebuilds transforms/BVH and starts row workers per frame.
- Rich mesh/material formats should compile to typed, validated assets with explicit budgets. Do not allow formats to silently fetch external resources.
- New scene semantics require a contract/version decision, reference update, negative-input tests and a rendered example.

## Reliability limits

The engine rejects malformed authoring input but is not a sandbox for hostile native dependencies. Running on untrusted files still relies on Rust dependencies and FFmpeg parsers. Renderer cancellation is cooperative. If a user-supplied FFmpeg executable hangs without reading stdin or exiting, the current CLI has no encoder watchdog. Hard termination can leave owned partial files, which may be removed manually after ensuring no render still uses them. The binary never deletes unrelated temporary files.
