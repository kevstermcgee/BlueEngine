#!/usr/bin/env python3
"""Measure bounded context and automatic check plans, optionally executing one verification set.

No source edits, cache resets, session-log ingestion or external publication. End-to-end
agent measurements must be supplied separately, using development_tasks.json's rubric.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[1]


def measure(root, level=1, execute=None):
    spec = importlib.util.spec_from_file_location('benchmark_workflow', root / 'tools/workflow.py')
    workflow = importlib.util.module_from_spec(spec); spec.loader.exec_module(workflow)
    tasks = json.loads((ROOT / 'tools/development_tasks.json').read_text())['tasks']
    rows = []
    for task in tasks:
        started = time.monotonic()
        if hasattr(workflow, 'feature_map'):
            packet = workflow.context(root, task['query'], level=level)
        else:
            packet = workflow.context(root, task['query'])
        discovery_seconds = time.monotonic() - started
        started = time.monotonic()
        plan = (workflow.change_plan(root, [task['path']]) if hasattr(workflow, 'change_plan') else
                workflow.validation_plan([task['path']]))
        row = {'task': task['id'], 'query': task['query'], 'path': task['path'],
               'context_level': packet.get('level', 2),
               'context_seconds': round(discovery_seconds, 6),
               'context_bytes': len(json.dumps(packet, separators=(',', ':')).encode()),
               'suggested_files': sum(len(m['read_first']) for m in packet['matches']),
               'source_reads_for_lookup': 0, 'plan_seconds': round(time.monotonic() - started, 6),
               'verification_scope': plan['scope'], 'commands': len(plan['commands']),
               'cargo_commands': sum(c[0] == 'cargo' for c in plan['commands']),
               'implementation_seconds': None, 'total_task_seconds': None}
        if execute == task['id']:
            # Execute via the existing runner for identical diagnostics/logs/test-evidence checks.
            runner_spec = importlib.util.spec_from_file_location('benchmark_runner', root / 'tools/be2.py')
            previous = {name: sys.modules.get(name) for name in ('workflow', 'upgrade')}
            sys.path.insert(0, str(root / 'tools')); sys.modules['workflow'] = workflow
            try:
                runner = importlib.util.module_from_spec(runner_spec); runner_spec.loader.exec_module(runner)
                started = time.monotonic()
                try:
                    runner.check(plan)
                    row['verification_ok'] = True
                except runner.CommandFailure as error:
                    row.update(verification_ok=False, failure=error.packet)
                row['verification_seconds'] = round(time.monotonic() - started, 3)
            finally:
                sys.path.pop(0)
                for name, module in previous.items():
                    if module is None: sys.modules.pop(name, None)
                    else: sys.modules[name] = module
        rows.append(row)
    revision = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=root, capture_output=True, text=True, check=True).stdout.strip()
    return {'schema_version': 1, 'revision': revision, 'root': str(root),
            'measurement': 'Context and verification tools only; missing agent task timings are null.', 'rows': rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=ROOT, help='Checkout to measure (supports pre-change workflow)')
    parser.add_argument('--level', type=int, choices=(1, 2, 3), default=1)
    parser.add_argument('--execute', choices=[t['id'] for t in json.loads((ROOT / 'tools/development_tasks.json').read_text())['tasks']],
                        help='Actually run one verification set; never seven repeated full suites')
    parser.add_argument('--out', type=Path, help='Write JSON measurements')
    args = parser.parse_args()
    result = measure(args.root.resolve(), args.level, args.execute)
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))
    if any(r.get('verification_ok') is False for r in result['rows']): sys.exit(1)


if __name__ == '__main__': main()
