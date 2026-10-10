# Odd Matter

**More matter. Less wall.** An offline native spatial puzzle made with BlueEngine.
Guide a small drone into the green exit socket in six 5 × 5 × 5 voxel vaults.
Depth, height and sideways position all affect the routes and overlap junctions.

Fixed matter contributes one layer. Each movable bar adds another layer in every
cell it covers. Odd layer counts are solid; zero and even counts are empty.
Bars slide through matter and each other on their marked rails. Their size and
orientation do not change. The outer frame always blocks the drone.

A bar movement leaves the drone stationary. If its cell becomes solid, the drone
is crushed. **Z undoes that action immediately**, including the entire occupancy
change. R retries only the current vault. There is no timer, move limit or undo
penalty. Park outside a moving bar's swept volume before relocating its passage.

| Control | Action |
| --- | --- |
| 1 | Select drone |
| 2 / 3 / 4 | Select amber / rose / lime bar, when present |
| A / D | Move left / right along X |
| Q / E | Move down / up along Y |
| W / S | Move back / front along Z |
| Right mouse drag / wheel | Orbit / zoom the 3D camera |
| Hold C | Cut through the drone; hidden solids stay as faint outlines |
| Hover direction buttons | Forecast green openings and red closures |
| Shift + direction | Preview without executing; release and press the direction to move |
| Z / R | Undo one action / restart this vault |
| Enter | Advance after an exit; play again after the sixth exit |
| H | Toggle the current vault's field notes |
| F5 / F9 | Engine quick-save / quick-load, including the undo trail |
| Escape | Shared native pause, controls, audio settings and quit menu |

The direction and selection buttons also accept clicks. Each key press moves one
cell; holding a key does not repeat. World axes stay fixed when the camera orbits.
The labeled compass follows the camera, and three cross-section diagrams show
exact layer counts at the drone's current depth, height and sideways slice.

The first vault teaches two-layer cancellation. The second requires a depth
crossing. The third hands a horizontal passage to a vertical shaft. The fourth
restores a plug with two bars, then reopens it with a third. The last two require
safe berths, passage reuse and junctions at different heights and depths.

## Development and verification

The game uses the canonical `custom-sim` scaffold and icon tooling. `src/lib.rs`
is the only rule authority. Public `Intent` commands advance via BlueEngine's
fixed-step `Lifecycle`; the renderer reads parity and the same read-only forecast
used by the simulation. `Snapshot` declares `SavePolicy::Exact`; the engine owns
save framing, checksums, atomic slots and restore. No browser runtime is present.

From the engine checkout:

```sh
python3 tools/be2.py map describe
export BE2_TOOLS="${CARGO_TARGET_DIR:-$PWD/target}/itest/be2-tools"
python3 tools/be2.py check --game games/odd-matter --loop inner
python3 tools/be2.py check --game games/odd-matter --loop integration
cargo clippy --locked --all-targets --manifest-path games/odd-matter/Cargo.toml -- -D warnings
cargo build --locked --manifest-path games/odd-matter/Cargo.toml --profile fast
```

`map` builds fresh canonical authoring tooling. The generated native check script
currently searches release/fast/debug automatically, so select the fresh `itest`
binary with `BE2_TOOLS` (or `scripts/check.py --tools PATH`). If your target directory
differs, use the executable path printed by `map`. Native scripts are unchanged
copies of the canonical scaffold; refresh them through a temporary `custom-sim`
scaffold instead of patching their tool discovery locally.

The rule tests run complete authored routes through public input and assert parity,
crushing, undo, blocked moves, safe relocation, restart, deterministic replay and
exact save continuation. `verification_input` executes those same public commands
in the native client with `--verify-route` (allow at least 1,500 frames).

Use `tools/xcapture.py BINARY --size 1280x800 --frames 30,300 -- --script
'bar1@5,up@15,drone@25,right@35,cut:0-300'` for native frames. On Linux, the optional
`scripts/test_native_controls.py` sends actual X11 keyboard and mouse events to the
window, without script input or an automated winning route. Run it inside
`xvfb-run` with the binary path and a new output directory. Inspect its PNGs.
It checks preview, cutaway, crush/undo, save/retry/load, an exit, orbit, zoom and
shared pause/quit. This does not certify physical Windows devices or speaker output.

**Delivery is a Windows x64 EXE installer. Linux is a verification target.**
On Windows, run `python scripts/ship.py ship --no-install`, then the game check.
Linux can verify its native package and isolated smoke with the same command
under a virtual display. Do not install shortcuts, publish or substitute a Linux
package for the required Windows installer. Cross-platform release verification
must be rerun on Windows by the supervisor.

The latest Linux review captures and real-control evidence are in
`.blue-check/review-campaign/` and `.blue-check/review-controls/`. The control test
uses actual device events with no scripted route. The campaign capture uses
108 public intentions and reaches the sixth exit. These are development evidence;
neither certifies unfamiliar-player fun or physical Windows devices.
