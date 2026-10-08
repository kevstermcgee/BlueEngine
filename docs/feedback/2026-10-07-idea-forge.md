# Idea Forge: development feedback for BlueEngine

Date: 2026-10-07. Updated: 2026-10-08. Project: `games/idea-forge`. Runtime: portable, 2D, offline.
Delivery: Windows x64 EXE installer on BlueEngineGames. Linux is a native verification
target. The rendering-free CLI is available from source. An initial browser build
was tested from an older branch; browser gameplay is retired on current main and
no browser package was deployed.

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
retains a pointer target only for a mouse press or canvas tap. That target is
latched until the fixed-step press edge is consumed, including display frames
that run no simulation tick; movement after the click cannot move its target.
Pause, focus loss, restart and load clear pending input and its target. A focused button receives Space/A. The exploratory browser branch also cleared
stale mobile-panel targets; that change is not part of native delivery. Simulation still consumes only
Intent and never reads device APIs.

Regression: engine `pointer_action_tests`, game keyboard/pointer tests, and
native package checks; the initial browser branch also passed desktop controller
and independent touch verification. Future refinement:
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
JSON through its headless CLI; desktop favorites/history use engine storage.
No hand-written persistence format or unexpected browser network service was added.

Recommendation: provide a bounded user-triggered download intent with explicit
filename/content-type/text-size validation. Support native save dialogs; clipboard should require an explicit action and
report denial visibly. Browser text-download experiments are outside current native delivery.
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

### IF-07: cache and compiled target paths were offloaded to slow storage

Severity: medium for iteration time. Status: task-local build mitigation.

The first headless compile/test completed in 5m55s. Concurrent Cargo jobs later
stalled while host I/O pressure exceeded 90 percent. Both compiler cache and the
main checkout's `target/debug` and `target/itest` were symlinks to a USB-mounted
drive. This is host evidence, not an engine benchmark or proof of one root cause.
Bypassing only the compiler cache was insufficient. No global cache or existing
compiled targets were modified.

Mitigation: stop only this task's process trees, serialize local checks, use
command-local `RUSTC_WRAPPER=''` and a real local-disk `CARGO_TARGET_DIR`. Full
Linux/Windows CI provides completion evidence; interrupted local full runs do not.
The initial anonymous, empty-target reproduction passed on local disk.
Recommendation: show resolved cache/target paths and observed stalls in timing
diagnostics without silently changing a user's build configuration.

### IF-08: headless success did not compile the presentation implementation

Severity: low. Status: corrected application error.

The first browser compilation caught E0185: this project's `show_hud` method used
`&self`, while `draw::Game::show_hud` is an associated function. Headless rule tests
had passed because the presentation implementation is feature-gated. Corrected
the implementation to `fn show_hud() -> bool`. Keep both presentation compilation
and headless behavior in the delivery gates; neither replaces the other.

### IF-09: the default cue sparkle covered prose

Severity: low. Status: corrected presentation behavior and restored native hook.

Desktop preview showed the shared pickup/success sparkle at the default canvas
center, briefly obscuring the mechanic text. Override the existing `cue_point`
hook to put generation feedback on Generate and favorite feedback on Keep.
The initial branch provided this hook. Current native main had removed it, so
the compatible default-center hook is restored for this utility; no new particle
system is needed.
Recommendation: mention this hook next to portable UI/utility guidance so text
interfaces do not inherit an arena-centered effect by accident.

### IF-10: short touch verification replay outran audio activation

Severity: medium. Status: fixed in the exploratory browser branch; outside native delivery.

The first complete build passed desktop but failed independent touch verification:
audio decoded three buffers and both browser contexts eventually ran, but zero
sound submissions were recorded. The three-frame accelerated route could consume
all gameplay cues before the gesture's asynchronous AudioContext resume completed.
This was a real failed gate, not a missing audio asset or a passing touch test.

Resolution: accelerated verification waits for the real platform audio-active
signal before advancing the public-input route. It does not inject cues, change
rules or synthesize a success counter. Ordinary play timing is unchanged. The
shipping touch gate must still prove activation, decode and actual submissions.
This readiness guard is separate from proof of audible or enjoyable audio.

### IF-11: stale checkout guidance selected a retired publication route

Severity: high for delivery. Status: corrected workflow; policy already exists.

The initial engine checkout was on `couch-games`, which still documented browser
publication. Its package passed desktop/touch checks, anonymous exact-commit
retrieval and empty-target reproduction. The adapter then refused to rewrite the
current companion catalog because its integration point had changed. No companion
push occurred. Reading current main revealed ADR 0045/0049 and the explicit
Windows-installer-only policy; updating an adapter to reintroduce browser hosting
would have violated that policy.

Resolution: rebuilt this feature on current engine main, refreshed the generated
project/ship tooling, declared Windows distribution and Linux verification, and
used the native installer workflow. Recommendation: check current distribution
policy and companion contract before expensive builds, including when the task
starts from a long-lived engine branch. Fail before source retrieval/reproduction
when the current destination rejects the chosen delivery type.

### IF-12: the native shared client had no persistent restart hook

Severity: medium for utilities. Status: addressed with a compatible engine hook.

The current native client reset every game with `G::new(7)` when R was pressed,
which would discard Idea Forge's library. Added `GameLogic::restart()` with that
same default behavior; the utility overrides it to request another idea while
retaining generated IDs and favorites. Existing games keep their original reset.
The utility's restart/snapshot tests verify library preservation. Future utilities
should choose reset semantics deliberately and describe them in their controls.

### IF-13: a browser icon set did not meet native package requirements

Severity: medium for packaging. Status: corrected assets; shared gate worked.

The initial native package launched and produced a reviewed 960x540 capture, but
failed icon-files and icon-art: the ICO omitted 20/24/40/96/256 sizes, its large
frames were not PNG-encoded and the 32px art had only four colors. Browser display
success did not satisfy native resource quality requirements.

Resolution: regenerated this utility's complete icon set using the fresh canonical
`be2.py map icon "Idea Forge" games/idea-forge/assets --replace` command. Inspected
the resulting artwork and reran package verification. Recommendation: run native
identity/resource checks before expensive release builds, and select icon tooling
for the actual delivery target rather than accepting a web-only bitmap set.

### IF-14: integration metadata failed existing workflow and ledger checks

Severity: low. Status: corrected authoring errors; existing checks caught them.

Adding the independent utility CI job changed the aggregate dependency list, but
this task initially left the workflow regression's expected list unchanged. The
full Python suite failed, so the expectation was updated to require the utility
job, its success condition and Windows isolated smoke. A later ledger test rejected
this task's `identity` area; the supported category is `assets`. Validate new
entries with the learning tool's schema, rather than assuming a descriptive label
is supported. Neither failed full local run is passing evidence.

Review-release staging also exposed a missing local Git author identity. The
failed commit was followed by an accidental dispatch of the old branch revision;
that review run was canceled without publishing. Staging now uses a command-scoped
publisher identity and compares the remote branch SHA with the intended commit
before dispatch. Recommendation: make commit success and exact branch revision
explicit prerequisites in manual release orchestration.

### IF-15: hosted Windows runners lack the native client's required OpenGL

Severity: medium for CI. Status: verified in the real isolated Windows package smoke.

The Windows utility tests and Clippy passed. Native identity, full icon set,
package hashes and embedded FileDescription/ProductName/icon also passed. The
isolated executable then exited 101 in miniquad's WGL initialization with
`WGL_ARB_pixel_format is required`. This is a real failed launch, not successful
Windows runtime evidence. The hosted runner's graphics environment was insufficient.

Resolution: the Windows CI lane downloads a version-pinned, SHA-256-checked
[Mesa distribution](https://github.com/pal1000/mesa-dist-win/releases/tag/26.2.4)
and selects llvmpipe in the launcher process. Its temporary DLL directory is
inherited by child processes as described in
[Microsoft's DLL search contract](https://learn.microsoft.com/en-us/windows/win32/dlls/dynamic-link-library-search-order).
The standard ship verifier still copies only declared product files and executes
that isolated package; renderer DLLs remain CI dependencies and are not bundled
with the utility. No smoke skip or injected success is used. The rerun produced
an actual nonblank
960x540 capture; its native Windows receipt records `smoke: true` at engine
revision `54b5e87ca766`. The picture was inspected. Both native utility CI jobs passed.

Recommendation: provide documented software-rendered Windows smoke infrastructure
for portable OpenGL clients. Keep hardware-driver and audible-output claims separate
from this virtual-runtime evidence. Preserve resource-only failures and launch
failures as distinct diagnostics, since one does not imply the other passed.

### IF-16: one parallel local engine test failed an immediately released UDP port

Severity: low for CI stability. Status: open; isolated retry passed.

A full local run stopped at stage 14/39: `the_real_spawner_skips_ports_in_use`
failed its positive port-availability assertion after dropping the held UDP socket.
The remaining 493 tests in that selection passed. The exact isolated retry passed
immediately. A concurrent allocation of the released ephemeral port is a plausible
explanation, not a demonstrated root cause. No spawner behavior was changed for
this offline utility. Full Linux/Windows CI subsequently passed all engine stages.

Recommendation: review the test's assumption that a released port stays available
between drop and probe under parallel socket tests. Record bind errors and avoid
promoting a partial full run or isolated retry into full-suite completion evidence.

### IF-17: a task-local verification cache exhausted disk capacity

Severity: medium for local verification. Status: capacity recovered; interrupted
full-run evidence remains partial.

The final feedback check passed stages 1 through 19 of the 39-stage engine plan.
Stage 20, schema-validation Clippy, could not write Rust metadata or its incremental
query cache: OS error 28, no space left on device. The error also prevented the
runner from finishing its normal report. The task-owned target directory occupied
24 GiB, including approximately 9.8 GiB of incremental caches; the root volume had
no available space. This is a build-resource failure, not a passing full check.

Recovery removed only the inactive `debug/incremental` and `itest/incremental`
directories inside this task's isolated target, recovering approximately 9.7 GiB.
Source, user caches, completed packages and verification captures were preserved.
Retry the failed stage and all remaining commands from the maintained full plan;
keep their continuation evidence separate from an uninterrupted full-check receipt.

Recommendation: report available capacity before a full matrix, budget task-local
cache growth, and preserve a failure receipt even when its normal disk is full.
Offer safe task-owned cache cleanup and explicit continuation with content binding.

The explicit continuation subsequently passed all maintained commands 20 through
39, including the complete tooling test batch. Commands 1 through 19 had passed
before the capacity failure. This covers the maintained local plan across two
receipts; it is not described as one uninterrupted successful runner receipt.
The ignored `.be2-work/final-check-continuation/report.json` records every remaining
command and exit code. Full merged-source Linux/Windows CI is separate successful
end-to-end evidence.

## Verification and publication evidence

The initial branch `db361f4786b07e71fea41100a0154b9ade7c1159` passed 10 headless/CLI/
identity tests, all six inspected browser preview captures, desktop/controller and
independent touch shipping checks, anonymous exact-revision retrieval and an
empty-target rebuild with matching package hashes. Package ID:
`ad62463a401423065d192c50dbe8d33a7fae157ade781bd7d8558d935f096a12`.
Full Linux/Windows engine checks and browser CI passed:
https://github.com/kevstermcgee/BlueEngine/actions/runs/37729911409.
This is historical development evidence, not native delivery certification.

Native completion evidence:

- Source `54b5e87ca766`, merged through PR 15 into `c83bc392f717`.
- Full Linux/Windows engine verification, shipping release builds and aggregate
  gates passed: https://github.com/kevstermcgee/BlueEngine/actions/runs/37738209848.
- The merged source `c83bc392f717` also passed full Linux/Windows CI, including
  stock-audio offscreen verification:
  https://github.com/kevstermcgee/BlueEngine/actions/runs/37741901873.
- Native utility CI passed on both systems: 11 rule/CLI/identity/layout tests,
  formatting, Clippy, package/icon checks and isolated executable smoke. Windows
  also verified embedded title/product/icon resources. Native captures were reviewed.
- Virtual X11 keyboard input with the pointer hovering Generate activated focused
  Keep correctly. R retained the library; K saved, another Generate changed it, and
  L restored three generated ideas and one favorite. Evidence is in the ignored
  `.blue-check/manual-input` folder, with storage isolated from user data.
- The complete catalog review passed at companion source `32acb2db20af`:
  https://github.com/kevstermcgee/BlueEngineGames/actions/runs/37735869264.
- The selected Windows installer review passed at `fc476e9e2c68`:
  https://github.com/kevstermcgee/BlueEngineGames/actions/runs/37736894026. Its
  catalog, ZIP and EXE hashes were checked; the payload is Windows x64 and includes
  the standard updater. This review artifact was not claimed as a published release.

Production delivery verified on 2026-10-08 at 07:35 UTC:

- Complete-catalog Windows production build passed:
  https://github.com/kevstermcgee/BlueEngineGames/actions/runs/37741291032.
  It uses companion source `e5d8e76c8bce` and engine `54b5e87ca766`.
- Immutable release: [blueengine-54b5e87ca766-games-e5d8e76c8bce-37741291032-1](https://github.com/kevstermcgee/BlueEngineGames/releases/tag/blueengine-54b5e87ca766-games-e5d8e76c8bce-37741291032-1).
- Pages deployment passed:
  https://github.com/kevstermcgee/BlueEngineGames/actions/runs/37744249759.
- Public [Idea Forge download page](https://kevstermcgee.github.io/BlueEngineGames/games/idea-forge/) returned HTTP 200,
  offered the exact release's Windows installer, and appeared in the catalog.
  The live landscape screenshot matched the authored screenshot's SHA-256.
- Public EXE and ZIP downloads returned HTTP 200 and matched published checksum
  files and GitHub asset digests. Installer: 2,906,763 bytes,
  SHA-256 `65af3cca0c4e236944acd01433fa64cc344467dee4e2a5b1b89d9b32a7ecb329`.
  ZIP SHA-256: `26443f6976250fb971bd478c220d7fc651f64439ba54e5d1c243ac4f65810371`.
- The published catalog contains 35 playable entries, including Idea Forge.
  The ignored `.blue-check/publication/native.json` stores the verification receipt.
- The normal exporter synchronized merged engine `c83bc392f717` into companion
  `c934da54dc60`. That complete production build also passed:
  https://github.com/kevstermcgee/BlueEngineGames/actions/runs/37741924713.
  Its Pages deployment passed:
  https://github.com/kevstermcgee/BlueEngineGames/actions/runs/37746983417.
- Latest [immutable release](https://github.com/kevstermcgee/BlueEngineGames/releases/tag/blueengine-c83bc392f717-games-c934da54dc60-37741924713-1) was independently verified at
  08:01 UTC. The public page and catalog link this release's installer; installer,
  ZIP and screenshot returned HTTP 200. Hashes matched published sums and API
  digests. Latest installer: 2,906,971 bytes,
  SHA-256 `78bfce8cc355ab0f87997c47a37cbff87c438cea4cbb0daf69e4e6c6be8d7082`.
  Latest ZIP SHA-256: `3be1b7d10a4d549f84c2a2c7ae3b47eb1a4525f948381c0499301960103c11a4`.
  `.blue-check/publication/native.json` identifies this latest verification; its
  downloaded artifacts are stored under the matching release-tag subdirectory.

Ignored `.blue-check` and CI artifacts hold detailed evidence;
no session logs or credentials belong in this file. Interrupted local full checks
are not reported as successful runs.

Hardware limits: virtual displays, software rendering and emulated controller/
touch checks do not certify physical phones/controllers or audible speaker output.

## What future engine runs should retrieve

Learning ledger entries use `utility`, `buttons`, `pointer`, `keyboard`, `wrapping`,
`export`, `scaffold`, `path`, `probe`, `distribution`, `native` and `restart` keywords.
Representative query: `portable utility buttons keyboard pointer wrapping export`.
The learning loop should surface these traps before another creative tool is built.
Promote shared capabilities only with their own regression evidence.
