# Embertrail controller platformer update feedback

This update moves the existing Embertrail campaign from its previous BlueEngine checkout to upstream main `1d8d501`. The work exposed a gap between the portable simulation API and the controller experience a platformer needs. The most valuable engine follow-up is a portable menu and input layer with independent actions and current-track metadata. Collision, fixed-step simulation, snapshot migrations, checked audio bundles, and read-only Scene composition remain useful foundations.

## Independent gameplay actions

**Evidence:** `src/two_d/mod.rs::Intent` exposed one action alongside movement axes, sprint, pointer, and look. Embertrail previously encoded jump height in Up/W and casting in Down + action. Space and Up produced different jump arcs. This was a player-facing problem, not a tuning preference.

**Change in this branch:** add a serde-defaulted `secondary` action, bind X / controller West in the shared client, and preserve its edge through the existing fixed-step input accumulator. Old serialized intentions deserialize with the new action off. Embertrail uses one action for a fixed jump and secondary for casting.

**Follow-up:** provide semantic action bindings in the portable device contract, including held and pressed states, D-pad, sprint triggers, and device-appropriate prompts. Avoid forcing games to encode commands in pointer coordinates or overload movement. Check maintained consumers with exhaustive Intent initializers when extending this public Rust struct.

**Acceptance:** a one-frame secondary press survives a device frame with no simulation tick, fires once during a multi-tick frame, does not jump, and remains independent of crouching and the primary action. Confirming Resume or Atlas must not leak a jump into gameplay.

## Portable menus need the same controller support as custom clients

**Evidence:** `src/two_d/client.rs` owns pause state but its shared panel lacks the controller-navigated resume, level selection, current-track download, and quit flow required here. `viewer::game_client` provides a richer local menu for a different client path. Embertrail needed a custom client while retaining engine timing, devices, renderer, storage, and snapshot framing.

**Follow-up:** expose a reusable portable menu model and input result independently of its drawing. Games should supply labels and presentation while reusing focus movement, confirm/back consumption, pause/resume, save errors, quit, and audio actions. The title screen and pause screen should accept the same controller controls. Desktop close should pass through the same save path as Quit.

**Acceptance:** real keyboard and controller navigation can reach every row; focus remains visible; menu confirmation is consumed; loading clears pending input; quit saves before exit; and save failure is visible without silently destroying the last good slot. Scripted menu events and physical device observations should be recorded separately.


A title-options capture exposed a separate composition trap: Scene sorts explicit depth rather than submission order. The shared title text helper used depth 210 while the options panel used 202, so submitting the panel last still left inactive title labels on top. Embertrail now omits the inactive title UI while preserving its animated backdrop. A reusable menu layer should own a scoped depth range or explicitly replace the previous screen. Capture nested screens and inspect their labels; successful navigation and downloads alone do not verify visual isolation.

## Music identity, lifetime, and downloadable assets

**Evidence:** the original game used the same three short stems throughout eight worlds. `GameLogic::audio_level` changes layer levels but does not identify a currently playing composition or its title/download asset. A mixer counter cannot detect that the score sounds repetitive. A long score also magnifies the cost of eagerly loading every bank.

**Game solution:** authored multi-section compositions with explicit titles and world, underroot, guardian, antagonist, title, and victory contexts. The custom client loads the current composition and retains a previous composition only during the transition. Pause shows the selected composition and downloads its exact checked WAV. Pausing keeps the music playing.

**Follow-up:** add current composition metadata and a bounded track-lifecycle API alongside checked AudioBank bundles. Expose title, asset identity, loading/failure state, and export capability. The native download helper should support copying a validated large asset without blocking the frame loop. Keep uncompressed runtime size and decoded voice lifetime visible in authoring reports; the audio bank's 44.1 kHz PCM requirement makes a campaign soundtrack materially larger than a small stem demo.

**Acceptance:** each context selects the intended named composition; pause does not restart it; crossing a context boundary frees the previous voice after its fade; toggles do not leak playing voices; downloads match the selected checked file byte for byte; and both arrangement variation and wrapped loop seams receive separate review.

## Camera scrolling must preserve world identity

**Evidence:** Embertrail's old background selected silhouette height from the screen loop index while wrapping its x position modulo a cell width. The same visible tree changed height at a camera boundary. Ground texture also used screen-space x, making surface detail slide against collision geometry. The result could look stuck or abruptly replaced even though the camera continued moving.

**Game solution:** derive scenery identity from world-cell index, then project that cell through the camera and its parallax ratio. Anchor ground detail to tile world coordinates. Root interiors use a separate stable backdrop instead of inheriting distant exterior camera offsets.

**Follow-up:** document this invariant in the 2D guide and offer a small world-cell iterator or example. This was a game presentation error; it does not require changing authoritative camera or physics rules.

**Acceptance:** capture overlapping views immediately before and after positive and negative cell crossings. The same tree, cloud, and terrain mark keeps its shape and world position. Test root entry, root return, death/respawn, and course changes separately from ordinary camera travel.

## A control change requires a course and save compatibility audit

**Evidence:** simply deleting Up/W's lower ascent gravity would reduce the old jump's reach and strand routes authored for the larger arc. Boss courts also needed space for readable attacks and a checkpoint that did not replay a whole course after a failed fight.

**Game solution:** tune one approximately 148-pixel jump arc, verify required terrain clearance with public intentions, and provide pre-arena recovery and checkpoint placement. State version 2 migrates old campaign progress and rebuilds the loaded course at its entrance so changed geometry cannot strand an older checkpoint. Exact continuation remains the contract within the new version.

**Follow-up:** include representative traversal and old-save fixtures in the custom platformer starter. Treat collision tests, input-route tests, visual inspection, and physical-controller feel as different evidence. A deterministic route that merely survives while stuck against a wall must not be described as campaign completion.

## Pixel sprites need explicit sampling and facing

**Evidence:** `Scene::sprite_png` in `src/two_d/draw.rs` creates a texture without exposing a sampler choice. Embertrail retains a thread-local Texture2D cache to call `set_filter(Nearest)`. It also carries mirrored PNG variants because the presentation contract does not document a sprite flip around its bounds. The original sprite's scarf and face made its facing ambiguous to the player.

**Follow-up:** expose nearest/linear sampling and explicit flip_x/flip_y in sprite options, with a documented anchor. Keep the source character's visual facing explicit in the asset specification. Rendering a correct negative scale is not enough if the sprite anchor changes unexpectedly.

**Acceptance:** render the same side-profile character moving left and right at 1x and non-integer viewport scales. Its face leads, scarf trails, collision alignment stays fixed, and sampling matches the requested filter. This should use one source image per animation frame.

## Readable type must be configurable in Scene rendering

**Evidence:** the player explicitly reported that the fonts were hard to read. The Scene renderer previously always used Macroquad's default ProggyClean bitmap font, including tiny HUD and story text. A game could load another TTF but could not apply it to Scene text through the renderer's public API.

**Change in this branch:** `Renderer::set_font(Font)` applies an embedded font consistently to glyph preparation and drawing. Embertrail bundles licensed DejaVu Sans, enlarges HUD numbers and labels, wraps narrative captions into two lines, and checks contrast on opaque backing panels. Existing renderers retain their previous default.

**Follow-up:** give the starter a readable body font and minimum-size guidance, with explicit title/body roles. Verify normal desktop window sizes instead of relying on large screenshots. Measure text with the selected font before laying out menus.

## Native focus, muted capture, and decode cost

The shared portable client's focus query returns true, while Macroquad exposes no desktop focus getter. Embertrail needed a small user32/X11 adapter to pause when focus leaves the window. The first Linux link failed on `-lX11` even though the runtime soname was installed. The game now links the runtime soname explicitly and caches the display connection. A reusable engine focus event would avoid game-owned native handles and platform linking.

Macroquad also initializes its audio backend before the game can apply `--mute`. The host's default ALSA device panicked in a background thread with a sample-rate error during a silent capture. A declared null PCM sink allows software audio checks; this does not attest to physical playback. An engine capture option should avoid opening devices when sound is not requested and report backend startup failure clearly.

Finally, the async WAV load wrapper decodes on the render thread. Moving checksums and file I/O to a worker reduces work but does not make decode streaming. A future score API should expose bounded predecoded submission or streaming so a new minute-long track does not hitch platforming.

## Discovery and verification friction

A context prompt for this existing portable GameLogic platformer surfaced stock 3D movement and metre-based `ControllerProfile` tuning. Runtime-aware routing should first inspect the game's `Input = Intent` contract and direct authors to integer collision, button edges, support probes, and snapshot migration. In particular, a zero-subpixel vertical move produces no `Rect::slide` hit; grounded state needs an explicit support test. A test caught the resulting idle grounded flicker and platform-carry risk before delivery.

The first full Linux engine run passed 625 of 626 library tests and failed the existing process-discovery timing test under load. Its exact isolated rerun passed without a code change; the full suite was rerun. Keep this kind of environment timing evidence separate from game behavior regressions.

## Verified updates include publication

Delivery of this update initially stopped at a local candidate and required the player to ask separately for the BlueEngineGames website update. The player explicitly clarified that publication must be automatic behavior for verified game updates, unless they request local-only work. Treat this as part of delivery rather than a separate routine approval prompt.

The completion path is: retain the successful native Windows installer and its exact public source/build evidence; publish a versioned release in the game's established release repository; update the BlueEngineGames entry with its immutable asset URL, size, checksum, current description and screenshot; preserve earlier release entries; deploy Pages; and verify the live listing and download. Reuse the verified artifact rather than silently rebuilding different bytes for publication. Never mark a local candidate as a published update.

A future delivery helper should join these existing gates and emit one receipt with the source commit, successful build, release URL, asset digest, website commit and successful deployment. Its default for an existing published game should finish this path after checks pass, while an explicit local-only request skips external publication. Acceptance requires the live catalog version and download digest to match the verified game, previous downloads to remain intact, and failure to stop with concrete resumable state instead of asking the player to repeat the publication request.

## Verification scope

The game and engine checks, captured presentation, music analysis, native package tests, and their remaining limits are recorded with the update's verification artifacts. Software rendering and synthetic inputs establish implementation behavior; they do not attest to a person hearing speakers or using a physical controller.
