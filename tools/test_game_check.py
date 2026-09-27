"""Exercise the shipped game runner's failure behavior and validation boundary."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('game_check', Path(__file__).resolve().parents[1] /
                                              'templates/game_check.py')
game_check = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(game_check)


class GameCheckTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        (self.root / 'game.json').write_text(json.dumps({'map': 'maps/main.json'}))
        (self.root / 'maps').mkdir()
        (self.root / 'maps/main.json').write_text('{}')
        self.native = self.root / 'native tool'
        self.native.write_bytes(b'test executable')

    def report(self):
        return json.loads(next((self.root / '.blue-check').glob('*/report.json')).read_text())

    def test_project_check_compiles_once_and_content_check_uses_no_cargo(self):
        content = game_check.commands(self.root, self.native, True, ['tests/win.json'])
        self.assertEqual([cmd[1] for cmd in content], ['audit', 'lint', 'game-validate', 'sim'])
        self.assertEqual(content[0][2], str((self.root / 'maps/main.json').resolve()))
        full = game_check.commands(self.root, self.native)
        self.assertEqual(full[-1], ['cargo', 'test', '--locked'])
        self.assertEqual(len(full), 4)

    def test_static_client_map_is_checked_even_when_game_uses_another(self):
        (self.root / 'game.json').write_text(json.dumps({'map': 'maps/custom map.json'}))
        (self.root / 'maps/custom map.json').write_text('{}')
        checks = game_check.commands(self.root, self.native, True)
        self.assertEqual([cmd[1] for cmd in checks], ['audit', 'lint', 'audit', 'lint', 'game-validate'])
        self.assertEqual(checks[0][2], str((self.root / 'maps/main.json').resolve()))
        self.assertEqual(checks[2][2], str((self.root / 'maps/custom map.json').resolve()))

    def test_declared_checks_are_always_executed_including_empty_block(self):
        (self.root / 'maps/main.json').write_text('{"checks": {}}')
        checks = game_check.commands(self.root, self.native, True)
        self.assertEqual([cmd[1] for cmd in checks], ['audit', 'lint', 'verify', 'game-validate'])

    def test_failure_preserves_logs_stops_checks_and_reports_false(self):
        def fail(command, **kwargs):
            kwargs['stdout'].write('specific native error\n')
            return subprocess.CompletedProcess(command, 7)
        with patch.object(game_check.subprocess, 'run', side_effect=fail) as invoke:
            self.assertFalse(game_check.run(self.root, self.native))
        self.assertEqual(invoke.call_count, 1)
        report = self.report()
        self.assertFalse(report['ok'])
        self.assertEqual(len(report['checks']), 1)
        self.assertFalse(report['checks'][0]['ok'])
        log = next((self.root / '.blue-check').glob('*/1.log'))
        self.assertIn('specific native error', log.read_text())

    def test_cargo_failure_timeout_and_missing_binary_cannot_pass(self):
        for failure in [subprocess.TimeoutExpired(['tool'], 600), OSError('missing')]:
            with patch.object(game_check.subprocess, 'run', side_effect=failure):
                self.assertFalse(game_check.run(self.root, self.native))
        with patch.object(game_check.subprocess, 'run', side_effect=[
                subprocess.CompletedProcess([], 0)] * 3 + [subprocess.CompletedProcess([], 1)]):
            self.assertFalse(game_check.run(self.root, self.native))
        self.assertFalse(game_check.run(self.root, self.root / 'missing'))

    def test_content_success_is_labelled_and_does_not_spawn_cargo(self):
        with patch.object(game_check.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0)) as invoke:
            self.assertTrue(game_check.run(self.root, self.native, True))
        self.assertEqual(invoke.call_count, 3)
        self.assertEqual(self.report()['scope'], 'content')
        self.assertTrue(self.report()['ok'])
        self.assertTrue(self.report()['native_sha256'])


@unittest.skipUnless(os.environ.get('BE2_TOOLS'), 'Real native integration runs through check_authoring.py')
class GeneratedGameIntegrationTests(unittest.TestCase):
    def test_generated_content_passes_and_invalid_map_fails(self):
        native = str(Path(os.environ['BE2_TOOLS']).resolve())
        flags = getattr(subprocess, 'CREATE_NO_WINDOW', 0)
        with tempfile.TemporaryDirectory() as directory:
            project = Path(directory) / 'game with spaces'
            subprocess.run([native, 'new-game', 'workflow-test', str(project)],
                           check=True, capture_output=True, creationflags=flags)
            command = [sys.executable, str(project / 'scripts/check.py'),
                       '--tools', native, '--content-only']
            success = subprocess.run(command, capture_output=True, text=True, creationflags=flags)
            self.assertEqual(success.returncode, 0, success.stdout + success.stderr)
            summary = json.loads(success.stdout)
            self.assertTrue(summary['ok'])
            report = json.loads(Path(summary['report']).read_text())
            self.assertEqual([item['command'][1] for item in report['checks']],
                             ['audit', 'lint', 'game-validate'])
            # Authored checks are not skipped: require an impossible object.
            map_path = project / 'maps/main.json'
            document = json.loads(map_path.read_text())
            document['checks'] = {'lint': {'max_warnings': 100},
                                  'objects': {'exist': ['definitely-missing-object']}}
            map_path.write_text(json.dumps(document))
            failure = subprocess.run(command, capture_output=True, text=True, creationflags=flags)
            self.assertNotEqual(failure.returncode, 0)
            summary = json.loads(failure.stdout)
            self.assertFalse(summary['ok'])
            report = json.loads(Path(summary['report']).read_text())
            self.assertEqual(report['checks'][-1]['command'][1], 'verify')
            self.assertFalse(report['checks'][-1]['ok'])
            log = Path(summary['report']).parent / report['checks'][-1]['log']
            self.assertIn('definitely-missing-object', log.read_text())


if __name__ == '__main__':
    unittest.main()
