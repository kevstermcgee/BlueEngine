# Blue Engine: real-time extension

See README.md for controls and ARCHITECTURE.md for the inherited offline pipeline.

- `viewer/controller.rs`: pure player simulation, look angles, grounded movement, circle versus AABB collision. No window or device dependency.
- `viewer/room.rs`: typed Vesper scene assembly, collision bounds, stable semantic entities and occlusion-aware focus query.
- `viewer/mesh.rs`: primitive tessellation and static BVH lighting bake; GPU mesh batches with per-fragment metal highlights.
- `bin/blue-engine.rs`: window lifecycle, input, mouse capture, pause/settings UI, graphics submission and capture smoke mode.

Keep future gameplay components keyed by entity ID rather than names or mesh indices. Rendering objects and collision proxies are deliberately separate: a desk blocks the whole table volume while decorative objects do not require individual physics bodies. The camera has a small near plane and player radius to avoid clipping through walls.

The library remains `#![forbid(unsafe_code)]`. The Windows executable confines small documented unsafe blocks to read-only Win32 focus and key-state queries. The keyboard event subscriber supports the window library; Windows polling supplements hardware scan-code input for accessibility-generated key presses and missed releases. Edge state is tracked separately from movement so holding Escape does not toggle the menu repeatedly. Brief between-frame taps survive as one frame of movement.

All assets needed by the viewer are embedded procedurally. Segoe UI is loaded from the existing Windows fonts folder when present, with the graphics library's built-in font as fallback. No Windows font files are redistributed.

Current constraints: fixed studio, static baked lighting, conservative box collision proxies, no saved preferences, no object actions, no gamepad/mobile support. Cross-platform builds have not been executed. Imported Vesper scene files still use the offline CLI; the viewer does not yet expose a general-purpose scene loader.
