# Scatterlock: AI development feedback

Run: `e8fc229b`.
Project: `games/scatterlock`. State: running.

## Generated mechanic

Choose a beacon, then double or halve every loose cube’s distance from it simultaneously. Cubes keep their size. Changing the beacon between pulses turns spreading and gathering into precise transportation; delivering a cube removes it from the moving formation but creates a new obstacle.

## Novelty and playtesting

The closest thematic comparison is Maquette’s manipulation of spatial scale through a recursive world ([official description](https://www.annapurna.com/interactive/maquette)). Scatterlock instead scales only the positions of a shared cargo formation around selectable origins; objects and architecture retain their dimensions. Unlike the prior Perspective Pocket, there is no silhouette fitting, perspective-dependent resizing or depth-for-width exchange. Unlike Crosshaul, no coordinates are exchanged between selected partners: every loose cube undergoes the same spatial transformation, and docking progressively changes the affected group. This is a concrete mechanical distinction, not certification of worldwide originality or fun.

Players may experience the transformation as arithmetic rather than spatial reasoning, or find simultaneous paths difficult to read. The first prototype must establish a useful maneuver through animation and destination previews without requiring coordinate calculations. If delivery order adds bookkeeping without a satisfying discovery, the mechanic needs revision before adding chambers.

## Findings from the AI that developed and reviewed the game

## Measured game-run cost

Tokens: 8857810; wall seconds: 1405.85; Codex seconds: 1372.094. Measured from Codex CLI turn.completed usage; cached input included once.

### 1. tooling (medium)

Observed: Canonical scaffolding rejected the destination because protected runner metadata made the otherwise empty game folder nonempty.

Reproduce: Run map new-game against a destination containing only protected runner metadata directories.

Workaround: Generated into a temporary child folder, moved the scaffold into place, and corrected relative engine paths.

Proposed engine improvement: Support initialization beside recognized protected metadata without overwriting existing game files.

### 2. tooling (low)

Observed: xcapture failed at XOpenDisplay without retaining the underlying Xvfb startup diagnostics.

Reproduce: Run tools/xcapture.py against the native binary in this restricted environment.

Workaround: Used direct xvfb-run with retained display logs for captures, native input review, and package smoke.

Proposed engine improvement: Probe display connectivity before launch and retain Xvfb stderr in capture failure reports.

### 3. audio (medium)

Observed: The native audio backend initialized during muted runs and its worker panicked with an ALSA rate-setting error, although rendering continued.

Reproduce: Launch the native client under Xvfb with --mute using this host's default ALSA device.

Workaround: Configured a null ALSA device for software control review and package smoke. Physical audio remains unverified.

Proposed engine improvement: Avoid opening audio hardware for muted unattended runs, or report backend initialization failures without a worker panic.

### 4. input (medium)

Observed: Lifecycle preserves edge flags but replaces held data each frame. Scatterlock’s frame-local beacon payload therefore changed a pending selection from B to A.

Reproduce: Select B on a zero-tick frame, feed an idle frame, then consume the next fixed tick. The previous client selected A.

Workaround: Persist the latest beacon payload in the game-owned Controls adapter. Apply it only when the selection edge is consumed. Regression tests now pass.

Proposed engine improvement: Add a targeted-selection example beside Lifecycle::feed showing how edge payloads survive zero-tick frames. Existing engine semantics are documented.

### 5. tooling (low)

Observed: Invoking be2.py map icon from the game directory with relative output assets resolved against the engine checkout and failed on its read-only assets directory.

Reproduce: From games/scatterlock, invoke the absolute be2.py path with map icon Scatterlock assets --replace.

Workaround: Pass the absolute game assets directory. Canonical icon generation then succeeds.

Proposed engine improvement: Resolve authoring output paths relative to the caller, or clearly display the resolved destination before writing.

### 6. input (high)

Observed: src/main.rs creates Held::default() every rendered frame. SELECT edges survive until a fixed tick, but InputAccumulator::feed replaces their beacon payload each frame. A pending selection of B–E therefore becomes A after an intervening frame.

Reproduce: Feed Held { beacon: 1 } with SELECT, run no tick, then feed default Held with no edges. take_tick() returns SELECT with beacon 0. This timing occurs above the 60 Hz simulation rate; fixed-frame scripted evidence misses it.

Workaround: Use Tab to cycle beacons.

Proposed engine improvement: Preserve the selection payload until consumption or encode beacon identity in edge bits. Verify clicks and number keys across frames without ticks.

### 7. ui (low)

Observed: Chambers four and six place B and D at identical coordinates. Their badges overlap, and the click loop selects the later index.

Reproduce: Inspect common() in src/lib.rs and the final campaign capture; click the shared B/D badge.

Workaround: Use sidebar selection or Tab; both origins currently produce identical transformations.

Proposed engine improvement: Deduplicate the coincident beacon and update routes, or provide distinct readable selection targets.

### 8. audio (medium)

Observed: Engine/backend friction: the isolated Linux smoke log contains an ALSA worker panic, 'Can't set rate.', despite completed rendering and a successful process exit. Muted review runs also encountered this initialization issue.

Reproduce: Launch the native client on an ALSA device that rejects the requested sample rate, including with --mute.

Workaround: The software-control review supplies a child-process null ALSA configuration. This does not verify audible playback.

Proposed engine improvement: Handle unavailable audio devices gracefully and expose backend initialization failure in audio status; avoid device initialization for silent runs where feasible.

### 9. ui (low)

Observed: Small cargo labels, beacon badges and destination/socket wireframes overlap in the default camera view. Empty sockets rely primarily on color for matching.

Reproduce: Inspect the initial control capture and attached secured-chamber capture at their native resolutions; compare the clustered amber and cyan cargo, badges and cages.

Workaround: Orbit and zoom the camera; use sidebar beacon selection.

Proposed engine improvement: Label sockets with matching cargo numbers and improve label separation and preview contrast.

### 10. audio (medium)

Observed: Engine/backend friction: the successful isolated-package smoke log contains an audio-thread panic reporting an unsupported ALSA rate. Rendering and puzzle completion continued, but this run does not establish working sound.

Reproduce: Launch the native package on a displayless Linux host whose ALSA device rejects the backend's requested sample rate; inspect stderr even when muted.

Workaround: The native-control verification script configures an ALSA null device for software evidence. Physical playback still requires listening on supported hardware.

Proposed engine improvement: Make audio initialization fail gracefully and avoid opening the audio backend for muted captures.

## Verification and delivery

Agent claims are separate from supervisor-executed checks. Virtual displays and
software rendering do not certify physical devices or audible hardware output.

- game-format: exit 0, 0.114 seconds.
- game-tests: exit 0, 0.215 seconds.
- game-clippy: exit 0, 0.165 seconds.
- native-package: exit 0, 9.165 seconds.
- game-format: exit 0, 0.166 seconds.
- game-tests: exit 0, 0.375 seconds.
- game-clippy: exit 0, 1.405 seconds.
- native-package: exit 0, 19.788 seconds.

## Supervisor-observed failed attempts

- build: Independent review blocked completion: Click and number-key beacon selection can select beacon A instead of the requested beacon when the input arrives on a rendered frame without a simulation tick.

Detailed command logs, AI responses and package captures stay in the ignored
run directory. This file contains authored findings and checked delivery receipts.
