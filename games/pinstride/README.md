# Pinstride

A native 3D spatial movement puzzle for BlueEngine. Swing a rigid baton 90 degrees around
its pinned tip. The arriving tip becomes the new pin, so every move changes both the pose
and the pivot available next. Seat the blue circle and orange diamond in their matching
floating sockets to complete a chamber. Eight chambers finish the game.

## Play

- Click a numbered destination ghost or press **1–4** to preview a quarter-turn.
- **Space** commits the selected swing, or continues after docking both tips.
- **Right mouse drag** or **arrow keys** orbits; the **wheel** zooms.
- **Z** undoes a completed move or cancels an animation. **R** restarts the chamber.
- **H** shows an optional next-move hint. **Escape** pauses and opens the shared menu.
- **F5 / F9** saves / loads through BlueEngine Snapshot, including an in-flight swing,
  chamber progress and the entire undo history. **F** toggles fullscreen.

Blue is a circle, orange a diamond. The pulsing rings identify the current pin. The preview
marks which tip will arrive, and a red marker identifies the first swept obstruction.
The entire baton must clear the solid boxes and the wire chamber limits. Walls between
the camera and baton fade as a visual cutaway; their outlined volumes remain solid.

There is no gravity, timer, move budget or limited life. A blocked request changes nothing.
A blind pocket can leave the new pin with no legal swing: undo or restart immediately.
Undo can also recover earlier planning mistakes. Tutorial: swing **up**, then **right**.

## Build and verify

```text
cargo build
python3 scripts/check.py --skip-ship
python3 ../../tools/be2.py check --game . --loop shipping
```

The project declares **3d**, **offline**, **Windows delivery** and **Linux verification**.
This Linux development host does not certify a Windows executable or installer.
On Windows, run the canonical `python scripts/ship.py ship --no-install`, then the
installer preparation/build/test gates described in the engine's `docs/NATIVE_DELIVERY.md`.
Do not install shortcuts during automated verification.

Native capture and a real public-input route through all eight chambers:

```text
python3 ../../tools/xcapture.py /path/to/pinstride --frames 5,25,70,100,160,250,320,400,460 -- --verify
```

`--verify` chooses legal intentions with the finite-state solver, with four fixed ticks per
rendered frame. It never injects poses, advances rooms without docking, or alters the rules.
`--script "one@10,commit@20,one@60,commit@70,next@110"` uses the same preview/commit controls.
`--input-report` prints accepted requests, snaps and rejections for native-control review.
The capture path uses software rendering and muted sound; physical devices remain a separate
review under `assets/native-review.json`.

## Rules and implementation

`src/lib.rs` is rendering-free fixed-step authority (`Intent`, `Simulation`, exact `Snapshot`).
Resting poses sit on a two-metre 3D lattice. Each pose has exactly four perpendicular swing
candidates; straight translation and half-turns are absent. A conservative angular bound
inflates the continuous swept-segment collision query between samples, including both tips.
Preview and commit share that query. Quarter-turn animation is read from authoritative
swing ticks and hands the anchor over only on arrival.

The authored pose graphs have verified routes of 2, 7, 4, 7, 13, 13, 9 and 17 swings.
Tests cover the anchor handoff, clear endpoints with a blocked arc, rejected requests,
colour-specific docking, a reachable blind-pocket failure, unlimited undo/restart,
all eight public-input solutions, determinism, exact mid-swing saves and invalid-load atomicity.

`src/main.rs` uses BlueEngine's shared native GameShell, ClientInput, Lifecycle, kit materials,
mesh primitives, capture and SoundBank. Original game identity/icon use the fresh canonical
scaffold and icon tool. Liberation Sans is embedded under its bundled font license.
