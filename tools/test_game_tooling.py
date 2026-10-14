"""Public project entry points, pinned distribution, safe migration and hooks."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from tools import game_tooling as tooling


class SharedToolingTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix='be-tooling-')
        self.addCleanup(self.scratch.cleanup)
        self.game = Path(self.scratch.name) / 'game spaced café 雪'
        (self.game / 'scripts').mkdir(parents=True)
        self.engine = tooling.ROOT
        for command in ('check', 'dev', 'ship', 'project'):
            shutil.copy(self.engine / f'templates/game_{command}.py', self.game / f'scripts/{command}.py')

    def test_migration_and_wrong_cwd_leave_game_owned_content_and_hooks(self):
        hook = self.game / 'scripts/tooling_hooks.py'
        hook.write_text('def prepare_package(project): pass\n')
        content = self.game / 'assets'
        content.mkdir()
        (content / 'art.txt').write_text('original artwork')
        tooling.migrate(self.game)
        tooling.check(self.game)
        self.assertEqual(hook.read_text(), 'def prepare_package(project): pass\n')
        self.assertEqual((content / 'art.txt').read_text(), 'original artwork')
        for command in ('check', 'ship'):
            done = subprocess.run([sys.executable, str(self.game / f'scripts/{command}.py'), '--help'],
                                  cwd=self.scratch.name, capture_output=True, text=True)
            self.assertEqual(done.returncode, 0, done.stderr)
        self.assertLess((self.game / 'scripts/ship.py').stat().st_size, 900)

    def test_unknown_customization_is_rejected_before_writes(self):
        destination = self.game / 'scripts/ship.py'
        destination.write_text('# intentionally different package validator\n')
        before = {p.name: p.read_bytes() for p in (self.game / 'scripts').iterdir()}
        with self.assertRaisesRegex(ValueError, 'unreviewed customization'):
            tooling.migrate(self.game)
        self.assertEqual(before, {p.name: p.read_bytes() for p in (self.game / 'scripts').iterdir()})

    def test_override_and_pin_fail_before_execution_or_output_replacement(self):
        tooling.migrate(self.game)
        preserved = self.game / 'dist/completed.txt'
        preserved.parent.mkdir()
        preserved.write_text('completed package')
        env = dict(os.environ, BE2_TOOLING_ROOT=str(self.game))
        done = subprocess.run([sys.executable, str(self.game / 'scripts/ship.py'), 'info', '--json'],
                              cwd=self.scratch.name, env=env, capture_output=True, text=True)
        self.assertNotEqual(done.returncode, 0)
        self.assertIn('Restore the engine checkout', done.stderr)
        self.assertEqual(preserved.read_text(), 'completed package')
        pin = self.game / 'scripts/tooling.json'
        recorded = json.loads(pin.read_text())
        recorded['sha256'] = '0' * 64
        pin.write_text(json.dumps(recorded))
        with self.assertRaisesRegex(ValueError, 'stale tooling pin'):
            tooling.check(self.game)

    def test_consumer_check_accepts_hook_but_detects_launcher_drift(self):
        tooling.migrate(self.game)
        (self.game / 'scripts/tooling_hooks.py').write_text('def prepare_package(project): pass\n')
        tooling.check(self.game)
        path = self.game / 'scripts/dev.py'
        path.write_text(path.read_text() + '# accidental copied implementation\n')
        with self.assertRaisesRegex(ValueError, 'drift'):
            tooling.check(self.game)

    def test_binary_override_requires_fresh_cargo_artifact_identity(self):
        binary = self.game / 'fresh tool'
        binary.write_bytes(b'verified source artifact')
        explicit = self.game / 'override café'
        explicit.write_bytes(binary.read_bytes())
        cargo = subprocess.CompletedProcess([], 0, json.dumps({'reason': 'compiler-artifact',
            'target': {'name': 'be2-tools'}, 'executable': str(binary)}) + '\n', '')
        with patch.object(tooling.subprocess, 'run', return_value=cargo) as invoke:
            self.assertEqual(tooling.fresh_tools(self.engine, explicit), str(explicit))
            self.assertEqual(invoke.call_args.kwargs['cwd'], self.engine)
            self.assertIn('--locked', invoke.call_args.args[0])
            explicit.write_bytes(b'stale artifact')
            with self.assertRaisesRegex(ValueError, 'incompatible or stale'):
                tooling.fresh_tools(self.engine, explicit)
        with patch.object(tooling.subprocess, 'run', return_value=subprocess.CompletedProcess([], 1, '', 'error')):
            with contextlib.redirect_stderr(io.StringIO()), self.assertRaisesRegex(ValueError, 'freshness'):
                tooling.fresh_tools(self.engine)

    def test_hook_failure_preserves_completed_package(self):
        ship = tooling.module_from(self.engine / 'templates/game_ship.py', 'be2_hook_test_ship')
        (self.game / 'scripts/tooling_hooks.py').write_text('def prepare_package(project):\n    raise RuntimeError("render failed")\n')
        project = type('Project', (), {'root': self.game, 'identity': object()})()
        with patch.object(ship, 'require_icon'), patch.object(ship, 'cargo_build_release') as build:
            with self.assertRaisesRegex(ship.ShipError, 'render failed.*preserved'):
                ship.cmd_package(project)
            build.assert_not_called()

    def test_windows_process_paths_are_data_not_powershell_source(self):
        dev = tooling.module_from(self.engine / 'templates/game_dev.py', 'be2_dev_test')
        exe = self.game / "name'$(anything).exe"
        with patch.object(dev.sys, 'platform', 'win32'), patch.object(dev.subprocess, 'run') as invoke:
            dev.close_running(exe)
        command = invoke.call_args.args[0]
        self.assertNotIn(str(exe), command[-1])
        self.assertEqual(invoke.call_args.kwargs['env']['BLUEENGINE_DEV_EXE'], str(exe))

    def test_all_migrated_consumers_use_current_pin(self):
        inventory = json.loads((self.engine / 'tools/game_tooling_inventory.json').read_text())
        for item in inventory['projects']:
            with self.subTest(game=item['game']):
                tooling.check(self.engine / item['game'])


if __name__ == '__main__':
    unittest.main()
