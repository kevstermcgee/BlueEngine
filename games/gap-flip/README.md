# Gap Flip

Trade empty space. Find a way together.

A small offline spatial puzzle with ten authored rooms, native 2D presentation, and no terminal failure. Windows x64 EXE installers are the delivery format; Linux is a verification target.

## Play

Left-click a token, then **Horizontal Flip** or **Vertical Flip**. Cyan and amber dotted bands show the empty cells on its two sides. The outlined ghost shows the exact landing. Click that ghost to commit; the two bands exchange sides.

A token reflects within its uninterrupted floor corridor: destination = first cell + last cell - current cell. Walls, edges, and other tokens end that corridor. Equal gaps disable the flip and cost nothing. Goals do not block, lock, or change a token. The letter and color of each goal match its token.

Match every symbol simultaneously to light **Next room**. Match the tenth room and click **Finish**. A solved token sometimes has to leave its goal to make another token's landing possible.

- **Undo** reverses the last committed flip, without a limit, including after a room is matched.
- **Restart** or **R** resets the current room; earlier room progress stays intact.
- **Esc** pauses. **K** saves. **L** loads the complete room, selection, preview, move history, and animation through engine Snapshot storage.
- **M** toggles sound; **F** toggles fullscreen.

There are no timers, hazards, move budgets, pushing, or terminal losses. Experiment freely. Undo and restart are the retry loop.

## Run and verify

From the engine checkout:

```sh
cargo run --manifest-path games/gap-flip/Cargo.toml
python3 tools/be2.py check --game games/gap-flip --loop inner
python3 tools/be2.py check --game games/gap-flip --loop integration
python3 tools/be2.py check --game games/gap-flip --loop shipping
```

For the current evidence and remaining native gates, see [STATUS.md](STATUS.md).

Headless rule tests include an exact breadth-first solver, exhaustive two-token corridor reversibility, neighbor-boundary dependencies, a solved helper that must leave and return, disabled/invalid actions, unlimited undo, restart, deterministic public-input campaign completion, and exact Snapshot continuation and rejection. The authored shortest solution lengths are 1, 4, 5, 8, 9, 10, 11, 12, 14, and 16 flips.

If the project checker cannot find native tooling in a configured shared target directory, set `BE2_TOOLS` to the fresh `be2-tools` path reported by `python3 tools/be2.py map describe`.

The native `--verify --mute` route selects tokens, clicks axis buttons, commits ghosts, undoes/repeats the first move, and completes all ten rooms. It uses the same Intent path as mouse play. Captures and X11 input checks are separate evidence; they do not establish new-player comprehension or human enjoyment.

From this game on Windows:

```sh
python scripts/ship.py ship --no-install
```

This builds the game's own icon and executable resources, packages the native game, and runs isolated packaged-game smoke without installing shortcuts. A Linux check does not certify Windows resources or the Windows installer.
