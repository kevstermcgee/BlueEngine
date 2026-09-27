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
validation implementation itself. The discovery index is deliberately not used as a
dependency graph. Renaming an independent Python file into an engine path therefore
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
builds. The generated client remains static: its separate GameDocument runs through
the stock engine, not automatically through the custom static loop.
