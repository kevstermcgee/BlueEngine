# BE2 validation — 2026-09-23

## Passed locally on Windows x64

- `cargo fmt --check`.
- `cargo test --locked`: 70 tests (55 library, 2 client input, 13 offline CLI).
- `cargo clippy --all-targets --locked -- -D warnings`.
- Default release build, including client and inherited offline CLI.
- `cargo test --locked --no-default-features`: 48 tests.
- `cargo clippy --all-targets --locked --no-default-features -- -D warnings`.
- Release headless build with default features disabled.
- Headless dependency tree contains Serde/JSON only; no Macroquad, Miniquad, image/PNG, Ctrl+C/window library or networking transport.
- Two-player headless benchmark: 60,000 ticks in 33.692 ms (0.562 microseconds per tick) on this host. This exercises movement, stance, jumping and room collision, without networking or game rules. It is not a VPS capacity estimate.
- Paced runner: 60 ticks in 1000.202 ms. Headless Windows executable: 355,840 bytes.
- Prop exporter produced four standalone scenes; each compiled through the engine in a test. Stable prop entities have matching collision bounds, and the player spawn is clear.
- Release room, props and character capture modes completed. PNGs were inspected, including the actual rendered pause menu, clean HUD, softer scene lighting, and reduced first-person wrench coverage. Selected images are in previews/.

## Smoothness and correctness

Movement tests cover 30/60/144/240 Hz presentation rates, bounded long-frame catch-up, a jump queued across a sub-tick frame, reset interpolation, matching headless/client motion (floating-point tolerance), invalid input, maximum player count, existing wall/ceiling collision and jump/crouch behavior. Both keyboard layouts and between-frame press/release edges have automated tests.

Native UI automation successfully clicked Enter the room. Keyboard injection did not provide reliable observable results in the test session; live WASD/arrows, mouse capture/release and Escape/Q should receive a human playtest. The regression fix preserves event-subscriber press edges instead of relying on a brief native-poll sample. Do not interpret unit tests as a completed hardware-input playtest.

## Performance observations

The final static room, including four added props, has 51,836 triangles and 38,918 shared vertices. The equivalent unshared triangle stream has 155,508 entries: about 75% fewer vertex entries. The baked shade cache avoids recomputing identical vertices. Tool buffers reuse their meshes and the character reuses geometry capacity.

An initial 1024x697 capture measured original startup at 0.657 s versus BE2 at 0.366 s. Mean frame times were 17.338 versus 17.366 ms, near display/presentation pacing; this does not demonstrate an FPS gain. The final hidden-window 960x600 smoke measured startup at 0.305 s and mean frame time at 17.029 ms. Different viewports and short, vsync-paced captures are not controlled GPU benchmarks.

## Remaining scope

Linux compilation, actual Debian/Ubuntu VPS memory/CPU usage, GPU/driver coverage and sustained playtesting are not verified here. CI contains Linux and Windows checks but was not executed remotely.

PulseNet transport integration, authentication/session lifecycle, packet sequencing, snapshots, prediction/reconciliation, server combat and interactions, player collision, disguises, prop possession, rounds and scoring are not part of this release. Props are static inspectable/hittable assets. The headless executable is a local simulation/benchmark, not an online listener.
