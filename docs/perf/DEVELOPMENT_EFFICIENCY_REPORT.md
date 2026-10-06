# Development efficiency pass — 2026-10-06

Baseline: `83842f99a5a4df4dd98c3da46fb04f6878815175`.
Implementation revision: `7c147ff91c7406400accad2cb4faaf18efef7007`.
This report measures development work, not runtime speed. Existing dirty
`metrics.jsonl` data and the untracked Linux headless guide were preserved.

## What was already working

The repository already had a feature index, bounded context lookup, exact-test
iteration, optimized dependency profiles, rendering-free authority, browser compiler
caching, conservative shipping selection, game upgrade checks, scenario exploration,
snapshots and reusable clients. Those remain the foundation. This pass adds automatic
iteration selection and cheaper access to those systems; it does not create another
runtime, CLI registry or gameplay framework.

## Bottlenecks and changes

1. **Ordinary engine edits selected 32 shipping commands.** Automatic
   `check --changed --loop inner` now selects whole-library unit tests, indexed
   integration suites of owners and transitive consumers, and relevant Python checks.
   Integration expands feature modes. Shipping remains the default, with the original
   conservative full plan. Unknown ownership, build boundaries and undeclared content
   scenarios fall back to full checks. This partial index cannot certify shipping.
2. **Public use and implementation discovery arrived together.** Context level 1
   gives public guides, APIs, examples and iteration commands. Levels 2 and 3 expose
   contracts and implementation ownership only when needed. The feature map is
   derived from the existing index; `features --validate` verifies all 54 entries,
   paths, test suites and declared dependency IDs.
3. **Authoring required an isolated release/LTO build.** `be2.py map` now uses the
   shared headless `itest` binary, with Cargo checking freshness on each invocation.
   Distribution builds still use their isolated release outputs.
4. **Each maintained game manifest compiled its own copy of the engine.** The
   checker now shares its target directory across Cargo manifests and native authoring.
   Cargo fingerprints, original profiles and features decide reuse. Independent Python
   fixtures retain their output isolation. No engine crate split was needed for this gain.
5. **CI repeated checks already executed by the canonical runner.** Removed repeated
   rustdoc, headless, authoring, Python and game/portable commands in the engine lane.
   The historical successful Ubuntu lane spent 171 seconds on those duplicate steps.
   Both OS matrices, release builds, stock audio, Leo packaging/rendering, strict browser
   tests, clean public-source reproduction and the final aggregate guards remain.
6. **A new game could compile WASM but could not create a local browser package.**
   Local builds now hash actual inputs and record local provenance without requiring
   Git/origin setup. Publication still requires exact committed public HTTPS sources,
   anonymous retrieval, clean empty-target reproduction and deployment receipts.
7. **Iteration required knowing each game's scripts and lockfile setup.**
   `check --game DIR --loop inner|integration|shipping` reuses the project's tests and
   declared package gates. First-use metadata resolves the scaffold's seeded lock;
   existing game locks never auto-refresh. Whole-game evidence accepts empty bin/doc
   harnesses only when real project tests executed. Exact selections still reject any
   empty requested harness. Fresh-agent trials found both defects; real Cargo regressions
   now exercise them.
8. **Failures required opening long logs.** Structured recovery identifies observed
   files/spans, category, reproduction/doctor commands and retained successful stages.
   Full logs remain available. Reports count executed tests, fresh/built artifacts and
   child CPU/RSS where supported.
9. **Visual corrections repeated the complete browser shipping gate.** Fresh-agent
   inspection exposed three full gates for two small presentation corrections.
   `web preview GAME` now uses the same builder and browser scenario in one isolated
   profile, capturing desktop, portrait and landscape views. It preserves `dist/web`,
   declares incomplete input/storage/offline/update evidence and cannot authorize
   publication. Final shipping still runs both independent profiles and every gate.
10. **First-use discovery rejected ordinary requests.** Natural-language context
    queries now allow 500 characters, retaining bounded output. Source-checkout
    instructions use `be2.py map describe`, which builds the correct shared tool;
    packaged `author.py` remains supported.

## Reproducible seven-task benchmark

Run `python3 tools/dev_bench.py --root CHECKOUT --out OUTPUT.json` using
[development_tasks.json](../../tools/development_tasks.json). It measures discovery
packets and selected commands, without editing source or pretending to measure game
quality. `--execute TASK` runs one actual verification set, rather than seven full suites.
Implementation/discovery wall time, failed attempts and full task completion are null
where no agent performed that task. Exact model token telemetry was unavailable.

| Representative task | Context bytes before → after | Suggested files before → after | Check commands before → after | Cargo commands before → after |
|---|---:|---:|---:|---:|
| Gameplay condition | 2,262 → 441 | 4 → 2 | 32 → 5 | 29 → 1 |
| Asset tooling | 1,356 → 529 | 4 → 1 | 2 → 3 | 0 → 0 |
| Reusable path feature | 2,459 → 796 | 3 → 1 | 32 → 2 | 29 → 1 |
| Replication behavior | 2,612 → 628 | 4 → 2 | 32 → 5 | 29 → 1 |
| Portable starter | 1,491 → 605 | 3 → 2 | 32 → 3 | 29 → 1 |
| Package tooling | 2,573 → 834 | 3 → 1 | 32 → 3 | 29 → 0 |
| Upgrade tooling | 1,672 → 459 | 3 → 1 | 1 → 2 | 0 → 0 |

Total packet size falls 14,425 → 4,292 bytes (**70.2%**); suggested files 24 → 10
(**58.3%**). Lookup reads no implementation files. Planning remains milliseconds and
does not merit another cache. Small Python-only scopes sometimes gain an index
validation command; eliminating work must not eliminate necessary evidence.

This compares the old automatic shipping choice with the new automatic inner loop.
An expert could already manually choose a narrow exact test. The improvement is
making that appropriate selection discoverable and automatic, while retaining shipping.
The matrix is not seven fully implemented changes or an end-to-end speedup estimate.

## Build, test, resources and cache evidence

An actual automatic inner-loop path-feature check at the implementation revision
passed **477 unit tests / 2 commands in 27.812 seconds**: Cargo compilation took
4.93 seconds and tests 22.77 seconds. It reported 130 fresh / 1 built artifact,
68.213 child CPU seconds and 855,416,832 bytes maximum child RSS. This is focused
behavioral evidence; it is not equivalent to the shipping suite. There is no matched
baseline focused execution, so a build/test percentage is unavailable. A packaging-tool
change selected three Python/index commands and passed 245 executed tests in
20.028 seconds, with no Cargo build; child CPU was 19.980 seconds and maximum
child RSS 170,119,168 bytes. Windows-only cases remain skipped on Linux.

The final canonical Linux run at implementation revision `7c147ff` passed
**32 gates / 2,940 executed test cases** in **697.824 seconds**, with 730.032 child
CPU seconds and a 2,812,006,400-byte maximum child RSS. Cargo reported 3,309 fresh /
370 built artifacts (**89.9% reuse across reported artifact events**, not a
unique-artifact or test-result cache rate). Tests always ran. Default and headless
test commands took 159.692 and 180.321 seconds; those totals include compilation.
Cargo reported compilation portions of 28.81 and 47.25 seconds; the respective
harness durations sum to 129.62 and 131.87 seconds. Portable units compiled in
8.70 seconds and executed in 23.45 seconds. These are observed stage breakdowns,
not matched before/after compiler improvements.

The preceding complete shared-target run at `2d6b17e` passed 32 gates / 2,936 cases
in 737.269 seconds, with 982.629 CPU seconds, 2,820,243,456-byte maximum child RSS
and 3,530 fresh / 149 built artifacts (96.0% event reuse).

Historical recent successful full runs took 407.154 and 447.876 seconds in warmer
conditions. Both new runs were slower: they included template-induced recompilation
and first-use game/profile builds; the earlier run also had benchmark contention.
During the final run a point-in-time OS sample showed high I/O pressure, with
`some avg60=72.92` and `full avg60=67.33` percent; this does not establish its cause
or describe the entire run. These are not matched samples;
there is **no demonstrated full-suite wall-time speedup** from this comparison.
Nor is the removed 171-second CI duplication a measured new CI run saving.

An earlier run, stopped deliberately after exposing isolated game targets, rebuilt
Lantern Run headless in 192.803 seconds. The shared-target run took 64.092 seconds for
that stage. Cache conditions differ, so the apparent 66.8% reduction is an observation,
not a controlled compiler-speed result. The stronger evidence is Cargo's engine library
fingerprints: subsequent Pocket Breaker, Orchard Watch and Lantern Grove headless and
client checks report the engine library **fresh**, with no engine rebuild. Their headless
checks took 1.498, 1.532 and 2.404 seconds, including tests and game compilation.

The normal dependency graph remains 97 packages headless, 137 default and 40 browser.
Headless excludes Macroquad; browser excludes Rapier and Quinn. No runtime capability,
profile assertion or feature boundary was removed. Existing browser compiler caching
uses content/compiler hashes; new sharing reuses Cargo fingerprints. No passing test
result is cached. Cheap context/dependency plans were deliberately not cached.

Local concurrency remains bounded to the existing independent Python lane beside serial
Cargo. No additional build fan-out was introduced on this four-core host. CPU/RSS before
and after are not matched measurements; RSS is an OS child high-water mark, not total
simultaneous machine memory. Windows resource telemetry and physical-device measurements
are unavailable.

The final review found only 1.3 GiB free on the internal disk. Two obsolete isolated
generated game target directories were removed after switching checks to the shared
target, leaving 4.9 GiB free; source, packages and evidence were retained. The newly
attached 119.5 GB flash drive initially required administrator authentication; the user
then ran the supplied format/mount command. It is ext4 at `/mnt/blueengine-usb`.
Completed benchmark games, packages and evidence were archived there. The active
compiler cache remains internal; storage cleanup is not a measured compiler-speed gain.
After verification, the inactive Windows GNU target cache was archived and verified
with `tar --compare` before removing its internal copy: 1,319,895,040 archived bytes
in 26.274 seconds. It can be restored with `tar -xf` into `target` if needed. Cargo can
also rebuild it without the drive; no source, release package or engine capability was
removed. The active native cache was not moved, and no build now requires the USB drive.

## Fresh-agent trials

The same fresh-context model/task created a distinct portable 2D game with five
collectibles, a locked exit, timer loss, restart, snapshots and behavioral assertions.
The baseline took **593.98 seconds**, read 22 files (approximately 114,690 loaded text
bytes) and passed five native tests. Local browser packaging failed first without Git
and then without a public origin, so desktop/mobile verification never ran. There is no
baseline time to a fully verified browser artifact and therefore no honest end-to-end
percentage to calculate.

The first after trial at `56a2692` reached its corrected, verified browser package in
**554.651 seconds** (measurement/report writing finished at 694.811 seconds). It passed
seven game tests and all strict desktop/mobile package gates. Authoring/scaffolding took
20.660 seconds versus the baseline's 118-second release-tool build; this observed 82.5%
reduction mixes profile/cache conditions and includes scaffolding only in the after time.
Warm focused gameplay tests then took 0.106 seconds; the preceding compilation/test run
took 66.186 seconds. The agent recovered setup errors plus the two workflow defects fixed
above, and corrected a duplicate outcome panel before rerunning the shipping gate.

It read eight complete public guide/template files (67,996 bytes), seven game files
(33,507 current bytes), and one bounded engine-source grep for the restart key. Total
unique file reads fell 22 → 16; comparable full/range text inventories suggest roughly
11% less loaded text, but byte counters are not exact context telemetry. Repeated probe
commands remained: 46 grouped shell blocks were recorded. Baseline command inventory
has a different grouping basis, so no command-count speedup is claimed. Its 11 measured
stages consumed 331.542 child CPU seconds, maximum child RSS 1,457,304 KiB; unwrapped
commands are excluded and source/cache conditions differ from before.

The resulting games retain the same modest single-room collection-game scope and
distinct coherent presentation. Parent inspection agrees with the agent's assessment;
the after game adds isolated desktop/touch verification beyond baseline native evidence.
See [baseline frame](development-efficiency/before-frame.png),
[after gameplay](development-efficiency/after-frame.png) and
[after portrait touch](development-efficiency/after-mobile.png). These are automated
captures, not proof of physical-device usability or subjective audio quality.

A later fresh-checkout trial at `9283d2` took **657.170 seconds to full verification**,
read 88,605 source/document bytes (22.7% below the baseline inventory), and passed
seven game tests plus both complete browser profiles. It used 28 grouped commands,
15 polling calls and three full shipping builds. The earlier first-use lock/harness
defects were absent. This slower trial is retained rather than selecting only the
faster trial. Repeated visual shipping checks motivated the preview command.

For that same game's deterministic route, one-profile browser preview took
**34.025 seconds / 5.018 child CPU seconds / 241,491,968-byte maximum child RSS**.
A warm complete shipping build took **88.780 seconds / 11.967 CPU seconds /
239,214,592-byte maximum child RSS**. The preview's browser-only iteration was 61.7%
shorter and used 58.1% less measured CPU, with similar maximum child RSS. This compares
different evidence scopes intentionally: the saved work remains mandatory before
shipping. A complete first preview including game compilation took 59.293 seconds,
67.340 CPU seconds and 837,550,080-byte maximum child RSS. It preserved the previous
shipping artifact; publication rejected its partial evidence. Compiler cache was
warm for the subsequent full build; no passing test result was reused.
The final real-browser preview regression also passed in 53.008 seconds, exercising
the complete preview command, artifact preservation, six captures and shipping refusal.

The final fresh checkout/context at implementation revision `7c147ff` created
**Midnight Reliquary** and completed verification/inspection in **711 seconds**;
117.295 seconds of report writing are separate. It used focused game tests, one
visual preview and two complete shipping builds. The second shipping build followed
a README correction; runtime package identity was unchanged, so that repeat was
unnecessary in retrospect. It inspected 12 frames, including a real-input timeout
loss at tick 2,700, and passed five game behavior tests plus identity validation,
12 desktop checks and 11 independent touch checks.

It opened 16 source/document files versus baseline 22; whole-file reader inventory
was 80,824 bytes versus baseline approximately 114,690 (29.5% lower). Two public
interface snippets had unrecorded byte counts, and one response was truncated; this
is not exact model-context telemetry. It recorded 29 grouped commands, 20 instrumented
invocations, four instrumented operator mistakes and two additional nonzero diagnostic
groups. Instrumented child CPU was 325.161 seconds; maximum child RSS 1,459,724 KiB.
The first focused game build took 81.017 seconds despite warm authoring tooling:
standalone games retain their own Cargo profiles and dependency checkout fingerprints.
The root instructions now state that distinction explicitly.

Parent inspection of [final gameplay](development-efficiency/delivery-frame.png) and
[final touch layout](development-efficiency/delivery-mobile.png) found coherent
presentation and readable game state. Baseline packaging was blocked and the final
trial took longer than its partial baseline run: **the requested end-to-end speedup
acceptance is not demonstrated**. The measured context/selection/reuse gains and
smaller-agent task improvement are real; they must not be substituted for that missing
matched full-game result. A next trial needs identical completed verification scopes,
matched cache conditions and separately timed game design versus engine workflow.

Two fresh `gpt-6-luna`, medium-effort contexts also changed stock Three Switches to
require any two switches while retaining all three interactables. Both created and
executed physical winning and blocked-exit scenarios without Cargo. The clean baseline
took **156 seconds**; after took **88.395 seconds**, an observed **43.3% reduction**.
Baseline needed targeted scenario implementation navigation and opened the large map;
after used public scenario guidance and did not open map contents or engine implementation.
Grouped command inventories are not exact subprocess counts. After still had four
recovered usage failures, so that failure rate was not eliminated.

This is one pair, not a statistical model-quality claim. An earlier baseline trial had
incomplete timing, and another accidentally read updated root guidance; neither is used
for the primary comparison. Nominal file sizes differ from loaded text, especially for
truncated responses. Exact context tokens and CPU/RSS for these small trials are unavailable.

## Windows CI failures

The failing [run 37472869793](https://github.com/kevstermcgee/BlueEngine/actions/runs/37472869793)
had a successful Ubuntu engine lane and two Windows Python assertions:

- Smoke pruning counted retained `.log` files as capture directories. It now counts
  directories, with a regression proving pruning retains failure logs.
- Upgrade verification compared Windows short and long path spellings as strings.
  It now compares filesystem identity with `Path.samefile`.

Both aggregate `test` jobs correctly failed because the Windows engine matrix failed.
That guard remains; their red status was not an independent Ubuntu test defect.
Linux regressions pass. A new Windows runner result is still required; local Linux
success does not certify the real Windows shell tests.

## Reuse, capability and remaining work

The design audit distinguishes defects from preserved complexity:

| Observed design issue | Evidence and disposition |
|---|---|
| Incomplete first-use game verification | Real fresh games exposed lock resolution and empty doctest rejection; fixed with real Cargo regression fixtures. |
| Unnecessary authoring/distribution coupling | Release authoring builds and per-game target isolation were observed; shared `itest` tooling/targets now avoid that work. |
| Incomplete verification ownership | The feature graph is declared, not compiler-derived; unknown paths and content routes remain conservative full checks. Extend the index with real scenario coverage rather than guessing. |
| Authoring embedded in runtime library | `src/viewer/newgame.rs` includes scaffold scripts and templates; edits invalidate library artifacts. A future boundary needs compatibility and before/after build evidence. |
| Native portable dependency coupling | `Cargo.toml` selects physics/transport/TLS dependencies for every native target. Their optional isolation is a future measured design task; existing APIs remain available. |
| Common systems apparently duplicated | The audit found expected generated shipping infrastructure and small fixture/protocol clusters. Existing shared mechanics already serve recent games; no speculative framework extraction. |

The public `vesper3d` library name, retained native/stock/custom simulation paths,
transport boundaries, custom clients and full shipping gates are compatibility or
capability contracts. Their age or apparent complexity alone is not a defect. No
Rust runtime code was removed or rewritten during this pass.

The duplicate audit scanned 228 files across 16 recent projects and found 18 clusters.
The largest are expected scaffold shipping/build copies. Recent portable games already
use shared input, camera/viewport, pause/restart, snapshots, storage, audio and mobile
controls. Stock rules compose counters, interactables, timers and conditions. Existing
determinism, snapshot continuation, physical scenarios, transport fixtures and browser
gates are the reusable test vocabulary. Public guides now expose these paths and shared
HUD/menu ownership. We did not extract another gameplay framework from superficial
similarity or restrict custom simulation, shaders, physics, networking or custom clients.

Full Linux verification covers simulation, physics/save compatibility, protocol/netplay,
portable modes, maintained games, Clippy, rustdoc and authoring. Strict browser/game
shipping evidence is recorded separately. Real Windows shells, all remote CI release
lanes, physical mobile/controller/audio performance and external publication need their
own evidence. Local package verification makes no deployment claim.

The largest remaining build coupling is authoring/template `include_str!` inside the
single engine library: a documentation/template edit can invalidate heavy runtime/test
artifacts. The next highest-leverage change is to measure and design an authoring/runtime
boundary that preserves public compatibility, followed by indexing content-to-scenario
routes so ordinary asset changes can narrow safely. Native portable builds also retain
Rapier, Quinn, Tokio and TLS dependencies because they are unconditional native
dependencies. That is a measured dependency boundary to investigate, not evidence
that those capabilities are unnecessary. Native CLI help conventions and wrapper
command coverage still cost failed attempts. Do not replace conservative fallback
with guesses to make these numbers smaller.

Structured evidence is in [development-efficiency](development-efficiency/).
