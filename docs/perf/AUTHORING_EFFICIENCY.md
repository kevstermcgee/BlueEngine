# Focused authoring improvement pass

The implementation starts at `cb19b58e5c952ba83f120d801a85d929307ed19f`, the current
`origin/main` integration revision observed in this session. The primary checkout
was on `couch-games` at `76743a0ced2fdceed136d88c42b0f1d304579785` with unrelated
uncommitted artwork, learning, performance and pixel-art changes. It was left
untouched. Work is on `maintenance/authoring-efficiency` in a separate worktree.
The final revision, exact commands and results are recorded in the live receipts
below after committing this report. No merge, publication or deployment was performed.

## Implementation and verified leads

The project tooling lead was confirmed. Thirteen engine-repository consumers held
substantial copies of check/dev/ship/project implementations. Most older checks
missed functionality already repaired in the canonical templates. Reverie's
soundtrack rendering and Leo's audio rendering were intentional differences; both
are preserved as explicit Python hooks. Executable build resources, Rust game
source, controls, cameras, fonts, artwork and music were preserved. An inventory
of reviewed variants prevents overwriting unknown customization.

Generated projects now invoke maintained engine implementations through pinned thin
entry points. The content pin, relative engine location and revision provenance
prevent silently selecting a main-branch checkout or PATH executable. Source
binaries use Cargo freshness and exact artifact paths, including shared targets;
overrides must match verified artifacts. Exported tools use their existing package
manifest. Windows process cleanup passes paths as environment data. Canonical
Python implementation edits are no longer embedded Rust build inputs. See
[PROJECT_TOOLING.md](../PROJECT_TOOLING.md) for commands, supported source/package
layouts, pin upgrades, hooks and migration scope.

All 13 compatible projects in this engine checkout were migrated. The companion
BlueEngineGames checkout is a separate, older delivery checkout (observed 65
commits behind its remote tracking branch); it was inspected read-only and not
rewritten or merged. Its delivery policy and old-revision consumers are preserved.
New consumers and all engine-local migrated consumers are guarded by launcher/pin
compatibility checks that permit game-owned hooks.

Physics and QUIC dependencies were confirmed unconditional for narrow consumers.
Optional `physics` and `networking` boundaries now preserve shared types, existing
serialization, math and default client capabilities. Additive `native-client`
supports custom presentation/input without selecting stock physics/networking;
new custom-sim scaffolds and the representative migrated Signal Garden client use it.
Signal Garden retains its own rules, camera, HUD, music and controls. Existing explicit
no-default consumers that use
rigid bodies must add `physics`; production transport requires `networking` and
fails explicitly when absent. Schema generation now uses a small internal native
helper behind the existing command. See [DEPENDENCY_BOUNDARIES.md](../DEPENDENCY_BOUNDARIES.md).

Discovery failures were reproduced for jumping, offline 2D, retired browser paths,
large custom worlds and inconsistent learning hints. Existing context/start now
expose public fields/signatures, runtime-specific answers, working examples,
focused checks and complete limitations. Mandatory initial reading uses one bounded
entry contract. Source navigation's 100-line cutoff and missing async recognition
were repaired with long-body/literal/multiline regressions.

Prior fixes already unified the main test profile, shared game targets, overlapped
independent Python work, retained final CI aggregation, retired browser gameplay,
and made schemas opt-in. They were preserved. Confirmed remaining waste included
2D lint/docs using a different profile and heavyweight Python tooling embedded in
Rust. Those are repaired. The planner now knows optional feature/path ownership,
shared tooling and downstream suites. Unknown or executable documentation inputs
fall back conservatively; reviewed prose runs current-content drift/link checks.
Tests still execute on cache hits. Default/full stock/Linux/Windows/shipping gates
remain available. No CI acceleration percentage is claimed.

Unmerged `feat/creative-identity` and `improve/custom-replication` implementations
were inspected and not merged or duplicated. The latter's larger atomic snapshot
work is not advertised as available in this branch. Native shipping branches were
also inspected; their packaging work was not replaced.

## Selected dependency evidence

These count selected normal package/version nodes on `x86_64-unknown-linux-gnu`,
not lockfile membership, compiled lines, elapsed build time or AI cost.
[Exact graphs and configurations](dependency-boundaries/graphs.json) include the
historical manifest, feature flags and toolchain. Separate external workspaces
compile public APIs without default-feature unification masking narrow failures.

| Consumer/configuration | Before | After |
| --- | ---: | ---: |
| Pure custom rules | 97 | 17 |
| Offline 2D (`two-d`) | 126 | 54 |
| Spatial headless (`physics`) | 97 | 72 |
| Custom multiplayer (`networking`) | 97 | 47 |
| Schema validation | 156 | 89 |
| Schema generation | 103 | 24 |
| Custom native client | 135 | 65 |
| Default stock client | 135 | 135 |

## Practical discovery and reading measurements

Run `python3 tools/be2.py context "QUESTION" --level 1 --compact`.

| Question | Returned public path / limitation |
| --- | --- |
| Make the player jump higher | `player_profile.jump_height`, metres, finite `0..=3`; runtime identifies stock/custom Controller |
| Offline 2D enemies and scoring | `two-d`, `GameLogic: Snapshot<Input = Intent>`, game-owned Rust rules |
| Spatial 3D custom enemies | `custom-sim`, `Simulation::step`, optional spatial physics |
| Save/restore custom simulation | `Snapshot::capture/restore`, engine save slots and continuation test |
| Distinctive HUD and original audio | `kit::hud`, `AudioBank::load(root, muted, sfx_volume, music_volume)` |
| Windows installer | First verify `dist` with ship; separate existing Inno installer and installer-test path |
| Custom multiplayer | `NetGame` + `ClientView`, shared `netplay::cli::serve` |
| Larger custom world | Current 1100-byte framing/transport budget; no stock interest management/partial replication or fragmentation |
| Browser support | Retired; native Windows x64 installer delivery |

Stock configured player limits and custom lobby settings are not demonstrated
capacity. Queue acceptance is not delivery. Reliable events are opt-in and bounded;
hub development UDP does not inherit production QUIC encryption. These warnings
remain complete in compact answers.

The discovery regression ceilings are 3,600 UTF-8 bytes for the selected level-1
questions, 8,000 for level 2, 22,000 for start plus mandatory engine instructions,
and 30,000 including generated game instructions. Required warnings/signatures are
preserved rather than truncated. Historical required document unions for offline
2D/custom/custom-multiplayer were 42,344 / 61,181 / 80,781 bytes. Current required
engine documents are approximately 14,144 bytes; start plus engine/generated
instructions were 21,687 / 26,720 / 28,505 bytes during implementation. Final receipt
refreshes exact values. Historical unions are reconstructed from baseline routing
and mandatory guides; no baseline start-packet size is claimed. Reading bytes are
proxies, not model tokens, billed usage, or measured AI cost.

## Verification, timings and practical limits

Hardware: Intel N97, four cores, Linux x64; Rust/Cargo 1.98.1. The root filesystem
had under 200 MB free. Two early scratch builds ran out of space before their
requested checks completed; those failures are retained, and only this task's
partial output was removed. Final artifacts/temp/logs use the mounted USB drive.
Final runs disable incremental state (`CARGO_INCREMENTAL=0`) to fit the available
storage while retaining assertions, profiles and actual test execution.

Implementation evidence includes 479 narrow library tests, feature-disabled
production/scenario rejection tests, external game-owned dash scoring and engine
snapshot continuation, 2D collision, physics saves, custom NetServer and schema
fixtures. Discovery/springboard/learning tests and long/async navigator tests
passed. Shared tooling tests cover stale overrides, package inputs, wrong working
directories, Unicode/spaces, migration preflight, hooks and drift. Final receipts
supersede intermediate passing results and include failures if a gate blocks.

Exact final-revision receipt locations in this checkout:

- `.be2-work/final-verification.json`: required canonical engine check report and
  any platform/shipping checks, tied to source revision and actual commands.
- `.be2-work/discovery-measurements.json`: before/current outputs and reading union.
- `.be2-work/consumer-comparison.json`: baseline/current existing Pocket Breaker and
  Signal Garden public behavior/save tests with untouched rules and tests.
- `.be2-work/iteration-measurements.json`: repeated warm/no-change checks, genuine
  never-before-compiled fixture rule edits, and current-content iteration.
- The canonical check's timestamped directory contains complete individual logs,
  executed test counts, stage wall times, aggregate elapsed time and child CPU.

First fixture setup uses warm shared dependencies and is not a pristine cold-engine
measurement. Changed iterations use new scoring values, not restored previously
verified source. Content-only checks leave Rust, device and shipping behavior
unverified. Local elapsed completion and summed command durations are reported
separately; they are not GitHub billed runner minutes. Nested helper builds may
not appear in the direct Cargo artifact counters. No universal build-speed claim
is supported by these measurements or the historical CI probe.

Native Linux package smoke uses Xvfb/software OpenGL, not physical devices.
No Windows host, real GPU/input/audio device or installer execution is certified
by Linux tests or Windows cross type-checking. Existing CI evidence does not match
this branch's final revision and is not substituted. Full Windows engine CI and
Windows installer/resource/launch gates remain required before release.

The scaffold ship command verifies a native package and optional local installation;
it does not produce an EXE installer. Existing Reverie Inno config/tests demonstrate
the supported separate path. New scaffolds still need installer configuration and
review before public distribution. This pass exposes that gap without replacing
companion delivery infrastructure.

## Next improvements ranked by observed friction

1. Complete installer preparation through the existing delivery path for new
   scaffolds. The practical installer question currently needs a separate game-owned
   Inno config/test after the package gate; this is the clearest remaining end-to-end gap.
2. Review and land the existing custom-replication work through its own evidence
   gates. Larger custom worlds currently face the reproduced single-frame ceiling;
   stock partial replication is a separate runtime. Avoid a competing implementation.
3. Add reusable native device review receipts for game-owned controls/cameras/audio
   through the existing capture/shipping workflow. Software captures and counters
   leave physical behavior unverified, and current learning entries repeatedly flag
   input capture, camera convention and audio state traps.
