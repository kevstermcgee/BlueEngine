# Gap Flip verification status

Revalidated on 2026-10-08 against the current engine checkout. The ten-room game retains corridor reflection, exact ghost previews, animated dotted-gap exchange, matching letter/color goals, unlimited undo and current-room restart. There is no terminal loss, timer or move budget. Authoritative integer rules use fixed-step Intent input and engine Snapshot storage; the shared native client owns devices and lifecycle.

This revalidation regenerated the complete title-seeded icon set through fresh `python3 tools/be2.py map icon "Gap Flip" games/gap-flip/assets --replace` tooling and added exhaustive two-token corridor regression coverage. It preserves the authored mechanic and rooms.

## Current passing evidence

- `python3 tools/be2.py check --game games/gap-flip --loop integration`: 16 test executions, comprising seven rule tests and one identity test in each feature mode; formatting also passes.
- `cargo clippy --locked --manifest-path games/gap-flip/Cargo.toml --all-targets -- -D warnings`: passes.
- Shipping's `scripts/check.py --skip-ship`: passes with `BE2_TOOLS` set to the fresh canonical `itest/be2-tools` path. Without that override the generated checker fails to discover tools in this configured shared target directory.
- Shipping rebuilt the Linux release package in `dist/`. Identity, icon formats/art, executable/window wiring and the complete three-file package manifest pass integrity checks.

The behavioral tests verify shortest solutions of 1, 4, 5, 8, 9, 10, 11, 12, 14 and 16 flips, and reject independent-token solutions for rooms 3-10. Room 4 requires its initially matched helper A to leave its goal. Across every two-token floor placement on the authored boards, flips preserve the axis, avoid the other token, exchange gap lengths and reverse exactly.

The public click route completes all ten rooms and includes undo/recommit. Deterministic Snapshot replay covers selection, preview, animation, undo history, room transitions and final completion. Invalid clicks, disabled equal-gap flips, malformed saves, unlimited undo and current-room restart are covered without introducing a loss state.

The required checkout-wide `python3 tools/be2.py check --changed` also ran. It did **not** pass: the engine library had 622 passes and one hub registry fixture failure (`Text file busy`); that exact test passed on an immediate isolated rerun. The independent Python batch had one unrelated supervisor fixture error from its native-compilation free-space guard. The full report is retained under `.be2-work/check-20261008T141030285266Z-sf_1mkyl/`. No engine or supervisor implementation was changed, and the isolated retry does not certify the full gate.

## Remaining native gates

The shipping task reports **incomplete_shipping_evidence**, because isolated package smoke is skipped without a display. This is not a complete shipping pass. Windows executable resources, x64 installer generation and Windows isolated smoke require the native Windows gate.

Both revalidation attempts to inspect the running native client failed before rendering: the real XTest control script reports Xvfb display-listener bind failures, and canonical `tools/xcapture.py` against the freshly rebuilt package reports `XOpenDisplay() failed`. The failed input report is retained under `.blue-check/native-controls-revalidation/`. No passing current native-input, visual or hardware evidence is claimed.

Retained supervisor captures from the earlier build were inspected for board layout, previews, helper tokens and legibility. The regenerated icon PNG was inspected. Those earlier captures do not certify this revision. New-player landing prediction, deliberate helper use and enjoyment remain untested.

On a Linux host permitting local X11 sockets, rerun:

```sh
python3 games/gap-flip/scripts/native_controls.py games/gap-flip/dist/gap-flip --out games/gap-flip/.blue-check/native-controls-fresh
BLUEENGINE_DATA_DIR=/tmp/gap-flip-capture-storage python3 tools/xcapture.py games/gap-flip/dist/gap-flip --frames 2,12,28,44,62,78,90 --size 1280x720 --out /tmp/gap-flip-campaign -- --verify --mute
```

The isolated XTest route exercises real native mouse/keyboard events: selection, equal gaps, preview/commit, undo, R restart, K/L save/load, pause, room advance, both axes around a wall and changing helper boundaries. It produces PNGs and reads checksummed engine Snapshot outputs. It does not install shortcuts or target a user's desktop.

For the required Windows delivery gate, run from this game on Windows:

```sh
python scripts/ship.py ship --no-install
```

Linux is declared for verification; delivery remains a native Windows x64 EXE installer. No publication, shortcut installation or human playtest is claimed.
