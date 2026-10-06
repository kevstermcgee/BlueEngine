# Static model import measurements

This pass adds static glTF art tooling and a small CC0 catalog. It does not add gameplay rules or a scripting language. Baseline engine revision: `ca2799fa7c4b34d52ec5580c9a3935c00d306865`; the measured implementation was an uncommitted isolated worktree. Final revision and Linux/Windows CI are recorded with delivery.

## Fresh-context authoring trials

Three fresh agents per cohort authored the same bounded furniture function: one table, four inward-facing chairs and a square floor lamp at prescribed positions and dimensions. The baseline used public procedural draw primitives; the after route used preprocessed CC0 models. Agents did not compile or run games. Timing begins at each agent’s first recorded clock observation; some observations followed their initial AGENTS read. Figures include source/diary authoring and exclude coordinator compilation, lint corrections, packaging and renders. These are bounded authoring trials, not end-to-end game completion. The task is documented in `model-import/TASK.md`.

| Metric | Before, n=3 | Initial after, n=3 | Refined after, n=3 |
|---|---:|---:|---:|
| Authoring seconds | 120.357 / 86.000 / 137.693 | 111.923 / 93.582 / 101.750 | 53.000 / 81.973 / 92.650 |
| Median seconds | 120.357 | 101.750 | 81.973 |
| Median authored Rust bytes | 2,962 | 2,239 | 2,340 |
| Agents reading engine implementation | 3 | 0 | 0 |
| Agent compilation/test invocations | 0 | 0 | 0 |

The refined median authoring time is 31.9% lower; authored source bytes are 21.0% lower. Tokens are **unavailable**, and bytes are not tokens. No claim is made about smaller models, total game completion, or a general quality score.

The first after cohort inspected more files, including raw model vertices, while discovering orientation. That regression is retained in `model-import/measurements.json`. We added reviewed chair `forward: +z` metadata, compact catalog search, a public facing explanation, and model-related keywords in the existing feature index. Final agents inspected three public instruction/guide files and catalog packets; baseline agents inspected three or four files including one engine implementation. Total direct-file median stayed three; engine-source reading fell to zero. Diary command accounting is not exhaustive and did not demonstrate a command-count reduction.

Reproducible coordinator verification:

```sh
CARGO_TARGET_DIR=/home/kevin/BlueEngine/target cargo build --locked --profile itest --no-default-features --features portable --example model_import_compare --example imported_room
xvfb-run -a -s '-screen 0 960x640x24' "$CARGO_TARGET_DIR/itest/examples/model_import_compare" before-1 /tmp/before-1.png
xvfb-run -a -s '-screen 0 960x640x24' "$CARGO_TARGET_DIR/itest/examples/model_import_compare" final-1 /tmp/final-1.png
```

Replay all `before-{1,2,3}`, `after-{1,2,3}` and `final-{1,2,3}` cases. Exact observed commands, exit codes, elapsed times and capture hashes are in `model-import/render-results.json`; all nine rendered successfully. Original authored byte counts/hashes are retained in the measurement JSON. Replay embeddings are relative; formatting, three bit-identical f32 literal spellings and one const thread-local initializer were normalized for Clippy. These corrections are coordinator work and are excluded from agent authoring times.

All captures were inspected (the six baseline/initial captures are byte-identical to previously reviewed captures). Furniture is legible with separated legs and chair backs. Imported and procedural geometry is not pixel-identical: lamp silhouettes and some chair yaw differ. The default portable renderer remains unlit; these are visual function checks, not photorealism or physical lighting claims.

## Converter setup and warm imports

First-use cold compilation initially took 97.632 seconds and failed because the first adapter rejected `KHR_materials_unlit`. This is a failed attempt, **not a successful cold import measurement**. The extension is now supported and covered by the actual Kenney fixtures. Kenney's chair/table contain zero-area triangles: default import rejects them; explicit `--repair-degenerate` reports original triangle IDs, removes only those findings and reruns the full existing `kit::lint`.

After source compilation, three independent warm output packs took 0.1856, 0.1702 and 0.1718 seconds. Each Cargo freshness check reported 164 fresh artifacts and zero rebuilt artifacts; conversion itself took approximately 0.0043 seconds. Exact argv, results and separate build/conversion timings are in `model-import/measurements.json`. A representative command is:

```sh
CARGO_TARGET_DIR=/home/kevin/BlueEngine/target python3 tools/assets.py import-model tests/fixtures/models/chair.glb --output /tmp/cc0-chair-new --id chair --pack-id cc0/benchmark --source-url https://kenney.nl/assets/furniture-kit --attribution 'Kenney, Furniture Kit 2.0, CC0-1.0' --tag furniture --scale 2.5 --repair-degenerate --forward +z
```

The output directory must not already exist. No independent verification cache was added: Cargo owns executable freshness; pack provenance and catalog validation use content hashes. A warm result is not a cold-environment claim.

The canonical fetch also succeeded:

```sh
python3 tools/assets.py fetch-models --output /mnt/blueengine-usb/be2-model-test-pack
```

Observed result: `ok: true`, 25,190 selected bytes, upstream license plus three glTF models, `CC0-1.0`, `checksum: verified`. The reviewed 5.13 MB archive is not committed. Committed test models and converted fixtures remain small. Archive/member SHA-256 and the 256 KiB selection budget are enforced.

## Verification and operational limits

Focused checks passed:

- `cargo test --locked --profile itest --no-default-features --features model-import --lib model_import`: six passed, zero failed, 0.09 seconds test execution; 17.49 seconds Cargo completion after latest geometry changes.
- `cargo clippy --locked --profile itest --no-default-features --features model-import,portable --all-targets -- -D warnings`: passed; 5.52 seconds after replay lint fixes.
- `python3 -m unittest tools.test_assets tools.test_model_import`: ten passed in 0.853 seconds.
- `python3 tools/assets.py validate`: three packs, 74 assets, zero issues.
- Native imported-room capture: imported furniture and colored texture visually inspected. Run `python3 tools/xcapture.py "$CARGO_TARGET_DIR/itest/examples/imported_room" --frames 5 --out /tmp/cc0-room-capture --timeout 60`.

The canonical browser build passed in 106.167 seconds. Desktop and emulated-touch checks passed WASM startup, input, saves/reload, offline/update recovery, focus recovery and the ordinary winning scenario; desktop synthetic controller checks passed. The native scenario hash matched the browser route. Imported furniture and a textured prop are visible in both reviewed playing captures. `model-import/browser-evidence.json` retains the exact command, artifact identity, flags, metrics and physical-device limitations. Package size was 1,239,965 bytes (WASM 1,093,339 bytes). No publication was attempted.

Native shipping passed: a game-specific icon and desktop shortcut, three declared files with verified hashes, then an isolated packaged-game smoke render. Release compilation took 33.40 seconds. The 960 × 540 smoke capture was inspected and contains imported furniture and the colored texture. Exact commands, project results, package identity and platform skips are in `model-import/native-evidence.json`. Windows resource/shortcut launch checks were skipped on this Linux host.

Full engine verification passed with `CARGO_TARGET_DIR=/home/kevin/BlueEngine/target TMPDIR=/mnt/blueengine-usb/be2-task-tmp python3 tools/perf.py record --suite check --note "Static glTF CC0 authoring and bounded replay; compiled caches relocated to USB after disk-full recovery"`. All 34 gates and 2,977 recorded test executions passed in 719.623 seconds, with zero failed commands. Cargo reported 3,716 fresh and 380 built artifacts. Recorded child CPU was 1,366.969 seconds; maximum child RSS was 3,002,949,632 bytes (one child high-water mark, not simultaneous system memory). `model-import/engine-evidence.json` retains every gate command and timing. Existing 32 gates were preserved and two importer gates added.

The baseline exact revision has green Linux/Windows Engine checks ([run 37517340217](https://github.com/kevstermcgee/BlueEngine/actions/runs/37517340217)). Exact implementation-revision remote CI is checked after commit; this local report does not certify Windows execution.

Two earlier full-check attempts are retained locally: one failed because TMPDIR was inside a Git checkout (moving it outside fixed the twelve affected regression tests); one failed with a disk-full linker error. No checks were weakened. To recover space, compiled caches and inactive cached tools were moved to the user-authorized USB, retaining their original paths. That changes the I/O environment, so whole-engine build-speed comparisons would be misleading. No build-time, CPU or memory improvement is claimed from this art pass.

Static import is deliberately bounded: no skins/animation/morphs, alpha-mask/blend or double-sided materials; base color factor/texture only, with explicit warnings for unused PBR maps. Oversized draw chunks split without triangle loss; inputs beyond the complete lint budget reject loudly. Headless callers read only conservative collider metadata; custom rendering, shaders, logic and the existing physics/networking paths remain available.

The remaining first-use bottleneck is compiling the converter through the existing native `kit::lint` feature boundary. A separate geometry ownership boundary should be considered only with a measured benefit. The requested next item is content/code hot reload, evaluated separately with its own ADR and evidence.
