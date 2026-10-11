# Sway

Your sound, in color. A native music visualizer built with BlueEngine.

The published product is a Windows x64 installer. Linux builds are for renderer,
decoder and keyboard development checks; file-manager drops on Linux depend on
upstream backend fixes and are not a supported distribution target.

Choose music, or drop one or more files anywhere in the window. Sway plays them
in order and repeats the playlist, choosing a fresh visual identity on every
listen. Adding more files keeps the current song playing and appends to the queue.
MP3, WAV, FLAC, OGG/Vorbis, M4A/AAC, AIFF and the audio track of supported MP4
media work through the streaming decoder. Damaged or unsupported tracks are
skipped; if none can play, Sway shows the import screen with an error.

During playback the only overlay is the song's filename, without its extension,
in the lower left for ten seconds. Space pauses both sound and motion, including
that timer. There is no persistent HUD.

- **Space:** pause/resume music and animation together.
- **O:** choose one or multiple files; **drag and drop:** import one or multiple files.
- **Esc / Tab / right click:** open or close options.
- **Left / Right:** previous or next song.
- **R:** a fresh look without restarting the song.
- **F:** fullscreen.

Options offer Automatic or six visual families (Orbit, Silk, Prism, Bloom,
Stardust, Tide), six color palettes, and Calm/Flow/Surge motion. Automatic gently
transitions between new seeded chapters rather than replaying one animation.
Spectrum, waveform, energy and onset information come from the decoded samples
sent to the player. FFT and history buffers have fixed bounds; tracks stream
from disk instead of preloading the entire playlist. Songs remain local.

The native client composes BlueEngine's `Scene::render_view`, logical viewport,
fixed-step `Simulation`, deterministic variation rules and `Snapshot` contract.
Its custom streaming audio client uses the engine's graphics-only feature so
it can coexist with a different audio backend without conflicting ALSA bindings.

From this directory:

```sh
cargo test --no-default-features
python scripts/check.py
python scripts/ship.py ship --no-install
```

Development QA accepts repeated `--file PATH`, `--seed NUMBER`, `--style 0..6`,
`--size WIDTHxHEIGHT`, standard engine capture flags, and `--benchmark-seconds N`.
`--mute` disables device output while advancing the same decoded sample tap at
real elapsed time. `--verify` imports the bundled one-second test tone; it does
not synthesize pretend reactions. Performance measurements use ordinary native
frames after a one-second warmup, including actual frame intervals.

See `docs/feedback/2026-10-11-sway.md` in the engine for improvement notes.
