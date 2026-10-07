# Lantern Run
Collect the four lanterns, avoid the pink patrol and reach the teal exit.
WASD/arrows move. Click/Enter starts. K saves, L resumes, M toggles sound, R restarts.

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
