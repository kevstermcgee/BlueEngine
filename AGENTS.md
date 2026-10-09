# Working on BlueEngine

Use `python3` on Unix (`python` on Windows). First run
`python3 tools/be2.py start "<task>" --compact` in this checkout. Add `--kind`,
`--project`/`--target` or engine `--path` when needed; see docs/AI_SPRINGBOARD.md.
It reuses context, reads current inputs and probes readiness without building/installing.
Run `next TASK_ID` or `resume TASK_ID` to refresh progress; `check ... --task TASK_ID`
binds observed checks to current content. Notes never certify checks.
Direct `context "<task>" --level 1 --compact` remains available without a build.
Read the packet's selected
paths, not the whole repository. Exact feature IDs or diagnostic IDs narrow lookup;
low confidence means inspect/narrow before editing, never invent an API.
Use `context FEATURE --level 2` for contracts and `--level 3` for implementation ownership.
Use `be2-tools src find/outline/show` for deeper symbol navigation when needed.

For automatic iteration use `python3 tools/be2.py check --changed --loop inner --plan`,
then execute without `--plan`. It tests library units, declared consumer suites and
relevant tooling; unknown/build-boundary inputs fall back to full checks. Use
`--loop integration` when the feature is complete. For a standalone game use
`check --game DIR --loop inner`, then `--loop shipping` for its declared native/package gates.
Engine dependency tests are required when engine inputs change, not after every game rule edit.
For a single regression, use the packet's `iterate` plan; select an indexed suite,
`--test SUITE::exact_test`, or `--typecheck` (library only). Choose one useful check,
not all three. Iteration success is not final verification.

After editing: `python tools/be2.py check --changed --plan` reports affected
features, uncertainty and the verification commands. Then run
`python tools/be2.py check --changed`. It includes staged/unstaged/untracked files;
use `--base REV` for committed work. Only reviewed independent Python scopes narrow
validation. Rust, manifests, content, docs, validation infrastructure and unknown
paths automatically require full checks. `check` always runs the full suite.
Automatic inner/integration plans and feature test suggestions are for iteration;
a plan is not passing evidence. Shipping remains the default; never merge on inner evidence alone.
Keep full Linux/Windows CI. Inspect visuals/controls manually when they change.

Distribution policy: BlueEngine games ship as **native Windows x64 EXE installers only**.
Use `start "<game task>" --kind new-game --target windows`, then the native
`python scripts/ship.py ship` gate on Windows. BlueEngineGames is a download-only
site; do not publish browser games there. Browser gameplay/WASM is retired; see docs/BROWSER_WORKFLOW.md for migration.

Never violate:
- Authoritative simulation is shared, fixed-step and rendering-free. Clients send
  intentions; presentation must not duplicate gameplay authority.
- Respect active transport limits; queue acceptance is not acknowledgement.
- Preserve public vesper3d compatibility, semantic IDs and visual/collision agreement.
- Preserve completed outputs on failure; never shell-interpolate scene values.
- Preserve official assets/branding artwork. No plugin/service installation needed.

Scaffolding/authoring uses `python3 tools/be2.py map ...` with fresh shared itest tooling;
release builds are for shipping. Browser commands return retirement diagnostics without building.

Game projects start with their own AGENTS.md and project check. Read the start packet's
selected guide and docs/GAME_QUICKSTART.md. CLI `new-game NAME DIR` defaults to portable;
pass `stock` explicitly for declarative GameDocument counters/interactables/timers.
Offline 2D enemy/projectile/scoring/AI rules use `two-d`. Simple 3D drawing over 2D
collision uses `three-d`; spatial 3D enemies, projectiles, AI or physics use `custom-sim`.
Hybrid/portable compose 2D rules and 3D drawing. Native multiplayer uses `custom-sim`
(or `stock` for declarative rules); Rapier/native world APIs use `custom-sim`.
The generated starter table in docs/GAME_QUICKSTART.md comes from templates/starters.json.
Native completion requires the game's own title/icon, executable resources, complete
package and isolated packaged-game smoke. `scripts/check.py` checks content/code/package;
`scripts/blue ship --no-install` packages and smoke-tests. Installation, when requested,
uses `scripts/blue ship` and checks its shortcut target/icon.
Saving and loading state is engine-owned (docs/SAVE_STATE.md):
F5/F9 in the stock client, `devkit::Snapshot` for a custom simulation; never hand-write save files. Authoring starts
with `python3 tools/be2.py map describe` in a source checkout (`tools/author.py describe`
for packaged tools or an explicit `BE2_TOOLS`); discover assets with
`python tools/assets.py search TEXT`. Retrieve detailed contracts through context;
see docs/ENGINE_MAINTENANCE.md only for relevant maintenance obligations.
Custom-sim games: `docs/CUSTOM_SIM_CHEATSHEET.md` is the game-facing API on one page (read it before grepping the
source). Online play is `viewer::netplay` (`docs/NETPLAY.md`): implement `NetGame` + `ClientView`, do not write a
server (`netplay::cli::serve` is the server main; `be2-hub`, ADR 0037, hosts many games' rooms). See a game without a
display with `python tools/xcapture.py`. Silent-failure traps are documented in `docs/AI_DEV_FEEDBACK.md`.
Published-source changes also require `python scripts/publish_games.py check`.

Audio authoring: `be2-tools audio describe`, `docs/AUDIO.md`, and `assets/audio/observatory/project.json`.
Use named rendered bundles with `kit::AudioBank`; edits to audio JSON need rendering, not a Rust rebuild.
Engine schema maintenance: edit the owning Rust types/constraints, then run
`python3 tools/be2.py schemas --write` and `schemas --check` (docs/SCHEMAS.md).
Game authors use committed schemas and existing validators; generation is optional native tooling.
Stock games bind them through `presentation.audio` (docs/AUDIO.md); render into `assets/audio` for
packaging. `game-validate` checks configured assets; the headless authority never opens a device.
Inspect state/errors and check bundles. Measure loops with `audio_report.py --loop`; numeric evidence
does not prove subjective quality or audible hardware playback. Keep audio out of gameplay authority.

Expect small tasks to touch 1-4 source files, subsystem work 3-8. Above 10, recheck
impact; simulation + networking + presentation may belong at a shared lower layer.
These are prompts to reconsider scope, not limits on necessary work or reading.

DONE WHEN requested behavior works, focused behavioral evidence and affected checks
pass, and public docs reflect changed public contracts. STOP. Do not refactor nearby
code, add speculative abstractions or expand scope. Tasks may override this default.

Build speed: iterate engine code with `cargo build --profile fast`, not `--release`. Engine `be2.py check` uses the `itest`
profile for tests, Clippy, rustdoc and native authoring, locally and in CI (same assertions).
Standalone `check --game` retains that game's Cargo profiles and shared target directory;
its development build may need separate artifacts from the engine's `itest` build.
For engine verification, `--profile dev` restores the plain profile.
CI caches compiled dependencies, always executes checks,
and retains both platform/feature modes plus shipping release builds.
The independent Python batch overlaps serial Cargo work; native integration remains ordered.
`--serial` provides a comparison/debugging path, and reports join all started work on failure.
Measured data lives in docs/perf (`python tools/perf.py report`); after a change meant to speed things up,
run `python tools/perf.py record --note "what changed"` so the next session can see whether it worked.
For verification speed use `record --suite check --note "what changed"`: one full check with
per-stage measurements in the current target directory; no extra benchmark rebuild.

Learning loop (ADR 0038, `docs/learning/README.md`): before writing low-level code, run `python3 tools/be2.py context "<task>" --compact`;
its `learned` lines name traps other games hit and what the engine already provides. When you finish a game task, record each friction item
(engine source you had to read, code you copied or wrote that other games will too, a silent failure, a big token cost) with
`python3 tools/learn.py record --game NAME --area AREA --tokens N --note "..." --keywords "future,query,terms" [--trap ...] [--duplicated PATHS]`.
Verify retrieval with `python tools/be2.py context "representative future query" --compact`. Missing keywords mean archive-only, no hints. Never copy session logs
or credentials into a repository.
