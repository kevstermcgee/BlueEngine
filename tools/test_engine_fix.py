"""Separate engine fixes must reproduce a regression before accepting a fix."""
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from tools import engine_fix


class EngineFixTests(unittest.TestCase):
    def fixture(self, root):
        subprocess.run(['git', 'init', '-q', str(root)], check=True)
        for key, value in [('user.name', 'Fixture'), ('user.email', 'fixture@example.invalid')]:
            subprocess.run(['git', '-C', str(root), 'config', key, value], check=True)
        (root / '.gitignore').write_text('.be2-work/\n')
        (root / 'src').mkdir()
        (root / 'src/lib.rs').write_text('baseline')
        subprocess.run(['git', '-C', str(root), 'add', '.'], check=True)
        subprocess.run(['git', '-C', str(root), 'commit', '-qm', 'Baseline'], check=True)
        (root / 'src/lib.rs').write_text('fixed')
        (root / 'tests').mkdir()
        (root / 'tests/regression.rs').write_text('regression')
        (root / 'tests/input.json').write_text('test fixture')

    def test_only_baseline_failure_then_fixed_success_reaches_full_check(self):
        for before_code, executed, accepted, alias in [(1, 1, True, False), (0, 1, False, False),
                                                        (1, 0, False, False), (1, 1, True, True)]:
            with self.subTest(before_code=before_code, executed=executed, alias=alias), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                if alias:
                    # Resolving a valid spelling must preserve containment (also Windows 8.3 names).
                    (root / 'alias').mkdir()
                    root = root / 'alias' / '..'
                self.fixture(root)
                original_run = subprocess.run
                checks = []
                def run(command, **kwargs):
                    if command[:2] == [engine_fix.sys.executable, 'tools/be2.py']:
                        checks.append(command)
                        return subprocess.CompletedProcess(command, 0)
                    return original_run(command, **kwargs)
                def regression(checkout, suite, name, log):
                    if checkout != root:
                        self.assertEqual((checkout / 'src/lib.rs').read_text(), 'baseline')
                        self.assertEqual((checkout / 'tests/regression.rs').read_text(), 'regression')
                        self.assertEqual((checkout / 'tests/input.json').read_text(), 'test fixture')
                        return {'exit_code': before_code, 'executed': executed, 'log': str(log)}
                    return {'exit_code': 0, 'executed': 1, 'log': str(log)}
                with patch.object(engine_fix, 'ROOT', root), patch.object(engine_fix, 'regression', side_effect=regression), patch.object(engine_fix.subprocess, 'run', side_effect=run):
                    if accepted:
                        receipt = engine_fix.verify('HEAD', 'regression::reproduces_failure')
                        self.assertTrue(receipt['full_check'])
                        self.assertEqual(len(checks), 1)
                    else:
                        with self.assertRaisesRegex(ValueError, 'fail against the baseline'):
                            engine_fix.verify('HEAD', 'regression::reproduces_failure')
                        self.assertEqual(checks, [])
                self.assertEqual((root / 'src/lib.rs').read_text(), 'fixed')

    def test_engine_changes_without_a_regression_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            (root / 'tests/regression.rs').unlink()
            with patch.object(engine_fix, 'ROOT', root):
                with self.assertRaisesRegex(ValueError, 'needs a changed/new integration regression'):
                    engine_fix.verify('HEAD', 'regression::test')
