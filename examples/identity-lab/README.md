# Identity Lab

The same four-switch puzzle is presented as a field notebook, a mechanical instrument,
and a toy arcade. Selecting a lamp toggles it and its next neighbour; light all four
within four turns. Arrows select, Space/click operates, Esc pauses, R restarts,
K saves, L loads, M toggles effects/ambience and N toggles music.

`src/lib.rs` is the sole rule authority. `src/presentation.rs` uses public `Game::interface`,
`UiFrame`, action hit regions, named fonts and matching metrics. Each layout uses the
same client-owned actions. Native main resolves assets and supplies the Windows focus
callback. There is no independent UI, audio, save, input or lifecycle runtime.

| Presentation | Interface and motion | Typography | Sound/music |
|---|---|---|---|
| Notebook | Paper spread, ruled observations, left-page index; restrained motion | Liberation Serif Italic | Imported synthetic graphite/page gestures; sparse synthetic room air |
| Instrument | Enamel panel, gauges, four physical control keys, status lamp | Liberation Mono | Imported synthetic relay/latch transients, low alarm; no music |
| Arcade | Bold faces, stacked large menu pads, pulsing focus and bouncing lamps | Liberation Sans Bold | Authored melodic cues and short percussive bass phrase |

These are illustrative choices, not built-in engine styles. All use the same semantic
events and different PCM assets. Audio sources are synthetic and authored for this
example, not recordings; fonts carry their bundled Liberation license.

From this directory:

```sh
cargo test --no-default-features
cargo run -- --identity notebook
cargo run -- --identity instrument
cargo run -- --identity arcade
python scripts/check.py --skip-ship
python scripts/ship.py ship --no-install
```

Ship on Windows for Windows resources/package evidence. Linux is a development target.
To verify all three from an isolated package, run `python scripts/presentation_smoke.py
 dist/identity-lab.exe .blue-check/identity-review` on Windows (binary has no `.exe` on Linux).
On Linux prefix `xvfb-run -a` for an isolated virtual display. For a development binary,
pass `--assets assets`. The script preserves captures/logs on failure, tests normal,
small and square windows, actual click hit regions, start/pause/resume/results,
save/load/restart, fonts, named submissions, missing effects/bindings and muted bypass.
It uses a null ALSA device on Linux; physical audio and controllers need separate inspection.
Look at the captured PNGs: hash equivalence proves shared behavior, not artistic quality.

Audio iteration needs data rendering, not recompilation. From the engine root:

```sh
python3 tools/be2.py map audio render examples/identity-lab/assets/audio-source/notebook.json NEW_BUNDLE
python3 tools/be2.py map audio check NEW_BUNDLE
python3 tools/audio_report.py NEW_BUNDLE/music-air.wav --loop --json
```

After checking, replace the corresponding runtime bank between runs. Keep JSON/imports,
checked bank files and fonts in `assets/`; the identity's `"package": ["assets"]`
declaration includes them in standard packaging. The reference
banks are committed to make startup/package checks reproducible. Editing a source project
without rerendering does not change what the runtime plays.

For authoring other games use the authoritative engine [presentation contract](../../docs/GAME_PRESENTATION.md)
and [audio contract](../../docs/AUDIO.md), discoverable through `be2.py context`.
