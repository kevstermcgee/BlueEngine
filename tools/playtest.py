#!/usr/bin/env python3
"""Bounded parallel runs of the real headless workshop. Build once, isolate every seed."""
import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import time
ROOT = Path(__file__).resolve().parents[1]
try:
    from tools.task_inputs import engine_identity
except ImportError:
    from task_inputs import engine_identity


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')


def worker(args):
    # A single-threaded child applies its own limit before exec; no preexec_fn in threads.
    import resource
    memory, binary, *command = args
    limit = int(memory) * 1024 * 1024
    resource.setrlimit(resource.RLIMIT_AS, (limit, limit))
    os.execv(binary, [binary, *command])


def run_one(binary, seed, ticks, memory, seconds, deadline, output, fault=False):
    folder = output / str(seed)
    folder.mkdir()
    command = [str(binary), 'playtest', str(seed), str(ticks)] + (['fault'] if fault else [])
    replay = [str(binary), 'replay-playtest', str(folder / 'trace.json')]
    row = {'seed': seed, 'ticks_budget': ticks, 'memory_mib': memory,
           'run_command': shlex.join(command), 'replay_command': shlex.join(replay)}
    write(folder / 'run.json', row)  # Seed/command survives timeout, OOM or interruption.
    remaining = min(seconds, deadline - time.monotonic())
    if remaining <= 0:
        return {**row, 'ok': False, 'failure': 'total time budget exhausted'}
    try:
        done = subprocess.run([sys.executable, str(Path(__file__).resolve()), '--worker', str(memory), *command],
                              cwd=ROOT, capture_output=True, text=True, timeout=remaining)
        (folder / 'stderr.log').write_text(done.stderr, encoding='utf-8')
        try:
            report = json.loads(done.stdout)
        except ValueError:
            (folder / 'stdout.log').write_text(done.stdout, encoding='utf-8')
            return {**row, 'ok': False, 'failure': 'missing structured result', 'returncode': done.returncode}
        write(folder / 'trace.json', report.pop('trace'))
        write(folder / 'result.json', report)
        return {**row, **report, 'ok': done.returncode == 0 and report['ok'], 'returncode': done.returncode}
    except subprocess.TimeoutExpired:
        return {**row, 'ok': False, 'failure': 'run time budget exhausted; replay using recorded seed command'}


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if argv and argv[0] == '--worker':
        worker(argv[1:])
        return 0
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, required=True, help='Fresh headless workshop binary; see docs/PLAYTESTING.md')
    p.add_argument('--seeds', default='0,1,2,3,4,5,6,7')
    p.add_argument('--ticks', type=int, default=10000)
    p.add_argument('--concurrency', type=int, default=2)
    p.add_argument('--memory-mib', type=int, default=128, help='Per-process address-space bound (Linux)')
    p.add_argument('--seconds', type=float, default=30, help='Per-run wall time')
    p.add_argument('--total-seconds', type=float, default=120)
    p.add_argument('--out', type=Path, required=True, help='New evidence directory, never overwrite prior evidence')
    p.add_argument('--fault-fixture', action='store_true', help='Inject duplicate gathering to test the oracle')
    args = p.parse_args(argv)
    try:
        seeds = sorted(int(s) for s in args.seeds.split(','))
        if len(seeds) != len(set(seeds)) or not 1 <= len(seeds) <= 256 or any(s < 0 or s >= 2**64 for s in seeds):
            p.error('Use 1..256 unique u64 seeds')
    except ValueError:
        p.error('Seeds must be comma-separated u64 integers')
    if not 22 <= args.ticks <= 100000 or not 1 <= args.concurrency <= 4 or not 32 <= args.memory_mib <= 4096:
        p.error('Budgets: ticks 22..100000, concurrency 1..4, memory MiB 32..4096')
    if not 0 < args.seconds <= 3600 or not 0 < args.total_seconds <= 14400:
        p.error('Time budgets must be positive: per-run <=3600s, total <=14400s')
    if sys.platform != 'linux':
        p.error('Address-space enforcement currently requires Linux; native game delivery remains Windows x64')
    binary = args.binary.resolve(strict=True)
    args.out = args.out.resolve()
    if args.out.is_relative_to(ROOT):
        ignored = subprocess.run(['git', '-C', str(ROOT), 'check-ignore', '-q', str(args.out)], check=False)
        if ignored.returncode:
            p.error('Output must be outside source or in an ignored evidence directory')
    args.out.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    identity = subprocess.run(['git', '-C', str(ROOT), 'rev-parse', 'HEAD'], capture_output=True, text=True, check=True).stdout.strip()
    source = subprocess.run(['git', '-C', str(ROOT), 'diff', 'HEAD', '--binary'], capture_output=True, check=True).stdout
    metadata = {'revision': identity, 'working_diff_sha256': hashlib.sha256(source).hexdigest(),
                'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                'binary': str(binary), 'source_identity': engine_identity(ROOT), 'concurrency': args.concurrency, 'seeds': seeds,
                'ticks_per_run': args.ticks, 'memory_mib_per_run': args.memory_mib,
                'seconds_per_run': args.seconds, 'total_seconds': args.total_seconds}
    write(args.out / 'inputs.json', metadata)
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        jobs = [pool.submit(run_one, binary, seed, args.ticks, args.memory_mib, args.seconds,
                            started + args.total_seconds, args.out, args.fault_fixture) for seed in seeds]
        rows = sorted((job.result() for job in jobs), key=lambda row: row['seed'])
    report = {**metadata, 'ok': all(row['ok'] for row in rows), 'runs': rows,
              'elapsed_seconds': round(time.monotonic() - started, 3),
              'claims': 'Behavioral simulation evidence only; no rendering, physical input, audible output or enjoyment claim'}
    write(args.out / 'report.json', report)
    print(json.dumps({k: v for k, v in report.items() if k != 'runs'}))
    return 0 if report['ok'] else 1


if __name__ == '__main__':
    sys.exit(main())
