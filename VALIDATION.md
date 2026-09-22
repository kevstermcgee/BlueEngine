# Validation — Vesper3D 0.1.0

Verified locally on Windows x64, September 22, 2026. The initial core compiled on Rust 1.87.0; the final source, tests, checks and distributed executable were built with Rust 1.98.1 after the host toolchain changed during development. Renderer worker count: 12. FFmpeg and ffprobe were available locally.

## Automated checks

- **35 tests passed:** 22 library tests and 13 CLI integration tests; no tests ignored. The FFmpeg-dependent tests executed on this machine.
- `cargo clippy --all-targets --locked -- -D warnings` passed.
- `cargo fmt --check` passed.
- Release build passed with locked dependencies.

Tests cover analytic intersections, transformed geometry, BVH agreement with brute force, affine inversion, parallel slab boundaries, keyframe interpolation, invalid keys, hierarchy/visibility, cycles, singular/explosive scales, frame-count rounding, strict unknown fields, deterministic RGB across thread counts, repetition and expansion limits, invalid sampling budgets, cancellation before/during rendering, temporary-file cleanup, successful atomic replacement, output races, real PNG decoding, animated-image differences, invalid CLI input, missing materials/parents, dimensions, extreme portrait contact sheets, asset traversal, OBJ index checks/negative indices/quads, missing encoders, invalid audio, and actual MP4 encoding/probing/decoding.

## Showcase verification

`first-light.mp4` contains 288 frames at 1280×720, 24 fps, H.264/yuv420p video and AAC audio. ffprobe reports exactly 12.000 seconds and 3,733,644 bytes. A full decode with `ffmpeg -v error -i first-light.mp4 -f null -` completed without errors. Six decoded frames were inspected visually, along with high-quality stills and the grove/materials examples.

The film was rendered with the same renderer core before the later CLI/validation/repetition additions. Its scene does not use repeats. The final executable's additional features were tested separately, and its contact sheet was inspected. Exact image bytes across compiler versions are not promised.

## Measured performance

Wall-clock measurements from this machine, not portable performance guarantees:

| Scene / operation | Setting | Time |
|---|---|---:|
| First Light frame at 5s | 1280×720, standard, 12 workers | 2.28 s before background build load; 3.26 s with the film rendering concurrently |
| First Light frame at 5s | 960×540, high | 4.81 s |
| First Light frame at 5s | 640×360, draft | 0.20 s while film rendering |
| Hello frame at 1s | 640×360, standard | 0.11 s while film rendering |
| Grove frame | 640×360, standard, 113 expanded primitives | 0.14 s while film rendering |
| Materials frame | 640×360, standard | 0.25 s while film rendering |
| Full First Light film | 720p, standard, 288 frames | 874.59 s / 14m35s, including competing compilation and test work |

Scene complexity and quality matter substantially: the showcase includes several tessellated rings, reflective surfaces and three lights. This is not a real-time engine. Draft/small previews provide a much faster authoring loop. The 64-bit Windows executable is under 1 MB; FFmpeg remains a separate installation.

## Not established by these checks

No soak test lasting days, fuzzing campaign, sanitizer run, hostile-media audit, real-time guarantee, cross-platform execution result, or visual match across architectures is claimed. CI definitions are supplied, but remote CI was not run for this local repository. The README and architecture document describe missing features and known limitations.
