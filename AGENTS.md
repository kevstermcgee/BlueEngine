# Working on Blue Engine

Read README.md and BLUE_ARCHITECTURE.md for the viewer. Read AI_REFERENCE.md for Vesper scenes and ARCHITECTURE.md before changing the inherited offline renderer.

Keep the engine and authoring API native Rust. Keep player movement independent from the frame rate and from rendering. Preserve the library's unsafe-code prohibition; Windows input/focus queries belong only in the executable.

Preserve completed outputs on failure. Never shell-interpolate scene values. Keep semantic entity IDs stable for future interaction components.

Run `cargo fmt --check`, `cargo test --locked`, and `cargo clippy --all-targets --locked -- -D warnings` for engine changes. When changing visuals, render and inspect stills and the actual pause menu. Exercise both key layouts and cursor capture after input changes. Update the AI reference for scene-contract changes. Do not claim untested platforms or interactions work.
