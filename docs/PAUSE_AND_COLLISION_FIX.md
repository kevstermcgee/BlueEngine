# Pause menu and dropped-prop fix

Dropping a prop inside a player inserted a collider overlapping their current body.
The horizontal sweep rejected every move and had no overlap recovery. Controller now
looks for the nearest clear horizontal candidate within four metres before movement,
keeps feet height, and checks the recovery path against unrelated colliders. If no safe
candidate exists, it preserves position. This shared fix applies to local play and the
server, including Scientist, Feta and custom profiles.

Regression tests drop an actual held physics prop into both characters and verify
recovery followed by walking. Additional tests cover an adjacent wall, stable feet/no
jitter, and an enclosure where recovery must not cross walls.

Escape now opens a small Resume / Settings / Quit panel. Removed the engine heading,
introductory text, map subtitle and Vesper3D footer. Settings contains sensitivity,
FOV, inversion, hints, position reset and controls. Escape from Settings returns to the
main pause panel; Escape again resumes. Existing cursor capture code is retained.

Validation: full tools/be2.py check passed (154 default-feature Rust tests, 131 headless,
four Python authoring tests, both Clippy/rustdoc configurations, formatting and headless
dependency check). Final settings layout also passed default Clippy and release build.
Inspected release captures in previews/pause-menu. Capture QA exercises the actual
rendered menus; live clicking, physical keyboard layouts and cursor capture were not
manually exercised in this environment.
