# Lantern Grove

Collect four golden lanterns and reach the teal exit, avoiding the pink patrol. The 3D garden and a 2D minimap share one deterministic simulation. Windows: arrows/WASD/controller. Progress saves automatically; K/L use a separate manual checkpoint. Install the native Windows EXE to play offline.

Read ../../docs/PORTABLE_GAMES.md and AGENTS.md for the supported workflow.

## Windows EXE distribution

This game uses the existing native shared client, with the same fixed-step rules,
keyboard/mouse/controller input, audio, pause/restart and save/load. Public play
is through a Windows x64 EXE installer, not a browser.

From this folder on Windows:

```powershell
cargo test --locked --no-default-features
python scripts/ship.py ship
python scripts/check.py
```

`ship` builds `dist/`, embeds the game icon, creates a desktop shortcut and verifies
the isolated native payload. BlueEngineGames' Windows release workflow wraps that
payload in the per-game EXE installer. Optional web-target metadata is retained for
engine regression tests; it does not authorize browser publication.
