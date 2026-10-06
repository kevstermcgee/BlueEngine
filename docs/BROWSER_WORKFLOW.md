# Browser games: the shortest supported path

Run commands from the engine checkout. JSON goes to stdout; failures are nonzero.
Use the portable runtime for 2D, hybrid and 3D games. The renderer reads the same fixed-step
simulation on web and native. Start here; no Leo source exploration is required.

```sh
# Prerequisites: Rust + Cargo, Python 3.11+, Node + ws, Chromium/Chrome.
# Linux native test builds also need pkg-config, libasound2-dev and libudev-dev.
npm ci --ignore-scripts --prefix tools
python3 tools/be2.py web capabilities
python3 tools/be2.py web prepare games/lantern-run  # wasm target + locked dependency fetch
python3 tools/be2.py web build games/lantern-run    # native rules, WASM, isolated desktop + touch gates
python3 tools/be2.py web inspect games/lantern-run  # exact source, hashes, capabilities, byte counts
python3 tools/be2.py web serve games/lantern-run    # localhost:8000; Ctrl-C stops
python3 tools/be2.py web verify games/lantern-run   # fresh browser evidence for existing package
python3 tools/be2.py web reproduce games/lantern-run # public source, empty target, compare package
python3 tools/be2.py web publish games/lantern-run --backend github-pages --repository OWNER/BlueEngineGames
```

For Leo replace `games/lantern-run` with `assets/games/leo`. Build outputs are in `GAME/dist/web`;
verification artifacts are separate in `GAME/.blue-check/web`. `inspect`, `verify`, `serve` and
`reproduce` also accept a package directory directly. The directory publisher (`--backend directory
--destination DIR`) creates a deployment-ready catalog and receipt; it does not claim an external URL.
The GitHub Pages adapter requires the existing BlueEngineGames `site/build.py` and Pages workflow.
A successful publication receipt includes source retrieval, package ID, deployment commit/run and
remote manifest/file hash verification. A push alone is not successful publication.

Create games with `be2-tools new-game NAME DIR ENGINE portable`, or select `two-d`, `hybrid` or
`three-d` for examples. Run `python3 tools/be2.py build tools` to obtain be2-tools if needed.
Set `game.project.json` presentation/targets/mobile controls deliberately. The same browser commands
handle all three presentations. See [PORTABLE_GAMES.md](PORTABLE_GAMES.md) for composition and
[GAME_QUICKSTART.md](GAME_QUICKSTART.md) for starter selection. Existing games' `scripts/blue`
forwards to these engine commands.

## Release requirements

Commit and push all engine/game build inputs to their public source repositories **before** publication.
Build records both exact revisions, repository URLs and source paths, source/lock hashes, pinned loader,
Rust/Cargo versions, release target/profile, deterministic build timestamp, content-based package ID,
runtime requirements, headless proof and individual desktop/mobile browser stages.
`SOURCE_DATE_EPOCH` overrides the source commit timestamp; reproduction reuses the recorded value.
Uncommitted build inputs are explicitly marked and cannot publish. Packages with old/incomplete
metadata must be rebuilt. Publishing anonymously fetches every exact source revision, checks its Cargo project and game-source
hash, and runs a clean-checkout reproduction before any deployment write. Missing/private/local-only source fails closed.

`web reproduce` fetches public source into a new checkout and compiles into an empty target directory;
it never copies untracked artifacts or previous binaries. It preserves committed dependency paths,
fetches dependencies with Cargo.lock, runs the documented build/gates and compares runtime hashes,
package ID, source hashes, provenance, compatibility, headless proof and browser stage results.
Dependency archives may be reused; compiled outputs may not. Use the recorded Rust toolchain for
byte-identical WASM. Browser timing/log/screenshot evidence is deliberately outside runtime packages.

## What the evidence means

`.blue-check/web/browser.json` and `mobile.json` distinguish WASM instantiation, playable state,
real input, save/write, reload/read, Web Audio initialization, offline reload, deterministic gameplay
scenario, focus loss/return and interrupted/successful update recovery. The manifest separately records
compilation and structural package validity. Synthetic standard controllers test movement, primary
input and B/Start pause/resume; the touch lane uses real Chromium touch events, portrait and landscape.
Displayed shared commands, adapter commands and test expectations use `templates/web/controls.json`.
Game-specific movement/action descriptions still belong to the game and its meaningful probe.

Each report includes startup/time-to-playable, save/load round-trip latency, WASM linear memory,
Chromium JS heap when available, sampled RAF frame times/FPS and game step/draw/streaming timings.
Leo marks origin changes for authoritative chunk update measurements and separately reports whole-draw
times/stalls on frames that stream chunks (including deferred art work); no wall clock enters its simulation.
Chromium uses `--mute-audio`; API submission is measured with silent output. CDP/report polling contributes to latencies. SwiftShader results characterize this test host, not a
phone or GPU. Audio initialization/submission proves API execution, not audible or pleasing playback.

Runtime audio banks include bank.json and its referenced layers/effects, with credits. Preview mixes,
score sources and bank reports stay in authoring/verification storage. Everything required for offline
play remains precached; `inspect` reports WASM/audio/other/manifest/total bytes, files and precache bytes.
A service worker checks every candidate asset hash before activation. A failed candidate is removed;
the previous complete cache and origin-local saves survive. An upgrade replaces only that game's
asset cache. Browser storage eviction remains outside application control.

## Compatibility and updates

Supported: portable offline 2D/hybrid/3D, WebGL 1, WebAssembly, Web Audio, standard gamepad,
keyboard/mouse, declared touch layouts, fullscreen where the gesture API exists, origin-local
save/settings and HTTPS/localhost offline installation. Unsupported: native UDP/QUIC multiplayer,
native renderer/worker APIs in WASM, cross-device/native save sync. `web capabilities` is the compact
machine-readable front door. Missing storage/fullscreen/audio APIs produce explicit diagnostics.

For an older portable game: select and publicly push the desired engine revision, use
`python3 tools/be2.py upgrade plan GAME --to REV`, retain the game ID, origin and Snapshot migration
contract, then prepare/build/verify/reproduce/publish. An older native game needs a portable rendering
adapter; changing targets alone does not port it. Release metadata must describe the new source,
not copy the old manifest. A new version must pass save preservation and failed-update recovery gates.

Human checks still required: physical controllers, actual phones/tablets, iOS/Safari, hardware audio,
fullscreen/install affordances and GPU/device performance. Automated Chromium emulation is separate
from physical-device evidence; reports never claim devices or listeners that were not tested.
