# Upgrading an external game to a chosen engine revision

`python tools/be2.py upgrade plan` and `upgrade verify` move a game built on BlueEngine to an exact
engine revision, repairing requested problems, without rewriting the game for you. The contract: move
this game to this exact engine revision, preserve its identity and intended behavior, fix the requested
problems, and adopt additional engine functionality only when it is relevant.

This is planning and verification tooling, not an autonomous migration agent. It never edits the game,
switches an engine checkout, launches a build, changes a dependency, or touches the network. Applying a
plan (editing `Cargo.toml`, fixing code, running `cargo update` for specific crates) remains a human or
agent task guided by the plan's output.

## Plan: read-only baseline/target/migration analysis

```sh
python tools/be2.py upgrade plan GAME_DIR --to REF \
  [--engine-checkout PATH] [--fix ID:DESCRIPTION ...] [--adopt MIGRATION_ID ...] \
  [--out report.json] [--json]
```

`REF` is a branch, tag or commit resolved **once**, locally, in the engine checkout the game's
`Cargo.toml` path dependency points at (override with `--engine-checkout`). The tool never fetches; a
remote-only revision fails with a clear message instead of being guessed at. The exact resolved commit
is recorded in the packet; re-run planning if a branch you named has since moved.

The plan reports, distinctly:

- **Game source**: the game's own `git` revision and dirty state (or "not a git repository").
- **Last verified engine revision**: `assets/identity.json`'s `engine_revision` field, which already
  exists for every game scaffolded by `new-game` (ADR 0017) — this tool does not invent a new file for
  it, but treats it as what it is (a recorded value that can be stale, not a live fact).
- **Engine currently resolved by the dependency**: the exact commit the game's Cargo.toml path
  dependency resolves to *right now*, plus whether that checkout is a `pinned_detached` commit or a
  `floating_branch` — **a path dependency on a checkout tracking a branch is not a version pin**: it
  moves under the game whenever someone updates that checkout.
- **Engine binary currently used by the game's tools**: wherever `scripts/check.py`'s own `find_tools`
  resolves to (`BE2_TOOLS`, `PATH`, or the dependency's target directory). `be2-tools` embeds no
  build-commit stamp, so this is reported as unverifiable provenance, never assumed to match the
  dependency commit; a `BE2_TOOLS` override outside the resolved dependency is flagged explicitly.
- **Runtime classification**: `two_d` (explicit game.project.json + simulation library), `stock` (GameDocument), `custom_sim`, `custom_sim_netplay` (uses
  `viewer::netplay`), `mixed_or_legacy`, or `unknown`, from `game.json`/`src/lib.rs` evidence. An
  unrecognized or mixed layout is reported `uncertain` for every migration, never a silent "nothing to
  do" — see `tools/test_upgrade.py`'s `test_unknown_layout_is_uncertain_not_silently_clear`.
- **Template provenance**: `identity.json:provenance` when a game records it explicitly, else
  `inferred` from the runtime classification and recorded engine revision (clearly labeled as not
  separately confirmed), else `none`.
- **`scripts/check.py` drift**: whether the game's copy is byte-identical to this engine checkout's
  current `templates/game_check.py`, so a customized or stale copy is visible, never silently assumed
  current.
- **Migrations**: every entry in `tools/upgrade_migrations.json` applicable to the runtime, with a
  status of `already_applied`, `not_applicable`, `uncertain`, or one of the categories below, decided
  from revision range *and* source evidence together — a revision match alone is a hint, not a verdict.
- **Requested fixes** (`--fix ID:DESCRIPTION`, repeatable): tracked as stable task IDs with an
  acceptance criterion, to carry through Stage D and Stage F below.
- **Warnings**: every non-fatal uncertainty (dirty checkouts, floating pins, missing identity, mismatched
  tool binaries, unresolvable migrations) collected in one list, never buried in a field nobody reads.

### Migration categories

- `received_automatically` — applies with no game-side change (e.g. an internal rendering fix).
- `required_repair` — a real compatibility defect the dependency bump does not fix by itself.
- `requested_repair` — reserved for a registry entry tied to a specific requested-fix pattern.
- `optional_adoption` — new shared infrastructure a game may adopt; never applied unless `--adopt`ed.
- `uncertain` — baseline unknown, layout unrecognized, or history unresolvable locally; never collapsed
  into a false "not applicable".

## Verify: reuse the game's own check runner, fresh every time

```sh
python tools/be2.py upgrade verify GAME_DIR [--skip-ship] [--content-only] [--scenario PATH ...] \
  [--timeout SECONDS] [--out report.json] [--json]
```

This does not re-implement game verification: it runs the game's own `scripts/check.py`
(`templates/game_check.py`, ADR 0017/the "Standalone games" section of `CHANGE_WORKFLOW.md`) with the
`be2-tools` binary that script itself resolves, and wraps the result with the engine commit and tool
binary identity actually used *at verification time* (which can differ from the plan's target if the
checkout moved again — compare the two). A missing `scripts/check.py` or `be2-tools` binary is reported
`available: false` / `ok: false`, never guessed into a pass. The command always executes fresh; a prior
`.blue-check/*/report.json` is never read as current evidence.

## Staged workflow (what the plan supports; this tool does not run A-F for you)

A. Establish baseline behavior and reproduce the requested defects (manual/agent, using the game's own
   capture/scenario tools — see `docs/CUSTOM_SIM_CHEATSHEET.md`, `docs/HEADLESS_CAPTURE.md`).
B. Update the dependency to the plan's exact target commit and perform the `required_repair` migrations.
C. Re-run the game's existing checks/scenarios to confirm previously working behavior is intact.
D. Fix the tracked `--fix` tasks that still reproduce; attribute each fix to its task ID.
E. Adopt `optional_adoption` migrations only if explicitly `--adopt`ed; this is a deliberate rewrite,
   not a side effect of the version bump.
F. `upgrade verify` for the final, truthful evidence packet; package/ship only after it passes.

Compatibility changes (B) and gameplay tuning (D) are tracked separately in the plan's `migrations` and
`requested_fixes` so a failure in Stage F can be attributed to the right stage.

## Limitations and explicitly deferred automation

- No autonomous code rewriting: migrations and fixes are reported with repair guidance, never applied.
- `tools/upgrade_migrations.json` is seeded with a small number of genuine, verified historical changes
  (see its `reference` fields); it is not a complete changelog and is not meant to become one.
- Template provenance for games scaffolded before this tool existed is inferred, not recorded at
  generation time; `new-game` was not changed to write a separate template-revision field (deferred —
  would touch engine Rust source and its own full test gate for a benefit this tool already gets from
  existing `assets/identity.json.engine_revision` plus runtime classification).
- `be2-tools` has no embedded build-commit stamp; tool-binary provenance is reported as unverifiable,
  never assumed.
- A three-way compare against the game's file *as originally generated* (not just the current template)
  needs that generation-time snapshot, which is not recoverable for games created before this tool; the
  plan compares only the current template against the game's current copy and says so.
- No timing/throughput numbers are claimed for the planner itself beyond what is measured in
  `tools/test_upgrade.py`; this tool does not change engine build or test cost.

For a declared 2D web project, `upgrade verify` invokes its `scripts/web.py build`: headless tests, locked WASM, package integrity and real browser checks. It reports this web target explicitly; native desktop shipping remains a separate gate. Old 3D migrations are marked inapplicable. Read docs/TWO_D.md and the game AGENTS/lib/project files; a rule change normally stays in lib.rs and its tests.
