# Harvest Hollow play and engine feedback

The initial sidebar and tool cards occupied too much of the screen. User feedback led to a compact status strip, small tools and a Farm drawer that opens on demand, giving the farm substantially more space. Follow-up feedback removed the green framing and cleared canopy cover from the shipping crate.

The farm is designed around a quick, forgiving loop: clear a patch, plant, water, sleep, harvest and sell. The Smart tool makes the first session easy while dedicated tools let players work deliberately. Three nearly ripe starter crops give an early reward. Dry crops wait rather than die; rain and the sprinkler relieve repetitive watering. Wood, stone and seed finds make maintenance useful, and the pond deck adds a small cosmetic goal after the practical sprinkler.

Four original 16-bar scores give morning, afternoon, rain and evening their own character. Triangle and sine instruments, light plucked melodies and soft bass suit the small farm. Every rendered bank and full-mix loop passed the engine audio checks, with no detected clipping, clicks or DC issues. Composition sources and numerical reports are retained.

The fixed-step simulation and engine snapshots fit this genre well. Tests exercise a complete farm cycle, shipping, tree removal, resource costs, dry/rain/sprinkler growth, collision and exact save continuation. Click-to-walk and keyboard work actions use the same public intentions; presentation only reads state.

Final review also caught a pond-center interaction that was unreachable from shore. Multi-tile pond and cabin clicks now use the nearest reachable edge, with a public-input regression test.

Engine friction observed:

- Older local checkouts still document browser distribution, while the current October 13 release deliberately requires Windows installers. Fresh remote discovery was necessary before choosing delivery. The latest start packet correctly identified the retired target.
- The two-dimensional drawing and native input hook support custom farm UI without duplicating the engine clock, saves, audio or menus. The concise drawing guide is useful, but the native device_input hook needed API inspection for toolbar hotkeys.
- Named score banks support a varied soundtrack cleanly. Portable event sounds are still three indexed pickup/hit/success cues. Dedicated watering, seed and axe sounds would benefit from a named cue interface in the shared client.
- Committing rendered music made source uploads slow. The release workflow now renders the committed scores before shipping and checks the resulting banks. The default package must explicitly include audio bank directories. Bundle checks prove checked PCM assets and decoder submissions; they do not prove speaker playback or subjective musical quality.

Future improvements worth considering after this farm-only version: more crop varieties, a farm layout editor and named farming sound effects. Towns and NPC systems are outside this version's scope.

Verification limits: automated native captures and Windows package/installer checks run with software graphics. Physical audio, controllers and multiple consumer GPUs are not certified by those checks.
