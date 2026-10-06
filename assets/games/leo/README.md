# Leo

A little boy, an endless meadow, and another morning. Wander through fields and
groves of oak, silver birch, Scots pine and rowan, with daisies, buttercups, red
clover and cornflowers. The stylized plants use real species as visual references.
Each twelve-minute day moves continuously through sunrise, daylight, sunset and
stars. The main and pause menus show your current day; the walking view has no
day counter or status panel. There is no score, combat or deadline.

Browser: the same simulation and original character/meadow/sky art. WASD/arrows wander, Shift runs,
drag inside the game to look, Space hops, F toggles fullscreen, Esc pauses, M/N toggle nature/music,
and K/L save/load. Mobile: D-pad wanders, A hops, B pauses; drag the game to look. SELECT contains
sound/music/save/load below the game. Progress resumes on this device; Install app/Add to Home Screen
supports offline play after the first complete load. Browser and native saves remain independent.

`scripts/blue web build` verifies the isolated static package. `scripts/blue publish --backend
github-pages --repository OWNER/BlueEngineGames` deploys it. game.project.json explicitly selects the
browser binary/features, preserving the native client. Browser presentation uses contact shadows
and shorter decorative view distance to bound mobile memory/work; all 49 authoritative chunks remain.
Physical-phone performance and hardware audibility require separate testing.

Leo has a soft smile, rounded proportions and swept chestnut hair. He travels
without a backpack. Trees and the boy cast directional sun/moon shadows using
the engine's bounded, texel-snapped shadow map, with contact-shadow fallback on
devices unable to create the map. The starfield and warm horizon fade continuously.

WASD or the left stick moves; Shift runs; Space hops; mouse or right stick looks.
Esc opens the menu, including Controls and Settings. Music starts enabled; its
toggle is independent of nature sounds. M also toggles music. F5/F9 save/load a
quick slot. An engine-owned rotating autosave preserves days and position every
minute, at day changes and on a normal exit; the next normal launch resumes it.
A paused menu stops the authoritative clock.

The world streams a bounded 7×7 grid of deterministic 32-metre chunks, with
integer origin rebasing to preserve local movement precision. Trees block movement
and protect the third-person camera. Chunks return unchanged when revisited.
See [engine contracts](https://github.com/kevstermcgee/BlueEngine/blob/main/docs/PROCEDURAL_WORLDS.md)
and [audio credits](AUDIO.md). Cargo.toml's `vesper3d` path locates the matching local
engine checkout; build its native tools with `cargo build --profile fast --bin be2-tools`.

From this directory, build without opening a window:

```sh
cargo test --no-default-features
python scripts/check.py --skip-ship
python scripts/ship.py ship --folder PRIVATE_LAUNCHERS --no-launch --no-smoke
python scripts/check.py --ship-folder PRIVATE_LAUNCHERS
```

Committed source WAVs, scores and rendered checked bundles ship inside `assets`.
Audio JSON edits need rerendering, not a Rust rebuild: `python scripts/render_audio.py
ENGINE_TOOLS --output NEW_ASSET_ROOT/audio`, then capture with `--assets NEW_ASSET_ROOT`.
Existing bundles are never overwritten; validate the candidate before adopting it.

Capture flags: `--capture NEW_DIR --frames 0,30,90 --exit-after 100 --script
"fwd:0-90,sprint:0-90,jump@30" --seed 7 --perf`. `--day-seconds 4..3600` accelerates
the saved clock for evidence; normal play uses 720 seconds. `--portrait` gives a
front-facing capture view. `menu@FRAME` pauses/resumes, `music@FRAME` uses the same
persisted toggle as Settings, and `save@FRAME`/`load@FRAME` use F5/F9's path.
`--assets DIRECTORY`, `--settings FILE` and `--save-dir DIRECTORY` isolate evidence.
Capture/script runs are silent by default. Actual audio submission is verified
only with `--verify-audio` on explicitly opted-in remote Linux CI, a virtual
display and null ALSA sink. Never launch a local preview while the PC is in use.

The implementation is a bounded example of the engine primitives, with cached
code-generated meshes, flat ground and juvenile stylized plants. It is not a
photorealistic ecosystem simulation. Automated capture proves rendered frames
and submitted audio, not subjective musical quality or hardware audibility.
