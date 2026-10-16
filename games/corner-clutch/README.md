# Corner Clutch

Choose the driver. Change the crew.

A flat, top-down spatial puzzle in eight compact rooms. Click a marked crate to
select its gold corner grippers. Its immediate diagonal neighbors glow mint:
those crates ride with it, but never recruit their own neighbors. Side neighbors
stay still. Move the crew one cell, then check the new corners.

Put **every letter on its matching socket at once**. Sockets do not lock crates;
a parked crate can still help. Hover a direction button to see destination ghosts.
Red landings show a wall, a stationary crate or the board edge. If any landing is
blocked, the entire command is rejected without changing the arrangement.

There is no timer, life limit or move budget. Undo is unlimited. A solved room
waits for **Next room** or Enter, so you can still undo before continuing.

- Click a crate, use 1–4, or Tab to change drivers.
- Tap WASD or an arrow key for one cell. Holding does not repeat.
- Hover a direction button to preview; click it to move.
- Z or the Undo button: undo a move. R: reset only the current room.
- Hint: show the room's teaching suggestion. Esc: pause/resume.
- K: engine-owned quick-save. L: load. Progress also autosaves.
- M: sound. N: music setting. F: fullscreen.

Undo and driver selection work while a direction remains held; release and tap
again to make another keyboard move. Selection and movement stay ordered even
when several display frames pass before a fixed tick. Selection comes first if
it shares a frame with a direction press. Undo takes priority over other presses
in its frame. Releasing any key never moves a crate, including when another
direction remains held or opposite directions cancel.

The Windows x64 executable installer is the delivery target. Linux is declared
for development and verification; no browser build is supplied.

## Development

From this folder:

```
cargo test --no-default-features
python3 tools/solve_rooms.py
cargo run
```

The independent breadth-first search proves the eight layouts solvable in
2, 5, 6, 8, 9, 10, 12 and 14 moves. Room 3 takes nine moves if three-crate
carrying is forbidden, so its shortest route uses the middle driver before splitting
from an end. The search also exhaustively checks that the already parked helper
in room 5 must leave its socket. The Rust tests exercise the same
rules through public Intent commands, including the complete campaign, rejected
moves, undo, room reset, deterministic hashes and exact Snapshot continuation.
Saves include the complete undo stack and bind to the authored room data.
A reachable full deadlock is tested through ordinary moves: it stays recoverable
after waiting, saving and loading, with no terminal loss or penalty.
Input regressions cover held directions during Undo/selection, cursor coincidence
with hotkey targets, and display frames that run zero, one or multiple fixed ticks.
Closely spaced selection/movement, repeated Tab presses, deferred button-edge
clicks and undo are tested through the same native adapter used by the window.
The adapter queues device events until a fixed tick consumes each command;
restart and load discard pending device events. Clicking a moving crate keeps
its identity even when an earlier queued move changes its cell.
Every perpendicular direction pair is tested with either key released, including
after saving with both axes held. Version-one saves migrate to version two while
preserving the board, undo history and the previously held direction.

`src/lib.rs` owns the fixed 60 Hz integer simulation. `src/presentation.rs` reads
it and maps native key presses through `src/input.rs` into ordered public click intentions. The engine's shared
native client owns the viewport, devices, pause, storage, cue playback and timing.
The room data is compiled in, with no external path or scene interpolation.

On Windows, `python scripts/ship.py ship --no-install` builds and checks an
isolated native package. The scaffold includes installer preparation tooling;
installer creation/testing and physical device review require Windows. The
supervisor handles publication. See `VERIFICATION.md` for observed local evidence.
