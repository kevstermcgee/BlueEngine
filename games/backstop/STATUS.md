# Backstop verification status

Implemented: eight 9x7 rooms, exact rear-clearance shoves, wall/crate backstops, readable
rear/route/landing previews, first-obstruction marking, brief crate slides, docked cargo reuse,
exact deadlock detection, unlimited undo, current-bay retry, room advancement and campaign completion.
There is no timer, move budget or penalty for rejected launches. Deadlocks are recoverable attempts,
with no terminal loss overlay. The game uses the canonical two-d scaffold, Intent, Snapshot and
shared native client. Presentation is 2d; Windows is delivery and Linux is development verification.
No engine or other-game inputs were edited.

Observed checks on Linux:

- `be2.py check --game ... --loop inner`: passed.
- Final `be2.py check --game ... --loop integration`: passed, 20 executed tests across the
  rendering-free and native-feature configurations, including identity validation.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `python3 ../../tools/game_tooling.py check .`: passed; canonical pinned launchers retained.
- `scripts/check.py --skip-ship`: passed content/project/native Rust checks.
- `scripts/ship.py ship --no-install`: built a release executable and passed identity, full
  native icon family, artwork, executable/window wiring and package inventory/hash checks.
  It skipped graphical smoke because there is no display. Consequently the outer shipping
  gate correctly reports incomplete evidence (exit 3), rather than a shipping pass.

`verification_input` completes all eight rooms using ordinary public gameplay, including an
undo/re-shove in bay 1. Tests prove cargo rear stops, all-or-nothing obstruction rejection,
no chain pushing, deadlock recovery, helper reuse and undocking, deterministic replay,
held-key recovery, invalid-state rejection, and exact engine save/load continuation with
animation and undo history. Saves include the room-content fingerprint.

Native review remains **pending**. Attempts with `tools/xcapture.py` failed at XOpenDisplay.
The retained `tools/native_playtest.py` attempt could not start Xvfb: local/unix socket binds
failed. Independent Unix and loopback socket probes both returned Operation not permitted.
No game screenshots, real native-control pass, audible-device pass or packaged graphical smoke
is claimed. Logs from the attempt are retained in ignored `.blue-check/native-controls/`.
The game's canonical generated icon PNG was inspected, and icon validation passed.

On a display-capable Linux host, run:

```
cargo build
python3 tools/native_playtest.py /absolute/path/to/target/debug/backstop
python3 ../../tools/xcapture.py /absolute/path/to/target/debug/backstop --frames 1,9,23,39,52 -- --verify
python3 ../../tools/be2.py check --game . --loop shipping
```

The keyboard script drives the actual native window through X11 key events; it checks
Space, Z, R, K/L, Enter, WASD/arrows, Esc, an intentional deadlock and all eight deliveries.
Inspect its start/preview/undo/deadlock/pause/final captures and the replay captures for text
contrast, ruler clarity, crate/target agreement and sidebar fit. The script has been syntax
checked but cannot execute its native interaction phase in this restricted environment.
Software-display observations do not certify physical controls or audio.

On Windows, run the game's canonical `scripts/ship.py ship --no-install`, then installer
prepare/build/test in an isolated Windows account. These Windows resource, installer and native
smoke gates are pending. Nothing was installed, published, pushed or committed here.

Changed-file planning classified the new game's paths as unknown and proposed unrelated full
engine gates. Explicit standalone `--game` loops were used; the engine remained read-only.
