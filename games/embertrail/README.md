# Embertrail

**A little light. A very big adventure.** A controller-first 2D platforming campaign built with BlueEngine 0.2.0.

Vesper, the Bell Regent, stole the Dawnheart from Mote's village. Without its warmth, home will freeze. Carry the first spark across eight islands, free their bound guardians, ring eight dawn bells, and bring the Dawnheart home. Characters greet Mote automatically as the journey unfolds; there are no dialogue choices.

32 authored courses visit Dewlight Gardens, Echo Root Caverns, Copper Marsh, Frostbloom Orchard, Sunglass Ruins, Cloudseed Skyways, Duskweave Woods, and the Hollow Citadel. Each world teaches a new combination of terrain and creatures before its bell court. Vesper's night collars explain why the islands' protectors have turned hostile.

## Play

The game is designed for a standard PC controller. [Windows release downloads](https://github.com/kevstermcgee/Embertrail/releases) contain the published installer; development builds and candidate installers are verified separately before release.

| Controller | Keyboard | Action |
| --- | --- | --- |
| Left stick / D-pad | A/D or Left/Right | Move |
| A / bottom face button | Space | Jump with one consistent arc |
| X / left face button | X | Cast a seed with the Ember Lantern |
| RT / right bumper | Shift | Run |
| Down | S / Down | Duck, brake, enter an open root stairway or leave a grotto |
| Menu / Start | Esc | Pause |
| Y / View | Tab | Open or close the trail atlas |
| Stick / D-pad, A, B | Arrows, Enter, Esc | Navigate menus, confirm, go back |
| — | F | Fullscreen |

Up/W never jumps and does not change jump height. Jump buffering and coyote time make close landings and ledge takeoffs forgiving. Ice preserves a distinct feel with controllable braking.

Pause offers Resume, Trail atlas, Restart checkpoint, Restart course, Music, Sound effects, Download this song, Return to title, and Quit game. Menu / Esc also opens options on the title screen, including its current song download; B / Esc returns to the title while that song keeps playing. Confirming a menu action consumes the input. Normal play saves automatically through BlueEngine storage. Existing campaign progress migrates to version 2; an older saved course restarts at its entrance because course geometry and movement have changed. Player files remain outside the install directory.

## The islands

Collect sparks for extra lights and seek three optional moon shards in each course. Bloom Lantern protects an extra hit and breaks clay; Ember Lantern adds seed casting; Dawn Stars briefly protect against ordinary enemies. Stomp seed beetles into shells, then kick the shells along the trail. Ivory briars, crystal wisps, root lifts, and other obstacles belong to the region's ecology and machinery.

Open hollow-root stairways reveal the direction of travel through their dark opening, brass handrail, descending treads, and spark trail. Hold Down while standing in the entrance. No explanatory secret label is needed.

Dewmane, Chimeback, Brack, Rimehart, Solarch, Tempest, the Briar Matron, and Vesper have different attack patterns. Watch the wind-up, evade the attack, and strike the exposed mint collar during recovery. A fresh Bloom Lantern and checkpoint sit before each arena. Freed guardians stop attacking and open the dawn gate.

## Original soundtrack

Sixteen named, multi-section compositions provide approximately nineteen minutes of original music. Each composition lasts 61–86 seconds, with distinct phrases, harmony, rhythmic grids, instrumentation, and contrasting sections. Worlds, root grottos, guardian encounters, Vesper, the title, and homecoming have appropriate musical contexts.

The pause menu displays the composition actually playing. Choose **Download this song** while it plays to save its exact WAV in **Downloads / Embertrail Soundtrack**. Pausing keeps its playback position. Context transitions crossfade, retaining only the current and fading previous decoded compositions.

| Context | Composition |
| --- | --- |
| Title | A Small Light, A Long Road |
| Dewlight Gardens | First Sparks in Fernlight |
| Echo Root Caverns | When Crystals Learn to Sing |
| Copper Marsh | Reeds Beneath Copper Rain |
| Frostbloom Orchard | Snowglass Waltz |
| Sunglass Ruins | Caravans of the Sunseed |
| Cloudseed Skyways | Kites Above the Sleeping Sea |
| Duskweave Woods | Lanterns in the Briar Choir |
| Hollow Citadel | Ashes Keep Their Promises |
| Root grotto | Where the Roots Hide Stars |
| Woodland guardians | Thorns Around a Tender Heart |
| Stone and water guardians | Keepers of Stone and Water |
| Ice and storm guardians | A Crown of Ice and Thunder |
| Solarch | The Lion Behind the Sun |
| Vesper | Vesper's Last Toll |
| Homecoming | Every Window Holds the Dawn |

## Development and verification

This standalone checkout uses the sibling `BlueEngine-embertrail-update` checkout, based on upstream main `1d8d501`. The copy under the engine's `games/embertrail` uses the engine path `../..`. The update includes two small engine extensions: an independent secondary action and a renderer font override. The interface uses bundled, licensed DejaVu Sans for readable menus and HUD text.

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo run --no-default-features --example route_probe -- 3 600
```

`src/lib.rs` owns deterministic 60 Hz rules, `src/levels.rs` the authored courses, `src/story.rs` the short encounters, and `src/art.rs` the read-only presentation. `src/game_client.rs` uses engine input, timing, renderer, and checksummed snapshot storage. `src/soundtrack.rs` manages contextual checked audio and downloads. `scripts/make_world_art.py` regenerates pixel assets; `scripts/compose_soundtrack.py` and `scripts/render_score.c` author the score. Do not run the obsolete combined `make_art.py` generator over the new assets.

The tests cover rule behavior, all boss phases, independent inputs, exact continuation, versioned migration, secrets, checkpoints, controller Atlas navigation, and real-input traversal constraints. The development route search and its independent replay prove isolated-course reachability; [examples/README.md](examples/README.md) explains that scope. Native window input checks and visual captures are separate from deterministic tests. Physical controller feel and speaker playback require device review.

Windows delivery requires `python scripts/ship.py ship`, Inno Setup 6 compiling `scripts/installer.iss`, and `scripts/test-installer.ps1`. The update workflow checks the x64 GUI executable, complete package, shortcut, isolated graphical launch, installer upgrades, and save-preserving uninstall. Only a verified Windows installer is suitable for publication.
