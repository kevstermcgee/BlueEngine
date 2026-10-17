# Development machine utilities

Version 1.8.1. Debian/Linux, Python standard library, native Git/Cargo/systemd tools.
Source lives here; six commands in `~/.local/bin` point to `dev_tools.py`;
`dev-server-load` points to `server_load.py`, and `dev-server-isolation-test`
points to `server_isolation_test.py`. `dev-storage-manage` points to
`storage_policy.py` for reviewed cache retirement and disk admission.

These optional tools target Debian/Linux server hosts. They do not change BlueEngine's
game distribution policy. Link the desired scripts into the user's `~/.local/bin`;
the scripts use that user's home and existing project/configuration paths. Keep
these Python files together so the storage, benchmark, and isolation utilities can
import their shared modules. The engine's full Linux checker runs their scratch
suites; the Windows checker skips this Linux-only utility gate.

The optional feedback-maintenance runner and its integration tests are in
[`blueengine-nightly/`](blueengine-nightly/). Reviewed deployment examples are in
[`config/`](config/) and [`systemd/`](systemd/). They describe host configuration;
installing files and enabling schedules are separate operator actions. The unit
examples use the source checkout at `%h/BlueEngine/tools/machine` and a separate
IdeaForge runtime at `%h/BlueEngine-idea-forge-native`, where systemd expands `%h`
to the user's home. Keep this source checkout available after installation. Inspect
and adapt all absolute paths before installing on another host. Private policies,
feedback acknowledgments, recovery state, logs, and reports stay outside this
repository.

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
are saved. The storage policy uses a daily systemd user timer; the other utilities
remain on-demand. No new daemon is needed.

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
When a storage policy exists, it also verifies its required mounts before both
capacity checks, including the backing device of any build paths on those mounts.
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

## Long-term storage policy

```sh
dev-storage-manage plan --save
dev-storage-manage sweep
dev-storage-manage admit --workload ideaforge
```

The private policy is `~/.config/dev-tools/storage-policy.json`. It sets a 30 GiB
scheduled-build admission threshold, a 40 GiB warning threshold, an 80 GiB internal
Cargo-cache **review budget**, and 14 days' normal retention for reviewed retired
profiles. The budget is a warning, not a filesystem quota: pinned or unreviewed
caches are never discarded to meet it. Admission checks the exact configured
build filesystems, not an unrelated temporary filesystem. It is a starting
headroom check, not a reservation or guarantee of remaining space throughout a job.
Healthy admission checks only required mount identity and configured workload
capacity, then saves a concise successful report without measuring all caches.
These successful admission reports omit cache inventory/budget totals. Low
capacity retains the full inventory, previously reviewed cleanup, and reserve
recheck; missing/wrong storage never enters that cleanup fallback. The daily
maintenance sweep still performs its full inventory and reviewed retention work.

An optional `required_mounts` list binds relocated storage to its filesystem
identity. For example:

```json
{
  "required_mounts": [
    {"mountpoint": "/srv/storage", "uuid": "expected-filesystem-uuid",
     "fstype": "ext4", "fsroot": "/"}
  ]
}
```

Use the actual UUID from the reviewed storage configuration. Before inventory,
admission, retirement registration, or cleanup, the tool requires each exact mount
to have that UUID, filesystem type, bind root, and read/write mode. It also checks
that pinned/workload paths below it use that mount's device; a missing HDD must
never turn into writes to a fallback directory on the SSD. Future child paths
are checked through an existing ancestor without creating them. Legacy monitored
paths retain their separate compatibility-bind checks. The build wrapper repeats
this verification after acquiring its lock. These are preflight checks, not a
mechanism that prevents unmounting storage during a running job.

Policy paths must remain absolute and canonical. When relocating a cache and
leaving its old path as a compatibility symlink, record the destination in
`pinned_caches` and `workloads`. Inventory resolves aliases before measurement so
the original path and destination cannot count the same cache twice. Keep the
30 GiB SSD reserve for Cargo registry and other writes that remain internal.

Optional `managed_external_caches` entries enable daily rotation of exact,
reviewed external compiler caches. Only canonical
`/srv/storage/cache/cargo/{ideaforge,nightly,default}` targets are supported, and
`/srv/storage` must be registered in `required_mounts`. A reviewed entry is:

```json
{"target": "/srv/storage/cache/cargo/ideaforge",
 "manifest": "/home/YOUR_USER/BlueEngine-idea-forge-native/Cargo.toml",
 "budget_gib": 80, "profiles": ["dev", "itest", "fast"]}
```

Suggested starting review budgets are 80 GiB for IdeaForge and 40 GiB each for
nightly/default. Registration explicitly permits only those development profiles,
including generated binaries; whole-target pins still prevent retired-checkout
cleanup, while this separate reviewed scope permits rotation. Release profiles,
source, deployed artifacts, and run evidence are never rotation inputs. No
discovered or unregistered cache is added automatically.

Only the daily `sweep` rotates managed caches; neither healthy nor low-capacity
`admit` does so. Over-budget caches are remeasured under the maintenance lock.
Rotation requires Cargo provenance, safe profile directories, the expected
storage mount, inactive scheduled services, no live IdeaForge supervisor, and
exclusive access to nightly's existing runner lock. Both proposed
`35-build-maintenance.conf` drop-ins must be installed with this policy: their
shared/nonblocking lock conditions and the manager's exclusive lock close the
scheduled-start race. Conditions run after units become activating, which also
causes cleanup to defer. Direct unwrapped manual launches are outside this
protocol; observed Rust builds, executable/open-file/mapped-library consumers,
and unknown process-visibility gaps defer cleanup. Known systemd/login session
controllers retain the existing process-visibility exception.
Confirmed zombie processes and vanished PIDs cannot hold cache references and
are ignored; an uninspectable live Codex process still defers managed rotation.

Native Cargo preview and cleanup use an exact target/manifest/profile, offline
mode, a target-local intermediate build directory, and bounded runtime. Mounts,
registration, supervisors, and cache consumers are rechecked before apply.
Rotation stops once the target falls below its budget. These are development
cache thresholds: active caches and preserved/unregistered profiles may remain
over budget, recorded in the private sweep report for review. No broader cleanup
occurs to force the total under budget.

Only exact target/profile pairs recorded under `retired_caches` may be cleaned
automatically. Discovery never adds retirement entries. Pinned IdeaForge, nightly,
active game and shared build caches remain intact. Source, release profiles,
deployed binaries, server data/configuration, agent state and removable storage
are excluded from automatic deletion. Failed-run worktrees and evidence are
preserved. No worktree removal is automated.

Before each retirement, the tool verifies the reviewed commit is still HEAD, the
checkout is clean, and that commit is included in the recorded base. It checks
active checkout/cache references, previews with native Cargo, rechecks the checkout,
then cleans only the reviewed profile using the existing maintenance lock and
Cargo locks. Recent retired caches are kept under normal conditions; capacity
pressure may retire them early. Resuming a reviewed checkout invalidates its
retirement automatically. Limited process visibility is reported, following the
existing cache tool's native-lock policy; external launch references still require
review before registration.

To retire a newly completed task, first review its exact profiles, installation
references, and Cargo preview; then register that scope explicitly:

```sh
dev-cache-manage --cache /absolute/completed-checkout/target --profile dev
dev-storage-manage retire --cache /absolute/completed-checkout/target --profile dev
```

Registration requires a clean checkout whose HEAD is included in `origin/main`;
it records that exact commit and previews without deleting. Pinned scopes must be
explicitly unpinned in the policy before retirement can be registered. Until then,
newly discovered caches remain `review-required` in the inventory. Retention is
based on artifact writes; active jobs are checked separately, and this is not a
complete measurement of read-only cache use.

`dev-storage-maintenance.timer` runs the sweep daily at 22:40 Pacific. Storage
drop-ins on IdeaForge and nightly maintenance run admission before their existing
commands. Insufficient space leaves the task unstarted; IdeaForge's existing
hourly timer retains its retry behavior. The nightly timer retains its existing
cadence. These checks neither serialize jobs nor alter engine verification gates.
Private latest reports are `storage-plan.json`, `storage-sweep.json`, and
`storage-admit.json`; admission also writes a workload-specific report.
Capacity warnings appear in the reports and service journal. They do not send
email, chat messages, or external notifications. The installed source/config
paths must be preserved because the commands and services reference them.

### Local status for scheduled attempts

`scheduled_status.py blueengine-nightly` and `scheduled_status.py ideaforge-daily`
record a failed service attempt even when an `ExecStartPre` check prevented the
runner from starting. Proposed user-unit files are under `systemd/`: the
`dev-scheduled-failure@.service` template and a `40-local-failure-status.conf`
drop-in for each scheduled service. Installing those files and reloading the user
manager is a separate activation step; the reporter itself starts no services.
IdeaForge's drop-in also uses `OnSuccess` to replace a previous failure with a
completed-attempt status. Completion does not assert that a new game was created;
publication details remain in the linked batch journals. Nightly success reports
remain owned by the nightly runner.

The callback reads only selected `systemctl show` properties and the local storage
admission report. A saved admission refusal is described as the cause only when
its timestamp is within ten minutes of the service failure. Reports contain links
to the service journal, run logs, and last successful result or publication
history; they do not copy raw logs, command arguments, or environment values.
No external messages are sent and feedback acknowledgments/game state are untouched.

Latest status goes to `~/.local/state/blueengine-nightly/latest.md` or
`~/.local/state/ideaforge/latest.md`. Previous local reports and failure history
are retained privately in each directory's `scheduled-status/`; existing runner
history and `last-success.json` remain intact. Callbacks report current terminal
state and do nothing while a service is running; successful nightly callbacks
also leave its report intact. IdeaForge links use configured `runs_root` and
`daily_root` paths while retaining links to original run history. An ordinary
skipped `ExecCondition` does not trigger systemd's `OnFailure` callback.
Nightly links likewise read configured `state`/`runs_root`, keeping last success
and retained run-history links in state while linking future external runs.
Missing or malformed configuration falls back to the original local state paths.
Completion reporting also checks that the main process exited during the current
invocation, so an `OnSuccess` callback after a condition skip cannot claim completion.
The normal `Result=exec-condition` skip leaves existing reports untouched;
genuine condition errors with `Result=exit-code` still receive failure reports.

The proposed IdeaForge `30-idle-recovery.conf` adds a read-only `ExecCondition`
using `ideaforge_idle.py`. It reads the configured daily supervisor lock: a live
PID returns 1 (normal condition skip), a missing/stale lock returns 0 so the normal
runner handles it, and a malformed/unsafe lock returns 255 (service failure).
The probe never creates, removes, acquires, or rewrites locks and never waits.
The runner's own lock remains authoritative if another supervisor starts after
the probe. Nightly scheduling is unaffected.

The proposed IdeaForge `zz-refresh-runtime.conf` runs after existing storage
preflights and before the supervisor entrypoint. It requires clean staged and
unstaged tracked files, fetches trusted `origin/main` with interactive Git prompts
disabled and a bounded network timeout, then fast-forwards only the dedicated
`~/BlueEngine-idea-forge-native` runtime checkout. This lets future engine
fixes update the supervisor itself, in addition to the generated game's source.
Dirty/diverged checkouts and untracked overlaps fail visibly. No stash, force,
reset, branch switch, or update to other checkouts occurs. The fetch receives
180 seconds plus a 10-second forced-stop grace period. Installing this drop-in
is separate from creating it; no source synchronization occurs during tests.

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
cd ~/BlueEngine/tools/machine
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest -v
```

Exit codes: 0 means the command succeeded, 2 means a refusal, unavailable check, or
utility error. Build and native Cargo cleanup propagate their child exit codes;
a child killed by signal returns 128 plus the signal number.
