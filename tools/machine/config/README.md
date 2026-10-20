# Scheduled development configuration examples

These examples accompany the optional Linux utilities in `tools/machine`.
They are reference configurations, not an installer. Publishing them changes no
host services. Keep each host's actual configuration and reports outside Git.

## Paths and storage review

The systemd units expect the maintained utility checkout at `~/BlueEngine`, a
separate clean IdeaForge runtime checkout at `~/BlueEngine-idea-forge-native`,
and the game catalog at `~/BlueEngineGames`. `%h` in unit files expands to the
service user's home. JSON files do not expand `%h` or shell variables: replace
every `/home/YOUR_USER` with the actual absolute home path and replace
`YOUR_ORG` with the GitHub repository owner. The engine's configured origin must
exactly match `https://github.com/YOUR_ORG/BlueEngine.git`.

The reviewed external-storage layout is `/srv/storage`, with Cargo targets
`cache/cargo/{ideaforge,nightly,default}`, private build runs under `artifacts`,
and temporary directories under `scratch`. Managed cache rotation deliberately
accepts only these three target names in this namespace. Changing the mount
layout requires corresponding code and safety-test changes.

Before creating directories or moving caches, inspect the mounted filesystem:

```sh
findmnt -J -M /srv/storage -o TARGET,FSTYPE,UUID,FSROOT,OPTIONS,MAJ:MIN
```

Replace `REPLACE-WITH-REVIEWED-FILESYSTEM-UUID`, filesystem type and `fsroot` in
`storage-policy.json.example` with the exact reviewed identity. The example UUID
intentionally does not match a real drive. Mount admission refuses a missing,
substituted, read-only, or incorrectly rooted filesystem; a directory left on
the main drive is not an acceptable fallback. Only after confirming that mount,
create user-owned target/run/scratch directories with private permissions.

Review every managed target, manifest, profile and budget before installing the
policy. This example authorizes rotation of `dev`, `itest`, and `fast` profiles
in the three listed caches when they exceed their individual budgets and are
idle. Remove any cache not approved for that host. Release profiles, source,
installed games and run evidence are retained. `retired_caches` starts empty;
register an internal cache only through a separate exact-target retirement
review. Pinned external targets are excluded from generic internal retirement;
their explicit managed-cache entries govern budget rotation.

For an existing host, stop new scheduled starts and let active builds finish
before changing target placement. Preserve retained run journals and published
outputs. Copy reviewed caches to the mounted destination, verify complete
content and important release/package hashes, update retained target references,
then remove only verified obsolete compiler artifacts. Do not treat these
examples as authorization to delete a discovered checkout or worktree.

## Install reviewed configuration

Prepare copies in a staging directory and make the replacements above first.
Then install only the reviewed files, preserving existing host-specific values:

| Example | Private destination |
| --- | --- |
| `storage-policy.json.example` | `~/.config/dev-tools/storage-policy.json` |
| `ideaforge-daily.json.example` | `~/.config/ideaforge/daily.json` |
| `nightly/config.json.example` | `~/.config/blueengine-nightly/config.json` |
| `nightly/prompt.txt` | `~/.config/blueengine-nightly/prompt.txt` |
| `nightly/result.schema.json` | `~/.config/blueengine-nightly/result.schema.json` |

Use mode 0700 for private config/state/run directories and mode 0600 for config
files. Keep nightly's existing `run.lock`, `feedback-seen.json`, `latest.md`, and
`last-success.json` in its configured internal `state` directory when moving
only `runs_root`. The collector excludes both legacy and current private nightly
run roots and the configured `worktrees_root`. Use a short external worktree root
such as `/srv/storage/artifacts/bn`: the engine's bounded authoring-context tests
include absolute checkout paths, so deeply nested worktrees can exceed their
reading budget. The runner creates a unique private worktree directory there;
reports and feedback snapshots remain under `runs_root`. Failed worktrees remain
available for review. Failed or blocked feedback remains eligible for retry. Updated and
actionable no-change results acknowledge feedback only after the exact main
push's `.github/workflows/ci.yml` succeeds.

Copy the reviewed units and their drop-in directories from `../systemd/` into
`~/.config/systemd/user/`. Merge existing drop-ins deliberately; old custom path
guards may require separate review. The examples set external Cargo and scratch
paths explicitly, require storage admission before starting, defer while a
supervisor or storage maintenance is active, and fetch/fast-forward only a clean
IdeaForge runtime. They do not reset or stash local changes.

For a nightly runner version 3 upgrade, deploy `blueengine-nightly/run.py`,
`blueengine-nightly/VERSION`, `nightly/prompt.txt`, and `nightly/result.schema.json`
together while its existing service is idle. Preserve host paths, timeout values,
`run.lock`, `feedback-seen.json`, `feedback-queue.json`, and all retained reports.
The new output schema requires one disposition per selected digest topic; an old
schema cannot express these receipts and will safely fail before publication.
`max_feedback_items` is optional (default 24); it bounds daily triage without
discarding the rest. No timer changes are required. Run the upgraded runner's
`--check` and `--collect-only`, then inspect its private `feedback-digest.json`.
Collection never acknowledges, builds or publishes.

The schedules use `America/Los_Angeles`: storage review at 22:40, nightly engine
feedback maintenance at 23:00, and an hourly IdeaForge wakeup at minute 10 to
finish one 2D and one 3D game per day. Update the timers and daily JSON together
if changing timezone. `Persistent=true` catches missed wakeups after downtime.
Nightly has an eight-hour service budget, including four hours for the agent,
two hours for checks, and up to 90 minutes waiting for hosted CI.

Before enabling timers, review the cleanup policy and run admission and prerequisite checks:

Create the private internal state directories before enabling services: the
maintenance lock's parent must exist before its `ExecCondition` can run.

```sh
install -d -m0700 ~/.local/state/dev-tools ~/.local/state/blueengine-nightly
python3 ~/BlueEngine/tools/machine/storage_policy.py plan
python3 ~/BlueEngine/tools/machine/storage_policy.py admit --workload ideaforge
python3 ~/BlueEngine/tools/machine/storage_policy.py admit --workload nightly
python3 ~/BlueEngine/tools/machine/blueengine-nightly/run.py --check
systemd-analyze --user verify ~/.config/systemd/user/ideaforge-daily.service ~/.config/systemd/user/blueengine-nightly.service ~/.config/systemd/user/dev-storage-maintenance.service
systemctl --user daemon-reload
systemctl --user enable --now ideaforge-daily.timer blueengine-nightly.timer dev-storage-maintenance.timer
systemctl --user list-timers ideaforge-daily.timer blueengine-nightly.timer dev-storage-maintenance.timer
```

`admit` normally checks mount identity and reserve without scanning caches. If
capacity is low, it may apply already reviewed internal retirement scopes before
rechecking; review that policy before invoking it. `run.py --check` verifies
existing Codex authentication, GitHub permission, and the configured main branch.
It performs no maintenance or push. The nightly runner uses the configured Codex
session in an isolated worktree with unattended permissions; enable its timer
only after reviewing the static prompt and intended repository access. Do not
use `--baseline` to troubleshoot failures: it deliberately acknowledges the
current feedback set without implementing it.

The user manager must continue running while logged out for unattended timers.
Check `loginctl show-user "$USER" -p Linger`; configure lingering through the
host's normal administration if needed. Local failures appear in the unit
journal, nightly `latest.md`, and private dev-tools status reports. Inspect
`systemctl --user status UNIT` and `journalctl --user -u UNIT` when a run fails.

## Manual build defaults

The optional `environment.d` examples set defaults for future user-manager
processes. The shell snippets may be sourced from `.profile` and `.bashrc` after
copying them to `~/.config/shell/`; they preserve explicit per-task overrides.
They set paths and do not mount storage. Existing processes keep their original
environment. Check the required mount before starting manual builds, and use
`dev-build-run` for cooperative maintenance locking and reserve checks.

Merge the single `target-dir` key from `cargo-config.toml.example` into the
existing `~/.cargo/config.toml` `[build]` table. Preserve compiler wrappers,
linker settings and unrelated configuration. Project configuration, explicit
`--target-dir`, and `CARGO_TARGET_DIR` may override the Cargo default; inspect
those when diagnosing a build's actual storage location.
