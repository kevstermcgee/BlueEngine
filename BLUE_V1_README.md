# Blue Engine 0.1

A native first-person room viewer in Rust, built from the Vesper3D animation engine. Explore a furnished blue-accented studio at eye level using the keyboard and mouse.

## Run

Open **bin/BlueEngine.exe**, then click **Enter the room** or press Enter. On Windows, every launch opens maximized within the current monitor's work area, keeping the title bar and taskbar available. The view and menu adapt to the resulting client size; F switches between fullscreen and maximized view. No installation, internet connection, server, Rust toolchain, or FFmpeg is needed for the viewer. The room and shaders are compiled into the executable. Windows x64 with an OpenGL-capable graphics driver is the tested platform.

| Control | Action |
|---|---|
| W / Up | Walk forward |
| S / Down | Walk backward |
| A / Left | Strafe left |
| D / Right | Strafe right |
| Mouse | Look in any direction |
| Left-click | Swing the wrench |
| E | Use the object under the crosshair |
| Right-click or Backspace | Dismiss the information card |
| Hold Shift | Sprint (works with WASD and arrow keys) |
| Space | Small jump (about 35 cm) |
| Hold Ctrl or C | Crouch and move slowly |
| Escape / Tab | Pause or resume; release or capture the cursor |
| Enter | Enter or resume from the menu |
| F (or F11) | Toggle fullscreen / maximized view |
| H | Hide or show the on-screen hints |
| F3 | Show performance and camera coordinates |
| Q | Toggle first-person / third-person view |

Pause to adjust mouse sensitivity, vertical field of view, invert vertical look, or reset your position. Switching to another app pauses the viewer and releases its mouse capture. Settings last for the current session. Arrow keys move relative to your view; left and right strafe, matching A and D.

Standing eye height is 1.68 metres. Space triggers a small 35 cm jump with gravity and a grounded landing; midair jump presses are ignored. Hold either Ctrl key or C to crouch smoothly to a 0.98 metre eye height and move at a slower 1.3 m/s. Release to stand when there is enough overhead clearance. Crouching also works in midair without moving your feet artificially. Normal walking is 3.2 m/s (up from 2.6 m/s). Hold either Shift key to sprint at 5.6 m/s, and release to return smoothly to walking. Crouching takes priority over sprinting. Movement is normalized, accelerates and stops smoothly, slides along walls and furniture, and is subdivided to resist collision tunnelling. Collision accounts for body height, ceilings, and landing on low surfaces. There is no forced head bob or flying. The room is enclosed and the entrance remains closed.

## What comes from Vesper3D

The original Rust scene graph, materials, primitive definitions, models, transform math, ray intersection code and BVH are retained. Blue Engine builds its studio with those same scene types and compiles it through `geometry::Compiled`. The crystal and little robot are the original engine's reusable models.

A new real-time layer tessellates the evaluated primitives once, bakes static lighting and contact shadows using the original BVH, and draws those meshes through a GPU backend. The camera and player update each frame. This keeps offline scene construction out of the interactive frame loop. The live viewer is intentionally a static-room prototype; it is not the offline renderer running every frame and does not claim identical shading.

The original offline commands remain available in **bin/vesper3d.exe**. See VESPER_README.md and AI_REFERENCE.md for authoring and MP4 export. FFmpeg is needed only for the original video export workflow.

## Interactions

`src/viewer/room.rs` separates the scene, collision bounds and semantic entities. Each entity has a stable ID, display label and bounds. `Room::focus` uses the original BVH to find the first visible surface within 4.5 metres; occluded objects do not show a label. The crosshair shows a blue ring and a contextual E prompt when an object is available.

Aim at an object within 4.5 metres and press E once:

- **Desk monitor:** switch its display on or off.
- **Crystal:** toggle between blue and amber finishes.
- **Blue notebook:** read a short studio note.
- **Artwork, robot, lounge, workbench and table:** inspect a short description.
- **Entrance:** check its status; it remains closed in this room demo.

Left-click swings the wrench; E performs the displayed object action. Holding a button does not repeatedly activate it. Only the nearest visible surface is considered, so you cannot activate objects through walls or furniture. Information cards fade after six active seconds or can be dismissed with right-click or Backspace. Interaction prompts remain visible when H hides the general controls. Paused/menu clicks cannot trigger room actions, and resuming discards the initial click. Object states last for the current session; Reset position only resets the player.

`src/viewer/interaction.rs` owns the typed actions, state and feedback independently of keyboard/mouse input. Future actions can extend this module and assign an Action to a stable entity ID. Monitor/crystal appearances update with shader uniforms without rebuilding the room. Geometry and lighting stay static. Pickups, inventory and opening doors remain future work.

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

For repeatable interaction screenshots, use `BlueEngine.exe --capture-interactions PATH_TO_EMPTY_FOLDER`. It captures the monitor on/off, crystal blue/amber, notebook inspection, and pause menu. Same-named files in the supplied directory are replaced.

For repeatable jump/crouch screenshots, use `BlueEngine.exe --capture-motion PATH_TO_EMPTY_FOLDER`. This simulates a jump, a held crouch, and standing again; the report records the captured eye heights. As with the regular capture mode, same-named output files are replaced in the supplied folder.

See VALIDATION.md for actual checks and limitations. VESPER_VALIDATION.md records the inherited engine's earlier release. This repository preserves that engine's local Git history; no remote repository has been published.


## Wrench

A blue-sleeved hand holds an open-ended steel wrench. Left-click once to swing; each swing has a wind-up, one contact instant, and recovery. Aim at a surface within 1.65 metres to hit it. A successful strike produces sparks, an amber hit marker and the object name. Walls and furniture block hits on objects behind them. The wrench does not break or move the room furnishings; E retains the monitor, crystal and inspection interactions. Pause/focus loss cancels a pending swing, and menu/resume clicks never attack.

`BlueEngine.exe --capture-wrench PATH_TO_EMPTY_FOLDER` captures idle, contact, recovery and the menu, and records the hit count.


## Default character and third-person view

The default Blue mechanic wears a blue work jacket with reflective trim and a back emblem, dark trousers, knee pads and boots. The character has a simple face and short brown hair, and carries the same wrench used in first person. Walking animates the legs and free arm; crouching bends the knees, jumping raises the body, and the wrench follows the right hand during a swing.

Press Q to toggle views, including while paused. Mouse look, WASD/arrows, sprint, crouch, jump, E and left-click work in either view. Q no longer quits; use the menu's Quit button. Startup defaults to first person. The third-person camera follows over the right shoulder and pulls inward at walls/furniture. In very tight spaces it hides the body and shows the first-person hand to preserve visibility; it restores the body when clear. Targeting converges on the crosshair from the player's eye, so switching views does not extend interaction or wrench reach.

Use `--capture-character PATH_TO_EMPTY_FOLDER` for first/third-person, swing, crouch, jump, portrait and pause captures. `--third-person` starts in third person and can be combined with existing capture modes, including `--capture-wrench`.
