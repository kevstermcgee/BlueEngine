# Corner Clutch verification

Native 2D presentation, Windows x64 delivery and Linux verification are declared.
Edits in this pass stay inside the game. Engine code, templates, policy, other
games and learning-ledger files were not changed. Publication belongs to the
supervisor. No installation or shortcut was performed.

## Selection-order repair

The previous adapter let a later direction overwrite a pending driver selection
in the shared client's single retained action target. The regression reproduced
this: select C, tap Down on another display frame, run the first fixed tick;
A remained selected and moved instead of C.

`src/input.rs` now queues native commands and offers only the oldest command
until a changed simulation tick acknowledges consumption. Display frames with
zero ticks cannot replace it. Multiple ticks consume its action edge once.
Commands from one frame put selection before movement. A mouse selection captures
crate identity on the displayed board and resolves its current cell on dispatch.
Button targets use their centers, keeping deferred edge clicks inside the button.
Restart, room advance and Snapshot restore clear pending device events.
The device queue does not change authoritative state or its hash; all board
changes still happen in Simulation through public Intent. The saved state layout
is unchanged, so Snapshot version two and version-one migration are retained.

Five added tests cover the original failure, mixed zero/one/four-tick schedules,
same-frame commands, repeated Tab, ordered undo, stale pointer coordinates,
coincident cursors, held movement, deferred button edges, deterministic replay,
engine-framed save/load and clearing pending device events on reset/load.
The baseline selection-order assertion failed before queueing and passes after.

## Behavior evidence

`cargo test --locked --no-default-features` passes 21 rule tests plus the identity
test. `cargo clippy --locked --all-targets -- -D warnings` passes. The full game
integration and shipping checks are run with task-bound reports after these notes
are finalized; their stdout is retained in `.blue-check/ordered-final-*.log`.
The shipping report is the authority for which native gates actually passed.
`python3 scripts/check.py --content-only` also passes.

Existing behavioral coverage includes direct versus recursive pickup, middle
versus endpoint drivers, side/passenger blockers, atomic rejection, fresh corner
collection, parked helpers, complete public-input campaign replay, deterministic
hashes and exact Snapshot continuation. A reachable fully blocked arrangement
in room 7 stays recoverable after 3,600 idle ticks and save/load. All 240 moves
in the long-history test remain undoable. There is no terminal loss or penalty.

`python3 tools/solve_rooms.py` independently proves shortest solutions of
2/5/6/8/9/10/12/14 moves. Room 3 needs nine moves without three-crate carrying,
versus six with it. Room 5 cannot be solved while keeping its parked helper fixed;
that restricted search exhausts 47 states.

The native verification route selects drivers, moves via direction buttons,
undoes and advances all eight rooms through ordinary public inputs. Native
keyboard movement uses those same commands. A replay does not certify devices.

## Canonical tools and identity

The absolute engine tools/be2.py start command selected the two-d contract.
Fresh map tooling generated a temporary two-d scaffold inside `.blue-check`.
Its resource wiring, native launchers and tooling bootstrap match this game.
The ship/check/project scripts and installer/review helpers match their canonical
engine templates byte for byte. `.blue-check/ordered-canonical.json` records
these comparisons. The temporary scaffold was removed after comparison.

`map icon "Corner Clutch" ABSOLUTE_GAME/assets 3 --replace` regenerated the full
native icon set. Its PNG was opened and inspected. The existing installer AppId
was preserved.

There is a reproducible engine validation conflict: supervisor validation requires
full `templates/game_*.py` implementations, but fresh scaffolds emit pinned
launchers. `python3 ../../tools/game_tooling.py check .` rejects the full canonical
copy as `scripts/check.py drift`. This game retains the supervisor-required,
verbatim template scripts. No launcher-validation pass is claimed.

`check --changed --plan` proposed the whole engine suite because the new standalone
game is unclassified. This pass uses explicit `check --game` loops; engine inputs
were unchanged, so engine dependency suites are outside this game's repair.

## Native inspection and remaining gates

The retained native 960x540 image at
`.blue-check/smoke-20261015T184331007607/shot_00030.png` was opened and inspected.
The top-down board, marked sockets, gold driver, mint passenger, instruction
panel, direction controls, undo and progress are readable and separate.
This retained image predates the ordering fix.

A fresh xcapture preflight fails before rendering with `XOpenDisplay() failed`.
Direct Xvfb startup on a fresh display also cannot establish listening sockets.
The final packaged-binary capture is attempted separately and its output is kept
in `.blue-check/ordered-final-native.log`. Fresh visual/control inspection,
graphical package smoke and physical audio remain unverified in this sandbox.
Linux packages do not certify a Windows EXE or installer.

On a display-capable host, inspect normal play at 960x540 and 1280x720, especially
rapid click-C then Down, same-frame selection/direction, repeated Tab then Down,
selection after a pending move, button-edge clicks, held keys during undo,
opposing keys, coincident cursor targets, reset, pause and K/L save/load.
The game-owned criteria in `assets/native-review.json` include these cases.

For replay captures from the engine checkout:

```
python3 tools/xcapture.py GAME_DIR/dist/corner-clutch --frames 1,6,12 --size 1280x720 --out /tmp/corner-clutch-captures --sheet -- --verify
```

On Windows run `python scripts/ship.py ship --no-install`, then the canonical
installer preparation/build/test workflow in an isolated account. No desktop
installation or publication is authorized by this task.

## Audio limitation

The shared native client fixes its three cues to Coin/Hit/Success. Movement and
undo use Coin, rejected moves use Hit and completion uses Success, alongside
synchronized crate slides. Its public interface cannot select a mechanical
clack; that needs a checked game-owned one-shot mapping. Engine source was not
changed and audible playback is not claimed.
