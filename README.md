# Blue Engine 0.1

A native first-person room viewer in Rust, built from the Vesper3D animation engine. Explore a furnished blue-accented studio at eye level using the keyboard and mouse.

## Run

Open **bin/BlueEngine.exe**, then click **Enter the room** or press Enter. No installation, internet connection, server, Rust toolchain, or FFmpeg is needed for the viewer. The room and shaders are compiled into the executable. Windows x64 with an OpenGL-capable graphics driver is the tested platform.

| Control | Action |
|---|---|
| W / Up | Walk forward |
| S / Down | Walk backward |
| A / Left | Strafe left |
| D / Right | Strafe right |
| Mouse | Look in any direction |
| Hold Shift | Sprint (works with WASD and arrow keys) |
| Space | Small jump (about 35 cm) |
| Hold Ctrl or C | Crouch and move slowly |
| Escape / Tab | Pause or resume; release or capture the cursor |
| Enter | Enter or resume from the menu |
| F11 | Toggle fullscreen |
| H | Hide or show the on-screen hints |
| F3 | Show performance and camera coordinates |
| Q | Quit from the pause menu |

Pause to adjust mouse sensitivity, vertical field of view, invert vertical look, or reset your position. Switching to another app pauses the viewer and releases its mouse capture. Settings last for the current session. Arrow keys move relative to your view; left and right strafe, matching A and D.

Standing eye height is 1.68 metres. Space triggers a small 35 cm jump with gravity and a grounded landing; midair jump presses are ignored. Hold either Ctrl key or C to crouch smoothly to a 0.98 metre eye height and move at a slower 1.3 m/s. Release to stand when there is enough overhead clearance. Crouching also works in midair without moving your feet artificially. Normal walking is 3.2 m/s (up from 2.6 m/s). Hold either Shift key to sprint at 5.6 m/s, and release to return smoothly to walking. Crouching takes priority over sprinting. Movement is normalized, accelerates and stops smoothly, slides along walls and furniture, and is subdivided to resist collision tunnelling. Collision accounts for body height, ceilings, and landing on low surfaces. There is no forced head bob or flying. The room is enclosed and the entrance remains closed.

## What comes from Vesper3D

The original Rust scene graph, materials, primitive definitions, models, transform math, ray intersection code and BVH are retained. Blue Engine builds its studio with those same scene types and compiles it through `geometry::Compiled`. The crystal and little robot are the original engine's reusable models.

A new real-time layer tessellates the evaluated primitives once, bakes static lighting and contact shadows using the original BVH, and draws those meshes through a GPU backend. The camera and player update each frame. This keeps offline scene construction out of the interactive frame loop. The live viewer is intentionally a static-room prototype; it is not the offline renderer running every frame and does not claim identical shading.

The original offline commands remain available in **bin/vesper3d.exe**. See VESPER_README.md and AI_REFERENCE.md for authoring and MP4 export. FFmpeg is needed only for the original video export workflow.

## Future interaction

`src/viewer/room.rs` separates the scene, collision bounds and semantic entities. Each entity has a stable ID, display label and bounds. `Room::focus` uses the original BVH to find the first visible surface within 4.5 metres; occluded objects do not show a label. The crosshair labels relevant display pieces as you approach.

A later version can resolve the focused entity ID into an interaction component and dispatch an action. No pickup, door opening, inventory, or other action is bound in this version. Animated objects will need dynamic meshes and updated collision/focus geometry; the current lighting bake is static.

## Build and checks

```
cargo run --release --bin blue-engine
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
```

The default Cargo target is Blue Engine. The library remains named `vesper3d` to preserve the inherited code and tests. `cargo run --release --bin vesper3d -- doctor` runs the offline tool.

For a repeatable render smoke check, run `BlueEngine.exe --capture PATH_TO_EMPTY_FOLDER`. It renders three camera views, writes PNGs and a small timing report, then exits. Existing same-named captures in that explicitly supplied folder are replaced. Measurements include presentation/vsync and are not GPU-only benchmarks.

For repeatable jump/crouch screenshots, use `BlueEngine.exe --capture-motion PATH_TO_EMPTY_FOLDER`. This simulates a jump, a held crouch, and standing again; the report records the captured eye heights. As with the regular capture mode, same-named output files are replaced in the supplied folder.

See VALIDATION.md for actual checks and limitations. VESPER_VALIDATION.md records the inherited engine's earlier release. This repository preserves that engine's local Git history; no remote repository has been published.
