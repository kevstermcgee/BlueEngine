# Map plan

1. **House — playable:** two floors, living room, kitchen/dining area, two bedrooms, bathroom, staircase and fenced backyard.
2. **School — playable:** classrooms, cafeteria, gym and fenced outdoor play/learning grounds.
3. **Office — playable:** workspaces, meeting rooms, bathroom and a four-room annex.
4. **Convenience store — playable:** stocked store, parking, receiving bay and outdoor seating/stalls.

`viewer/maps.rs` selects map builders; `viewer/house.rs` builds the house and its collision world. Both the client and headless runner default to House. The original studio is retained for regression captures through `--studio`.

`viewer/props.rs::place` instantiates a shared prop definition with a unique per-map entity ID. Reuse the same cereal box, chair, table and apple geometry across future maps. Keep placements and collision bounds in the map builder, and prop geometry in the shared catalogue.

The house is an exploration map foundation. Prop possession, rounds, scoring and online matches remain future game-layer work.

House cover: shrub beds around all sides, an L-shaped backyard privacy screen, a living-room sofa pocket and wall-aligned bedroom furniture. Main routes stay open. Reachability and entrance sightlines are regression-tested; gameplay balance still needs player testing.

These JSON starters load explicitly through `--map`; see assets/maps/starters/README.md for launch commands and validation. Native audit and ten route checks pass. These are static exploration foundations, not completed Prop Hunt modes.

Latest map redesign and validation: assets/maps/starters/README.md. Desktop shortcuts use bin/BE2-decor.exe for the updated native basket geometry.

The four launcher maps were expanded to approximately twice their previous gross floor/yard area. The house now includes four garden pavilions and an allotment/picnic area. Details and current route/physics checks: assets/maps/starters/README.md.
