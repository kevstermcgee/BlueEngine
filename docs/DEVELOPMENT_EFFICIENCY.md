# Time to a correct change

Start with `python3 tools/be2.py context "task words" --level 1 --compact`.
Use Python's `python` command on Windows. Level 1 names public guides, examples and
iteration commands; level 2 supplies contracts; level 3 supplies the implementation,
interfaces, configuration, tests and consumers derived from the same feature index.
`features --feature ID` gives one full map; `features --validate` checks paths, suites
and dependency edges without compilation. The map is curated, not complete Rust reflection.

| Work | Inner loop | Complete feature | Before delivery/merge |
|---|---|---|---|
| Engine change | `check --changed --loop inner` | `check --changed --loop integration` | `check --changed`, full Linux/Windows CI and relevant target/package/device gates |
| One exact regression | `check --iterate FEATURE --test SUITE::test` | Automatic affected subsystem plan | Full shipping gates |
| Standalone game | `check --game DIR --loop inner` | `check --game DIR --loop integration` | `check --game DIR --loop shipping` and evidence for each declared target |
| Tooling | Automatic indexed Python checks and validators | Affected tool tests | Existing reviewed shipping scope; full checks if engine contracts also change |

Commands in the table follow `python3 tools/be2.py`. Add `--plan` to inspect JSON.
`--path FILE` explicitly selects an iteration path; `--changed` includes staged,
unstaged, deleted, renamed and untracked files. `--base REV` covers committed changes.
Shipping is the default. Unknown inputs, build manifests/CI/index boundaries, undeclared
content scenarios and missing behavioral evidence fall back to the full suite.
The partial feature graph never authorizes narrowing shipping gates. Browser/network/package
requirements remain visible even when deferred during iteration.

Native authoring uses `be2.py map ...`: Cargo checks freshness in the same headless
`itest` output used by authoring integration. No release/LTO build is needed to scaffold
or edit content. Distribution still uses isolated release outputs. Existing fast profiles,
feature gating, dependency caches and the separate rendering-free authority remain intact.

For portable games, read the game AGENTS, `src/lib.rs`, project requirements and the
public portable guide. The engine owns input normalization, fixed stepping, pause/restart,
storage/snapshots, audio triggers, viewport, mobile controls and browser installation.
Compose Rect/Body/Trigger, seeded RNG and game rules for unique mechanics. The stock
GameDocument runtime composes counters, interactions, conditions, timers and movers;
its explorer can generate a physically executed winning route. Custom code remains available
for novel mechanics; do not duplicate authority in drawing or adopt native-only APIs for web.

Verification building blocks already exist: `runtime::assert_deterministic`,
`snapshot::assert_resumes_exactly`, `snapshot::assert_loads_replay_identically`,
stock scenario intents/state assertions, transport/netplay fixtures, and the complete desktop
plus touch browser gate. Extend the starter's public-input win, loss and collision assertions.
Test restart, pickup idempotence and timer boundaries where the game's rules use them.
Keep snapshots complete. Do not replace a losing/collision case with a happy-path hash.

Local web build/verify works before Git/origin setup and produces an isolated verified
package. Publication still requires committed exact public source, anonymous retrieval,
empty-target reproduction and deployed file/manifest receipts. A local artifact does not
claim a URL or public-source reproducibility.

Check failures retain complete logs, compiler spans, test assertions and successful stages.
The recovery packet names the smallest observed location, likely failure category and next
reproduction/doctor command. Reports include stage time, attempted commands, executed tests,
Cargo fresh/built artifacts and POSIX child CPU/RSS where available. Artifact reuse means
Cargo checked inputs; it does not mean behavioral tests were skipped. Browser wrapper caches
remain keyed by implementation/compiler hashes. No stale passing test result is reused.

Measure with `python3 tools/dev_bench.py --out .be2-work/dev-bench.json`.
`--root OTHER_CHECKOUT` compares the older front door; `--execute TASK` runs one actual
verification set through the existing runner. The seven-task matrix is in
[development_tasks.json](../tools/development_tasks.json). Tool timings are not agent
completion timings. Use its separate fresh-context game rubric and record correctness,
visual quality, reads, attempts, CPU/memory and cache state; unavailable values stay null.
Do not reset caches or rerun seven broad suites just to fill a benchmark table.
