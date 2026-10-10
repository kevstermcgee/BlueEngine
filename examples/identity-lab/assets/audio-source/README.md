# Identity Lab sound sources

`notebook.json`, `instrument.json` and `arcade.json` are ordinary engine AudioProjects.
Pencil/page WAVs are deterministic filtered-noise gestures; relay/latch WAVs are damped
185 Hz tones mixed with filtered noise; room air is quiet filtered noise with a softened
boundary. They were synthesized for this example, not sampled from recordings.
Arcade cues and music are authored triangle/square/saw scores. All imports are 44.1 kHz
PCM16 mono; the engine preserves/stereo-renders them and checks the runtime bundles.

The synthetic source recipe is retained in `scripts/make_sources.py` at the game root.
