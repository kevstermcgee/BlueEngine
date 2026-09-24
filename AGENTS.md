# Working on Blue Engine

For **content authoring**, start with `python tools/author.py describe` and tools/AUTHORING.md. Use query/assets/recipes/schema and native map tools; do not load engine source into context. The supported workflow is static maps and inspectable props. Report unsupported gameplay requirements as engine work rather than inventing APIs. The architecture-reading and Rust-check requirements below apply to **engine maintenance**, not data-only authoring.

For engine maintenance, read README.md and BLUE_ARCHITECTURE.md for the viewer. Read AI_REFERENCE.md for Vesper scenes and ARCHITECTURE.md before changing the inherited offline renderer.

Keep the engine and authoring API native Rust. Keep player movement independent from the frame rate and from rendering. Preserve the library's unsafe-code prohibition; Windows input/focus queries and own-window lifecycle calls belong only in the executable.

Preserve completed outputs on failure. Never shell-interpolate scene values. Keep semantic entity IDs stable for future interaction components.

Run `cargo fmt --check`, `cargo test --locked`, and `cargo clippy --all-targets --locked -- -D warnings` for engine changes. When changing visuals, render and inspect stills and the actual pause menu. Exercise both key layouts and cursor capture after input changes. Update the AI reference for scene-contract changes. Do not claim untested platforms or interactions work.

For BE2, read BE2_ARCHITECTURE.md first. Also validate cargo test --locked --no-default-features and cargo clippy --all-targets --locked --no-default-features -- -D warnings. Keep PulseNet separate from the renderer.

## Agent editing tools

Read tools/README.md and tools/FEATURES.json before choosing an edit path. Use `python tools/be2.py doctor` for setup, `map help` for the native editor, and `check` for required validation with persistent logs. Map documents load through `--map FILE` in both client and headless runtime. The default map remains procedural Rust unless explicitly changed.

Map edits should use explicit IDs and preserve matching visual, collision and entity components. Export into new files, review `diff`, run relevant `route`/`ray` checks, and inspect captures. Generated room-N/collider-N IDs are stable within one exported document, not guaranteed across new exports from modified Rust. Keep new object IDs stable. tools/README.md explains schema limits and the distinction between data edits and code feature edits.

The toolkit is project-local; do not install plugins or add external services merely to use it. Update the feature index and editing guide when adding a new subsystem or tool. Package commands include tracked working files and newly built binaries; stage intended new files first, and report dirty state and checks honestly.

## Compact engine map

BE2 is one Rust package with the compatibility library name `vesper3d`. The
`be2` client and `be2-headless` runner share concrete movement/simulation types;
there is no orchestrator, online match server or shared-trait transport yet.
`client` gates graphics/audio; `offline` gates the inherited output renderer.

- Feature-to-file/check lookup: tools/FEATURES.json (maintain this existing map).
- Simulation contract: src/viewer/simulation.rs; physics: src/viewer/controller.rs.
- Static map contract: src/viewer/authoring.rs; CLI: src/bin/be2-tools.rs.
- Client wiring/UI: src/bin/blue-engine.rs; headless driver: src/bin/be2-headless.rs.
- Current architecture: BE2_ARCHITECTURE.md; decisions: docs/adr/README.md;
  vocabulary: docs/GLOSSARY.md; context review: docs/CONTEXT_REVIEW.md.
- Behavior examples: tests/simulation_flow.rs and tests/authoring.rs.

Build: `cargo build --locked` or
`cargo build --locked --no-default-features --bin be2-headless`.
Run `python tools/be2.py check` for the complete checks, including library rustdoc.
Read API docs with `cargo doc --locked --no-deps --lib --open`; add
`--no-default-features` for the rendering-free surface. Document public contract
changes and failure/edge semantics alongside code. Add ADRs for meaningful design
decisions; do not add traits or duplicate generated indexes solely for navigation.
CLAUDE.md imports this file; keep shared guidance here.
