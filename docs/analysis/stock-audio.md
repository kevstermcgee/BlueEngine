# Stock audio runtime integration

Starting revision: `343c5ab61155b489cefc33b2fdfe5b06a8499094`. Named audio authoring and the
standalone preview were complete, but stock GameDocument games still had no playback pipeline.
The stock runner now loads an opt-in `presentation.audio` bundle before gameplay, validates names
and assets, submits confirmed-state cues and adapts music layers to authoritative counters.
Missing configuration retains silent defaults. The existing backend and authoring format are reused.

The graphics-free AudioCursor observes counters, outcomes and round changes. GameSession's additive
read-only observer runs after each local fixed tick, preserving multiple changes in a catch-up frame.
Startup, online joining and save loads establish/rebase a baseline; resets emit a restart cue without
playing counter-reset history. The shared simulation and input coalescing are unchanged. Online state
snapshots can omit intermediate events; this is not a new reliable network event protocol.
Loading continues network-only polling before establishing its baseline; local authority stays at
its initial tick. This prevents slow loading from leaving the online handshake idle.

Settings use the existing persistent Sound/Music toggles; muted runs explicitly skip assets/devices.
The stock settings screen has no unimplemented export control. Loaded documents retain a private,
nonserialized content root for cwd-independent, confined bundle resolution. Constructed games can
supply GameOptions::audio_root. game-validate and generated checks verify configured bundles without
opening a device. Generated stock entry points now return nonzero on runtime failure; existing entry
points can adopt the documented Result handling. Package bundles under assets/audio.

The original silent Observatory acceptance fixture is retained. game-audio.json and the optional
author.py --audio path bind shutter, calibration, battery crossing, loss, restart and success cues,
plus the signal layer following calibration. The existing checked stereo assets are reused. Focused
tests cover the real loss/restart/win simulation, eight separate catch-up transitions with unchanged
checksum, pause/finish mixing, load baselines, invalid references/names, and cwd-independent paths.
An initial test confused the scenario's final assertion step (1542) with the actual completion step
(1539); authoritative state evidence corrected that expectation instead of delaying the cue.
The first full check also caught an oversized generated guide. Its audio directions were condensed
to one line linking the supported contract, retaining the existing 3000-byte limit (2876 bytes for
shared-starter, 2976 for the maximum-length game name).
The next Linux run exposed a hub startup failure. Its test port helper randomized each allocation
despite promising disjoint ranges; it now chooses one process rotation of nonoverlapping slots.
This removes that within-process collision opportunity without retrying or weakening assertions.
CI also explicitly uploads hidden work directories so diagnostic and rendered evidence survives.

No desktop game/capture was launched successfully in this work. Local GUI/audio verification was
cancelled to respect the user's silent, unobtrusive PC preference. The added Linux CI gate instead
runs the real shipping stock client under xvfb/software GL with a null ALSA sink. It requires 1543
captured frames, the expected cue events, adaptive levels, world/loss/reset/win/menu images, a missing
music asset failing nonzero before capture, and a muted run bypassing that missing asset. It retains
logs/captures/trace/evidence.json for inspection. Exact final gate outcomes and revision are reported
at delivery; the script's assertions alone are not passing rendered evidence.

Physical audibility, subjective balance/quality, real input hardware, live spatial/per-voice audio,
and sample-clock stem synchronization remain unverified or unsupported as documented in AUDIO.md.
No local visible window, audio playback, focus change, live server deployment or manual publishing
is needed for this verification. All default/headless/Linux/Windows, release, packaging and discovery
regression gates remain intact. Discovery measurements do not establish actual AI token savings.
