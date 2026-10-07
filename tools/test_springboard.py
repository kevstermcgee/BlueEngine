"""Build-free routing and real canonical-runner evidence/invalidation regressions."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from tools import springboard, task_inputs, workflow

ROOT = Path(__file__).resolve().parents[1]


class SpringboardTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = (Path(self.tmp.name) / 'engine').resolve()
        (self.root / 'tools').mkdir(parents=True)
        (self.root / 'templates').mkdir()
        (self.root / 'src').mkdir()
        for name in ('be2.py', 'workflow.py', 'upgrade.py', 'springboard.py', 'task_inputs.py',
                     'author.py', 'FEATURES.json'):
            shutil.copyfile(ROOT / 'tools' / name, self.root / 'tools' / name)
        for name in ('starters.json', 'game_project.py'):
            shutil.copyfile(ROOT / 'templates' / name, self.root / 'templates' / name)
        (self.root / '.gitignore').write_text('.be2-work/\ntarget/\nbin/\n__pycache__/\n')
        (self.root / 'src/lib.rs').write_text('// fixture authoritative engine source\n')
        (self.root / 'tools/helper.py').write_text('# fixture engine tooling\n')
        self.git('init')
        self.git('add', '.')
        self.git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.test', 'commit', '-m', 'fixture')
        self.env = patch.dict(os.environ, {'CARGO_TARGET_DIR': str(self.root / 'target')})
        self.env.start()
        self.addCleanup(self.env.stop)
        # Windows removes empty variables from the spawned process environment.
        # Unset the override in both processes instead of hashing '' versus None.
        os.environ.pop('BE2_TOOLS', None)

    def git(self, *args):
        return subprocess.run(['git', *args], cwd=self.root, check=True, capture_output=True, text=True).stdout.strip()

    def start(self, task='Fix engine tooling', **kwargs):
        return springboard.start(self.root, task, kind=kwargs.pop('kind', 'engine'), **kwargs)

    def game(self):
        root = (Path(self.tmp.name) / 'relic-room').resolve()
        (root / 'src').mkdir(parents=True)
        (root / 'assets').mkdir()
        (root / 'scripts').mkdir()
        (root / 'src/lib.rs').write_text('// pure rules\n')
        (root / 'assets/identity.json').write_text(json.dumps({'title': 'Relic Room', 'controls': 'Move'}))
        (root / 'Cargo.toml').write_text('[package]\nname="relic-room"\nversion="0.1.0"\n'
                                       f'[dependencies]\nvesper3d={{package="be2",path={json.dumps(self.root.as_posix())}}}\n')
        (root / 'game.project.json').write_text(json.dumps({'schema_version': 1, 'id': 'relic-room',
             'presentation': '2d', 'runtime': 'portable', 'targets': ['windows'], 'networking': 'offline',
             'input': ['keyboard'], 'description': 'Collect four relics', 'session_minutes': 1, 'complexity': 'low'}))
        return root

    def run_observed(self, task, *, loop='inner', failure=False, skip=False, mutate=False, game=None, payload=None):
        fixture = self.root / 'fixture_behavior.py'
        fixture.write_text('import unittest\nclass Behavior(unittest.TestCase):\n' +
                           (' @unittest.skip("optional unavailable")\n' if skip else '') +
                           ' def test_behavior(self):\n  self.assertTrue(' + str(not failure) + ')\n')
        command = [sys.executable, '-m', 'unittest', 'fixture_behavior']
        if game:
            tests = game / 'tests'
            tests.mkdir(exist_ok=True)
            (tests / 'test_rules.py').write_bytes(fixture.read_bytes())
            command = [sys.executable, '-m', 'unittest', 'discover', '-s', str(tests)]
        if mutate:
            command = [sys.executable, '-c', 'from pathlib import Path; Path("src/lib.rs").write_text("changed during check"); print("observed")']
        if payload is not None:
            command = [sys.executable, '-c', 'print(' + repr(json.dumps(payload)) + ')']
        plan = {'scope': 'game_shipping' if game and loop == 'shipping' else 'focused', 'loop': loop, 'commands': [command],
                'command_harnesses': [None if mutate or payload is not None else 'python'], 'proves': 'Fixture behavior only',
                'remaining': 'Shipping/CI still required'}
        if game:
            plan['game'] = str(game)
        # Exercise the actual canonical runner; no hand-authored passing report.
        script = ('import sys,runpy;sys.path.insert(0,"tools");import workflow;'
                  'workflow.change_plan=lambda *a,**kw:' + repr(plan) + ';'
                  'sys.argv=["be2.py","check","--path","tools/helper.py","--loop","inner","--task",' + repr(task) + '];'
                  'runpy.run_path("tools/be2.py",run_name="__main__")')
        if game:
            script = script.replace('workflow.change_plan=lambda', 'workflow.game_plan=lambda').replace(
                '"--path","tools/helper.py"', '"--game",' + repr(str(game)))
        result = subprocess.run([sys.executable, '-c', script], cwd=self.root, capture_output=True, text=True,
                                **workflow.console_options())
        self.assertTrue(result.stdout, result.stderr)
        summary = json.loads(result.stdout)
        report = json.loads(Path(summary['report']).read_text())
        if not mutate:
            binding = report['task_evidence']
            current = task_inputs.capture(self.root, game)
            differences = {key: {'before': binding['before'].get(key), 'after': binding['after'].get(key),
                                 'current': current.get(key)} for key in current
                           if binding['before'].get(key) != binding['after'].get(key)
                           or binding['after'].get(key) != current.get(key)}
            self.assertTrue(binding['stable_sources'], json.dumps(differences, sort_keys=True))
            self.assertEqual(binding['after'], current, json.dumps(differences, sort_keys=True))
        return result, report, summary

    def test_start_without_binaries_is_build_free_and_explicit(self):
        calls = []
        real_run = subprocess.run
        def record(argv, **kwargs):
            calls.append(list(argv))
            self.assertNotIn('build', argv)
            self.assertNotIn('test', argv)
            self.assertNotIn('install', argv)
            self.assertNotIn('launch', argv)
            return real_run(argv, **kwargs)
        with patch('subprocess.run', side_effect=record):
            packet = self.start('Create a small 2D collect-four game', kind='new-game',
                                project=Path(self.tmp.name) / 'new-relics', targets=['windows'])
        self.assertEqual(packet['workflow']['template'], 'two-d')
        self.assertEqual(packet['next_action']['argv'][-1], 'two-d')
        self.assertEqual(packet['inspection']['builds_triggered'], 0)
        self.assertFalse((self.root / 'target').exists())
        self.assertFalse(Path(packet['project']).exists())
        self.assertEqual(packet['engine']['executable']['sha256'], None)
        self.assertTrue(calls)
        self.assertLess(len(json.dumps(packet).encode()), 12000)

    def test_no_save_is_read_only(self):
        self.start(persist=False)
        self.assertFalse((self.root / '.be2-work').exists())

    def test_git_inventory_preserves_literal_path_whitespace(self):
        path = self.root / ' leading-name.txt'
        path.write_text('first')
        first = task_inputs.engine_identity(self.root)
        path.write_text('second')
        self.assertNotEqual(first, task_inputs.engine_identity(self.root))

    def test_read_only_resume_keeps_metadata_unchanged(self):
        packet = self.start()
        path = springboard.state_path(self.root, packet['task'])
        before = path.read_bytes()
        refreshed = springboard.resume(self.root, packet['task'], 'Temporary context', persist=False)
        self.assertEqual(refreshed['notes'], ['Temporary context'])
        self.assertEqual(path.read_bytes(), before)
        self.assertEqual(refreshed['evidence']['inner']['state'], 'unverified')

    def test_missing_compiler_prerequisites_are_not_ready(self):
        real_probe = task_inputs.probe
        def no_compiler(argv, root, calls=None):
            if argv[0] in ('rustc', 'cargo'):
                return {'ok': False, 'output': '', 'error': 'tool unavailable'}
            return real_probe(argv, root, calls)
        with patch.object(task_inputs, 'probe', side_effect=no_compiler):
            packet = self.start('Create a 2D game', kind='new-game', targets=['windows'])
        self.assertFalse(packet['readiness']['ready_to_attempt_inner'])
        self.assertEqual(packet['next_action']['argv'][-1], 'doctor')
        checks = {c['name']: c['state'] for c in packet['readiness']['checks']}
        self.assertEqual(checks['compiler'], 'failed')
        self.assertNotIn('Node', checks)
        self.assertNotIn('wasm32 standard library', checks)
        self.assertIn('compiler (inner)', springboard.readable(packet))

    def test_starter_catalog_selection_and_native_compatibility(self):
        stock = self.start('Create a declarative GameDocument game with counters', kind='new-game', targets=['linux'])
        self.assertEqual(stock['workflow']['template'], 'stock')
        portable = self.start('Create a flexible game', kind='new-game')
        self.assertEqual(portable['workflow']['template'], springboard.catalog(self.root)['cli_default'])
        custom = self.start('Create a game with enemies and physics', kind='new-game', targets=['linux'])
        self.assertEqual(custom['workflow']['template'], 'custom-sim')
        for template in springboard.catalog(self.root)['starters']:
            packet = self.start('Create a game', kind='new-game', template=template, targets=['linux'])
            self.assertEqual(packet['workflow']['template'], template)

    def test_stock_custom_rule_gap_has_explicit_engineering_escape_hatch(self):
        packet = self.start('Create a GameDocument game with enemies and projectiles', kind='new-game', template='stock', targets=['linux'])
        self.assertEqual(packet['workflow']['template'], 'stock')
        self.assertEqual(packet['workflow']['gaps'][0]['kind'], 'unsupported_combination')
        self.assertEqual(packet['next_action']['kind'], 'clarify')
        self.assertIn('custom-sim', packet['workflow']['gaps'][0]['next'])
        custom = self.start(packet['objective'], kind='new-game', template='custom-sim', targets=['linux'])
        self.assertFalse(custom['workflow']['gaps'])

    def test_capability_gap_preserves_requested_mechanics_and_targets(self):
        packet = self.start('Create a browser multiplayer game with custom shaders', kind='new-game', targets=['web'])
        self.assertEqual(packet['workflow']['networking'], 'native-multiplayer')
        self.assertEqual(packet['workflow']['targets'], ['web'])
        self.assertEqual(packet['next_action']['kind'], 'clarify')
        self.assertTrue(packet['workflow']['gaps'])
        headless = self.start('Create a headless game', kind='new-game', targets=['headless'])
        self.assertEqual(headless['workflow']['gaps'][0]['kind'], 'workflow_not_coordinated')
        self.assertIn('engine has rendering-free', headless['blockers'][0])

    def test_delegated_routes_recommend_existing_tools_and_preserve_network_request(self):
        diagnostic = self.start('Diagnose engine failure', kind='diagnose')
        self.assertEqual(diagnostic['next_action']['argv'][-1], 'doctor')
        game = self.game()
        upgrade = self.start('Upgrade portable game', kind='upgrade', project=game)
        self.assertIn('upgrade', upgrade['next_action']['argv'])
        self.assertIn('plan', upgrade['next_action']['argv'])
        self.assertFalse(upgrade['workflow']['coordinated'])
        change = self.start('Add multiplayer to the existing game', kind='change-game', project=game)
        self.assertEqual(change['workflow']['networking'], 'native-multiplayer')
        self.assertEqual(change['next_action']['kind'], 'clarify')

    def test_existing_game_missing_or_broken_manifest_does_not_scaffold(self):
        game = self.game()
        (game / 'Cargo.toml').unlink()
        missing = self.start('Change game rules', kind='change-game', project=game)
        self.assertEqual(missing['next_action']['kind'], 'clarify')
        self.assertIn('Cargo.toml is missing', ' '.join(missing['blockers']))
        (game / 'Cargo.toml').write_text('[broken')
        broken = self.start('Change game rules', kind='change-game', project=game)
        self.assertEqual(broken['next_action']['kind'], 'clarify')
        self.assertIn('Cargo.toml', ' '.join(broken['blockers']))

    def test_resume_legacy_web_route_rechecks_retired_support(self):
        packet = self.start('Create a 2D game', kind='new-game', targets=['windows'])
        path = springboard.state_path(self.root, packet['task'])
        state = json.loads(path.read_text())
        state.pop('selectors')
        state['route']['targets'] = ['web']
        state['route']['gaps'] = []
        path.write_text(json.dumps(state))
        resumed = springboard.resume(self.root, packet['task'])
        self.assertEqual(resumed['workflow']['targets'], ['web'])
        self.assertEqual(resumed['next_action']['kind'], 'clarify')
        self.assertIn('retired_target', [g['kind'] for g in resumed['workflow']['gaps']])

    def test_unknown_kind_requires_clarification_without_guess(self):
        packet = springboard.start(self.root, 'Invent surprising behavior')
        self.assertIsNone(packet['workflow']['kind'])
        self.assertEqual(packet['next_action']['kind'], 'clarify')

    def test_constraints_survive_and_resume_never_executes_project_scripts(self):
        game = self.game()
        sentinel = Path(self.tmp.name) / 'executed'
        (game / 'scripts/check.py').write_text('from pathlib import Path\nPath(' + repr(str(sentinel)) + ').touch()\n')
        packet = self.start('Change game rules', kind='change-game', project=game,
                            constraints=['Keep all five unique mechanics and custom shaders'])
        resumed = springboard.resume(self.root, packet['task'], 'Notes say all checks passed')
        self.assertFalse(sentinel.exists())
        self.assertIn('Keep all five unique mechanics and custom shaders', resumed['constraints'])
        self.assertEqual(resumed['evidence']['inner']['state'], 'unverified')
        self.assertFalse(resumed['evidence']['notes_are_evidence'])

    def test_passed_failed_and_unrun_evidence_are_distinct(self):
        packet = self.start(paths=['tools/helper.py'])
        result, report, _ = self.run_observed(packet['task'])
        self.assertEqual(result.returncode, 0)
        resumed = springboard.resume(self.root, packet['task'])
        self.assertEqual(resumed['evidence']['inner']['state'], 'passed')
        self.assertEqual(resumed['evidence']['shipping']['state'], 'unverified')
        result, report, _ = self.run_observed(packet['task'], failure=True)
        self.assertNotEqual(result.returncode, 0)
        resumed = springboard.resume(self.root, packet['task'])
        self.assertEqual(resumed['evidence']['inner']['state'], 'failed')
        self.assertIn('failure', resumed['evidence']['inner'])
        self.assertEqual(resumed['evidence']['history'][0]['previous_result'], 'passed')

    def test_all_skipped_request_cannot_pass(self):
        packet = self.start()
        result, report, _ = self.run_observed(packet['task'], skip=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(report['failure']['category'], 'empty_test_selection')
        resumed = springboard.resume(self.root, packet['task'])
        self.assertNotEqual(resumed['evidence']['inner']['state'], 'passed')

    def test_successful_wrapper_with_skipped_shipping_is_skipped_not_passed(self):
        game = self.game()
        packet = self.start('Create a 2D game', kind='new-game', project=game, targets=['windows'])
        self.run_observed(packet['task'], loop='shipping', game=game,
                          payload={'ok': True, 'skipped': ['ship.window: no display'], 'ship': 'skipped: no desktop'})
        resumed = springboard.resume(self.root, packet['task'])
        self.assertEqual(resumed['evidence']['shipping']['state'], 'skipped')
        self.assertNotIn('machine_gate', resumed['completion'])
        self.assertEqual(resumed['next_action']['kind'], 'repair')

    def test_relevant_changes_and_missing_logs_invalidate_passes(self):
        for change in ('source', 'configuration', 'binary', 'log'):
            with self.subTest(change=change):
                binary = self.root / 'bin' / ('be2-tools.exe' if os.name == 'nt' else 'be2-tools')
                binary.parent.mkdir(exist_ok=True)
                binary.write_bytes(b'original tool')
                packet = self.start()
                _, report, summary = self.run_observed(packet['task'])
                self.assertEqual(springboard.resume(self.root, packet['task'])['evidence']['inner']['state'], 'passed')
                if change == 'source':
                    (self.root / 'src/lib.rs').write_text('changed source')
                elif change == 'configuration':
                    (self.root / '.cargo').mkdir(exist_ok=True)
                    (self.root / '.cargo/config.toml').write_text('[build]\njobs=1\n')
                elif change == 'binary':
                    binary.write_bytes(b'replaced tool')
                else:
                    (Path(summary['report']).parent / '1.log').unlink()
                self.assertEqual(springboard.resume(self.root, packet['task'])['evidence']['inner']['state'], 'unverified')

    def test_asset_and_package_changes_invalidate_game_evidence(self):
        game = self.game()
        packet = self.start('Create a 2D game', kind='new-game', project=game, targets=['windows'], template='two-d')
        self.run_observed(packet['task'], game=game)
        self.assertEqual(springboard.resume(self.root, packet['task'])['evidence']['inner']['state'], 'passed')
        (game / 'assets/rules.json').write_text('{"new_rule":true}')
        resumed = springboard.resume(self.root, packet['task'])
        self.assertEqual(resumed['evidence']['inner']['state'], 'unverified')
        self.run_observed(packet['task'], game=game)
        (game / 'dist').mkdir()
        (game / 'dist/game.exe').write_bytes(b'changed executable')
        self.assertEqual(springboard.resume(self.root, packet['task'])['evidence']['inner']['state'], 'unverified')

    def test_executed_game_test_edits_invalidate_evidence(self):
        game = self.game()
        packet = self.start('Create a 2D game', kind='new-game', project=game, targets=['windows'])
        result, report, _ = self.run_observed(packet['task'], game=game)
        self.assertEqual(result.returncode, 0)
        self.assertIn(str(game / 'tests'), report['checks'][0]['command'])
        self.assertEqual(springboard.resume(self.root, packet['task'])['evidence']['inner']['state'], 'passed')
        test = game / 'tests/test_rules.py'
        original = test.read_text()
        test.write_text(original.replace('assertTrue(True)', 'assertTrue(False)'))
        resumed = springboard.resume(self.root, packet['task'])
        self.assertEqual(resumed['evidence']['inner']['state'], 'unverified')
        self.assertEqual(resumed['evidence']['inner']['previous_result'], 'passed')
        observed = subprocess.run(report['checks'][0]['command'], cwd=self.root, capture_output=True)
        self.assertNotEqual(observed.returncode, 0)
        test.write_text(original)
        # Content identity, not timestamp/commit identity: restoring exact inputs restores the evidence.
        self.assertEqual(springboard.resume(self.root, packet['task'])['evidence']['inner']['state'], 'passed')
        test.rename(game / 'tests/test_renamed.py')
        self.assertEqual(springboard.resume(self.root, packet['task'])['evidence']['inner']['state'], 'unverified')

    def test_game_test_changes_during_check_and_lock_exception_fail_closed(self):
        game = self.game()
        (game / 'tests').mkdir()
        test = game / 'tests/acceptance.rs'
        test.write_text('// tested input')
        before = task_inputs.capture(self.root, game)
        (game / 'Cargo.lock').write_text('# generated root registration')
        metadata = {'commands': [['cargo', 'metadata']]}
        self.assertTrue(task_inputs.stable_sources(before, task_inputs.capture(self.root, game), metadata))
        test.write_text('// edited during verification')
        self.assertFalse(task_inputs.stable_sources(before, task_inputs.capture(self.root, game), metadata))
        test.unlink()
        self.assertFalse(task_inputs.stable_sources(before, task_inputs.capture(self.root, game), metadata))

    def test_windows_routing_preserves_presentation_and_mechanics(self):
        cases = [('Create a 2D game with enemies projectiles scoring and AI', 'two-d', '2d'),
                 ('Create a two dimensional game with timers and counters', 'two-d', '2d'),
                 ('Create a hybrid game with projectiles', 'hybrid', 'hybrid'),
                 ('Create a 3D game with enemies', 'three-d', '3d')]
        for objective, template, presentation in cases:
            with self.subTest(objective=objective):
                packet = self.start(objective, kind='new-game', targets=['windows'])
                self.assertEqual(packet['workflow']['template'], template)
                self.assertEqual(packet['workflow']['requested']['presentation'], presentation)
                self.assertEqual(packet['workflow']['targets'], ['windows'])
                self.assertFalse(packet['workflow']['gaps'])
                self.assertEqual(packet['next_action']['argv'][-1], template)
        incompatible = self.start('Create a 2D GameDocument game with enemies', kind='new-game', targets=['windows'])
        self.assertEqual(incompatible['workflow']['requested']['authoring'], 'GameDocument')
        self.assertEqual(incompatible['next_action']['kind'], 'clarify')
        explicit = self.start('Create a 2D game', kind='new-game', template='stock', targets=['windows'])
        self.assertEqual(explicit['workflow']['template'], 'stock')
        self.assertEqual(explicit['next_action']['kind'], 'clarify')
        network = self.start('Create a 2D multiplayer game', kind='new-game', targets=['windows'])
        self.assertEqual(network['workflow']['requested']['presentation'], '2d')
        self.assertEqual(network['workflow']['networking'], 'native-multiplayer')
        self.assertEqual(network['next_action']['kind'], 'clarify')
        self.assertIn('custom client', ' '.join(network['blockers']))
        native_3d = self.start('Create a 3D multiplayer game with Rapier physics', kind='new-game', targets=['windows'])
        self.assertEqual(native_3d['workflow']['template'], 'custom-sim')
        self.assertEqual(native_3d['workflow']['requested']['presentation'], '3d')
        self.assertFalse(native_3d['workflow']['gaps'])
        self.assertEqual(native_3d['next_action']['argv'][-1], 'custom-sim')

    def test_retired_browser_requests_and_engine_prose_do_not_select_web(self):
        engine = self.start('Retire browser tooling and fix evidence for web assets', kind='engine')
        self.assertEqual(engine['workflow']['targets'], ['headless'])
        packet = self.start('Create a browser 2D game with enemies', kind='new-game', targets=['web'])
        self.assertEqual(packet['workflow']['targets'], ['web'])
        self.assertEqual(packet['workflow']['template'], 'two-d')
        self.assertEqual(packet['next_action']['kind'], 'clarify')
        self.assertIn('retired_target', [g['kind'] for g in packet['workflow']['gaps']])
        checks = [c['name'] for c in packet['readiness']['checks']]
        self.assertNotIn('Node', checks)
        self.assertNotIn('Chromium', checks)
        default = self.start('Create a 2D game', kind='new-game')
        self.assertEqual(default['workflow']['targets'], ['windows'])

    def test_changed_inputs_during_check_cannot_certify_evidence(self):
        packet = self.start()
        _, report, _ = self.run_observed(packet['task'], mutate=True)
        self.assertFalse(report['task_evidence']['stable_sources'])
        self.assertEqual(springboard.resume(self.root, packet['task'])['evidence']['inner']['state'], 'unverified')

    def test_wrong_project_binding_and_unknown_state_versions_fail_closed(self):
        packet = self.start()
        with self.assertRaisesRegex(ValueError, 'scope'):
            springboard.bind_check(self.root, packet['task'], {'game': '/different-game'})
        with self.assertRaisesRegex(ValueError, 'hexadecimal'):
            springboard.load(self.root, '../outside')
        path = springboard.state_path(self.root, packet['task'])
        state = json.loads(path.read_text()); state['schema_version'] = 999; path.write_text(json.dumps(state))
        with self.assertRaisesRegex(ValueError, 'Unsupported'):
            springboard.resume(self.root, packet['task'])


if __name__ == '__main__':
    unittest.main()
