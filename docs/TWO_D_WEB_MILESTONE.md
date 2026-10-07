# 2D and browser milestone evidence

Historical measurement report. Browser gameplay is now retired by ADR 0049; commands
and browser support below describe the earlier measured revision, not current guidance.


Baseline inspected: `80d66091b3ab66d50fe3a88bdeda41d60c59ecfb` (2026-10-05).
The exact final tested revision is recorded in each published `manifest.json` and in
`.be2-work/2d-web-milestone/final-evidence.json`. Those receipts are generated after committing the
implementation so package revision stamps describe the tested source. This report describes the
implemented contracts and measured authoring exercise; it does not substitute for the machine gates.

## Delivered path

`be2-tools new-game NAME DIR ENGINE two-d` creates a simulation library, read-only presentation,
identity/icon set, explicit requirements, tests and thin distribution wrappers. See [TWO_D.md](TWO_D.md)
and [ADR 0039](adr/0039-two-d-browser-artifacts.md).

2D shares Simulation, FixedStepper, input accumulation, seeded RNG, playback, Snapshot and generated
audio with native 3D. Existing public devkit imports remain compatible. Its presentation branch offers
letterboxed logical coordinates/camera offsets, layered rectangles/circles/text, embedded or decoded
sprites/atlas quads, transforms, tick animation and bounded particles. Integer swept AABB movement,
kinematic bodies and triggers remain headless. The common client supplies keyboard/mouse, optional
standard controller abstraction, HUD, pause/restart, sound settings and quick save/resume.

WASM uses Macroquad/WebGL/requestAnimationFrame and a narrow JS ABI for focus, localStorage, audio
activation, timestamps and gamepads. It excludes native 3D, physics and networking dependencies.
Gameplay contains no browser conditionals. Native 2D still compiles some legacy native dependencies,
although it initializes no 3D world; reducing that compile graph is deferred.

## Games and behavioral evidence

| Game | Different mechanic | Public-route final hash | Other tests |
|---|---|---|---|
| Lantern Run | Top-down wall collision, four pickups, moving hazard, exit | `d6a3ec4a1e7d653e` | Wall blocking, defeat/frozen terminal state, restart, exact save continuation |
| Pocket Breaker | Ball/paddle arcade physics; clear six glass bands | `3636af9d530c0c41` | Missed-ball loss, keyboard priority over idle mouse, restart, exact save continuation |
| Orchard Watch | Mouse-driven seed economy and three defense lanes | `ba4e414244089571` | No-guard loss, duplicate guard cost, restart, exact save continuation |

All three use the same client, drawing, audio cues, save/settings implementation and browser pipeline.
Lantern Run also uses a sprite/animation; shared particles accompany cues. These are short bounded
mini-games, not content-heavy demonstrations. Their requirements declare web/Linux/Windows and offline;
Orchard Watch deliberately requires mouse/keyboard rather than advertising controller gameplay.

Native `scripts/blue ship` passed for all three: title/icon/wiring, display-free manifest/file/hash/path
checks, launcher creation and isolated executable smoke outside the checkout. Each produced two real
1280×720 captures. Linux Windows-resource/shell-launch checks correctly report skipped. Separate project
checks passed; captures were visually inspected. The unchanged Signal Garden and default/headless engine
suites remain in the full gate. Windows cross-typechecking passed in 73.7 seconds; this is not Windows
execution. Linux/Windows CI is preserved and extended with the 2D games and a browser job.

## Browser and distribution evidence

Real Chromium is driven through CDP, using Node `ws` and software WebGL. The generated package is copied
outside the source checkout, served over local HTTP and actually loaded. Each game initialized, won
through the public input route with the native hash above, and passed a separate real-device mechanic
probe: keyboard movement, mouse paddle positioning, or clicking to plant a guard. Start/pause/restart
and Tab navigation are exercised. Screenshots include winning and ordinary playing states.

Audio evidence includes three decoded buffers, trusted gesture activation, running AudioContexts and
actual buffer-source starts. Settings and exact saved state survive page reload. Injected storage
failure reports an error and preserves the prior save. Automated counters do not prove a human heard
speakers; no listening, physical controller or other-browser certification is claimed.

PlatformStorage uses identical checksummed BE2SAVE bytes, game KIND/VERSION/MIGRATIONS and all-or-nothing
restoration. Browser values are namespaced by origin/game/slot, survive reload/restart until site data
is cleared, and have a 4 MiB value limit subject to browser quota. Unavailable/corrupt settings report a
notice and session defaults; writes fail explicitly. Native stores the same bytes beside its executable.
No device/domain synchronization is implied.

`dist/web/` contains index.html, game.wasm, exact locked loader.js, platform.js, thumbnail.png and
manifest.json, plus declared extras at their relative paths. Integrity rejects unsafe/reserved paths,
symlinks, case collisions, missing/modified/undeclared files and invalid manifest requirements without a
desktop. Browser dependency tests proved a checkout-adjacent file cannot satisfy an undeclared request,
and even an external HTTP 200 dependency is rejected. Existing isolated native smoke is retained.

Manifest schema 1 records ID/title/description, engine/game revisions and game source digest,
presentation/targets/input/networking, reproducible timestamp policy, thumbnail/play paths, optional
native download, loader/runtime/save compatibility, expected outcome/hash and every file SHA-256.
`catalog.json` automatically indexes browser deployments. Rebuilding the external example twice produced
identical hashes for every package file including the manifest.

## Publishing and authoring

```sh
python3 tools/be2.py web propose "Tiny Station"
python3 tools/be2.py web build games/lantern-run
python3 tools/be2.py publish games/lantern-run --backend directory --destination /tmp/library
python3 tools/be2.py publish games/lantern-run --backend github-pages --repository OWNER/BlueEngineGames
```

Publishing always runs validation, headless tests, locked WASM build, package construction and a fresh
browser gate. Build is separate and offline after prerequisites are cached. `--skip-browser` is
explicitly unverified and cannot publish. The directory backend returns a deployment-ready static
library, no fabricated URL. The optional GitHub Pages adapter changes only browser artifacts and the
central site's integration point in an isolated clone, preserving existing native downloads and other
games. It waits for deployment and checks the remote manifest and every asset hash before a URL receipt.
Final deployment receipts are in `*-publication.json` under the milestone evidence directory.

A staged fresh-context exercise created external `/tmp/be2-fresh-meadow-dash`, built/played it through
its own wrapper, added a dash mechanic with a collision regression in one `src/lib.rs`, reverified it,
used `upgrade plan/verify`, and published to a local static library. Existing winning behavior and save
continuation remained intact. Upgrade now recognizes 2D declarations and uses the game's browser gate
rather than recommending irrelevant 3D migrations or treating native-only checks as browser proof.

The author-facing read set is four files: engine TWO_D.md, game AGENTS.md, game.project.json and lib.rs.
The recorded context packet was 2,869 bytes; the initial docs/game read set was roughly 13 KB. No engine
implementation file was needed for the staged rule modification. Main/Cargo/identity changes are needed
only when changing glue, dependencies or branding. This is a workflow exercise by the implementation
agent, not an independent small-model benchmark; token burden and comparative model success were not
measured. Shared-engine implementation itself required source inspection across runtime/presentation/
scaffolding/tooling and should not be confused with the game author's read set.

Measured costs (local Linux, downloads cached; concurrent work can affect timings): clean WASM compiler
18.08 s, unchanged compiler 0.16 s; fresh external full workflow 75.8 s; changed-game workflows 39.2–41.9 s;
unchanged upgrade verification 39.3 s; one-command local publication 37.5 s. The three simultaneous demo
builds took 91–103 s including cache-lock contention. These are not cold dependency-download or complete
clean-native-build measurements.

## Repairs and final gates

Building rather than only inspecting revealed and fixed shared traps: Rust/lld needed explicit JS
imports; the supplied Macroquad JS bundle mismatched locked Miniquad; new notice glyphs invalidated the
font atlas; no-deps metadata left fresh seeded locks unsettled; headless route success did not exercise
actual mouse/key mechanics; unsupported requirements needed checks in native dev/shipping; and upgrade
classification sent 2D games toward irrelevant 3D guidance. Ledger L-064–066 records the major friction.

Focused commands: `cargo test --profile itest --no-default-features --features two-d --lib two_d` (3 tests),
matching storage tests (1), 2D Clippy, project/upgrade/web Python tests, each game's headless tests,
real browser builds, and opt-in BrowserDependencyTests (2 real negative browser tests, 66.6 s).

Final required commands:

```sh
python3 tools/be2.py check --changed --base 80d66091b3ab66d50fe3a88bdeda41d60c59ecfb --plan
python3 tools/be2.py check --changed --base 80d66091b3ab66d50fe3a88bdeda41d60c59ecfb
python3 scripts/publish_games.py check
```

Full gate receipts are `final-full-check.log` and its generated check report, summarized in
final-evidence.json. Earlier failed iteration logs are retained: sandbox-only socket failures and a
full run invalidated by editing an embedded template while tests were running are not passing evidence.
Final source is frozen before final verification/publication.

Remaining limits: WebGL-capable desktop Chromium verified, Firefox/Safari/mobile unverified; touch-first
input, browser multiplayer/UDP/QUIC, 3D web targets and 2D multiplayer unsupported; native focus pause
currently uses Esc; native Windows execution relies on CI and was not performed locally; sound cues are
fixed stock sounds with no configurable music bank; storage is local, not cloud; the publishing adapter
requires an existing workflow-based central GitHub Pages library. Directory publication needs one
external step: serve its output through any static host.

Next three improvements: (1) browser/device CI including Firefox/Safari and touch/controller interaction,
(2) optional native 3D/network dependencies to reduce 2D clean-build cost without changing native defaults,
(3) a small configurable audio/music bank with the same playback/settings evidence contract.
