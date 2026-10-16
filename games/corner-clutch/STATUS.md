Corner Clutch is a native top-down 2D puzzle with eight search-validated rooms.
It implements direct, nonrecursive diagonal pickup, atomic group movement,
matching sockets, unlimited undo and room-local restart. There is no terminal
loss, timer or move budget. Windows delivery and Linux verification are declared.

The selection-order regression is fixed with a game-local device queue. A later
move cannot overwrite an earlier selection before a fixed tick. Selection and
movement in one frame execute in order; deferred clicks retain crate identity.
Repeated Tab, undo, held keys, coincident cursors and button edges are covered.
The board changes only through Simulation and public Intent commands.
Snapshot version two and its version-one migration remain compatible.

All 22 headless tests, Clippy and the independent room search pass. Fresh canonical
scaffold resources and regenerated icon art were checked. Retained native visual
evidence was inspected. The final shipping command and its reports are described
in VERIFICATION.md. Fresh native controls/captures and Windows installer delivery
remain unverified because this host cannot open display sockets or run Windows.
Packaging scripts match engine templates; the pinned-launcher checker conflicts
with that supervisor requirement. Shared stock cues substitute for a clack.
