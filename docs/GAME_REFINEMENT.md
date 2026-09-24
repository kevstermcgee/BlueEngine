# GameDocument refinement — 2026-09-24

The feedback correctly identified the next missing layer: map authoring alone cannot
produce a playable objective. This pass implements a deliberately bounded version
instead of a general scripting framework.

## Delivered

- Separate validated GameDocument v1, one configurable movement profile, named spawn
  points, counters, ordered interaction rules, once flags and objective completion.
- One GameRuntime shared by local play and the authoritative server. Clients send
  intent; server-side range/occlusion decides the target. No client-owned results.
- Protocol 3 fingerprints map and game semantics. Small repeated full game snapshots
  handle loss/reordering and provide current state to joining clients.
- Native game-describe/schema/example/validate, compact GAME_QUICKSTART.md, feature
  discovery and ADR 0007. Example generation preserves existing output directories.
- A playable three-switches/exit fixture, graphical status and interaction hints,
  and Launch Three Switches.cmd. Existing official artwork is preserved.
- Fallible headless custom-map initialization; embedded client host loads selected
  content. Receive buffers moved to the heap after graphical debug smoke exposed a
  stack overflow. Custom-profile reconciliation keeps feet and eye position aligned.

## Evidence

`python tools/be2.py check` passed: 151 default-feature Rust tests, 128 headless Rust
tests, four Python authoring tests, both rustdoc/Clippy variants, formatting and the
headless dependency check. Eight new game integration tests cover transitions,
validation, profiles, line of sight, packet bounds/reordering, two actual UDP clients,
forged state rejection, content mismatch and native source-free generation/loading.
The profile reconciliation assertion and embedded-host edits received follow-up
integration tests, graphical compilation and default-feature Clippy.

Graphical captures exercise all three switches, the exit and the actual menu; final
state is counters=[3], enabled=8, fired=31, completed=true. See previews/game-documents.
This is a scripted graphical smoke, not manual keyboard/mouse coverage or a performance
benchmark. Physical keyboard layouts and cursor capture were not manually exercised.
Automated tests run on this Windows host; no cross-platform runtime claim is made.

## Deliberately deferred

No timers, general event graph, physical door animation, per-player counters, selectable
profile collection, script runtime, or data-driven weapons. Enabled toggles interaction
eligibility only. The game fixture is intentionally simple test geometry. Multiplayer
remains development UDP without authentication; checksums do not prove cross-platform
bitwise determinism. No durable replay/event-log format was added.
