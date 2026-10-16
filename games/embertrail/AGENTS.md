# Embertrail

Start/resume from the configured engine checkout with `python3 tools/be2.py start "<task>" --project GAME_DIR --compact`. Read this file, `src/lib.rs`, `src/main.rs`, and `game.project.json`.

- `GameLogic` + `Simulation` own fixed 60 Hz integer rules. Draw reads state. Gameplay input is `Intent`; action is the one jump, secondary casts. Never make Up/W a second jump.
- This game uses a custom client (`src/game_client.rs`) built on engine devices, timing, Scene rendering and PlatformStorage. It owns controller menu navigation, pause/retry/quit, settings, and contextual audio. `src/window_focus.rs` detects desktop focus.
- Saves use the engine's frame/hash/migration contract. Keep KIND stable, bump VERSION for released state changes and test real framed migration. Version 2 preserves campaign discoveries and rebuilds old saved course geometry at its entrance.
- `src/story.rs` provides automatic brief NPC greetings, guardian names and bell milestones. There are no dialogue choices. Mote must recover the village's Dawnheart from Vesper before home freezes.
- Keep the clear side profile of Mote and the collar recovery marker readable. Derive parallax scenery from world-cell identity before camera projection. Anchor ground detail to world coordinates.
- Use the bundled licensed sans font, readable HUD/menu sizes, and high contrast. Pixel artwork does not imply a pixel body font. Inspect at 960x540 and 1280x720.
- Score identity comes from `assets/audio/score/tracks.json`. Pause displays/downloads the actual current composition, even while another loads. Validate bundle checksums; do not equate counters with audible hardware playback.
- Regenerate assets with `scripts/make_world_art.py` and `scripts/compose_soundtrack.py`. The obsolete `make_art.py` combines old sprites and short stems and must not overwrite this update.
- `verification_input` and development route probes use public intentions. Do not mutate the game to pass verification. Capture-only showcase fixtures are presentation evidence, explicitly excluded from gameplay verification.

Run `cargo test --locked` and appropriate Clippy checks. From the engine use `be2.py check --game GAME_DIR --loop inner`, then integration and shipping. See `examples/README.md` for bounded traversal search and independent replay.

Windows distribution uses `python scripts/ship.py ship`, Inno Setup 6, and `scripts/test-installer.ps1`. Publish only a Windows x64 installer after its native workflow passes. Keep player saves/settings outside the install directory. Compilation, software captures, and audio submissions do not certify physical controller or speaker review.
