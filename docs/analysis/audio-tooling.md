# Named audio authoring milestone

Starting revision: `75e6d00e8b5674b364d32b49659b125ad30291f2`, after the stabilization and
verification-speed updates passed Linux/Windows CI and were pushed to main. Work uses the isolated
checkout and `audio-tooling` branch. Exact final revision and final gates are reported at delivery.

The existing synthesizer already provided deep DSP, 23 effect presets, procedural music and ambient
loops. The gap was a routine data-authoring path: Rust closures, numeric cue indices, music synthesis
at startup, and swallowed worker/decoder failures. Named versioned projects now expose those tools
alongside authored stereo polyphonic scores, envelopes and filters. Rendered PCM bundles can be
checked and reused by the game. This milestone adds no external service or audio dependency.

Read `be2-tools audio describe` and `docs/AUDIO.md` first. The compact contract and reference JSON
support routine work without source exploration. `be2.py context` routes a representative composition
query to the feature/docs/example, with an explicit regression. L-061 records the supported path;
L-026 retains the still-open live spatial/pitch limitation, distinguishing authored stereo pan.
The four prior discovery task sets retain their aggregate and individual regression coverage.
These are development fixtures and estimated context size, not measured AI token savings or game
completion rates.

The Observatory audio fixture contains four cues and two music layers. Its note-based battery alarm
and success phrase, lowpassed minor-key bed and alternating stereo accents demonstrate the schema;
footsteps/shutter reuse preset variants. Music release tails wrap rather than disappearing at each
loop boundary. A common attenuation ceiling protects any subset of adaptive layers. Scores, imported
files and bundles have strict format/count/resource validation; corrupt runtime assets fail before
device decoding. Existing `SoundBank` APIs survive, while failure states and music-only readiness
are now meaningful.

Evidence is committed at `assets/audio/observatory/evidence`: the 5.333-second stereo mix, native
960x600 preview image, source-project SHA-256, runtime submission events and numeric quality reports.
The native run loaded the checked bundle, submitted battery/shutter/step/success on distinct frames,
ramped adaptive layers, captured at ready frame 180 and stopped loops at 240. The image was inspected.
The full mix and all eight effect variants passed `audio_report.py --fail-on-issues` (music with
`--loop`); music seam jump was 0.001099 and no seam discontinuity was detected. The first iteration
flagged an onset in the legacy success preset; the example was refined through a data-only authored
phrase and rerendered with the same binary. That earlier quality failure is not passing evidence.

Behavioral Rust checks cover named bundle roundtrip/no overwrite, same-platform repeatability,
checksum corruption, every adaptive subset, wrapped tails, A4 pitch/hard-left pan/full release,
schema/range/resource failures, missing/truncated/wrong-rate/traversing imports, generated music-only
layers and the real CLI. Loader tests cover failed/disconnected workers, partial-bank failure,
music-only readiness and incremental delivery. Python checks distinguish continuous nonzero loop
endpoints from a real seam discontinuity, including opposite-polarity stereo. Final full verification
also retains default/headless Rust, rustdoc, Clippy, headless boundaries, native authoring and all
Python suites; both platform CI and sandbox packaging remain required.

The negative native fixture removes one music asset. Loading must fail before readiness/cue
submission and produce no success capture. The first preview iteration printed its async error but
exited zero; its entry point now emits structured failure and exits nonzero. Final negative runtime
evidence is recorded alongside the successful preview instead of treating that earlier exit as a pass.

Limits: numeric evidence does not assess pleasantness, balance by ear, audible physical hardware,
actual AI token use or player-facing performance. The backend submits stems individually; no
sample-clock start guarantee. Authored pan/pitch is PCM, not a live spatial/per-voice API. Asset
headroom is not a final runtime limiter across simultaneous effects plus music. Large banks need
partitioning. The stock GameDocument runner's audio pipeline and split-screen co-op are separate
work; this fixture does not duplicate gameplay authority or claim those features.
