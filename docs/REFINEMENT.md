# BlueEngine refinement — 2026-09-24

> Historical validation snapshot. Protocol 3, GameDocument v1 and optional
> HMAC-authenticated UDP sessions were added later. Use
> [BE2_ARCHITECTURE.md](../BE2_ARCHITECTURE.md), the root README and
> `be2-tools describe` for the current contract.

## Goal and scope

Continue BlueEngineAntigravity at 3b337cc as BlueEngine, preserving history and MIT
attribution. Your earlier authoring task prioritized small, stable tools and minimal
engine-source reading. The supplied AI reviews were assessed against current code;
they were proposals, not instructions to implement every suggested subsystem.

This pass prioritizes trustworthy discovery, prototype ergonomics and concrete
network correctness. No legacy-map polish or large game implementation was added.

## Delivered

- `src/prelude.rs`, `viewer/builder.rs`: owned declarative SceneBuilder over the
  existing MapDocument validator. Boxes keep geometry/collision/entity data together;
  catalog props retain reusable meshes and physics. Fallible world initialization.
- Stable-ID `HeadlessWorld::impulse` and copied `prop_position` avoid body-index and
  borrowing boilerplate. Existing low-level and compatibility APIs remain available.
- Native `describe`, bounded `search`, and `export-lab`. Command signatures drive
  CLI validation/help/discovery; the existing feature index supplies source/check
  evidence. Python describe includes native discovery and supports `BE2_TOOLS`.
- A **408-token** quickstart (OpenAI o200k_base tokenizer), with an exact compiled
  **30-line** moving-character/physics example. A test prevents example/doc drift.
- Protocol **2** requires an initial map fingerprint. Canonical graph ordering makes
  fingerprints repeatable. Mismatched maps and full servers reject sessions.
- Malformed/oversized UDP is discarded instead of terminating the server; whole
  datagrams are received before enforcing 1400 bytes. Receive work is bounded.
- Overlapping room membership resolves by smallest stable ID, avoiding randomized
  HashMap iteration affecting spatial interest.
- Current README/architecture/capability truth replaces contradictory multiplayer
  claims. Three ADRs document format/naming, discovery evidence and the handshake.
- New focused tests cover builder transactions, physics/movement, discovery/parser
  agreement, Test Lab export preservation, fingerprints, capacity and garbage packets.
  CI now also verifies headless dependencies and fresh-native Python authoring.

## Validation

`python tools/be2.py check` passed on Windows: formatting, rustdoc and Clippy with
warnings denied in both configurations, **143 default-feature tests**, **120
headless tests**, the headless dependency guard, and **four Python authoring workflow
tests** against the freshly built native binary. The 30-line prototype also ran.
Linux/Windows hosted CI is configured; local results do not claim a hosted run.
The original multiplayer suite includes 12 scenarios covering real UDP, a separate
server process, loss/reordering, ownership contention and combat occlusion.

A debug-build diagnostic run measured mean simulation tick 680 microseconds,
snapshot creation 0.71 microseconds, delta creation 0.28 microseconds, and room lookup
207 nanoseconds. These are single-machine smoke measurements, not a calibrated
release benchmark, before/after speedup or regression guarantee. Existing replay-test
verified four checkpoints across 120 ticks of two in-memory runs.

## Limits and next three priorities

1. **Versioned game documents and generic gameplay boundaries.** Add explicit spawn,
   body/controller and small trigger/counter/action contracts shared by both hosts.
   Scientist/Feta/weapons remain demo-coupled. The builder is not a general event
   loop, arbitrary-mesh rigid-body or gameplay scripting framework.
2. **Bounded production replication and sessions.** Add compact binary encoding,
   snapshot chunking/bandwidth budgets, server-issued reconnect tokens and an
   authenticated transport. At the time of this pass, JSON/UDP and FNV map
   fingerprints were development compatibility checks rather than authentication or
   encryption; both peers had to rebuild for protocol 2.
3. **Durable replay and measured performance guards.** Record versioned traces with
   content identity and ordered joins/inputs, compare Windows/Linux fixtures, then
   establish release/allocation baselines before changing lazy-body architecture.
   Existing replay-test is in-memory; all eligible rigid bodies still allocate up front.

No claim is made that curated prose is mechanically proven: evidence references and
executable workflows are checked, while architectural descriptions still need review.
No manual graphical two-client or input-device smoke test was performed in this pass.
