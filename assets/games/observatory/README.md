# Observatory Night Watch

This small stock fixture substitutes for **Night Shift at the Observatory**, which was unavailable in this
checkout. It runs in the existing stock executable; it is not a separately packaged/published game project.
Open the initially closed amber shutter, turn around the telescope baffle, calibrate the blue control three
times, then use the green uplink. The authoritative 20-second battery expires if the watchkeeper waits.
E interacts/restarts, WASD/arrows move, Escape opens the stock menu.

Reproduce the content using supported operations (build the toolkit once):

```sh
cargo build --locked --profile fast --bin be2-tools --bin be2
python assets/games/observatory/author.py NEW_DIRECTORY --tools target/fast/be2-tools
target/fast/be2-tools sim NEW_DIRECTORY/win.json
target/fast/be2 --game assets/games/observatory/content/game.json --scenario assets/games/observatory/content/loss-restart-win.json --capture NEW_CAPTURE_DIRECTORY
```

On Windows use `.exe` suffixes. The recipe builds two rooms and a doorway from a blueprint, adds the shutter
and baffle with `add_box`, creates all five related records for each control with `add-interactable --write`,
and moves the calibrator by 0.2 m using a canonical translate patch. No duplicated bounds are recalculated
by the recipe. Rules, profile and presentation are authored as documented JSON. `game-validate` validates
every intermediate game and the final exact visual/collision/entity agreement. Outputs must be new directories.
The recipe also generates a physically verified win and runs game-aware lint with that evidence.
The committed blueprint, patches and authoring evidence document the operations; temporary intermediate maps
are reproducible and omitted from the fixture.

`content/win.json` is a physically verified 341-step route, rather than just the abstract winning sequence.
`content/loss-restart-win.json` loses at step 1200, restarts at 1201, and completes at 1542. Its assertions check
loss, battery depletion, reset counters/battery, and the later physical win. Repeated presses use `wait_ticks:1`
to protect sequencing even when earlier movement blocks their scheduled timestamps.
`content/closed-gate.json` deliberately omits the shutter-opening action: abstract exploration finds the same
winning sequence, but physical generation refuses to write a verified win. This failure is an expected
negative result, not a passing physical run.

Static `lint`/`reach` still describe **initial** map collision: they report targets behind the closed shutter
as initially unreachable. `lint edited-map.json --game=game.json --scenario=win.json` retains those findings as
`initial-unreachable-now-reached` information only after observing each enabled control within authoritative
reach/line of sight in the verified run. A passing loss-only scenario leaves both errors unresolved; a failed,
mismatched or spawn-overriding scenario is refused. No mover is exempted. This demonstrates a selected route;
the closed-gate variant remains unproven. All engine verification gates remain intact.

`tests/stock_observatory.rs` reproduces canonical authoring, rejects mismatched content, executes the turning
route and changing shutter, checks body-profile routing, compares separated/coalesced repeated interactions,
and verifies loss/restart/win plus the rendered-session checksum. It also covers HUD defaults, formatting and
invalid configuration. Real frame captures are documented in the implementation evidence report.
