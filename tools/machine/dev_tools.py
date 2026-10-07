#!/usr/bin/python3
"""Small Debian workstation utilities. No third-party Python dependencies."""
import argparse
import configparser
import contextlib
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
import urllib.request

VERSION = "1.1.1"
HOME = Path.home()
STATE = HOME / ".local/state/dev-tools"
BIN = HOME / ".local/bin"
PACKAGES = {
    "sysstat": ["iostat", "pidstat", "mpstat"],
    "strace": ["strace"],
    "shellcheck": ["shellcheck"],
    "hyperfine": ["hyperfine"],
    "rsync": ["rsync"],
}
ALIASES = {
    "dev-env-doctor": "doctor", "dev-cache-manage": "cache",
    "dev-build-run": "build", "dev-worktree-audit": "worktrees",
    "game-server-health": "server", "dev-tools-bootstrap": "bootstrap",
}


def private_dir(path):
    if path.is_symlink():
        raise ValueError(f"Refusing symlink directory: {path}")
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    if path.stat().st_uid != os.getuid():
        raise ValueError(f"Directory is not owned by this user: {path}")
    path.chmod(0o700)


def probe(command, cwd=None, timeout=8):
    """Bounded diagnostic commands; callers must not pass credential arguments."""
    try:
        result = subprocess.run(command, cwd=cwd, text=True, capture_output=True,
                                timeout=timeout, env={**os.environ, "GIT_OPTIONAL_LOCKS": "0"})
        return {"ok": result.returncode == 0, "exit_code": result.returncode,
                "output": result.stdout.strip(), "error": result.stderr.strip()[:2000]}
    except (OSError, subprocess.TimeoutExpired) as exc:
        return {"ok": False, "error": type(exc).__name__}


def emit(data):
    print(json.dumps(data, indent=2, sort_keys=True))


def report_save(name, data):
    private_dir(STATE)
    destination = STATE / (name + ".json")
    fd, temporary = tempfile.mkstemp(prefix=".report-", dir=STATE)
    try:
        with os.fdopen(fd, "w") as stream:
            json.dump(data, stream, indent=2, sort_keys=True)
            stream.write("\n")
        os.replace(temporary, destination)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
    return destination


@contextlib.contextmanager
def coordination_lock(wait=0):
    private_dir(STATE)
    fd = os.open(STATE / "build-maintenance.lock", os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "a+") as stream:
        deadline = time.monotonic() + wait
        while True:
            try:
                fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                if time.monotonic() >= deadline:
                    raise ValueError("Another coordinated build or maintenance operation is running.")
                time.sleep(min(0.2, max(0, deadline - time.monotonic())))
        yield


def executable(name):
    found = shutil.which(name)
    if found:
        return found
    for directory in (BIN, Path("/usr/sbin"), Path("/sbin")):
        p = directory / name
        if p.is_file() and os.access(p, os.X_OK):
            return str(p)
    return None


def space(path):
    path = Path(path).expanduser().resolve()
    while not path.exists():
        path = path.parent
    usage = shutil.disk_usage(path)
    fs = os.statvfs(path)
    return {"probe": str(path), "device": path.stat().st_dev,
            "available_gib": round(usage.free / 1024**3, 3),
            "total_gib": round(usage.total / 1024**3, 3),
            "available_inodes": fs.f_favail}


def user_processes():
    rows = []
    uncertain = []
    for p in Path("/proc").glob("[0-9]*"):
        comm = None
        try:
            if p.stat().st_uid != os.getuid() or int(p.name) == os.getpid():
                continue
            comm = (p / "comm").read_text().strip()
            cwd = os.readlink(p / "cwd")
            rows.append({"pid": int(p.name), "name": comm, "cwd": cwd})
        except (FileNotFoundError, ProcessLookupError):
            continue
        except PermissionError:
            uncertain.append(int(p.name))
            if comm in {"cargo", "rustc"}:
                rows.append({"pid": int(p.name), "name": comm, "cwd": "unavailable"})
    return rows, uncertain


def inside(path, root):
    try:
        Path(path.removesuffix(" (deleted)")).resolve().relative_to(root)
        return True
    except (ValueError, OSError):
        return False


def cache_references(root):
    rows, uncertain = user_processes()
    busy = []
    for row in rows:
        # Unwrapped Rust builds can start writing after a process snapshot. Native
        # cargo clean acquires Cargo's own locks as well as our cooperative lock.
        if row["name"] in {"cargo", "rustc"} or inside(row["cwd"], root):
            busy.append(row)
            continue
        p = Path("/proc") / str(row["pid"])
        try:
            references = [p / "exe", *(p / "fd").iterdir()]
        except (FileNotFoundError, ProcessLookupError):
            continue
        except PermissionError:
            uncertain.append(row["pid"])
            continue
        for reference in references:
            try:
                if inside(os.readlink(reference), root):
                    busy.append(row)
                    break
            except PermissionError:
                uncertain.append(row["pid"])
            except (FileNotFoundError, ProcessLookupError, OSError):
                continue
    return busy, sorted(set(uncertain))


def discover_caches():
    """Inventory only established Cargo targets on the home filesystem."""
    result = []
    device = HOME.stat().st_dev
    seen = set()
    for project in sorted(HOME.iterdir()):
        if not project.is_dir() or project.is_symlink() or project.name.startswith("."):
            continue
        projects = [project]
        games = project / "games"
        if games.is_dir() and not games.is_symlink():
            projects.extend(p for p in sorted(games.iterdir()) if p.is_dir() and not p.is_symlink())
        for folder in projects:
            manifest = folder / "Cargo.toml"
            target = folder / "target"
            if not manifest.is_file() or not target.is_dir() or target.is_symlink():
                continue
            target = target.resolve()
            if target in seen or target.stat().st_dev != device:
                continue
            # Refuse trees that resemble a source checkout, even when named target.
            if (target / ".git").exists() or (target / "Cargo.toml").exists():
                continue
            marker = target / "CACHEDIR.TAG"
            tagged = False
            if marker.is_file() and not marker.is_symlink():
                with marker.open() as stream:
                    tagged = stream.readline(256).rstrip() == "Signature: 8a477f597d28d172789f06886806bc55"
            fingerprints = any((target / p / ".fingerprint").is_dir() for p in ["debug", "itest", "fast", "release"])
            if not ((target / ".rustc_info.json").is_file() or tagged and fingerprints):
                continue
            seen.add(target)
            result.append({"target": str(target), "manifest": str(manifest.resolve())})
    return result


def cache_inventory():
    result = []
    for cache in discover_caches():
        root = Path(cache["target"])
        measured = probe(["du", "-x", "-k", "--max-depth=2", str(root)], timeout=30)
        sizes = {}
        if measured["ok"]:
            for line in measured["output"].splitlines():
                count, path = line.split("\t", 1)
                sizes[path] = int(count) * 1024
        busy, uncertain = cache_references(root)
        profiles = []
        for profile, dirname in [("dev", "debug"), ("itest", "itest"), ("fast", "fast")]:
            p = root / dirname
            if not p.is_dir() or p.is_symlink():
                continue
            profiles.append({"profile": profile,
                             "size_gib": round(sizes.get(str(p), 0) / 1024**3, 3),
                             "incremental_gib": round(sizes.get(str(p / "incremental"), 0) / 1024**3, 3),
                             "dry_run": [executable("cargo") or "cargo", "clean", "--frozen", "--dry-run",
                                         "--manifest-path", cache["manifest"], "--target-dir", cache["target"],
                                         "--profile", profile]})
        result.append({**cache, "size_gib": round(sizes.get(str(root), 0) / 1024**3, 3),
                       "measurement_ok": measured["ok"], "busy": busy,
                       "uninspectable_user_processes": uncertain, "profiles": profiles,
                       "review_candidate": measured["ok"] and not busy and not uncertain})
    return sorted(result, key=lambda r: r["size_gib"], reverse=True)


def doctor(args):
    rows, uncertain = user_processes()
    pressure = {}
    for name in ["cpu", "memory", "io"]:
        p = Path("/proc/pressure") / name
        if p.exists():
            pressure[name] = p.read_text().strip()
    memory = {}
    for line in Path("/proc/meminfo").read_text().splitlines():
        key, value = line.split(":", 1)
        if key in {"MemTotal", "MemAvailable", "SwapTotal", "SwapFree"}:
            memory[key + "_kib"] = int(value.split()[0])
    network = []
    for p in sorted(Path("/sys/class/net").iterdir()):
        if p.name == "lo":
            continue
        row = {"interface": p.name}
        for field in ["operstate", "speed", "duplex"]:
            try:
                row[field] = (p / field).read_text().strip()
            except OSError:
                row[field] = "unavailable"
        network.append(row)
    names = list(ALIASES) + [n for group in PACKAGES.values() for n in group]
    names += ["cargo", "rustc", "git", "sccache", "mold", "smartctl", "ethtool", "nft", "Xvfb"]
    tool_versions = {}
    for name in ["rustc", "cargo", "git", "sccache", "mold"]:
        binary = executable(name)
        if binary:
            tool_versions[name] = probe([binary, "--version"], timeout=4)
    paths = {"home": space(HOME), "temp": space(tempfile.gettempdir()),
             "cargo_home": space(os.environ.get("CARGO_HOME", HOME / ".cargo"))}
    if os.environ.get("CARGO_TARGET_DIR"):
        paths["selected_target"] = space(os.environ["CARGO_TARGET_DIR"])
    warnings = [f"{name}: {value['available_gib']} GiB available; recommended build reserve is 30 GiB"
                for name, value in paths.items() if value["available_gib"] < 30]
    data = {"version": VERSION, "time": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
            "storage": paths, "memory": memory, "pressure": pressure, "network": network,
            "tools": {n: executable(n) for n in names}, "tool_versions": tool_versions,
            "rust_builds": [r for r in rows if r["name"] in {"cargo", "rustc"}],
            "uninspectable_user_processes": uncertain,
            "failed_system_services": probe(["systemctl", "--failed", "--no-pager", "--no-legend"]),
            "failed_user_services": probe(["systemctl", "--user", "--failed", "--no-pager", "--no-legend"]),
            "warnings": warnings,
            "limits": ["Readiness does not certify tests, SMART health, firewall rules, or external reachability."]}
    if args.save:
        report_save("doctor", data)
    emit(data)
    return 0


def cache_command(args):
    if args.plan and args.apply:
        raise ValueError("--plan and --apply cannot be combined.")
    if args.apply and not (args.cache and args.profile):
        raise ValueError("--apply requires an exact --cache path and --profile.")
    if args.cache or args.profile:
        if not args.cache or not args.profile:
            raise ValueError("Provide both --cache and --profile.")
        selected = Path(args.cache).expanduser()
        if selected.is_symlink():
            raise ValueError("Refusing a symlink cache path.")
        caches = {c["target"]: c for c in discover_caches()}
        cache = caches.get(str(selected.resolve()))
        if not cache:
            raise ValueError("Cache is not a discovered Cargo target on the home filesystem.")
        profile_dir = selected / ("debug" if args.profile == "dev" else args.profile)
        if not profile_dir.is_dir() or profile_dir.is_symlink():
            raise ValueError("The requested profile is absent or is a symlink.")
        command = [executable("cargo") or "cargo", "clean", "--frozen", "--manifest-path", cache["manifest"],
                   "--target-dir", cache["target"], "--profile", args.profile]
        with coordination_lock():
            busy, uncertain = cache_references(selected.resolve())
            if args.apply and busy:
                emit({"ok": False, "busy": busy, "uninspectable_user_processes": uncertain})
                return 2
            if args.apply and uncertain:
                print("dev-cache-manage: limited process visibility for PIDs " + ", ".join(map(str, uncertain))
                      + "; using native Cargo artifact locks. See the cache inventory for review details.", file=sys.stderr)
            if not args.apply:
                command.append("--dry-run")
            # Native cargo owns lock/fingerprint semantics. Never emulate its locks
            # or manually recursively delete arbitrary incremental directories.
            result = subprocess.run(command, env={**os.environ, "CARGO_NET_OFFLINE": "true"})
            if args.apply:
                report_save("last-cleanup", {"ok": result.returncode == 0, "exit_code": result.returncode,
                            "target": cache["target"], "profile": args.profile,
                            "uninspectable_user_processes": uncertain,
                            "time": time.strftime("%Y-%m-%dT%H:%M:%S%z")})
            return result.returncode
    data = {"time": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "storage": space(HOME),
            "caches": cache_inventory(), "mutation": False,
            "notes": ["Candidates require review; profiles include binaries as well as incremental data.",
                      "Release artifacts, sources, dist packages, removable storage, and standalone targets without a manifest are excluded.",
                      "Estimates can overlap hard-linked files and may change during concurrent builds."]}
    if args.save:
        report_save("cache-plan", data)
    emit(data)
    return 0


def command_option(command, option):
    for i, value in enumerate(command):
        if value == option and i + 1 < len(command):
            return command[i + 1]
        if value.startswith(option + "="):
            return value.split("=", 1)[1]
    return None


def build_command(args):
    command = args.command
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        raise ValueError("Supply a command after --; it is executed without a shell.")
    cwd = Path(args.cwd).expanduser().resolve()
    if not cwd.is_dir():
        raise ValueError("Build working directory does not exist.")
    env = os.environ.copy()
    env["CARGO_BUILD_JOBS"] = str(args.jobs)
    cli_target = command_option(command, "--target-dir")
    if args.target_dir and cli_target:
        raise ValueError("Use either --target-dir on the wrapper or on the child command, not both.")
    selected = args.target_dir or cli_target or env.get("CARGO_TARGET_DIR")
    manifest = command_option(command, "--manifest-path")
    base = cwd
    if manifest:
        m = Path(manifest).expanduser()
        base = (m if m.is_absolute() else cwd / m).resolve().parent
    target = Path(selected).expanduser() if selected else base / "target"
    if not target.is_absolute():
        target = cwd / target
    if args.target_dir:
        env["CARGO_TARGET_DIR"] = str(target.resolve())
    paths = {"target": target, "temp": Path(tempfile.gettempdir()),
             "cargo_home": Path(env.get("CARGO_HOME", HOME / ".cargo"))}
    def preflight():
        status = {k: space(v) for k, v in paths.items()}
        low = [k for k, v in status.items() if v["available_gib"] < args.min_free_gib]
        if low:
            raise ValueError(f"Insufficient build headroom at {', '.join(low)}; need {args.min_free_gib} GiB. Run dev-cache-manage --plan.")
        return status
    preflight()
    prefix = ["nice", "-n", "10"]
    if executable("ionice"):
        prefix += [executable("ionice"), "-c", "2", "-n", "7"]
    with coordination_lock(args.wait):
        storage = preflight()
        started = time.monotonic()
        print(f"dev-build-run: jobs={args.jobs}, target={target.resolve()}", file=sys.stderr)
        process = subprocess.Popen(prefix + command, cwd=cwd, env=env, start_new_session=True)
        previous = {}
        def forward(signum, frame):
            try:
                os.killpg(process.pid, signum)
            except ProcessLookupError:
                pass
        try:
            for signum in [signal.SIGINT, signal.SIGTERM]:
                previous[signum] = signal.signal(signum, forward)
            code = process.wait()
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
            for signum, handler in previous.items():
                signal.signal(signum, handler)
        code = code if code >= 0 else 128 - code
        # Do not persist arbitrary child arguments: they may include credentials.
        report_save("last-build", {"ok": code == 0, "exit_code": code, "cwd": str(cwd),
                    "program": Path(command[0]).name, "jobs": args.jobs, "storage_before": storage,
                    "target": str(target.resolve()), "elapsed_seconds": round(time.monotonic() - started, 3),
                    "time": time.strftime("%Y-%m-%dT%H:%M:%S%z")})
    return code


def parse_worktrees(output):
    rows = []
    for item in output.split("\0"):
        if item.startswith("worktree "):
            rows.append({"path": item[9:]})
        elif rows and item:
            key, _, value = item.partition(" ")
            rows[-1][key] = value or True
    return rows


def worktrees_command(args):
    result = []
    for repo in args.repo or [HOME / "BlueEngine", HOME / "BlueEngineGames", HOME / "Feta"]:
        repo = Path(repo).expanduser().resolve()
        inventory = probe(["git", "-C", str(repo), "worktree", "list", "--porcelain", "-z"])
        if not inventory["ok"]:
            result.append({"repository": str(repo), "ok": False, "error": inventory.get("error")})
            continue
        for row in parse_worktrees(inventory["output"]):
            path = Path(row["path"])
            row["repository"] = str(repo)
            row["exists"] = path.is_dir()
            if row["exists"]:
                status = probe(["git", "-C", str(path), "status", "--porcelain", "-z"], timeout=5)
                row["status_known"] = status["ok"]
                row["dirty"] = bool(status.get("output")) if status["ok"] else None
                row["untracked"] = sum(s.startswith("?? ") for s in status.get("output", "").split("\0")) if status["ok"] else None
                divergence = probe(["git", "-C", str(path), "rev-list", "--left-right", "--count", "HEAD...@{upstream}"], timeout=5)
                if divergence["ok"]:
                    ahead, behind = map(int, divergence["output"].split())
                    row["local_ahead"] = ahead
                    row["local_behind"] = behind
                else:
                    row["upstream"] = "unavailable or detached"
                processes, _ = user_processes()
                row["processes_in_tree"] = [p for p in processes if inside(p["cwd"], path.resolve())]
            else:
                row["cleanup_candidate"] = (inside(str(path), Path("/tmp")) and "prunable" in row and "locked" not in row)
                row["note"] = "A missing directory may be on disconnected storage; no pruning is performed."
            result.append(row)
    data = {"worktrees": result, "mutation": False,
            "limits": ["Ahead/behind uses local tracking refs; no fetch or remote reachability check.",
                      "Ignored build directories are not evidence of an expendable source tree."]}
    if args.save:
        report_save("worktrees", data)
    emit(data)
    return 0 if all(r.get("ok", True) and r.get("status_known", True) for r in result) else 2


def report_archive_usage(config, games):
    """Read bounded file-size metadata only; never read or trim archived matches."""
    result = {"files": [], "total_bytes": 0, "truncated": False, "retention": "No deletion"}
    try:
        registry = configparser.ConfigParser(interpolation=None, inline_comment_prefixes=("#", ";"))
        registry.read(config)
        value = registry.get("hub", "report_dir", fallback="").strip().strip('"')
        root = Path(value).expanduser()
        if not value or not root.is_absolute():
            return {**result, "inventory": "unavailable: no explicit absolute report_dir"}
        result["root"] = str(root)
        for game in games:
            folder = root / game
            if folder.is_symlink() or not folder.is_dir():
                continue
            for index, entry in enumerate(folder.iterdir()):
                if index >= 128:
                    result["truncated"] = True
                    break
                if not re.fullmatch(r"port-[0-9]{1,5}", entry.name) or entry.is_symlink():
                    continue
                archive = entry / "matches.jsonl"
                try:
                    metadata = archive.stat(follow_symlinks=False)
                except FileNotFoundError:
                    continue
                if stat.S_ISREG(metadata.st_mode):
                    result["files"].append({"game": game, "path": str(archive), "bytes": metadata.st_size})
                    result["total_bytes"] += metadata.st_size
    except (OSError, configparser.Error) as error:
        result["error"] = str(error)
    return result


def server_command(args):
    units = ["blueengine-hub.service", "blueengine-ddns.service", "blueengine-portmap.service",
             "blueengine-ddns.timer", "blueengine-portmap.timer"]
    rows = []
    for unit in units:
        properties = probe(["systemctl", "--user", "show", unit, "-p", "LoadState", "-p", "ActiveState", "-p", "SubState",
                            "-p", "Result", "-p", "ExecMainStatus", "-p", "NRestarts"], timeout=4)
        values = dict(l.split("=", 1) for l in properties.get("output", "").splitlines() if "=" in l)
        rows.append({"unit": unit, "query_ok": properties["ok"], **values})
    config = HOME / ".config/blueengine/hub.conf"
    hub = HOME / "blueengine/be2-hub"
    games = args.game or []
    if not games and config.is_file():
        registry = configparser.ConfigParser(interpolation=None, strict=False)
        registry.read(config)
        games = [s[5:].strip() for s in registry.sections() if s.startswith("game ")]
    readiness = []
    for game in games:
        if not re.fullmatch(r"[A-Za-z0-9_-]+", game):
            raise ValueError("Unexpected game identifier")
        receipt = HOME / ".local/share/blueengine/deployed" / (game + ".json")
        deployment = {"receipt": "unavailable"}
        command = [str(hub), "status", game, "--config", str(config), "--wait", "1"]
        if receipt.is_file():
            try:
                record = json.loads(receipt.read_text())
                expected = record.get("info", {}).get("build")
                if not isinstance(expected, str) or not re.fullmatch(r"[0-9a-fA-F]{8}", expected):
                    raise ValueError("invalid receipt build")
                deployment = {"receipt": "read", "phase": record.get("phase"), "expected_build": expected}
                command += ["--expect-build", expected]
            except (OSError, ValueError, AttributeError):
                deployment = {"receipt": "unreadable or invalid"}
        reply = probe(command, timeout=6)
        if deployment["receipt"] == "unreadable or invalid":
            reply["ok"] = False
        readiness.append({"game": game, "deployment": deployment, **reply})
    service_ok = all(r["query_ok"] and r.get("LoadState") == "loaded" and r.get("Result", "success") == "success"
                     and (r.get("ActiveState") == "active" if r["unit"].endswith(".timer") or r["unit"] == "blueengine-hub.service"
                          else r.get("ExecMainStatus") == "0") for r in rows)
    data = {"local_ok": bool(readiness) and service_ok and all(r["ok"] for r in readiness),
            "services": rows, "games": readiness,
            "report_archives": report_archive_usage(config, games),
            "external_reachability": "unverified; timer success does not certify router mappings or public connectivity"}
    if args.save:
        report_save("server-health", data)
    emit(data)
    return 0 if data["local_ok"] else 2


def apt_package(name):
    policy = probe(["apt-cache", "policy", name])
    candidate = next((l.split(":", 1)[1].strip() for l in policy.get("output", "").splitlines() if "Candidate:" in l), None)
    if not candidate or candidate == "(none)":
        raise ValueError(f"No Debian candidate for {name}")
    output = probe(["apt-cache", "show", f"{name}={candidate}"])
    if not output["ok"]:
        raise ValueError(f"Cannot read Debian package metadata for {name}")
    fields = {}
    for line in output["output"].split("\n\n", 1)[0].splitlines():
        if ": " in line and not line.startswith(" "):
            k, v = line.split(": ", 1)
            fields[k] = v
    if fields.get("Architecture") not in {"amd64", "all"} or fields.get("Package") != name:
        raise ValueError("Unexpected Debian package identity")
    filename = fields.get("Filename", "")
    if not filename.startswith("pool/") or ".." in Path(filename).parts:
        raise ValueError("Unexpected Debian package path")
    sha = fields.get("SHA256", "")
    if len(sha) != 64 or any(c not in "0123456789abcdef" for c in sha):
        raise ValueError("Missing valid SHA256 package metadata")
    origin = "https://security.debian.org/debian-security/" if filename.startswith("pool/updates/") else "https://deb.debian.org/debian/"
    return {"package": name, "version": candidate, "url": origin + filename,
            "sha256": sha, "size": int(fields["Size"]),
            "installed_kib": int(fields["Installed-Size"]), "commands": PACKAGES[name]}


def install_package(package):
    name = package["package"]
    root = HOME / ".local/share/dev-tools/packages" / name / package["version"]
    BIN.mkdir(parents=True, exist_ok=True)
    managed = HOME / ".local/share/dev-tools/packages"
    for command in package["commands"]:
        destination = BIN / command
        if destination.exists() or destination.is_symlink():
            if not destination.is_symlink() or not inside(str(destination.resolve()), managed.resolve()):
                raise ValueError(f"Refusing to replace unmanaged command: {destination}")
    if not root.exists():
        root.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix=".install-", dir=root.parent) as directory:
            staging = Path(directory)
            archive = staging / "package.deb"
            h = hashlib.sha256()
            size = 0
            with urllib.request.urlopen(package["url"], timeout=40) as response, archive.open("wb") as output:
                while chunk := response.read(1024 * 1024):
                    size += len(chunk)
                    if size > package["size"]:
                        raise ValueError("Download exceeds Debian's declared package size")
                    h.update(chunk)
                    output.write(chunk)
            if size != package["size"] or h.hexdigest() != package["sha256"]:
                raise ValueError("Debian package checksum/size mismatch")
            payload = staging / "payload"
            subprocess.run(["dpkg-deb", "-x", str(archive), str(payload)], check=True, timeout=30)
            for command in package["commands"]:
                binary = payload / "usr/bin" / command
                if not binary.is_file():
                    raise ValueError(f"Expected executable missing: {command}")
                dependencies = probe(["ldd", str(binary)])
                if "not found" in dependencies.get("output", "") or "not found" in dependencies.get("error", ""):
                    raise ValueError(f"System shared library missing for {command}; install dependencies with apt first")
            (payload / "installation.json").write_text(json.dumps(package, indent=2) + "\n")
            payload.rename(root)
    elif (root / "installation.json").is_symlink() or not (root / "installation.json").is_file():
        raise ValueError("Existing payload has no installation provenance")
    for command in package["commands"]:
        destination = BIN / command
        fd, temporary = tempfile.mkstemp(prefix=".link-", dir=BIN)
        os.close(fd)
        os.unlink(temporary)
        try:
            os.symlink(root / "usr/bin" / command, temporary)
            os.replace(temporary, destination)
        finally:
            if os.path.lexists(temporary):
                os.unlink(temporary)
    return {"package": name, "version": package["version"], "installed_at": str(root), "commands": package["commands"]}


def bootstrap_command(args):
    selected = args.package or list(PACKAGES)
    if args.check:
        emit({"commands": {c: executable(c) for p in selected for c in PACKAGES[p]},
              "system_packages_managed": False})
        return 0 if all(executable(c) for p in selected for c in PACKAGES[p]) else 2
    packages = [apt_package(p) for p in selected]
    if args.install:
        with coordination_lock():
            data = {"installed": [install_package(p) for p in packages], "mode": "user-local Debian payloads; no system services installed"}
            report_save("installed-tools", data)
            emit(data)
    else:
        emit({"packages": packages, "estimated_payload_mib": round(sum(p["installed_kib"] for p in packages) / 1024, 2),
              "mutation": False, "mode": "user-local Debian payloads; no privileged maintainer scripts"})
    return 0


def positive(value):
    value = int(value)
    if value < 1:
        raise argparse.ArgumentTypeError("must be at least 1")
    return value


def nonnegative(value):
    value = float(value)
    if value < 0 or not math.isfinite(value):
        raise argparse.ArgumentTypeError("must be finite and nonnegative")
    return value


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", action="version", version=VERSION)
    sub = parser.add_subparsers(dest="operation", required=True)
    p = sub.add_parser("doctor", help="Read-only machine/tool health as JSON")
    p.add_argument("--save", action="store_true")
    p.set_defaults(run=doctor)
    p = sub.add_parser("cache", help="Inventory or explicitly clean one native Cargo profile")
    p.add_argument("--plan", action="store_true", help="Inventory only (default)")
    p.add_argument("--save", action="store_true")
    p.add_argument("--cache")
    p.add_argument("--profile", choices=["dev", "itest", "fast"])
    p.add_argument("--apply", action="store_true", help="Clean the exact selected profile after review")
    p.set_defaults(run=cache_command)
    p = sub.add_parser("build", help="Coordinate a resource-limited command and preserve its result")
    p.add_argument("--cwd", default=os.getcwd())
    p.add_argument("--target-dir")
    p.add_argument("--jobs", type=positive, default=2)
    p.add_argument("--min-free-gib", type=nonnegative, default=20)
    p.add_argument("--wait", type=nonnegative, default=0, help="Maximum seconds to wait for the cooperative lock")
    p.add_argument("command", nargs=argparse.REMAINDER)
    p.set_defaults(run=build_command)
    p = sub.add_parser("worktrees", help="Read-only dirty/ahead/stale worktree audit")
    p.add_argument("--repo", action="append")
    p.add_argument("--save", action="store_true")
    p.set_defaults(run=worktrees_command)
    p = sub.add_parser("server", help="Query existing hub and user service health without restarting")
    p.add_argument("--game", action="append")
    p.add_argument("--save", action="store_true")
    p.set_defaults(run=server_command)
    p = sub.add_parser("bootstrap", help="Check, plan, or install checksum-verified Debian utilities locally")
    group = p.add_mutually_exclusive_group()
    group.add_argument("--check", action="store_true")
    group.add_argument("--plan", action="store_true")
    group.add_argument("--install", action="store_true")
    p.add_argument("--package", choices=list(PACKAGES), action="append")
    p.set_defaults(run=bootstrap_command)
    argv = list(sys.argv[1:] if argv is None else argv)
    alias = ALIASES.get(Path(sys.argv[0]).name)
    if alias and argv != ["--version"]:
        argv.insert(0, alias)
    args = parser.parse_args(argv)
    try:
        return args.run(args)
    except (ValueError, OSError, subprocess.SubprocessError) as exc:
        print(f"dev-tools: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
