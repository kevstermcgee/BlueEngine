# Scheduled collaborative feedback maintenance

Runner version 5. Extend the existing nightly timer; do not install a second
feedback-maintenance schedule.

The existing timer now wakes hourly. Its service passes `--scheduled`: new AI work
starts once per Pacific date during the 23:00 hour; other wakeups only resume a
pending immutable delivery. Failed delivery resumes wait 1, 2, 4, then at most 8
hours. The journal retains the next retry time, and the shared runner lock prevents
overlap. Direct invocation without `--scheduled` bypasses the delay for recovery.
Missing wiki prerequisites are included in the pre-authoring game context.

`run.py` is the optional Linux host runner for BlueEngine's nightly feedback loop.
It uses Python's standard library and existing Git, GitHub CLI, Cargo, Python,
and authenticated Codex installations. It does not install tools or enable a
schedule itself. See [`../config/README.md`](../config/README.md) for reviewed
configuration and systemd examples.

The runner reads `~/.config/blueengine-nightly/config.json`, `prompt.txt`, and
`result.schema.json`. The configured repositories must already exist; the engine
origin must match the configured GitHub repository. State, locks, reports, and
feedback acknowledgments remain private under the configured `state` directory.
The durable `feedback-queue.json` keeps per-topic outcomes and concrete next
steps, separate from the immutable per-run digest and snapshots.
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

Canonical player reviews, AI development reports, learning-ledger observations,
GitHub issues/comments, and `docs/community/events/*.json` feedback enter the
same queue. Human and AI reports from either collaborator have the same priority
rules. Equal content is grouped with all source origins; reporter identity,
timestamps and human/AI labels never boost a topic. Player summary and game feeds
are derived context and cannot duplicate canonical reviews. Changing a review's
observation, game or version creates a new topic; renaming its reporter does not.
Old whole-file review fingerprints migrate once to content fingerprints when
first collected by version 3.

Each run writes `feedback-digest.json` with at most `max_feedback_items` selected
topics (default 24, range 1–100), snapshot links, target scope, and bounded
excerpts. Transparent keywords put reported data loss/security first, reported
crashes/launch/publish failures next, then other observations. These are routing
hints requiring verification, not a severity verdict. Remaining topics stay
pending. The AI must return a disposition for every selected ID before any push:

| Disposition | Daily behavior |
| --- | --- |
| `addressed` | Preserve the verified fix receipt; stop repeating this content. |
| `no_change` | Preserve the concrete reason (for example already fixed). |
| `deferred` | Keep a concrete next experiment and retry after one day. |
| `needs_human` | Hold with an exact action/authorization/external-owner step. |

After three deferred reviews, the topic is held for human direction to avoid
spending every night on the same blocker. Requeue one held/deferred topic with
`--retry-feedback ID`; this records a manual-retry receipt and resets only that
topic's review budget. Failures preserve prior queue/acknowledgment state. Queue
outcomes become durable only after existing local and exact-source hosted gates
pass; AI claims never close a finding. New evidence can create a new topic.

Community publication requests also appear in the digest. The runner verifies
their source, role and review state using the community release workflow.

## Engine, launcher and game corrections in one session

Selected launcher or companion-game topics create a second private worktree from
current companion main. The same AI session sees both private paths. Engine-owned
games remain in engine source; a companion game must have a trusted native
definition and no managed engine files under its source directory. An explicit
preserve entry never overrides contradictory engine ownership. Unknown source is
not removed by the exporter, so newly published companion forks remain writable.
The creator head/source tree is captured before authoring; pending
edits absent from main must be integrated before corrective maintenance. Other local checkouts, including unfinished
user work, stay read-only. The companion scope permits `launcher/`, `site/`,
`distribution/` and companion-owned game folders. Workflow/automation changes,
credentials, updater signing trust/endpoints, and generated release feeds are
rejected. Both private HEADs must remain unchanged until the harness takes over.

Launcher/shared feedback UI fixes require a consistent version increase in all
five version files; each changed game needs a Cargo package version increase and
matching lock. The harness runs site/distribution/JavaScript checks and affected
launcher core/build gates. It commits an isolated candidate, waits for exact-source
Windows distribution review, signed launcher UI/upgrade review when affected,
and each changed game's native Linux/Windows and installer review. Every changed
game is reviewed before any companion main push. The engine's automatic export
can advance companion main while engine checks run. Before companion checks,
the harness saves its complete patch and applies it to current main if the edited
game baselines are unchanged. Conflicts retain the patch and private checkout.
Another main advance after hosted review retains checked work; it never force-pushes stale checks.

Companion game production uses the native release adapter and its immutable
public installer/ZIP receipt. Reviewed corrections register an optimistic creator
revision before source/public publication and attach the checked release,
preserving concurrent drafts. Launcher production
uses the existing signed workflow, then checks the live feed version, immutable
asset metadata, exact production tag/source, four immutable assets, cryptographic
signature and signed version, downloaded EXE size and SHA-256. Site/distribution-only
corrections require their exact main production gates. `companion-receipt.json`
records checks, commits, hosted runs, game journals and public proofs privately.

Feedback acknowledgments and queue outcomes are saved only after both repositories'
required gates succeed. One repository can already be published when a later gate
fails; reports record that distinction and retain unfinished source and receipts.
If companion source reached main but public delivery failed, private
`pending-companion.json` makes the next scheduled run resume the original receipt
before another AI session or feedback acknowledgment. Failed launcher publication
reruns the original workflow/source and a newer attempt; it never rebuilds the
same immutable version from a later source commit. A concurrent creator draft
cannot become the parent of a corrective update: submission uses the pre-authoring
expected revision, or reuses an already delivered identical game tree.
Missing account access, unavailable source or true external prerequisites can need
human action. An ordinary launcher/game correction does not require approval
merely because its source lives in the companion repository.

Selected engine-owned `game.project.json` games also capture creator source before
the AI starts. Every directly edited game and addressed game topic needs that
baseline. The harness retains a delivery intent before engine main publication,
then uses the authored native release adapter for local, engine/native CI,
Windows installer and immutable public gates. Source revision registration comes
before native review, and creator head is checked again before public delivery.
`engine-games-receipt.json` retains release journals and creator receipts; a
metadata outage reuses an already completed release. The existing pending pointer
resumes either or both repositories without another AI session. Unselected game
edits and legacy `assets/games` changes are retained before publication for native
source migration. Addressed game topics cannot be acknowledged solely from
Engine CI; unsupported source needs an explicit deferred migration step.

Publication requests are actionable AI work, not an unconditional queue executor.
The prompt permits the trusted release/resume helper for reviewed authenticated
maintainer source; contributor requests still require source review/integration.
The harness rejects `addressed` or `no_change` for a request without a server
release receipt for its exact requested revision. An explicitly closed unmerged
publication PR can support `no_change`; otherwise defer with the precise review,
integration or release step still needed.

New feedback runs Codex in a separate worktree. A blocked result, changed HEAD,
dirty `no_changes` result, failed engine checks, or concurrent remote update stops
publication. Updates run the canonical engine checker and use an ordinary push
after confirming the base still matches. Failed work and reports are retained.

Before acknowledging actionable feedback, an update must pass the hosted Engine
workflow for the exact SHA, `main` branch,
`push` event, and `.github/workflows/ci.yml` path. Failed, cancelled, missing, or
unverifiable CI leaves the feedback eligible for retry. A push can therefore
succeed while the run still reports failure pending CI; the report records that
distinction. Successful temporary worktrees are removed; failed worktrees remain
available for review.

A clean `no_changes` result can cross at most 100 single-parent, feedback-only
commits: only JSON below `docs/player-feedback/reviews/` and `games/`, plus
`summary.json`, or `docs/community/events/`. The current player-feedback and/or
community-event validator must pass for the crossed data, and the first code
ancestor must pass the same exact-SHA/push/main/workflow CI check. A merge or any
other path stops inheritance. This handles both `[skip ci]` and GITHUB_TOKEN's
suppression of automatic push workflows without accepting unverified code. The
receipt records reviewed tip, every crossed commit and verified ancestor. Feedback
stays untrusted observation. Successful results atomically acknowledge fingerprints
and remove only their own worktree; retries skip AI once those fingerprints are seen.

The default hosted CI wait is 5,400 seconds, polling every 30 seconds. Configuration
may adjust these bounded waits. A service timeout must cover the agent, local
checks, CI wait, and collection overhead; the example uses eight hours.

## Commands and evidence

```sh
python3 run.py --check        # Check existing authentication and push destination
python3 run.py --collect-only # Collect feedback without agent, push, or acknowledgment
python3 run.py               # Run maintenance for feedback not yet acknowledged
python3 run.py --retry-feedback ID # Requeue one held/deferred topic without running AI
```

`--baseline` deliberately acknowledges current feedback without implementing it;
use only when reviewing and accepting a baseline. Ordinary scheduled runs should
not pass it. Runs with no new or due feedback skip Codex and builds. Reports are local
files, not notifications to third parties. A successful report does not certify
physical audio/display hardware or replace the configured engine gates.

Scratch integration tests use isolated temporary Git repositories and fake
agent/CI commands; they never publish or clean real caches:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest -v
```
