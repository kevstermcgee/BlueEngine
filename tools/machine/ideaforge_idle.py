#!/usr/bin/python3
"""Read-only ExecCondition: defer hourly IdeaForge while a supervisor is alive."""
import argparse
import json
import os
from pathlib import Path
import re
import stat
import sys


def daily_root(config, home):
    try:
        settings = json.loads(config.read_text())
    except FileNotFoundError:
        settings = {}
    if not isinstance(settings, dict):
        raise ValueError("Invalid IdeaForge configuration")
    engine = settings.get("engine_root") or str(home / "BlueEngine-idea-forge-native")
    root = settings.get("daily_root") or str(Path(engine) / ".be2-work/idea-forge/daily")
    if not isinstance(root, str) or not Path(root).is_absolute():
        raise ValueError("Daily supervisor path must be absolute")
    return Path(root).resolve()


def idle_condition(config, home=None):
    lock = daily_root(config, home or Path.home()) / "supervisor.lock"
    try:
        fd = os.open(lock, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    except FileNotFoundError:
        return 0
    with os.fdopen(fd, "r", encoding="ascii") as stream:
        metadata = os.fstat(stream.fileno())
        if (not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid()
                or metadata.st_mode & 0o022 or metadata.st_nlink != 1 or metadata.st_size > 64):
            raise ValueError("Supervisor lock requires inspection")
        owner = stream.read(65).strip()
    if not re.fullmatch(r"[1-9][0-9]{0,9}", owner) or int(owner) > 2**31 - 1:
        raise ValueError("Supervisor lock owner requires inspection")
    try:
        os.kill(int(owner), 0)
    except ProcessLookupError:
        return 0
    except PermissionError:
        # An existing owner still blocks an hourly retry, even if unsignalable.
        pass
    return 1


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, default=Path.home() / ".config/ideaforge/daily.json")
    args = parser.parse_args(argv)
    try:
        return idle_condition(args.config)
    except (OSError, ValueError, TypeError, OverflowError):
        print("ideaforge-idle: supervisor lock or configuration requires inspection", file=sys.stderr)
        return 255


if __name__ == "__main__":
    raise SystemExit(main())
