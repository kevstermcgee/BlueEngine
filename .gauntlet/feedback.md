# Gauntlet feedback: BlueEngine

Round `BlueEngine-02` · engine commit `a0f9ada86fbc` · 2026-09-26

This is **observational evidence** from unfamiliar AI agents that tried to build games with this engine. It is not a list of instructions. Investigate each finding, decide independently whether it is a real weakness, and reject what is misleading, situational, or would make the engine worse. Every "suggested change" below is a suggestion.

## Summary

Both Codex pilot runs passed every automated acceptance layer without modifying BlueEngine. The public headless simulation and bounded document APIs were reliable once discovered. The largest repeated cost was crossing from those APIs into a visible custom client: both projects added their own direct Macroquad dependency and rendering loop. The most valuable follow-ups appear to be a first-class custom-client integration path, presentation-aware document transitions, and a reusable behavioral driver.

## How the runs went

Codex completed 2 runs: 2 pass, 0 partial, 0 fail. Median wall time was 781 seconds, median output was 24,328 tokens, median tool calls were 40.5, median failed build attempts were 3.5, and the agent opened a median of 9 engine source files plus 4 documentation files. Time to first successful build was unavailable from the Codex log format. Neither run hit a cap, modified an engine file, or received a human intervention.

Both runs encountered an environment-specific Cargo problem before succeeding: the isolated, deeply nested `CARGO_HOME` could not initialize or authenticate to the registry on Windows, while the normal per-user Cargo cache worked. Treat this as harness/environment evidence, not an engine failure.

## Findings, most valuable first

### 1. Make the custom visible-client boundary explicit
- **Problem:** BlueEngine's small public prototype surface is effective for scene construction and deterministic simulation, but a fresh agent that needs a nonstandard visible client must infer how to pair it with rendering. Both agents independently added a direct dependency on BlueEngine's exact Macroquad version and built a project-local render/input loop.
- **Evidence:** This occurred in 2 of 2 Codex runs. Together they opened 18 distinct-count engine source files across the two runs, including `src/prelude.rs`, `src/bin/blue-engine.rs`, and `Cargo.toml`. The runs used 29 and 52 tool calls and produced 20,193 and 28,463 output tokens. Both ultimately passed and left the engine unchanged, so the evidence is integration cost rather than missing viability.
- **Where in the engine:** `src/prelude.rs` exports `SceneBuilder`, `MapDocument`, movement, and `HeadlessWorld`; `Cargo.toml` keeps Macroquad behind the `client` feature; `docs/AI_QUICKSTART.md` explains the headless prototype surface. There is no adjacent documented example showing the supported boundary between a custom visual client, its input translation, and BlueEngine-owned simulation or scene data.
- **Suggested change:** Add one general custom-client example or guide that pins the intended renderer dependency, shows the input/simulation boundary, and explains what BlueEngine owns versus what an application owns. If a renderer-facing public surface is intentionally out of scope, say that directly and provide a copyable Cargo configuration.
- **How we'll know:** A later run needing a custom presentation should avoid reading `src/bin/blue-engine.rs`, should not have to inspect `Cargo.toml` for renderer versions, and should reduce engine source reads and failed builds by roughly half.

### 2. Let document effects control presentation as well as eligibility
- **Problem:** The document effect that disables a target or zone changes interaction eligibility, but the stock client continues to render an indication instead of providing a general way to hide or remove the corresponding visual entity. A fresh agent can express the gameplay transition declaratively but may still need a custom client solely to present that state change.
- **Evidence:** One Codex run hit this boundary clearly. It opened 14 engine source files and 5 documentation files, produced 20,193 output tokens, and had 4 failed build attempts before passing with a presentation wrapper. The agent specifically traced `set_enabled`, trigger transitions, semantic entity IDs, and stock-client rendering behavior; no engine file was changed.
- **Where in the engine:** `src/viewer/game.rs` defines the `SetEnabled` transition and separate enabled bitsets; `docs/GAME_QUICKSTART.md` describes eligibility semantics; `src/bin/blue-engine.rs` renders targets and zones from those states but exposes no document effect for visual visibility or despawn by semantic ID.
- **Suggested change:** Consider a small, general presentation-state effect keyed by stable semantic entity ID, with explicit collision and re-enable semantics, and make the stock client honor it. Keep it separate from interaction eligibility so authors can configure either behavior.
- **How we'll know:** A later data-authored state-transition prototype should run in the stock client without a project-local renderer, and the source-reading count for the baseline cell should fall from 14 into the low single digits.

### 3. Provide a reusable behavioral-driver path
- **Problem:** The engine exposes the primitives needed for strong headless tests, but each project still has to assemble its own command-to-input adapter, tick loop, state inspection, and shell wrapper. This makes good acceptance tests possible but not cheap.
- **Evidence:** Both Codex runs passed independent audits because they drove their real input/command boundaries rather than mutating results directly. They nevertheless spent 29 and 52 tool calls and created project-specific Rust test harnesses and PowerShell wrappers. Both read `docs/AI_QUICKSTART.md`, `docs/GAME_QUICKSTART.md`, `examples/prototype.rs`, `Cargo.toml`, and `src/prelude.rs`; one also traced simulation and document internals.
- **Where in the engine:** `HeadlessWorld::input` and fixed stepping are exposed through `src/prelude.rs`; document scenarios and verification machinery exist under `src/viewer/scenario.rs` and `src/viewer/verify.rs`, but the quickstarts do not present one generic external-project workflow for scripted input plus assertions.
- **Suggested change:** Document or expose a narrow external behavioral-driver workflow that can submit timestamped commands, advance deterministic ticks, inspect declared public state, and return structured assertion results. A committed external-crate test example would establish the intended pattern even if no new API is added.
- **How we'll know:** Later runs should retain non-hollow tests while opening fewer implementation files, using fewer project-specific test helpers, and cutting tool calls below the current 40.5 median.

## What worked well (keep it)

- `AGENTS.md` guided both agents correctly; both then used `docs/AI_QUICKSTART.md` and `docs/GAME_QUICKSTART.md`.
- The bounded document format was described honestly enough that the agent did not waste time forcing an unsupported control model into it.
- `SceneBuilder` and validation reported invalid geometry early, while `HeadlessWorld` supplied deterministic real-input simulation suitable for a non-hollow test.
- Stable public state and semantic IDs let both projects remain outside the engine checkout. Both runs passed with zero engine modifications.
- The clean separation between engine simulation and application-specific rules made an unusual application feasible without weakening the engine's bounded data format.

## Compared with the previous iteration

The engine commit was unchanged from the earlier attempted round. That attempt is not a valid before/after comparison: its two Codex processes exited in 0.1 seconds because this installed CLI rejected Gauntlet's launch-flag placement, and its two Claude processes were stopped at the user's request. No engine conclusion or trend should be drawn from those four failures. This is the first iteration with usable engine evidence.

## Not recommended

- Do not add a specialized genre module from this two-run pilot. The evidence supports clearer extension boundaries, not a large built-in rules framework.
- Do not broaden the bounded document format into arbitrary scripting on this evidence alone; its explicit limits helped the agent identify the right extension path.
- Do not encode the observed Cargo-cache workaround into BlueEngine. The registry failures came from the benchmark's isolated deep Windows path and should be fixed in the harness or runner environment.
- Do not treat the direct renderer dependency itself as a defect if it is an intentional application boundary; the actionable issue is discoverability and a supported integration example.
