# Blue Engine: real-time extension

See README.md for controls and ARCHITECTURE.md for the inherited offline pipeline.

- `viewer/controller.rs`: pure player simulation, look angles, grounded movement, jumps, crouching, and height-aware cylinder versus AABB collision. No window or device dependency.
- `viewer/room.rs`: typed Vesper scene assembly, collision bounds, stable semantic entities and occlusion-aware focus query.
- `viewer/mesh.rs`: primitive tessellation and static BVH lighting bake; GPU mesh batches with per-fragment metal highlights.
- `bin/blue-engine.rs`: window lifecycle, input, mouse capture, pause/settings UI, graphics submission and capture smoke mode.

Keep future gameplay components keyed by entity ID rather than names or mesh indices. Rendering objects and collision proxies are deliberately separate: a desk blocks the whole table volume while decorative objects do not require individual physics bodies. The camera has a small near plane and player radius to avoid clipping through walls.

The library remains `#![forbid(unsafe_code)]`. The Windows executable confines small documented unsafe blocks to read-only Win32 focus and key-state queries. The keyboard event subscriber supports the window library; Windows polling supplements hardware scan-code input for accessibility-generated key presses and missed releases. Edge state is tracked separately from movement so holding Escape does not toggle the menu repeatedly. Brief between-frame taps survive as one frame of movement.

All assets needed by the viewer are embedded procedurally. Segoe UI is loaded from the existing Windows fonts folder when present, with the graphics library's built-in font as fallback. No Windows font files are redistributed.

Current constraints: fixed studio, static baked lighting, conservative box collision proxies, no saved preferences, no object actions, no gamepad/mobile support. Cross-platform builds have not been executed. Imported Vesper scene files still use the offline CLI; the viewer does not yet expose a general-purpose scene loader.


Jump/crouch update:
Feet height, vertical velocity, grounding, and body height are tracked independently from the camera. Gravity is integrated with bounded substeps and swept vertical collision. Jump height is 0.35 m with gravity 12 m/s^2. Standing body height is 1.80 m, crouched body height 1.10 m, and eye clearance below the head is 0.12 m. Stance changes at 4 m/s and standing expansion stops at overhead obstructions. Horizontal speed is 1.3 m/s crouched, 2.6 m/s normally, and 4.2 m/s with Shift. Pause freezes vertical simulation and clears horizontal momentum; resuming continues the jump naturally. Movement.jump is a press edge; the executable maps Space to that edge and Ctrl/C to held crouch input. Reset restores grounded standing state.
