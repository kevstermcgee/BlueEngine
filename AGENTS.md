# Working on BlueEngine

First run `python tools/be2.py context "<task>" --compact` in this checkout.
It needs only Python and the feature index, no build. Read the packet's selected
paths, not the whole repository. Exact feature IDs or diagnostic IDs narrow lookup;
low confidence means inspect/narrow before editing, never invent an API.
Use `be2-tools src find/outline/show` for deeper symbol navigation when needed.

For an iteration check, use the packet's `iterate` plan; select an indexed suite,
`--test SUITE::exact_test`, or `--typecheck` (library only). Choose one useful check,
not all three. Iteration success is not final verification.

After editing: `python tools/be2.py check --changed --plan` reports affected
features, uncertainty and the verification commands. Then run
`python tools/be2.py check --changed`. It includes staged/unstaged/untracked files;
use `--base REV` for committed work. Only reviewed independent Python scopes narrow
validation. Rust, manifests, content, docs, validation infrastructure and unknown
paths automatically require full checks. `check` always runs the full suite.
Feature test suggestions are for iteration; a plan is not passing evidence.
Keep full Linux/Windows CI. Inspect visuals/controls manually when they change.

Never violate:
- Authoritative simulation is shared, fixed-step and rendering-free. Clients send
  intentions; presentation must not duplicate gameplay authority.
- Respect active transport limits; queue acceptance is not acknowledgement.
- Preserve public vesper3d compatibility, semantic IDs and visual/collision agreement.
- Preserve completed outputs on failure; never shell-interpolate scene values.
- Preserve official assets/branding artwork. No plugin/service installation needed.

Browser front door: read docs/BROWSER_WORKFLOW.md. Use `python3 tools/be2.py web capabilities`,
`web prepare GAME`, `web build GAME`, `web verify GAME`, `web inspect GAME`, `web serve GAME`,
`web reproduce GAME` and `web publish GAME`. Portable 2D/hybrid/3D share these commands. Commit/push
source before publication; a receipt must confirm source retrieval AND deployed manifest/files.

Game projects start with their own AGENTS.md and project check. For new games read docs/PORTABLE_GAMES.md and choose portable (flexible), two-d, three-d or hybrid; use the shared browser/native client and explicit game.project.json requirements. A web game is done when the isolated browser/package gate passes and a deployment receipt or publication-ready artifact exists; native targets retain their desktop ship gate. Pick the starter by the rules
(docs/GAME_QUICKSTART.md): `GameDocument` counters/interactables/timers use the stock starter;
enemies, projectiles, scoring, AI or per-frame physics use `new-game ... custom-sim`. A native game is
done only when it ships with its own icon and desktop shortcut (`scripts/blue ship`; its
`scripts/check.py` fails until it does). Saving and loading state is engine-owned (docs/SAVE_STATE.md):
F5/F9 in the stock client, `devkit::Snapshot` for a custom simulation; never hand-write save files. Authoring starts
with `python tools/author.py describe`; discover assets with
`python tools/assets.py search TEXT`. Retrieve detailed contracts through context;
see docs/ENGINE_MAINTENANCE.md only for relevant maintenance obligations.
Custom-sim games: `docs/CUSTOM_SIM_CHEATSHEET.md` is the game-facing API on one page (read it before grepping the
source). Online play is `viewer::netplay` (`docs/NETPLAY.md`): implement `NetGame` + `ClientView`, do not write a
server (`netplay::cli::serve` is the server main; `be2-hub`, ADR 0037, hosts many games' rooms). See a game without a
display with `python tools/xcapture.py`. Silent-failure traps are documented in `docs/AI_DEV_FEEDBACK.md`.
Published-source changes also require `python scripts/publish_games.py check`.

Audio authoring: `be2-tools audio describe`, `docs/AUDIO.md`, and `assets/audio/observatory/project.json`.
Use named rendered bundles with `kit::AudioBank`; edits to audio JSON need rendering, not a Rust rebuild.
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

Build speed: iterate with `cargo build --profile fast`, not `--release`. `be2.py check` runs tests on the `itest`
profile for tests, Clippy, rustdoc and native authoring, locally and in CI (same assertions);
`--profile dev` restores the plain profile. CI caches compiled dependencies, always executes checks,
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
