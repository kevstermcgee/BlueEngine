# Native authoring follow-up

This pass inspected the actual `couch-games` checkout at
`76743a0ced2fdceed136d88c42b0f1d304579785`, including its unrelated dirty artwork,
pixel-art tooling and measurement files. Those files remain untouched. The user
selected the existing unmerged authoring implementation as the base:
`e5f0a90a030cec59534314984f953c673c54b359`. Work is isolated on
`maintenance/authoring-followup-pass`; no branches were merged and no game was
published, installed or deployed. `cb19b58` was used only as historical evidence.

The earlier implementation is described in [AUTHORING_EFFICIENCY.md](AUTHORING_EFFICIENCY.md).
Its shared tooling, optional dependency boundaries, native delivery helpers,
source navigation and nine practical public answers already existed. The separate
creative-presentation and custom-replication branches were preserved. In
particular, their larger custom-world replication work is not available merely
because this branch documents networking.

## Implemented remaining gaps

1. Historical Reverie and Leo shipping variants could pass migration while losing
   their intentional soundtrack/audio preparation. Migration now requires their
   matching reviewed hook before replacing the old script. Missing or different
   hooks reject before any file write. Current thin-launcher projects retain
   arbitrary game-owned hooks. Tests compare the legacy and migrated preparation
   outcome and preserve completed package files, including CRLF hook checkouts.
2. Focused checks could omit changed feature-gated client modules and executable
   mains. Explicit path ownership now enables the actual input, presentation,
   physics and cinematic modules. Changed explicit Cargo binary paths select
   their binary and required features alongside library tests. Optional feature
   typechecks work even when default features omit that subsystem. A real external
   fixture proves that a passing library cannot hide a broken executable.
3. All 13 inventoried tooling pins now have explicit Python compatibility-check
   ownership. Their inner loop avoids unrelated Rust compilation; unknown pins,
   game content and executable/unknown documentation remain conservative. Shipping
   still requires the full gate. Plans explicitly describe evidence left pending.
4. Exact `GameDocument`, load/validate, `AudioBank::load`, and
   `Lifecycle::quick_save/quick_load` queries now identify their public contract.
   Stock declarative tasks have an actionable starter command, runtime and limits.
   Stock start packets identify game/map JSON and the actual client entry instead
   of a nonexistent simulation library. Existing projects list existing entry
   files. Two stale learning hints now point to shared tooling while preserving
   their historical notes.

## Shared tooling and migration

The maintained implementation remains in the engine templates, loaded through
small project entry points and a revision-provenance/content pin. Cargo proves
source-binary freshness; packaged tools verify their manifest inputs. Explicit
overrides must match verified content. Invocation remains independent of caller
directory; spaces, Unicode and Windows paths remain data, never shell source.
See [PROJECT_TOOLING.md](../PROJECT_TOOLING.md) for upgrade commands and hooks.

All 13 previously migrated engine-local projects have refreshed pins. Their Rust
rules, controls, cameras, presentation, assets, executable resources and hooks
were preserved. Scratch migrations of actual historical Reverie/Leo copies
reject missing preparation and accept their reviewed extraction. Companion
repository consumers remain on their own delivery/revision policy; they were not
rewritten. Scaffold and older-consumer verification use the existing fixtures.

## Resolved dependency graphs

Selected normal-edge package/version nodes on `x86_64-unknown-linux-gnu`, with
default features disabled except the stock-default row. These are resolved graphs,
not Cargo.lock membership, timings or AI usage. The dependency changes already
existed at the selected authoring base; this follow-up does not claim another
dependency reduction. Exact features, commands and nodes are in
`.be2-work/followup-graphs.json`; isolated external consumers are also verified by
the existing boundary gate.

| Consumer | Observed original checkout | Selected base / follow-up |
| --- | ---: | ---: |
| Pure custom rules | 97 | 17 |
| Offline 2D (`two-d`) | 126 | 54 |
| Spatial headless (`physics`) | 97 | 72 |
| Custom multiplayer (`networking`) | 97 | 47 |
| Schema validation | 156 | 89 |
| Schema generation | 103 | 24 |
| Custom native client | 135 | 65 |
| Default stock client | 137 | 135 |

The default difference comes from the prior opt-in cinematic exporter change.
Headless physics/networking do not select device dependencies. Pure rules/2D
do not select Rapier or QUIC. Compatibility re-exports and shared serialization
types remain unchanged. The existing custom rules fixture owns its dash-scoring
behavior and uses engine snapshots to verify continuation.

## Discovery and reading evidence

Use the existing context command with `--level 1 --compact`. All original nine
questions already had correct answers at the selected base: jump height,
offline 2D enemies/scoring, spatial custom rules, saves, HUD/audio, installer,
custom multiplayer, larger worlds, and browser retirement.

| New regression query | Actual answer | Before / after UTF-8 output bytes |
| --- | --- | ---: |
| `GameDocument` | `game_documents`, load/validate, stock rules and limits | 1617 / 1645 |
| Build a stock declarative game | Explicit stock scaffold command and game/map validation | 1221 / 1815 |
| `AudioBank::load` | Client-owned audio loading signature and rendered bundles | 1777 / 1488 |
| `Lifecycle::quick_save` | Engine quick slot, Snapshot versioning and failure Notice | 2026 / 1400 |

Selected reading paths for these questions become the 5,501-byte entry contract;
previous selections totaled roughly 29–60 KB. Output may grow when it supplies a
previously missing answer. Existing 3,600-byte level-1, 8,000-byte level-2,
22,000-byte engine-intake and 30,000-byte game-intake regression budgets remain.
Required warnings and signatures are complete. Bytes are reading-volume proxies,
not model tokens, billed usage or measured AI cost.

Custom `NetGame` uses the shared server CLI but does not inherit stock interest
management, partial replication or configured-capacity evidence. Its current
single-state framing limit remains explicit. Development UDP does not inherit
production QUIC security. Queue acceptance remains distinct from acknowledgement.
Browser gameplay/WASM remains retired; Windows x64 installer delivery and Linux
development remain the supported paths.

## Verification and measurement contract

Final-revision receipts are kept in `.be2-work/followup/verification.json` and the
canonical check's complete stage logs. They record the final Git revision, source
identity, exact environment, outcomes, wall-clock completion, summed stage time
and CPU where available. `.be2-work/followup/discovery.json` holds actual answers;
`.be2-work/followup-plans.json` holds before/after commands;
`.be2-work/legacy-migration-verification.json` holds historical-consumer evidence.
Only receipts with a passing outcome certify executed gates.

Hardware is Intel N97 / four cores, Linux x64, Rust/Cargo 1.98.1. After authorized
cleanup of disposable hard-drive build caches, verification uses an isolated warm
copy of prior authoring dependencies in
`/home/kevin/.cache/blueengine-authoring-followup-target`, a task-specific hard-drive
temp directory, two build jobs, no incremental state, and itest debug symbols
disabled. Cargo fingerprints still decide reuse and tests execute on cache hits.
Assertions/features are preserved; these settings do not constitute a speedup
comparison against a different profile/cache. Cache copying and concurrent
focused tests are excluded from no-change timing claims.

Cold engine setup is not measured. Genuine custom-rule edits, no-change checks,
content-only checks, integration and shipping evidence are labeled separately in
the receipts when executed. Plan size and selected dependencies alone do not
demonstrate faster compilation. Local summed stage duration is not billed CI
runner time. No universal performance or AI-cost reduction is claimed.

The initial full Linux gate passed at `51de597`: 45 stages, 1,420.921 seconds
wall-clock completion, 1,563.697 summed stage seconds, and 4,589 reported test
executions across configurations. The independent Python batch ran 758 tests
with 60 declared skips; real scaffold integration executes in its separate gate.
The final-revision gate runs again after committing these measured results.

| Measured inner loop | Observed seconds | Evidence and limitation |
| --- | ---: | --- |
| New rules fixture, warm dependencies | 0.2905 | One game artifact built; engine setup was already warm |
| No-change rules, three runs | 0.0369–0.0373 | Program and snapshot assertions execute; zero built artifacts |
| Fresh dash scores 19/23/29 | 0.273–0.286 | Each new source hash builds one game artifact and runs assertions |
| Fresh JSON jump heights 0.71/0.72/0.73 | 0.0077–0.0079 | Current game/map validation; no build, visual or device evidence |
| Current-content discovery, three runs | 0.0829–0.087 | No Rust/device/shipping verification |
| Inventoried tooling-pin inner check | 22.624 | Six Python commands, 354 reported test executions; full gates pending |

The pin-only plan formerly fell back to 45 full commands and now selects six
compatibility commands. This compares scope, not equivalent coverage or a
before/after speed ratio. Nested fixture builds are not counted by the direct
Cargo artifact counter. The scoring fixture reuses the maintained public-rules
example and engine save/restore continuation checks; it introduces no second
simulation or save format in the engine.

Linux package smoke uses Xvfb/software OpenGL. It does not certify physical
graphics, input or audio devices. This Linux session cannot execute Windows
installers or Windows CI; those final platform/resource/launch gates remain
required before delivery. Existing CI on other revisions is not substituted.

## Remaining work ranked by observed friction

1. Compound `start` requests can select only one contract: HUD plus original audio
   currently hides audio from the start packet, although context returns both.
   Preserve the intake bound while exposing all selected public answers.
2. Other historical learning hints still advertise repaired limitations, including
   old navigator caps. Reconcile them against fix provenance and test retrieval;
   retain the historical notes.
3. Compact netplay packets repeat the same complete warnings in two fields. Share
   their representation without dropping any runtime/security/replication limit.
   This would remove about 1 KB of repeated reading from those measured tasks.

Windows execution/device evidence remains unfinished because no Windows host is
available. Companion migration and larger-world networking changes remain
separate work, with explicit authorization/ownership required where appropriate.
