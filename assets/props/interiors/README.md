# Interior prop library

53 reusable data-only static prefab scenes, with bottom-center origins and conservative collision/inspection bounds. They use native MapDocument v1 primitives. No new renderer features or dependencies are required.

```powershell
python tools/place_interior.py --list
python tools/place_interior.py assets/maps/starters/office.json mug new-office.json --id spare-mug --at=-6.1,0.8,5 --yaw 0
```

The helper validates the complete result with the packaged native audit before creating a new output. It preserves the source, rejects existing outputs and occupied IDs, and supports quarter-turn placement with matching bounds. Run route checks after placement. These prefabs are separate from the seventeen native `add_prop` kinds; `tools/author.py assets` continues to describe that native catalogue.

Catalogue IDs: student-desk, backpack, locker, file-cabinet, waste-bin, computer, papers, mug, printer, coffee-machine, microwave, register, bottle, snack-bag, carton, lunch-tray, wall-window, bench, bookcase, soap-dispenser, math-board, reading-board, closed-double-door.

Additional templates: wall-clock, plain-basket, retail-shelf, sofa-two-seat, pencil-cup, trophy, water-dispenser, toilet-fixture, sink-vanity, toilet-roll, orange, pear, carrot, bread-loaf, produce-stand, shopping-basket, small-cubby, cubby-bin.

Unlit lamp templates: floor-lamp-drum, floor-lamp-reading, table-lamp-ceramic. All use non-emissive materials; placement origins are bottom-center.

Coffee machine cleanup: body and upper housing now meet without overlapping volumes or coplanar side patches; thicker tray and front control button. Updated office, market and house instances. Bounds unchanged; all three native audits and geometry checks passed. Backup/report: .be2-work/coffee-cleanup.

New framed art: painting-coast, painting-sunset, painting-geometric, painting-botanical, school-solar-print, school-shape-print, market-citrus-print, market-coffee-print.

Shopping baskets: shopping-basket now has open slotted sides and paired carry handles; shopping-basket-stack is a reusable nested three-basket group. Both use bottom-center origins.
