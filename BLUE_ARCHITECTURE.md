# Blue Engine: real-time extension

See README.md for controls and ARCHITECTURE.md for the inherited offline pipeline.

- `viewer/controller.rs`: pure player simulation, look angles, grounded movement, jumps, crouching, and height-aware cylinder versus AABB collision. No window or device dependency.
- `viewer/room.rs`: typed Vesper scene assembly, collision bounds, stable semantic entities and occlusion-aware focus query.
- `viewer/mesh.rs`: primitive tessellation and static BVH lighting bake; GPU mesh batches with per-fragment metal highlights.
- `bin/blue-engine.rs`: window lifecycle, input, mouse capture, pause/settings UI, graphics submission and capture smoke mode.

Keep future gameplay components keyed by entity ID rather than names or mesh indices. Rendering objects and collision proxies are deliberately separate: a desk blocks the whole table volume while decorative objects do not require individual physics bodies. The camera has a small near plane and player radius to avoid clipping through walls.

The library remains `#![forbid(unsafe_code)]`. The Windows executable confines small documented unsafe blocks to Win32 focus/key queries and operations on its own window. The keyboard event subscriber supports the window library; Windows polling supplements hardware scan-code input for accessibility-generated key presses and missed releases. Edge state is tracked separately from movement so holding Escape does not toggle the menu repeatedly. Brief between-frame taps survive as one frame of movement.

All assets needed by the viewer are embedded procedurally. Segoe UI is loaded from the existing Windows fonts folder when present, with the graphics library's built-in font as fallback. No Windows font files are redistributed.

Current constraints: fixed studio, static baked lighting, conservative box collision proxies, no saved preferences, no inventory/door opening, no gamepad/mobile support. Cross-platform builds have not been executed. Imported Vesper scene files still use the offline CLI; the viewer does not yet expose a general-purpose scene loader.


Jump/crouch update:
Feet height, vertical velocity, grounding, and body height are tracked independently from the camera. Gravity is integrated with bounded substeps and swept vertical collision. Jump height is 0.35 m with gravity 12 m/s^2. Standing body height is 1.80 m, crouched body height 1.10 m, and eye clearance below the head is 0.12 m. Stance changes at 4 m/s and standing expansion stops at overhead obstructions. Horizontal speed is 1.3 m/s crouched, 3.2 m/s normally, and 5.6 m/s when holding Shift to sprint. Pause freezes vertical simulation and clears horizontal momentum; resuming continues the jump naturally. Movement.jump is a press edge; the executable maps Space to that edge and Ctrl/C to held crouch input. Reset restores grounded standing state.

Interaction update:
`viewer/interaction.rs` defines Action, Interactions and Feedback. Room entities declare ToggleMonitor, CycleCrystal or Inspect. E dispatches once through the activation guard; left-click starts the wrench swing. The activation path rechecks the current ray against Room::focus; it never trusts an earlier HUD target. Menu and mouse-capture settling frames reject activations. Information cards expire after six active seconds or dismiss with right-click/Backspace.

`Room::render_tags` supplies bounds and numeric tags for the monitor display and crystal. Mesh baking assigns tags to whole evaluated primitives using their bounds centres. The shader changes only those tagged surfaces through ObjectStates uniforms. No runtime geometry rebake or BVH change is needed for colour/display toggles. Keep render-tag bounds synchronized with moved props; tests and visual checks prevent the pedestal from being tinted with the crystal. Focus entities for the notebook and monitor precede the larger table/workbench bounds. The monitor can also be selected from its frame; the crystal's selection excludes the pedestal.

Maximized startup:
`src/bin/platform_window/mod.rs` enumerates top-level visible windows belonging to the current GUI thread, then queues WM_SYSCOMMAND / SC_MAXIMIZE for its own window. Posting the command avoids synchronous re-entry into the graphics handler. The startup future yields a frame before loading the scene, allowing Windows to maximize and the renderer to process the resize. Fullscreen is explicitly disabled on launch. Windows chooses the window's current monitor work area, retaining normal window chrome and taskbar space; rendering reads the actual resized client dimensions. The existing desktop shortcut additionally has WindowStyle=3 (Run maximized). Capture reports include the native maximized/caption state and viewport size. Non-Windows builds retain their existing startup behaviour.


Wrench update:
`viewer/wrench.rs` owns input-independent swing timing and impact data. Contact at 0.18 seconds uses the current camera ray against the nearest actual world surface, limited to 1.65 metres. Recovery completes at 0.52 seconds, with one hit at most per swing. The impact stores world position, normal, semantic label, age and a session hit counter. No collider/geometry mutation occurs. Pausing or losing focus cancels wind-up; the active simulation controls time. `bin/wrench_view/mod.rs` draws a procedural hand, sleeve and wrench into a separate transparent depth target so nearby world surfaces do not clip the viewmodel. Its fixed 65-degree lens keeps the hand readable as world FOV changes. Sparks use the normal and contact point in world space, while the hit marker is drawn in the HUD. No new assets or dependencies.


Character and perspective update:
`viewer/camera.rs` contains the pure Perspective/View camera and aim calculations. The shoulder boom sweeps a 0.18 m sphere against expanded collision boxes and checks actual scene geometry. There is no minimum boom length that could push the camera through a wall; when clearance is under 0.55 m the view hides the body. Third-person camera rays select a visible aim point, then gameplay rays originate at the player's eye to preserve range and occlusion. Movement and controller state do not depend on perspective.

`bin/character/mod.rs` draws the default procedural Blue mechanic, with a distance-driven gait, stance-driven knees/torso/head height, look pitch and shared wrench swing phase. Controller exposes read-only feet/body height for rendering. `wrench_view` shares the hand/tool geometry and swing curve between world and first-person rendering. Q toggles in play or pause; Quit is button-only. No scene schema or external assets changed.
