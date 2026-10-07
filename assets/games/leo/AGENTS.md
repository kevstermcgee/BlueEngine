# Working on Leo

Keep the user's PC silent and unobtrusive: no local game launches, windows, playback,
focus changes or automatic artifact opening. Background checks use low priority and
bounded concurrency. Render/audio evidence runs only on remote Linux CI's virtual
display with null ALSA. Package launchers in a private folder, never the desktop.

Locate the engine checkout through the `vesper3d` dependency path in Cargo.toml
(the published game uses a different relative path). Before engine reads, run
`python tools/be2.py context "<need>" --compact` from that engine root.
Follow its selected paths and public contracts.
Read `README.md`, `AUDIO.md` and engine `docs/PROCEDURAL_WORLDS.md` for this game.
`docs/CUSTOM_SIM_CHEATSHEET.md` covers movement, input, geometry, sound and menus.

`src/lib.rs` is pure fixed-step authority: seed, integer chunk/local coordinates,
Controller, tick/day duration and deterministic plants/collision. No device, window
or wall clock. Use engine `devkit::procedural`, not a growing global-float world or
visiting-order RNG. Keep blocking trunks aligned with visible geometry. Flat ground
is the Controller's implicit floor. Rebase previous/current poses together.

`src/main.rs` owns ClientInput/GameShell/Lifecycle, saved settings, checked AudioBanks,
capture and engine-owned saves. `src/scene.rs` reads authority, caches bounded chunk
art and renders a boy/sky/HUD. Invalidate caches when the loaded seed changes.
Day count, sky and audio derive from the saved simulation clock. Paused menus stop
the clock; music settings never alter gameplay. Music defaults on, independently
of nature sounds. No synthesis on the runtime loop.

Save through Snapshot/SaveSlots only; `SimState` contains everything affecting the
future. Keep SavePolicy::Exact and test continuation at distant chunk origins.
Derived plants/art and interpolation history are regenerated, not authoritative.

Audio data: `assets/audio-source` holds score JSON and credited PCM excerpts;
`assets/audio` holds checked rendered banks. Edit data, render into a new directory
with `scripts/render_audio.py ENGINE_TOOLS --output NEW_DIRECTORY`, check quality,
then adopt reviewed bundles. No Rust rebuild for audio edits. Never claim numeric
quality or null-sink submission proves subjective listening or hardware audibility.

Iterate with `cargo test --no-default-features` or the selected packet check.
Final: `python scripts/check.py --tools ENGINE_TOOLS --ship-folder PRIVATE_FOLDER`,
Linux/Windows CI and inspected remote captures. Package with
`python scripts/ship.py ship --folder PRIVATE_FOLDER --no-launch --no-smoke`.
Report launch/hardware controls as unverified. Capture flags/scripts: README.

Optional native portable presentation: read game.project.json, src/browser.rs and
engine docs/PORTABLE_GAMES.md. The historical source name is retained for compatibility;
`cargo run --no-default-features --features portable-client --bin leo-portable` runs it
natively. Keep the original Sim/SimState, saves, audio banks and credits. src/scene.rs
applies a validated viewport to every camera; default native main.rs stays independent.
Browser/WASM builds and publication are retired. Native captures, package checks and
physical input/audio evidence remain separate requirements.
