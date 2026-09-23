# Blue Engine 0.1 validation

Verified on Windows x64, September 22, 2026.

## Maximized window startup update

All 58 tests, formatting, strict Clippy and the locked release build passed. The final executable's capture reported `maximized=true`, `caption=true`, and a 1024x697 client viewport on the available 1024x768 test display. A visible desktop launch was inspected: the title bar shows the Restore control and the app fills the work area with a centered menu. The desktop shortcut was updated and reread to verify WindowStyle=3 and its unchanged engine target.

The app uses Windows' own maximization of its current monitor, rather than hard-coded full-screen dimensions or manual centering. Physical multi-monitor and mixed-DPI hardware configurations were not available for a separate test; no exhaustive multi-monitor test is claimed. F11 remains opt-in fullscreen. Changes apply to Windows startup; no cross-platform maximized-startup claim is made.

## Object interaction update

58 tests pass (44 library, 1 input mapping, 13 CLI); formatting, strict Clippy and the locked release build pass. Five new interaction tests cover E/click equivalence and pause/capture guards, monitor toggling and contextual prompts, crystal state isolation (including the pedestal), out-of-range/occluded/empty targeting, inspection feedback, expiry and dismissal.

The final executable's `--capture-interactions` run produced six rendered views: monitor on, monitor off, blue crystal, amber crystal, notebook information, and pause menu. The actionable prompts, feedback wrapping, visual state changes and menu were inspected. A pedestal tint discovered in the first visual pass was corrected and rechecked. The standard three-view capture also completed cleanly.

The updated Windows app was opened and its pause menu checked; entering with a mouse click and pressing E without a reachable target did not trigger an object action. E/left-click action equivalence is covered by tests; the state-changing visual checks use the scripted application capture path rather than claiming a complete human playthrough of every object.

## Faster movement and sprint update

Normal walking increases from 2.6 to 3.2 m/s (about 23%). Holding either Shift key now explicitly sprints at 5.6 m/s; release transitions smoothly back to walking. Crouching remains 1.3 m/s and overrides sprint. The HUD and pause menu identify Shift as Sprint.

53 tests pass (39 library, 1 input mapping, 13 CLI). The new sprint test checks diagonal normalization, steady sprint speed, smooth release to walking, and crouch priority at 30, 60 and 144 Hz. Existing wall collision and long-frame tests run at the new sprint speed. Formatting, strict Clippy, and the locked release build passed. A fresh executable capture completed without graphics warnings.

## Jump and crouch update

- 52 tests pass (38 library, 1 input mapping, 13 inherited CLI integration tests).
- Formatting, strict Clippy, and the locked release build pass.
- Seven new controller tests cover 35 cm jump height at 30/60/144 Hz, return to ground, rejection of midair jumps, ceiling collision, smooth crouch and reduced speed, blocked standing under a low overhang, landing on a low ledge and falling off it, and airborne crouch/pause preserving vertical motion.
- The final executable's `--capture-motion` path rendered the real controller at jump, crouch and standing states. Reported eye heights: 2.0295687 m, 0.98 m and 1.68 m. Jump and crouch screenshots were inspected. Startup was 0.475 s and the short capture averaged 16.490 ms per frame on this machine.
- Added Space, both Ctrl keys and C to the native input path; on-screen hints and the pause menu describe the controls. The actual updated app was opened successfully and its new HUD inspected. The jump/crouch visual check is scripted; it is not a claim of an extended physical-keyboard play test.

The earlier release checks below remain historical evidence for the inherited controls and renderer.


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


## Wrench update

Validated on Windows: all 61 tests passed (47 library, 1 viewer, 13 CLI), cargo fmt --check, strict all-target Clippy, locked offline release build, and diff whitespace checks. Three new melee tests cover wind-up/contact/recovery, one hit per swing, range, nearest-surface occlusion, cancellation, current aim at contact, and 30/60/144 Hz timing.

The --capture-wrench run rendered and was visually inspected at idle, contact, recovery and pause menu. Its report recorded exactly one hit, a 1024x697 viewport and 16.937 ms average frame time including presentation. The hand/wrench, amber hit marker, target label and contact sparks were visible. A live launch exercised Enter, W, Up, mouse look via pointer movement, left-click and Escape; the live snapshot occurred after the short swing, so precise contact timing is established by the deterministic render and unit tests. The actual pause menu was inspected. Executable hashes confirmed bin/BlueEngine.exe matches the tested release build. The existing desktop shortcut target is unchanged.

Objects receive impact feedback but are not destructible or movable. No audio added. Other platforms were not tested.

## Default character and perspective update

All 65 tests passed (51 library, 1 viewer, 13 CLI), along with formatting, strict all-target Clippy and the locked offline release build. Camera tests cover perspective round trips, first-person ray preservation, player-origin targeting, shoulder aim convergence, corner sweeps and wall/ceiling clearance at extreme pitch.

Rendered character captures were inspected for first/third person, wrench swing, crouch, jump, a front portrait and the actual menu. A third-person wrench render showed the character, hit marker and monitor feedback. Camera convergence and reach are covered separately by the camera and melee tests. The live check on September 23 verified Q in the pause menu without quitting, Enter to enter third person, W and Up movement, Q in both directions during play, and Escape returning to the pause menu. The executable used by the existing desktop shortcut matches the tested release build by SHA-256. No cross-platform or extended play-session claim is made.
