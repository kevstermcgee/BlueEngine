# Observatory audio project

This small audio fixture accompanies Observatory Night Watch's identity. Its cue names are `battery`,
`shutter`, `step` and `success`; the authored minor-key music has `orbit` and `signal` layers. It is a
standalone preview fixture, not a new gameplay rule or a stock-runtime audio hookup.

```sh
be2-tools audio describe
be2-tools audio validate assets/audio/observatory/project.json
be2-tools audio render assets/audio/observatory/project.json NEW_BUNDLE
be2-tools audio check NEW_BUNDLE
cargo run --profile fast --example audio_preview -- NEW_BUNDLE
```

Edit the JSON's notes, instruments, gains or presets and render a new directory. Relaunch the same
preview executable with it: no engine rebuild. Keys 1–9 play named cues; Up/Down fade the second music
layer; Escape stops the loops and closes. `--smoke CAPTURE.png` submits all four cues across separate
frames, ramps the adaptive mix, captures at ready frame 180 and closes at 240 (30-second timeout).

For quality evidence run `python tools/audio_report.py NEW_BUNDLE/preview-mix.wav --loop --json
--fail-on-issues`; one-shot effects omit `--loop`. A nonzero loop endpoint is valid when the wrapped
waveform is continuous. The shipped renderer keeps tails across the loop boundary instead of fading
every loop to silence. Numerical checks and a native preview prove assets and submission, not how
pleasant the music sounds or whether a particular physical device was audible.

Committed [preview](evidence/preview.png), [stereo mix](evidence/preview-mix.wav) and
[measurements/submission events](evidence/evidence.json) document the exercised fixture. The evidence
directory is not a runtime bank; render the source project to create its complete bundle.
