#!/usr/bin/python3
"""Write private status for failed scheduled services and completed IdeaForge attempts."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import time

import dev_tools as tools

JOBS = {"blueengine-nightly": ("BlueEngine nightly", "nightly", "blueengine-nightly"),
        "ideaforge-daily": ("IdeaForge daily", "ideaforge", "ideaforge")}
PROPERTIES = ("ActiveState", "SubState", "Result", "ExecMainStatus", "StateChangeTimestamp",
              "InactiveExitTimestampMonotonic", "ExecMainExitTimestampMonotonic")


def service_status(job):
    result = tools.probe(["systemctl", "--user", "show", job + ".service", "--no-pager",
                          "--timestamp=unix", *["--property=" + key for key in PROPERTIES]], timeout=10)
    if not result["ok"]:
        raise ValueError("Service status could not be verified")
    return dict(line.split("=", 1) for line in result["output"].splitlines()
                if "=" in line and line.split("=", 1)[0] in PROPERTIES)


def token(value):
    return value if isinstance(value, str) and re.fullmatch(r"[A-Za-z0-9_-]{1,64}", value) else "unavailable"


def storage_reason(report_path, failed_at):
    try:
        report = json.loads(report_path.read_text())
        checked_at = datetime.fromisoformat(report["time"]).timestamp()
        if abs(checked_at - failed_at) > 600:
            return "The saved storage check is too old to explain this failure."
        if report.get("admitted") is True:
            return "The latest saved storage check passed; see the service journal for this failure."
        if report.get("admitted") is not False:
            return "No current storage admission result is available."
        match = re.fullmatch(r"[a-z0-9-]+ retained: need ([0-9.]+) GiB free on every build filesystem; lowest is ([0-9.]+) GiB",
                             report.get("reason", ""))
        if match:
            return f"Storage admission refused: {match[2]} GiB free; {match[1]} GiB required."
        return "Storage admission refused; inspect the linked admission report."
    except (OSError, ValueError, KeyError, TypeError):
        return "No current storage admission result is available."


def atomic_text(path, contents):
    fd, temporary = tempfile.mkstemp(prefix=".scheduled-status-", dir=path.parent)
    try:
        with os.fdopen(fd, "w") as stream:
            stream.write(contents)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def nightly_links(home):
    state = home / ".local/state/blueengine-nightly"
    try:
        config = json.loads((home / ".config/blueengine-nightly/config.json").read_text())
    except (OSError, ValueError):
        config = {}
    if not isinstance(config, dict):
        config = {}
    def configured(key, fallback):
        value = config.get(key)
        if isinstance(value, str) and Path(value).is_absolute() and not any(c in value for c in "\r\n<>"):
            return Path(value)
        return fallback
    state = configured("state", state)
    retained = state / "runs"
    runs = configured("runs_root", retained)
    links = f"[Last successful result]({state / 'last-success.json'}) · [Current run logs]({runs})"
    if runs != retained:
        links += f" · [Retained run logs]({retained})"
    return links + "\n\n"


def forge_links(home):
    original = home / "BlueEngine-idea-forge-native/.be2-work/idea-forge"
    try:
        config = json.loads((home / ".config/ideaforge/daily.json").read_text())
    except (OSError, ValueError):
        config = {}
    if not isinstance(config, dict):
        config = {}
    def configured(key, fallback):
        value = config.get(key)
        if isinstance(value, str) and Path(value).is_absolute() and not any(c in value for c in "\r\n<>"):
            return Path(value)
        return fallback
    engine = configured("engine_root", home / "BlueEngine-idea-forge-native")
    daily = configured("daily_root", engine / ".be2-work/idea-forge/daily")
    runs = configured("runs_root", engine / ".be2-work/idea-forge/runs")
    links = f"[Publication journals]({daily / 'batches'}) · [Current run logs]({runs})"
    if runs != original / "runs":
        links += f" · [Retained run logs]({original / 'runs'})"
    return links + "\n\n"


def record_status(job, status, home=None, now=None):
    home = home or tools.HOME
    now = now or datetime.now(timezone.utc)
    # Delayed callbacks describe current terminal status, never an older failure.
    if status.get("Result") in (None, "exec-condition") or status.get("ActiveState") in ("active", "activating", "reloading", "deactivating"):
        return None
    completed = status["Result"] == "success"
    if completed and job != "ideaforge-daily":
        # The nightly runner owns its authoritative successful report.
        return None
    if completed:
        # OnSuccess can also follow a skipped ExecCondition. Its old main exit
        # must not be reported as completion of this new, skipped invocation.
        try:
            entered = int(status["InactiveExitTimestampMonotonic"])
            exited = int(status["ExecMainExitTimestampMonotonic"])
        except (KeyError, TypeError, ValueError):
            return None
        if not 0 < entered <= exited:
            return None
    title, workload, directory = JOBS[job]
    state = home / ".local/state" / directory
    latest = state / "latest.md"
    history = state / "scheduled-status"
    tools.private_dir(state)
    tools.private_dir(history)
    if latest.is_symlink():
        raise ValueError("Latest status must not be a symlink")
    previous = None
    if latest.is_file():
        if latest.stat().st_size > 1024 * 1024:
            raise ValueError("Existing report is too large to archive safely")
        contents = latest.read_text()
        previous = history / ("previous-" + hashlib.sha256(contents.encode()).hexdigest()[:20] + ".md")
        if not previous.exists():
            atomic_text(previous, contents)
    stamp = status.get("StateChangeTimestamp", "")
    event_at = float(stamp[1:]) if re.fullmatch(r"@[0-9]+(?:\.[0-9]+)?", stamp) else now.timestamp()
    admission = home / ".local/state/dev-tools" / f"storage-admission-{workload}.json"
    outcome = "completed" if completed else "failed"
    detail = ("Scheduled attempt completed. Publication details are recorded in the linked batch journals."
              if completed else "The scheduled attempt did not complete; a preflight failure may occur before the runner starts.")
    body = (f"# {title}\n\nStatus: {outcome}\n\n"
            f"Observed: {now.isoformat()}\n\n"
            f"Service `{job}.service`: result `{token(status.get('Result'))}`, "
            f"state `{token(status.get('ActiveState'))}/{token(status.get('SubState'))}`, "
            f"main exit status `{token(status.get('ExecMainStatus'))}`. "
            + detail + "\n\n")
    if not completed:
        body += storage_reason(admission, event_at) + "\n\n"
    body += f"[Storage admission report]({admission})\n\n"
    if job == "blueengine-nightly":
        body += nightly_links(home)
    else:
        body += forge_links(home)
    if previous:
        body += f"[Previous local report]({previous})\n\n"
    body += f"Service journal: `journalctl --user -u {job}.service --since '@{int(event_at) - 600}'`\n"
    atomic_text(history / f"{outcome}-{time.time_ns()}.md", body)
    atomic_text(latest, body)
    return latest


def record_failure(job, status, home=None, now=None):
    if status.get("Result") == "success":
        return None
    return record_status(job, status, home, now)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("job", choices=sorted(JOBS))
    args = parser.parse_args(argv)
    try:
        path = record_status(args.job, service_status(args.job))
        print(f"scheduled-status: {path if path else 'service has recovered or is running; no report changed'}")
        return 0
    except (OSError, ValueError, TypeError) as exc:
        print(f"scheduled-status: local report could not be updated ({type(exc).__name__})", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
