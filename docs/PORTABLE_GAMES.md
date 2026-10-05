# One game: mobile browser, installed browser app, native desktop

Read this, the game's AGENTS.md, game.project.json and src/lib.rs. Use `portable` for new projects
unless the user explicitly needs the legacy native world/netplay renderer. The CLI defaults to portable;
explicit `stock`/`custom-sim` and the legacy scaffolding library default remain compatible.

```sh
be2-tools new-game my-game ../my-game ../BlueEngine portable
# Optional visual examples: two-d / three-d / hybrid. Same runtime and services.
cd ../my-game
cargo test --no-default-features
scripts/blue web build               # headless + desktop AND mobile browser + offline install
scripts/blue ship                    # native executable + shortcut + isolated package smoke
scripts/blue publish --backend github-pages --repository OWNER/BlueEngineGames
```

The game chooses tools by value to gameplay, not by distribution. `presentation` is `2d`, `3d` or
`hybrid`; it describes the game for the catalog, not a restriction on drawing APIs. `runtime: portable`
supports all three on web/Linux/Windows/macOS. Targets remain explicit: never silently substitute one.
Start flexible (hybrid) when the request leaves dimensionality open; choose a simpler mode when sufficient.
Legacy native games keep their existing paths. Porting one to web means adapting its presentation to the
portable client; changing metadata does not port its renderer or native UDP/QUIC transport.

## Shared authoring surface

`vesper3d::portable` is the recommended import; existing `two_d` imports remain compatible. Simulation,
GameLogic, integer intentions, fixed step, seeded RNG, snapshots, collision, triggers, audio, save/settings,
pause/restart and verification are shared across these distribution targets. Draw reads simulation only.
The minimal rules/input/drawing reference is [TWO_D.md](TWO_D.md).

`draw::Scene` orders 2D primitives/sprites/text and 3D views in one layer list. `World::new(camera_position,
camera_target)` offers cube, sphere and arbitrary textured Mesh/Vertex geometry. Configure its public
Camera3D for perspective/orthographic projection or camera motion. `scene.world(layer, logical_rect, world)`
places the 3D view in logical pixels; later 2D layers can be a minimap, health bar, UI or foreground sprite.
Use full-canvas views for 3D games, smaller views for 3D elements inside a 2D game. There is no requirement
that simulation use the same dimensionality as presentation. It can use 2D collision for a 3D-looking game.
The hybrid starter demonstrates a 3D garden and 2D minimap; `games/lantern-grove` is its complete example.
Native-only world rendering/QUIC/Rapier APIs are not magically browser APIs; unsupported requirements fail.

## Mobile controls (below the canvas)

In game.project.json choose:

```json
{
"runtime": "portable",
"presentation": "hybrid",
"targets": ["web", "linux", "windows"],
"mobile_controls": {"layout": "dpad", "action_label": "Action"}
}
```

- `dpad`: four direction buttons plus an optional action. Good for movement/platform/arcade rules.
- `paddle`: horizontal position slider. Good for mouse/paddle/aiming games.
- `tap`: touch the game to select/place. Good for management/strategy; no fake directional pad.

The mobile panel uses a Game Boy-style directional cross and round A/B buttons. A performs the configured
action (or Restart when action_label is null); B pauses/resumes. Small START and SELECT controls sit below.
START plays/pauses; SELECT opens Sound/Save/Load and Restart when A has a gameplay action.
Supply a short meaningful action_label or null when the game has no action. A coarse primary pointer selects the
mobile panel; desktop uses existing keyboard/mouse/controllers. Touches feed exactly the same Intent as
those devices. Pointer capture permits holding direction and action together. Cancellation, focus loss
and hidden pages clear held controls. Focus never scrolls the panel out from under a finger. Gestures
activate browser audio; control listeners do not disable normal scrolling outside the game/panel.

The isolated browser gate runs a separate real-touch CDP profile in portrait and landscape. It exercises
Start/Pause/restart, the game's meaningful probe, audio, saves/settings and offline reload. A keyboard
probe is insufficient evidence for mobile. Emulation validates the supported path; it is not actual
Android/iPhone hardware or Safari certification.

## Progress and installation

Normal play restores automatic progress at startup and checkpoints about once a second, also when
paused/focus is lost or a game finishes. A sudden close may lose the last second. Save/Continue use a
separate manual slot, so autosave never overwrites a deliberate checkpoint. Verification/capture always
starts fresh and cannot overwrite the player's automatic progress. Errors are visible; prior saves survive
failed writes. Snapshot KIND/VERSION/MIGRATIONS govern compatibility for both slots and all targets.

Browser progress/settings/favorites survive reload and browser restart on **this device and origin**,
until site data is cleared or the browser evicts it. Installing the browser game preserves the same origin
storage. No account, cross-device sync or browser/native synchronization is implied. Native saves live in
user data (XDG data home, macOS Application Support, Windows LocalAppData), not the installation folder.
Old executable-adjacent saves/settings migrate on read. BLUEENGINE_DATA_DIR overrides native storage for
isolated tests or portable installations. These changes let progress survive native package replacement.

Every new web artifact includes app.webmanifest and a versioned service worker caching only declared
package files. Browser Install app / Add to Home Screen is the local install option for 2D/3D/hybrid.
Its first complete online load makes offline play available. HTTPS (localhost for development) is required;
a file:// package is not an installed app. Browser support determines the install menu/prompt. Native ship
remains available for declared desktop targets. Local building needs no hosting account.

Asset cache updates replace asset caches only, never saves/settings. Each game's scope has its own cache.
The browser gate checks installability and actually disables networking before reloading and restoring
progress; successful cargo compilation is not installation evidence. No guarantee of permanent browser
storage is possible; storage quota/privacy/eviction are platform constraints with explicit diagnostics.

## One catalog

Publishing merges verified browser metadata into the main game feed by stable game ID. A game with both
browser and native versions has one card, both actions. Filters select presentation, browser/download and
networking; data/native source classification is independent. Existing native download URLs remain intact.
Star buttons use origin-local storage and always sort favorites ahead of the chosen secondary order.
Failed storage writes leave the old persisted favorites intact and explain the session-only change.
The legacy web/index page redirects to the central feed. A standalone directory publisher uses the same
feed/favorites UI. The publishing backend owns site integration; gameplay knows nothing about hosting.
