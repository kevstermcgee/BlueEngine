# Scheduled engine feedback maintenance

`run.py` is the optional Linux host runner for BlueEngine's nightly feedback loop.
It uses Python's standard library and existing Git, GitHub CLI, Cargo, Python,
and authenticated Codex installations. It does not install tools or enable a
schedule itself. See [`../config/README.md`](../config/README.md) for reviewed
configuration and systemd examples.

The runner reads `~/.config/blueengine-nightly/config.json`, `prompt.txt`, and
`result.schema.json`. The configured repositories must already exist; the engine
origin must match the configured GitHub repository. State, locks, reports, and
feedback acknowledgments remain private under the configured `state` directory.
`runs_root` optionally puts retained feedback, agent logs, and worktrees on the
reviewed external filesystem. Optional `worktrees_root` separates short private
worktree paths from those longer retained run paths; for example,
`/srv/storage/artifacts/bn` receives a new random `bn-` directory for each
actionable attempt. This keeps emitted authoring paths within the normal intake
budgets. Its directories are created privately, and existing work is never reused.
Each run's `worktree.json` records its exact worktree path. Without this option,
the original `runs_root/RUN/worktree` layout remains in use.
`CARGO_TARGET_DIR` is inherited when set, otherwise
the runner uses `state/target`. The service's storage admission and maintenance
guards must run before this script.

## Publication contract

The collector reads current engine and games source, feedback from their existing
worktrees, and GitHub issues/PR comments. It excludes both the legacy and configured
private nightly run and worktree directories. Content fingerprints distinguish new feedback
from generated reports and administrative edits.

New feedback runs Codex in a separate worktree. A blocked result, changed HEAD,
dirty `no_changes` result, failed engine checks, or concurrent remote update stops
publication. Updates run the canonical engine checker and use an ordinary push
after confirming the base still matches. Failed work and reports are retained.

Before acknowledging actionable feedback, both an update and a clean `no_changes`
result must pass the hosted Engine workflow for the exact SHA, `main` branch,
`push` event, and `.github/workflows/ci.yml` path. Failed, cancelled, missing, or
unverifiable CI leaves the feedback eligible for retry. A push can therefore
succeed while the run still reports failure pending CI; the report records that
distinction. Successful temporary worktrees are removed; failed worktrees remain
available for review.

The default hosted CI wait is 5,400 seconds, polling every 30 seconds. Configuration
may adjust these bounded waits. A service timeout must cover the agent, local
checks, CI wait, and collection overhead; the example uses eight hours.

## Commands and evidence

```sh
python3 run.py --check        # Check existing authentication and push destination
python3 run.py --collect-only # Collect feedback without agent, push, or acknowledgment
python3 run.py               # Run maintenance for feedback not yet acknowledged
```

`--baseline` deliberately acknowledges current feedback without implementing it;
use only when reviewing and accepting a baseline. Ordinary scheduled runs should
not pass it. Runs with no new feedback skip Codex and builds. Reports are local
files, not notifications to third parties. A successful report does not certify
physical audio/display hardware or replace the configured engine gates.

Scratch integration tests use isolated temporary Git repositories and fake
agent/CI commands; they never publish or clean real caches:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest -v
```
