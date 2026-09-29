#!/usr/bin/env python3
"""Record and compare BlueEngine build/test/runtime performance.

Rows are appended to docs/perf/metrics.jsonl (one JSON object per line, never rewritten) so a
later session can compare like with like: same metric, profile, kind and host, across commits.

  python tools/perf.py record [--suite build|test|sim|server|all] [--profile fast|release|itest|dev] [--note TEXT]
  python tools/perf.py report [--metric NAME]     latest value per series vs the previous one
  python tools/perf.py env                        the host/toolchain block a row would carry

`--suite server` starts be2-headless on loopback and joins 1, 2, 4 and 8 synthetic clients (plus a 9th
to confirm refusal: the server is capped at 8) using examples/server_load.rs, recording server CPU,
memory, tick time and per-client bandwidth. `all` does not include it (about 3 minutes).

Measurements build in a private target directory (BLUE_PERF_TARGET, default
~/.cache/blueengine-perf) so they never disturb your own target/. Incremental rows edit
src/viewer/lint.rs with a throwaway function and restore the original bytes afterwards.
"""
import argparse
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
METRICS = ROOT / 'docs' / 'perf' / 'metrics.jsonl'
PROBE_FILE = ROOT / 'src' / 'viewer' / 'lint.rs'
PROBE = '\n#[allow(dead_code)]\npub fn __perf_probe() -> u64 { 0x%x }\n'
HEADLESS = ['--no-default-features', '--bin', 'be2-headless']


def run(cmd, **kw):
    return subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, **kw)


def sh_out(cmd):
    try:
        return run(cmd).stdout.strip()
    except OSError:
        return ''


def environment():
    cpu = ''
    try:
        cpu = re.search(r'model name\s*:\s*(.+)', Path('/proc/cpuinfo').read_text()).group(1)
    except (OSError, AttributeError):
        cpu = platform.processor()
    cfg = Path.home() / '.cargo' / 'config.toml'
    cfg_text = cfg.read_text() if cfg.exists() else ''
    return {
        'commit': sh_out(['git', 'rev-parse', '--short', 'HEAD']),
        'dirty': bool(sh_out(['git', 'status', '--porcelain', '--untracked-files=no'])),
        'cpu': cpu, 'cores': os.cpu_count(),
        'os': platform.platform(),
        'rustc': sh_out(['rustc', '--version']),
        'linker': 'mold' if 'mold' in cfg_text else ('lld' if 'lld' in cfg_text else 'default'),
        'sccache': bool(shutil.which('sccache')) and 'sccache' in cfg_text + os.environ.get('RUSTC_WRAPPER', ''),
    }


def target_dir(tag):
    base = Path(os.environ.get('BLUE_PERF_TARGET', Path.home() / '.cache' / 'blueengine-perf'))
    return base / tag


def append(rows, note):
    METRICS.parent.mkdir(parents=True, exist_ok=True)
    env = environment()
    stamp = datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
    with METRICS.open('a', encoding='utf-8') as f:
        for row in rows:
            row = {'ts': stamp, **env, **row}
            if note:
                row['note'] = note
            f.write(json.dumps(row, sort_keys=True) + '\n')
            print(f"{row['metric']:<28} {row['value']:>10} {row['unit']:<6} "
                  f"profile={row.get('profile', '-')} kind={row.get('kind', '-')}")


def profile_args(profile):
    return ['--release'] if profile == 'release' else ([] if profile == 'dev' else ['--profile', profile])


def timed(cmd, env):
    start = time.monotonic()
    result = run(cmd, env=env)
    return round(time.monotonic() - start, 2), result


def build_rows(profile):
    tag = f'build-{profile}'
    tdir = target_dir(tag)
    shutil.rmtree(tdir, ignore_errors=True)
    env = {**os.environ, 'CARGO_TARGET_DIR': str(tdir)}
    cmd = ['cargo', 'build', '--locked', *profile_args(profile), *HEADLESS]
    secs, res = timed(cmd, env)
    if res.returncode:
        sys.exit(res.stderr[-2000:])
    rows = [{'metric': 'build_headless', 'kind': 'cold', 'profile': profile, 'value': secs, 'unit': 's'}]
    original = PROBE_FILE.read_bytes()
    try:
        PROBE_FILE.write_bytes(original + (PROBE % time.time_ns()).encode())
        secs, res = timed(cmd, env)
    finally:
        PROBE_FILE.write_bytes(original)
    if res.returncode:
        sys.exit(res.stderr[-2000:])
    rows.append({'metric': 'build_headless', 'kind': 'incremental_edit', 'profile': profile,
                 'value': secs, 'unit': 's'})
    return rows


def test_rows(profile):
    tdir = target_dir(f'test-{profile}')
    shutil.rmtree(tdir, ignore_errors=True)
    env = {**os.environ, 'CARGO_TARGET_DIR': str(tdir)}
    base = ['cargo', 'test', '--locked', *(['--profile', profile] if profile != 'dev' else []),
            '--no-default-features']
    secs, res = timed(base + ['--no-run'], env)
    if res.returncode:
        sys.exit(res.stderr[-2000:])
    rows = [{'metric': 'test_build', 'kind': 'cold', 'profile': profile, 'value': secs, 'unit': 's'}]
    secs, res = timed(base, env)
    rows.append({'metric': 'test_run_total', 'kind': 'warm', 'profile': profile, 'value': secs,
                 'unit': 's', 'passed': res.returncode == 0})
    # Per-suite wall time: the slow suites are where iteration time goes.
    for m in re.finditer(r'Running (?:unittests |tests/)?(\S+?)(?:\.rs)? \(.*?\n(?:.*?\n)*?'
                         r'test result: .*? finished in ([\d.]+)s', res.stdout):
        name, t = m.group(1), float(m.group(2))
        if t >= 1.0:
            rows.append({'metric': f'test_suite:{name}', 'kind': 'warm', 'profile': profile,
                         'value': t, 'unit': 's'})
    return rows


def sim_rows(profile):
    tdir = target_dir(f'build-{profile}')
    exe = tdir / ('release' if profile == 'release' else profile) / 'be2-headless'
    if not exe.exists():
        sys.exit(f'{exe} missing: run `perf.py record --suite build --profile {profile}` first')
    res = run([str(exe), '--ticks', '36000'])
    m = re.search(r'mean_tick_us=([\d.]+)', res.stdout + res.stderr)
    if not m:
        sys.exit('could not parse be2-headless output')
    return [{'metric': 'sim_mean_tick', 'kind': 'runtime_2_players', 'profile': profile,
             'value': float(m.group(1)), 'unit': 'us'}]


LOAD_LEVELS = (1, 2, 4, 8, 9)
LOAD_SECONDS = 15


def cpu_seconds(pid):
    """utime+stime of a process in seconds, from /proc."""
    fields = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
    return (int(fields[11]) + int(fields[12])) / os.sysconf('SC_CLK_TCK')


def rss_peak_mb(pid):
    match = re.search(r'VmHWM:\s+(\d+) kB', Path(f'/proc/{pid}/status').read_text())
    return round(int(match.group(1)) / 1024, 1) if match else None


def server_rows(profile):
    tdir = target_dir(f'build-{profile}')
    env = {**os.environ, 'CARGO_TARGET_DIR': str(tdir)}
    exe = tdir / ('release' if profile == 'release' else profile) / 'be2-headless'
    build = run(['cargo', 'build', '--locked', *profile_args(profile), '--no-default-features',
                 '--bin', 'be2-headless', '--example', 'server_load'], env=env)
    if build.returncode:
        sys.exit(build.stderr[-2000:])
    load = exe.parent / 'examples' / 'server_load'
    rows = []
    for index, clients in enumerate(LOAD_LEVELS):
        port = 41000 + index
        log = target_dir('server-logs') / f'server-{clients}.log'
        log.parent.mkdir(parents=True, exist_ok=True)
        with log.open('w') as out:
            server = subprocess.Popen([str(exe), '--server', f'127.0.0.1:{port}', '--transport', 'development'],
                                      cwd=ROOT, stdout=out, stderr=subprocess.STDOUT)
            try:
                time.sleep(1.0)
                before, wall = cpu_seconds(server.pid), time.monotonic()
                res = run([str(load), f'127.0.0.1:{port}', '--clients', str(clients),
                           '--seconds', str(LOAD_SECONDS)])
                busy = cpu_seconds(server.pid) - before
                window = time.monotonic() - wall
                rss = rss_peak_mb(server.pid)
            finally:
                server.terminate()
                server.wait(timeout=10)
        if res.returncode:
            sys.exit(f'server_load failed at {clients} clients: {res.stderr[-1000:]}')
        report = json.loads(res.stdout.strip().splitlines()[-1])
        status = re.findall(r'mean_us=(\d+) max_us=(\d+)', log.read_text())
        kind = f'clients_{clients}'
        common = {'kind': kind, 'profile': profile}
        rows += [
            {'metric': 'server_cpu_pct_of_core', 'value': round(100 * busy / window, 2), 'unit': '%', **common},
            {'metric': 'server_rss_peak', 'value': rss, 'unit': 'MB', **common},
            {'metric': 'server_clients_joined', 'value': report['clients_joined'], 'unit': 'n',
             'refused': report['clients_refused'], 'refusal_reason': report['refusal_reason'], **common},
        ]
        if status:
            rows += [{'metric': 'server_tick_mean', 'value': int(status[-1][0]), 'unit': 'us', **common},
                     {'metric': 'server_tick_max', 'value': int(status[-1][1]), 'unit': 'us', **common}]
        for key, unit in (('bytes_per_client_per_s', 'B/s'), ('max_packet_bytes', 'B'),
                          ('update_gap_ms_mean', 'ms'), ('update_gap_ms_max', 'ms'),
                          ('update_gaps_over_100ms', 'n'), ('resyncs', 'n'),
                          ('generator_late_frames', 'n')):
            rows.append({'metric': 'load_' + key, 'value': report[key], 'unit': unit, **common})
    return rows


def load():
    if not METRICS.exists():
        return []
    return [json.loads(line) for line in METRICS.read_text(encoding='utf-8').splitlines() if line.strip()]


def report(metric):
    series = {}
    for row in load():
        if metric and row['metric'] != metric:
            continue
        key = (row['metric'], row.get('kind'), row.get('profile'), row.get('cpu'), row.get('linker'))
        series.setdefault(key, []).append(row)
    for key, rows in sorted(series.items(), key=lambda kv: str(kv[0])):
        last = rows[-1]
        prev = f"  (prev {rows[-2]['value']} @ {rows[-2]['commit']})" if len(rows) > 1 else ''
        print(f"{key[0]:<28} {key[1]:<18} {key[2]:<8} linker={key[4]:<8} "
              f"{last['value']:>9} {last['unit']} @ {last['commit']}{prev}")


def main(argv):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest='cmd', required=True)
    r = sub.add_parser('record')
    r.add_argument('--suite', choices=['build', 'test', 'sim', 'server', 'all'], default='all')
    r.add_argument('--profile', choices=['fast', 'release', 'itest', 'dev'], default=None)
    r.add_argument('--note', default='')
    rp = sub.add_parser('report')
    rp.add_argument('--metric')
    sub.add_parser('env')
    args = p.parse_args(argv)
    if args.cmd == 'env':
        print(json.dumps(environment(), indent=2))
    elif args.cmd == 'report':
        report(args.metric)
    else:
        rows = []
        if args.suite in ('build', 'all'):
            rows += build_rows(args.profile if args.profile in ('fast', 'release') else 'fast')
        if args.suite in ('test', 'all'):
            rows += test_rows(args.profile if args.profile in ('itest', 'dev') else 'itest')
        if args.suite in ('sim', 'all'):
            rows += sim_rows(args.profile if args.profile in ('fast', 'release') else 'fast')
        if args.suite == 'server':
            rows += server_rows(args.profile if args.profile in ('fast', 'release') else 'fast')
        append(rows, args.note)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
