# Blue Engine 0.1 validation

Verified on Windows x64, September 22, 2026.

- `cargo fmt --check`: passed.
- `cargo test --locked --offline`: 45 tests passed (31 library, 1 viewer input mapping, 13 inherited CLI integration tests).
- `cargo clippy --all-targets --locked --offline -- -D warnings`: passed.
- Release build with locked dependencies: passed.
- Final executable render smoke check: three camera PNGs generated without graphics warnings. Earlier geometry-batch clipping and inverted menu issues were fixed and rechecked visually.

Movement tests cover matched WASD/arrow mappings, simultaneous aliases without doubled speed, opposing keys, normalized diagonals, 60/144 Hz agreement, yaw-relative movement with fixed eye height, pitch clamps, invalid time steps, stopping, wall sliding, and extended simulated movement remaining inside the room. The inherited offline tests still exercise PNG output, MP4 encoding, cancellation, scene validation and output protection.

The actual Windows app was opened and checked through desktop automation. Verified: the entry button, Enter to resume, W and Up moving forward, Right strafing, F3 statistics, mouse-driven view rotation, Escape pause, Quit, and automatic pause after switching to Explorer and back. Keyboard taps were verified using the displayed camera coordinates. The Windows key-state fallback was added after the automation's scan-code-free key injection did not reach the graphics library's normal input path. This is a short functional check, not an extended human play session.

The final capture run reported 0.470 seconds to prepare the scene and meshes, 50,672 triangles, 17 mesh batches, and 16.963 milliseconds per displayed frame (approximately 59 FPS, including presentation and screenshot overhead). The live window generally showed 58–61 FPS. These are measurements of this machine, not guarantees for other GPUs or resolutions. The capture viewport was constrained by the test desktop; no 4K performance claim is made.

The library's unsafe-code prohibition is retained. Small read-only Win32 calls are confined to the executable for focus and keyboard state. The executable needs no network, FFmpeg, installation, or runtime scene files.

Not established: Linux/macOS execution, long-duration soak testing, every graphics driver, physical hardware held-key feel, extreme high-DPI combinations, or dynamic lighting/physics. The live prototype deliberately has static scene geometry and no object actions. The pre-existing CI file is retained, but remote CI has not run for this local repository. Original Vesper release measurements are in VESPER_VALIDATION.md.
