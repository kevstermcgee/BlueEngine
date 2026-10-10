"""Exercise the shipped game runner's failure behavior and validation boundary."""
import contextlib
import hashlib
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

SPEC = importlib.util.spec_from_file_location('game_check', Path(__file__).resolve().parents[1] /
                                              'templates/game_check.py')
game_check = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(game_check)

LOCK = ['cargo', 'metadata', '--format-version', '1', '--quiet']
TEST = ['cargo', 'test', '--locked']


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
        # the lock step settles a seeded Cargo.lock; the test step is the only one that compiles
        self.assertEqual(full[-2:], [LOCK, TEST])
        self.assertEqual(len(full), 5)

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

    def test_mover_game_lint_uses_chosen_scenario_without_blanket_exemptions(self):
        (self.root / 'game.json').write_text(json.dumps({'map':'maps/main.json','movers':[{'id':'gate'}]}))
        static = game_check.commands(self.root, self.native, True)
        self.assertEqual(static[1], [str(self.native), 'lint', str((self.root / 'maps/main.json').resolve())])
        proven = game_check.commands(self.root, self.native, True, ['tests/win.json','tests/lose.json'])
        self.assertEqual(proven[1][-2:], ['--game=' + str((self.root / 'game.json').resolve()),
                                        '--scenario=' + str((self.root / 'tests/win.json').resolve())])
        self.assertEqual([cmd[1] for cmd in proven], ['audit','lint','game-validate','sim','sim'])

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
                subprocess.CompletedProcess([], 0)] * 4 + [subprocess.CompletedProcess([], 1)]):
            self.assertFalse(game_check.run(self.root, self.native))
        self.assertFalse(game_check.run(self.root, self.root / 'missing'))

    def test_content_success_is_labelled_and_does_not_spawn_cargo(self):
        with patch.object(game_check.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0)) as invoke:
            self.assertTrue(game_check.run(self.root, self.native, True))
        self.assertEqual(invoke.call_count, 3)
        self.assertEqual(self.report()['scope'], 'content')
        self.assertTrue(self.report()['ok'])
        self.assertTrue(self.report()['native_sha256'])

    def test_repeated_timestamp_keeps_both_reports_and_original_failure_log(self):
        output = io.StringIO()
        def result(code, text):
            def invoke(command, **kwargs):
                kwargs['stdout'].write(text)
                return subprocess.CompletedProcess(command, code)
            return invoke
        with patch.object(game_check.datetime, 'datetime') as clock, contextlib.redirect_stdout(output):
            clock.now.return_value.strftime.return_value = 'same-timestamp'
            with patch.object(game_check.subprocess, 'run', side_effect=result(7, 'original failure')):
                self.assertFalse(game_check.run(self.root, self.native, True))
            with patch.object(game_check.subprocess, 'run', side_effect=result(0, 'later success')):
                self.assertTrue(game_check.run(self.root, self.native, True))
        first, second = [Path(json.loads(line)['report']) for line in output.getvalue().splitlines()]
        self.assertNotEqual(first, second)
        self.assertFalse(json.loads(first.read_text())['ok'])
        self.assertTrue(json.loads(second.read_text())['ok'])
        self.assertEqual((first.parent / '1.log').read_text(), 'original failure')
        self.assertEqual((second.parent / '1.log').read_text(), 'later success')


class FindToolsTests(unittest.TestCase):
    """A game finds a built be2-tools in the engine checkout it depends on, without BE2_TOOLS."""

    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        base = Path(temp.name)
        self.game, self.engine = base / 'game', base / 'BlueEngine'
        self.game.mkdir()
        self.engine.mkdir()
        (self.game / 'Cargo.toml').write_text(
            '[dependencies]\nvesper3d = { package = "be2", path = "../BlueEngine", default-features = false }\n')
        clean = {k: v for k, v in os.environ.items() if k not in ('BE2_TOOLS', 'CARGO_TARGET_DIR')}
        patcher = patch.dict(os.environ, clean, clear=True)
        patcher.start()
        self.addCleanup(patcher.stop)
        which = patch.object(game_check.shutil, 'which', return_value=None)
        which.start()
        self.addCleanup(which.stop)

    def build(self, profile):
        suffix = '.exe' if os.name == 'nt' else ''
        path = self.engine / 'target' / profile / ('be2-tools' + suffix)
        path.parent.mkdir(parents=True)
        path.write_bytes(b'tool')
        return str(path)

    def test_nothing_built_means_none_so_the_caller_can_explain(self):
        self.assertIsNone(game_check.find_tools(self.game))

    def test_a_binary_in_the_engine_target_dir_is_found(self):
        built = self.build('fast')
        self.assertEqual(Path(game_check.find_tools(self.game)).resolve(), Path(built).resolve())

    def test_release_is_preferred_over_debug(self):
        self.build('debug')
        release = self.build('release')
        self.assertEqual(Path(game_check.find_tools(self.game)).resolve(), Path(release).resolve())

    def test_itest_is_preferred_over_old_profiles_and_path(self):
        self.build('release')
        self.build('fast')
        built = self.build('itest')
        with patch.object(game_check.shutil, 'which', return_value='/old/be2-tools'):
            self.assertEqual(Path(game_check.find_tools(self.game)), Path(built))

    def test_selected_absolute_and_relative_target_match_authoring_discovery(self):
        from tools import author
        self.build('itest')
        name = 'be2-tools.exe' if os.name == 'nt' else 'be2-tools'
        for target in (self.engine.parent / 'shared target', Path('relative target')):
            with self.subTest(target=target), patch.dict(os.environ, {'CARGO_TARGET_DIR': str(target)}):
                base = target if target.is_absolute() else self.engine / target
                built = base / 'itest' / name
                built.parent.mkdir(parents=True)
                built.write_bytes(b'current tools')
                self.assertEqual(Path(game_check.find_tools(self.game)), built)
                self.assertEqual(author.native_binary(self.engine), built)
                built.unlink()
                # Do not substitute the engine's default target when Cargo selected another one.
                self.assertIsNone(game_check.find_tools(self.game))

    def test_dependency_spellings_and_python_3_10_find_tools(self):
        built = self.build('itest')
        for dependency in ('be2 = { path = "../BlueEngine" }',
                           'vesper3d = { path = "../BlueEngine" }',
                           'engine = { package = "be2", path = "../BlueEngine" }'):
            (self.game / 'Cargo.toml').write_text('[dependencies]\n' + dependency + '\n')
            for modules in ({}, {'tomllib': None}):
                with self.subTest(dependency=dependency, modules=modules), patch.dict(sys.modules, modules):
                    # Windows temp roots can use RUNNER~1 while discovery expands
                    # the same existing file to runneradmin. Compare file identity.
                    self.assertTrue(Path(game_check.find_tools(self.game)).samefile(built))

    def test_packaged_tools_and_path_remain_available(self):
        built = self.build('itest')
        packaged = self.engine / 'bin' / Path(built).name
        packaged.parent.mkdir()
        packaged.write_bytes(b'packaged')
        self.assertEqual(Path(game_check.find_tools(self.game)), packaged)
        (self.game / 'Cargo.toml').unlink()
        with patch.object(game_check.shutil, 'which', return_value='/packaged/be2-tools'):
            self.assertEqual(game_check.find_tools(self.game), '/packaged/be2-tools')

    def test_project_refresh_preserves_metadata_and_other_scripts(self):
        source = self.engine / 'templates/game_project.py'
        source.parent.mkdir()
        source.write_bytes(b'canonical validator')
        (self.game / 'game.project.json').write_bytes(b'authored requirements')
        scripts = self.game / 'scripts'
        scripts.mkdir()
        (scripts / 'check.py').write_bytes(b'custom checker')
        for previous in (None, b'old validator'):
            if previous is not None:
                (scripts / 'project.py').write_bytes(previous)
            self.assertEqual(game_check.refresh_project(self.game), scripts / 'project.py')
            self.assertEqual((scripts / 'project.py').read_bytes(), source.read_bytes())
            self.assertEqual((self.game / 'game.project.json').read_bytes(), b'authored requirements')
            self.assertEqual((scripts / 'check.py').read_bytes(), b'custom checker')

    def test_unavailable_refresh_leaves_existing_validator_intact(self):
        scripts = self.game / 'scripts'
        scripts.mkdir()
        (scripts / 'project.py').write_bytes(b'local validator')
        with self.assertRaisesRegex(ValueError, 'source engine dependency'):
            game_check.refresh_project(self.game)
        self.assertEqual((scripts / 'project.py').read_bytes(), b'local validator')

    def test_failed_refresh_preserves_completed_validator_and_cleans_temporary_file(self):
        source = self.engine / 'templates/game_project.py'
        source.parent.mkdir()
        source.write_bytes(b'canonical validator')
        scripts = self.game / 'scripts'
        scripts.mkdir()
        validator = scripts / 'project.py'
        validator.write_bytes(b'local validator')
        with patch.object(game_check.os, 'replace', side_effect=OSError('write failed')):
            with self.assertRaisesRegex(OSError, 'write failed'):
                game_check.refresh_project(self.game)
        self.assertEqual(validator.read_bytes(), b'local validator')
        self.assertEqual(list(scripts.iterdir()), [validator])

    def test_the_environment_variable_still_wins(self):
        self.build('release')
        with patch.dict(os.environ, {'BE2_TOOLS': '/somewhere/be2-tools'}):
            self.assertEqual(game_check.find_tools(self.game), '/somewhere/be2-tools')

    def test_a_game_without_an_engine_path_finds_nothing(self):
        (self.game / 'Cargo.toml').write_text('[dependencies]\nserde = "1"\n')
        self.build('release')
        self.assertIsNone(game_check.find_tools(self.game))


@unittest.skipUnless(os.environ.get('BE2_TOOLS'), 'Real native integration runs through check_authoring.py')
class GeneratedGameIntegrationTests(unittest.TestCase):
    def test_all_starters_validate_plan_and_use_the_fresh_authoring_binary(self):
        from tools import workflow
        engine = Path(__file__).resolve().parents[1]
        requirements = workflow.project_module(engine)
        native = Path(os.environ['BE2_TOOLS']).resolve()
        flags = getattr(subprocess, 'CREATE_NO_WINDOW', 0)
        env = os.environ.copy()
        env.pop('BE2_TOOLS', None)
        env['CARGO_TARGET_DIR'] = str(native.parent.parent)
        if native.parent.name == 'debug':
            # --profile dev is an explicit verification override. Canonical map authoring uses itest.
            env['BE2_TOOLS'] = str(native)
        with tempfile.TemporaryDirectory() as directory:
            for starter, presentation in [('stock', '3d'), ('custom-sim', '3d'), ('two-d', '2d'),
                                          ('three-d', '3d'), ('hybrid', 'hybrid'), ('portable', 'hybrid')]:
                with self.subTest(starter=starter):
                    project = Path(directory) / starter
                    subprocess.run([str(native), 'new-game', 'scaffold-' + starter, str(project),
                                    str(engine), starter], check=True, capture_output=True, creationflags=flags)
                    declaration = requirements.validate_project(project)
                    self.assertEqual(declaration['presentation'], presentation)
                    self.assertEqual(declaration['runtime'],
                                     'legacy-native' if starter in ('stock', 'custom-sim') else 'portable')
                    self.assertIn('CLI starter: `' + starter + '`; project runtime: `' + declaration['runtime'] + '`',
                                  (project / 'STATUS.md').read_text())
                    self.assertEqual(declaration['targets'], ['windows'])
                    declaration['targets'] = ['linux', 'windows']
                    (project / 'game.project.json').write_text(json.dumps(declaration))
                    self.assertEqual((project / 'scripts/project.py').read_bytes(),
                                     (engine / 'templates/game_project.py').read_bytes())
                    for loop in ('inner', 'integration', 'shipping'):
                        plan = workflow.game_plan(engine, project, loop)
                        self.assertEqual(plan['requirements']['native_packaging'], ['linux', 'windows'])
                        self.assertTrue(plan['commands'])
                    self.assertEqual(game_check.engine_path(project), engine)
                    command = [sys.executable, str(project / 'scripts/check.py'), '--skip-ship']
                    if starter not in ('stock', 'custom-sim'):
                        command.append('--content-only')
                    checked = subprocess.run(command, cwd=directory, env=env, capture_output=True,
                                             text=True, timeout=600, creationflags=flags)
                    self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
                    summary = json.loads(checked.stdout)
                    report = json.loads(Path(summary['report']).read_text())
                    self.assertTrue(report['ok'])
                    self.assertEqual(report['native_sha256'], hashlib.sha256(native.read_bytes()).hexdigest())
                    if starter in ('stock', 'custom-sim'):
                        self.assertIn('test', [item['command'][1] for item in report['checks']])
                    if starter == 'custom-sim':
                        (project / 'scripts/project.py').unlink()
                        missing = subprocess.run(command, env=env, capture_output=True, text=True,
                                                 creationflags=flags)
                        self.assertNotEqual(missing.returncode, 0)
                        self.assertIn('--refresh-project', missing.stderr)
                        refreshed = subprocess.run([sys.executable, str(project / 'scripts/check.py'),
                                                    '--refresh-project'], env=env, capture_output=True,
                                                   text=True, creationflags=flags)
                        self.assertEqual(refreshed.returncode, 0, refreshed.stderr)
                        self.assertEqual(requirements.validate_project(project), declaration)
                        self.assertEqual((project / 'scripts/project.py').read_bytes(),
                                         (engine / 'templates/game_project.py').read_bytes())

    def test_generated_content_passes_and_invalid_map_fails(self):
        native = str(Path(os.environ['BE2_TOOLS']).resolve())
        flags = getattr(subprocess, 'CREATE_NO_WINDOW', 0)
        with tempfile.TemporaryDirectory() as directory:
            project = Path(directory) / 'game with spaces'
            subprocess.run([native, 'new-game', 'workflow-test', str(project), 'stock'],
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


class RunnerCase(unittest.TestCase):
    """A temp project plus helpers to run the runner in-process with the tool commands faked."""

    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.native = self.root / 'native tool'
        self.native.write_bytes(b'test executable')

    def run_check(self, *args, runner=None, **kwargs):
        """game_check.run with cargo and the native tool faked (exit 0) unless `runner` says otherwise.
        Returns (ok, printed JSON line, the commands that were run)."""
        commands, real_run = [], subprocess.run

        def fake(command, **options):
            commands.append([str(part) for part in command])
            if runner is not None:
                answer = runner(command, options, real_run)
                if answer is not None:
                    return answer
            if options.get('stdout') not in (None, subprocess.PIPE, subprocess.DEVNULL):
                options['stdout'].write('')
            return subprocess.CompletedProcess(command, 0, stdout='', stderr='')

        out = io.StringIO()
        with patch.object(game_check.subprocess, 'run', side_effect=fake), contextlib.redirect_stdout(out):
            ok = game_check.run(self.root, self.native, *args, **kwargs)
        return ok, json.loads(out.getvalue().strip().splitlines()[-1]), commands

    def report(self):
        return json.loads(sorted((self.root / '.blue-check').glob('*/report.json'))[-1].read_text())


class ProjectsWithoutGameDocumentTests(RunnerCase):
    """Custom-simulation games have no game.json: the runner must not fail on that."""

    def names(self, **kwargs):
        return [cmd[1] for cmd in game_check.commands(self.root, self.native, **kwargs)]

    def test_no_game_json_and_no_map_leaves_only_cargo_in_a_full_check(self):
        self.assertEqual(self.names(content_only=True), [])
        self.assertEqual(game_check.commands(self.root, self.native), [LOCK, TEST])

    def test_main_map_is_validated_when_it_exists(self):
        (self.root / 'maps').mkdir()
        (self.root / 'maps/main.json').write_text('{}')
        self.assertEqual(self.names(content_only=True), ['audit', 'lint'])
        self.assertEqual(self.names(), ['audit', 'lint', 'metadata', 'test'])

    def test_custom_named_audio_banks_are_checked_before_cargo(self):
        bank = self.root / 'assets/audio/nature'
        bank.mkdir(parents=True)
        (bank / 'bank.json').write_text('{}')
        commands = game_check.commands(self.root, self.native)
        self.assertEqual(commands[0], [str(self.native), 'audio', 'check', str(bank.resolve())])
        self.assertEqual(commands[1:], [LOCK, TEST])

    def test_authored_checks_run_verify_and_game_validate_is_skipped(self):
        (self.root / 'maps').mkdir()
        (self.root / 'maps/main.json').write_text('{"checks": {}}')
        self.assertEqual(self.names(content_only=True), ['audit', 'lint', 'verify'])
        self.assertNotIn('game-validate', self.names())

    def test_scenarios_still_run(self):
        commands = game_check.commands(self.root, self.native, True, ['tests/a.json'])
        self.assertEqual([cmd[1] for cmd in commands], ['sim'])
        self.assertEqual(commands[0][2], str((self.root / 'tests/a.json').resolve()))

    def test_a_full_check_of_such_a_game_runs_cargo_test_locked(self):
        ok, line, commands = self.run_check()
        self.assertTrue(ok)
        self.assertEqual(commands, [LOCK, TEST])
        self.assertEqual([c['name'] for c in self.report()['checks']], ['lock', 'test'])

    def test_a_game_json_without_map_still_reports_the_missing_key(self):
        (self.root / 'game.json').write_text('{}')
        ok, line, commands = self.run_check()
        self.assertFalse(ok)
        self.assertIn('map', line['error'])
        self.assertEqual(commands, [])


class LockStepTests(RunnerCase):
    def setUp(self):
        super().setUp()
        (self.root / 'game.json').write_text(json.dumps({'map': 'maps/main.json'}))
        (self.root / 'maps').mkdir()
        (self.root / 'maps/main.json').write_text('{}')

    def test_full_check_settles_the_lock_then_tests_locked(self):
        ok, line, commands = self.run_check()
        self.assertTrue(ok)
        self.assertEqual([c[1] for c in commands], ['audit', 'lint', 'game-validate', 'metadata', 'test'])
        self.assertEqual(commands[-2:], [LOCK, TEST])
        self.assertFalse(any('generate-lockfile' in ' '.join(c) for c in commands))  # that would drop the pins

    def test_lock_step_is_its_own_named_item_in_the_report(self):
        self.run_check()
        checks = self.report()['checks']
        self.assertEqual([c['name'] for c in checks], ['audit', 'lint', 'game-validate', 'lock', 'test'])
        self.assertEqual(checks[3]['command'], LOCK)
        self.assertTrue(all(c['ok'] for c in checks))

    def test_lock_runs_whether_or_not_a_lockfile_exists(self):
        for lock_present in (False, True):
            (self.root / 'Cargo.lock').unlink(missing_ok=True)
            if lock_present:
                (self.root / 'Cargo.lock').write_text('# seeded from the engine\n')
            with self.subTest(lock_present=lock_present):
                self.assertEqual(self.run_check()[2][-2], LOCK)

    def test_metadata_output_stays_out_of_the_log_but_errors_are_kept(self):
        seen = {}

        def runner(command, options, _real):
            if command == LOCK:
                seen.update(options)
                options['stderr'].write('error: failed to parse manifest\n')
                return subprocess.CompletedProcess(command, 101)
            return None

        ok, line, commands = self.run_check(runner=runner)
        self.assertFalse(ok)
        self.assertIs(seen['stdout'], subprocess.DEVNULL)
        log = Path(line['error'].split('see ')[-1])
        self.assertIn('failed to parse manifest', log.read_text())
        self.assertEqual(commands[-1], LOCK)  # cargo test never started

    def test_content_only_never_touches_cargo(self):
        ok, line, commands = self.run_check(True)
        self.assertTrue(ok)
        self.assertNotIn('cargo', [c[0] for c in commands])


SHIP_PASS = {'ok': True, 'checks': [
    {'name': 'identity', 'status': 'pass', 'detail': 'fine'},
    {'name': 'package', 'status': 'warn', 'detail': 'dist/x.exe is older than the release build'},
    {'name': 'shortcut-file', 'status': 'skip', 'detail': 'no desktop'}],
    'skipped': ['shortcut-file: no desktop', 'launch: not requested (pass --launch)'], 'next': ''}
SHIP_FAIL = {'ok': False, 'checks': [
    {'name': 'identity', 'status': 'pass', 'detail': 'fine'},
    {'name': 'shortcut-file', 'status': 'fail', 'detail': 'C:\\Desktop\\Zed.lnk does not exist: run python scripts/ship.py shortcut'},
    {'name': 'shortcut-unique', 'status': 'fail', 'detail': 'second failure'}],
    'skipped': [], 'next': 'python scripts/ship.py shortcut'}


class ShipGateTests(RunnerCase):
    def setUp(self):
        super().setUp()
        (self.root / 'game.json').write_text(json.dumps({'map': 'maps/main.json'}))
        (self.root / 'maps').mkdir()
        (self.root / 'maps/main.json').write_text('{}')
        (self.root / 'scripts').mkdir()
        self.marker = self.root / 'ship-ran.txt'

    def install_ship(self, verdict=None, code=0, raw=None):
        """A stand-in scripts/ship.py that records that it ran and prints `verdict` (or `raw`)."""
        body = ('import json, pathlib, sys\n'
                f'pathlib.Path({str(self.marker)!r}).write_text(" ".join(sys.argv[1:]))\n'
                + (f'sys.stdout.write({raw!r})\n' if raw is not None else f'print(json.dumps({verdict!r}))\n')
                + f'sys.exit({code})\n')
        (self.root / 'scripts/ship.py').write_text(body)

    def run_with_real_ship(self, *args, **kwargs):
        def runner(command, options, real_run):
            if command[0] == sys.executable:
                return real_run(command, **options)
            return None
        return self.run_check(*args, runner=runner, **kwargs)

    def test_a_passing_verify_passes_the_check_and_is_recorded(self):
        self.install_ship(SHIP_PASS)
        ok, line, commands = self.run_with_real_ship()
        self.assertTrue(ok, line)
        self.assertEqual(line['ship'], 'pass')
        report = self.report()
        self.assertEqual(report['ship'], SHIP_PASS)  # the parsed verify JSON, not a summary
        self.assertEqual(report['checks'][-1]['name'], 'ship')  # the last stage
        self.assertEqual(commands[-1], [sys.executable, str(self.root / 'scripts/ship.py'), 'verify', '--json'])
        self.assertEqual(self.marker.read_text(), 'verify --json')
        self.assertIn('ship.shortcut-file: no desktop', report['skipped'])
        self.assertIn('ship.launch: not requested (pass --launch)', report['skipped'])
        self.assertEqual(report['warnings'], ['ship.package: dist/x.exe is older than the release build'])
        self.assertEqual(line['warnings'], report['warnings'])

    def test_a_failing_verify_fails_the_whole_check_with_the_first_failing_check(self):
        self.install_ship(SHIP_FAIL, code=1)
        ok, line, _commands = self.run_with_real_ship()
        self.assertFalse(ok)
        self.assertEqual(line['ship'], 'fail')
        expected = ('ship gate: shortcut-file: C:\\Desktop\\Zed.lnk does not exist: run python scripts/ship.py shortcut. '
                    'Run: python scripts/ship.py ship')
        self.assertEqual(line['error'], expected)
        self.assertEqual(self.report()['error'], expected)
        self.assertEqual(self.report()['ship'], SHIP_FAIL)
        self.assertFalse(self.report()['ok'])

    def test_private_launcher_folder_is_forwarded_without_skipping_ship_verification(self):
        self.install_ship(SHIP_PASS)
        folder = self.root / 'private launchers'
        ok, line, commands = self.run_with_real_ship(ship_folder=folder)
        self.assertTrue(ok, line)
        self.assertEqual(line['ship'], 'pass')
        self.assertEqual(commands[-1][-2:], ['--folder', str(folder.resolve())])
        self.assertEqual(self.report()['checks'][-1]['name'], 'ship')

    def test_the_gate_runs_after_cargo_test_not_before(self):
        self.install_ship(SHIP_PASS)
        _ok, _line, commands = self.run_with_real_ship()
        self.assertEqual([c[1] for c in commands[:-1]], ['audit', 'lint', 'game-validate', 'metadata', 'test'])
        self.assertTrue(commands[-1][1].endswith('ship.py'))

    def test_a_crashing_or_silent_verify_cannot_pass(self):
        for name, kwargs in {'garbage': {'raw': 'Traceback (most recent call last)\n'},
                             'empty': {'raw': ''}, 'exit 2 with an error object': {
                                 'verdict': {'ok': False, 'error': 'not a game project'}, 'code': 2}}.items():
            with self.subTest(name):
                self.install_ship(**kwargs)
                ok, line, _commands = self.run_with_real_ship()
                self.assertFalse(ok)
                self.assertEqual(line['ship'], 'fail')
                self.assertTrue(line['error'].startswith('ship gate: '), line['error'])
                self.assertTrue(line['error'].endswith('Run: python scripts/ship.py ship'), line['error'])
        self.assertIn('not a game project', line['error'])

    def test_a_verdict_with_ok_false_and_no_failing_check_still_fails(self):
        self.install_ship({'ok': False, 'checks': [], 'skipped': []}, code=1)
        ok, line, _commands = self.run_with_real_ship()
        self.assertFalse(ok)

    def test_a_zero_exit_code_alone_is_not_enough(self):
        self.install_ship({'ok': False, 'checks': [{'name': 'wiring', 'status': 'fail', 'detail': 'x'}]}, code=0)
        ok, line, _commands = self.run_with_real_ship()
        self.assertFalse(ok)
        self.assertIn('wiring', line['error'])

    def test_skip_ship_runs_the_full_check_without_the_gate_and_says_so(self):
        self.install_ship(SHIP_FAIL, code=1)
        ok, line, commands = self.run_with_real_ship(skip_ship=True)
        self.assertTrue(ok)
        self.assertFalse(self.marker.exists())
        self.assertEqual(line['ship'], 'skipped: --skip-ship')
        self.assertEqual(self.report()['ship'], 'skipped: --skip-ship')
        self.assertIn('ship: skipped: --skip-ship', self.report()['skipped'])
        self.assertEqual(commands[-1], TEST)  # everything else still ran

    def test_content_only_never_runs_the_gate(self):
        self.install_ship(SHIP_FAIL, code=1)
        ok, line, commands = self.run_with_real_ship(True)
        self.assertTrue(ok)
        self.assertFalse(self.marker.exists())
        self.assertEqual(self.report()['ship'], 'not run: content-only')
        self.assertEqual(line['ship'], 'skipped: content-only')
        self.assertIn('ship: not run: content-only', self.report()['skipped'])

    def test_a_game_without_ship_py_is_skipped_visibly(self):
        ok, line, _commands = self.run_with_real_ship()
        self.assertTrue(ok)
        self.assertEqual(line['ship'], 'skipped: no scripts/ship.py')
        self.assertEqual(self.report()['ship'], 'skipped: no scripts/ship.py')

    def test_an_earlier_failure_means_the_gate_never_ran(self):
        self.install_ship(SHIP_PASS)

        def runner(command, options, real_run):
            if command == TEST:
                return subprocess.CompletedProcess(command, 101)
            return None

        ok, line, _commands = self.run_check(runner=runner)
        self.assertFalse(ok)
        self.assertFalse(self.marker.exists())
        self.assertEqual(line['ship'], 'skipped: an earlier check failed')
        self.assertIn('Command failed (101)', line['error'])

    def test_the_gate_has_its_own_log(self):
        self.install_ship(SHIP_PASS)
        self.run_with_real_ship()
        item = self.report()['checks'][-1]
        log = next((self.root / '.blue-check').glob('*/' + item['log']))
        self.assertIn('"ok": true', log.read_text())

    def test_main_passes_the_flags_through(self):
        calls = []
        with patch.object(game_check, 'run', side_effect=lambda *a: calls.append(a) or True), \
                patch.object(sys, 'argv', ['check.py', '--tools', 'x', '--skip-ship']):
            self.assertEqual(game_check.main(), 0)
        self.assertEqual(calls[0][2:], (False, [], True, None))
        with patch.object(game_check, 'run', side_effect=lambda *a: calls.append(a) or True), \
                patch.object(sys, 'argv', ['check.py', '--tools', 'x', '--content-only']):
            game_check.main()
        self.assertEqual(calls[1][2:], (True, [], False, None))

    def test_manual_text_names_the_evidence_and_what_still_needs_eyes(self):
        self.assertIn('dist/ship.json', game_check.MANUAL)
        self.assertIn('launch', game_check.MANUAL)
        self.assertIn('smoke', game_check.MANUAL)
        self.assertIn('captures', game_check.MANUAL)
        self.assertIn('controls', game_check.MANUAL)
        self.run_check(True)
        self.assertEqual(self.report()['manual'], game_check.MANUAL)


GIT = shutil.which('git')


def git(directory, *args):
    # Short-lived fixture repositories must not launch work that races their deletion.
    subprocess.run(['git', '-C', str(directory), '-c', 'user.name=t', '-c', 'user.email=t@example.com',
                    '-c', 'commit.gpgsign=false', '-c', 'maintenance.auto=false', '-c', 'gc.auto=0',
                    *args], check=True, capture_output=True,
                   creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))


@unittest.skipUnless(GIT, 'git is needed to make a stand-in engine checkout')
class EngineRevisionTests(RunnerCase):
    def setUp(self):
        super().setUp()
        self.game = self.root / 'game'
        (self.game / 'assets').mkdir(parents=True)
        self.engine = self.root / 'engine'
        self.engine.mkdir()
        git(self.engine, 'init', '-q')
        (self.engine / 'lib.rs').write_text('// engine')
        git(self.engine, 'add', '.')
        git(self.engine, 'commit', '-q', '-m', 'engine')
        self.head = subprocess.run(['git', '-C', str(self.engine), 'rev-parse', 'HEAD'], capture_output=True, text=True,
                                   check=True).stdout.strip()
        self.write_manifest('vesper3d = { package = "be2", path = "../engine", default-features = false }')
        self.identity({'engine_revision': self.head[:12]})

    def write_manifest(self, dependency):
        (self.game / 'Cargo.toml').write_text(f'[package]\nname = "g"\n\n[dependencies]\n{dependency}\n')

    def identity(self, data):
        (self.game / 'assets/identity.json').write_text(json.dumps(dict({'title': 'T'}, **data)))

    def test_matching_revision_is_silent(self):
        self.assertEqual(game_check.engine_warnings(self.game), [])
        self.identity({'engine_revision': self.head})  # a full hash also matches
        self.assertEqual(game_check.engine_warnings(self.game), [])
        self.identity({'engine_revision': self.head[:12].upper()})
        self.assertEqual(game_check.engine_warnings(self.game), [])

    def test_different_revision_warns_and_names_both(self):
        self.identity({'engine_revision': 'deadbeef0000'})
        warnings = game_check.engine_warnings(self.game)
        self.assertEqual(len(warnings), 1)
        self.assertIn('deadbeef0000', warnings[0])
        self.assertIn(self.head[:12], warnings[0])

    def test_the_warning_reaches_the_report_but_never_fails_the_check(self):
        (self.game / 'game.json').write_text(json.dumps({'map': 'maps/main.json'}))
        (self.game / 'maps').mkdir()
        (self.game / 'maps/main.json').write_text('{}')
        self.identity({'engine_revision': 'deadbeef0000'})
        self.root = self.game

        def real_git(command, options, real_run):  # everything else stays faked
            return real_run(command, **options) if command[0] == 'git' else None

        ok, line, _commands = self.run_check(True, runner=real_git)
        self.assertTrue(ok)
        self.assertEqual(len(line['warnings']), 1)
        self.assertEqual(self.report()['warnings'], line['warnings'])

    def test_the_other_dependency_spellings_are_found(self):
        self.identity({'engine_revision': 'deadbeef0000'})
        for dependency in ['be2 = { path = "../engine" }', 'vesper3d = { path = "../engine" }',
                           'other = { package = "be2", path = "../engine" }']:
            with self.subTest(dependency=dependency):
                self.write_manifest(dependency)
                self.assertEqual(len(game_check.engine_warnings(self.game)), 1)

    def test_python_3_10_style_manifest_reading_finds_the_engine_too(self):
        self.identity({'engine_revision': 'deadbeef0000'})
        with patch.dict(sys.modules, {'tomllib': None}):  # `import tomllib` raises ImportError
            self.assertEqual(game_check.engine_path(self.game), self.engine.resolve())
            self.assertEqual(len(game_check.engine_warnings(self.game)), 1)

    def test_nothing_to_compare_means_no_warning(self):
        self.identity({})  # no engine_revision recorded
        self.assertEqual(game_check.engine_warnings(self.game), [])
        self.identity({'engine_revision': 'deadbeef0000'})
        self.write_manifest('serde = "1"')  # no engine path dependency
        self.assertEqual(game_check.engine_warnings(self.game), [])
        self.write_manifest('be2 = { path = "../nowhere" }')  # a path that does not exist
        self.assertEqual(game_check.engine_warnings(self.game), [])
        (self.game / 'Cargo.toml').unlink()
        self.assertEqual(game_check.engine_warnings(self.game), [])
        (self.game / 'assets/identity.json').unlink()
        self.assertEqual(game_check.engine_warnings(self.game), [])
        (self.game / 'assets/identity.json').write_text('not json')
        self.assertEqual(game_check.engine_warnings(self.game), [])

    def test_no_git_means_no_warning(self):
        self.identity({'engine_revision': 'deadbeef0000'})
        with patch.object(game_check.shutil, 'which', return_value=None):
            self.assertEqual(game_check.engine_warnings(self.game), [])

    def test_an_engine_that_is_not_a_git_checkout_means_no_warning(self):
        self.identity({'engine_revision': 'deadbeef0000'})

        def remove(function, path, _error):  # git marks its object files read-only on Windows
            os.chmod(path, 0o700)
            function(path)

        handler = {'onexc': remove} if sys.version_info >= (3, 12) else {'onerror': remove}
        shutil.rmtree(self.engine / '.git', **handler)
        with patch.dict(os.environ, {'GIT_CEILING_DIRECTORIES': str(self.root.parent)}):  # do not find a repo above
            self.assertEqual(game_check.engine_warnings(self.game), [])


if __name__ == '__main__':
    unittest.main()
