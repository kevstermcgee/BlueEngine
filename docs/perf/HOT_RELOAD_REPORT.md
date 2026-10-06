# Hot reload acceptance: stopped

The user required stopping an item when measurements did not show a benefit or the tool proved unreliable. Neither prototype is shipped. Engine code and dependencies are restored to the accepted static-model-import revision (`d0f17267880a5432e809b5eb25fcd016a53531bf`). No verification gate was removed.

## Content experiment

The prototype explicitly watched map/GameDocument/rendered audio JSON; optional `--watch-audio PROJECT.json` rendered the existing AudioProject format off the game loop into checked memory without rewriting the shipped bank. It used existing validators, checked engine saves for unchanged authority, fresh local authority for changed content, current position/look and the existing camera. Invalid content remained unapplied; online/host sessions refused reload. One rendering worker was allowed at a time. There was no new gameplay language, gameplay system or independent verification cache.

Focused commands:

```sh
CARGO_TARGET_DIR=/home/kevin/BlueEngine/target TMPDIR=/mnt/blueengine-usb/be2-task-tmp cargo test --locked --profile itest --no-default-features --lib reload
CARGO_TARGET_DIR=/home/kevin/BlueEngine/target TMPDIR=/mnt/blueengine-usb/be2-task-tmp cargo clippy --locked --profile itest --all-targets -- -D warnings
CARGO_TARGET_DIR=/home/kevin/BlueEngine/target TMPDIR=/mnt/blueengine-usb/be2-task-tmp cargo build --locked --profile itest --bin be2 --bin be2-tools
python3 .be2-work/hot-reload/measure_content.py
```

The focused test run passed 12 tests (nine new, three existing), zero failures, 0.94 seconds execution, 9.47 seconds including Cargo. Clippy passed in 11.05 seconds before the final two audio tests; the built native client passed compilation in 22.25 seconds. An initial frontend borrow-check failure was corrected before those successful checks. These are prototype evidence, **not full shipping verification**.

The measurement launched the existing stock example on Linux Xvfb/software OpenGL and detected actual panel-color changes from X11 pixels. It did not treat a "reload succeeded" message as visible evidence. Every restart trial used a new process; watcher trials retained one process. Both used a warm compiled executable. The first restart includes first-use process/graphics/cache overhead; no empty-compiler-cache or cold-OS measurement was made. The exact argv, binary SHA-256, source hashes, pixels and events are in [content-results.json](hot-reload/content-results.json).

| Edit-to-visible seconds | Trial 1 | Trial 2 | Trial 3 | Median |
|---|---:|---:|---:|---:|
| Restart | 0.4768 | 0.1902 | 0.1950 | 0.1950 |
| Watch | 0.2716 | 0.2694 | 0.2917 | 0.2716 |

The median regressed **39.3%**. Valid replacement preparation took 2.74–2.91 ms; the conservative 250 ms hash-polling interval dominated. Both routes triggered zero builds. Runtime events and unit tests support pose preservation, but this benchmark did not measure navigation recovery, model tokens, a fresh-agent authoring task, CPU/memory impact or overall task completion. We did not infer an unmeasured whole-task benefit to excuse the slower observed metric. The negative result stops this implementation.

An invalid GameDocument produced the existing CLI diagnostic, retained the previous blue panel/world, then accepted the repair. The two retained captures were visually inspected. Audio source changes, invalid audio and shipped-bank preservation have headless coverage; audio-device playback and native map/client controls were not fully exercised before stopping. Windows runtime/graphics checks and browser content watching were not performed.

For review/reproduction, [content-prototype.patch.gz](hot-reload/content-prototype.patch.gz) contains the rejected source and tests. Decompress and apply it in an isolated checkout of the baseline above, build the two native binaries, then run:

```sh
python3 docs/perf/hot-reload/measure_content.py --binary /path/to/target/itest/be2
```

The preserved script only adds portable binary/output arguments to the originally measured driver. Results go into ignored `.be2-work`; the patch is a review artifact and supplies no shipping capability. Do not enable a faster polling rate without measuring CPU/memory on large maps. The next experiment, if requested separately, should use native file events with content validation and bounded fallback polling.

## Rust/Subsecond experiment

Official Dioxus CLI **0.7.10** was downloaded to the USB and checked against its published SHA-256. `subsecond = "=0.7.10"` and `dioxus-devtools = "=0.7.10"` were used in an external two-file prototype: a library owned a counter's `step` and presentation functions; `main.rs` called them through `subsecond::call` after `connect_subsecond`. This mirrors the current custom-sim starter's main/library ownership without compiling hot-patching into this engine, its tests, server, release or CI.

```sh
dx serve --platform linux --hot-patch --interactive false --open false --json-output
CARGO_HOME=/path/to/private-cargo-home RUSTC_WRAPPER='' dx serve --platform linux --hot-patch --interactive false --open false --json-output
```

The private Cargo home reused registry/bin symlinks and omitted the host's sccache/mold configuration. Initial builds completed in 53.99 and 27.74 seconds respectively. This is setup evidence, not a hot-patch speed comparison.

| Configuration | Edit | CLI claimed patch | Changed running output |
|---|---|---:|---|
| Host | library | 139 ms | absent for 30.0863 s |
| Host | tip-crate control | 129 ms | absent for 15.0580 s |
| Clean configuration | library | 264 ms | absent for 15.0396 s |
| Clean configuration | tip-crate control | 174 ms | absent for 15.0172 s |

[Subsecond results](hot-reload/subsecond-results.json) retain commands, package/checksum identity and outcomes. A compiler/CLI success was **not** accepted as behavioral success. The cause was not fully isolated; the failed tip-crate control prevents attributing everything to library ownership. Current [Subsecond source/docs](https://docs.rs/subsecond/0.7.10/src/subsecond/lib.rs.html) also describe tip-crate, thread-local and struct-layout limitations. Those documents are investigation leads, not independent proof of our failure's cause.

The prototype built on the installed stable Rust 1.98.1; no nightly was installed. Dioxus CLI is required. Windows was unavailable for this experiment, so neither Windows reliability nor cross-platform recovery is claimed. Struct-layout crash/restart/snapshot recovery was not attempted after ordinary code patching failed. Trials stopped early, rather than running three fresh-agent cohorts against an unreliable path. No Dioxus dependency, unsafe patch integration, compatibility refactor or hot-patch feature is added to the engine.
