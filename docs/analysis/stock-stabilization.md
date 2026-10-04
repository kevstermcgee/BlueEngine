# Deployment, discovery and stock authoring stabilization

Starting engine revision: `4136080c7f42c8c2becf7b9597bf2de11e5c44a3` (remote main when inspected).
Work was performed in an isolated checkout; unrelated local checkouts were preserved.
The final revision and exact gate results accompany the delivered commit/CI run, rather than assuming that an
earlier run validates later edits.

## Findings and changes

All reported blockers remained at the starting revision: Unix `write` used the wrong pointer type, CI omitted
the learning suite, the rejected-source shortcut skipped incomplete rollback recovery, historical server/hub
workarounds competed with the supported path, and discovery floors protected only the main task set.
The compiler declaration/call now use `c_void`/pointer conversion while retaining the signal handler's
allocation-free, async-signal-safe write path. CI includes learning without removing either OS, headless,
Clippy, rustdoc, release-build or other existing gates.
The first full Windows run also exposed five pre-existing hub fixture failures: fictional Unix-rooted paths
were compared with drive-qualified Windows config paths. The fake InfoSource now normalizes those fixture
identities on Windows; production path resolution and all hub assertions remain intact.
Two real-process Windows tests also needed an explicit initialization barrier/readiness wait before measuring
UDP responses; the broken-server fixture now uses an executable whose `--info` fails on both OSes.
The upgrade drift fixture now copies exact template bytes, preserving checkout line endings rather than
normalizing them while testing a byte hash. Windows process-cleanup verification requires permission to use
`taskkill`: the sandbox denied it, while the unchanged cleanup test passed outside the sandbox. The final full
local workflow is run with that permission; the earlier sandbox failure is not reported as a pass.
The post-push rerun also exposed an intermittent Linux temporary-repository cleanup race with Git background
maintenance. Fixture Git commands now disable automatic maintenance/GC, so deletion cannot race their own
background lock cleanup. The engine-warning assertions and production Git behavior are unchanged.
Another post-push Windows rerun exposed repeated wall-clock timestamps colliding in generated-game
report directories. Engine/game reports now reserve a unique suffix atomically, with forced-clock
regressions preserving both reports and the original failure log.

Rollback decisions verify the restored SHA-256 before skipping and resume incomplete installation, activation
and readiness. Recovery uses only a validated previous artifact matching the receipt; absent or inconsistent
copies fail clearly without rebuilding the rejected source. Fixtures cover stopped hub recovery, reload and
readiness failures, both sides of rename, actual failed rename/retry, missing or modified installed artifacts
(including preserved size/mtime), missing/corrupt previous artifacts and per-game isolation. Existing candidate,
dependency, source-change-during-build and previous-retention checks remain. These are updater fixture results,
not evidence of a live deployed hub or real network rollback.

L-014 and L-015 are promoted in place with supported `cli::serve`/hub advice, retaining the historical notes.
Recording instructions and generated game instructions include retrieval terms and a follow-up context query;
omitting keywords emits an archive-only warning. New authoring, sequencing and recovery lessons are retrievable.
Discovery regression checks score all four previously tuned sets and protect individually retrieved expected
targets. The screenshot/playback case finds `custom_simulation` and `src/viewer/devkit/playback.rs` again through
accurate module descriptions and feature keywords. Verification/report generation do not append evaluation
history. Current scores and limitations are in [the learning report](../learning/REPORT.md): these development
fixtures do not measure actual token savings, development speed or game completion.

## Bounded stock milestone

Night Shift at the Observatory was unavailable in the checkout. The explicitly identified substitute is
[Observatory Night Watch](../../assets/games/observatory/README.md), a stock content fixture in the existing engine
executable. It opens a shutter, routes around a baffle, requires three calibration presses, transmits an
observation, and can lose on battery depletion then restart into a successful run. It is not a newly published
or separately shipped program; no live server deployment or manual game publication was performed.
The authorized main push runs the repository's configured workflows.

The reproducible recipe uses blueprint/build, add_box, add-interactable and translate. A fractional translate
exposed another concrete bug: independently shifted bounds rounded differently from the shifted visual center.
The existing operation now preserves exact agreement for already matching, jointly selected records, and still
rejects inconsistent content. No hand recalculation or weakened validation is involved.

Opt-in routed walks use current collision and the active profile, with bounded waiting/replanning for moving
gates. `wait_ticks:1` separates overdue repeated presses across executed simulation steps without changing
authority's interaction coalescing. The successful 341-step scenario is independently replayed through the
shared simulation. The 1542-step acceptance run asserts battery loss at 1200, restart/reset at 1201, and later
success. The never-opening gate variant remains abstractly winnable and honestly physically unproven.
Static map lint remains a description of initial collision. Opt-in game-aware lint verifies matching content
and authored spawns, retaining an initial unreachable finding as information only for each individual target
observed within authoritative interaction reach/line of sight in a passing scenario. A loss-only run leaves
unresolved errors; failed, mismatched or spawn-overriding scenarios are refused. No blanket mover exemptions
are used. Generated content checks use the first supplied scenario for this evidence when movers exist.
Later scenario reachability applies to the demonstrated run, not all possible states.

Stock presentation adds optional objective/outcome wording, visible labeled/formatted counters, RGBA palette
and bounded HUD scale/margin/width/crosshair. It reads authoritative GameState and retains defaults for existing
JSON. Direct Rust GameDocument literals need `presentation: None` unless opting in. No gameplay rule is duplicated.

## Rendered and behavioral evidence

The Windows stock executable executed the same verified scenario through `GameSession`, with actual GPU capture.
The images were inspected: readable amber objective and battery/three-step calibration HUD, coral battery-loss
wording, reset HUD, mint transmission-success wording and the correctly titled stock pause menu. No clipped
wording was observed at the captured 960x600 size. The renderer/session regression also matches the headless
scenario's final checksum. This evidence exercises scripted control intents; it does not prove live keyboard,
controller, fullscreen or network behavior.

| Evidence | Result |
|---|---|
| [Authoring recipe](../../assets/games/observatory/author.py) and [operation records](../../assets/games/observatory/content/authoring-evidence.json) | Creates three canonical controls and a fractional edit; strict validation passes |
| [Winning scenario](../../assets/games/observatory/content/win.json) | 341 executed steps, physical completion |
| [Loss/restart/win scenario](../../assets/games/observatory/content/loss-restart-win.json) | All four assertions pass, final checksum `2ee88ad4f8858336` |
| [Negative gate](../../assets/games/observatory/content/closed-gate.json) | No verified/written physical winning scenario |
| [Start](../../assets/games/observatory/evidence/world.png), [loss](../../assets/games/observatory/evidence/loss-0.png), [reset](../../assets/games/observatory/evidence/reset-1.png), [win](../../assets/games/observatory/evidence/win-1.png), [menu](../../assets/games/observatory/evidence/menu.png) | Actual rendered frames inspected |
| [Capture summary](../../assets/games/observatory/evidence/capture.json) | State transitions, battery countdown values and screenshot digests |

Required final verification: `python tools/be2.py check --changed --plan`, then `check --changed`,
`python scripts/publish_games.py check`, and the unchanged complete Linux/Windows Engine checks matrix on the
exact delivered commit. Check reports/CI distinguish passed, failed, skipped, pending and unavailable work.
No prior-revision or fixture-only result is substituted for a required final gate.

## Verification speed follow-up

CI now invokes the same full checker as local development, including learning and upgrade
tests, with full Git history for ancestry fixtures. Tests, Clippy, rustdoc and fresh native
authoring share `itest`; release builds and both OS/feature modes remain required.
Compiled dependencies are cached, every test still executes, and superseded runs on a ref
are cancelled without cancelling the other OS on failure. Sandbox/generated-project builds
share a target directory for compatible dependency reuse. Complete reports/logs are uploaded
on failure as well as success.

One reviewed independent Python batch overlaps serial Cargo work; native integration stays
ordered. Barrier fixtures prove overlap and that failures in either lane remain failures,
pending gates stop, and all started work joins before reporting. `--serial` restores sequential
execution. New report-directory clock fixtures preserve both reports and original failure logs.
The unchanged authority/input/rendered evidence above remains applicable; this follow-up changes
verification and generated-game reporting, not gameplay or rendering.

The passed fixed-tree local measurement is 307.773 s versus the earlier warm 439.681 s,
30.0% less wall time in one observation, with additional regressions and no reduced gates.
The intermediate run invalidated by an in-flight template edit failed the upgrade drift check
and is discarded; it appended no performance measurements. See [performance data](../perf/README.md)
for conditions and the first profile-warming run. Exact delivered-revision CI remains the final
cross-platform/release/package evidence, rather than inferring a pass from these local timings.
