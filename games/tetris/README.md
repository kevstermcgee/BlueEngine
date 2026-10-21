# Tetris

A native BlueEngine falling-block game with a classic 10x20 centered board and no side panels. Only two small upcoming pieces appear above it; score and statistics live in the pause/results panel. Marathon is endless; Sprint finishes at 40 lines; Ultra gives you two minutes to score. A blocked spawn or locked blocks in the hidden rows ends the run.

Arrow keys move, Down soft-drops, Up or X rotates clockwise, Z/Q reverses, C/Shift holds, Space hard-drops. 1/2/3 start a fresh Marathon/Sprint/Ultra run. Esc pauses, R retries the current mode, K saves, L loads, M toggles sound, N toggles music and F toggles fullscreen. Controllers: D-pad/left stick move; D-pad down soft-drops; D-pad up or Y/Triangle hard-drops; A/Cross rotates clockwise; B/Circle reverses; X/Square or either bumper holds; Start/Options pauses; Back/Select retries; right-stick click cycles modes.

SRS wall/floor kicks, seven-piece bags, 500 ms lock delay with at most 15 movement resets, two clear next-piece previews, ghost landing and once-per-piece hold. Clears score 100/300/500/800 times level, T-spins 400/800/1200/1600, back-to-back difficult clears multiply by 1.5 and combos add 50 per step. Perfect clears add 3500 times level. Soft/hard drops score 1/2 per row. Level rises every ten lines.

Built with the latest BlueEngine main at project creation (b2d559b). Simulation is fixed-step and rendering-free; snapshots retain the bag, generator, inputs, timers and pending line clears.

`cargo test --no-default-features` tests rules. `python scripts/ship.py ship --no-install` packages and checks the native build. Windows installers are published through the engine community release pipeline.

The original shaded tetromino icon is reproducibly rendered by `python3 scripts/make_icon.py`; it contains all native Windows shell sizes and transparent corners.
