#!/usr/bin/env python3
"""Verify an engine fix separately from a game run, requiring a failing baseline regression."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def regression(root, suite, name, log):
    command = ['cargo', 'test', '--locked', '--profile', 'itest',
               '--test', suite, name, '--', '--exact']
    with log.open('w') as output:
        result = subprocess.run(command, cwd=root, stdout=output, stderr=subprocess.STDOUT,
                                env={**os.environ, 'CARGO_TARGET_DIR': str(ROOT / 'target')})
    text = log.read_text(errors='replace')
    # Compilation failure or zero matching tests is not a reproduced regression.
    counts = re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed', text)
    executed = sum(int(a) + int(b) for a, b in counts)
    return {'exit_code': result.returncode, 'executed': executed, 'log': str(log)}


def verify(base, test):
    if not re.fullmatch(r'[\w./~^{}+-]+', base) or base.startswith('-'):
        raise ValueError('Invalid baseline revision')
    suite, separator, name = test.partition('::')
    if not separator or not re.fullmatch(r'[a-zA-Z0-9_-]+', suite) or not re.fullmatch(r'[a-zA-Z0-9_:]+', name):
        raise ValueError('--test requires SUITE::exact_test')
    test_path = Path('tests') / (suite + '.rs')
    revision = subprocess.check_output(['git', 'rev-parse', '--verify', base + '^{commit}'], cwd=ROOT, text=True).strip()
    changed = subprocess.check_output(['git', 'diff', '--name-only', revision], cwd=ROOT, text=True).splitlines()
    changed += subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard'], cwd=ROOT, text=True).splitlines()
    if test_path.as_posix() not in changed or not (ROOT / test_path).is_file():
        raise ValueError('The fix needs a changed/new integration regression test in tests/SUITE.rs')
    if not any(path.startswith('src/') for path in changed):
        raise ValueError('This separate path is for engine source fixes')
    reports = ROOT / '.be2-work/engine-fixes'
    reports.mkdir(parents=True, exist_ok=True)
    report_dir = Path(tempfile.mkdtemp(prefix='fix-', dir=reports))
    baseline = report_dir / 'baseline'
    subprocess.run(['git', 'worktree', 'add', '--detach', str(baseline), revision], cwd=ROOT, check=True, capture_output=True)
    try:
        # Compare canonical paths on both sides, including Windows short-name aliases.
        fixture_root = (ROOT / 'tests').resolve()
        if not fixture_root.is_relative_to(ROOT.resolve()):
            raise ValueError('Regression fixtures must stay within tests/')
        # Copy the regression and changed test fixtures, never the engine fix.
        for relative in sorted(set(path for path in changed if path.startswith('tests/'))):
            source, destination = ROOT / relative, baseline / relative
            if source.is_symlink() or not source.resolve().is_relative_to(fixture_root):
                raise ValueError('Regression fixtures must stay within tests/')
            if source.is_file():
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, destination)
            elif destination.is_file():
                destination.unlink()
        before = regression(baseline, suite, name, report_dir / 'before.log')
        if before['exit_code'] == 0 or before['executed'] != 1:
            raise ValueError('Regression must execute exactly once and fail against the baseline code; see ' + before['log'])
    finally:
        subprocess.run(['git', 'worktree', 'remove', '--force', str(baseline)], cwd=ROOT, check=True, capture_output=True)
    after = regression(ROOT, suite, name, report_dir / 'after.log')
    if after['exit_code'] or after['executed'] != 1:
        raise ValueError('Regression must execute exactly once and pass with the fix; see ' + after['log'])
    subprocess.run([sys.executable, 'tools/be2.py', 'check'], cwd=ROOT, check=True)
    receipt = {'baseline': revision, 'test': test, 'before': before, 'after': after, 'full_check': True}
    (report_dir / 'report.json').write_text(json.dumps(receipt, indent=2) + '\n')
    return receipt


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', required=True)
    parser.add_argument('--test', required=True)
    args = parser.parse_args()
    try:
        print(json.dumps(verify(args.base, args.test)))
    except (ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, str(error) + '\n')
