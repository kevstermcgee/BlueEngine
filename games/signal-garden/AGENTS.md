# Signal Garden agent guide

Read this file, `src/lib.rs`, and `tests/determinism.rs` for a rule change. Read `src/main.rs`
only for devices, visuals or menu changes; `src/audio.rs` only for sounds. `README.md` is the
player contract. Do not inspect engine implementation to rediscover APIs: the engine's
`docs/CUSTOM_SIM_CHEATSHEET.md` is the game-facing reference. Run its context command for
an unfamiliar capability: `python tools/be2.py context "<need>" --compact` in the engine root.

## Files and invariants

- `src/lib.rs`: sole authority, integer state, fixed 60 Hz `Input` → `Sim::step` → `Event`.
  `State` is also `Snapshot::State`; every future-deciding field is saved and hashed. No rendering,
  audio device, wall clock, privileged autoplay, or input handling belongs here.
- `src/main.rs`: shared `ClientInput`, `GameShell`, `Lifecycle`, `kit` drawing/HUD and `SoundBank`.
  The overview never captures the cursor. Gate device actions with `shell.accepting_input()`.
  `--script` and `--playback` drive the same `Input` as devices. The public input solver leaves
  patrols active. The shell owns menus and fullscreen; do not duplicate them.
- `tests/determinism.rs`: actual winning routes on three seeds, wrong relay, defeat, cooldowns,
  shield, exact save continuation, corrupt-load rejection and version-one save migration.
- `src/audio.rs` and `tests/audio.rs`: the same generated WAV bytes used by the client and analysis.
- `assets/identity.json`: title, controls, declared package files and a bounded smoke script.
  Icons, platform glue and `scripts/{check,ship,dev}.py` are generated infrastructure.

## Predictable modification and upgrade

The shield was the modification exercise: `Input.shield`, `State.shield_ticks`, one event and
one rule; map Q/pad LB and its script cue in main, then handle the event's sound/visual.
The existing no-shield playthroughs still win. A real pre-change save at tick 331 is
`tests/fixtures/v1.be2save`; VERSION 2 adds a migration that gives it zero shield time.
Any future save layout/hash change must bump VERSION and add a migration plus a fixture test.
Never overwrite a released fixture or migration to make a test pass.

For an engine update, from the engine root run:

```sh
python3 tools/be2.py upgrade plan games/signal-garden --to TARGET --out /tmp/garden-upgrade.json
```

Read that packet, update `Cargo.toml` if relocating the engine, and copy newer shared shipping
infrastructure when required: `cp templates/game_ship.py games/signal-garden/scripts/ship.py`.
Rebuild/reverify before changing `engine_revision` in identity. A revision warning is not test evidence.
This bundled source omits the optional historical engine_revision in identity; dist/ship.json records
the exact engine commit actually packaged. The default dependency is the surrounding engine checkout
(`../..`); after exporting this source
alone, point it at the intended engine checkout. No source path is used by the shipped executable.

## Commands

From this game's directory (optional shared build cache: `CARGO_TARGET_DIR=/path/to/engine/target`):

```sh
cargo test --locked --no-default-features     # simulation, audio bytes, saves; no display
scripts/blue dev --build-only                # optimized development build
python3 scripts/check.py --skip-ship         # full project iteration
scripts/blue ship                           # release package, shortcut, integrity, isolated capture smoke
python3 scripts/check.py                     # final project check including ship gate
```

A silent capture verifies graphics, not audio. `--audible` opts in; `--mute` always wins.
`SoundBank::status()` reports rendering/loading failures and playback submissions. A null audio
output cannot prove a listener heard the track. Headless rules and frame checks are distinct.

Generate the solver's exact input sequence and export the exact WAVs without a window:

```sh
GARDEN_REPLAY_OUT=/tmp/garden-inputs.json cargo test --locked --no-default-features --test determinism write_public_input_playthrough
GARDEN_AUDIO_OUT=/tmp/garden-audio cargo test --locked --no-default-features --test audio
```

From the engine root, capture the real final player screen (one tick per frame, fixed seed):

```sh
python3 tools/xcapture.py target/debug/signal-garden --frames 30,9860 --timeout 900 --out /tmp/garden-frames -- --seed 7 --playback /tmp/garden-inputs.json --expect won
```

Use new capture directories. `--expect won|lost|playing` makes an unexpected outcome fail;
`--perf` reports simulated timing separately from measured drawing work. `--script` understands
`fwd`, `back`, `left`, `right` holds, `interact` holds, `shield` edges, `menu`, `down`, `select`,
`fullscreen` edges, plus the engine's `save`/`load` edges. Example settings test:
`menu@60,down@61,down@62,select@63,select@65,down@66,select@67`.
Inspect world, outcome, settings and small-window frames. Package integrity needs no display;
launch/capture smoke needs one or Xvfb. Smoke runs only declared files in a temporary folder,
excluding local saves/settings/stale assets, and preserves its log in `.blue-check/`.
