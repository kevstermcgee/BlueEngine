# Corkscrew Key

A native 3D spatial puzzle: every quarter-turn of a five-cube key advances its center exactly one grid unit along the same **fixed world axis**. Translation and rotation are inseparable. The orange two-cube arm, teal arm, and violet arm remain one rigid object.

Match the gold socket's complete position **and orientation** in each of six chambers. Green outlines and trails show a legal screw step; red marks its first obstruction. All five cubes must clear every obstacle throughout the turn, including ceilings and vertically offset passages. A clear destination alone is insufficient. A subtle floor beacon helps distinguish translation from rotation.

There is no timer, move budget, permanent loss, or hazard damage. Blocked moves leave the pose and undo history untouched. Experiment, undo, or restart as often as you like.

| Control | Action |
| --- | --- |
| 1 / 2 / 3 | Select fixed world X / Y / Z |
| Right / Left | Preview positive / negative quarter-turn and one-unit advance |
| Space | Execute; after docking, open the next chamber |
| Z | Undo one successful move, including a completed docking |
| R | Retry the current chamber; after the finale, replay from chamber one |
| Right mouse drag | Orbit the camera without changing movement axes |
| Scroll | Zoom |
| H | Toggle an optional next-step hint; off the suggested route, undo or retry |
| K / L | Save / load using BlueEngine Snapshot storage |
| Esc | Pause |

Buttons provide the same public actions as the keyboard. A positive turn follows the right-hand rule about the positive world axis; Left reverses both turning and travel. X is orange, Y teal, Z violet. A turn animates for 0.3 seconds. Wait for it to settle before committing the next step. Selecting an axis or sign does not move the key.

Chambers teach a single step, rotation order, swept clearance under a beam, a vertical detour, a closed loop that changes orientation, and a final vault requiring vertical movement. Hints are optional; no route costs a resource. Returning to the starting center does not necessarily restore the starting orientation.

## Development and delivery

This game uses game-owned, rendering-free `Simulation<Input = Intent>` at 60 Hz, BlueEngine `Snapshot`, and the shared portable native client. The three-d starter provides a real perspective 3D world; every axis matters to collision and docking. Camera state is presentation-only and excluded from saves and authority hashes.

Poses contain integer centers and proper cube orientations (24 possibilities). Screw collision uses fixed-point SAT and recursively bounded swept intervals, not destination-only or unbounded float sampling. The smallest uncertain interval is 1/1024 of a step; paths with less than roughly 0.003 grid units of unresolved clearance are conservatively rejected. Preview and committed rules use the same check. Authored routes leave enough clearance and are verified by a small state-space search.

Windows x64 EXE installers are the delivery target. Linux is declared for development, headless tests, native captures, and isolated package verification. A Linux check does not certify Windows executable resources or a Windows installer.

From the engine checkout:

```sh
python3 tools/be2.py check --game games/corkscrew-key --loop inner
python3 tools/be2.py check --game games/corkscrew-key --loop integration
python3 tools/be2.py check --game games/corkscrew-key --loop shipping
cargo run --manifest-path games/corkscrew-key/Cargo.toml
```

On Windows, from the game directory, `python scripts/ship.py ship --no-install` builds and checks the complete native package and isolated smoke without installing a shortcut. Shipping tooling embeds the game's title, icon, and resources. Do not publish a browser build.

Tests cover all six public-input solutions, rotation order and the closed loop, 24 orientations, inverse steps, swept collision with clear endpoints, harmless rejection, undo/retry, deterministic replay, exact mid-animation save continuation, invalid snapshots, and a search proving the final socket cannot be reached without Y-axis movement.
