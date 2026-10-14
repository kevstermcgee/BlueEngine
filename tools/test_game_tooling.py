"""Public project entry points, pinned distribution, safe migration and hooks."""
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import runpy
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

    def legacy_preparation_fixture(self):
        """A bounded historical script with an inventoried, game-owned preparation call."""
        engine = Path(self.scratch.name) / 'migration engine'
        bootstrap = tooling.module_from(self.engine / 'templates/game_entry.py', 'be2_fixture_bootstrap')
        for name in bootstrap.FILES:
            destination = engine / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(self.engine / name, destination)
        legacy = ('def prepare_soundtrack(project):\n    project.prepared.append("authored soundtrack")\n'
                  'def cmd_package(project):\n    prepare_soundtrack(project)\n')
        hook = 'def prepare_package(project):\n    project.prepared.append("authored soundtrack")\n'
        digest = hashlib.sha256(legacy.encode('utf-8')).hexdigest()
        inventory = {'compatible': {'ship': [digest]}, 'required_hooks': {'ship': {digest: {
            'preparation': 'prepare_soundtrack(project)', 'reference': 'reviewed/tooling_hooks.py',
            'sha256': hashlib.sha256(hook.encode('utf-8')).hexdigest()}}}}
        (engine / 'reviewed').mkdir()
        (engine / 'reviewed/tooling_hooks.py').write_text(hook, encoding='utf-8')
        (engine / 'tools/game_tooling_inventory.json').write_text(json.dumps(inventory), encoding='utf-8')
        (engine / 'PACKAGE-MANIFEST.json').write_text(json.dumps({'commit': 'historical-fixture'}), encoding='utf-8')
        (self.game / 'scripts/ship.py').write_text(legacy, encoding='utf-8')
        return engine, hook

    def test_legacy_preparation_needs_reviewed_hook_before_any_writes(self):
        engine, reviewed = self.legacy_preparation_fixture()
        completed = self.game / 'dist/completed.txt'
        completed.parent.mkdir()
        completed.write_text('completed native package', encoding='utf-8')
        hook = self.game / 'scripts/tooling_hooks.py'
        for existing in (None, 'def prepare_package(project): pass\n'):
            with self.subTest(existing_hook=existing):
                if existing is not None:
                    hook.write_text(existing, encoding='utf-8')
                before = {p.relative_to(self.game): p.read_bytes() for p in self.game.rglob('*') if p.is_file()}
                with self.assertRaisesRegex(ValueError, 'remove prepare_soundtrack.*extract it.*No files changed'):
                    tooling.migrate(self.game, engine)
                self.assertEqual(before, {p.relative_to(self.game): p.read_bytes()
                                          for p in self.game.rglob('*') if p.is_file()})
        hook.write_text(reviewed, encoding='utf-8')
        tooling.migrate(self.game, engine)
        tooling.check(self.game, engine)
        self.assertEqual(hook.read_text(encoding='utf-8'), reviewed)
        self.assertEqual(completed.read_text(encoding='utf-8'), 'completed native package')

    def test_migrated_legacy_preparation_keeps_behavior_before_build(self):
        engine, hook = self.legacy_preparation_fixture()
        project = type('Project', (), {'root': self.game, 'identity': object(), 'prepared': []})()
        legacy = tooling.module_from(self.game / 'scripts/ship.py', 'be2_historical_preparation')
        legacy.cmd_package(project)
        expected = project.prepared[:]
        project.prepared.clear()
        (self.game / 'scripts/tooling_hooks.py').write_bytes(hook.replace('\n', '\r\n').encode('utf-8'))
        tooling.migrate(self.game, engine)
        ship = runpy.run_path(str(self.game / 'scripts/ship.py'), run_name='be2_migrated_preparation')
        implementation = sys.modules['be2_shared_ship']
        with patch.object(implementation, 'require_icon'), patch.object(implementation, '_review_source', return_value=None), \
                patch.object(implementation, 'cargo_build_release', side_effect=RuntimeError('stop before build')):
            with self.assertRaisesRegex(RuntimeError, 'stop before build'):
                ship['cmd_package'](project)
        self.assertEqual(project.prepared, expected)

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

    def test_unavailable_engine_at_scaffold_requires_explicit_pin_upgrade(self):
        tooling.migrate(self.game)
        path = self.game / 'scripts/tooling.json'
        recorded = json.loads(path.read_text())
        recorded['sha256'] = None
        path.write_text(json.dumps(recorded))
        result = subprocess.run([sys.executable, str(self.game / 'scripts/check.py'), '--help'],
                                capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('unverified', result.stderr)
        self.assertIn('migrate GAME', result.stderr)

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

    def test_hook_helpers_and_dataclasses_work_from_direct_and_runpy_callers(self):
        helper_name = f'be2_hook_helper_{id(self):x}'
        self.addCleanup(sys.modules.pop, helper_name, None)
        (self.game / f'scripts/{helper_name}.py').write_text(
            'def prepare(project, value):\n    project.prepared = value\n', encoding='utf-8')
        (self.game / 'scripts/tooling_hooks.py').write_text(
            'from __future__ import annotations\nfrom dataclasses import dataclass\n'
            '@dataclass\nclass Recipe:\n    value: int = 7\n'
            f'def prepare_package(project):\n    from {helper_name} import prepare\n'
            '    prepare(project, Recipe().value)\n', encoding='utf-8')
        tooling.migrate(self.game)
        original_path = sys.path[:]
        for caller in ('direct', 'runpy'):
            with self.subTest(caller=caller):
                sys.modules.pop(helper_name, None)
                if caller == 'runpy':
                    runpy.run_path(str(self.game / 'scripts/ship.py'), run_name='be2_review_runpy')
                    ship = sys.modules['be2_shared_ship']
                else:
                    ship = tooling.module_from(self.engine / 'templates/game_ship.py', 'be2_context_test_ship')
                registered = {name: value for name, value in sys.modules.items()
                              if name.startswith('be2_game_preparation_')}
                project = type('Project', (), {'root': self.game, 'identity': object()})()
                with patch.object(ship, 'require_icon'), patch.object(ship, '_review_source', return_value=None), \
                        patch.object(ship, 'cargo_build_release',
                                                                     side_effect=RuntimeError('stop before build')):
                    with self.assertRaisesRegex(RuntimeError, 'stop before build'):
                        ship.cmd_package(project)
                self.assertEqual(project.prepared, 7)
                self.assertEqual(sys.path, original_path)
                self.assertEqual({name: value for name, value in sys.modules.items()
                                  if name.startswith('be2_game_preparation_')}, registered)

    def test_hook_import_failure_is_actionable_and_restores_caller_context(self):
        ship = tooling.module_from(self.engine / 'templates/game_ship.py', 'be2_import_error_ship')
        (self.game / 'scripts/tooling_hooks.py').write_text('import be2_missing_hook_dependency\n', encoding='utf-8')
        preserved = self.game / 'dist/completed.txt'
        preserved.parent.mkdir()
        preserved.write_text('completed package', encoding='utf-8')
        project = type('Project', (), {'root': self.game, 'identity': object()})()
        original_path = sys.path[:]
        with patch.object(ship, 'require_icon'), patch.object(ship, 'cargo_build_release') as build:
            with self.assertRaisesRegex(ship.ShipError, 'ModuleNotFoundError.*be2_missing_hook_dependency.*preserved'):
                ship.cmd_package(project)
            build.assert_not_called()
        self.assertEqual(preserved.read_text(encoding='utf-8'), 'completed package')
        self.assertEqual(sys.path, original_path)

    def test_packaged_binary_requires_verified_current_inputs(self):
        import hashlib
        engine = Path(self.scratch.name) / 'packaged engine'
        binary = engine / 'bin' / ('be2-tools.exe' if os.name == 'nt' else 'be2-tools')
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b'packaged native tool')
        source = engine / 'Cargo.toml'
        source.write_bytes(b'package inputs')
        records = {'be2/' + str(p.relative_to(engine)).replace('\\', '/'):
                   hashlib.sha256(p.read_bytes()).hexdigest() for p in (binary, source)}
        (engine / 'PACKAGE-MANIFEST.json').write_text(json.dumps({'format': 1, 'sha256': records}))
        with patch.object(tooling.subprocess, 'run') as build:
            self.assertEqual(tooling.fresh_tools(engine), str(binary))
            build.assert_not_called()
            source.write_bytes(b'changed source')
            with self.assertRaisesRegex(ValueError, 'Packaged tooling input changed'):
                tooling.fresh_tools(engine)

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

    def test_historical_preparation_inventory_names_matching_reviewed_hooks(self):
        inventory = json.loads((self.engine / 'tools/game_tooling_inventory.json').read_text())
        for command, variants in inventory['required_hooks'].items():
            for digest, required in variants.items():
                with self.subTest(command=command, preparation=required['preparation']):
                    self.assertIn(digest, inventory['compatible'][command])
                    reviewed = (self.engine / required['reference']).read_text(encoding='utf-8')
                    self.assertEqual(hashlib.sha256(reviewed.encode('utf-8')).hexdigest(), required['sha256'])


if __name__ == '__main__':
    unittest.main()
