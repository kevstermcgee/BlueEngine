# ADR 0032: A version-aware upgrade workflow for external games

Status: Accepted

## Context

Games built on BlueEngine depend on it through a Cargo path dependency, which is not a version pin: it
resolves to whatever commit the referenced checkout currently has. Moving an existing game (Spooky Kart,
the physics mini-games) to a newer engine revision while fixing specific problems was done by hand each
time, re-deriving the same facts: what the game last verified against, what the dependency resolves to
now, which of the engine's real behavioral changes (ADR 0020's mouse-look fix, ADR 0022's netplay kit)
actually apply to that game's runtime, and whether its own `scripts/check.py` still matches the current
template. None of that is engine source work; all of it was repeated reading.

## Decision

- **`tools/upgrade.py`** (stdlib only, same conventions as `tools/workflow.py`): read-only planning
  (`plan`) and fresh, truthful re-verification (`verify`), wired into `python tools/be2.py upgrade
  plan|verify`. Planning never writes to the game, never switches a checkout, never builds, never
  touches the network; it resolves the requested target revision once, locally, and records its exact
  commit. Verification always re-runs the game's own `scripts/check.py` (ADR 0017) rather than
  reimplementing it, and a prior `.blue-check/*/report.json` is never treated as current evidence.
- **`tools/upgrade_migrations.json`**: a small registry of genuine, verified historical engine changes
  (currently three, each citing the exact commit and ADR that introduced it) that an external game may
  need to act on. Selection combines revision range (`git merge-base --is-ancestor`) with source evidence
  read from the game itself (a grep or file-presence check); a revision match alone never becomes a
  verdict, and an unrecognized or mixed runtime layout is reported `uncertain` for every entry rather
  than a false "nothing to do".
- **Template provenance** is read from `assets/identity.json`'s existing `engine_revision` field (ADR
  0017) and the runtime classification, not a new required file; a game may additionally record a
  `provenance` object there (unknown keys are already ignored by the native `Identity` parser), which
  this tool prefers over inference when present.
- **Reuse over reimplementation**: `upgrade.py` imports a game's own `scripts/check.py` by file path
  (the same `importlib.util.spec_from_file_location` pattern `tools/test_game_check.py` already uses) to
  call its `engine_path`/`find_tools`, so the plan reflects what that game's *own* tooling resolves,
  including a customized or stale copy — which the plan also reports by comparing it to the engine's
  current `templates/game_check.py`.
- **Validation scope**: `tools/upgrade.py`, `tools/test_upgrade.py` and `tools/upgrade_migrations.json`
  join `tools/author.py`, `tools/assets.py` and `scripts/publish_games.py` as a fourth independent,
  narrowly-scoped Python validation path in `tools/workflow.py`'s `validation_plan`, so an isolated
  change to this tool runs `python -m unittest tools.test_upgrade` instead of the full engine suite.
  Editing `tools/be2.py` or `tools/workflow.py` itself still requires the full gate, unchanged.

## Consequences

A game upgrade starts from a packet that already distinguishes last-verified revision, live dependency
resolution, and tool-binary identity — three things a recorded commit alone cannot prove were ever the
same binary. The seeded registry is intentionally small (three entries); it is a mechanism, not a
changelog, and is expected to grow only when a future engine change is confirmed to need it.

## Rejected or deferred

- **Recording template provenance at `new-game` scaffold time** (a new field written by
  `src/viewer/newgame.rs`): would require an engine Rust change and its full test gate for a benefit this
  tool already gets from the existing `engine_revision` field plus runtime classification. Left for a
  future change if inferred provenance proves insufficient in practice.
- **An embedded build-commit stamp in `be2-tools`**: would make tool-binary provenance verifiable instead
  of merely reported-as-unverifiable. Deferred; not needed for a first, honest version of this workflow.
- **Automatically applying `required_repair` migrations or requested fixes**: explicitly out of scope
  (no autonomous code-rewriting agent). The plan gives repair guidance and a verification requirement;
  applying it remains a human or agent task.
- **A three-way compare against the game's file as originally generated**: needs a generation-time
  snapshot this tool has no way to recover for games created before it existed. The plan compares only
  the current template against the game's current copy and says so.
