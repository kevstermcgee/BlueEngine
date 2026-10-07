# Orchard Watch

Spend seeds on guards and defend three orchard lanes through nine waves.
Click/Enter to start. Mouse or arrows steer the paddle in Pocket Breaker; click guard pads in Orchard Watch. K saves, L resumes, M toggles sound, R restarts.

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
payload in the per-game EXE installer. Browser gameplay is retired; native Linux/Windows checks and Windows package smoke remain required.

For package-only delivery use `python scripts/ship.py ship --no-install`; it keeps the
isolated smoke and never accesses the desktop. Plain `ship` additionally installs and
verifies this game's shortcut. Other applications' icon similarity cannot fail shipping.
