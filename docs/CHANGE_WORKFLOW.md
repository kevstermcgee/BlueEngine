# Small context, scoped validation

Start with the project's own files. For engine API discovery, run
`python tools/be2.py context movement` in the engine checkout. This searches the
existing feature index without reading Rust or building a native binary. The default
is three matches, with a hard maximum of five; use an exact feature ID and `--limit 1`
for one contract. It returns paths, semantics and behavioral evidence, not source.
Unknown queries return no matches. The full `features` command remains available.

The root [AGENTS.md](../AGENTS.md) routes tasks. Detailed invariants live in
[engine maintenance](ENGINE_MAINTENANCE.md) and are required for engine changes.
Historical architecture is no longer mandatory startup context for a game edit.

## Engine checks

`python tools/be2.py check --changed --plan` prints the actual command plan without
executing checks. `check --changed` executes it with persistent logs. Diff selection
includes staged, unstaged, deleted, renamed (both paths) and untracked non-ignored
files. HEAD is the default baseline; for committed work use `--base REV`, where REV
is the reviewed starting commit, not the new tip. Invalid refs/Git failures fail closed.

Only these independent Python changes have reviewed narrower scopes:

| Files (implementation and matching test) | Required checks |
|---|---|
| tools/author.py, tools/test_author.py | Fresh headless native tool build and all authoring integration tests |
| tools/assets.py, tools/test_assets.py | All asset tests and catalog validation |
| scripts/publish_games.py, scripts/test_publish_games.py | All publication tests and publication check |
| tools/upgrade.py, tools/test_upgrade.py, tools/upgrade_migrations.json | All game-upgrade planner/verifier tests |
| tools/learn.py, tools/test_learn.py, docs/learning data files (ledger, tasks, eval log, floor, dupes, report) | `python -m unittest tools.test_learn` (ADR 0038) |

Combined edits union their checks. Any other path uses the full engine suite,
including Rust, content, Cargo files, documentation, the feature index and the
validation implementation itself. The declared feature graph informs impact explanations, but is not sufficient to
reduce validation. Renaming an independent Python file into an engine path therefore
cannot bypass engine checks. No-change output is explicitly not baseline certification.
Ignored scratch/build files are not change inputs; do not put shipped inputs there.

`check` without flags retains every existing gate: formatting, rustdoc, tests and
Clippy in both feature configurations, the headless dependency boundary and native
authoring integration. It also runs workflow, asset, publishing, game-ship, game-check,
upgrade and media-tools Python tests.
Feature configurations are grouped to avoid repeated binary rebuilds. CI still runs
the full Linux/Windows matrix, independent of local scope selection.

### Windows type-check (`check --windows`)

`python tools/be2.py check --windows` (add `--plan` to only see it) runs `cargo check --locked --target
x86_64-pc-windows-gnu --all-targets` with default and with `--no-default-features`, from a Linux machine. It
needs `rustup target add x86_64-pc-windows-gnu` and a C compiler for ring's build script: a real
`x86_64-w64-mingw32-gcc` if present, otherwise the host `cc`/`ar`, which cc-rs is pointed at through
`CC_x86_64_pc_windows_gnu`/`AR_x86_64_pc_windows_gnu`. That works because `cargo check` never links: the C is
built as host objects nothing consumes. It is its own check; it does not combine with `--changed` and the
default `check` does not run it (the first cold run builds every dependency again for the Windows target,
a few minutes).

What it proves: all `cfg(windows)` code in the library, binaries, examples and tests (native key and focus
readers, `windows-sys` calls, console handlers) compiles and type-checks, so a Windows-only type error or a
missing `windows-sys` feature is caught before CI. Verified by temporarily adding a deliberate type error under
`#[cfg(windows)]`, which the check rejects.

What it does not prove: anything about running on Windows. Nothing is linked or executed, so linker errors,
runtime behavior, real key/focus/console handling and the Windows C code of dependencies are untested; a
type-check is not a Windows run. The Windows CI job stays the real gate, and input changes still need a manual
key test on Windows. The key reader's table logic is covered on Linux by driving it with a fake key source
(`ClientInput::begin_frame_with_key_source`).

Reports contain the resolved baseline, paths, plan and per-command log/status.
Failure packets include the failed command, observed category, diagnostic/location,
reproduction argv and full-log path. Complete output streams directly to disk.
Checks are never cached or accepted from stale receipts. Manual visual/input checks
remain required for relevant changes. A plan is not a validation result.

## Standalone games

New-game scaffolds contain a compact AGENTS.md, a CLAUDE.md import and one shared
`scripts/check.py` implementation. Existing projects can adopt
[the runner template](../templates/game_check.py) at that path when they use the same
starter layout: static client map at maps/main.json and GameDocument at game.json.
Add checks for any additional runtime maps or custom assets before using it in a
different layout; it cannot infer application-specific content dependencies.

The scaffold seeds the game's Cargo.lock from the engine's, so the game builds against the dependency
versions the engine was tested with and offline builds work. The first Cargo command settles it (adds
the game's own entry, drops crates only the engine's tests need) and `scripts/check.py` does that before
its locked build; commit the result. Do not run `cargo generate-lockfile`, which discards the pins.
Point BE2_TOOLS at a native binary from the same engine revision; build it once with
`python tools/be2.py build tools` in the engine checkout. The build prints its output
directory. No plugins or services are required.

- `python scripts/check.py --content-only` audits/lints maps/main.json and the game-selected
  map (deduplicated), runs each map's declared verification checks, and validates the
  GameDocument. No Cargo process runs. This is a content iteration result only.
  Without a map `checks` block, native lint's error policy applies; its warnings stay
  advisory. An explicit block (even empty) also runs native verify and its budgets.
- `python scripts/check.py` runs those checks plus `cargo test --locked` in the game
  project, which already compiles the game. The former preliminary `cargo check` is
  redundant. Engine dependency tests are not game-project tests.
- Add `--scenario PATH` for each relevant behavioral scenario. Map checks/scenarios
  must assert the behavior being changed; schema success alone is not playability.

Both shell wrappers delegate to this runner and propagate nonzero exits, including
Windows native-command failures. Windows runners give console process trees a hidden
console that descendants inherit, preventing Cargo/test subprocess popups. A Windows
regression test checks visibility in both the child and grandchild process.
Each invocation preserves logs and a JSON report
under .blue-check with scope, native binary hash and elapsed time. Console output is
one small result. Missing tools, invalid content, command errors and timeouts fail.

The full project check also ends with the **ship gate** (`scripts/ship.py verify`): the game must have a
package in `dist/`, its own identity and icon, and a desktop shortcut named after it, verified by reading it
back and comparing its icon with every other shortcut on the desktop. `--skip-ship` runs the rest while
iterating; without a desktop the shortcut checks are reported as skipped, never as passed. `scripts/blue
ship` creates and verifies the package and shortcut. The check also warns when `assets/identity.json`
records a different engine commit than the checkout the game points at.

Run the full project check on final files before delivery; after it passes, repeat
only if inputs change or a new concern appears. Presentation/input changes still need
world/menu inspection and real controls/fullscreen/movement checks in optimized
builds. Standard generated games use the shared playable runtime. Custom static loops
must explicitly adopt GameSession to run GameDocument behavior.

## Task packets and declared impact

`context "<task>" --compact` emits the same JSON without indentation. Packets select
at most five initial paths total, public API names, existing examples, constraints and tests.
Exact feature/diagnostic IDs have high routing confidence; natural language remains
explicitly low-confidence keyword retrieval. Lower-ranked records are omitted when
the combined records exceed 6 KB (the first record is always retained); omitted IDs
remain discoverable. Index tests bound every exact packet to 8 KB. No source is read.
Use the existing native `be2-tools src find/outline/show` for live signatures rather
than storing duplicate signatures in metadata. No physical-door API is implied by
retrieving GameDocument for a door task.

FEATURES.json remains the single index. Optional `depends_on` links describe reviewed
feature relationships; reverse `used_by` and transitive impact are computed. Existing
`files` are discovery/ownership hints, not exclusive ownership or exhaustive imports.
`check --changed --plan` includes owners, affected dependents, unmapped paths and
cross-boundary/large-scope warnings. Partial graph knowledge must never narrow final
verification: reviewed independent Python scopes remain the only fast paths.
Probably-unnecessary areas are advisory and derived from selected dependencies.

Use these annotations sparingly at critical source boundaries:
`AI-INVARIANT`, `AI-BOUNDARY`, `AI-WARNING`, `AI-HOTPATH`, `AI-COMPAT`, `AI-SECURITY`,
`AI-DEPRECATED`, `AI-CANONICAL`. Format: `AI-BOUNDARY ARCH-HEADLESS-001: short rule`.
Index the ID, kind, source, meaning and existing verification under `constraints`.
`context ARCH-HEADLESS-001` retrieves it; tests reject dangling IDs, paths and edges.
This is curated metadata, not an automatically complete scan of arbitrary comments.

Contracts use owns/inputs/outputs/may_depend_on/must_not_depend_on/performance/consumers;
checks supply VERIFY. Start with simulation, networking and presentation boundaries.
Canonical examples point to existing compiled examples/scenarios and their checks;
do not create another sample implementation. Decisions link existing ADRs. Only add
active traps with observed evidence and retire them once prevented. The replication
silent-loss regression in ADR 0014 is already tested, so it is indexed as a decision,
not a new active trap. No temporary lessons file is needed at present.

## Measurement

`context TASK --compact --record` appends query, emitted UTF-8 bytes, elapsed seconds
and the lookup's zero source/doc reads to ignored `.be2-work/context-metrics.jsonl`.
This is opt-in local instrumentation and is never uploaded. Check reports record
elapsed seconds, attempted/failed commands and changed paths; full command logs stay
on disk. Successful checks print one summary. These records measure the tools only,
not total task duration or agent behavior. An evaluator may attach task duration,
model tokens (if available), tool calls, source/doc opens, commands and repair loops;
missing values mean unavailable, never zero. Compare matched tasks/revisions and
correctness outcomes, and allow legitimate exploration. No vendor SDK is required.

Stop after requested behavior, focused evidence and affected verification pass;
update public docs only when the public contract changes. More reading is permitted
when the packet is uncertain. Full CI and relevant manual checks remain mandatory.

## Audit and measured examples (2026-09-27)

Retrieval comparison recorded at `edadb577` (not a rolling benchmark).
Baseline: `dab921cd3f0979142161f09af7e3e175c8421d0f`. Existing tools already supplied
build-free FEATURES lookup, native symbol navigation, compiled prototype quickstart,
scenario verification, generated external-game checks, change-scoped Python checks,
persistent logs and full two-platform CI. Replication regressions and headless Cargo
dependency enforcement already encode critical architecture. The missing piece was
prioritized context and linked impact, not another search or check framework.

Reproduced by loading baseline `tools/workflow.py` with baseline FEATURES.json and
calling `context(root, query)`; compared current default and `--compact` JSON. Byte
counts below are UTF-8 output excluding the final newline. Paths are recommendations,
not observed agent opens. Baseline and current use default limit 3.

| Query | Before bytes | After pretty / compact bytes | Before / after initial paths |
|---|---:|---:|---:|
| add replicated door state | 3380 | 4219 / 3336 | 25 / 5 |
| movement | 2353 | 1219 / 991 | 11 / 3 |
| prototype_api | 2516 | 2096 / 1758 | 16 / 3 |
| ARCH-HEADLESS-001 | 1771 | 2399 / 1794 | 11 / 3 |

The door task now retrieves multiplayer alongside authored game state; the baseline
missed networking in its top three. Exact IDs no longer bring incidental matches.
The headless diagnostic now routes to simulation and its dependency check rather
than unrelated map records. Some packets are larger because they carry actionable
contracts and constraints; correctness takes priority over byte count. Root AGENTS
shrunk from 2630 to 2416 bytes. Both versions perform zero engine source reads/builds
for lookup, so no improvement is claimed there. Full engine verification is unchanged.

This is a retrieval comparison, not a controlled agent trial: total task time, token
usage, failed commands, repair loops and actual source/doc opens remain unmeasured.
Use the local metrics with matched tasks to evaluate those outcomes. No Gauntlet
integration was found in this checkout. Deferred: inferred Rust dependency analysis,
protocol schema/version fingerprints, additional active traps and temporary lessons.
They need stronger evidence or a separate cohesive design; this change adds none.

## Engine iteration and diagnosis

Start revision for this improvement: `edadb577eb37c58da748352bf29a8b0b052458cd`,
clean checkout. Context, indexed evidence, source navigation and final gates already
existed. The runner collapsed child failures to exit 1, wrote logs only after return,
and could treat a zero-test command as success. This change extends that same runner.

Choose one useful iteration command, not a chain of redundant checks:

- Library type-check: `python tools/be2.py check --iterate multiplayer --feature-mode headless --typecheck`
- One regression: `python tools/be2.py check --iterate multiplayer --feature-mode headless --test replication_budget::oversized_world_makes_wire_progress`
- One suite: `python tools/be2.py check --iterate multiplayer --feature-mode headless --test replication_budget`
- Indexed subsystem suites: `python tools/be2.py check --iterate multiplayer --feature-mode headless`
- Tooling tests: `python tools/be2.py check --iterate change_workflow`

Add `--plan` to inspect without executing. Cargo iteration preserves default features;
`--feature-mode headless` explicitly opts into rendering-free checks. This avoids
quietly excluding presentation code when checking a graphics feature. Type-checking covers the
engine library, not binaries or runtime behavior. Suite targets come from existing
feature evidence and `tests/*.rs` paths; Python modules come from recognized indexed
unittest commands. Free-form prose is never executed. Unsupported mappings fail and
point back to context; no dependency closure is treated as complete coverage.

Success is labeled `scope: iteration`, with what passed and what remains unverified.
Final verification is still `python tools/be2.py check --changed --plan` followed by
`python tools/be2.py check --changed` (use `--base START_REV` for committed changes).
`python tools/be2.py check` always retains all full gates. Iteration cannot be combined
with changed-file selection. Linux/Windows CI and relevant manual checks remain.

Cargo compiler messages use JSON; libtest/unittest output is parsed separately.
Packets quote observed evidence, not an inferred root cause. Child exit codes survive
(e.g. Cargo 101); signals are recorded as negative returncodes and mapped to shell
128+signal exits. Runner-only exits: 3 for empty/unrecognized requested test evidence,
124 for `--timeout SECONDS`, 127 for a missing executable, 126 for launch errors.
These have no fabricated child status: timeout/launch returncodes are null; an empty
selection retains the child's actual 0. All-ignored selections also fail. No timeout
is imposed unless requested. On timeout the runner requests process-tree termination.
Full logs survive failure and timeout; reports include per-command time, log size,
executed tests and Cargo's reported fresh/built artifact counts. An artifact count
is not compile time or proof of freshness beyond Cargo's own dependency tracking.
The runner always invokes Cargo; it never accepts cached validation receipts.

Profiles, target directories, isolated release packaging and optimized builds are
unchanged. No profile/cache tuning or new development build option is claimed:
iteration already uses Cargo's dev/test profiles. Cold-cache/profile comparisons,
module-level libtest selection and arbitrary custom test harnesses are deferred.
Unknown test output fails closed in iteration. Total agent tokens, task time and
repair-loop savings are unavailable, not zero.

Measured on Windows with the existing target cache, unchanged profiles and one Cargo
build job. Logs/JSON records are local under `.be2-work/engine-iteration/`:

| Equivalent task/command | Before runner | After runner |
|---|---:|---:|
| Exact replication regression, warm-cache median of 3 | 0.409 s | 0.402 s |
| Same Cargo E0308 fixture, console bytes | 3616 | 1002 |
| Same Cargo E0308 fixture, child/runner exit | 101 / 1 | 101 / 101 |

The warm comparison used identical argv:
`cargo test --locked --no-default-features --message-format=json --test replication_budget oversized_world_makes_wire_progress -- --exact`.
Both had 135 fresh and 0 built artifacts; this is no demonstrated compile speedup.
The index edit (embedded in Rust) rebuilt 4 artifacts and ran the regression in
10.650 s, with 131 artifacts fresh. The starting cached pre-edit plain Cargo command
ran in 3.876 s; those conditions differ and must not be compared as a speedup.
No cold-cache build benchmark was done, so no profile/cache changes were made.

The diagnostic fixture used the same temporary dependency-free Rust library with
`pub fn value() -> u32 { "wrong type" }` and
`cargo test --offline --lib --message-format=json` on both runners. Single observed
runs took 0.279/0.180 s; initialization/cache order prevents a speed claim. Full logs
were 3406/3400 bytes; the compact packet retains E0308 and source line/column.
The deterministic runner exit-status regression fails against saved starting
`be2.py`/`workflow.py` (`1 != 7`) and passes after the patch. Existing workflow tests
now also exercise actual Cargo compile errors, assertion locations, passing and empty
exact selections, missing tools, timeout log preservation and conservative plans.
