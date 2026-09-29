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

## Open ideas, in expected order of value

1. Done on this machine: mold and sccache via `~/.cargo/config.toml` (not the repo, so Windows CI is
   unchanged). Edit-and-rebuild stayed 15 s: it is dominated by compiling the engine crate itself.
   Optional: cargo-nextest for test scheduling.
2. Tiered `be2.py check` (`cargo check`, then focused tests, then full).
3. Split the engine crate so an edit rebuilds less (simulation, net, presentation).
4. Fewer test binaries or a shared test crate to cut links.
