#!/usr/bin/python3
"""Manage explicitly reviewed Cargo caches and scheduled-build disk admission."""
import argparse
import contextlib
import fcntl
import json
import math
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
import time

import dev_tools as tools
import ideaforge_idle

GIB = 1024 ** 3
PROFILES = {"dev": "debug", "itest": "itest", "fast": "fast"}
DEFAULT_CONFIG = tools.HOME / ".config/dev-tools/storage-policy.json"
MANAGED_NAMESPACE = Path("/srv/storage/cache/cargo")
MANAGED_NAMES = {"ideaforge", "nightly", "default"}


class InsufficientStorage(ValueError):
    """Only capacity pressure permits the reviewed-cleanup admission fallback."""


def exact_path(value):
    path = Path(value).expanduser()
    if not path.is_absolute() or path != path.resolve():
        raise ValueError("Policy paths must be absolute and have no symlink components")
    return path


def overlaps(first, second):
    return tools.inside(str(first), second) or tools.inside(str(second), first)


def load_policy(path):
    path = exact_path(path)
    if path.stat().st_uid != os.getuid():
        raise ValueError("Storage policy must be owned by this user")
    return validate_policy(json.loads(path.read_text()))


def validate_policy(policy):
    if policy.get("version") != 1:
        raise ValueError("Unsupported storage policy version")
    for key in ("reserve_gib", "warning_free_gib", "cache_budget_gib", "idle_days"):
        value = policy[key]
        if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
            raise ValueError(f"Invalid storage threshold: {key}")
    if policy["reserve_gib"] < 8 or policy["warning_free_gib"] < policy["reserve_gib"]:
        raise ValueError("Storage reserve must be at least 8 GiB and below the warning threshold")
    critical = [tools.HOME / p for p in (
        "blueengine", ".config", ".cargo", ".rustup", ".codex", ".ssh",
        ".local/share/blueengine", "be2-hub-data", "Desktop", "Music", "Documents",
    )]
    protected = critical + [exact_path(p) for p in policy.get("protected_paths", [])]
    pinned = [exact_path(p) for p in policy["pinned_caches"]]
    seen = set()
    for entry in policy["retired_caches"]:
        target = exact_path(entry["target"])
        manifest = exact_path(entry["manifest"])
        if target in seen or target != manifest.parent / "target" or manifest.name != "Cargo.toml":
            raise ValueError("Retired caches must name a unique checkout's canonical target directory")
        seen.add(target)
        if manifest.parent.stat().st_dev != tools.HOME.stat().st_dev:
            raise ValueError("Automatic cleaning is limited to internal home storage")
        if any(overlaps(target, p) for p in protected + pinned):
            raise ValueError("Refusing a protected or pinned cache")
        if not entry.get("profiles") or any(p not in PROFILES for p in entry["profiles"]):
            raise ValueError("Only reviewed dev, itest and fast profiles can be retired")
        if len(set(entry["profiles"])) != len(entry["profiles"]):
            raise ValueError("Duplicate retired profile")
        if not isinstance(entry.get("reviewed_commit"), str) or len(entry["reviewed_commit"]) != 40:
            raise ValueError("A full reviewed commit is required")
        if not all(c in "0123456789abcdef" for c in entry["reviewed_commit"]):
            raise ValueError("Invalid reviewed commit")
        if not isinstance(entry.get("base"), str) or not entry["base"] or entry["base"].startswith("-"):
            raise ValueError("A retained upstream/base reference is required")
    for workload, paths in policy["workloads"].items():
        if not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,63}", workload) or not paths:
            raise ValueError("Workloads need exact storage paths")
        for p in paths:
            exact_path(p)
    mounts = set()
    for mount in policy.get("required_mounts", []):
        path = exact_path(mount["mountpoint"])
        if path in mounts:
            raise ValueError("Required mountpoints must be unique")
        mounts.add(path)
        if not re.fullmatch(r"[A-Za-z0-9-]+", mount["uuid"]):
            raise ValueError("Required mounts need an explicit filesystem UUID")
        if not re.fullmatch(r"[A-Za-z0-9_-]+", mount["fstype"]):
            raise ValueError("Required mounts need an explicit filesystem type")
        root = mount.get("fsroot", "/")
        if not isinstance(root, str) or not root.startswith("/") or str(Path(root)) != root or ".." in Path(root).parts:
            raise ValueError("Required mount roots must be absolute normalized paths")
    managed = set()
    for entry in policy.get("managed_external_caches", []):
        target, manifest = exact_path(entry["target"]), exact_path(entry["manifest"])
        if (target.parent != MANAGED_NAMESPACE or target.name not in MANAGED_NAMES
                or target in managed or MANAGED_NAMESPACE.parent.parent not in mounts):
            raise ValueError("Managed caches need unique registered Cargo targets on required external storage")
        managed.add(target)
        if (manifest.name != "Cargo.toml" or not manifest.is_file() or overlaps(target, manifest.parent)
                or any(overlaps(target, p) for p in protected)
                or any(p != target and overlaps(target, p) for p in pinned)):
            raise ValueError("Managed cache conflicts with source or protected paths")
        budget = entry["budget_gib"]
        if isinstance(budget, bool) or not isinstance(budget, (int, float)) or not math.isfinite(budget) or budget <= 0:
            raise ValueError("Managed cache budgets must be positive finite GiB")
        profiles = entry["profiles"]
        if not profiles or any(p not in PROFILES for p in profiles) or len(set(profiles)) != len(profiles):
            raise ValueError("Managed caches permit only registered dev, itest and fast profiles")
    return policy


def verify_required_mounts(policy, extra_paths=()):
    """Reject missing/replaced/read-only storage before any cleanup or build.

    A directory left at a missing mountpoint is not a valid fallback, even when
    its parent filesystem has enough space. Future child directories are checked
    through their nearest existing ancestor without creating them.
    """
    verified = []
    for expected in policy.get("required_mounts", []):
        mountpoint = exact_path(expected["mountpoint"])
        result = tools.probe(["/usr/bin/findmnt", "-J", "-M", str(mountpoint), "-o",
                              "TARGET,FSTYPE,UUID,FSROOT,OPTIONS,MAJ:MIN"], timeout=10)
        try:
            rows = json.loads(result["output"])["filesystems"] if result["ok"] else []
            if len(rows) != 1:
                raise ValueError("Missing or ambiguous mount")
            row = rows[0]
            identity = (str(mountpoint), expected["fstype"], expected["uuid"], expected.get("fsroot", "/"))
            if tuple(row.get(k) for k in ("target", "fstype", "uuid", "fsroot")) != identity:
                raise ValueError("Wrong filesystem identity or bind root")
            if "rw" not in row.get("options", "").split(","):
                raise ValueError("Filesystem is not writable")
            major, minor = (int(n) for n in row["maj:min"].split(":"))
            device = os.makedev(major, minor)
            if mountpoint.stat().st_dev != device:
                raise ValueError("Mount changed during verification")
        except (KeyError, TypeError, ValueError, OSError) as exc:
            raise ValueError(f"Required storage {mountpoint} is unavailable or changed: {exc}") from exc
        verified.append({"mountpoint": str(mountpoint), "uuid": expected["uuid"],
                         "fstype": expected["fstype"], "fsroot": expected.get("fsroot", "/"),
                         "device": device})
    paths = list(policy["pinned_caches"]) + list(extra_paths)
    paths += [entry["target"] for entry in policy.get("managed_external_caches", [])]
    paths += [path for entries in policy["workloads"].values() for path in entries]
    for value in paths:
        original = Path(value).expanduser()
        path = original.resolve()
        matches = [row for row in verified if original.is_relative_to(row["mountpoint"])
                   or path.is_relative_to(row["mountpoint"])]
        if not matches:
            continue
        row = max(matches, key=lambda item: len(Path(item["mountpoint"]).parts))
        while not path.exists():
            path = path.parent
        if path.stat().st_dev != row["device"]:
            raise ValueError(f"Required storage backing {value} differs from {row['mountpoint']}")
    return verified


def git(project, *args):
    result = tools.probe(["git", "-C", str(project), *args], timeout=20)
    if not result["ok"]:
        raise ValueError("Cannot establish retired checkout state with Git")
    return result["output"]


def eligible(entry):
    """A reviewed cache stops being expendable as soon as its checkout resumes."""
    target = exact_path(entry["target"])
    manifest = exact_path(entry["manifest"])
    if not manifest.is_file() or (target / ".git").exists() or (target / "Cargo.toml").exists():
        raise ValueError("Retired cache has missing provenance or resembles source")
    marker = target / "CACHEDIR.TAG"
    if not (target / ".rustc_info.json").is_file() and not (
        marker.is_file() and not marker.is_symlink() and
        marker.read_text().splitlines()[:1] == ["Signature: 8a477f597d28d172789f06886806bc55"]
    ):
        raise ValueError("Retired target lacks Cargo cache provenance")
    if git(manifest.parent, "rev-parse", "HEAD") != entry["reviewed_commit"]:
        raise ValueError("Retired checkout changed since review; preserve its cache")
    if git(manifest.parent, "status", "--porcelain=v1", "--untracked-files=all"):
        raise ValueError("Retired checkout has unfinished work; preserve its cache")
    git(manifest.parent, "merge-base", "--is-ancestor", entry["reviewed_commit"], entry["base"])
    busy, uncertain = tools.cache_references(target)
    rows, _ = tools.user_processes()
    if busy or any(tools.inside(row["cwd"], manifest.parent) for row in rows):
        raise ValueError("Retired cache or checkout is in use")
    return uncertain


def latest_write(path):
    newest = path.stat().st_mtime
    for directory, subdirs, files in os.walk(path, followlinks=False):
        subdirs[:] = [d for d in subdirs if not (Path(directory) / d).is_symlink()]
        for name in files:
            p = Path(directory) / name
            if not p.is_symlink():
                newest = max(newest, p.stat().st_mtime)
    return newest


def measure(paths):
    # Old cache aliases and their canonical relocated paths are one allocation.
    paths = sorted(set(str(Path(p).resolve()) for p in paths if Path(p).is_dir()))
    if not paths:
        return {}
    result = tools.probe(["du", "-sx", "-B1", *paths], timeout=60)
    if not result["ok"]:
        raise ValueError("Cache measurement was incomplete; preserve caches")
    return {p: int(size) for size, p in (line.split("\t", 1) for line in result["output"].splitlines())}


def inventory(policy):
    discovered = tools.discover_caches()
    paths = [c["target"] for c in discovered] + policy["pinned_caches"]
    paths += [c["target"] for c in policy["retired_caches"]]
    managed = {c["target"]: c for c in policy.get("managed_external_caches", [])}
    paths += list(managed)
    sizes = measure(paths)
    retired = {c["target"]: c for c in policy["retired_caches"]}
    pinned = set(policy["pinned_caches"])
    device = tools.HOME.stat().st_dev
    total = sum(size for p, size in sizes.items() if Path(p).stat().st_dev == device)
    free = tools.space(tools.HOME)
    warnings = []
    if free["available_gib"] < policy["warning_free_gib"]:
        warnings.append(f"Internal storage has {free['available_gib']} GiB free; warning threshold is {policy['warning_free_gib']} GiB")
    if total > policy["cache_budget_gib"] * GIB:
        warnings.append("Internal Cargo cache budget exceeded; review unregistered completed-task caches")
    for target, entry in managed.items():
        if sizes.get(target, 0) > entry["budget_gib"] * GIB:
            warnings.append(f"Managed cache {target} exceeds its {entry['budget_gib']} GiB development-cache budget")
    external = []
    for p in policy.get("monitor_paths", []):
        if Path(p).exists():
            status = tools.space(p)
            external.append(status)
            if status["available_gib"] < 10:
                warnings.append(f"Monitored storage {p} has only {status['available_gib']} GiB free")
    return {"time": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "storage": free,
            "internal_cache_gib": round(total / GIB, 3), "cache_budget_gib": policy["cache_budget_gib"],
            "reserve_gib": policy["reserve_gib"], "monitored_storage": external,
            "caches": [{"target": p, "allocated_gib": round(size / GIB, 3),
                        "policy": "managed-external" if p in managed else "pinned" if p in pinned else "reviewed-retired" if p in retired else "review-required"}
                       for p, size in sorted(sizes.items(), key=lambda item: item[1], reverse=True)],
            "warnings": warnings, "actions": [], "source_and_server_deletion": False}


def sweep(policy, report):
    verify_required_mounts(policy)
    pressure = report["storage"]["available_gib"] < policy["warning_free_gib"] or report["internal_cache_gib"] > policy["cache_budget_gib"]
    for entry in policy["retired_caches"]:
        for profile in entry["profiles"]:
            directory = Path(entry["target"]) / PROFILES[profile]
            if not directory.exists():
                continue
            action = {"target": entry["target"], "profile": profile}
            try:
                exact_path(directory)
                if (directory / "Cargo.toml").exists() or (directory / ".git").exists():
                    raise ValueError("Profile resembles source")
                uncertain = eligible(entry)
                age = (time.time() - latest_write(directory)) / 86400
                if not pressure and age < policy["idle_days"]:
                    action.update(status="retained", reason="Within the retention window")
                else:
                    args = argparse.Namespace(plan=False, save=False, cache=entry["target"], profile=profile, apply=False)
                    if tools.cache_command(args) != 0:
                        raise ValueError("Native Cargo cleanup preview failed")
                    # Preview or concurrent work cannot authorize a changed checkout.
                    eligible(entry)
                    verify_required_mounts(policy)
                    args.apply = True
                    status = tools.cache_command(args)
                    action.update(status="cleaned" if status == 0 else "failed", exit_code=status,
                                  uninspectable_user_processes=uncertain)
            except (ValueError, OSError, subprocess.SubprocessError) as exc:
                action.update(status="retained", reason=str(exc))
            report["actions"].append(action)
    report["storage_after"] = tools.space(tools.HOME)


def process_gone_or_zombie(pid):
    try:
        status = (Path("/proc") / str(pid) / "status").read_text()
    except (FileNotFoundError, ProcessLookupError):
        return True
    except OSError:
        return False
    return any(line.startswith("State:") and line.split()[1:2] == ["Z"] for line in status.splitlines())


def managed_cache_idle(policy, entry):
    """Check complete scheduled workflows, including gaps between Cargo stages."""
    verify_required_mounts(policy, [entry["target"]])
    target = exact_path(entry["target"])
    for unit in ("ideaforge-daily.service", "blueengine-nightly.service"):
        result = tools.probe(["systemctl", "--user", "show", unit, "--no-pager",
                              "--property=LoadState", "--property=ActiveState"], timeout=10)
        values = dict(line.split("=", 1) for line in result.get("output", "").splitlines() if "=" in line)
        if not result["ok"] or values.get("LoadState") != "loaded" or values.get("ActiveState") not in ("inactive", "failed"):
            raise ValueError("Scheduled development is running or its idle state is unverified")
    config = tools.HOME / ".config/ideaforge/daily.json"
    if ideaforge_idle.idle_condition(config, tools.HOME) != 0:
        raise ValueError("An IdeaForge supervisor is still active")
    busy, uncertain = tools.cache_references(target)
    if busy:
        raise ValueError("A Rust build or managed-cache consumer is active")
    controllers = {"systemd", "(sd-pam)", "sshd", "sshd-session"}
    for pid in uncertain:
        if process_gone_or_zombie(pid):
            continue
        try:
            if (Path("/proc") / str(pid) / "comm").read_text().strip() not in controllers:
                raise ValueError("Managed-cache process visibility is incomplete")
        except FileNotFoundError:
            pass
    rows, _ = tools.user_processes()
    for row in rows:
        if process_gone_or_zombie(row["pid"]):
            continue
        try:
            maps = (Path("/proc") / str(row["pid"]) / "maps").read_text()
        except FileNotFoundError:
            continue
        except PermissionError:
            if row["name"] not in controllers:
                raise ValueError("Managed-cache mapped-library visibility is incomplete") from None
            continue
        for line in maps.splitlines():
            fields = line.split(None, 5)
            if len(fields) == 6 and fields[5].startswith("/") and tools.inside(fields[5], target):
                raise ValueError("A process has a mapped managed-cache artifact")


@contextlib.contextmanager
def nightly_idle_lock():
    """Hold the runner's existing lock without creating or changing its file."""
    path = tools.HOME / ".local/state/blueengine-nightly/run.lock"
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    with os.fdopen(fd, "r") as stream:
        metadata = os.fstat(stream.fileno())
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid():
            raise ValueError("Nightly coordination lock requires inspection")
        try:
            fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ValueError("A nightly supervisor is still active") from None
        yield


def managed_profile(policy, entry, profile, config=None):
    if config is not None and load_policy(config) != policy:
        raise ValueError("Managed-cache registration changed; retain its artifacts")
    target, manifest = exact_path(entry["target"]), exact_path(entry["manifest"])
    if target.parent != MANAGED_NAMESPACE or target.name not in MANAGED_NAMES:
        raise ValueError("Managed target is outside its registered namespace")
    if target.stat().st_uid != os.getuid() or manifest.name != "Cargo.toml" or not manifest.is_file():
        raise ValueError("Managed cache ownership or manifest changed")
    marker = target / "CACHEDIR.TAG"
    provenance = target / ".rustc_info.json"
    if not (provenance.is_file() and not provenance.is_symlink()) and not (
            marker.is_file() and not marker.is_symlink() and
            marker.read_text().splitlines()[:1] == ["Signature: 8a477f597d28d172789f06886806bc55"]):
        raise ValueError("Managed target lacks Cargo cache provenance")
    directory = exact_path(target / PROFILES[profile])
    if not directory.is_dir():
        raise ValueError("Managed profile is not a directory")
    if any((path / name).exists() for path in (target, directory) for name in ("Cargo.toml", ".git", "src")):
        raise ValueError("Managed cache or profile resembles source")
    managed_cache_idle(policy, entry)
    return directory


def managed_clean(entry, profile, preview):
    command = [tools.executable("cargo") or "cargo", "clean", "--frozen",
               "--manifest-path", entry["manifest"], "--target-dir", entry["target"],
               "--profile", profile, "--config", "build.build-dir=" + json.dumps(entry["target"])]
    if preview:
        command.append("--dry-run")
    env = {**os.environ, "CARGO_NET_OFFLINE": "true", "CARGO_TARGET_DIR": entry["target"],
           "CARGO_BUILD_TARGET_DIR": entry["target"], "CARGO_BUILD_BUILD_DIR": entry["target"]}
    return subprocess.run(command, env=env, timeout=60,
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode


def rotate_managed_caches(policy, report, config=None):
    """Daily-only rotation of explicitly registered development profiles."""
    for entry in policy.get("managed_external_caches", []):
        action = {"target": entry["target"], "kind": "managed-external",
                  "budget_gib": entry["budget_gib"], "profiles": []}
        try:
            verify_required_mounts(policy, [entry["target"]])
            target = exact_path(entry["target"])
            if not target.is_dir():
                continue
            with tools.coordination_lock(), nightly_idle_lock():
                if config is not None and load_policy(config) != policy:
                    raise ValueError("Managed-cache registration changed; retain its artifacts")
                managed_cache_idle(policy, entry)
                size = measure([target])[str(target)]
                action["allocated_before_gib"] = round(size / GIB, 3)
                if size <= entry["budget_gib"] * GIB:
                    continue
                for profile in entry["profiles"]:
                    if not (target / PROFILES[profile]).exists():
                        continue
                    managed_profile(policy, entry, profile, config)
                    if managed_clean(entry, profile, True):
                        raise ValueError("Managed Cargo cleanup preview failed")
                    managed_profile(policy, entry, profile, config)
                    action.setdefault("attempted_profiles", []).append(profile)
                    if managed_clean(entry, profile, False):
                        raise ValueError("Managed Cargo cleanup failed")
                    action["profiles"].append(profile)
                    size = measure([target])[str(target)]
                    if size <= entry["budget_gib"] * GIB:
                        break
                action.update(status="cleaned" if action["profiles"] else "retained",
                              allocated_after_gib=round(size / GIB, 3))
                if size > entry["budget_gib"] * GIB:
                    action["reason"] = "Preserved or unregistered profiles still exceed the cache budget"
        except (ValueError, OSError, KeyError, subprocess.SubprocessError) as exc:
            action.update(status="failed" if action.get("attempted_profiles") else "retained",
                          reason=str(exc) if isinstance(exc, ValueError) else type(exc).__name__)
        report["actions"].append(action)


def admit(policy, workload):
    if workload not in policy["workloads"]:
        raise ValueError("Unknown scheduled workload")
    verify_required_mounts(policy)
    storage = [tools.space(p) for p in policy["workloads"][workload]]
    low = [row for row in storage if row["available_gib"] < policy["reserve_gib"]]
    if low:
        raise InsufficientStorage(f"{workload} retained: need {policy['reserve_gib']} GiB free on every build filesystem; lowest is {min(r['available_gib'] for r in low)} GiB")
    return storage


def save_policy(path, policy):
    path = exact_path(path)
    fd, temporary = tempfile.mkstemp(prefix=".storage-policy-", dir=path.parent)
    try:
        with os.fdopen(fd, "w") as stream:
            json.dump(policy, stream, indent=2)
            stream.write("\n")
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def retire(policy, config, target, profiles):
    verify_required_mounts(policy)
    target = exact_path(target)
    entry = {"target": str(target), "manifest": str(target.parent / "Cargo.toml"),
             "profiles": profiles, "reviewed_commit": git(target.parent, "rev-parse", "HEAD"), "base": "origin/main"}
    # Explicit registration is the approval boundary; discovery never registers.
    updated = dict(policy)
    updated["retired_caches"] = [r for r in policy["retired_caches"] if r["target"] != str(target)] + [entry]
    validate_policy(updated)
    if target not in {Path(c["target"]) for c in tools.discover_caches()}:
        raise ValueError("Only discovered internal Cargo targets can be registered")
    eligible(entry)
    for profile in profiles:
        if tools.cache_command(argparse.Namespace(plan=False, save=False, cache=str(target), profile=profile, apply=False)) != 0:
            raise ValueError("Native Cargo preview failed; retirement was not registered")
    with tools.coordination_lock():
        if load_policy(config) != policy:
            raise ValueError("Storage policy changed during review; registration was not saved")
        eligible(entry)
        verify_required_mounts(policy)
        save_policy(config, updated)
    return entry


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", default=str(DEFAULT_CONFIG))
    commands = parser.add_subparsers(dest="operation", required=True)
    p = commands.add_parser("plan", help="Read-only cache inventory and capacity warnings")
    p.add_argument("--save", action="store_true")
    commands.add_parser("sweep", help="Apply only previously reviewed retirement scopes")
    p = commands.add_parser("admit", help="Check scheduled-build reserve; review-cleanup fallback only when capacity is low")
    p.add_argument("--workload", required=True)
    p = commands.add_parser("retire", help="Review and register one completed checkout's cache profiles")
    p.add_argument("--cache", required=True)
    p.add_argument("--profile", choices=sorted(PROFILES), action="append", required=True)
    args = parser.parse_args(argv)
    try:
        policy = load_policy(args.config)
        if args.operation == "admit" and args.workload not in policy["workloads"]:
            raise ValueError("Unknown scheduled workload")
        mounts = verify_required_mounts(policy)
        if args.operation == "retire":
            tools.emit({"registered_retirement": retire(policy, args.config, args.cache, args.profile)})
            return 0
        if args.operation == "admit":
            try:
                capacity = admit(policy, args.workload)
            except InsufficientStorage:
                # Only low capacity justifies the expensive inventory and the
                # existing narrowly reviewed cleanup/recheck path below.
                pass
            else:
                report = {"time": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
                          "admitted": True, "admitted_storage": capacity,
                          "reserve_gib": policy["reserve_gib"], "required_mounts": mounts,
                          "actions": [], "source_and_server_deletion": False}
                tools.report_save("storage-admission-" + args.workload, report)
                tools.report_save("storage-admit", report)
                tools.emit(report)
                return 0
        report = inventory(policy)
        report["required_mounts"] = mounts
        if args.operation != "plan":
            sweep(policy, report)
        if args.operation == "sweep":
            rotate_managed_caches(policy, report, args.config)
        if args.operation == "admit":
            try:
                report["admitted_storage"] = admit(policy, args.workload)
                report["admitted"] = True
                tools.report_save("storage-admission-" + args.workload, report)
            except ValueError as exc:
                report.update(admitted=False, reason=str(exc))
                tools.report_save("storage-admission-" + args.workload, report)
                tools.emit(report)
                return 2
        if args.operation != "plan" or args.save:
            tools.report_save("storage-" + args.operation, report)
        tools.emit(report)
        return 0
    except (ValueError, OSError, KeyError, TypeError, subprocess.SubprocessError) as exc:
        print(f"dev-storage-manage: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
