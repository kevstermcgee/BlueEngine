Corkscrew Key is implemented at games/corkscrew-key with six native 3D chambers, fixed-step screw movement, continuous swept collision previews, unlimited undo/retry, optional hints, and engine Snapshot saving. Windows delivery and Linux verification are declared.

Verified locally:
- Game integration: eight tests pass in both headless and native-feature modes, including identity, all public-input solutions, swept collision, deterministic replay, exact save continuation, and a state-space search proving the final chamber requires vertical movement.
- Game Clippy passes with warnings denied. The engine input extension's seven focused tests, Clippy, rustdoc, and the headless boundary check pass.
- Native replay reaches the final win; inspected captures are under .blue-check/polished-captures. The key has chamfered colored arms, perspective depth, an orbit camera, fixed world-axis labels, goal and sweep outlines, and explicit harmless rejection feedback.
- A real XTest input session exercised Space, Z, K, and L through the native window. Inspected the dock, undo and loaded pose; further live controls could not be completed reliably because later virtual-display launches failed. Automated public-input routes are separate evidence.
- Linux release packaging and identity/icon/payload integrity verification pass. No shortcuts were installed.

Remaining supervisor gates:
- Windows x64 installer, executable resources, and isolated Windows packaged-game smoke.
- Repeat the complete live keyboard/mouse route, including axis/sign changes, R, hints, camera orbit/zoom and pause.
- The attempted Linux shipping gate and isolated release capture failed to open the virtual display (XOpenDisplay). Package integrity passes separately; isolated native smoke is not certified here.
- Full engine check was attempted: 610 Rust tests passed; 13 localhost networking tests failed with sandbox socket PermissionDenied. The independent Python batch also failed its socket-based server-load fixture. Full Linux/Windows checks remain required; no tests or policies were weakened.

The compatible engine change is limited to draw::Game::device_input, its shared-client integration, a retained-command regression test, and docs/PORTABLE_GAMES.md. No browser game, publication, installation, commit, or learning-ledger changes were made.
