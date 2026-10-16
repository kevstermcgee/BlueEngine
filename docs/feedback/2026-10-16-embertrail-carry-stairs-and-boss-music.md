# Embertrail 0.2.2: carry collisions, stair travel and battle score

The player reported being pushed into stationary platforms, abrupt stair travel and insufficiently intense boss music. This follow-up fixes game-owned rules and presentation while retaining the existing engine, fixed jump, save framing, controller menus and bounded prepared audio. The useful engine follow-ups are a clearer 2D carry contract, reusable transition timing and visible-encounter audio bindings.

## Sweeping after an unchecked carry is too late

An authored course 2 fixture reproduces the fault at world tick 175. A grounded rider starts at `(1168,286,22,30)` on a horizontal platform with home `(1184,316)`. Its next translation is `(-1,0)`. Directly adding that translation places Mote at x1167, overlapping the stationary tile `(1104,300,64,14)` by one pixel. The subsequent ordinary `Rect::slide` begins embedded; it checks candidate positions, not a safe prior contact, so it cannot be relied on to repair the error.

This was the game's unchecked carry, not evidence that the engine sweep tunnels from a clear starting position. The replacement sweeps rider motion against static solids before assigning coordinates. A horizontal platform may slide beneath a rider blocked by a wall. An upward lift holds its path phase when a ceiling prevents its rider moving up, avoiding both crushing and a later catch-up jump. Falling/top-crossing behavior and ordinary jumping remain covered separately.

The authored contact sweep exercises 795 blocked contacts with no static overlap. In the original fixture the new result stays at x1168, and eight public Right/run inputs move Mote to x1210 without overlap. Existing inputs still clear all 32 isolated main courses, defeat all eight guardians and reach the final ending without a lost life. These checks do not certify a continuous human campaign.

Proposed engine improvement: a portable integer carry helper that takes old/new mover bounds, the rider and static solids, returning applied displacement, blocked axes and support status. Document the policy for a blocked upward lift separately from crushing, horizontal slipping and one-way platform crossing. Tests should include a rider against a wall, under a ceiling, falling onto an ascending top, stepping away and exact save/resume. Do not copy a direct coordinate offset as a platform recipe.

Discovery for “2D moving platform carry” initially surfaced stock 3D `Controller::ride` and ADR 0026. That API belongs to a different runtime. The selected 2D entry should route to `two_d::Rect::slide` and an explicit carry example; apparent keyword similarity is not enough to reuse the 3D recipe.

## A fixed bug can persist inside an old save

A version 3 autosave could already contain the one-pixel penetration. Preventing new carry errors alone would leave that saved position embedded. The real framed V3 migration fixture now repairs x1167 to the nearest clear x1168 and retains the course, campaign, checkpoint and pickups. A normal public movement input then escapes. Valid old states retain their existing fields exactly; recovery is confined to positions actually intersecting solid terrain.

Proposed save guidance: when a motion defect can write an invalid pose, test both prevention and loading a genuine affected prior-version frame. A migration test made by relabeling a new state with old version numbers misses absent fields and old geometry. Keep legal-pose recovery explicit in game validation rather than silently resetting campaign progress.

## One stair clock owns travel, camera and input

Stairs previously teleported Mote and the camera in one tick. The new authoritative `StairTransition` stores elapsed time, direction and source gate for 40 fixed steps. World time, moving platforms, hazards and normal movement wait during travel. A fresh Down edge starts it; a held button cannot immediately reverse the trip. The scene reads that same clock for stepped descent, threshold clipping, emergence and the warm occlusion covering the midpoint camera swap.

Version 4 adds the transition and input/boss latches through the engine migration contract. Framed saves at all 40 points in each direction resume identically. No wall-clock tween writes the player's authoritative position. Native review also checks that the two worlds and player do not show through the midpoint fade, that emergence reaches ground height before leaving the stair threshold, and that pause/Atlas cannot advance a hidden trip or leak input.

The underground route probe was updated as well: its public Down commands wait with neutral intentions through both complete animations and assert the actual far exit's source gate. It distinguishes world ticks from travel steps. A receipt saying only “outside the room” would miss an unfinished animation or an emergency entrance return.

Proposed engine support: a small serializable transition clock with midpoint/completion events, input gating and a presentation progress value, without imposing one visual style. Acceptance needs midpoint save/load, focus pause, held-button re-entry prevention and captures of both directions. Do not infer animation state from whichever world happens to be visible.

## Intensity comes from arrangement and encounter visibility

The score previously selected a boss theme from AI activation. That is different from the boss entering the viewport. A final audit also found that the painted boss extends 12 pixels beyond its collision rectangle; using collision bounds would still start the music late. The visibility predicate and sprite layout now share the rendered bounds. First visibility latches the battle theme until defeat, death or course change, including temporary offscreen attacks and pause/Atlas. It uses already prepared handles and a gentle 1.6-second transition.

A master-gain increase alone would not add musical tension. The five existing guardian/Vesper compositions instead gain 110–159 quiet heartbeat, frame-drum and chord-tone bass events, with four section strengths, rests and phrase-end variation. Names, melody, harmony, tempo and duration stay intact. The other eleven recordings and the 16-track catalogue remain byte-identical, and no WAV bytes or cache slots were added. Measured RMS changes are only 0.01–0.04 dB; the intended difference is rhythmic texture. Numeric headroom and waveform checks do not certify speaker listening.

An always-on second music layer would complicate the promise that pause downloads the actual named WAV being heard. Updating the five checked recordings keeps that promise, rather than exporting only one component of a live mix. Macroquad's exposed sound API here also supports gain rather than an authored tempo/filter control; do not invent such a method or decode new minute-long audio in active play.

Proposed engine contract: encounter audio should distinguish visibility, latched engagement and completion from AI phases, camera distance and pause screens. A named prepared composition must remain linked to its downloadable recording. Keep the existing four-track cap, preparation-only decode and first-use sprite prewarm; the prior hitch fix must survive presentation polish.

## Evidence should use real inputs and preserve its scope

The client now has an explicit capture-only public-Intent replay: a normal isolated course entrance, one supplied intention per presented frame, and neutral intentions after exhaustion. Pause, Atlas and loading suspend consumption. This allows a real guardian defeat and score return to be reviewed without mutating enemy health, invulnerability or player position. Staged stair showcases are separately marked as fixtures and start travel with public Down inputs.

The native course-4 replay first exposes the living boss at world tick 450: its rendered left edge is 3152 and the viewport's right edge is 3154. The prepared battle composition starts then; real combat reduces health 3 → 2 → 1 → 0 and returns to the world theme at defeat tick 829. The decode count stays at three throughout. This is evidence of actual encounter-driven switching, rather than a presentation fixture setting the boss dead.

The native Windows package and installer gates remain mandatory before publication. Verified delivery includes the immutable release, installer checksum and public source, BlueEngineGames history and screenshot update, a successful Pages deployment and live verification. The player's repeated request to include these development hurdles in engine feedback is honored by this document and retrievable learning-ledger entries, not just a chat summary.

The previous release also exposed delivery friction outside the engine: transferring the full 354 MB Actions artifact was slow over one connection. Sixteen bounded HTTPS ranges completed its exact archive in 271.6 seconds; full API-provided SHA-256 and every ZIP CRC were checked before replacing the original transfer. Tokens were used only for the initial GitHub API request, and redirected signed URLs stayed in memory without cross-host Authorization headers or logs. A future delivery helper should offer a small installer/proof artifact alongside the complete QA package, preserve resumable partial work, and verify exact source/build/asset identity rather than treating a successful upload as a successful publication. This is distribution evidence, not a game frame-time improvement.
