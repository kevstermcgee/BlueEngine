# Backstop

A native 2D spatial logistics puzzle about arranging the space **behind** you.
Deliver every crate to a cyan target, then press Enter to advance through eight compact bays.
Windows x64 EXE installer delivery; Linux is the development and verification platform.

Every shove moves cargo **exactly one cell plus the consecutive clear cells behind the pusher**.
Walls, crates and board edges stop that measurement. All forward cells must be clear: a blocked
launch is rejected in full, never shortened. After a launch, you enter the crate's former cell.
Targets are ordinary floor. Delivered crates can move again and can act as backstops.
There is no pulling, chain pushing, clock, move limit, life counter or rejection penalty.

- Arrows / WASD: walk one cell and face that direction; hold to repeat.
- Space: shove the adjacent crate. Blue cells measure the rear strip; numbered amber cells show
  travel; a bold outline marks the landing. Red marks an invalid route and X its first obstruction.
- Z or Undo button: undo the last walk, turn or shove, including a solved room or deadlock.
- R or Retry button: restart **this bay**, keeping your campaign position.
- Enter or Next Bay button: advance after docking all cargo.
- K / L: engine-owned save / load, including bay, facing, cargo and undo history.
- Esc: pause. M: sound toggle. F: fullscreen.

An exact search detects arrangements that cannot reach every target. The attempt is marked
`NO ROUTE TO GOAL`, with Z and R immediately available. This recoverable failure has no terminal
loss screen. Complete all eight bays to finish the shift. Bays introduce wall distances,
installing a cargo backstop, delivery order, helper reuse and temporarily undocking a delivery.

## Development

Use the engine's `tools/be2.py start` with this project. `src/lib.rs` owns the 60 Hz integer simulation,
public Intent commands, exact solvability search and Snapshot validation. `src/rooms.rs` contains
all eight layouts and public-input solution routes. `src/presentation.rs` only reads state; the
shared native client owns devices, timing, audio cues, menus and storage. There is no browser build.

```
cargo test --no-default-features
cargo run
python3 ../../tools/be2.py check --game . --loop inner
python3 ../../tools/be2.py check --game . --loop integration
python3 ../../tools/be2.py check --game . --loop shipping
```

The rule suite covers rear walls and cargo backstops, exact-route rejection, no chain pushing,
recoverable deadlock, undo/retry, held input, all eight room routes, deterministic replay,
invalid saves and exact engine Snapshot continuation including a sliding crate and undo history. Undo and retry suppress held directions until release, so a held key cannot immediately redo recovery.
`verification_input` completes the campaign through ordinary walking/shoving/undo/advance commands.

On Windows, run `python scripts/ship.py ship --no-install` for packaging and isolated smoke,
then the scaffold's `installer prepare`, `installer build` and isolated `installer test` gates.
Linux checks and software screenshots do not certify a Windows installer or physical audio.
No shortcuts are installed during development. Device review criteria are in `assets/native-review.json`.

On a Linux host with Xvfb and display sockets available, inspect the real keyboard path and native captures with:

```
cargo build
python3 tools/native_playtest.py /absolute/path/to/target/debug/backstop
```

This script sends X11 key events to the unmodified shared client, reads engine-written saves to
check observations, and captures the actual framebuffer. It never injects game state. See STATUS.md
for the current environment's pending review and delivery evidence.
