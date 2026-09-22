# Working on Vesper3D

Read AI_REFERENCE.md for scenes; read ARCHITECTURE.md before changing the engine.
Keep the renderer and authoring API native Rust, bounded and deterministic.
Preserve existing completed outputs on failure. Never shell-interpolate scene values.
Use `cargo fmt --check`, `cargo test --locked`, and `cargo clippy --all-targets --locked -- -D warnings` for engine changes.
When changing visuals, render and inspect a still or contact sheet as well as running tests.
Update the AI reference with every scene-contract change. Do not claim untested backends or formats work.
