# BlueEngine next phase — verified baseline and proposed sequence

Phase 0, 2026-10-13. Audited freshly fetched main
`579b90868e7e20005f874d2689e35a5f8a23e559`; this is still the outside review's
revision. The original dirty couch-games checkout was preserved. This PR changes
documentation and learning records only. The sequence and completion criteria are
in [ADR 0051](adr/0051-the-next-phase.md), **proposed for owner review**.

## What the review gets right, and what changes its interpretation

### A — copied project tooling

There are 12 games under games/ plus Leo under assets/games/. For those 13
projects, ship/check/build.rs/dev have 4/2/2/3 byte variants; 2/3/9/3 respectively
match their current templates. Looking at games/ alone gives the review's three
shipping variants: nine older, two current and customized Reverie. The nine older
ships differ by one diagnostic line, not a broad shipping-policy rewrite. Ten
checkers still lack the current discovery/refresh code. Reverie's extra soundtrack
preparation is an intentional current-main fix and must survive centralization.

Physical Python lines: games/ 46,235 in 57 files; with Leo 50,315 in 64; tools/
plus scripts/ 24,415 in 58. Common ship/check/dev/project families account for
45,699 game lines; other authored game Python is 536. The historical dupes report
labels a 13-copy template cluster expected output, but no current synchronization
gate proves it stays current. Thin shims remain valuable even where hashes differ
by only one hint.

**Already implemented:** newgame::write_shipping_files emits game.project.json
and scripts/project.py for every starter, distinguishes the custom-sim starter
from legacy-native runtime, and current game_check finds itest/CARGO_TARGET_DIR
and exposes --refresh-project. The six-starter matrix regression exists in
tools/test_game_check.py. Rift Delver's missing-metadata feedback describes its
older 9189d68 baseline; L-131/L-132 and the hardening audit need current guidance
scoped separately from stale consumers. Discovery presence alone is not proof an
override binary was built from matching source.

**Still open:** Lantern Run, Lantern Grove, Orchard Watch and Pocket Breaker
AGENTS positively require WASM/Chromium near their native-only retirement policy.
Migrate those instructions and old helpers; do not recreate the canonical fixes.

### B — one package and heavy dependency edges

Tracked src Rust is 86,366 physical lines in 176 files; viewer is 73,155 in 143.
Viewer has 75 immediate entries: 67 Rust files and eight directories. A lexical
balanced-brace inventory reproduces 28 functions over 200 lines, including
blue-engine::main 1,703 and be2-tools::run 1,110. It includes tests and is not a
Rust AST or proof all long functions need refactoring.

Rapier3D, Ring, Rustls, Tokio and Quinn are nonoptional. Lantern Run's 237-line
simulation disables default features yet its resolved normal dependency graph
includes all five. Cargo tree was executed without compiling; this establishes
membership, not compile time. Presentation/offline/model/schema feature gates
already exist, so the engine is not completely ungated.

Icon is 4,715 lines, including tests from line 3,673; synth is 4,118, including
tests from 2,311. The deterministic icon codec/math contract is real. AudioProject
directly uses synth DSP and its 23 presets: those are authoring and generator
layers, not two independent audio engines. Extract/gate ownership before changing
deterministic algorithms. No new build-speed result is claimed by this audit.

A dependency-free, no-build Cargo probe established that a shared workspace lock
can contain unused optional network members while a selected game graph excludes
them. Standalone narrow and defaults-disabled facade consumers omitted the unused
dependency in the probe. The real acceptance checks both Lantern's standalone
lock and selected normal graph; lock membership alone is not compilation evidence.

### C — discovery and documentation

An explicit lexical proxy finds 1,488 documented / 2,403 bare pub fn/struct/enum/
trait/type/const declaration lines, 61.9%; structs 240/442, enums 45/89, kit plus
devkit 561/599. Missing counts for game/fps/arena/controller are 47/35/33/32.
The review's near-identical numbers use an unavailable counting implementation;
this difference is not a measured documentation improvement. The proxy excludes
fields, reexports and several Rust syntaxes and includes unreachable declarations.
Define reachable exported API coverage before enforcing the proposed 95% gate.

Actual Python context for “make the player jump higher” ranks simulation_contract,
custom_simulation and arena_shooter_foundation; movement and the jump_height schema
answer are absent. Exact jump_height also fails to surface the field. Python is
already IDF-ranked; nonexact results are always labeled low confidence. Native
search source replay is alphabetical all-word substring matching: movement is
fifth among nine whole records, approximately 16,918 reconstructed output bytes.
No verified baseline native binary was available, so this is a **source-algorithm
reproduction**, not an executed native command. browser_games still appears as an
active high-confidence exact capability, although browser commands are retired.

The measured prescribed initial routes are 72,980 UTF-8 bytes for custom 3D and
46,821 for 2D, including start packets (4,733/4,477) and selected mandatory guides.
Author template source is separate: 30,232/5,343 bytes. These are defined readsets,
not actual model usage. Existing context has a 6,000-byte multi-match pruning guard,
not a hard full-packet bound: it retains a lone oversized match and adds other fields
afterward. read_first allowance five and learned allowance five/600 characters also
exist; complete start/guide intake is not budgeted. “No budgets” is too broad.

Start for a custom-sim networking question has no learned traps while direct
context returns three relevant hints. The separate unmerged replication pass
already fixes initial guidance/public-guide selection; reconcile it first.

The existing source navigator stops brace-span scanning after 100 lines and does
not recognize async fn. It cannot substantiate the large-function inventory;
that required the explicitly labeled lexical fallback. Repair these navigation
limits with a bounded correctness regression separately from search ranking.

### D — policy seams and supposed duplicates

Stock InputFrame still has fire_wrench/fire_pistol, and CharacterKind still names
Scientist/Feta. Custom NetGame inputs are already game-owned, and Controller::for_profile
already provides generic data-driven movement. Weapon/wrench policy moved into
stock_demo; old modules are forwarding compatibility exports, though stock content
is still compiled unconditionally. Preserve adapters before evolving stock wire.

The 628-line DFHB Deadfall shim is a shipped-client compatibility path with tests
and a refuse policy under ADR 0037. The configurable current default endpoint is
blue-engine.duckdns.org:4100, not the review's deadfall hostname. Stock and custom
netplay share transport/codec/session services but have distinct replication and
authority adapters; neither server's capacity implies the other's. Templates and
scaffold_stock are two source-generation styles behind one dispatcher/common
shipping writer, not independent runtimes.

GameSession has nine as_ref plus two as_mut game unwraps. Its private loaded-game
world always supplies Some; generic HeadlessWorld validly constructs None and save
restoration checks matching presence. No panic was reproduced. A private validated
stock wrapper could improve that invariant; globally removing Option would break
the valid generic-world escape hatch.

### E — vocabulary, history and checksum scope

BlueEngine is already the product/branding name. be2 package/binaries and vesper3d
imports are deliberately retained under ADR 0004; several other names are history
or retired launchers. Two accepted ADR files are numbered 0003. GLOSSARY mistakes
the default eight players for the configurable maximum 1,024 and denies the now
implemented server/manager/hub. Neither configured maximum proves game capacity.

Among 139 Markdown docs, browser|wasm matches 34; adding web gives 35, not 70.
Many are valid retirement, native caveat, feedback or historical evidence. Preserve
those facts while removing active misleading routes. Under a defined six-extension
source count, docs/archive has 23 files/2,822 physical lines; 11 Rust files are
outside archive, including the live Cargo example docs/perf/model-import/compare.rs.
Move that example with Cargo/harness paths. Preserve useful inline working examples.
Sixteen Blue documentation files match red[-_ ]?engine comparison prose; no other
engine repository, source, executable or module was opened or imported.

The Sep30 feedback really repeats two Blue checksum pairs. Its original Blue
fixtures/receipts are not tracked, so **the historical cause cannot be established**.
Current checksum does include GameState counters/bits/outcomes; it is not a game
content fingerprint. It omits saved GameRuntime timer countdowns and previous
per-player zone occupancy. A current-source Python replay produces the same hash
for modeled countdowns 3 versus 1, or previous occupancy 0 versus 1, despite modeled
next-step differences. This is source-model evidence, not a live engine regression
or proof it caused the historical table values. Saves explicitly preserve those
fields. Clarify checksum scope/version with a dedicated public regression before
changing hashes; do not “correct” owner feedback on a guess.

### F — CI and performance

There are 13 expanded main-CI jobs. The canonical plan has 40 top-level commands,
36 directly invoking Cargo; nested fixture/tool builds add more. The review's
approximately 45 total lacks a recursive-count method. Two engine two-d Clippy/
rustdoc commands use implicit dev despite broad itest wording; standalone games
intentionally use their own profiles. Engine/default/headless releases and game
package/render gates remain meaningful separate outputs.

Hosted timings already exist in docs/perf/README.md. Current main
[run 38157567010](https://github.com/kevstermcgee/BlueEngine/actions/runs/38157567010)
passed all 13 jobs: workflow creation→updated 1,956 seconds; summed job duration
6,894 seconds, **114.9 unweighted runner-minutes, not billed minutes**. Linux engine
took 1,360 seconds (full check 1,000), Windows 1,946 (full check 1,443); container
154. These are one run, not a paired optimization result. Raw timestamp methods
are in [the baseline data](perf/NEXT_PHASE_BASELINE.json).

The documentation-only pre-optimization [PR probe 38416714540](https://github.com/kevstermcgee/BlueEngine/actions/runs/38416714540)
passed all 13 jobs at source `518d6c053f3a69bb28473fa427ab3065305f8936`;
CI checked out merge `74ab9451c617f53a00329cc8b81ee3b52e87905a`.
Creation→updated was 2,003 seconds (33m23s), summed job duration 7,077 seconds
(117.95 unweighted minutes, not billed). Linux engine took 1,135 seconds, full
check 817; Windows 1,992, full check 1,516. Both engine/Leo caches hit; all six
IdeaForge/Wormhole/Sway caches missed. Ubuntu 24.04.5 image 20261004.327.1 and
Windows Server 2025 image 20260925.250.1 used rustc 1.99.0. The baseline records
exact stage commands, timestamps, runner/toolchain metadata and cache keys.
This is one unchanged-workflow observation, not a paired speed result.
Cancelled duplicate push run 38416523391 is excluded from these totals.

Action logs show an actual cache-key mismatch: engine/Leo cache setup omits
CARGO_TARGET_DIR, IdeaForge includes it in the hashed environment; identical
lock/shared-key suffixes still produce incompatible prefixes and IdeaForge misses.
Profile incompatibility may add work, but is not the sole established cause. Games
have save-if:false; that avoids competing writers and is not automatically wrong.

There are five pinned Mesa setup blocks across workflows, three in main CI, each
taking only 3–4 seconds in this run. Docker lacks a persistent external cache but
does not push an image. Main src changes schedule the BlueEngineGames source
exporter; this is source synchronization, not automatic installer publication.
Changing that policy needs release/catalog acceptance rather than a speed claim.
Docs-only feature pushes/PRs run CI without deployment or companion-repo export.

Ten local check_total rows span approximately 308–556 seconds on a 12-core Windows
host and 464–4,654 on N97 Linux, across different dirty revisions/cache/flags; they
are not controlled comparisons. Five benchmark tests exist: two print timing
without timing limits but do assert correctness. The 4,000 µs mean sim-step ceiling
is a two-player/500-tick regression budget, not demonstrated capacity or a full
frame budget. No benchmark or speed improvement was measured in this phase.

### G — original-looking games

Eight of 12 games have only icon/identity assets, not ten. IdeaForge adds rule data;
Reverie has fonts/eight authored scores, Wormhole four authored adaptive banks, and
Sway a media fixture/logo. No bounded games/ Rust/TOML/JSON/README scan found model,
catalog, starter-map, loft/sweep/rounded_box adoption. Sandbox is a real catalog
consumer outside that population; Leo uses named audio banks. Icon-only folders
do not prove identical visuals: game-owned shaders/procedural art live in code.

Named AudioProjects, optional stock palette/HUD wording, CC0 model import and shape
primitives already exist. Portable client effect cues are still fixed and declared
bank effects rejected; the pending creative branch addresses much of that seam.
Scaffolds do not offer the existing importer as an authored step. Lathe/extrude/
path-array helper names were not found, but some operations can use existing sweep/
section/path APIs. A concrete repeated need should justify each addition.

Wormhole's empty overlays, Sway's streamed media, Rift's coherent primitives and
the audio guide's silence/effects-only choices override forced defaults. The
two-consecutive-game criterion needs a frozen configuration, no cherry-picking,
declared captures/listening/rubric and actual native package/input evidence; byte
differences alone cannot establish original, audible or enjoyable presentation.

### H — learning signal

Before this audit's records: 141 entries, 76 promoted, 51 open, 12 duplicate and two
wontfix. Ten nonzero token annotations have mixed provenance; no entry has a
measurement_ref. Missing provenance does not prove fabrication or zero effort.
The generated learning REPORT still snapshots 65 entries, while its generator
labels mixed annotations “Measured tokens.” Preserve original feedback, refresh
derived status later and label measurement/estimate/unavailable honestly.

The existing read-only evaluation executed 77 context subprocess cases: feature
recall 1.000; 67/77 met all expected feature/path requirements. Base tasks met
34/41, each later set 11/12. These tuned regression fixtures measure retrieval,
not game completion, independent agent cohorts or billed token savings. IdeaForge
already collects exposed numeric usage/whole-run receipts and marks missing usage;
do not build another collector. Session telemetry is unavailable for this audit.

## Sequence mirrored from ADR 0051

| Phase | Work and measurable completion |
|---|---|
| 1 | Shared project implementations with small shims, upgrade migration and explicit deliberate overrides; games Python <2,000 lines and no template dupe cluster. Preserve game hooks and prove installed packages carry no engine tooling. |
| 1 | Extract each compatible leaf/seam directly to its layer in a bounded PR, without rule changes; optional dependency selection is separate. Offline Lantern normal graph **and standalone lock** exclude network/physics; compatible legacy feature imports/native profiles pass; matched fast sim-edit build is recorded. |
| 1 | Existing/fresh hosted probe first; measured cache/profile reuse before tuning. Preserve Linux/Windows guards, release/package/container checks. Record matched cold/warm method, median critical path and unweighted job duration. Publication policy gets its own decision. |
| 2 | Core-first meaningful public documentation, then defined exported coverage ≥95%; ranked native/Python schema-first stock responses and no active retired-browser capability. Existing recall floors and new feedback cases hold. |
| 2 | Actual initial 3D documentation/packet route ≤30,000 UTF-8 bytes; required source/metadata counted separately, starter author hooks ≤4,000. Enforce packet/reference budgets; three controlled cold author trials per platform/route, proposed 2D ≤5 min and 3D ≤10 min. No current result is claimed. |
| 2 | Generic action/data adapters, stock/legacy gating and sunset, canonical source-generation/authoring ownership, checksum-scope regression, naming/ADR/glossary/history fixes. Keep generic worlds, both server contracts and compatibility. |
| 3 | Reconcile creative hooks; optional style/sound data and existing importer scaffold step; add only measured geometry needs. Two consecutive unprompted, uncherry-picked IdeaForge games pass the declared owner identity rubric and both native gates. |

One PR per bounded item; structural moves and behavioral changes are separate.
Each real implementation decision gets its ADR. Feedback wins the conflicts
listed in ADR 0051. The prior manual-upgrade deferral does not excuse copied drift;
the current compatibility facade does not permit an abrupt import rename.

Pending, not landed: creative-identity at 41676f8 includes UI/font/effect/style work;
improve/custom-replication at 1807420 includes bounded snapshots, initial guide
retrieval and task-input corrections. Reconcile their patches with full CI before
reimplementing them. This Phase 0 PR includes neither branch's code.

## Methods, friction and verification state

Counts use tracked file bytes/physical splitlines and explicit scopes; copy variants
use SHA-256 plus reviewed diffs. Cargo tree establishes actual dependency membership.
Source queries use selected context packets and symbol ranges. Native search and
checksum models are labeled source replays. CI timestamps/cache excerpts are
read-only hosted observations; no historical private sessions were opened.

Representative commands:

```sh
python3 tools/be2.py start "Phase 0 next phase: verify review A-H against HEAD and owner feedback, write sequencing ADR and REPORT only" --kind engine --path docs/REPORT.md --compact
python3 tools/be2.py context game_scaffolding --level 3 --compact
python3 tools/be2.py context "make the player jump higher" --level 2 --compact
python3 tools/be2.py context browser_games --level 2 --compact
cargo tree --locked --offline --manifest-path games/lantern-run/Cargo.toml --no-default-features --edges normal
gh api repos/kevstermcgee/BlueEngine/actions/runs/38157567010/jobs?per_page=100
python3 tools/be2.py check --changed --base 579b908 --plan
```

These audit/plan commands are not a new passing engine check. The documentation PR
must run the existing full Linux/Windows CI before a merge request; main's green
run does not certify the changed documents/ledger. No visual/control/audio/package
behavior changed in Phase 0. The completed probe covers its source SHA above,
including all 40 canonical stages on each platform; it does not certify the
subsequent measurement/ledger freeze. Final full-CI outcomes are linked against
their exact source SHA in PR #24, without a timing-update/commit loop.

Every distinct encountered friction is recorded through tools/learn.py with a
representative retrieval query. Numeric annotations for new records are
ceil(selected UTF-8 evidence bytes/4) **read-volume estimates**, with provenance
in the baseline data and entry note: not actual/billed model usage, not full task
cost, not historical attribution. Shared evidence footprints must not be summed.
L-142 is reserved by the pending replication branch; this PR's new IDs start at
L-143 so the two append-only histories can later reconcile without duplicate IDs.

Executed local Phase 0 checks: Markdown file links passed;
`python3 -m unittest tools.test_learn` passed 55 tests in 3.900 seconds; all 35
new records retrieved through actual representative context commands. The first
learning run failed the historical assumption that movement has no hints after a
native-search record used that broad keyword. Search-specific metadata was
narrowed and the test friction recorded; no tests or implementation were changed.
The final record covers duplicate push/PR CI triggers: cancelling the redundant
push run makes its always-run guards fail on cancelled prerequisites. Those
failures are distinct from the required PR run, and duplicate work is excluded
from the probe's timing totals. The failed cancelled guards remain visible in
aggregate checks. Both normal workflow runs will finish at the final revision;
the trigger policy is deferred to the measured CI item.
These focused results do not replace full CI or certify the proposed future work.

Phase 0 stops after showing this proposed plan and its actual verification state
to the owner. No Phase 1–3 implementation is authorized by this PR.
