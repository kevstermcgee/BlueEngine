# BE2 vocabulary

| Term | Meaning here |
|---|---|
| BE2 / Blue Engine 2 | Current package; `be2` client, `be2-headless` simulation runner and `be2-tools` editor. |
| Vesper / `vesper3d` | Inherited library name and optional offline renderer binary; not a separate server. |
| Controller | Player movement, stance, look and collision state; `position` is the eye position, feet height is separate. |
| Movement | Intent rather than a position update. Jump is a press edge; other actions/axes persist in the headless world until replaced. |
| Tick | One 1/60-second simulation update. A render frame may contain zero or several ticks. |
| PlayerStepper | Client accumulator and pose interpolation; reset when discarding timing debt. |
| HeadlessWorld | Local room and up to eight player states; exposes join/input/step/player/leave without sockets. |
| Room | Compiled scene geometry, collision proxies and semantic entities. |
| Node | Visual scene object; its ID need not equal a collider or entity ID. |
| Collider | Axis-aligned collision proxy, independent of visible geometry. |
| Entity | Stable semantic ID, label and interaction bounds/action. |
| MapDocument | Strict versioned static-map JSON containing scene, collision and entity data. |
| Scene | Vesper visual description; scene-only export omits map collision and entities. |
| Prop / prefab | Reusable object geometry or data template. Neither implies possession or a disguise mechanic. |
| Route / ray | Controller reachability along explicit waypoints / visibility against render geometry. Neither substitutes for the other. |
| PulseNet | Planned transport integration, not included in this repository. |
| Orchestrator / match server | Future deployment concepts; BE2 currently implements neither a matchmaking orchestrator nor an online match server. |

Coordinates are metres with Y up; sizes named `half_extents` are half the full dimensions. Prop origins are at their bottoms. Consult each API for orientation and bounds conventions.
