# Backstop: AI development feedback

Run: `2cddb70b`.
Project: `games/backstop`. State: published.

## Generated mechanic

Every shove moves a crate exactly one cell plus the number of consecutive clear cells behind you. Walls and other crates end that measurement. To control where cargo lands, arrange the space behind the pusher.

## Novelty and playtesting

The closest comparison is Sokoban's push-only, crate-to-goal structure ([rules](https://webdocs.cs.ualberta.ca/~games/Sokoban/thegame.html)). Backstop replaces ordinary shoves with exact distances calculated from rear clearance, making cargo placement also configure future movement ranges. Unlike the prior Afterpull mechanic, walking never attracts remote crates or activates vacated cells. Unlike Borrowed Falls, distance is neither captured nor transferred. This is a concrete mechanical distinction, not certification of worldwide originality or fun.

The rule could become tedious counting or frequent rejected pushes. Persistent ruler previews must make distance immediately readable, and rooms must reward backstop reuse and delivery order rather than repeated arithmetic.

## Findings from the AI that developed and reviewed the game

## Measured game-run cost

Tokens: 4341827; wall seconds: 5935.624; Codex seconds: 977.223. Measured from Codex CLI turn.completed usage; cached input included once.

### 1. platform (medium)

Observed: Xvfb could not bind display sockets; XOpenDisplay failed. Unix and loopback socket probes returned Operation not permitted, blocking native captures, keyboard inspection and graphical package smoke.

Reproduce: Run tools/xcapture.py against the native binary, or run the game's tools/native_playtest.py in this sandbox.

Workaround: Preserved the checked Linux package and a repeatable keyboard/capture script. Documented pending native review in STATUS.md.

Proposed engine improvement: Add display/socket readiness probes and preserve Xvfb diagnostics; perform graphical review on a display-capable worker.

### 2. tooling (low)

Observed: Start rejected the designated scaffold destination as nonempty when it contained only runner metadata directories.

Reproduce: Run new-game start against the freshly prepared game directory.

Workaround: Generated the canonical scaffold in a child directory, moved generated files into the game root and corrected relative engine/tooling paths.

Proposed engine improvement: Allow scaffolding into metadata-only directories after checking generated paths for collisions.

### 3. workflow (low)

Observed: Changed-file planning classified the new standalone game as unknown input and proposed full engine and unrelated-game checks.

Reproduce: Run be2.py check --changed --plan after scaffolding a standalone game.

Workaround: Used explicit --game inner, integration and shipping checks. Engine inputs remained unchanged.

Proposed engine improvement: Recognize new standalone game ownership from game.project.json and Cargo.toml before selecting full engine checks.

### 4. audio (medium)

Observed: Engine friction: the retained native smoke log contains an ALSA backend thread panic, "Can't set rate." Rendering completed, but smoke success does not establish functioning audio.

Reproduce: Launch the native package on a Linux audio device that rejects the requested sample rate; inspect stderr alongside the capture.

Workaround: Use a compatible audio-device configuration and keep this host's audio evidence marked unverified.

Proposed engine improvement: Handle audio initialization failure gracefully and surface backend failure separately from graphical smoke success.

### 5. docs (low)

Observed: STATUS.md still says no screenshots or packaged graphical smoke exist, although the supplied capture and current receipt establish both.

Reproduce: Compare STATUS.md lines 19–34 with dist/ship.json and the retained smoke capture.

Workaround: Use the current receipt and capture for local graphical evidence; retain the separate keyboard-playthrough and physical-audio limitations.

Proposed engine improvement: Refresh STATUS.md to reflect the newer native evidence.

## Verification and delivery

Agent claims are separate from supervisor-executed checks. Virtual displays and
software rendering do not certify physical devices or audible hardware output.

- game-format: exit 0, 0.064 seconds.
- game-tests: exit 0, 0.966 seconds.
- game-clippy: exit 0, 0.165 seconds.
- native-package: exit 0, 1.368 seconds.
- generated-game-linux-windows: https://github.com/kevstermcgee/BlueEngine/actions/runs/39070989511 (exact source `8d378f609bb1`).
- engine-checks: https://github.com/kevstermcgee/BlueEngine/actions/runs/39070990049 (exact source `8d378f609bb1`).
- windows-installer-review: https://github.com/kevstermcgee/BlueEngineGames/actions/runs/39073793514 (exact source `2dcea90e4202`).
- production-release: https://github.com/kevstermcgee/BlueEngineGames/actions/runs/39074053305 (exact source `2dcea90e4202`).
- download-page: https://github.com/kevstermcgee/BlueEngineGames/actions/runs/39076561431 (exact source `2dcea90e4202`).

Published game: https://kevstermcgee.github.io/BlueEngineGames/games/backstop/
Immutable release: https://github.com/kevstermcgee/BlueEngineGames/releases/tag/blueengine-8d378f609bb1-games-2dcea90e4202-39074053305-1
Installer SHA-256: `e34b61e61889cb8a525c88574b53500149c3ff62eadfc9c583dc3978918226b1`.
ZIP SHA-256: `70903b61461fd49e81491fe57df8dc0fc95174c91cfdc81ed7a43bd8a9492b7d`.

Detailed command logs, AI responses and package captures stay in the ignored
run directory. This file contains authored findings and checked delivery receipts.
