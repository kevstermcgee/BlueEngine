# Furnished BE2 maps

## Expanded maps — September 23, 2026

Each launcher's map now provides approximately twice its previous gross playable floor/yard area. Existing interiors are retained.

| Map | Original / expanded area | Added spaces | Added physical props |
| --- | --- | --- | --- |
| House | 562.64 / 1125.42 m² | Rear garden, workshop, guest pavilion, potting shed, garden kitchen and picnic/allotment area | 81 |
| School Wing | 352 / 704 m² | Fenced play deck, sand/activity area, outdoor class tables and learning garden | 57 |
| Office | 352 / 704 m² | Connected training, project, archive and staff lounge annex | 67 |
| Convenience Store | 352 / 704 m² | Parking forecourt, covered receiving bay, picnic seating and farm stands | 64 |

Area means gross floors plus accessible grounds, before subtracting walls/furniture; upper house slabs are included once. Ground and objects were added at their normal scale. House additions are beyond the former rear fence. Other maps connect through their former front doors. Close and reopen the existing map shortcut to load the expansion. A bare executable launch still uses the legacy procedural house; use the named shortcuts or `--map` for these maps.

Validation: four clean native audits; 18 retained interior routes, 25 extension routes (including the play-deck steps), and three newly open entrance routes; 1,200-tick headless loads for every map. Runtime physics checks recognize all 269 new loose props and exercise settling plus pickup/drop as both Scientist and Feta. Targeted offline room/yard renders and the native pause menu were reviewed. Not exhaustive playtesting of every camera or physics collision.

Recheck physics with `cargo run --release --locked --no-default-features --example validate_expanded_maps`. Persistent map reports/hashes: `expansion-validation.json`. New route fixtures use `.expansion-*.json` and `.entry-open.json`; `.exit-negative.json` files are historical pre-expansion fixtures and are no longer expected to fail. Originals, authoring patches, renders and detailed logs are retained in `.be2-work/map-expansion`.

## Earlier furnishing history


Open the desktop shortcuts BE2 - House, BE2 - School Wing, BE2 - Office and BE2 - Convenience Store. Close an older running map and relaunch to load updated files. Shortcuts now target bin/BE2-decor.exe because the previous BE2.exe was running during installation.

- School: two consistently oriented classrooms with written blackboards, separately mounted clocks and noticeboards, supply cubbies, open cafeteria commons, serving counter, lockers, trophy/reading area and gym.
- Office: eight seats in two desk pods, reception, lounge, rear conference room, kitchen and two-stall restroom with vanity, soap, mirror, paper and towels. Meeting signage is mounted on a solid wall.
- Market: six plain panel/post shelves with spaced stock, simple storage baskets, three divided stands of oranges, pears and carrots, bread display, carry baskets, checkout and drinks coolers.
- House: entry cubby, basket, kitchen bread and bathroom paper added to the furnished house snapshot.

The original front doors of the school, office and market are now open connections to their extensions. Windows/mirrors remain decorative opaque surfaces. Loose native catalog props support E pickup/drop and rigid-body physics; large furniture and architecture remain fixed.

53 reusable data-only interior templates are in assets/props/interiors. Use tools/place_interior.py for validated placement; the seventeen native prop kinds remain separate.

Validation: four clean native audits, sixteen successful controller routes, three deliberately blocked exterior routes; 41 successful rotated template placement audits; all 18 new template bounds checked; shelf/basket panels checked for non-overlapping volumes. All four maps passed 600-tick headless smoke checks. Targeted interior renders reviewed, plus native market client captures. The native basket now uses five abutting panels with a regression test; full tools/be2.py check and build all passed. Still images and geometry checks do not establish every possible live-camera view or exhaustive traversal.

Previous maps/library/binaries: .be2-work/distinct-maps/before-publish. Reports, captures and scratch outputs: .be2-work/distinct-maps/revision-4. Map audits/routes: redesign-validation.json.

Lamp update: school teacher-desk lamps and a reading floor lamp; office lounge floor lamp and reception table lamp. Lunchroom bin moved clear of the serving counter. Two map audits, ten circulation routes, new prop bounds/rotated placements and two headless smoke runs passed. Details: .be2-work/distinct-maps/lamps-1.

Office fridge contrast update: blue-gray doors, dark cabinet edges, pale handles and a pinned note. Native audit and kitchen controller route passed; close-up render reviewed. Backup/report: .be2-work/fridge-contrast.

Market signage: Corner Market moved onto the wall beside the entrance; EXIT remains above the doors. Both now have contrasting backing panels. Native audit passed and final render reviewed. Backup: .be2-work/market-sign-spacing/before.json.

Wall decor update: seven additional framed pieces per map and five unlit lamps across all four maps. Eight reusable art templates added. All four audits, sixteen controller routes, eight rotated asset placements, art/scene overlap checks, floor-lamp clearances and four headless smoke runs passed. Room renders reviewed. Backups/reports: .be2-work/distinct-maps/wall-decor-4.

House landing: sofa, inward-facing chair, coffee table, reading papers/mug, floor lamp, rug and two framed prints. Entry cubby reoriented fully inside the front wall with its basket/lamp. Audit, existing upstairs/garden routes and new landing/bedroom route pass; collider checks and renders reviewed. Backup: .be2-work/distinct-maps/landing-2/before.json.

Office stall correction: the two previously detached open leaves now sit beside the shared stall divider with visible hinges, clear of the window and toilet entrances. Audit and bathroom route passed; final render reviewed. Backup: .be2-work/stall-door-fix/before.json.

Market basket update: replaced the box tower with a nested basket stack with open sides, rims and carry handles. Native audit, four routes, prefab bounds and two rotated placements passed. Close-up render reviewed; backup/report: .be2-work/distinct-maps/shopping-baskets-1.


Bedroom decor update: the large bedroom now has a reading loveseat, low table with books/mug, rug, floor lamp and dresser with a cactus/keepsake bowl. The smaller bedroom has a bedside cabinet/lamp, coastal print, rug and laundry basket. All original components preserved. Native audit (no duplicate boxes), four routes including new house.route-4.json, collision-overlap check and 600-tick headless smoke passed. Reviewed both bedroom renders and native client bedroom/menu captures. Updated the house JSON used by the map shortcut; procedural default and binaries unchanged. Backup/review: .be2-work/bedroom-decor; persistent report: assets/maps/starters/bedroom-validation.json.
