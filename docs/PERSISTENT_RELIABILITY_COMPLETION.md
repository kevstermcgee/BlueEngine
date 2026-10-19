# Persistent reliability completion

This report supplements the historical milestone evidence in `docs/perf/reliability/`;
those artifacts and their source identities remain unchanged. The review snapshot
was fetched before editing: main `1441a433a76aa3c8d0f507dfe370a5c9583882b1`,
PR #28 `3bc35536c202f35442d9d55982100d27b3dc754c`, still draft and unmerged.
Work proceeded in an isolated worktree without resetting an existing checkout.

## Corrections

* Shared ClientInput consumes GameShell's effective native focus/neutral-return
  decision before exposing device commands. Focused pause-menu commands remain
  distinct from unfocused input; authority on a remote server continues running.
* Final shipping verification excludes compiled-template inputs from independent
  Python scopes. The embedded bootstrap retains native generator checks; live
  generated-launcher/digest compatibility is exercised through scaffolding.
  Maintained pin-only updates retain Python-only checks.
* Workshop progression assertions directly check completion/refunds and blocked
  escrow/cause through public commands and the transaction preview.
* Afterpull supplies a mechanically different authoritative adapter. Common
  process orchestration, budgets, evidence and replay remain in tools/playtest.py.
* Current authoring docs point to shared focus ownership. Historical Afterpull
  observations are labeled historical; optional narrative requirements are unchanged.

## Verification record

The source-bound observations below are summarized in
[persistent-reliability-completion.json](perf/persistent-reliability-completion.json).
Raw reports, traces, original saves, migration destinations, immutable binaries and
failed attempts remain under `/srv/storage/artifacts/persistent-reliability-completion/`.
The JSON records their paths, source digests and binary hashes. These local paths
are retained workspace evidence; hosted workflow artifacts are separately available
from [PR #28](https://github.com/kevstermcgee/BlueEngine/pull/28).

| Observation | Source and result |
| --- | --- |
| Workshop seeded matrix | `f849ad3`: seeds 0–7, 10,000 ticks each, parallel and serial (160,000 total), identical state/trace/coverage signatures. Per group: 1,523 completions, 2,113 refunds, 182 delivery recoveries, 2,656 reloads and 80 migrations. |
| Afterpull seeded matrix | `f849ad3`: the same seeds/budgets and 160,000 total ticks; identical signatures. Per group: 44,729 moves, 7,010 undos, 3,992 retries, 40 room transitions, 1,976 losses and wins, 2,584 reloads. |
| Continuous longer sessions | Afterpull `f849ad3`: 100,000 ticks, 3,226 reloads, hash `2228df09e2c6351e`. Workshop `cc7eaad`: 100,000 ticks, 3,322 reloads and 100 supported migrations, hash `6e65c264ef1ede04`; exact full replay matches. Both retain the 128 MiB worker limit. |
| Detector self-tests | Test-only stalled completion/refund fixtures fail the new direct Workshop assertions. The duplicate-command fixture fails at tick 28; Afterpull's incomplete-undo fixture fails at tick 6. Both CLI failures replay exactly. These are injected faults, not naturally discovered game defects. |
| Termination recovery | `f849ad3`: a real 128 MiB Workshop child was killed after its first migration. All 33 streamed checkpoint folders survived; original/migrated checksums and engine save inspection pass. This tests process termination, not power loss. |
| Native input | `f849ad3`: hosted Windows foreground-window/GetAsyncKeyState/SendInput integration passed through the actual shared adapter. Shared regressions cover background F5/F9/R/Enter, held return, queued taps, between-frame focus loss, controller neutrality and focused menus. Real X11/XTest integration also passed. |
| Native/package compatibility | Canonical local Workshop and Afterpull Linux shipping gates passed, with genuine packaged-game captures. Hosted Linux/Windows consumer checks, package smoke and the Workshop Windows installer build passed at `f849ad3`. These checks cover the declared consumers, not every game's complete runtime. |
| Shipping planner/bootstrap | Focused planner/discovery tests pass. The compiled bootstrap retains Cargo/native scaffolding checks; a real generated project exercises launcher/digest compatibility. Pin-only changes retain Python-only verification. |

The Windows observation is software-generated native input, not physical-device
testing. Its exact job is linked in the JSON. Native capture uses muted intent and
a null audio device; empty stderr there does not establish physical backend health.

Two additional verification defects were found and fixed. The source documentation
audit traversed retained `dist` output and reported copied package links as source
failures; `95d9ef6` excludes generated output while testing that broken source links
still fail and completed package bytes survive. A real 100,000-tick Workshop run
exhausted its memory budget when serializing duplicate copies of the report;
`cc7eaad` streams the report. The same run and replay now pass, and a real CLI
integration regression enforces the original 128 MiB limit. Original failures,
including their saves, remain intact.

The final input audit also found that shell pause handling consumed controller
Start before the documented game-over restart helper could read it. The forward
repair retains that one accepted edge for restart, using the frame's shared native
focus/neutral decision before pause handling. Shared-adapter regressions execute
the real pause transition and cover background input, held return, the neutral
boundary and edge expiry. Ordinary hotkeys, menu actions and gameplay retain their
transition suppression. Earlier isolated controller-helper tests missed this
sequence; the final delivery links on PR #28 include the forward repair's checks.

Workshop's doubled eight-seed serial/parallel workload exceeded the default
120-second aggregate wall budget on this shared host; all eight parallel and four
serial sessions had passed. Its retained retry explicitly budgets 300 seconds
(175.599 seconds observed), without raising per-worker memory or 30-second limits.
The continuous 100,000-tick run explicitly budgets 300 seconds per worker; it took
19.015 seconds. These are budget observations, not performance comparisons.

The canonical command is `python3 tools/be2.py check --changed --base
3bc35536c202f35442d9d55982100d27b3dc754c`; affected standalone games additionally
use `check --game GAME --loop shipping`. Exact final delivery checks and the merged
main workflow are linked from PR #28 and the completion response. Earlier reports
certify only their recorded source; no unexecuted check is certified here.

Headless gameplay checks do not certify rendering, networking, physical input,
controller feel, visual clarity or enjoyment. Software-generated OS input is
separate from physical-device review. Muted rendering/capture success does not
certify audio backend health or physical audibility; the historical Linux
Macroquad/quad-snd ALSA worker panic remains documented with its diagnostics.

The historical roughly 153× result applies only to its recorded blocked-machine
workload, not overall engine speed, frame rate or multiplayer capacity. No runtime
performance claim is added here. Larger snapshot/lifecycle work, world-region
persistence and installed-player update/rollback remain subsequent milestones.
