# Reverie

**A museum that dreams.** A contemplative first-person game made with BlueEngine 0.2.0, pinned to upstream revision `9d6265b0d4be4edcd631f6df1ef94811f2ff275c` (11 October 2026).

Walk through an endless branching museum of Wonder, Longing, Grief, Joy, Unease, Anger, Tenderness and Acceptance. Every gallery holds six living artworks: three animated paintings, two moving sculptures and a light installation. Each work has a title, a fictional artist and a story. Five frame designs, seeded compositions, moving forms, scenery and palettes make every gallery a new arrangement.

Paintings are traversable worlds. Walk inside, approach their smaller frames and enter again. The first key lies **two paintings deep**. Later rooms hide their key one to three paintings deep, or inside an awakened sculpture or installation. After finding a key, either gold door opens. Return through the entrance to explore the other branch; collected keys remain in your archive. There is no final room, score penalty or time limit.

Eight original musical movements use felt-like piano, sustained strings and sparse high notes. Room changes crossfade between movements, and painting depth changes the balance. Faint footsteps can be disabled independently in Settings. Music, sound, volume, mouse sensitivity, gentle motion and fullscreen controls are available. The soundtrack is synthesized from the committed, authored note scores; no downloaded music or external account is required.

| Controls | Action |
|---|---|
| WASD / arrows / left stick | Walk |
| Mouse / right stick | Look |
| E / controller A | Enter art, awaken a sculpture, collect a key or open a door |
| Q / controller X | Read title, artist, medium and origin story |
| B / controller B | Leave one painting layer |
| H | A gentle clue |
| M | Explored map |
| Esc / controller Start | Pause |
| Shift | Walk faster |
| F5 / F9 | Save / load this moment |
| F11 | Fullscreen |

The pause menu includes an explored gallery map and painting-world markers. Drag or use arrow keys to pan, and scroll to zoom. Saves preserve the seed, room path, recursive return positions, discovered rooms, explored painting worlds and collected keys through BlueEngine's atomic, checksummed snapshot API.

## Development

From the engine checkout, render the score once, then build:

```sh
cargo build --profile itest --no-default-features --bin be2-tools
python3 games/reverie/scripts/compose.py target/itest/be2-tools
cargo run --manifest-path games/reverie/Cargo.toml
```

On Windows, use `python` and `target/itest/be2-tools.exe`. The standard game scripts work from `games/reverie`: `python scripts/check.py --skip-ship`, `python scripts/ship.py ship --no-install`. Rendering the audio is required before running or packaging. The composer checks existing bundles rather than replacing them; after changing a score, render to a new directory and replace the old generated bundle intentionally.

For visual captures:

```sh
python3 tools/xcapture.py target/debug/reverie --frames 30,330 --out /tmp/reverie-frames -- --seed 3 --script 'fwd:0-300,interact@305'
```

`--seed N` generates another museum; the designed first visit defaults to seed 3. Procedural art recombines a curated narrative vocabulary and geometric motifs, rather than depending on an online generation service. Geometry is bounded to the current scene; the archive grows with places actually visited.

Windows delivery is a per-user x64 installer through the BlueEngineGames download catalog. Packaging a clean checkout automatically builds the shared authoring tool and renders any missing musical movements. The `Reverie native build` workflow retains standalone installer and package verification evidence. Linux is a development and verification target.
