# 2D presentation → browser/native

For mobile controls, 3D/hybrid composition, autosave, offline install and the unified feed, read [PORTABLE_GAMES.md](PORTABLE_GAMES.md).

Read this page, your game AGENTS, `game.project.json` and `src/lib.rs`. No 3D renderer knowledge is needed.

```sh
python3 tools/be2.py web propose "Tiny Station"  # editable proposal, no creation/publishing
python3 tools/author.py describe
be2-tools new-game lantern-run ../lantern-run ../BlueEngine two-d
cd ../lantern-run
cargo test --no-default-features
scripts/blue web build                         # tests → WASM → clean package → real browser
scripts/blue publish --backend directory --destination /tmp/game-library
scripts/blue publish --backend github-pages --repository OWNER/BlueEngineGames
```

Adjust presentation, targets, input, networking, description, session length and complexity in
`game.project.json` **before implementing**. Unknown keys/unsupported combinations fail. Web supports portable 2D/3D/hybrid + offline. Legacy native 3D and existing native multiplayer remain unchanged.
Browser UDP/QUIC and portable multiplayer are unsupported. No fallback targets.
Native packaging checks declared native targets; legacy games without the file retain their existing path.

The canonical starter is a small complete collector. Examples with different mechanics:
`games/lantern-run`, `games/pocket-breaker`, `games/orchard-watch`.

## Game surface (`vesper3d::two_d`)

- `Simulation` + `GameLogic`: fixed 60 Hz integer rules, `Intent { x, y, pointer, action }`, outcome,
  public-input verification route and one meaningful real-device probe. All state stays rendering-free.
- `Snapshot`: same checksummed BE2SAVE frame, exact continuation, version and migration contract as 3D.
- `Rect::{overlaps,contains,slide}`: half-open integer bounds, swept axis movement that cannot tunnel;
  `Body::step(gravity,walls)` supplies small kinematic physics; `Trigger::enter` detects entrances.
- `draw::Scene`: `rect`, `circle`, `text`, `sprite`, `sprite_png`; stable integer painter layers.
  Texture transforms have position, scale and rotation. `sprite` accepts atlas source rectangles.
  Embedded PNGs decode once in the renderer. Invalid/duplicate asset IDs fail explicitly.
- `draw::Animation::new(frames,ticks_per_frame)?.frame(tick)`; `Particles` are bounded, presentation-only.
- `Viewport::fit` letterboxes a logical canvas; pointer coordinates outside it are `None`. Rendering
  can offset by a camera `Point`. The shared client uses an 800×450 logical canvas.
- `draw::Game::draw` reads your state; `client::run` owns timing/input, pause/restart, sound,
  settings, saves, error notices and verification evidence. Main only supplies identity/icon configuration.

Rules/tests live in lib.rs. Presentation is its `#[cfg(feature="client")]` implementation. Every sound
cue is 0 pickup, 1 damage, 2 success; shared generated WAVs load asynchronously. New genres may need
a custom sound bank later. This milestone intentionally has no editor or genre framework.

## Browser/platform contract

The browser build excludes native 3D/physics/network dependencies. Macroquad provides WebGL and its
requestAnimationFrame loop. The platform ABI handles focus, storage, audio activation and standard
gamepad polling. Gameplay has no browser cfgs. Randomness uses the shared seeded RNG, never browser
entropy. Browser loss of canvas/page focus pauses simulation; native players use Esc to pause. fixed-step catch-up is bounded. Verification accelerates the
same public inputs at up to 60 ticks/frame and compares the complete state hash with headless native.

Click/Enter starts and activates audio. Only game controls are consumed; Tab and Ctrl/Cmd/Alt shortcuts
remain browser controls. Standard controllers are normalized; physical controller testing is separate.
Audio proof means decoded buffers, running AudioContexts and playback submissions, not human audibility.

Saves/settings persist in localStorage per **origin + game ID**, across reload/browser restart until
the player clears site data. They do not follow the player to another browser/device/domain. Save
compatibility uses Snapshot VERSION/MIGRATIONS; storage namespace v1 is independent of save version.
Missing settings use defaults. Unavailable/corrupt storage produces a visible notice and session defaults;
failed writes preserve the prior value. K saves and L restores; M persists sound. Native uses the same
API/bytes in user data (see PORTABLE_GAMES.md). PlatformStorage supports 4 MiB browser values; quota remains browser-defined.

Install prerequisites once: Rust WASM target (`rustup target add wasm32-unknown-unknown`), cached Cargo
dependencies, Node with `ws`, and Chromium/Chrome (`BE2_CHROMIUM` overrides discovery). Local builds need
no hosting credentials or network after setup. `web build --skip-browser` is explicitly unverified and
cannot publish. Normal builds exercise isolated declared files on local HTTP through Chromium CDP.

## Distribution

`dist/web/` is static-hostable: index.html, game.wasm, loader.js, platform.js, thumbnail.png and manifest.json;
declared extra identity files retain their relative paths (use assets/). Every file hash and path is checked. Extra files,
symlinks, missing hashes/assets and case collisions fail. Loader sources come from the exact locked
Miniquad/quad-snd crates; missing imports fail rather than being stubbed. Output excludes source/target paths.
SOURCE_DATE_EPOCH (or commit time) stabilizes package timestamps. Rebuild proof should compare actual bytes.

Manifest schema 1 includes ID/title/description, revisions, presentation/targets/input/networking,
timestamp, thumbnail/play paths, optional native download, runtime/save/loader compatibility, expected
state hash and every file SHA-256. Library catalog.json indexes these fields automatically.

Build and publish are separate. The directory backend atomically installs a verified package and
updates a small catalog while preserving other games; it returns a local receipt, never invents a URL.
The optional GitHub Pages adapter uses an isolated checkout of the existing central game library,
preserves its native download site, deploys under `web/ID/`, waits for its workflow and confirms the
remote manifest and every file hash before returning a URL. It needs gh-authenticated push access and the library's
site/build.py + pages.yml contract. Another host only needs an adapter accepting this static package.

Headless tests prove rules/saves. Browser smoke proves initialization, matching replay outcome/hash,
audio activation/submission, one real mechanic interaction, persistence reload/failure behavior, canvas
and successful declared asset requests; external/undeclared runtime URLs fail. Inspect `.blue-check/web/browser.png`; software Chromium does not prove
Safari/Firefox/mobile behavior, physical controllers, speaker quality or sustained hardware performance.
