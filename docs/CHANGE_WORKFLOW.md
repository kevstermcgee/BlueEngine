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

Combined edits union their checks. Any other path uses the full engine suite,
including Rust, content, Cargo files, documentation, the feature index and the
validation implementation itself. The declared feature graph informs impact explanations, but is not sufficient to
reduce validation. Renaming an independent Python file into an engine path therefore
cannot bypass engine checks. No-change output is explicitly not baseline certification.
Ignored scratch/build files are not change inputs; do not put shipped inputs there.

`check` without flags retains every existing gate: formatting, rustdoc, tests and
Clippy in both feature configurations, the headless dependency boundary and native
authoring integration. It also runs workflow, asset and publishing Python tests.
Feature configurations are grouped to avoid repeated binary rebuilds. CI still runs
the full Linux/Windows matrix, independent of local scope selection.

Reports contain the resolved baseline, paths, plan and per-command log/status.
Failure console output is capped at a 4,000-character tail; complete logs are retained.
Checks are never cached or accepted from stale receipts. Manual visual/input checks
remain required for relevant changes. A plan is not a validation result.

## Standalone games

New-game scaffolds contain a compact AGENTS.md, a CLAUDE.md import and one shared
`scripts/check.py` implementation. Existing projects can adopt
[the runner template](../templates/game_check.py) at that path when they use the same
starter layout: static client map at maps/main.json and GameDocument at game.json.
Add checks for any additional runtime maps or custom assets before using it in a
different layout; it cannot infer application-specific content dependencies.

Generate and commit the game's Cargo.lock once with `cargo generate-lockfile`.
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
