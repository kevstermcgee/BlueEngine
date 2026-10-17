# Crosshaul status

Implemented eight native 3D chambers, paired axis exchanges, simultaneous travel,
solid baffles and exact swept-box route rejection, matching color/symbol docks,
allowances and failure, undo refunds, restart, progression and final completion.

The native client has mouse picking, cargo-list/number selection, X/Y/Z, Space,
right-drag orbit, wheel zoom, path/destination previews, first-obstruction outlines,
height projections, rules, hints, and engine-owned F5/F9 saves. The canonical map
scaffold, icon tool, executable-resource wiring and installer scripts are retained.

Passed locally:

- 11 tests, including full public-input campaign completion, the required docked
  carrier interaction, both path collisions, failure/retry, deterministic replay,
  exact in-flight save/load, malformed-save rejection, and legal hint generation.
- Standalone inner check and `scripts/check.py --skip-ship` with the native binary.
- `cargo clippy --all-targets -- -D warnings` and formatting.
- Native debug and release builds; Linux development package and icon/manifest
  integrity verification. The generated icon was inspected.

Not verified:

- Native gameplay captures and real mouse/keyboard operation. `xcapture.py` fails
  at XOpenDisplay. A direct Xvfb launch fails to bind its Unix socket; a separate
  Unix-socket probe returns Operation not permitted. No policy was changed.
- Isolated graphical package smoke: shipping skipped it because no display is
  available. The coordinated shipping gate correctly reports incomplete evidence.
- Windows x64 resources, packaged smoke, installer build/test and physical audio.
  Run those gates on Windows; the Linux development package is not a Windows EXE.

Reproduce native review on a display-enabled machine using the controls in README,
`assets/verification_script.txt`, and `assets/native-review.json`. No shortcuts were
installed, and no source was committed, published, or changed outside this game.
