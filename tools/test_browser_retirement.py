"""Native contracts and fail-fast migration coverage for retired browser gameplay."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from tools import workflow

ROOT = Path(__file__).resolve().parents[1]


class BrowserRetirementTests(unittest.TestCase):
    def test_old_commands_fail_without_build_install_or_outputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            game = Path(temporary) / 'uncreated-game'
            for command in (['web', 'prepare'], ['web', 'build'], ['web', 'preview'],
                            ['web', 'publish'], ['publish'], ['web', 'capabilities']):
                result = subprocess.run([sys.executable, str(ROOT / 'tools/be2.py'), *command, str(game)],
                                        cwd=temporary, capture_output=True, text=True)
                self.assertEqual(result.returncode, 2, result.stderr)
                packet = json.loads(result.stdout)
                self.assertEqual(packet['code'], 'BROWSER-RETIRED')
                self.assertFalse(packet['ok'])
                self.assertEqual(packet['builds_triggered'], 0)
                self.assertIn('windows', packet['next'])
                self.assertFalse(game.exists())
                self.assertEqual(list(Path(temporary).iterdir()), [])

    def test_project_preserves_native_constraints_and_rejects_retired_requirements(self):
        validate = workflow.project_module(ROOT).validate_project
        with tempfile.TemporaryDirectory() as temporary:
            game = Path(temporary)
            base = {'schema_version': 1, 'id': 'native-room', 'presentation': '2d',
                    'runtime': 'portable', 'targets': ['windows'], 'networking': 'offline',
                    'input': ['keyboard', 'mouse', 'controller'], 'description': 'A native game',
                    'session_minutes': 5, 'complexity': 'medium'}
            def write(value):
                (game / 'game.project.json').write_text(json.dumps(value))
            for presentation in ('2d', '3d', 'hybrid'):
                for target in ('windows', 'linux', 'macos'):
                    write(base | {'presentation': presentation, 'targets': [target]})
                    self.assertEqual(validate(game)['targets'], [target])
            for change, diagnostic in (({'targets': ['web', 'windows']}, 'retired'),
                                       ({'web_build': {'binary': 'old-game'}}, 'retired'),
                                       ({'input': []}, 'input'), ({'input': [{}]}, 'input'),
                                       ({'targets': [{}]}, 'targets'), ({'targets': ['windows', 'windows']}, 'targets'),
                                       ({'description': ''}, 'description'), ({'id': 23}, 'Game ID'),
                                       ({'session_minutes': True}, 'session_minutes'),
                                       ({'networking': 'native-multiplayer'}, 'offline only'),
                                       ({'target': ['windows']}, 'Unknown project'),
                                       ({'mobile_controls': {'layout': 'invalid'}}, 'mobile_controls')):
                with self.subTest(change=change):
                    write(base | change)
                    with self.assertRaisesRegex(ValueError, diagnostic):
                        validate(game)
            write(base | {'runtime': 'legacy-native', 'presentation': '3d', 'networking': 'native-multiplayer'})
            self.assertEqual(validate(game)['networking'], 'native-multiplayer')

    def test_supported_games_have_native_target_and_full_native_shipping_gate(self):
        for name in ('lantern-run', 'pocket-breaker', 'orchard-watch', 'lantern-grove'):
            game = ROOT / 'games' / name
            project = workflow.project_module(ROOT).validate_project(game)
            self.assertIn('windows', project['targets'])
            self.assertNotIn('web', project['targets'])
            plan = workflow.game_plan(ROOT, game, 'shipping')
            self.assertIn([sys.executable, str(game / 'scripts/check.py')], plan['commands'])
            self.assertFalse(any('web' in c for c in plan['commands']))
            self.assertEqual(plan['requirements']['native_packaging'], sorted(project['targets']))

    def test_native_task_helpers_do_not_import_retired_toolkit(self):
        result = subprocess.run([sys.executable, '-c',
                                'import sys; from tools import task_inputs, springboard; '
                                'assert not any(k.endswith(("web_games", "web_release", "web_reproduce", "web_publish")) for k in sys.modules)'],
                                cwd=ROOT, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == '__main__':
    unittest.main()
