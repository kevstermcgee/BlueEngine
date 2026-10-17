# Crosshaul

Two cargoes. One coordinate. Every route matters.

A native 3D spatial logistics puzzle with eight chambers. Click two hovering cargo
cubes, choose a world axis, and exchange only that coordinate. Both routes must be
clear. The cubes move together; neither takes the other's full position.

Match every cargo to its colored, symbol-marked dock within the chamber's exchange
allowance. A correct dock may need to be dismantled to lend a coordinate to another
cube. The third chamber explicitly requires this. Later chambers require height and
depth changes around shelves and screens, then introduce four cargoes.

- Left-click two cargoes, click the cargo list, or use **1–4** to select them.
- **X / Y / Z** choose width / height / depth. Destination frames, arrowheads and
  paired dashed routes preview both slides. Red highlights show the first obstacle.
- **Space** exchanges. Rejected routes and equal-coordinate commands are free.
- **Right-drag** orbits the perspective camera. **Wheel** zooms. World axes stay fixed.
- **Backspace** undoes an exchange and refunds its allowance, even after failure or
  during a slide. **R** restarts the current chamber. There is no timer.
- **Enter** advances after all cargoes dock. Complete chamber eight to finish.
- **H** gives one legal next-move hint. **Tab** shows the complete rules.
- **F5 / F9** use BlueEngine's quick-save and quick-load, including undo history and
  a slide in flight. **Esc** opens the shared native menu. **F** toggles fullscreen.

The filled boxes are cargo. Open frames are docks. Color-independent symbols are
A/+ (cyan), B/= (amber), C/X (violet), and D/: (lime). Plumb lines and floor rings
show each cube's horizontal projection; orbit to inspect hidden passages. Numbers
and the cargo list can select occluded cubes.

## Develop and verify

```sh
cargo run --bin crosshaul
cargo test --no-default-features
python3 scripts/check.py --skip-ship
python3 ../../tools/be2.py check --game games/crosshaul --loop inner
```

Run the last command from the engine checkout, or use its absolute path and this
folder's absolute path for `--game`. Simulation is a rendering-free fixed 60 Hz
`devkit::Simulation<Input = Intent>`. The native application uses shared
`ClientInput`, `GameShell`, `Lifecycle`, `Snapshot`, kit materials and meshes.
Integer axis sweeps test baffles, stationary cargo and relative partner movement.
Rendering reads state and interpolates the authority's synchronized travel interval.

`assets/chambers.json` contains authored positions, unit-cell baffles, allowances
and legal reference solutions. `assets/verification_input.json` is a complete
campaign of public `Intent` commands. `assets/verification_script.txt` exercises the
same selection/axis/exchange cues in the native client. Hints run breadth-first
search through the same collision rule as player input.

```sh
python3 ../../tools/xcapture.py /path/to/debug/crosshaul --frames 12,55 -- --script 'a@2,b@4,y@6,swap@15'
```

The native review criteria are in `assets/native-review.json`. Linux verifies rules,
saving, controls and software-rendered native captures. Delivery is a **Windows x64
EXE installer**, never a browser build. Windows resources and installer tooling are
provided by the canonical scaffold. On Windows, run `python scripts/ship.py ship
--no-install` for a packaged isolated smoke, then `installer prepare` / `installer
build` / `installer test` as documented in the engine's `docs/NATIVE_DELIVERY.md`.
Linux evidence does not certify a Windows installer or physical audio playback.
