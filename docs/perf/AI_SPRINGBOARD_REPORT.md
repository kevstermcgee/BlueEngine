# Bounded AI springboard

Reviewed baseline: `83842f99a5a4df4dd98c3da46fb04f6878815175`.
Implementation began on `bd5cb054328932584d455a148ac48034773de508`, preserving the existing
context/feature map, verification planner, authoring, browser, upgrade and diagnostic tools.
Unrelated metrics and Linux-headless notes were left intact. No runtime architecture was replaced.

The remaining startup costs were coordination: choosing a dependable starter, probing target
readiness rather than just tool presence, retaining an interrupted goal, and determining whether
observed results still matched the current inputs. The native quickstart also used an unqualified
stock command although the executable default had already become portable.

`python3 tools/be2.py start "<task>" --compact` is now the shared source-checkout entry.
`next TASK_ID` and `resume TASK_ID` refresh it. `--no-save` works on all three; `--detail`
provides deeper context. Execute the packet's structured command separately. None of these discovery
operations builds, installs, launches, imports game scripts or executes verification.

The coordinator uses the existing feature/context index and canonical tools. A shared
`templates/starters.json`, summarized in native `describe`, supplies launcher/CLI starter metadata and
portable default targets. CLI default remains portable; library default remains stock. Explicit
stock instructions now agree with executable behavior. Browser/native/custom/headless/networking
requests retain their differences; unsupported combinations name the gap and extension route.

Checks use `check ... --task TASK_ID` and retain their original reports/logs. Input binding covers
current Git-inventoried engine content, existing browser game hashes, assets/config/locks,
environment/tool identity and package outputs. A permitted first metadata step can register only
the game lock. Mid-check changes and subsequent changes invalidate results. Notes cannot pass a gate.
Schema, planned checks, behavior, skipped checks, failures and shipping remain separate. Current
shipping evidence certifies its machine scope, while CI/manual/device/publication obligations remain
explicit. Native games, diagnosis and upgrades retain their direct tools; portable existing games
reuse the project stages. This adds no LLM, scheduler, separate feature index or verification cache.

## Measured discovery

[Curated measurements](AI_SPRINGBOARD_MEASUREMENTS.json) record each sample. Each uses a fresh Python
process and instruments every subprocess call. Three start samples were taken per condition.
The cold condition means an empty binary target, not a cold OS/toolchain/dependency cache. No compiler
cache was deleted and no cold full-build benchmark was performed.

| Operation | Wall time | Compact output | Read-only probes | Builds triggered |
| --- | ---: | ---: | ---: | ---: |
| Start, no compiled authoring tool | 0.420–0.426 s | 5,074 bytes | 7 | 0 |
| Start, compiled tool available | 0.438–0.454 s | 5,145 bytes | 7 | 0 |
| Next, interrupted game | 0.470 s | 7,704 bytes | 9 | 0 |
| Resume, interrupted game | 0.458 s | 7,704 bytes | 9 | 0 |

The warm packet hashes the discovered executable; its presence is explicitly not freshness evidence.
Next/resume with `--no-save` left metadata byte-for-byte unchanged. The packet selects at most three
initial references, one feature match and three project files; deeper ownership is opt-in. This is
an output bound, not a measurement of model tokens or actual files an agent reads.

## Exercised routes and evidence

- New portable 2D game: executed the packet's explicit `map new-game` action; its Cargo freshness
  check took 0.09 s and reused the authoring tool. Set only the requested web target, changed the
  sample to two hearts, and added pickup-single-use/exit-requires-all-four behavioral coverage.
  Initial inner check: 23.706 s, two commands including first-use metadata, four executed tests,
  121 fresh/5 built Cargo artifacts. Warm inner check: 0.475 s, one command, four tests,
  126 fresh/0 built artifacts. Integration: 30.061 s, three commands, eight test cases across
  both feature modes, 292 fresh/10 built artifacts. These are current-cache observations, not
  before/after speedups attributable to this coordinator.
- Focused engine maintenance: started before implementing read-only resume, detected the change,
  selected the existing Python scope, and recorded inner/integration reports. The latest focused
  inner run took 17.894 s, three commands, 156 tests and no Cargo artifacts. Native starter/catalog
  regression iteration also passed: 70.236 s, four commands, 733 tests, 130 fresh/9 built artifacts.
- Interrupted/resumed work: saved the game goal and note after a passing inner check. An asset edit
  changed that pass to unverified, retained its previous result, and recommended an inner check.
  Current integration and shipping are rerun against frozen inputs for final verification.
- Nineteen focused springboard regressions cover missing binaries/no implicit builds, all six
  starter choices, unsupported feature combinations, target prerequisites, delegation, malformed
  projects, literal Git filenames, read-only resume, notes without evidence, changed source/assets/
  config/binaries, missing logs, mid-check mutation, real failing tests, all-skipped selection
  failure and a shipping wrapper whose successful exit still contains a skipped gate.
- Real browser package verification exercised desktop and emulated portrait/landscape controls,
  WASM/native deterministic outcome/hash agreement, input, save/reload, blocked writes, audio
  initialization, offline startup, interrupted updates, focus and synthetic controller support.
  All five captures were inspected: readable HUD, playing scene, completion panel and touch controls.
  Hardware audio, physical controllers, Android/iOS Safari and another OS were not tested.

The first full attempt caught the native `describe` size guard: embedding the entire catalog
made the packet exceed its existing 6,000-byte limit. The fix exposes catalog defaults and
a detail reference, preserving the limit and adding catalog agreement assertions. Repeated startup hints were consolidated around the shared entry. Its failed
report remains recorded and resume reported a failed shipping state with a repair action.

GitHub run `37501554146` on the implementation base also exposed two genuine failures:
Windows used the 8.3 temp-path spelling in a test expectation, while the checker correctly used
an absolute resolved path; the expectation now resolves its path. The browser fixture passed a
relative game path directly to the builder, making the native report path relative to Cargo’s
game working directory. The builder now normalizes that public input, with a focused path/report
regression and the real browser boundary/preview fixture (all three tests passed locally in
126.904 s using the same relative fixture path as CI). The two three-second test jobs were
aggregate guards, not additional test failures. The in-progress full run was interrupted to
fix these before repeating final verification; its partial evidence remains a failure.

The replacement Windows run caught a new test-fixture portability issue: an empty
`BE2_TOOLS` value existed in the parent Python environment but was removed from the
spawned Windows process. The strict input binding correctly treated these environments
as different. The fixture now unsets the override in both processes and directly checks
before/after/current identity agreement, with diagnostic differences on failure. No
verification identity was relaxed. An isolated Windows diagnostic branch tested this
without replacing main's full CI gates.

The subsequent full run caught the analogous generated stock-guide 3,000-byte guard.
Startup guidance was consolidated with its existing context instructions; the original guide
bound and verification/architecture rules remain intact. Both size regressions were checked
with their exact native tests before the final full run.

Final gates use the existing full checker and strict game checker, without narrowing shipping:

```sh
python3 tools/be2.py check --task ENGINE_TASK --timeout 600
python3 tools/be2.py check --game GAME_DIR --loop integration --task GAME_TASK --timeout 600
python3 tools/be2.py check --game GAME_DIR --loop shipping --task GAME_TASK --timeout 600
```

The final full run is measured once using the canonical `tools/perf.py` recorder, with task binding
attached and metrics redirected to ignored `.be2-work/springboard-final-metrics.jsonl` to preserve
preexisting metrics. Its report retains every command, test count, elapsed time, CPU and child RSS;
there is no extra benchmark build. Final execution results and paths are reported with delivery.
Windows CI/release gates remain required; a Linux run does not certify Windows execution. Full gates
retain fixed-step/headless, networking, public compatibility, both feature modes and native authoring.

The initial completed local full gate passed 32 commands and 2,960 executed test cases
in 779.871 s (921.513 child CPU seconds; 2,926,788,608 bytes maximum child RSS,
not simultaneous whole-machine peak). The strict sample-game shipping gate passed in
83.655 s. It required one unchanged retry after an intermittent audio-activation failure;
the earlier failure remains recorded and no audio check was skipped or weakened. Final
post-fixture verification and CI results are reported with delivery. The separate games
publishing workflow still lacks a Windows download definition for `lantern-grove`, a
catalog failure also observed before this work; exporter tests pass.

## Short packet example

Abbreviated fields from a new-game packet (the actual packet retains all constraints/identities):

```json
{
  "schema_version": 1,
  "objective": "Create a small 2D collect-four game",
  "workflow": {"kind": "new-game", "template": "two-d", "runtime": "portable", "targets": ["web"], "networking": "offline"},
  "context": {"references": ["docs/TWO_D.md", "docs/PORTABLE_GAMES.md"], "project_files": ["AGENTS.md", "game.project.json", "src/lib.rs"]},
  "next_action": {"cwd": "ENGINE", "argv": ["python3", "ENGINE/tools/be2.py", "map", "new-game", "relic-room", "GAME_DIR", "ENGINE", "two-d"], "expected": "Create explicit starter; builds fresh tooling only when executed"},
  "evidence": {"inner": {"state": "unverified"}, "shipping": {"state": "unverified"}},
  "inspection": {"read_only": true, "builds_triggered": 0}
}
```

## Remaining limitations

No fresh-agent before/after trial was conducted, so this work claims no reduction in model tokens,
overall game-completion time or smaller-model failures. No existing build/test speedup is attributed
to this coordinator. Dependency-cache availability and actual compilation remain unverified until
checks execute. Prerequisite probes are advisory, not proof of a toolchain or device functioning.

Task metadata is local and ignored; use a new start operation to deliberately change a selected
route. Static path dependency resolution currently requires this engine checkout. Delegated native
checks are not promoted to coordinated completion. Overall creative completion still requires
objective/visual review; publication still requires committed retrievable source and receipts.

Input invalidation intentionally fingerprints the whole engine inventory, including documentation,
so unrelated engine metadata can cause conservative invalidation. The next useful improvement is
measuring whether existing ownership/build identities can safely narrow that input set, without
introducing a competing cache or weakening correctness. Full CI and strict browser verification
remain the largest costs at the final boundary. They are retained.
