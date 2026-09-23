# Map plan

1. **House — playable prototype:** two floors, living room, kitchen/dining area, two bedrooms, bathroom, staircase and fenced backyard.
2. **School — planned:** classrooms, cafeteria and gym.
3. **Office — planned:** workspaces, meeting rooms and bathroom.
4. **Convenience store — planned.**

`viewer/maps.rs` selects map builders; `viewer/house.rs` builds the house and its collision world. Both the client and headless runner default to House. The original studio is retained for regression captures through `--studio`.

`viewer/props.rs::place` instantiates a shared prop definition with a unique per-map entity ID. Reuse the same cereal box, chair, table and apple geometry across future maps. Keep placements and collision bounds in the map builder, and prop geometry in the shared catalogue.

The house is an exploration map foundation. Prop possession, rounds, scoring and online matches remain future game-layer work.

House cover: shrub beds around all sides, an L-shaped backyard privacy screen, a hall cabinet, living-room sideboard, kitchen island and bedroom divider. Main routes stay open. Reachability and entrance sightlines are regression-tested; gameplay balance still needs player testing.
