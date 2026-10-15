# Optional contextual content (Poe)

Poe is the discoverable name for Blue's small, optional contextual-response and
scene tools. Choose no narrative, flavor, dialogue, authored scenes, and systemic
events independently. A game with ordinary enemies, names, objective text or UI
labels needs none of these tools. Do not create story files or character sheets
merely because they are supported. The [authoring skill](../tools/skills/poe/SKILL.md)
is loaded only for an explicit relevant capability request.

Milestone 1 supports event-indexed responses and small authored interactions for
custom simulations. It does not convert stock games, add mandatory state, watch
content globally, or schedule a director. Flavor can remain ordinary game text;
use responses only when selection against live state is useful. Rich biographies,
editorial ledgers and writing guidance stay outside runtime state and packages.

## Public entry points

Discover with `python3 tools/be2.py context optional_narrative --level 1 --compact`.
The dependency feature is `narrative`, disabled by default:

```toml
vesper3d = { package = "be2", path = "../BlueEngine", default-features = false, features = ["narrative"] }
```

```sh
cargo run --locked --profile itest --no-default-features --features physics,networking,narrative --bin be2-tools -- narrative describe
cargo run --locked --profile itest --no-default-features --features physics,networking,narrative --bin be2-tools -- narrative lint CONTENT.poe BINDINGS.json
cargo run --locked --profile itest --no-default-features --features physics,networking,narrative --bin be2-tools -- narrative inspect CONTENT.poe BINDINGS.json
cargo run --locked --profile itest --no-default-features --features narrative --example narrative_walk -- --fixture reactive --seed 7 --limit 600
cargo run --locked --profile itest --no-default-features --features native-client,narrative --example narrative_native -- --fixture scene
```

The existing `be2-tools` binary requires `physics` and `networking`; the narrative
runtime and headless walk do not. Native preview uses the smaller `native-client` surface.

`--content FILE` on either fixture replaces its default text. A fixture walk uses
its actual game commands and queries; it is not a pretend reader with copied flags.
See the examples' reports for initial conditions, actions and bounded assumptions.
The native example is a separate spatial 3D yard with authoritative collision and
movement. [Exemplar notes](NARRATIVE_EXEMPLARS.md) explain its two original voices.
These are source-checkout examples. A shipped game declares only its selected
runtime content in `assets/identity.json`'s package list and resolves it through
`devkit::runtime_assets`; see [custom client assets](CUSTOM_CLIENT.md). Authoring
guides/sheets are excluded. The stock/custom starters and their package manifests
remain storyless unless the author opts in.
The engine's source-SDK archive still includes its tracked examples and guides;
that authoring distribution is separate from a game's explicit package manifest.

## A small text format

The version line is `poe 1`. Blank lines, indentation and lines beginning with `#`
are ignored. Optional `note TEXT` editorial fields are accepted and omitted from
the runtime representation. Stable IDs bind author edits, saves, future localization
and future audio. This is a Blue-specific format; it is not Ink or Yarn compatible.
Text is literal, with typed query interpolation as `{query_id}`. No arbitrary eval,
Rust, shell, or per-frame parsing occurs. Use `parse(file, text)` for parsing,
`parse_validated(file, text, &registry)` for authoring, or `load(path, &registry)`
for a bounded file load. Validate before activation; parsing is never per-frame.

```text
poe 1
response bridge_ready
 event gate_opened
 speaker yard_pa
 category announcement
 text Bridge ready. Please bring all of yourself across.
 when bridge_open == true
 priority 10
 cooldown 120
 repeat once
endresponse
```

IDs contain 1–96 ASCII letters/digits, underscores, hyphens or dots. A response
requires an ID, event, speaker, category and text. Conditions repeat and
are ANDed. Comparisons use a registered query ID, `==`, `!=`, `<`, `<=`, `>` or `>=`,
and a JSON literal of its declared type. Numbers are integers; text/bools support
only their legal comparisons. `priority` is an integer. Cooldowns and `after N`
repeat intervals use authoritative game ticks. `repeat` is `always`, `once`, or
`after N`. Unsupported fields and versions fail with source location.
For localization/audio keys, use `response/ID`, `scene/SCENE_ID/NODE_ID`, and
`choice/SCENE_ID/NODE_ID/CHOICE_ID`. Slash is excluded from authored IDs, so these
scoped keys stay distinct; presentation exposes their component IDs.

```text
scene lantern_offer
node ask
 speaker keeper
 text The bridge needs one cell. Yours is the only one here.
 choice lend consequential -> thanks | Use my cell to open the bridge.
  when cells >= 1
  do open_bridge true
 endchoice
 choice keep expressive -> end | I need to keep it.
 endchoice
endnode
node thanks
 speaker keeper
 text I'll remember which side you left yourself on.
 next end
endnode
endscene
```

The first node is the entry. A node has speaker/text and optional repeated `when`
conditions, then choices or one `next NODE_ID`/`next end`. A choice has stable ID,
classification, destination (`NODE_ID` or `end`), and player-facing label after `|`.
A choice block contains optional repeated `when` and `do COMMAND JSON_VALUE`, then
`endchoice`. Every node needs a valid ending or path. Destinations stay in their
scene. Informational, expressive and consequential choices are all legitimate.
Consequential classification records intent; it cannot certify that a boolean
actually changes the player's experience. Trace that separately in review.

The sample command is intentionally game-defined: `open_bridge` must validate the
cell, spend it and open the real collision gate as one atomic transaction. Do not
implement those as three uncoordinated story writes.

A bindings file describes the public typed contract, not initial game values:

```json
{
  "queries": { "bridge_open": "bool", "cells": "int" },
  "commands": { "open_bridge": "bool" }
}
```

Queries are registered `bool`, `int` or `text` values. Commands accept one value of
their registered type; a game can expose a coarse transaction such as `open_bridge`
instead of exposing implementation detail to writers. Speakers are checked through
the live game's presence query. A linter cannot promise a speaker will exist later.

## State and execution

`viewer::narrative` supplies validated content, the typed `Registry`, `View` reads,
`Commands::apply_atomic`, and explicit `NarrativeState`. Bind the view directly to
real gameplay. The runtime has no second variable database, inventory, event bus,
save system, general scripting VM or autonomous writer. A game's existing fixed
step decides when an event is emitted and when narrative intentions are committed.
Presentation reads state and submits intentions.

Eligibility reads one consistent immutable view. Inspection does not mutate state,
consume RNG, move time, start cooldowns, or mark lines seen. Selection is explicit;
commit revalidates. For each event, only its indexed candidates are considered.
Ordering is priority descending, number of satisfied conditions descending, then
stable ID ascending. There is no narrative RNG. No eligible candidate means silence;
missing speakers, expired contexts, cooldowns and repetition can all cause silence.
Detailed rejection diagnostics are requested explicitly rather than retained on the
hot path.

Response queues are bounded. The queue holds at most eight contexts with strictly increasing game-issued IDs
greater than zero; rejected full-queue inserts do not consume their ID. Expiration
is inclusive through its authored tick. Dispatch considers contexts oldest first.
Speaker/category cooldowns begin at selection and prevent a line in a different
entry from bypassing pacing. A higher-priority response may interrupt; equal or lower
priority waits within the bound. Selection, presentation and completion are separate.
An interrupted line is not fully heard. `once` means once selected for this run,
even when interrupted; selected, presented and completed counts remain distinct. Game consequences belong to authoritative
commands, never to a typewriter animation or voice clip finishing; muting cosmetic
responses must preserve play outcomes.

Scene choices carry an active-scene revision/token. Revalidate the selected choice
against live state when committed: a displayed cell may have been spent meanwhile.
Apply effects in authored order in one `apply_atomic` batch, then advance progression.
The game command implementation must either commit the whole batch or leave gameplay
unchanged on failure. Repeated/old input cannot apply the batch again. A failed batch
keeps progression at the same decision. Safe interruption cannot duplicate rewards.

Ownership is explicit:

| Fact | Owner and lifetime |
|---|---|
| Health, cells, bridge collision, objective completion | Existing authoritative gameplay state; normal game lifetime |
| Active conversation, decision token, response cooldown/history | `NarrativeState` owned by the game simulation; saved if continuing the run |
| Authored commitment/relationship or witnessed information | Game-owned state with deliberate encounter/run/persistent lifetime; query/command binding |
| Text reveal cursor, focus, cosmetic speech timing | Replaceable native presentation; transient |
| Rich biography, world/belief notes, setup/payoff rationale | Authoring material; excluded from runtime |

Embed the narrative state actually used in the game's existing `devkit::Snapshot`
implementation and include it in the authoritative state hash. Resume game state and
narrative state together. Do not hand-write a parallel save. A game designed to depend
on scene progress cannot discard that progress when loading: removing a capability
requires an explicit save compatibility change/migration or a deliberately new run.

## Native interaction and development reload

Presentation is game-owned. The native fixture demonstrates readable text, choice
focus, separate reveal and advance intentions, and blocked gameplay input while a
scene owns focus. In this fixture, scene focus freezes locomotion while authoritative ticks continue;
there is no gameplay countdown. Other games decide explicitly whether timers pause.
The authoritative runtime has no implicit pause, typewriter clock or audio dependency.
Use stable IDs for later localization/audio work; this milestone adds no voice
production pipeline.

Reload is an explicit development action. Read, parse and validate before activation.
Keep the previous valid content and state on any failure. Activate only at the game's
safe boundary, outside effect commitment. Compatible text edits preserve progress.
When a scene is active, changed choice topology/effects or removed active references
must be rejected, or the game must deliberately restart/migrate at an inactive
boundary. Never silently restart an active scene and replay committed effects.
No live executable update or global file watcher is involved.

## Validation and evidence boundaries

Hard failures cover syntax/version, duplicate/invalid IDs, references, unknown
bindings, type errors, illegal destinations and nodes missing a continuation or
choices. The executor makes at most one transition per explicit advance/choice
intention; every jump yields to input, so no immediate autonomous cycle exists.
Explicit endings, reconverging branches and player-yielding loops are valid.
Source diagnostics retain author file, node and line. Limits are 256 KiB source,
512 responses, 32 scenes, 128 nodes per scene, eight choices per node, 16 guards/effects
per item, 4 KiB source text and 16 KiB interpolated text. These bound content, queues
and execution; a headless walk has its own explicit action/state bound.

Current warnings cover repeated response text, missing unconditional response or choice fallbacks,
unreachable nodes and choices with equal guards/effects/destinations. An unresolved
setup belongs in editorial review and the optional ledger. Warnings are not proof
of a defect. Declared external,
UI, gameplay and future readers count before treating a write as unused. This format
has no rich character-sheet schema; required sheet fields belong to the selected
substantial-scene authoring profile and editorial review. Lint does not infer lies,
knowledge, motivation, emotional impact or literary quality from prose.

Report content validity, runtime behavior, native presentation, editorial review and
human playtesting separately. Deterministic replay within tested seeds/conditions
is not exhaustive coverage or proof of cross-platform bitwise identity. Native
captures/input probes do not certify physical controllers, Windows devices or audio
hardware. Keep raw measurements with seed, limits, tool calls, repairs, source exposure,
reload cost and runtime/build overhead; avoid a single story-quality-per-token score.

## Why a narrow frontend

Checked 2026-10-14 against primary documentation. The cost judgments below are Blue
integration estimates, not measured claims about upstream projects.

| Option | Authoring/diagnostics | Binding, maintenance and licensing cost |
|---|---|---|
| Ink | Mature readable branching text, compiler and interactive Inky preview | MIT; upstream core/compiler and runtime are C#. Compiled story JSON and its own story/save versions would require a Rust adapter/port/interop plus deliberate state/save alignment. External functions/variables can bind game state, but Blue still needs typed command transactions and native presentation. |
| Yarn Spinner | Readable dialogue nodes, options, commands, compiler and editor tooling | Core MIT; engine integrations are separate. Replaceable variable storage and functions suit game-owned truth. Blue would still need a Rust integration, typed bridge, deterministic lifecycle/save ownership and native focus/reload policy. |
| Narrow Blue frontend | Small scene-oriented text, source-located diagnostics, direct native/headless tools; fewer language/editor features | Reuses Blue's Rust runtime, serde and existing snapshot system with no new third-party dependency. Blue owns a deliberately limited parser/validator; adding language features has a real maintenance cost. |

Sources: [Ink repository and license](https://github.com/inkle/ink),
[Ink game integration](https://github.com/inkle/ink/blob/master/Documentation/RunningYourInk.md),
[Yarn core and license](https://github.com/YarnSpinnerTool/YarnSpinner),
[Yarn variable storage](https://yarnspinner.dev/docs/godot/06-components/03-variable-storage/).

For this milestone choose the narrow frontend, primarily for the small Rust state
bridge and optional dependency boundary. Do not convert existing content. Readability
and short source are hypotheses to measure on equivalent text/JSON examples; tiny
static labels may cost less as ordinary game strings. The [raw authoring comparison](perf/narrative-authoring.json) records
actual bytes/lines and available token measurements rather than asserting a DSL is
universally cheaper. The final two files cost 873/858 UTF-8 bytes in Poe versus
1500/2297 in readable equivalent JSON (973/1129 compact). A text revision replaces one line
in both formats; adding a guard adds one Poe line versus five to seven readable
JSON lines. Model token counts were unavailable; whitespace words are not tokens. No external story-authoring repository is required or assumed.

## Subsequent milestones with bounded acceptance

A director remains a separate optional scheduling policy. First reuse existing
scheduling/timer facilities and expose only permitted gameplay commands. Acceptance:
no-dialogue event pool; fixed seed/initial state; prerequisites, priority, cooldown,
conflict groups and budgets; deliberate quiet/recovery; protected encounters;
expiration; explainable rejection; deterministic exhaustion with no invented resources
or rewritten history. A tension proxy must be labeled authored pacing, not emotion.
Use a three-event, no-dialogue fixture (seed 17, 300 fixed ticks, budget two):
protect ticks 10–50, expire one event at tick 40, and require 60 recovery ticks
after an intervention. Assert no command during protection/recovery, no expired
event firing later, no third intervention, explicit quiet/exhaustion diagnostics,
and identical command traces after a midpoint snapshot resume. A command outside
the registered permitted set must fail validation before scheduling.

Larger graphs require bounded headless traversal with saved decisions, explicit
limits/assumptions, optional knowledge contracts/provenance and arc setup/payoff
reports. Acceptance: reconverging branches, explicit endings, bounded/yielding loops,
knowledge revelations via commands, intentional unresolved setups, stale choices,
save/resume, and no duplicate transactions across retry/interruption/reload. New
syntax is justified by a real consumer, not a universal RPG framework.
Use two reconverging branches and one player-yielding loop, seeds 7 and 11,
at most 1,000 transitions and 128 visited nodes per walk. Record unexplored
frontiers when either limit is reached. An explicit revelation command must record
its source node; loading after it must preserve knowledge without reapplying its
gameplay effect. An intentionally unresolved ledger entry must stay a visible,
reasoned exception rather than a hard failure.
