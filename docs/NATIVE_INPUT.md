# Verify native controls through the authoritative game

Rule tests and `--verify`/`--script` prove simulation intentions. They do not prove that a window receives
keyboard and mouse events or that its device mapping produces those intentions. Keep both checks.
For a small documentation packet, use `python3 tools/be2.py context "native keyboard mapping" --level 1 --compact`.
The shared 2D client maps devices into `Intent`; customize `draw::Game::device_input(&mut self, input: Intent) -> Intent` for
pointer targets or alternate controls. The custom-sim starter uses `ClientInput` and its existing `Held`
match. Extend those mappings rather than writing another game loop for tests.

On Linux, `tools/xcapture.py --input SCRIPT.json` sends X11 XTest events into the game's isolated Xvfb
window. They pass through the native window backend, the real mapping, and the fixed-step simulation.
Install Xvfb and the system X11/XTest libraries. Build once, then change scripts without recompiling:

```sh
python3 tools/be2.py map new-game input-probe .be2-work/input-probe . two-d
cargo build --manifest-path .be2-work/input-probe/Cargo.toml
python3 tools/xcapture.py .be2-work/input-probe/target/debug/input-probe \
  --input docs/examples/native-input.json --frames 300 --timeout 90 --out .be2-work/input-shots
```

If `CARGO_TARGET_DIR` is set, use that directory's `debug/input-probe` instead. The sample assumes the
unmodified 2D starter's initial player position and action counter. Adapt predicates to your game's
snapshot fields and winning/losing transitions. Inspect the resulting PNG as well as the report.

Scripts are a list of at most 256 steps:

```json
[
  {"key":"d", "down":true},
  {"key":"space", "down":true},
  {"wait":{"field":"state.player.x", "gte":73}},
  {"key":"d", "down":false},
  {"key":"space", "down":false},
  {"expect":{"field":"accepted_input.action_ticks", "gte":1}}
]
```

Keys use X11 keysyms (`space`, `Escape`, `Return`, `Left`, `a`). Buttons are 1–5; `{"move":[200,150]}`
uses window-display pixels. Held keys stay down until release; multiple down events represent
simultaneous input. Repeated presses need releases and state waits between them so the window has
observed each transition. `wait` allows up to 15 seconds for a dotted report field to reach `eq` or
numeric `gte`; `expect` asserts the current report. At least one device event and one assertion are
required. Releases are cleaned up even on failure. Unknown fields, missing reports, assertion failures,
invalid scripts, or unexpected windows fail explicitly. The script and process have bounded deadlines.

The shared 2D and custom-sim clients emit `--input-report` JSON with readiness, tick, hash, paused/focused
flags and authoritative `state` captured by the existing `Snapshot` contract. This flag is opt-in;
normal runs do not serialize a snapshot every frame. A custom client can emit the same report at its
normal loop boundary, using its actual simulation snapshot. Never mutate state to satisfy the probe.
The driver refuses replay/autopilot flags and reports marked `verified`: injected events must reach the
ordinary native input path.

Cover press/release, held movement, repeat, simultaneous actions, mouse buttons/movement and menu or
pause transitions. Assert resulting state, including that releases stop an action. Test conflicts with
reserved client controls: Esc pause, R restart, K/L save/load, M mute and the start/menu action. A game
requiring conflicting meanings needs an explicit mapping or custom client. Pointer games should assert
the selected target or authoritative position, not merely a mouse-coordinate report.

`input-evidence.json` records X11/XTest as the source, assertions and the final snapshot. This is an
OS/window integration test on a virtual display. It does not establish physical keyboard/mouse behavior,
Windows device behavior, visual quality or audible sound. Test those on native hardware before claiming
complete playability. Headless rule/replay tests remain faster for exhaustive gameplay cases.

The [Corkscrew Key script](examples/corkscrew-key-native-input.json) is a complete-game example:
axis/sign keys, a screw step, undo, a mouse axis selection and hint state. Use its native binary with
`--size 1280x720 --frames 300`; its mouse coordinates assume that viewport. It asserts the existing
authoritative snapshot and does not reproduce the puzzle rules.
