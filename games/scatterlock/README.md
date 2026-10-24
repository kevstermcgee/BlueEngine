# Scatterlock

A native 3D spatial cargo puzzle built with BlueEngine. Dock all cargo in six chambers. There is no gravity, timer or hazard. Every cube stays the same size.

Choose a beacon. **Spread** doubles every loose cube's distance from it; **Gather** halves it, in all three coordinates simultaneously. Switching origins between pulses transports the formation. A cube docks only if its center finishes in its matching capture region. Docked cargo stops moving and remains solid.

Dotted paths and wire destinations preview the complete pulse. A red marker identifies the first obstruction. Swept collision rejects the entire pulse if any path intersects the chamber boundary, baffles, another moving cube or docked cargo. A rejected pulse costs nothing. Clear previews illuminate receiving sockets.

Valid pulses spend one allowance. If the last pulse leaves cargo loose, **Z** restores the previous formation and allowance; **R** restarts. Secure all cargo to unlock the next chamber. Finish chamber six to complete the game.

| Input | Action |
| --- | --- |
| Tab, 1–5, click beacon badge or sidebar | Choose origin |
| E / Q | Preview Spread / Gather |
| Space or Enter | Execute; advance when chamber is complete |
| Z / R | Undo / restart current chamber |
| Right mouse drag / wheel | Orbit / zoom |
| Escape | Pause menu |
| F5 / F9 | Engine-owned quick save / load, including undo history |

Chambers introduce origin-shift transportation, elevated deliveries, immobile cargo, three simultaneous paths, obstructive delivery order, and a five-pulse finale. Amber, cyan and violet have matching numbered cargo and wire berths. Beacon markers are non-solid.

The public mini-wiki is `wiki.json`, with the rules, exact controls, design rationale and four inspected native gameplay screenshots in `assets/wiki/`. The opening card uses centered BlueEngine Games text. The camera looks across the cargo formation so its separate paths stay visible, and berths carry matching numbers as well as colors.

In chambers four and six, B and D share an origin. Their world badges appear side by side with separate click targets and lines back to that origin. Cargo numbers and LOCKED labels travel with their cubes throughout each pulse animation.

## Development and verification

`cargo test --no-default-features` tests exact position scaling, swept obstructions, simultaneous collision, allowance exhaustion, undo, six public-input solutions, deterministic replay and Snapshot continuation. Control regressions exercise 240 Hz rendered frames against 60 Hz simulation ticks: a beacon click or number selection survives intervening quiet frames, the latest selection wins, and catch-up ticks consume each pulse once. `verification_input()` is a complete campaign route through the public Intent interface. `assets/verification.script` drives the same native controls via Lifecycle cues.

`cargo test --bin Scatterlock` also checks separate B/D badge hit targets at several UI scales and the shared cargo positions used for geometry and labels during movement and docking.

`cargo build` builds the native development client. Run the resulting `Scatterlock` binary. On a displayless Linux host, `xvfb-run -a python3 scripts/capture_campaign.py BINARY --out NEW_DIR` captures all six chambers using the public script cues and checks final completion. `xvfb-run -a python3 scripts/verify_controls.py BINARY --out NEW_DIR` exercises native X11 key and mouse events, including loss, undo, click, save/load, orbit and pause. Add `--opening-only` to inspect the opening card separately. Each command requires a new output directory and preserves previous evidence. The review children use a null ALSA device; these are software checks, not evidence of audible output or physical devices.

Windows x64 EXE installers are the delivery target. Linux is declared for development verification only. On Windows use the canonical `python scripts/ship.py ship --no-install`, then the installer prepare/build/test workflow. No shortcut installation or publication is performed by this task. Physical controls, hardware rendering and audible output remain native-review criteria in `assets/native-review.json`.
