#!/usr/bin/python3
"""Prove per-service memory containment with two disposable systemd user units."""
import argparse
import subprocess
import sys
import time
import uuid

sys.dont_write_bytecode = True
import dev_tools as util


def command(args):
    return subprocess.run(args, text=True, capture_output=True, timeout=10, check=True)


def properties(unit):
    output = command(["systemctl", "--user", "show", unit,
        "-p", "ActiveState", "-p", "Result", "-p", "ControlGroup", "-p", "MemoryMax"]).stdout
    return dict(line.split("=", 1) for line in output.splitlines() if "=" in line)


def start(unit, script):
    command(["systemd-run", "--user", "--quiet", f"--unit={unit}", "--service-type=exec",
        "--property=MemoryMax=64M", "--property=MemorySwapMax=0", "--property=TasksMax=8",
        "--property=RuntimeMaxSec=20", "--property=OOMPolicy=stop", "--property=Restart=no",
        "--property=NoNewPrivileges=yes", "--property=UMask=0077",
        "/usr/bin/python3", "-I", "-c", script])


def run():
    prefix = "dev-room-probe-" + uuid.uuid4().hex
    healthy, failing = [prefix + suffix + ".service" for suffix in ("-healthy", "-memory")]
    report = {"version": util.VERSION, "ok": False, "units": [healthy, failing],
        "scope": "Two temporary user services only; no hub configuration or running room changes",
        "memory_limit_mib": 64, "limitation": "Containment prototype, not a deployed room spawner"}
    try:
        start(healthy, "import time; time.sleep(30)")
        before = properties(healthy)
        report["healthy_before"] = before
        if before["ActiveState"] != "active" or int(before["MemoryMax"]) != 64 * 1024**2:
            raise ValueError("Scratch service memory limit was not established")
        start(failing, "import time; payload = bytearray(128 * 1024**2); time.sleep(5)")
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            failed = properties(failing)
            if failed["ActiveState"] in ("failed", "inactive"):
                break
            time.sleep(0.1)
        after = properties(healthy)
        report.update(memory_service=failed, healthy_after=after)
        report["ok"] = failed["Result"] == "oom-kill" and after["ActiveState"] == "active"
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        report["error"] = str(error)
    finally:
        # Exact random names belonging to this invocation only, including partial startup.
        for unit in (failing, healthy):
            for action in ("stop", "reset-failed"):
                try:
                    subprocess.run(["systemctl", "--user", action, unit],
                        capture_output=True, timeout=10, check=False)
                except (OSError, subprocess.SubprocessError):
                    report["ok"] = False
                    report["cleanup_error"] = unit
        util.report_save("server-isolation-test", report)
    util.emit(report)
    return 0 if report["ok"] else 2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", action="store_true", help="Launch disposable memory containment test")
    args = parser.parse_args()
    if not args.run:
        util.emit({"mutation": False, "plan": "Create two temporary 64 MiB user services; exceed one limit, verify the other stays active, then remove both", "live_services_changed": False})
        return 0
    return run()


if __name__ == "__main__":
    raise SystemExit(main())
