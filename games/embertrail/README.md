# Embertrail

**A little light. A very big adventure.** An original 2D platforming campaign built with BlueEngine's portable Rust runtime. Mote, a scarf-wearing lantern spirit, must recover the dawn stolen by the Hollow Keepers.

32 authored courses span eight worlds: Dewlight Gardens, Echo Root Caverns, Copper Marsh, Frostbloom Orchard, Sunglass Ruins, Cloudseed Skyways, Duskweave Woods, and the Hollow Citadel. Each fourth course ends with a guardian encounter. All characters, artwork, course layouts, and the chip-music score were created for this game.

## Play

[Download the Windows x64 installer](https://github.com/kevstermcgee/Embertrail/releases/latest/download/embertrail-setup-windows-x64.exe).

Run `embertrail-setup-windows-x64.exe`. It installs the native game and original score for your Windows account, adds an Embertrail Start Menu shortcut, and offers a desktop shortcut. No browser or WASM runtime is used. Progress and settings are stored in `%LOCALAPPDATA%\BlueEngine` and survive upgrades and uninstall.

| Control | Action |
| --- | --- |
| Left/Right or A/D | Move; accelerate and brake |
| Space or Up/W | Jump; hold Up/W for a higher jump |
| Shift + direction | Run |
| Down/S | Duck/brake; enter a hollow root or leave its secret grotto |
| Down/S + Space | Cast bouncing ember seeds with the Ember Lantern |
| Click Atlas | Replay any unlocked course |
| Esc | Pause/resume |
| R | Retry current course; preserve campaign unlocks and best shards |
| K / L | Save / load a manual checkpoint |
| M / N | Toggle sound / music |
| F | Fullscreen |

The shared runtime supports standard controllers. Normal play saves automatically; K/L use a separate manual slot.

The compact HUD keeps course, sparks, lights, moon shards, time, score and Atlas
in small translucent panels along the top edge. Control hints appear before play
and in the pause menu, leaving the bottom of the playfield clear.

## Field guide

- **Sparks:** collect 100 for an extra life. Three optional **moon shards** reward exploration in each course.
- **Bloom Lantern:** survive an extra hit and break clay blocks from below.
- **Ember Lantern:** survive two hits and cast bouncing seeds. Cache contents upgrade a powered Mote.
- **Dawn Star:** ten seconds of invincibility against ordinary enemies.
- **Dew mites:** stomp them. **Seed beetles:** stomp into a shell, then touch the shell's side to kick it through enemies.
- **Drift wisps:** flying creatures that can be stomped. **Thorn buds:** timed hazards; jump around them.
- **Hollow Keepers:** leap on their heads, cast seeds, or send a rolling shell at them. Their gate opens after defeat.
- **Hollow roots:** press Down at the glowing rune to enter a secret spark grotto. Press Down at its far lantern to return.
- **Checkpoint lanterns:** relight when passed; falling returns you to the latest one. R retries the whole course.
- **Frostbloom:** icy acceleration and longer braking distance.

There is no character, artwork, music, or level layout from Nintendo's games in Embertrail.

## Develop and verify

This source lives in `games/embertrail` in the BlueEngine repository; the engine path is `../..`. Rust, Python 3, the MSVC build tools and Windows SDK are required for native Windows development. Inno Setup 6 builds the single-file installer. Art generation uses BlueEngine's pixel art exporter.

```powershell
cargo test --locked
.\scripts\blue.ps1 ship
& 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe' scripts/installer.iss
```

`src/lib.rs` owns fixed 60 Hz integer rules and engine snapshots. `src/levels.rs` owns the 32 explicit course sequences; `src/art.rs` is read-only presentation. `src/tests.rs` covers deterministic input replay, exact save continuation, power-ups, jumps, shell behavior, secrets, checkpoints, campaign progression, and invalid saves. `scripts/make_art.py` authors original sprites, icons, and the score; its rendered runtime audio lives in `assets/audio/trail/`.

The dedicated [Embertrail release repository](https://github.com/kevstermcgee/Embertrail) builds the committed public source on Windows, checks the game rules and score, verifies the native package, shortcut, window icon and isolated graphical launch, and tests installer installation, upgrade and save-preserving uninstall. Only the verified Windows installer is published.
