# Verification status

Peelturn is implemented as an offline native 2D game. Windows x64 is the delivery target; Linux is declared for development verification. The engine and other games remain unchanged.

All 11 tests (10 rule tests and the identity test) pass. The rendering-free test suite exercises clockwise deposits, the sixth impact stopping on the first deposit, permanent beads steering a later packet, necessary launch order, reopening a launched packet's original cell, eight-bead routes, initially rejected directions, all three loss conditions, unlimited undo/retry, deterministic public replay, engine Snapshot save-frame continuation, and invalid-state rejection. The public verification route solves all twelve boards by selecting, aiming and launching all 29 packets. It does not set the outcome or modify a board directly.

Native development/release compilation, formatting, game identity/icon validation, and Linux package integrity pass. The canonical shipping gate reports incomplete evidence: isolated smoke is skipped because no display is available, and Windows executable-resource/installer verification cannot run on this Linux host. This is not a shipping pass.

The reported game-clippy failure is resolved: the board-clear input check uses
`Option::is_none_or` instead of `map_or(true, ...)`. On 2026-10-17,
`cargo clippy --locked --all-targets --all-features -- -D warnings` and
`cargo fmt --all -- --check` passed. The task-bound inner gate passed all 11 tests;
the shipping gate passed project checks and rebuilt the Linux package, then reported
`incomplete_shipping_evidence` because isolated native smoke was skipped.
Fresh canonical `map new-game ... two-d` and `map icon ... 0` outputs were generated
in temporary directories for comparison. The existing game retains canonical check,
ship, tooling bootstrap and Windows resource code, plus its own copper icon.

## Capture and device limitation

Native visual inspection and actual key/mouse exercise are **pending**. Running the engine's `tools/xcapture.py` against the compiled game failed with `XOpenDisplay() failed`. Direct Xvfb diagnostics showed that this sandbox forbids binding display sockets (`Operation not permitted` for an AF_UNIX bind); it also has no supplied DISPLAY. No native screenshot or physical/audio pass is claimed. The authored `assets/native-review.json` lists the checks still needed on an unrestricted display host.

From an engine checkout with display access, capture the initial board with:

```
python3 tools/xcapture.py /path/to/peelturn --frames 1,25 --size 1280x720 --out /tmp/peelturn-initial
```

To inspect the real public-input route, append `-- --verify` and capture frames through 367. Inspect the initial board and impact animation at 800x450 and 1280x720, then use actual Tab/arrows/Space/Backspace/R/H/Escape/K/L and mouse selection. On board 3, launching A right must jam; undo, launch B right, then A left to clear. K/L must resume both an animated route and its undo history. Verify focus changes separately in the shared client.

The changed-file planner classifies this new standalone game as unknown and proposes the full engine suite. Explicit `check --game` inner/integration/shipping gates were used because all source edits are confined to this game; no engine dependency behavior changed.

Windows completion still requires native package smoke, executable resources, installer gates and physical review. No install, shortcut, commit, publication or deployment was performed.
