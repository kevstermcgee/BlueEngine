# Embertrail 0.2.1: player feedback, interaction and first-use hitches

This follow-up addresses observed text overflow, repetitive interaction sounds, an unwanted controls footer, repeated underground layouts, slippery movement and intermittent stalls near new enemies. No engine Rust change was required. The useful engine work is shared presentation and preparation contracts; the room designs and movement targets remain game decisions.

## Measure text with the font that draws it

The previous captions wrapped by character count. Proportional glyph widths, real font ascent and the available panel height were absent from that estimate. A string could pass the nominal length limit and still extend outside its box. Embertrail now uses the same loaded DejaVu Sans Font in both its renderer and layout helpers, measures actual advances, wraps inside padded bounds, places baselines from ascent and ellipsizes the last allowed line. The change covers story, NPC/guardian captions, HUD notices, Atlas, title and pause screens. Controls moved to an optional pause page instead of a permanent game overlay.

Proposed engine improvement: a Scene text-box primitive taking bounds, padding, font, line height, alignment and explicit overflow policy. Validation should capture the longest actual story, track title and error message at both supported window sizes, including nested menus. String length tests or a successful menu click do not establish text containment.

## Semantic sound events, bounded playback

Many unrelated game events shared the coin cue. This was event-routing and content debt, not an absence of engine audio playback. The game now names 25 cue families and maps gameplay/menu events to 34 checked original WAV recordings. Material and motion inform the sound: clay fractures, wood knocks, spring coils, descending roots, seed casts, shell kicks and guardian impacts have separate envelopes and timbres. Quiet movement variants have cooldowns; a four-voice budget gives important events priority.

The generated bank is 2,752,456 bytes and 15.60 seconds in total. Numeric checks establish 34 unique hashes, valid 44.1 kHz stereo PCM, bounded peaks and near-zero endpoints. They do not establish perceived quality through real speakers. Proposed engine support: typed semantic event bindings, named variant groups, priority, cooldown and voice limits that retain existing checked AudioBank packaging. Do not replace all feedback with one collection sound merely because it is easy to submit.

## Traction needs genre-specific targets

At full sprint, the previous normal air drag allowed roughly 114.38 pixels of drift over 41 ticks after releasing direction. Ground release drift was 24.43 pixels over 10 ticks. The revised ordinary-ground release stops in 3 ticks/5.625 pixels; ordinary-air release stops in 5 ticks/10.078125 pixels. Reversal changes velocity sign in 3 ground ticks or 4 air ticks. A continuously held running jump retains the same 51 ticks, 292.85 horizontal pixels and 148-pixel rise, preserving authored reach.

Frostbloom surface ice intentionally retains different traction. Its warm underground rooms use ordinary traction, and ice no longer controls air braking. All 32 main courses were searched and independently replayed through public inputs with no life lost; final Vesper replay reaches the ending. These are isolated-course reachability receipts, not a continuous human campaign or a physical-controller assessment.

Proposed engine documentation: explain velocity, braking and reversal in observable distances/ticks alongside the fixed-step subpixel contract. A platformer should choose those targets before tuning coefficients. Keep jump reach checks and real traversal evidence when changing traction.

## Underground variation must change play

The old secret rooms repeated one flat layout with different surface-world colors. The replacement has eight regional mechanical archetypes with four authored variants each: root steps, crystal waterways, copper pumps, warm spring pads, glass reliquaries, cloud cisterns, thorn nurseries and citadel gear lifts. Geometry, hazards, movers, springs and reward routes differ. Art reads the actual room objects; it does not imply platforms absent from collision.

The dedicated room probe begins at an explicitly placed real surface entrance after ordinary spawn protection expires, then uses public inputs for entry, reward and far-stair return. Its independent replay asserts the real reward and no life lost. This scope deliberately excludes surface access and a contiguous campaign. A bounded search failure is not evidence that a room is impossible; inspect the heuristic before changing game rules to satisfy it.

All 32 rooms pass the strengthened probe and separate-process replay with matching state hashes. The first greedy beam found 27 and crowded below five reward shelves; retaining spatial alternatives found their legitimate lift, spring and side approaches without changing any rule. Independent replay also checks the actual far-stair overlap when Down exits, rather than inferring which exit was used from the final outside state.

Version 3 migration replaces only version 2 underground geometry while preserving surface objects and campaign progress. An inside-room save resumes at the new legal entrance. Tests use genuine engine-framed version 2-shaped data without the newly added fields, plus exact live-room continuation for all 32 courses. Defaulting new metadata alone would not migrate old collision geometry safely.

## Async audio names did not imply frame-safe decoding

Native profiling measured 144.971 ms for the title WAV decode and 142.645 ms for the meadow WAV. In Macroquad 0.4.14, `load_sound_from_bytes` is async but its body directly calls quad-snd 0.2.8 `Sound::load`; the mixer decodes every sample into a Vec synchronously. The prior file/checksum worker removed disk work but left this decode on the render thread. Reading those dependency implementations was necessary to explain the stall.

The game now draws a loading transition and prepares only the current course's world, grotto and applicable boss recordings. Active context switches submit cached handles. At most four unique decoded music tracks, including any outgoing fade, remain resident; it does not preload all sixteen. The final preparation also needs a yielded loading frame and a reset fixed-step/input clock, otherwise the next interactive frame can catch up the loading delay. Exact currently playing track identity, pause position and WAV download remain independent of preparation.

Proposed engine API: an explicit bounded prepare/loading contract or safe worker PCM decode with a constant-time device submission boundary. Expose resident bytes and decode events. Public async wrappers should document synchronous work. Do not add a second audio device or access global graphics/audio context unsafely just to move decode to a worker.

## First appearances and verification scope

The game's sprite helper decoded PNG and uploaded Texture2D on its first draw. A new enemy or guardian therefore caused resource preparation in the same frame as platforming. All 22 small sprites and common UI glyph sizes now warm the same caches before interactive play. First-use misses are observable rather than inferred from average FPS.

`--profile-frames` records simulation, presentation, renderer submission, audio update, snapshot/save CPU and consecutive active frame intervals. Preparation is reported separately. Capture mode intentionally disables persistence, so a capture-only timing run would hide real autosave work. Timing runs must omit capture and use an isolated `BLUEENGINE_DATA_DIR`. Do not add a save worker without measuring the save category. Software OpenGL and an ALSA null sink describe that native test process, not physical display latency, speaker output or controller feel.

The isolated native 600-frame run without capture contained 569 active frames with real movement, jumps, pickups, death and autosaves. Active CPU p99 was 1.514 ms and maximum 2.590 ms; audio-update maximum was 0.477 ms and save maximum 1.207 ms. There were zero active sprite cache misses or active CPU frames above 16.67 ms. Consecutive active frame intervals had p99 21.993 ms and maximum 24.713 ms on this software-rendered process. The three full music decodes remained visible as 135–191 ms preparation work. This evidence supports removing the identified active decode/upload stalls; it does not certify every device or an indefinite play session. A separate cross-world boss-course transition kept the cache at four unique music tracks or fewer, including the outgoing theme (about 104.2 MB estimated decoded PCM at its peak).

A native positioned-room fixture then used actual public Down inputs to exit and re-enter: the audible track identity changed underroots → meadow → underroots and playback submissions increased 1 → 2 → 3 while the decode count stayed 2 → 2 → 2. Distinct portal exit/entry cues were submitted, and the downloaded currently playing underroots WAV matched its checked SHA-256 exactly. This separates a real transition from a script merely intending to reach a portal.

An initial baseline process printed its final JSON and then exited with signal 11. That is a failed process, regardless of a successful capture. The client now explicitly stops/drops sound and cached GPU/font handles while the native context remains alive. Three ordinary UI quits from title, gameplay pause and a boss-course fixture returned zero, as did the bounded profiling and transition runs. Repeat this gate before packaging; keep any remaining failure distinct from simulation evidence.

Verified delivery continues through the established native Windows workflow, immutable game release, BlueEngineGames history update, deployment and live verification. This automatic publication behavior is recorded in the original update feedback and game/release AGENTS; it must not require the player to ask again.
