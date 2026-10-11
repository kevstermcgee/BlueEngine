# Wormhole

An endless journey through light. A cinematic, contemplative simulation inspired by the feeling of the Stargate passage in *2001: A Space Odyssey*. No objectives, score, failure or destination.

Fall through sculpted, twisting tunnels: petals, rounded angular chambers, luminous rivers, curtains, rushing rings and fine streams. Shape, palette, distortion and motion continuously blend as new regions arrive. Region identities come from a 64-bit seeded procedural sequence, not a recorded animation or finite scene playlist. Floating point rendering uses a moving local origin, so long journeys retain smooth motion. R creates a new seed; automatic progress and K/L preserve the exact journey.

Drag to look around, or use WASD/arrows/right stick. Space/A cycles **Float**, **Fall** and **Surge**. F toggles fullscreen, Esc pauses, N toggles music, M toggles sound, K saves and L loads. The journey starts immediately, with no titles, HUD, menus or notification overlays.

The original score **An Endless Journey Through Light** layers warm sustained harmony and sparse, wistful bell-like melodies. Four independent 43-, 47-, 53- and 59-second phrases in a shared D-Lydian/B-minor harmonic world combine with continuously changing seeded orchestration. The recorded stems recur, while their combined arrangement evolves with the journey. No film soundtrack or third-party recordings are used. Music pauses with the experience.

Windows x64 installer distribution is provided through BlueEngine Games. Native Linux development also works. This is a native BlueEngine game; the Games website offers downloads.

From this directory:

```sh
cargo test --no-default-features
cargo run
python3 scripts/check.py --skip-ship
python3 scripts/ship.py ship --no-install
```

`tools/compose.py` writes the reproducible score projects in `assets/score/`; render each to a new `assets/audio/<name>` directory with BlueEngine `audio render`, then `audio check`. Runtime files are checked PCM bundles. `src/wormhole.frag` is the authored ray-marched presentation submitted through BlueEngine's `Scene::render_view`. Simulation is fixed-step and rendering-free; persistence uses engine Snapshot.

For visual QA, `--capture OUT --preview-seconds N` advances the simulation through actual neutral fixed steps before capturing (at most one hour). It is disabled in `--verify` runs and never writes player progress. Rendering, input and package launch checks are distinct from deterministic rule tests.
