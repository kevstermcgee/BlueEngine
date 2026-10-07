#!/usr/bin/python3
"""Benchmark concurrent Spooky Kart rooms on loopback without touching the hub."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
import dev_tools as util


def process_usage(pid):
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    result = {"cpu_seconds": (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")}
    for line in Path(f"/proc/{pid}/status").read_text().splitlines():
        if line.startswith(("VmRSS:", "VmHWM:")):
            key, value, *_ = line.split()
            result[key.rstrip(":")] = int(value) * 1024
    return result


def memory_available():
    for line in Path("/proc/meminfo").read_text().splitlines():
        if line.startswith("MemAvailable:"):
            return int(line.split()[1]) * 1024
    raise ValueError("Cannot establish available memory")


def check_reserve(args):
    free = util.space(util.STATE)["available_gib"]
    available = memory_available() / 1024**3
    if free < args.min_free_gib or available < args.memory_reserve_gib:
        raise ValueError(f"Load test reserve reached: disk {free:.2f} GiB, memory {available:.2f} GiB")
    return {"free_gib": free, "memory_available_gib": round(available, 3)}


def identity(path):
    path = Path(path).expanduser().resolve(strict=True)
    if not path.is_file() or not os.access(path, os.X_OK):
        raise ValueError(f"Not an executable file: {path}")
    if path.stat().st_uid != os.getuid():
        raise ValueError(f"Benchmark binary must be owned by this user: {path}")
    return {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def stop_process(process):
    """Stop only a session created by this tool, including any descendants."""
    if process is None:
        return
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait(timeout=5)


def reserve_ports(count):
    held = []
    try:
        # Avoid the ephemeral client-port range and the hub's public port pool.
        for port in range(24300, 24400):
            candidate = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            try:
                candidate.bind(("127.0.0.1", port))
            except OSError:
                candidate.close()
                continue
            held.append(candidate)
            if len(held) == count:
                return held
        raise ValueError("Not enough free isolated loopback test ports")
    except BaseException:
        for candidate in held:
            candidate.close()
        raise


def read_json_lines(path):
    if path.stat().st_size > 4 * 1024**2:
        raise ValueError("Benchmark report exceeds its bounded size")
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def private_log(path):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    return os.fdopen(fd, "w")


def summarize_room(room, clients, elapsed):
    bot_lines = read_json_lines(room["bot_log"])
    client = bot_lines[-1]
    if client.get("clients") != clients or client.get("races_completed", 0) < room["races"]:
        raise ValueError("Bot driver did not complete the requested races")
    matches = read_json_lines(room["directory"] / "matches.jsonl")
    if len(matches) < room["races"]:
        raise ValueError("Server did not preserve every requested race report")
    matches = matches[:room["races"]]
    servers = [m["net"]["server"] for m in matches]
    peers = [p for m in matches for p in m["net"]["peers"]]
    sends = [m.get("send") for m in matches]
    if any(len(m["net"]["peers"]) != clients for m in matches):
        raise ValueError("Not all requested players participated in each race")
    seconds = sum(m["report"]["race_seconds"] for m in matches)
    ticks = sum(s["ticks"] for s in servers)
    before, after = room["before"], room["after"]
    return {
        "room": room["index"], "ok": True, "players": clients,
        "races_completed": client["races_completed"], "window_seconds": round(elapsed, 3),
        "archive_bytes": (room["directory"] / "matches.jsonl").stat().st_size,
        "send_outcomes": ({key: sum(s[key] for s in sends) for key in
            ("attempts", "accepted", "accepted_bytes", "backpressured", "errors", "oversized", "snapshots_accepted")}
            if all(s is not None for s in sends) else None),
        "server_cpu_pct_of_core": round(100 * (after["cpu_seconds"] - before["cpu_seconds"]) / elapsed, 3),
        "driver_cpu_pct_of_core": round(100 * room["driver_cpu_seconds"] / elapsed, 3),
        "server_peak_rss_mib": round(after["VmHWM"] / 1024**2, 3),
        "driver_sampled_peak_rss_mib": round(room["driver_peak"] / 1024**2, 3),
        "tick_mean_us": round(sum(s["tick_us_mean"] * s["ticks"] for s in servers) / max(1, ticks), 3),
        "tick_max_us": max(s["tick_us_max"] for s in servers),
        "inputs_skipped": sum(p["inputs_skipped"] for p in peers),
        "repeated_input_pct": round(100 * sum(p["ticks_repeated"] for p in peers) / max(1, ticks * clients), 4),
        "left_early": sum(bool(p["left_early"]) for p in peers),
        "bad_datagrams": sum(s["bad_datagrams"] for s in servers),
        "per_client_down_kib_s": round(sum(p["bytes_out"] for p in peers) / max(1, seconds * clients * 1024), 3),
        "per_client_up_kib_s": round(sum(p["bytes_in"] for p in peers) / max(1, seconds * clients * 1024), 3),
        "prediction_corrections": sum(p["corrections"] for p in client["per_client"]),
        "prediction_snaps": sum(p["snaps"] for p in client["per_client"]),
        "max_prediction_error_m": max(p["max_error_m"] for p in client["per_client"]),
    }


def run_level(args, count, directory, binaries):
    rooms, processes, files = [], [], []
    held = reserve_ports(count)
    result = {"rooms": count, "clients_per_room": args.clients, "ok": False}
    env = {key: value for key, value in os.environ.items() if not key.endswith("JOIN_KEY")}
    started = None
    try:
        for index, reservation in enumerate(held):
            check_reserve(args)
            path = directory / f"room-{index+1}"
            path.mkdir(mode=0o700)
            address = f"127.0.0.1:{reservation.getsockname()[1]}"
            reservation.close()
            server_log = path / "server.log"
            stream = private_log(server_log)
            files.append(stream)
            server = subprocess.Popen(["nice", "-n", "10", binaries["server"]["path"],
                "--listen", address, "--transport", "development", "--status-lines",
                "--report-dir", str(path), "--auto-start", "0", "--seed", "11"],
                cwd=path, env=env, stdin=subprocess.DEVNULL, stdout=stream,
                stderr=subprocess.STDOUT, start_new_session=True, umask=0o077)
            processes.append(server)
            rooms.append({"index": index+1, "directory": path, "address": address,
                "server": server, "server_log": server_log, "races": args.races})
        deadline = time.monotonic() + 30
        while True:
            if any(room["server"].poll() is not None for room in rooms):
                raise ValueError("An isolated room failed to start; see its private server.log")
            if all("STATUS game=spooky-kart " in room["server_log"].read_text() for room in rooms):
                break
            if time.monotonic() >= deadline:
                raise ValueError("Isolated room startup timed out")
            check_reserve(args)
            time.sleep(0.1)
        started = time.monotonic()
        for room in rooms:
            room["before"] = process_usage(room["server"].pid)
            room["bot_log"] = room["directory"] / "bots.jsonl"
            stream = private_log(room["bot_log"])
            files.append(stream)
            # wait4 measures reaped drivers too; no process-wide cumulative RSS guess.
            driver = subprocess.Popen(["nice", "-n", "10", binaries["bots"]["path"],
                room["address"], "--clients", str(args.clients), "--races", str(args.races),
                "--seconds", str(args.seconds)], cwd=room["directory"], env=env,
                stdin=subprocess.DEVNULL, stdout=stream, stderr=subprocess.STDOUT,
                start_new_session=True, umask=0o077)
            processes.append(driver)
            room.update(driver=driver, driver_peak=0, driver_cpu_seconds=0.0, driver_done=False)
        last_progress = started
        while True:
            now = time.monotonic()
            if now - started > args.seconds + 10:
                raise ValueError("Concurrent race test exceeded its time bound")
            check_reserve(args)
            for room in rooms:
                if room["server"].poll() is not None:
                    raise ValueError("An isolated server exited during its race")
                if not room["driver_done"]:
                    try:
                        room["driver_peak"] = max(room["driver_peak"], process_usage(room["driver"].pid).get("VmRSS", 0))
                    except FileNotFoundError:
                        pass
                    pid, status, usage = os.wait4(room["driver"].pid, os.WNOHANG)
                    if pid:
                        room["driver"].returncode = os.waitstatus_to_exitcode(status)
                        if room["driver"].returncode != 0:
                            raise ValueError("Bot driver failed; see its private bots.jsonl")
                        room["driver_done"] = True
                        room["driver_cpu_seconds"] = usage.ru_utime + usage.ru_stime
                        room["driver_peak"] = max(room["driver_peak"], usage.ru_maxrss * 1024)
                for name in ("server_log", "bot_log"):
                    if room[name].stat().st_size > 4 * 1024**2:
                        raise ValueError("Benchmark log exceeds its size bound")
            if all(room["driver_done"] for room in rooms):
                break
            if now - last_progress >= 30:
                print(f"dev-server-load: {count} rooms, {now-started:.0f}s elapsed", file=sys.stderr, flush=True)
                last_progress = now
            time.sleep(0.25)
        elapsed = time.monotonic() - started
        for room in rooms:
            room["after"] = process_usage(room["server"].pid)
        result["room_results"] = [summarize_room(room, args.clients, elapsed) for room in rooms]
        result.update(ok=True, window_seconds=round(elapsed, 3),
            total_server_cpu_pct_of_core=round(sum(r["server_cpu_pct_of_core"] for r in result["room_results"]), 3),
            total_driver_cpu_pct_of_core=round(sum(r["driver_cpu_pct_of_core"] for r in result["room_results"]), 3),
            slowest_tick_us=max(r["tick_max_us"] for r in result["room_results"]))
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        result["error"] = str(error)
    finally:
        for process in reversed(processes):
            stop_process(process)
        for stream in files:
            stream.close()
        for reservation in held:
            reservation.close()
    return result


def run(args):
    binaries = {"server": identity(args.server), "bots": identity(args.bots)}
    info = util.probe([binaries["server"]["path"], "--info"])
    if not info["ok"] or "game=spooky-kart" not in info["output"].splitlines():
        raise ValueError("This load fixture requires a Spooky Kart server and its matching bot driver")
    report = {"version": util.VERSION, "label": args.label, "ok": False,
        "binaries": binaries, "server_info": info["output"], "rooms": args.rooms,
        "clients_per_room": args.clients, "races": args.races, "timeout_seconds_per_level": args.seconds,
        "transport": "loopback development UDP", "scheduling": "nice 10",
        "limitations": ["Local bot CPU is reported separately; this is not an external load-generator capacity certification.",
            "Tick mean/max describe server step work, not complete receive-loop cost or scheduling lateness.",
            "Transport loss/delay injection, tail quantiles, Internet and QUIC/TLS remain separate checks.",
            "Raw private reports are retained; no report-retention deletion is automatic."],
        "levels": [], "mutation": "isolated test processes and private reports only"}
    if not args.run:
        report.update(ok=True, mutation=False)
        util.emit(report)
        return 0
    previous = {}
    def interrupted(signum, frame):
        raise KeyboardInterrupt
    for signum in (signal.SIGINT, signal.SIGTERM):
        previous[signum] = signal.signal(signum, interrupted)
    try:
        with util.coordination_lock():
            check_reserve(args)
            util.private_dir(util.STATE)
            directory = Path(tempfile.mkdtemp(prefix="server-load-", dir=util.STATE))
            report.update(started=time.strftime("%Y-%m-%dT%H:%M:%S%z"), report_directory=str(directory))
            for count in args.rooms:
                level_dir = directory / f"rooms-{count}"
                level_dir.mkdir(mode=0o700)
                print(f"dev-server-load: testing {count} rooms with {args.clients} players each", file=sys.stderr, flush=True)
                result = run_level(args, count, level_dir, binaries)
                report["levels"].append(result)
                util.report_save("server-load-benchmark", report)
                if not result["ok"]:
                    break
            report["ok"] = len(report["levels"]) == len(args.rooms) and all(r["ok"] for r in report["levels"])
            if any(identity(binaries[k]["path"]) != binaries[k] for k in binaries):
                report.update(ok=False, error="A benchmark binary changed during the run")
    except KeyboardInterrupt:
        report.update(ok=False, error="Interrupted; isolated test processes stopped")
    finally:
        for signum, handler in previous.items():
            signal.signal(signum, handler)
        if "started" in report:
            report["completed"] = time.strftime("%Y-%m-%dT%H:%M:%S%z")
            util.report_save("server-load-benchmark", report)
    util.emit(report)
    return 0 if report["ok"] else 2


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    action = parser.add_mutually_exclusive_group()
    action.add_argument("--plan", action="store_true", help="Show configuration without launching rooms (default)")
    action.add_argument("--run", action="store_true", help="Run bounded isolated room tests")
    parser.add_argument("--server", default=str(util.HOME / "blueengine/spooky-kart-server"))
    parser.add_argument("--bots", default=str(util.HOME / "SpookyKart/target/release/spooky-kart-bots"))
    parser.add_argument("--rooms", type=int, nargs="+", choices=(1, 2, 4, 8), default=[1, 2, 4, 8])
    parser.add_argument("--clients", type=int, choices=range(1, 9), default=8)
    parser.add_argument("--races", type=int, choices=range(1, 11), default=1)
    parser.add_argument("--seconds", type=int, default=180)
    parser.add_argument("--label", choices=("deployed", "baseline", "candidate"), default="deployed")
    parser.add_argument("--min-free-gib", type=util.nonnegative, default=20)
    parser.add_argument("--memory-reserve-gib", type=util.nonnegative, default=2)
    args = parser.parse_args(argv)
    if not 1 <= args.seconds <= 1800 or len(args.rooms) != len(set(args.rooms)):
        parser.error("seconds must be 1..1800 and room levels must be distinct")
    try:
        return run(args)
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        print(f"dev-server-load: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
