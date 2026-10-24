# Retained IdeaForge publication recovery

## Baseline and selected scope

The selected milestone is recovery of an existing IdeaForge companion export when
BlueEngineGames main advances. The controller still fetches main without
integrating it into an existing private worktree; publication then requires the
new main to be an ancestor. Repeating the same stages cannot repair that base.
This is the controller defect reported in [issue #36](https://github.com/kevstermcgee/BlueEngine/issues/36),
distinct from its manually recovered Scatterlock run and the companion's already
merged storyless-wiki validation fix.

The repository's current `AGENTS.md` and `start` help were read first. Entry:

```sh
python3 tools/be2.py start "Recover retained IdeaForge companion publication after main advances, preserving verified source and generated work" --kind engine --compact
```

Task `8032195a47c6` selected `docs/COLLABORATION.md`, `docs/IDEA_FORGE.md` and
`docs/GAME_AUTHORING_ENTRY.md`. Source ownership is `tools/idea_forge.py` and
`tools/test_idea_forge.py`; existing feedback integration provides a relevant
retained-worktree precedent.

| Input | Observed baseline |
| --- | --- |
| Primary engine checkout | `/home/kevin/BlueEngine`, branch `couch-games`, HEAD `d7159cbc01ec7c17128fef9f75d1f9d7473c0e90`; dirty tracked and untracked work preserved |
| Fetched engine main | `cc2e9f54bed69b325bd5016a68712667756faf70` |
| Isolated implementation checkout | `/home/kevin/BlueEngine-retained-recovery`, branch `fix/ideaforge-retained-recovery`, initially clean at fetched main |
| Companion checkout | `/home/kevin/BlueEngineGames`, clean local main `1fee05d6e3d44471ed04f251451fc9d2c8fd2d86`; fetched origin/main `11afbb9fa4f8f3919aff66c640f3fc0d24c97061` |
| Host | Debian 13, Linux `6.12.113+deb13-amd64`, Intel N97, four logical CPUs |
| Toolchain | Python 3.13.5, Rust 1.98.1, Cargo 1.98.1 |
| Available verification | Local Git remotes/worktrees, Python/Rust tooling, Xvfb and Linux software rendering |
| Unavailable verification | Native Windows execution and Wine; physical-device, audible playback and hardware rendering checks were not established |
| Managed-cloud context | No cloud environment-status tool or `/etc/codex/network-policy.json` was exposed; no cloud readiness is claimed |

## Reconciled evidence

- [PR #40](https://github.com/kevstermcgee/BlueEngine/pull/40) is merged as the
  fetched engine main above. `Intent.commands` and `IntentAccumulator` retain
  unordered game-owned bits until one tick consumes them; repeated identical bits
  coalesce. Old serialized intentions default to zero, while exhaustive Rust
  literals require the new field or a default update. This does not supply an
  ordered edit queue.
- The logical-screen camera helper exists and documents positive screen Y zoom,
  top-left picking, physical viewports and the distinct render-target convention.
  [Issue #39](https://github.com/kevstermcgee/BlueEngine/issues/39) remains open,
  but its requested adapter and regression implementation are present. Existing
  PR evidence includes synthetic native off-center input; physical input is a
  separate category.
- [Issue #37](https://github.com/kevstermcgee/BlueEngine/issues/37) remains open,
  but `game_tooling.pin()` now reconstructs all declared digest inputs from its
  recorded commit and rejects mismatched working-tree bytes before migration.
  `python3 -m unittest tools.test_game_tooling -q` passed all **23 tests** on this
  checkout, including real-Git restoration and dirty-input cases.
- PR #40 reports successful checks on its final branch commit `942ce33`, including
  [Engine checks](https://github.com/kevstermcgee/BlueEngine/actions/runs/40042020036).
  At baseline inspection, the separate post-merge
  [Engine checks](https://github.com/kevstermcgee/BlueEngine/actions/runs/40045153472)
  on `cc2e9f5` were **in progress**; this report does not count them as a pass.
  [Publish games](https://github.com/kevstermcgee/BlueEngine/actions/runs/40045153449)
  on that merge revision had succeeded.
- [PR #16](https://github.com/kevstermcgee/BlueEngine/pull/16) remains open and
  unmerged at `05b2871eb1b382165c364d5c00ab6ee78ef32859`. Its broad recovery,
  networking and audio work overlaps partially with later main implementations;
  current feedback integration already preserves released engine source in a
  separate checkout. Its historical green checks do not certify current combined
  source. Review necessary pieces individually; do not merge it wholesale.
- [PR #25](https://github.com/kevstermcgee/BlueEngine/pull/25) remains open and
  unmerged at `ac5161f2bec2b654ef979aecaed478b4dd71a57c`. Harvest Hollow is absent
  from current main and is unrelated to this milestone. Leave the branch and PR
  intact for a separate game integration review.

## Dependency-ordered roadmap

The order prioritizes completed-game delivery before authoring convenience and
broader evaluation. Items 2–5 remain investigation leads until reproduced against
their selected baseline; they are not claims of verified current defects. Later
API milestones can be reviewed independently; no API dependency between typography
and text editing is implied.

| Milestone | Failure, evidence and consumers | Existing overlap and smallest boundary | Dependencies and compatibility risks | Measurable acceptance |
| --- | --- | --- | --- | --- |
| **1. Retained publication recovery — selected** | A retained IdeaForge export cannot progress after companion main advances; #36, Scatterlock and other scheduled generated games | Extend existing Forge stages/state and retained-worktree recovery; manual checkpoint/merge worked, generic export refresh is missing | Verify repository, branch and worktree ownership; preserve frozen engine source and valid source gates; conflicts, dirty work, interruptions and uncertain pushes must retain recoverable state | Real local Git fixtures cover clean/dirty refresh, unrelated changes, conflicts, published retries, interruption, stale installer evidence and duplicate ownership; changed candidates require exact new review |
| 2. Standalone installer-to-launcher handoff | A working installer/site can still be rejected by shipped launcher trust policy; #30, BlueCraft and BlueCity | Companion publisher/preflight and reusable release workflow; trusted mirrors and actual launcher validator already solved individual releases | Preserve allowlists, immutable bytes/provenance, historical URLs, valid cache and complete-library latest release; depends on reliable candidate identity | Safe fixture publishes nothing; actual supported launcher validator accepts current/historical trusted URLs, rejects untrusted ones with the offending game/URL, and distinguishes transport from validation failures |
| 3. Ordered portable text edits | BlueCity queues/throttles seed edits itself; #38; command bits cannot preserve edit order | Bridge existing `TextField`/`CharFilter` and portable fixed-step input rather than add another editor | Explicit bounds/overflow, focus and shared-hotkey ownership; preserve old intentions, replay/save contracts and device-free authority | Rapid typing/paste/delete/selection retains order over zero/multiple ticks; pause/focus clear stale edits; replay and applicable save continuation agree |
| 4. Game-owned typography | BlueCity copies glyph/resource/DPI plumbing and encountered a custom font teardown crash; #38 | Extend existing Scene text/font cache and optional shared-overlay styling; custom rendering remains supported | GPU lifetime, glyph preflight, resizing, device scale and unchanged defaults; game retains style policy | Minimal normal authoring fixture uses regular/bold fonts in gameplay/overlays on Linux/Windows; ordinary and capture exits are clean; resize/new glyph checks pass |
| 5. Independent authoring trial | Current authored examples and self-checks do not quantify unfamiliar-agent completion reliability | Existing Engine Gauntlet, learning ledger, discovery and verification machinery; no new dashboard | Stable public contracts and delivery route from earlier milestones; private evaluator checks; authorized model access only | Fixed clean baselines/tasks for 2D, spatial 3D and saved-game modification; record success, retries, interventions, elapsed stages and unfinished runs; real token/cost data only where available |

## Required behavior and exclusions

Keep three identities separate: frozen verified engine/game source, companion
publication candidate, and package/installer/release evidence. Integrating a newer
companion baseline must preserve the first and invalidate only evidence affected
by changes to the latter two. Conflicts retain diagnostics and worktrees for
explicit resolution. A tooling migration belongs in a separately reviewed engine
integration candidate, without silently rewriting the frozen release source.

This session excludes UI/font/text APIs, audio, networking, simulation, save schema
changes, renderer changes, performance optimization, paid/fresh-model trials,
browser delivery, live schedules, real game releases and companion deployment.
The native lifecycle and text-only studio branding remain unchanged.

## Implementation and verification record

The common `Forge` now owns companion worktree identity checks, scoped checkpoint
and merge recovery, candidate snapshots and publication reconciliation. The
community release subclass reuses that implementation instead of carrying a
second copy. Existing journals keep version 1; absent recovery fields are derived
from the recorded candidate. No game API, Rust dependency, save, replay or native
lifecycle contract changes.

Before export, unrelated edits and distinct index/working versions stop without
mutation. Generated checkpoints and merge conflicts remain recoverable after an
interruption. Export stages only catalog-owned output, its catalog, the selected
native definition and thumbnail. Literal NUL-delimited Git pathspecs travel over
stdin to avoid Windows command-line length and filename-globbing problems.
Publication retains the exact game/thumbnail/definition/ownership/source pin,
repeats installer review when the candidate changes, and confirms already-pushed
ancestry before retrying. It clears the stale installer dispatch marker as well
as the stale receipt; source receipts remain intact. Deliberate rebuilds archive
and clear old candidate snapshots. Frozen source/tooling migration is explicit
and documented in the existing IdeaForge guide.

Evidence at the implementation checkpoint:

- Baseline: **117** IdeaForge/community/companion-release tests passed; **23**
  tooling tests passed, including restoration from recorded Git revisions.
- Final focused run: **140** tests passed in **79.819 s** on this N97 host while
  canonical verification was also active. This is an observation, not a speed
  comparison. The complete final **24-test** recovery module also passed in
  **7.492 s**, including the additional large-path test.
- The new real-Git fixtures cover clean and dirty retained exports, unrelated
  staged/unstaged/untracked work (including hidden index edits), conflicting
  edits and committed resolution, wrong branch/repository/path/origin, interrupted
  checkpoint/merge/review, stale installer dispatch/receipts, changed game identity,
  main advancing during review, uncertain push response, rebuild and run locking.
- The full export fixture uses the actual `scripts/publish_games.py` CLI, real
  frozen engine source, newer companion validator and actual local catalog,
  distribution and site gates. It preserves wiki/game bytes and unowned creator
  notes, refreshes the candidate snapshot, rechecks the installer through mocked
  GitHub observations and pushes only to its local bare remote. These observations
  are a test of orchestration, not a real Windows installer or public release.
- The same publication fixture with the exact baseline `cc2e9f5`
  `Forge.publish_games` method fails at `merge-base` after remote main advances:
  zero publication pushes and one original installer dispatch. Current recovery
  passes with one local publication push and two candidate-specific reviews.
  A deliberate hidden-index fixture also failed before its guard fix; this is
  detector evidence, not a newly observed defect in a shipped game.
- `python3 scripts/publish_games.py check` passed: **1,014** publication files.
  Discovery with “retained export companion main advanced stale installer receipt”
  selects the IdeaForge contract and existing public guide.
- The documentation gate initially required renewed README scope review after
  the capability contract changed. The existing README already describes this
  workflow and its Windows installer gates; review recorded that specific reason
  with `tools/readme_scope.py update --unchanged-reason`. The documentation and
  README scope checks then passed without weakening their checks.
- The indexed IdeaForge feature check passed **204** tests in **78.686 s** before
  the final added cases. Canonical **57-command** verification was started at this
  checkpoint. Exact-commit Linux/Windows CI and its consumer/package lanes remain
  required before integration; pending, skipped or cancelled jobs are not passes.

Reproduce the final local behavior and required integration checks:

```sh
python3 -m unittest tools.test_idea_forge_recovery -q
python3 -m unittest tools.test_idea_forge tools.test_idea_forge_recovery tools.test_community tools.test_community_release_companion -q
python3 -m unittest tools.test_game_tooling -q
python3 scripts/publish_games.py check
python3 tools/be2.py check --changed --base cc2e9f54bed69b325bd5016a68712667756faf70 --plan
python3 tools/be2.py check --changed --base cc2e9f54bed69b325bd5016a68712667756faf70
```

The base is explicit so committing the changes cannot make a clean working tree
hide them. Private command logs and baseline-comparison script remain under
`.be2-work`; they are not published. The implementation branch is
`fix/ideaforge-retained-recovery`; its pull request and the final delivery report
record exact tested commits, final checks and main integration status.

No live retained run, schedule, release or companion service was modified. Native
Windows execution is available only through CI; physical input, hardware GPU
performance and audible playback remain unverified here. No new game/native
interaction changed in this milestone. No independent model trial was run, and
no before/after authoring-time, model reliability or runtime performance
improvement is claimed.

Remaining limits: conflicting edits need explicit committed resolution; unrelated
creator dirt needs explicit disposition; a newer/divergent engine catalog pin
still requires source integration and fresh source gates, and current canonical
tooling may need migration in the separate integration checkout. Recovery never
silently changes the frozen release source. The one recommended next milestone
is the supported installer → website → launcher acceptance path from roadmap 2.

Engine checks run on pushes and pull requests with Linux/Windows canonical checks,
release builds, consumer/package lanes and final aggregations. The main-only
publication workflow filters content/source/exporter paths; changes limited to
IdeaForge tooling and these guides do not match it. Main branch protection queried
as unprotected and repository rulesets were empty at inspection; this does not
replace the assignment's requirement to finish applicable checks before merging.
