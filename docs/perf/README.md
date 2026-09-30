# Performance data

`metrics.jsonl` is append-only: one JSON row per measurement, never edited. Rows carry commit,
dirty flag, CPU, cores, rustc, linker and sccache state, so compare only rows whose `metric`,
`kind`, `profile`, `cpu` and `linker` match. Record with `python tools/perf.py record`, read with
`python tools/perf.py report`. Pass `--note` saying what changed since the last run.

Fields: `metric` (`build_headless`, `test_build`, `test_run_total`, `test_suite:<name>`,
`sim_mean_tick`), `kind` (`cold`, `incremental_edit`, `warm`, `runtime_2_players`), `profile`,
`value`, `unit`. Rows with `source: manual-...` were measured by hand, not by the script.

## Profiles (opt-in; default dev/release and CI are unchanged)

- `--profile fast`: release without LTO, 16 codegen units, incremental. For iterating and for
  running the headless server on this machine.
- `--profile itest`: dev with optimized dependencies and line-tables-only debuginfo. For tests.
- `--release`: single-unit thin LTO. For shipping only.

## Baseline (Intel N97, 4 cores, 15 GB, rustc 1.98.1, default linker, no sccache)

| Step | Before | After |
|---|---|---|
| headless build after a code edit | 63 s (`--release`) | 15 s (`fast`) |
| `physics_saves` + `save_state` suites | 306 s (`dev`) | 12 s (`itest`; one-time 118 s compile) |
| full `cargo test --no-default-features` | 402 s (`dev`) | 51 s (`itest`, mold + sccache) |
| cold headless build | 82 s (`fast`) | 38 s (`fast`, warm sccache) |
| cold test build | 118 s (`itest`) | 48 s (`itest`, warm sccache) |

Findings behind the numbers: the whole engine is one crate, so `--release` recompiles about 52k
lines as a single serial LTO unit; debug rapier/parry make physics tests 10-25x slower; each of
the 27 test binaries links the full engine. Cold builds spend about 60 s in rapier, parry,
nalgebra, rustls, quinn and ring.

Full `python tools/be2.py check` (all gates, both feature modes): 1,068 s of commands on `dev`, 376 s on
`itest`, which is now its default (`--profile dev` opts out; CI runs cargo directly and is unchanged).

## Server load (loopback, development transport, house map, `--profile fast`)

`python tools/perf.py record --suite server` (about 2 minutes) runs `examples/server_load.rs` against
a real `be2-headless` process. Synthetic clients only walk and jump; no combat, no moving props.

| Clients | Server CPU (% of one core) | Tick mean / max | Per-client bandwidth | Largest packet |
|---|---|---|---|---|
| 1 | 2.2 | 290 / 405 us | 7.6 KB/s | 396 B |
| 2 | 2.5 | 328 / 610 us | 11.1 KB/s | 587 B |
| 4 | 3.1 | 424 / 841 us | 18.4 KB/s | 972 B |
| 8 | 4.7 | 700 / 1569 us | 20.4 KB/s | 1100 B |

Server RSS stays about 6 MB. World updates arrive every 50 ms (20 Hz) with a worst gap of 67 ms and no
resyncs. A 9th client is refused ("Server is full (8 players)"): the cap is hard-coded at
`src/viewer/server.rs:230`, so 16 or 32 players needs an engine change, not just a faster machine.
Headroom is large on this CPU, but the largest packet already uses 1100 of the 1400-byte limit at 8 idle
players. Not covered: real network loss/latency, the production QUIC/TLS transport, props in motion, combat.

## Publishing (BlueEngineGames, GitHub Actions, windows-latest)

"Build Windows releases" took 15-26 minutes on each of the last five runs (19m, 26m, 15m, 16m, 20m).
Reading `.github/workflows/releases.yml` and `package_native_releases.ps1` in that repo: there is no Rust
cache; the engine is built `--release` from scratch; then each of 6 native games is built `--release`
with its own `target/`, so the physics/math/rendering dependencies compile 7 times; and every game is
rebuilt on every run even when only one changed. Baseline step times (run 36606293975): engine build 243 s, native games 865 s.

Result of the fix on branch `faster-windows-release` of BlueEngineGames (shared target dir + cache):
1,151 s -> 799 s cold cache (native games 508 s) -> 436 s warm cache (engine 159 s, native games 238 s).
The remaining cost is compiling the engine crate itself once per profile group; skipping unchanged
games would be the next step.

## Spooky Kart (a game built on the engine, `~/SpookyKart`)

`python tools/perf.py record --suite kart` runs the game's own load test (`tools/load_test.py` in that repo, or
`$BLUE_KART_DIR`): its real server over real UDP loopback with 1, 4 and 8 bot clients, one full two-lap race each
(about 7 minutes). Metrics are `kart_*`; per-character results are `kart_character_mean_place` and
`kart_character_win_rate` (rows carry a `character` field). Baseline, dev build, N97:

| Clients | Server CPU | Server tick mean / max | RAM | Per client down / up | RTT | Prediction corrections (max error) |
|---|---|---|---|---|---|---|
| 1 | 1.4% of a core | 132 / 517 us | 3.7 MB | 18.0 / 2.2 KB/s | 17 ms | 2 (0.20 m) |
| 4 | 1.6% | 149 / 566 us | 3.8 MB | 18.2 / 2.2 KB/s | 17 ms | 229 (1.11 m) |
| 8 | 1.9% | 173 / 507 us | 3.7 MB | 18.1 / 2.2 KB/s | 17 ms | 1,328 (0.88 m) |

No hard prediction snaps at any level; corrections rise with players because clients do not predict kart-to-kart
bumps. What this game taught us about the engine, with evidence, is in `~/SpookyKart/docs/ENGINE_LESSONS.md`.

## Open ideas, in expected order of value

1. Done on this machine: mold and sccache via `~/.cargo/config.toml` (not the repo, so Windows CI is
   unchanged). Edit-and-rebuild stayed 15 s: it is dominated by compiling the engine crate itself.
   Optional: cargo-nextest for test scheduling.
2. Tiered `be2.py check` (`cargo check`, then focused tests, then full).
3. Split the engine crate so an edit rebuilds less (simulation, net, presentation).
4. Fewer test binaries or a shared test crate to cut links.
