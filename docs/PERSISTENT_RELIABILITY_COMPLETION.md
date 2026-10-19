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

Verification is in progress. The final tested revision, content identity, canonical
reports, replay experiments, hosted CI and remote-main status will be recorded here
before delivery. No unexecuted check is certified by this report.

Headless gameplay checks do not certify rendering, networking, physical input,
controller feel, visual clarity or enjoyment. Software-generated OS input is
separate from physical-device review. Muted rendering/capture success does not
certify audio backend health or physical audibility; the historical Linux
Macroquad/quad-snd ALSA worker panic remains documented with its diagnostics.

The historical roughly 153× result applies only to its recorded blocked-machine
workload, not overall engine speed, frame rate or multiplayer capacity. No runtime
performance claim is added here. Larger snapshot/lifecycle work, world-region
persistence and installed-player update/rollback remain subsequent milestones.
