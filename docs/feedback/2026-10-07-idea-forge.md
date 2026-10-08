# Idea Forge: development feedback for BlueEngine

Date: 2026-10-07. Project: `games/idea-forge`. Runtime: portable, 2D, offline.
Delivery target: browser on BlueEngineGames. Native executables are not a declared
delivery target; the rendering-free CLI is available from source.

## Product and mechanic quality

The utility generates briefs from a seeded, shuffled catalog of 36 distinct
mechanics, equally divided among puzzle, action and strategy. Every brief has a
specific player action and rule, play loop, explanation of the intended fun,
minimal prototype, design risk and optional story. The catalog contains no
cosmetic variants counted as new mechanics. Generation excludes every previously
generated ID, even after genre changes and save/load. Exhaustion is explicit.

This finite offline catalog does not establish worldwide novelty or prove fun.
Each generated idea's risk is a concrete playtest question; prototype the core
interaction before expanding it. Example: Borrowed Falls conserves falling
distance; its first prototype should prove that players understand the ruler and
discover upward movement by themselves. More authored mechanics can be added in
a versioned update; preserve saved ID/index compatibility or supply a migration.

## Development issues and proposed improvements

### IF-01: keyboard/controller actions inherited a hovered pointer

Severity: high for button interfaces. Status: addressed with an opt-in engine API.

Evidence: `src/two_d/client.rs` constructed every Intent with the current mouse
position, including Space and controller A. A tool could not distinguish a click
on a button from a keyboard activation with the cursor hovering over that button.
This made full keyboard navigation unreliable without application heuristics.

Resolution: `GameLogic::pointer_target_only_on_press()` defaults to false, keeping
continuous aiming/paddle input compatible. Idea Forge opts in. The shared client
retains a pointer target only for a mouse press or canvas tap. The mobile action
button clears its old canvas target before setting its action edge. A focused
button then receives Space/A and the panel action. Simulation still consumes only
Intent and never reads device APIs.

Regression: engine `pointer_action_tests`, game keyboard/pointer tests, and
desktop controller plus independent touch browser verification. Future refinement:
an explicit device-source field would also support accessible text/UI widgets.

### IF-02: relative scaffold engine path silently produced a wrong dependency

Severity: medium. Status: workaround; engine validation remains open.

Reproduction from an engine checkout:
`be2.py map new-game idea-forge games/idea-forge . two-d`.
The command succeeded but generated `vesper3d.path = "."`, interpreted by Cargo
relative to the game, not the authoring command's working directory. The first
test reported no matching `be2` package in the game directory.

Workaround: corrected the game's dependency to `../..`. Recommendation: resolve
the provided engine path in the command's directory, verify its package identity,
then write a path relative to the output game. Add nested-output and absolute/
relative-path fixtures. Error before creating output if the engine cannot resolve.

### IF-03: an isolated worktree does not inherit browser test dependencies

Severity: low. Status: resolved with the documented preparation flow.

The initial `be2.py start` readiness packet found Node and Chromium but reported
`ERR_MODULE_NOT_FOUND` for `ws`. Other worktrees had their own node_modules.
`npm ci --ignore-scripts --prefix tools` and `web prepare` resolved this checkout's
prerequisites. The readiness diagnostic was useful and did not install implicitly.
Recommendation: keep setup scoped to the selected worktree and show the exact
preparation command in its failed readiness item. Do not rely on sibling artifacts.

### IF-04: prose layout needs application-owned line wrapping

Severity: medium for creative tools. Status: local implementation; shared UI gap.

The public drawing surface supplies text primitives, not wrapped paragraphs or
accessible button layout/navigation. This project supplies ASCII-aware wrapping,
button hit rectangles, focus navigation and three content pages. The source has
an authored-text capacity test, with visual inspection still required because
character counts do not measure proportional glyph widths.

Recommendation: expose measured wrap/layout primitives independent of simulation,
plus keyboard/controller focus and semantic button hit regions. Verify all content
at the supported logical viewport and both mobile orientations. Keep the shared
notice area at y=395 separate from application content. Avoid a general UI framework
until another consumer demonstrates the same needs.

### IF-05: portable exports lack a shared browser file-download interface

Severity: medium for utilities. Status: known product limitation and local CLI.

The portable game API handles save state, but does not expose a documented
user-triggered text download/clipboard command. This utility exports Markdown and
JSON through its headless CLI; browser favorites/history use engine storage.
No hand-written persistence format or unexpected browser network service was added.

Recommendation: provide a bounded user-triggered download intent with explicit
filename/content-type/text-size validation. Support native save dialogs and browser
Blob downloads; clipboard should require a gesture and report denial visibly.
Test failed export without destroying the user's existing file or saved library.

### IF-06: restart can preserve progress, invalidating an absolute input probe

Severity: medium for verification. Status: addressed in this project.

A library-preserving `restart()` intentionally keeps previously generated ideas.
A probe such as `generated > 1` can then pass before any new real-device action.
Idea Forge takes a new observation baseline after restart and restore and requires
the generated-plus-favorite count to increase. This baseline is verification-only,
excluded from gameplay and state hashing. Pointer/keyboard/controller probes still
use real public actions; replay never mutates state to pass a check.

Recommendation: document the distinction between a persistent achievement and a
fresh device interaction. Prefer before/after assertions in the generic browser
gate so project probes cannot accidentally certify an earlier session's activity.

## Verification and publication evidence

Evidence is kept in the game's ignored `.blue-check` folders and the engine's
ignored `.be2-work` reports; no session logs or credentials belong in this file.
Required delivery checks: catalog completeness and no repeated IDs, seeded
determinism, exact snapshot continuation, malformed-state preservation, genre/
history/favorite behavior, optional-story exports, desktop and independent touch
browser input, storage/reload and failed writes, focus clearing, offline install,
interrupted update recovery, public exact-revision retrieval and empty-target
reproduction. Publication requires a receipt verifying the remote manifest and
every deployed file, not merely a push.

Final results and publication receipt are recorded below when these gates finish.

Hardware limits: software Chromium and emulated touch/controllers do not certify
physical phones/controllers, Safari or audible speaker output. These remain
unmeasured; no physical-device claim is made.

## What future engine runs should retrieve

Learning ledger entries use `utility`, `buttons`, `pointer`, `keyboard`, `wrapping`,
`export`, `scaffold`, `path` and `probe` keywords. Representative query:
`portable utility buttons keyboard pointer wrapping export`.
The learning loop should surface these traps before another author implements a
creative tool. Promote shared capabilities only with their own regression evidence.
