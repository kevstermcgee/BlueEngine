# Vesper3D

**Describe a world. Give it motion. Render a film.**

A native Rust animation engine designed for AI authors. A small declarative JSON scene becomes a deterministic 3D animation and a normal H.264 MP4. No browser, Python runtime, graphics driver, Blender installation, cloud service, or generated source code is needed to render.

Vesper3D is a new, independent engine and local Git repository. It shares Flick's AI-first objectives, but uses its own 3D renderer, scene contract, and codebase.

## Start here

The delivery includes a Windows x64 executable at `bin/vesper3d.exe`. FFmpeg with `libx264` on PATH is needed for MP4; still frames and contact sheets work without it.

```powershell
.\bin\vesper3d.exe doctor
.\bin\vesper3d.exe validate examples\hello.json
.\bin\vesper3d.exe frame examples\hello.json renders\hello.png --time 1 --quality high
.\bin\vesper3d.exe render examples\hello.json renders\hello.mp4
```

For an AI, provide **[AI_REFERENCE.md](AI_REFERENCE.md)**. It is the complete compact authoring contract. The executable also prints it with `vesper3d reference`.

This is a complete scene for a waving robot:

```json
{"nodes":[{"id":"friend","shape":"robot","motion":{"wave":1}}]}
```

A small scene with a reflective floor, animated sphere, and reusable material:

```json
{
  "duration": 4,
  "materials": {
    "gold": {"color": [0.9, 0.5, 0.12], "metallic": 0.8, "roughness": 0.2},
    "floor": {"color": [0.04, 0.08, 0.13], "metallic": 0.4}
  },
  "nodes": [
    {"id": "floor", "shape": "box", "material": "floor", "pos": [0,-0.2,0], "scale": [12,0.2,12]},
    {"id": "ball", "shape": "sphere", "material": "gold", "pos": [[0,[-2,1,0]],[2,[0,2,0]],[4,[2,1,0]]]}
  ]
}
```

## What is implemented

| Area | Available now |
|---|---|
| Geometry | Analytic spheres, boxes, capped cylinders and cones; smooth torus meshes; faceted crystals; OBJ polygon meshes |
| Reusable models | Articulated robot, layered tree, rocket; named materials; parented groups |
| Compact populations | Deterministic repeats in rows, rings, or seeded scattered volumes |
| Animation | Vector keyframes, four easing modes, inherited transforms, visibility intervals, spin, orbit, bob, robot walking and waving |
| Cinematography | Animated camera position/target, orbit, vertical field of view, sampled depth of field |
| Rendering | CPU ray tracing, BVH acceleration, multithreaded rows, GGX direct-light highlights, metal response, sampled soft shadows, ambient occlusion, recursive reflections |
| Finishing | Atmospheric fog, emissive glow, bloom, ACES-style tone mapping, sRGB output, antialiasing |
| Deliverables | PNG stills, six-frame contact sheets, H.264 MP4, optional AAC audio |
| Automation | JSON status/errors, CLI and Rust library, headless execution, absolute-time frames, CPU-worker limit |
| Reliability | Strict input validation, bounded scene complexity, locked dependencies, cancellation checks, temporary outputs, success-only commits |

The renderer is physically inspired, with explicit artistic approximations. Ambient light and occlusion approximate indirect illumination. Reflections are sharp and attenuated by roughness; they are not stochastic rough-surface transport. Emissive objects bloom but require a separate light to illuminate nearby objects. Source radii sample a box-shaped area. This keeps output reproducible and avoids the noisy reflections of a low-sample path tracer.

## Examples

| File | Purpose |
|---|---|
| `examples/hello.json` | Tiny robot greeting; quickest starting point |
| `examples/first-light.json` | 12-second observatory showcase: animated robot, mechanical planet, orbiting lights, camera move, original chime score |
| `examples/materials.json` | Five primitives and material responses with an orbiting camera |
| `examples/mesh.json` | Local OBJ import |
| `examples/grove.json` | One declaration creates a ring of trees; another scatters glowing particles |

The included `first-light.wav` is an original synthesized chime score. No third-party media or downloaded models are used in the showcase.

```powershell
.\bin\vesper3d.exe contact examples\first-light.json renders\board.png --quality draft
.\bin\vesper3d.exe render examples\first-light.json renders\first-light.mp4 --quality standard
.\bin\vesper3d.exe frame examples\first-light.json renders\poster.png --time 5 --quality ultra
```

## Performance and quality

Use `draft` for blocking and `standard` for animation; use `high` or `ultra` for polished stills and final shots where more render time is acceptable. `--width 640` reduces resolution while preserving aspect ratio. `--threads 4` limits renderer workers; FFmpeg uses two encoding workers separately.

| Preset | Camera samples/pixel | Shadow samples/light | AO samples | Reflection bounces |
|---|---:|---:|---:|---:|
| draft | 1 | 1 | 0 | 1 |
| standard | 4 | 1 | 1 | 1 |
| high | 9 | 2 | 2 | 2 |
| ultra | 25 | 4 | 4 | 2 |

This is an **offline film renderer**, not a real-time viewport. BVH traversal reduces intersection work; analytic primitives avoid unnecessary tessellation; each frame is streamed to the encoder instead of accumulating a frame directory. Pixel seeds do not depend on thread scheduling or frame order. See [VALIDATION.md](VALIDATION.md) for measured timings and tested limits.

Memory scales with a single frame and scene geometry, not film duration. RGB and three floating-point image buffers use approximately 39 bytes/pixel before scene data and encoder overhead: about 36 MB at 720p and 323 MB near the maximum pixel count. Geometry is compiled once; world transforms and the BVH are rebuilt per frame. There is no GPU requirement or network access during rendering.

## Build and test

Rust 1.87 or newer and its native platform linker are required. Dependencies are pinned and `Cargo.lock` is committed. The first build needs the Rust package registry; subsequent cached builds can use `--offline`.

```text
cargo build --release --locked
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --check
```

Windows build output: `target/release/vesper3d.exe`. On Linux/macOS: `target/release/vesper3d`. The code uses portable Rust and process APIs; this delivery was tested on Windows. CI is included for Windows and Linux. Linux/macOS support is not claimed as locally verified.

## Failure handling

Outputs are refused if they already exist unless `--overwrite` is explicit. A render writes to a unique sibling temporary file, waits for successful encoding, and only then commits it. Without overwrite, an atomic hard-link operation also protects against a destination created during rendering. Filesystems without hard-link support report an error rather than weakening that protection. With overwrite, the temporary file is renamed into place.

Ctrl+C requests cancellation at a row/frame boundary. Encoder failure, broken pipes, and ordinary cancellation remove the temporary output and preserve an existing completed video. An OS kill or power loss can leave a `.partial.*` file; it does not turn that partial file into the requested final output. Audio/mesh references are restricted to the scene directory. The engine does not evaluate scripts or invoke a shell.

## Scope of version 0.1

This is a working, tested engine for procedural 3D films and explainers. It is not a replacement for a mature general-purpose DCC package. There is currently no graphical editor, GPU backend, texture mapping, skeletal asset import, glTF/FBX, IK, collision simulation, fluid/cloth solver, text layout, transparency/refraction, motion blur, or full global illumination. OBJ import uses flat normals and one material per node; convex polygon faces are fan-triangulated. Robots use built-in joint animation.

The small modules and documented scene contract make those extensions possible without changing the authoring workflow. See [ARCHITECTURE.md](ARCHITECTURE.md).

## References

Scene serialization uses [Serde JSON](https://docs.rs/serde_json/1.0.140/serde_json/); PNG export uses [png](https://docs.rs/png/0.17.16/png/); video export uses [FFmpeg rawvideo and MP4 formats](https://ffmpeg.org/ffmpeg-formats.html). The ray tracer, transforms, geometry, animation, materials, camera, and image finishing are implemented in this repository.

MIT licensed. Third-party Rust packages retain their respective licenses. FFmpeg is an external dependency and is not redistributed in this package.
