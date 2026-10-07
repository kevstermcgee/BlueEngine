# Development machine utilities

Version 1.1.1. Debian/Linux, Python standard library, native Git/Cargo/systemd tools.
Source lives here; six commands in `~/.local/bin` point to `dev_tools.py`;
`dev-server-load` points to `server_load.py`, and `dev-server-isolation-test`
points to `server_isolation_test.py`.

These optional tools target Debian/Linux server hosts. They do not change BlueEngine's
game distribution policy. Link the desired scripts into the user's `~/.local/bin`;
the scripts use that user's home and existing project/configuration paths. Keep
all three Python files together so the benchmark and isolation probe can import
the shared utility module. The engine's full Linux checker runs their scratch suite;
the Windows checker skips this Linux-only utility gate.

## Start here

```sh
dev-env-doctor --save
dev-cache-manage --plan --save
dev-worktree-audit --save
game-server-health --save
dev-tools-bootstrap --check
```

Reports are JSON. `--save` writes bounded latest reports under
`~/.local/state/dev-tools`, mode 0600 in a directory of mode 0700.
No environment dumps, process command lines, tokens, or arbitrary build arguments
are saved. No daemon or recurring task is installed.

## Concurrent room benchmark

```sh
dev-server-load --plan
dev-server-load --run --rooms 1 2 4 8 --clients 8 --label baseline
dev-server-load --run --server /path/to/candidate-server --rooms 1 8 --label candidate
```

This fixture uses the installed Spooky Kart server and its existing native bot driver.
It starts private loopback UDP rooms outside the hub port pool, measures server and
bot CPU separately, validates completed races and saves binary hashes. The tool holds
the cooperative build lock, applies nice 10, enforces a time limit and disk/memory
reserves, and stops only its own process groups on failure or interruption. It opens
no public rooms. The default plan launches no processes except the server's `--info` probe.

`server-load-benchmark.json` is the latest private checkpoint; raw per-run logs remain
in private `server-load-*` directories for review. Each run bounds its log sizes;
old runs are not automatically removed. CPU percentages use one core as 100%.
Reported server tick mean/max cover step work; scheduling lateness and receive work
are not included in those tick timings. Wire byte counters are attempted sends,
not acknowledgements or proof of delivery. Local bot load does not certify Internet,
QUIC/TLS, adverse network conditions or a maximum supported player count. Compare
matching game builds, compiler profiles, seeds and host conditions before claiming
a performance improvement.

Candidate servers with transport telemetry also report acceptance, backpressure,
errors and oversized messages separately; older binaries show `send_outcomes: null`.
Per-room archive sizes are included. `game-server-health` adds a bounded metadata-only
inventory of configured match archives, with their total size. Neither command trims
the archive; any retention policy remains a separately reviewed change.

## Room isolation prototype

`dev-server-isolation-test` prints its plan. `dev-server-isolation-test --run`
creates two temporary systemd user services with separate 64 MiB memory limits,
zero swap allowance and short runtime limits. It deliberately exceeds one limit,
requires an `oom-kill` result while the sibling stays active, then stops and resets
only its own randomly named units. Evidence goes to `server-isolation-test.json`.
This proves host containment support; it does not integrate isolation into the
hub's current child-process spawner. It changes no live server unit or limits.

## Build coordination

```sh
dev-build-run --cwd ~/BlueEngine -- python3 tools/be2.py check --changed --loop inner
dev-build-run --cwd ~/BlueEngine --wait 300 -- python3 tools/be2.py check --changed
```

The wrapper sets `CARGO_BUILD_JOBS=2`, uses lower CPU/I/O scheduling priority, and
requires 20 GiB available at the inferred target, registry, and temporary location.
It checks again after acquiring the shared maintenance lock. Command arguments
are passed literally without a shell; child exit status and signals are preserved.
The wrapper performs no extra builds and uses existing target placement. An
explicit wrapper `--target-dir` sets `CARGO_TARGET_DIR` only for this invocation.
It does not replace engine/game verification, source instructions, shipping gates,
or the existing deployment tool.

The lock coordinates only participating invocations; existing unwrapped builds
continue normally. Cargo can override jobs via explicit child options. This is
priority management and cooperative coordination, not a CPU or memory hard cap.
Project `.cargo/config.toml` and scripts may select additional paths; inspect those
before a large build. Use an explicit target to make the wrapper's preflight exact.
`--min-free-gib 0` is for tiny commands/tests where the normal build reserve does
not apply; it is not the recommended engine build setting.

## Cache review and cleanup

Default inventory discovers marked Cargo `target` directories below home projects
and their `games/*` folders, on the home filesystem. Symlink targets, removable
storage, source-like directories, standalone targets without a Cargo manifest,
and release profiles are excluded from cleanup. Source checkouts and `dist/`
packages are never cleanup inputs.

Inventory is a candidate list, not authorization to delete. Before applying a
candidate, review running processes, installation/shortcut references, required
profiles, and native Cargo dry-run output. Cleaning one whole profile removes
its generated binaries and dependencies as well as incremental data; later
builds will regenerate them. A cache size is not a promise of freed space.

```sh
# Inspect an exact discovered target/profile through Cargo's supported dry run:
dev-cache-manage --cache ~/Feta/target --profile dev
# Only after that exact deletion scope has been approved:
dev-cache-manage --cache ~/Feta/target --profile dev --apply
```

Applying cleanup refuses observed active user Rust builds and processes referencing
the target. Protected session processes can hide `/proc` details: these visibility
gaps are reported and recorded, rather than treated as proof that a cache is busy
or idle. Review those gaps and external launch references before applying a cleanup.
It uses the common coordination lock and native
`cargo clean --frozen` for Cargo's artifact locks and metadata semantics. It never
guesses Cargo lock filenames or uses a recursive delete on an arbitrary path.
External launch references cannot be exhaustively inferred; operator review is
required. Cache snapshots may change while other work runs.

## Git and server health

`dev-worktree-audit` defaults to BlueEngine, BlueEngineGames, and Feta. Add
`--repo PATH` for another repository. It reports dirty/untracked files, local
ahead/behind counts, active working directories, and stale registrations.
It disables optional Git index writes, does not fetch, and never prunes. Missing
removable-storage worktrees are not automatically expendable.

`game-server-health` queries the existing hub with `status GAME --wait 1` and
reads systemd service/timer properties. When a deployment receipt is present, it
asks the hub to confirm that exact build. Add `--game ID` to narrow the query.
It does not create rooms, restart/reload services, renew mappings, or certify
external reachability. Exit 2 means a check failed or could not establish health.

## Installing native diagnostics

```sh
dev-tools-bootstrap --plan
dev-tools-bootstrap --install
```

The allowlist is sysstat (iostat/pidstat/mpstat), strace, ShellCheck,
hyperfine, and rsync. Bootstrap reads candidate versions/checksums from the local
Debian APT metadata and downloads only from Debian HTTPS origins. Payloads are
checked for exact size and SHA256 and for missing shared libraries before linking.
It retains package/version/checksum provenance under `~/.local/share/dev-tools`.
It refuses unmanaged commands in `~/.local/bin`.

Because administrator access is unavailable, payloads are installed per user:
no system dpkg records, privileged package scripts, cron entries, or sysstat
history collection services are installed. `iostat` and `pidstat` work on demand;
`sar` and its system-managed history collection are not exposed. Future security updates to these
user-local packages require an explicit bootstrap installation after APT lists
are updated by normal system maintenance. Old versions are retained until reviewed.

Examples:

```sh
iostat -xz 1 5
pidstat -dru 1 5
shellcheck /path/to/script.sh
hyperfine --runs 3 'python3 tools/be2.py context movement --compact'
```

Utilities and saved reports stay on internal storage. Google Drive authorization
and backup scheduling are not configured. The temporary user-local rclone
installation has been removed.

## Verification

```sh
cd ~/server-stability/dev-tools
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest -v
```

Exit codes: 0 means the command succeeded, 2 means a refusal, unavailable check, or
utility error. Build and native Cargo cleanup propagate their child exit codes;
a child killed by signal returns 128 plus the signal number.
