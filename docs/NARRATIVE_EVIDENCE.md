# Optional narrative milestone 1 — evidence

Implementation and bounded native examples are in this checkout. Start with
[the capability-aware Poe guide](../tools/skills/poe/SKILL.md) and
[the public contract](NARRATIVE.md). No existing game needs conversion.

## Delivered boundary

- Cargo `narrative` is off by default, has no dependencies, and adds no fields or
  callbacks to stock gameplay. No runtime is constructed, content loaded, narrative
  events selected, RNG consumed, or watcher started for an absent capability.
- The text frontend retains source locations, checks typed bindings and graph
  references, and produces a validated representation. Responses are event-indexed;
  interpolation compiles at activation, rather than parsing during presentation.
- Selection is pure and deterministic. Explicit commits own cooldowns/history;
  selection, presentation and completion differ. Queues, content and output sizes
  are bounded, and silence is valid.
- Scenes implement only choices, player-driven jumps and explicit endings. A choice
  revalidates its token and live conditions, stages progression, and calls one
  game-owned atomic command batch. The fixture spends a real cell and changes real
  collision; dialogue completion never controls gameplay truth.
- Existing engine snapshots carry narrative progress and hashes with gameplay.
  Reload validates before activation, preserves compatible text/progress, and
  rejects active semantic changes. Removed capabilities require explicit save
  migration/restart. No alternate save, variable database, event bus or VM exists.
- The game owns native text reveal/focus, controller intentions, interruption and
  pause policy. Scenes freeze movement/gravity; the game clock continues. Esc pauses
  fixed ticks. Barks remain passive and may be suppressed without gameplay changes.

Director scheduling, large graphs, machine-checked knowledge provenance, rich
character-sheet schemas, localization/audio production and campaign systems remain
outside this milestone. Concrete follow-up tests are in the contract. The guide and
exemplar notes cover character/voice/knowledge/ledger craft at authoring time.

## Acceptance evidence

A: Existing storyless Timed Relay loads and remains winnable without story files or
narrative state (`tests/narrative_optional.rs`, feature disabled). Starters, ordinary
characters, labels and stock rules were not converted.

B: [Relay Yard](../assets/narrative/relay-yard.poe) adds a maintenance PA around
pickup, gate and crossing events. Storyless, spoken and quiet runs produce identical
gameplay state. A maintained Prop Hunt checkout was inspected read-only; adding
networked speech there would cross participant-redaction contracts, so this bounded
milestone uses an isolated engine fixture.

C: [Borrowed Light](../assets/narrative/borrowed-light.poe) spends the actual cell,
opens the collision gate, and acknowledges the cost after the player crosses.
Keeping the cell leaves it and the gate unchanged. The simulation uses the shared
3D Controller, vertical stairs, height-sensitive collection and matching collision
geometry. It is not planar rules with a 3D drawing.

[Raw headless reports](perf/narrative-fixtures/runs.json) retain exact commands,
seed 7, the 340-tick bound and share/keep/quiet routes. The [fixture contract](NARRATIVE_EXEMPLARS.md#public-fixture-contract-and-controls)
specifies the initial setup. All five runs have
zero execution errors and matching midpoint-save suffix replays. These routes are
bounded demonstrations, not exhaustive state exploration.

Focused behavioral evidence: parser 5 tests; runtime 16 tests; fixtures 7 tests;
storyless acceptance 1 test; discovery/automatic workflow 21 tests. Coverage includes
malformed content, types/IDs, silence, stale selections/choices, interruption, absent
speakers, expiration, bounded queues, atomic failure, repeated input, no duplicate
spending, deterministic resume, invalid save state and failed/incompatible reload.
A read-only review found one interpolation-size fallback edge, which was repaired
and regression-tested before final verification.

## Public authoring trial and costs

The [fresh-agent trial](perf/narrative-trial.json) first handled a storyless arcade
brief. Its packet selected ordinary portable-game guidance with no narrative kit.
It then added and revised one contextual response using only public documentation,
content, bindings, linter and headless tools. Both versions validated; the revision
was selected at tick 205 and selected/presented/completed exactly once. No engine or
private fixture source was read. The report retains commands, repairs, source
exposure and timing. Actual model token counters were unavailable and remain null.

[Public discovery regressions](perf/narrative-discovery.json) also cover explicit
story/plot requests, storyless requests and reactive barks without a story. The
focused 21 tests and the complete Python batch passed: 624 reported, 58 skipped,
566 executed. The final canonical run repeats that batch on the repaired workflow.

The [format comparison](perf/narrative-authoring.json) records equivalent semantic
JSON, source bytes, lines and edit costs. The final Poe examples use 873/858 bytes;
compact JSON uses 973/1,129 bytes and pretty JSON 1,500/2,297 bytes. A text change
costs one line in both readable formats; a guard adds one Poe line versus 5–7 pretty
JSON lines. These examples do not establish universal token savings or story quality.
Initial draft measurements and exact raw source/JSON snapshots are retained.

[Runtime/build measurements](perf/narrative-runtime.json) retain profile, workload,
binary sections, seeds/limits where relevant, and raw timing scope. The host has
heavy shared-cache I/O contention; lock waits and startup/build cost are not runtime
selection cost. The optional parser/model/runtime module has 1,895 production lines
and 609 unit-test lines, including comments and blank lines. The report separately
counts fixture, CLI and integration-test work; no new third-party dependencies were
introduced. Against pristine revision
`76743a0`, the disabled `itest` headless ELF remains 38,852,944 bytes; the tool ELF
grows by 3,056 bytes in the final sample. The public discovery response remains
within its existing 6,000-byte budget at 5,984 bytes. Earlier paired build wall
times were 59.15/71.72 seconds, including
36.92/34.41 seconds of lock wait; disk contention prevents a causal compile-time
overhead estimate. This is an observed profile/host sample, not a universal zero-cost
claim.

Enabling `narrative` in the same unstripped profile adds 760,800 bytes to the
headless executable and 1,470,776 bytes to the authoring tool relative to the final
disabled sample. These are measured feature costs, separate from absent-capability
residual overhead; release binaries and heap overhead were not measured.

## Presentation, editorial review and remaining evidence

[Native command/state reports](perf/narrative-fixtures/native-runs.json) retain
three successful software-rendered runs and four inspected frames: [storyless](perf/narrative-captures/storyless/shot_00030.png),
[reactive](perf/narrative-captures/reactive/shot_00030.png), [choice](perf/narrative-captures/scene/shot_00210.png)
and [callback after load](perf/narrative-captures/scene/shot_00275.png). The final
F6 reload measured 0.277517 ms. F5/F9 retained one committed cell spend; dismissing
the passive bark preserved the storyless gameplay result. Reveal and advance use
distinct actions, and bark dismissal uses Enter/LB so A remains jump. Review also
replaced unsupported font glyphs with readable ASCII. These frames and scripted
intentions do not certify physical controllers, Windows devices, hardware audio or
human playtesting.

Agent editorial review differentiated the officious PA from Mara's restrained
request and concrete callback. It removed a line implying an unimplemented lamp
mechanic and guarded an offer against unavailable inventory. Fallback warnings are
intentional where silence is appropriate. Neither passing lint nor these reviews
certifies emotional impact; no human playtest was performed.

[The canonical Linux engine/consumer report](perf/narrative-verification.json)
records all 39 gates passing: default/headless and narrative tests, Clippy and
rustdoc; 2D/model-import/schema modes; Signal Garden, Lantern Run, Pocket Breaker,
Orchard Watch, Lantern Grove and Leo; headless boundaries, public authoring and the
complete Python batch. Earlier failed attempts and their repairs are retained.

Tracked evidence was updated during that successful run, so its task-source binding
is not stable. After this report is frozen, the [final checkout receipt](../.be2-work/narrative-evidence/full-check-5.stdout.json)
links the repeated canonical check and its source-stability verdict. Its outputs
stay in the ignored work directory. The Linux/Windows CI matrix is preserved;
Windows runtime and physical-device evidence require their corresponding hosts.
