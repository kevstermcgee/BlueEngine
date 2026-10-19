# Afterpull

Leave a little gravity behind. A compact native, top-down spatial puzzle in six rooms.

Walk onto an empty tile. The square you left scans four cardinal rays: the first visible crate on each ray proposes moving one tile toward your footprint. Walls, your new position, and the first crate block sight. All proposals use the same board. Crates proposing the same destination jam and stay still; other pulls proceed. Pulls never chain. You cannot push crates. Docks do not anchor them.

Fill every gold dock simultaneously, then press Enter to continue. The last room introduces open pits: losing a crate fails the room immediately. There is no clock or move limit. Unlimited undo and retry remain available after failure and completion.

- Tap WASD or an arrow key to step one tile. Holding a direction does not repeat.
- Hold Space with a direction to preview without spending a turn. Mint arrows show pulls, red crossed arrows show jams, red arrows into pits warn of failure. The forecast counts moving crates, jammed crates and dock arrivals, with an explicit pit warning. Release Space and tap a direction to commit.
- Z undoes the entire previous turn, including a failed or clearing turn.
- R retries the current room. Enter advances a cleared room or finishes the journey.
- Escape pauses. K saves and L loads through the shared engine Snapshot storage. M toggles sound; F toggles fullscreen.

Preview commands retain their preview meaning while waiting for a fixed tick, even if Space is released first. A direction pressed in the same native frame as a Space release is also treated as a preview; release Space, then make a fresh tap to commit.

Focus loss freezes puzzle ticks and discards pending movement, previews, undo and room-advance commands. Return to the window, release all controls, then make a fresh tap; no queued turn is replayed. Escape still provides manual pause.

The engine's shared `two_d::client` owns native focus observation, physical held-state queries and the return-to-neutral gate through `viewer::focus`. Afterpull retains only its public Intent mapping; it has no game-local focus adapter. Linux uses X11 (including XWayland) and identifies its process-specific startup title before restoring the title to Afterpull. Windows retains foreground events between rendered frames. Device observations never enter puzzle snapshots. Automated `--verify` replays run independently of desktop focus; they do not certify actual window-focus behavior.

Room lessons progress through remote pulls, body shielding, crate shielding, simultaneous motion, an essential jam, and a pit-lined finale. Numbered amber crates, outlined docks, mint player and striped pits have distinct shapes as well as colors.

Every step leaves a suction pulse. The last turn's summary stays visible, jams show crossed arrows, and dock arrivals flash mint and play a success cue. Whole-turn undo reverses every simultaneous pull.

The simulation is rendering-free and advances at the engine's fixed 60 Hz. Native input maps to public `Intent` commands; read-only drawing displays the same pull plan used by the authority. Saves include the current room, complete turn history, preview and feedback state. No custom save files or browser runtime are used.

Windows x64 EXE installer is the delivery target. Linux is the native development/verification target. Use `python3 scripts/check.py` for project integrity, `cargo test --no-default-features` for rules, and `cargo run --features client` for the native game. On Windows, `python scripts/ship.py ship --no-install` packages and smoke-tests; installer preparation/build/test remain the native Windows delivery gate. No installation is performed by development checks.

The shared Linux window regression runs from the engine checkout: `xvfb-run -a cargo test --locked --profile itest --no-default-features --features two-d --lib viewer::focus::platform::tests::native_focus_uses_real_window_without_a_gl_context -- --ignored --exact`. It creates real X11 windows, checks identification under reparenting and a competing title, and checks focus transitions including a complete switch between samples. It needs no OpenGL context. Ordinary headless tests leave this display-dependent test explicitly ignored.

Bounded authoritative playtesting is documented in the engine's [PLAYTESTING.md](../../docs/PLAYTESTING.md). Keep native package, software-generated input and physical-device observations separate.
