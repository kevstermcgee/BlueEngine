# The next phase: thin games, optional layers, useful authoring output

Status: Proposed — Phase 0; owner review precedes implementation.

Date: 2026-10-13. Audited development HEAD: `579b90868e7e20005f874d2689e35a5f8a23e559`.
ADR 0050 is reserved by the separate, unmerged custom-replication pass; this record
does not import that implementation or claim it has reached main.

## Context

BlueEngine should let an author write a small Rust game against Simulation,
Snapshot, draw::Game and NetGame/ClientView. Its Python front door should answer
the first questions without compiling. A game owns its rules, presentation,
camera, controls and audio arrangement; shared services should make those choices
cheap. Larger simulations are a direction for these boundaries, not a capacity
claim or a request for a new ECS, renderer, transport or cloud platform.

[The Phase 0 report](../REPORT.md) verifies review A–H against the revision above,
separates current defects from stale consumers and pending branches, and records
methods and limits. This record proposes a sequence only. No phase below is
implemented by this documentation change.

The owner's game feedback takes precedence over a blanket proposal:

- [Rift Delver](../feedback/2026-10-09-rift-delver.md) needs spatial rules,
  geometric queries and clear scaffolding contracts; it also reports that simple
  primitives enabled coherent low-poly art. More imported art is an option, not
  an obligatory replacement.
- [Wormhole](../feedback/2026-10-10-wormhole.md) requires unobstructed,
  game-owned rendering and an authored adaptive score. Preserve its overlay
  opt-outs, lifecycle, save and input paths.
- [Sway](../feedback/2026-10-11-sway.md) needs streamed user media, including
  synchronized pause, rather than a generated replacement soundtrack.
- [Idea Forge](../feedback/2026-10-07-idea-forge.md) and the
  [hardening audit](../hardening/feedback-audit.md) distinguish source discovery,
  cache/storage failures, actual input, audio initialization and presentation
  evidence. A screenshot or nominal fixed-step rate cannot close those issues.

Some requested fixes already landed: every canonical starter emits project
metadata and a validator; generated checks discover the shared itest tool. The
remaining problem is copied consumers and stale guidance. Stock weapon policy
already has a stock_demo home and the controller has a data-profile entry point.
Generic HeadlessWorld can legitimately have no GameRuntime: do not remove that
escape hatch merely because GameSession constructors always provide one.

## Decision

Use **BlueEngine** as the product and future public crate family name. Preserve
the current vesper3d import contract during migration; a separate naming/facade
ADR must define the deprecated compatibility package/re-exports, feature aliases,
release boundary and consumer checks before any public name is changed. Cosmetic
renaming is not allowed to break a game or native development command.

Work through Phases 1–3 in order. Each bounded implementation item gets one PR
and its own ADR for a real decision. Where an item needs both a move and new
behavior, split those into separately reviewed subitems: never hide a behavioral
change inside a structural move. Re-layering and crate extraction are one move,
not two successive reorganizations. All full Linux/Windows final guards and
shipping builds remain required before asking the owner to merge.

### Phase 1 — structure and measured build work

1. **Thin project tooling.** Move the canonical ship/check/dev implementations
   into engine-owned modules and leave approximately 20-line game shims. Rust
   build.rs should call a small engine build helper without networking, physics
   or rendering dependencies, not copy the implementation or build the full
   engine for the host merely to embed resources.
   Preserve arguments, diagnostics, overrides, source identity, output paths,
   package isolation and intentional game work such as Reverie's soundtrack
   preparation. Use the existing upgrade planner for all 12 games plus Leo and
   the version-pinned BlueEngineGames population; do not create a second updater.
   Treat extraction/migration separately from a new divergence-rejection policy.
   An explicit game-project declaration permits deliberate customization.
   Before migrating, the tooling ADR must define shared entry resolution for
   both source checkouts and packaged authoring tools: the engine toolkit bundle
   supplies its implementation, games retain only shims, and explicit native-tool
   overrides still work. An executable path alone does not locate Python modules.
   Test missing/mismatched toolkit diagnostics without copying a fallback into
   the game or requiring engine source for an already-supported packaged route.

   Done: fewer than 2,000 physical Python lines under games/; no engine-template
   cluster in a fresh dupes report; every starter plans and passes its applicable
   checks without manual helper copies; old projects migrate idempotently with
   deliberate overrides retained; the four contradictory game AGENTS files are
   corrected. Inspect isolated installed packages to prove engine tooling is
   absent. Never remove a game-specific script solely to meet the line count.

2. **One move per layer, then optional dependency selection.** Extract the
   existing sim, stock net, netplay/hub, presentation/kit, devkit, two_d and
   authoring/tools layers behind a compatible facade, one compatible dependency
   leaf or seam per bounded PR. Move each directly to its eventual layer; do not
   pre-reorganize viewer and then move the same code again. Keep icon generation and
   synth authoring in their own optional crates, preserving deterministic output
   fixtures. Do not change simulation rules or replace either networking API as
   part of the move. Preserve legacy default and no-default-feature imports until
   the compatibility migration is explicitly decided. In a separate feature/dependency item, make Rapier and the
   Tokio/QUIC/TLS stack optional and migrate consumers to declared requirements.
   Exact crate edges and legacy feature aliases need their implementation ADR;
   this record is not a mandate to invent a trait for every module.

   Done: standalone Lantern Run's settled Cargo.lock has no quinn, rustls, ring
   or rapier3d, and its actual normal build graph excludes those packages and
   Tokio. Check both: a workspace-wide lock may resolve unused optional members.
   Existing public imports and native profiles build; all consumers keep their
   behavior. Measure the same sim edit with `cargo build --profile fast` before
   and after, including cold/warm state, compiler, host and changed inputs, and
   record it with `python3 tools/perf.py record`. No speed claim from graph size.

3. **Measured CI items.** Use existing hosted timings and this documentation PR
   as the pre-optimization probe. Record workflow/step wall time, unweighted job
   minutes, runner/compiler, exact commands, cache keys and cache condition.
   First investigate the observed CARGO_TARGET_DIR cache-environment mismatch;
   then measure profile reuse and duplicate work. Preserve game-owned profiles
   where useful, or give incompatible outputs honest cache keys. Consolidating
   the five pinned Mesa setups is maintenance, not a proved leading speed win.
   Docker caching/trigger changes and tag-pinned source publication are separate
   coverage/distribution decisions with their own acceptance.

   Done: matched before/after hosted observations in docs/perf, with at least
   three successful warm observations per configuration and a separately labeled
   cold observation when obtainable. Preserve both required test guard names,
   feature assertions, release/package/container coverage and failure artifacts.
   Seek a lower median critical path and total unweighted job duration; reject
   an apparent saving produced by skipped checks or incompatible workloads.

### Phase 2 — tool output replaces author source inspection

4. **Document the public surface and prevent regression.** Start with the
   extracted core and commonly routed game/controller/fps/arena contracts, then
   the other crates. Document inputs, units, ownership, errors and edge semantics,
   including fields/variants. Define the exported-item denominator and exclusions
   in the coverage ADR: today's lexical pub counts are only a baseline proxy.
   Use the existing checker for the gate, not another evidence system.

   Done: documented core surface first, then at least 95% of the defined public
   authored surface in every supported feature configuration, with no regression
   and no placeholder comments. Existing export compatibility remains tested.

5. **Rank discovery and answer stock questions with data.** Share the current
   IDF approach with native search; return bounded useful contracts instead of
   whole unrelated feature records. Remove retired capabilities from active
   lookup while retaining one migration diagnostic. For a stock project, “make
   the player jump higher” must first surface jump_height, its schema path,
   constraints and validation; custom projects must get their own API route.
   Reconcile the pending initial-learning/public-guide fixes before doing them
   again. Preserve feedback occurrences while updating stale actionable hints.

   Done: fixed representative queries and new owner-feedback queries retrieve
   the correct schema/API contract in the top result on both front doors; existing
   retrieval floors hold. Repair the source navigator's 100-line span cap and
   missing async recognition in its own bounded correctness PR, with long-body
   and raw-string regressions; ranking does not repair symbol ranges.
   A stock edit needs no Rust source inspection. Report
   retrieval quality separately from controlled author completion outcomes.

6. **Budget the entry route, then verify a cold author run.** Budget root and
   game AGENTS, the complete start packet, and every mandatory references/read_first
   document. Move implementation files to optional ownership/symbol navigation.
   Split the custom-sim cheat sheet into a small first page and deferred detail;
   expose a few kilobytes of author hooks rather than a full client application.

   Done: initial 3D entry documentation plus packet at most 30,000 UTF-8 bytes,
   compared with the measured 72,980-byte documentation/packet baseline. Required
   game source and metadata are counted separately; starter author-edit hooks
   total at most 4,000 bytes, without limiting subsequently authored game rules. Proposed
   component caps: root AGENTS 4,000, game AGENTS 3,000, start packet 6,000 and each
   selected first guide 8,000 bytes; the aggregate cap also applies. Test actual
   packet routes and newly referenced files, not a handpicked sum. On declared
   Linux/Windows hosts with documented prerequisites, run three frozen-config
   cold-checkout author trials per route: 2D first native launch within five
   minutes and custom 3D within ten. State registry/target-cache conditions and
   source opens; failure remains a failure. These are targets, not current results.

7. **Fix the seam through bounded compatibility work.** Introduce generic action
   bits/data profiles with legacy adapters; gate example/stock content and the
   Deadfall compatibility shim, with a sunset ADR. Preserve the optional-game
   generic world contract. Netplay remains the custom-simulation authoring API;
   stock networking remains the GameDocument adapter until a separately justified
   migration can preserve both. They share services, not necessarily one policy.
   AudioProject is the data-authoring route; synth recipes can remain generators.
   The canonical scaffold must have one engine-owned implementation behind both
   Python/native entry points. Define these contracts before deleting a path.

   Clarify checksum scope in a separate behavioral decision/regression: current
   world checksum omits saved timer countdowns and previous zone occupancy. Equal
   hashes are not proof of equal complete game continuation or different-game
   identity. Historical repeated table values cannot be attributed without their
   missing fixtures. Naming, duplicate ADR numbering, GLOSSARY and standalone
   source files under docs/ each get a bounded documentation/move item. Archive
   obsolete browser instructions, retain active native docs and a concise
   retirement guide, and keep useful working code examples in documentation.

   Done: legacy input/import consumers pass; stock and custom worlds remain valid;
   lifecycle/save/authority tests cover adapters and optional routes; modeled
   checksum omissions gain actual public-API regression coverage; no active
   author route recommends browser gameplay; all ADR links resolve uniquely.

### Phase 3 — original identity is a cheap choice

8. **Reuse existing capabilities before adding another system.** Reconcile the
   pending creative-identity hooks as their own compatible item. Offer game-owned
   style/palette and semantic sound bindings; adopt named AudioProjects/banks
   rather than force every game into a fixed menu or generated music. Converting
   preset definitions to data must preserve their seeded waveform contract and
   needs an authoring ADR. Silence, effects-only play and user media remain valid.
   Offer the existing model import as a scaffold step with its license/size/failure
   checks. Add one bounded shape helper only when a repeated consumer operation
   establishes the need; do not add three speculative APIs together.

   Done: two consecutive recorded IdeaForge games, without an additional user
   uniqueness prompt or cherry-picking, have owner-accepted distinct presentation
   and sound arrangements. Freeze engine/coordinator/model/settings first; record
   briefs/seeds and any internal identity instruction. Inspect gameplay/menu/outcome
   captures, listen to declared audio, test lifecycle/save/input and isolated native
   packages on both platforms. Use a declared rubric for composition/silhouette,
   palette/contrast, interface/motion and timbre/rhythm/orchestration. Different
   hashes alone do not establish originality, audibility or enjoyable play.

## Consequences and evidence rules

These boundaries add migration work but reduce copied policy and accidental
dependencies. They preserve author escape hatches and the shared fixed-step,
rendering-free authority, semantic IDs, visual/collision agreement, actual payload
limits, acknowledgement semantics and completed outputs on failure. Official
branding and game identity assets are preserved. No other engine is opened or
imported; only independently evaluated ideas may cross.

Every friction occurrence is recorded through tools/learn.py and retrieval is
checked with a representative query. Preserve historical feedback; a promoted
entry needs verified current capability, not prose. Session token telemetry is
unavailable in this Phase 0 audit. New token fields are explicitly labeled
ceil(selected UTF-8 evidence bytes/4) estimates, not measured or billed usage;
they exclude reasoning and cannot be summed into total task cost or used as proof
of a token saving. Do not infer historical per-finding costs. Harvest feedback
with its revision and reproduction before treating a ledger status as current.

Phase 0 is done when A–H are classified with methods and caveats, this ADR and
REPORT agree, friction records retrieve, the documentation-only PR has its exact
verification/CI state recorded, and the owner is shown the proposal. Stop there.
Do not implement Phase 1 before the owner's review of this proposed sequence.
