# Stable Feta follow camera

The old camera placed Feta's eye 35 cm above a 1.25 m pitched boom, swept an
18 cm radius against nearby objects, and hid the character below 55 cm clearance.
The boom could hit furniture even when Feta's body comfortably fitted underneath.

Feta now uses a centered 1.05 m boom, a 6 cm height offset and 7 cm collision buffer.
Look pitch changes the view direction, not boom height. Obstruction distance no longer
changes the viewing angle, and the body-hide threshold matches the smaller character.
Walls/actual obstructions still shorten the boom for clipping protection.

CameraRig retracts immediately and eases outward exponentially as space clears. Only
boom distance is smoothed: movement-follow and mouse look remain direct. Local aiming
and rendering use the same rig policy, with a fresh collision check for each pose.
First-person placement remains unchanged; Scientist retains its shoulder composition
and receives the same smooth obstruction release.

Regression coverage walks Feta through chairs, tables and desks with static/physical
collision and pitches -1.2, 0 and +1.2 radians. It asserts constant camera offset,
unchanged viewing angle and visible character. Wall tests verify immediate protection,
gradual release at 30/60/144 Hz, and collision changes between rig update and rendering.
The full check suite passed 156 default-feature Rust tests, 133 headless Rust tests,
four Python authoring tests, Clippy/rustdoc in both configurations and headless checks.

Visual QA uses the actual camera and movement path through previews/feta-camera/table.json:
`be2 --map previews/feta-camera/table.json --capture-camera NEW_DIRECTORY`.
Inspected approach, under-table, exit and pause captures. This scripted smoke does not
claim manual playtesting or coverage of every authored nook; solid walls still require
camera retraction. No movement, input bindings, physics or network protocol changed.
