# Embertrail

**A little light. A very big adventure.** An original 2D platforming campaign built with BlueEngine's portable Rust runtime. Mote, a scarf-wearing lantern spirit, must recover the dawn stolen by the Hollow Keepers.

32 authored courses span eight worlds: Dewlight Gardens, Echo Root Caverns, Copper Marsh, Frostbloom Orchard, Sunglass Ruins, Cloudseed Skyways, Duskweave Woods, and the Hollow Citadel. Each fourth course ends with a guardian encounter. All characters, artwork, course layouts, and the chip-music score were created for this game.

## Play

[Play Embertrail in your browser](https://kevstermcgee.github.io/Embertrail/).

Run the Linux package with `./dist/embertrail`, or use the **Embertrail** desktop shortcut. For browser play, run `scripts/blue web serve` and open the localhost URL it prints. The browser package is `dist/web/`; serve it over HTTP, rather than opening its HTML file directly. The first complete load enables offline play and browser installation.

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

Touch uses the D-pad and Jump button. Hold Up to extend jumps. The shared runtime also supports standard controllers. Touch/controller movement uses the same simulation. Normal play saves automatically on this device; K/L use a separate manual slot.

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

This source lives in `games/embertrail` in the BlueEngine repository; the engine path is `../..`. Rust and Python 3 are required; browser checks additionally use Node, ws, and Chromium. Art generation uses BlueEngine's standard-library pixel art exporter.

```
python3 ../../tools/be2.py check --game . --loop inner
scripts/blue web preview
CARGO_TARGET_DIR=../../target python3 ../../tools/be2.py check --game . --loop shipping
```

`src/lib.rs` owns fixed 60 Hz integer rules and engine snapshots. `src/levels.rs` owns the 32 explicit course sequences; `src/art.rs` is read-only presentation. `src/tests.rs` covers deterministic input replay, exact save continuation, power-ups, jumps, shell behavior, secrets, checkpoints, campaign progression, and invalid saves. `scripts/make_art.py` authors original sprites, icons, and the score; its rendered runtime audio lives in `assets/audio/trail/`.

The dedicated [Embertrail release repository](https://github.com/kevstermcgee/Embertrail) hosts browser releases. Create a verified artifact with `scripts/blue publish --backend directory --destination /path/to/release`. BlueEngine retrieves the public source and rebuilds in a clean checkout. Copy the verified `/path/to/release/embertrail` package to the release repository's `public/` directory and push it; its Pages workflow checks all runtime hashes before deployment. Confirm the live manifest and file hashes after deployment.
