# Odd Matter: AI development feedback

Run: `427fab3d`.
Project: `games/odd-matter`. State: running.

## Generated mechanic

Solid volumes cancel wherever they overlap: one layer is a wall, two layers make a traversable cavity, and three become solid again. Slide matter through a vault to move empty passages around a small drone—including the passage it currently occupies.

## Novelty and playtesting

Closest found: [Geometry Overlap](https://www.y8.com/games/geometry_overlap), where overlapping flat shapes cancel while matching a target picture. Here cancellation creates occupied, traversable 3D cavities that must be relocated safely during navigation. Unlike prior Inside-Out Key, nothing swaps between a container and surrounding space; unlike Color Custody, solidity depends on local overlap counts rather than transferred tokens. Worldwide originality and fun remain unverified.

Interior occlusion and counting overlapping volumes may obscure the rule. Keep chambers small, provide cutaways and outcome previews, and establish two-layer cancellation before introducing restored solidity.

## Findings from the AI that developed and reviewed the game

### 1. tooling (medium)

Observed: The fresh custom-sim scaffold omitted game.project.json and scripts/project.py, requiring manual dimension and target declarations.

Reproduce: Run tools/be2.py map new-game NAME DIR . custom-sim in a fresh directory; inspect its outputs.

Workaround: Added 3D Windows/Linux project metadata and copied the canonical project validator into the game.

Proposed engine improvement: Generate project metadata and its validator for custom-sim, preserving the start packet's presentation and targets.

### 2. tooling (medium)

Observed: The generated checker reported 'No be2-tools found' after canonical map tooling built the tool in itest. Discovery omitted that profile.

Reproduce: Run tools/be2.py map describe, then the generated game's scripts/check.py --content-only without --tools.

Workaround: Initially passed --tools explicitly, then added itest first in this game's checker discovery. Default checks passed.

Proposed engine improvement: Include itest in canonical tool discovery and prefer the fresh tool produced by the map workflow.

### 3. workflow (low)

Observed: Shipping regenerated package artifacts and invalidated earlier passing test records despite unchanged source fingerprints.

Reproduce: Run task-bound inner and integration checks, then shipping; refresh the task's evidence.

Workaround: Reran source checks after shipping, waiting for each check to finish. All three recorded phases passed.

Proposed engine improvement: Compare stage-relevant inputs: source/config/tool fingerprints for tests and package artifacts for shipping.

### 4. tooling (medium)

Observed: Fresh map tooling builds be2-tools in itest, but the canonical game checker does not search that profile. A content check failed with 'No be2-tools found' immediately after a successful map build.

Reproduce: Run tools/be2.py map describe with only itest/be2-tools built, then run the game's scripts/check.py --content-only without BE2_TOOLS.

Workaround: Pass --tools or set BE2_TOOLS to the fresh itest executable while retaining unchanged canonical scripts.

Proposed engine improvement: Add itest discovery to templates/game_check.py and test it with a shared target directory and fresh custom-sim scaffold.

### 5. tooling (low)

Observed: The shipping coordinator classified XOpenDisplay() startup failures as test_failure and suggested inspecting a gameplay assertion. Direct canonical shipping under its own virtual display passed.

Reproduce: On this Linux host, compare be2.py check --game games/odd-matter --loop shipping under xvfb-run with direct scripts/ship.py ship --no-install under a dedicated virtual display.

Workaround: Run the canonical ship script directly under a dedicated virtual display and inspect its isolated package captures.

Proposed engine improvement: Classify display startup failures separately and provide display-readiness diagnostics and native capture recovery instructions.

### 6. audio (medium)

Observed: Native package smoke and real-control logs contain an ALSA backend worker panic, "Can't set rate", despite muted execution. Gameplay and captures complete successfully. This is host audio/backend friction, not a demonstrated portability blocker.

Reproduce: Launch the audio-enabled native client with --mute on the reviewed Linux virtual-display host; backend initialization still attempts ALSA setup.

Workaround: Use these runs as visual/input evidence. Perform separate --audible verification with a functioning audio device.

Proposed engine improvement: Support device-free muted startup and expose backend initialization failures through the shared audio status.

### 7. tooling (low)

Observed: Canonical native check tooling discovery omits the engine's itest profile. With only fresh itest tooling present, automatic discovery returns no executable. The README documents the workaround.

Reproduce: Build be2-tools in the shared target's itest profile, unset BE2_TOOLS, and invoke canonical find_tools discovery.

Workaround: Set BE2_TOOLS to the itest executable or pass --tools PATH.

Proposed engine improvement: Update canonical template discovery to recognize the supported itest profile while preserving freshness checks.

## Verification and delivery

Agent claims are separate from supervisor-executed checks. Virtual displays and
software rendering do not certify physical devices or audible hardware output.

- game-format: exit 0, 0.115 seconds.
- game-tests: exit 0, 0.343 seconds.
- game-clippy: exit 0, 0.265 seconds.
- native-package: exit 0, 7.77 seconds.

## Supervisor-observed failed attempts

- build-and-review: Insufficient free space for native compilation; choose a larger target disk
- build-and-review: Insufficient free space for native compilation; choose a larger target disk
- build-and-review: Insufficient free space for native compilation; choose a larger target disk
- build-and-review: Insufficient free space for native compilation; choose a larger target disk
- build-and-review: Insufficient free space for native compilation; choose a larger target disk
- build-and-review: Insufficient free space for native compilation; choose a larger target disk
- build-and-review: Insufficient free space for native compilation; choose a larger target disk
- build-and-review: Insufficient free space for native compilation; choose a larger target disk
- build-and-review: Insufficient free space for native compilation; choose a larger target disk
- build-and-review: Insufficient free space for native compilation; choose a larger target disk
- build-and-review: Insufficient free space for native compilation; choose a larger target disk
- build-and-review: Insufficient free space for native compilation; choose a larger target disk
- build-and-review: Insufficient free space for native compilation; choose a larger target disk
- build: Native packaging scripts must come from the fresh canonical engine templates

Detailed command logs, AI responses and package captures stay in the ignored
run directory. This file contains authored findings and checked delivery receipts.
