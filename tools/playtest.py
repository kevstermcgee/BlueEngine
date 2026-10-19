#!/usr/bin/env python3
"""Bounded runs/replay of game-owned headless adapters. Build once, isolate every seed."""
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


def execute(binary, command, row, memory, seconds, deadline, folder):
    """Common process/evidence boundary; game rules and actions stay in the adapter."""
    write(folder / 'run.json', row)  # Seed/command survives timeout, OOM or interruption.
    remaining = min(seconds, deadline - time.monotonic())
    if remaining <= 0:
        return {**row, 'ok': False, 'failure': 'total time budget exhausted'}
    streamed = folder / 'streamed-checkpoints'
    streamed.mkdir()
    try:
        done = subprocess.run([sys.executable, str(Path(__file__).resolve()), '--worker', str(memory), *command],
                              cwd=ROOT, capture_output=True, text=True, timeout=remaining,
                              env={**os.environ, 'BE2_PLAYTEST_EVIDENCE_DIR': str(streamed)})
        (folder / 'stderr.log').write_text(done.stderr, encoding='utf-8')
        try:
            report = json.loads(done.stdout)
        except ValueError:
            (folder / 'stdout.log').write_text(done.stdout, encoding='utf-8')
            return {**row, 'ok': False, 'failure': 'missing structured result', 'returncode': done.returncode}
        if not isinstance(report, dict) or not isinstance(report.get('trace'), dict) or not isinstance(report.get('ok'), bool):
            (folder / 'stdout.log').write_text(done.stdout, encoding='utf-8')
            return {**row, 'ok': False, 'failure': 'invalid structured result', 'returncode': done.returncode}
        if report['ok'] and (type(report.get('ticks')) is not int or not 1 <= report['ticks'] <= 100000
                             or ('ticks_budget' in row and report['ticks'] != row['ticks_budget'])
                             or not isinstance(report.get('hash'), str) or len(report['hash']) != 16
                             or any(c not in '0123456789abcdefABCDEF' for c in report['hash'])):
            (folder / 'stdout.log').write_text(done.stdout, encoding='utf-8')
            return {**row, 'ok': False, 'failure': 'successful result lacks bounded ticks or authoritative hash',
                    'returncode': done.returncode}
        trace = report.pop('trace')
        write(folder / 'trace.json', trace)
        # Keep the original framed saves independently of the summary, including on a failed run.
        for index, checkpoint in enumerate(report.get('checkpoints', [])):
            for field, name in [('original_hex', 'original'), ('migrated_hex', 'migrated')]:
                if checkpoint.get(field):
                    try:
                        data = bytes.fromhex(checkpoint[field])
                    except (ValueError, TypeError):
                        write(folder / 'result.json', report)
                        return {**row, 'ok': False, 'failure': 'invalid checkpoint evidence', 'returncode': done.returncode}
                    checkpoint_folder = folder / 'checkpoints' / str(index)
                    checkpoint_folder.mkdir(parents=True, exist_ok=True)
                    (checkpoint_folder / f'{name}.be2save').write_bytes(data)
                    write(checkpoint_folder / 'metadata.json', {k: v for k, v in checkpoint.items() if not k.endswith('_hex')})
        write(folder / 'result.json', report)
        # Large checkpoint bytes remain in each result, not duplicated into the aggregate.
        summary = {k: v for k, v in report.items() if k != 'checkpoints'}
        summary['checkpoint_count'] = len(report.get('checkpoints', []))
        summary['trace_sha256'] = hashlib.sha256(json.dumps(trace, sort_keys=True).encode()).hexdigest()
        return {**row, **summary, 'ok': done.returncode == 0 and report['ok'], 'returncode': done.returncode}
    except subprocess.TimeoutExpired as error:
        (folder / 'stdout.log').write_bytes(error.stdout or b'')
        (folder / 'stderr.log').write_bytes(error.stderr or b'')
        return {**row, 'ok': False, 'failure': 'run time budget exhausted; rerun using recorded command'}


def run_one(binary, seed, ticks, memory, seconds, deadline, output, fault=False):
    folder = output / str(seed)
    folder.mkdir()
    command = [str(binary), 'playtest', str(seed), str(ticks)] + (['fault'] if fault else [])
    replay = [str(binary), 'replay-playtest', str(folder / 'trace.json')]
    row = {'seed': seed, 'ticks_budget': ticks, 'memory_mib': memory,
           'run_command': shlex.join(command), 'replay_command': shlex.join(replay)}
    return execute(binary, command, row, memory, seconds, deadline, folder)


def run_batch(binary, seeds, ticks, memory, seconds, deadline, output, concurrency, fault=False):
    with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as pool:
        jobs = [pool.submit(run_one, binary, seed, ticks, memory, seconds, deadline, output, fault) for seed in seeds]
        return sorted((job.result() for job in jobs), key=lambda row: row['seed'])


def gameplay_signature(row):
    return {key: row.get(key) for key in ['seed', 'ok', 'ticks', 'hash', 'failure', 'coverage', 'trace_sha256']}


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if argv and argv[0] == '--worker':
        worker(argv[1:])
        return 0
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, required=True, help='Fresh game-owned headless adapter; see docs/PLAYTESTING.md')
    p.add_argument('--seeds', default='0,1,2,3,4,5,6,7')
    p.add_argument('--ticks', type=int, default=10000)
    p.add_argument('--concurrency', type=int, default=2)
    p.add_argument('--memory-mib', type=int, default=128, help='Per-process address-space bound (Linux)')
    p.add_argument('--seconds', type=float, default=30, help='Per-run wall time')
    p.add_argument('--total-seconds', type=float, default=120)
    p.add_argument('--out', type=Path, required=True, help='New evidence directory, never overwrite prior evidence')
    p.add_argument('--fault-fixture', action='store_true', help='Ask the game adapter to inject its deliberate detector fixture')
    p.add_argument('--compare-serial', action='store_true', help='Rerun the same seeds serially and compare authoritative hashes/actions')
    p.add_argument('--replay-trace', type=Path, help='Replay a retained trace under the same process budgets, instead of generating seeds')
    args = p.parse_args(argv)
    if args.replay_trace and (args.compare_serial or args.fault_fixture):
        p.error('Replay uses the original trace fault/actions; do not combine with generation options')
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
    if args.replay_trace:
        args.replay_trace = args.replay_trace.resolve(strict=True)
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
                'binary': str(binary), 'source_identity': engine_identity(ROOT),
                'concurrency': 1 if args.replay_trace else args.concurrency, 'seeds': None if args.replay_trace else seeds,
                'ticks_per_run': None if args.replay_trace else args.ticks, 'memory_mib_per_run': args.memory_mib,
                'seconds_per_run': args.seconds, 'total_seconds': args.total_seconds,
                'compare_serial': args.compare_serial, 'replay_trace': str(args.replay_trace) if args.replay_trace else None}
    write(args.out / 'inputs.json', metadata)
    deadline = started + args.total_seconds
    comparison = None
    if args.replay_trace:
        folder = args.out / 'replay'
        folder.mkdir()
        # Copy a bounded immutable input before execution so subsequent source-file edits cannot alter replay.
        with args.replay_trace.open('rb') as source:
            trace = source.read(64 * 1024 * 1024 + 1)
        if len(trace) > 64 * 1024 * 1024:
            p.error('Trace exceeds 64 MiB')
        retained = folder / 'input-trace.json'
        retained.write_bytes(trace)
        command = [str(binary), 'replay-playtest', str(retained)]
        row = {'run_command': shlex.join(command), 'input_trace_sha256': hashlib.sha256(trace).hexdigest()}
        rows = [execute(binary, command, row, args.memory_mib, args.seconds, deadline, folder)]
    else:
        rows = run_batch(binary, seeds, args.ticks, args.memory_mib, args.seconds, deadline,
                         args.out, args.concurrency, args.fault_fixture)
        if args.compare_serial:
            serial_folder = args.out / 'serial'
            serial_folder.mkdir()
            serial = run_batch(binary, seeds, args.ticks, args.memory_mib, args.seconds, deadline,
                               serial_folder, 1, args.fault_fixture)
            comparison = {'ok': [gameplay_signature(row) for row in rows] == [gameplay_signature(row) for row in serial],
                          'serial_runs': serial}
            write(args.out / 'comparison.json', comparison)
    report = {**metadata, 'ok': all(row['ok'] for row in rows) and (comparison is None or comparison['ok']), 'runs': rows,
              'serial_parallel_equal': comparison['ok'] if comparison else None,
              'elapsed_seconds': round(time.monotonic() - started, 3),
              'claims': 'Behavioral simulation evidence only; no rendering, physical input, audible output or enjoyment claim'}
    write(args.out / 'report.json', report)
    print(json.dumps({k: v for k, v in report.items() if k != 'runs'}))
    return 0 if report['ok'] else 1


if __name__ == '__main__':
    sys.exit(main())
