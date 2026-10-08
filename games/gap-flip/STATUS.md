# Gap Flip verification status

Implemented ten native rooms with corridor reflection, exact previews, animated dotted-gap exchange, matching letter/color goals, unlimited undo, current-room restart, progress, instructions, a title-seeded canonical icon, and engine Snapshot saving. Simulation is fixed-step and rendering-free; mouse intentions use the shared native client.

Passed on Linux:

- `cargo clippy --locked --manifest-path games/gap-flip/Cargo.toml --all-targets -- -D warnings`: passed after replacing the eight-argument dotted-band helper with the existing `Corridor` value. The reported `game-clippy` failure is resolved.
- `python3 tools/be2.py check --game games/gap-flip --loop inner`: six rule tests plus identity/icon agreement.
- `python3 tools/be2.py check --game games/gap-flip --loop integration`: headless and client-feature suites plus formatting.
- Shipping stage `scripts/check.py --skip-ship`, with `BE2_TOOLS` pointing to the fresh canonical `itest/be2-tools`.
- Native Linux release compilation and package integrity: `dist/gap-flip`, its own icon assets, title/icon wiring, and the three-file package manifest passed.

The final `be2.py` shipping gate correctly returned **incomplete_shipping_evidence** because isolated native smoke was skipped without a display. This is not a complete shipping pass; the package remains available for the supervisor to inspect and smoke-test.

The follow-up integration check passed all 14 test executions (six rule tests and one identity test in each feature mode), and shipping rebuilt the Linux release package with the Clippy fix. Repeated native input and canonical capture attempts still failed before rendering: the isolated Xvfb log reports display-listener bind failures; `xcapture.py` reports `XOpenDisplay() failed`. The retained input attempt is in `.blue-check/native-controls-recheck/`. These failures provide no passing native-input or visual evidence.

The breadth-first rule test verifies shortest solutions of 1, 4, 5, 8, 9, 10, 11, 12, 14, and 16 flips, and rejects independent-token routes for rooms 3-10. Room 4 cannot be solved if its initially matched helper A stays fixed. The public click route completes all ten rooms, includes undo/recommit, and deterministically resumes exact snapshots through selection, preview, animation, history, room transitions, and final completion. Invalid clicks, equal gaps, malformed saves, full undo, and current-room restart are covered without adding a loss state.

Native visual/device-input validation is **unverified**. Both the canonical `tools/xcapture.py` attempt and `scripts/native_controls.py` failed before game rendering because this restricted environment prevents Xvfb from binding its display socket. No successful native captures or human playtest are claimed. The icon PNG was inspected. The visual design remains pending native review for contrast, layout, motion, and new-player understanding.

Run the retained real-device-event check on a Linux host permitting local X11 sockets:

```sh
python3 games/gap-flip/scripts/native_controls.py PATH_TO_GAP_FLIP_BINARY --out games/gap-flip/.blue-check/native-controls
```

This isolated XTest check selects tokens, verifies disabled equal gaps and non-mutating previews, commits ghosts, undoes completion, restarts, saves/loads pending previews, pauses, advances rooms, uses both axes around a wall, repositions helper boundaries, and undoes a full solution. It produces PNGs and reads checksummed engine Snapshot outputs. It does not write custom save files or target a user's desktop.

For native campaign captures:

```sh
BLUEENGINE_DATA_DIR=/tmp/gap-flip-capture-storage python3 tools/xcapture.py PATH_TO_GAP_FLIP_BINARY --frames 2,12,28,44,62,78,90 --size 1280x720 --out /tmp/gap-flip-campaign -- --verify --mute
```

Windows x64 installer/resource/isolated-smoke validation requires the native Windows gate, `python scripts/ship.py ship --no-install`, from this game. Linux verification does not certify Windows delivery. No shortcuts, publication, or installation are requested.
