"""Selection tests use real Git changes, not only hand-written path lists."""
import json
import os
import runpy
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace
from tools import workflow

ROOT = Path(__file__).resolve().parents[1]


class ContextTests(unittest.TestCase):
    def test_task_packet_and_diagnostic_routing(self):
        packet = workflow.context(ROOT, 'add replicated door state')
        self.assertEqual({item['id'] for item in packet['matches'][:2]},
                         {'multiplayer', 'game_documents'})
        self.assertIn('graphics', packet['probably_unnecessary'])
        self.assertIn('low', packet['confidence'])
        self.assertLess(len(json.dumps(packet)), 8000)
        self.assertLessEqual(sum(len(item['read_first']) for item in packet['matches']), 5)
        for code, owner in [('NET-BUDGET-002', 'multiplayer'),
                            ('NET-014', 'multiplayer'),
                            ('ARCH-HEADLESS-001', 'simulation_contract'),
                            ('API-PUBLIC-001', 'prototype_api')]:
            self.assertEqual(workflow.context(ROOT, code, 1)['matches'][0]['id'], owner)
        self.assertEqual(workflow.context(ROOT, 'zyxquantumunknown')['probably_unnecessary'], [])
        self.assertEqual(workflow.context(ROOT, 'TEST-SELECTION-001')['matches'][0]['id'], 'change_workflow')

    def test_boundary_failure_teaches_diagnostic_lookup(self):
        with patch('subprocess.run', return_value=SimpleNamespace(stdout='macroquad v0.4\n')):
            with self.assertRaisesRegex(SystemExit, 'ARCH-HEADLESS-001.*macroquad') as error:
                runpy.run_path(str(ROOT / 'tools/check_headless.py'), run_name='__main__')
        self.assertIn('python tools/be2.py context ARCH-HEADLESS-001', str(error.exception))

    def test_index_contracts_resolve_and_annotations_stay_queryable(self):
        features = workflow.index(ROOT)
        kinds = {'AI-INVARIANT', 'AI-BOUNDARY', 'AI-WARNING', 'AI-HOTPATH',
                 'AI-COMPAT', 'AI-SECURITY', 'AI-DEPRECATED', 'AI-CANONICAL'}
        ids = set()
        for name, feature in features.items():
            self.assertLess(len(json.dumps(workflow.context(ROOT, name, 1)).encode('utf-8')),
                            8000, f'INDEX-001 {name}: packet exceeds budget')
            for dependency in feature.get('depends_on', []):
                self.assertIn(dependency, features, f'INDEX-001 {name}: missing dependency')
            for path in feature.get('read_first', []) + ([feature['canonical_example']]
                                                        if 'canonical_example' in feature else []):
                self.assertTrue((ROOT / path).exists(), f'INDEX-001 {name}: {path}')
            for item in feature.get('constraints', []):
                self.assertNotIn(item['id'], ids, 'INDEX-001 duplicate diagnostic')
                ids.add(item['id'])
                self.assertIn(item['kind'], kinds)
                source = (ROOT / item['source']).read_text(encoding='utf-8')
                self.assertIn(item['kind'] + ' ' + item['id'] + ':', source,
                              f"INDEX-001 missing annotation; context {name}")
                self.assertTrue(item['verify'])
            for decision in feature.get('decisions', []):
                self.assertTrue((ROOT / decision['adr']).is_file())

    def test_impact_transitive_directory_unknown_and_cycles(self):
        result = workflow.impact(ROOT, ['src/viewer/simulation.rs', 'unknown.rs'])
        self.assertIn('shared_gameplay', result['affected'])
        self.assertEqual(result['unmapped'], ['unknown.rs'])
        self.assertIn('sandbox', workflow.impact(ROOT, ['src/bin/sandbox/input.rs'])['owners'])
        graph = {'a': {'depends_on': ['b']}, 'b': {'depends_on': ['a']}}
        self.assertEqual(workflow.closure(graph, ['a']), {'a', 'b'})
        self.assertIsNotNone(workflow.impact(ROOT, [f'unknown{i}' for i in range(11)])['scope_warning'])

    def test_bounded_exact_and_unknown(self):
        hit = workflow.context(ROOT, 'movement', 1)
        self.assertEqual(hit['matches'][0]['id'], 'movement')
        self.assertEqual(len(hit['matches']), 1)
        self.assertFalse(hit['engine_source_read'])
        self.assertLess(len(json.dumps(hit)), 3000)
        self.assertEqual(workflow.context(ROOT, 'zyxquantumunknown')['matches'], [])
        for query, limit, argument in [('', 3, 'query'), ('x' * 101, 3, 'query'), ('a', 0, 'limit'), ('a', 6, 'limit')]:
            with self.assertRaises(ValueError) as refused:
                workflow.context(ROOT, query, limit)
            self.assertIn(argument, str(refused.exception), 'the error names the argument at fault')
        with self.assertRaisesRegex(ValueError, '101'):
            workflow.context(ROOT, 'x' * 101, 3)

    def test_lookup_needs_only_feature_index(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'tools').mkdir()
            (root / 'tools/FEATURES.json').write_bytes((ROOT / 'tools/FEATURES.json').read_bytes())
            self.assertTrue(workflow.context(root, 'shared_gameplay')['matches'])


class DiskTests(unittest.TestCase):
    def test_low_space_warns_with_the_variables_that_move_cargo(self):
        usage = lambda path: SimpleNamespace(free=3e9 if 'small' in Path(path).parts else 900e9)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'small').mkdir()
            (root / 'big').mkdir()
            report = workflow.disk_report({'target': root / 'small' / 'not-yet' / 'target', 'temp': root / 'big'},
                                          usage=usage)
        self.assertEqual(report['locations']['target']['free_gb'], 3.0)
        self.assertEqual(report['locations']['temp']['free_gb'], 900.0)
        self.assertEqual(len(report['warnings']), 1)
        self.assertIn('target', report['warnings'][0])
        self.assertIn('CARGO_TARGET_DIR', report['hint'])
        self.assertIn('dist/', report['hint'])

    def test_plenty_of_space_or_an_unreadable_drive_is_quiet(self):
        roomy = workflow.disk_report({'target': ROOT}, usage=lambda p: SimpleNamespace(free=500e9))
        self.assertEqual((roomy['warnings'], roomy['hint']), ([], None))

        def broken(path):
            raise OSError('offline drive')
        self.assertEqual(workflow.disk_report({'target': ROOT}, usage=broken)['locations'], {})


class WindowsPlanTests(unittest.TestCase):
    """`check --windows` needs a Windows target and some C compiler for ring's build script, and says so."""

    def plan(self, tools, installed=True):
        which = lambda name: ('/usr/bin/' + name) if name in tools else None
        with tempfile.TemporaryDirectory() as directory:
            libdir = directory if installed else directory + '-missing'
            run = lambda *a, **k: SimpleNamespace(returncode=0, stdout=libdir + '\n')
            return workflow.windows_plan(which=which, run=run)

    def test_without_mingw_the_host_compiler_stands_in_for_the_type_check(self):
        plan = self.plan({'cargo', 'rustc', 'cc', 'ar'})
        self.assertEqual(plan['scope'], 'windows_typecheck')
        self.assertEqual(plan['env'], {'CC_x86_64_pc_windows_gnu': '/usr/bin/cc',
                                       'AR_x86_64_pc_windows_gnu': '/usr/bin/ar'})
        self.assertEqual(plan['commands'][0][:6], ['cargo', 'check', '--locked', '--target',
                                                   'x86_64-pc-windows-gnu', '--all-targets'])
        self.assertIn('--no-default-features', plan['commands'][1])
        self.assertIn('does NOT prove', plan['proves'])

    def test_a_real_mingw_gcc_is_used_untouched(self):
        self.assertEqual(self.plan({'cargo', 'rustc', 'x86_64-w64-mingw32-gcc'})['env'], {})

    def test_missing_pieces_fail_with_the_remedy_not_a_fake_pass(self):
        with self.assertRaisesRegex(RuntimeError, 'rustup target add'):
            self.plan({'cargo', 'rustc', 'cc', 'ar'}, installed=False)
        with self.assertRaisesRegex(RuntimeError, 'C compiler'):
            self.plan({'cargo', 'rustc'})
        with self.assertRaisesRegex(RuntimeError, 'toolchain'):
            self.plan(set())


class SelectionTests(unittest.TestCase):
    def test_iteration_reuses_index_and_never_claims_final_validation(self):
        plan = workflow.iteration_plan(ROOT, 'multiplayer', feature_mode='headless',
                                       test='replication_budget::oversized_world_makes_wire_progress')
        self.assertEqual(plan['scope'], 'iteration')
        self.assertEqual(len(plan['commands']), 1)
        self.assertEqual(plan['commands'][0][-3:], ['oversized_world_makes_wire_progress', '--', '--exact'])
        self.assertIn('--no-default-features', plan['commands'][0])
        self.assertIn('Final check', plan['remaining'])
        self.assertIn('--lib', workflow.iteration_plan(ROOT, 'movement', typecheck=True)['commands'][0])
        self.assertNotIn('--no-default-features', workflow.iteration_plan(ROOT, 'graphics', typecheck=True)['commands'][0])
        self.assertNotIn('--no-default-features', workflow.iteration_plan(
            ROOT, 'movement', typecheck=True, feature_mode='default')['commands'][0])
        self.assertEqual(workflow.iteration_plan(ROOT, 'change_workflow')['test_harness'], 'python')
        for kwargs in [{'test': 'unindexed'}, {'test': 'replication_budget::'},
                       {'typecheck': True, 'test': 'replication_budget'}, {'feature_mode': 'typo'}]:
            with self.assertRaises(ValueError):
                workflow.iteration_plan(ROOT, 'multiplayer', **kwargs)

    def test_all_original_engine_gates_remain(self):
        commands = workflow.full_commands()
        self.assertIn(['cargo', 'fmt', '--check'], commands)
        for features in ([], ['--no-default-features']):
            self.assertIn(['cargo', 'test', '--locked', '--profile', 'itest', *features], commands)
            self.assertIn(['cargo', 'test', '--locked', *features], workflow.full_commands('dev'))
            self.assertIn(['cargo', 'clippy', '--all-targets', '--locked', *features,
                           '--', '-D', 'warnings'], commands)
            self.assertIn(['cargo', 'rustdoc', '--locked', '--lib', *features,
                           '--', '-D', 'warnings'], commands)
        for script in ['tools/check_headless.py', 'tools/check_authoring.py']:
            self.assertIn([sys.executable, script], commands)

    def test_iteration_tests_use_the_fast_profile_unless_dev_is_asked_for(self):
        command = workflow.iteration_plan(ROOT, 'multiplayer', feature_mode='headless',
                                          test='replication_budget')['commands'][0]
        self.assertEqual(command[:5], ['cargo', 'test', '--locked', '--profile', 'itest'])
        command = workflow.iteration_plan(ROOT, 'multiplayer', feature_mode='headless',
                                          test='replication_budget', test_profile='dev')['commands'][0]
        self.assertNotIn('--profile', command)
        with self.assertRaises(ValueError):
            workflow.full_commands('release')

    def test_scopes_union_and_fail_closed(self):
        plan = workflow.validation_plan(['tools/author.py', 'tools/assets.py'])
        self.assertEqual(plan['scope'], 'author+assets')
        self.assertEqual(len(plan['commands']), 3)
        for path in ['src/viewer/controller.rs', 'Cargo.lock', 'README.md',
                     'tools/workflow.py', 'tools/FEATURES.json', '.github/workflows/ci.yml',
                     'assets/games/new/game.json', 'unrecognized.py']:
            plan = workflow.validation_plan(['tools/assets.py', path])
            self.assertEqual(plan['scope'], 'full', path)
            self.assertEqual(plan['commands'], workflow.full_commands())
        self.assertEqual(workflow.validation_plan([])['scope'], 'no_changes')
        self.assertEqual(workflow.validation_plan([])['commands'], [])

    def test_real_git_includes_staged_unstaged_untracked_deleted_and_renamed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def git(*args):
                return subprocess.run(['git', *args], cwd=root, check=True,
                                      capture_output=True, text=True, **workflow.console_options()).stdout.strip()
            git('init')
            git('config', 'user.email', 'test@example.invalid')
            git('config', 'user.name', 'Workflow Test')
            for name in ['staged.py', 'unstaged.py', 'deleted.py', 'old name.py']:
                (root / name).write_text('old\n')
            git('add', '.')
            git('commit', '-m', 'base')
            base = git('rev-parse', 'HEAD')
            (root / 'staged.py').write_text('new\n')
            git('add', 'staged.py')
            (root / 'unstaged.py').write_text('new\n')
            (root / 'deleted.py').unlink()
            git('mv', 'old name.py', 'new name.py')
            (root / 'untracked space.py').write_text('new\n')
            revision, paths = workflow.changed_paths(root)
            self.assertEqual(revision, base)
            self.assertEqual(set(paths), {'staged.py', 'unstaged.py', 'deleted.py',
                                         'old name.py', 'new name.py', 'untracked space.py'})
            git('add', '.')
            git('commit', '-m', 'changed')
            self.assertEqual(workflow.changed_paths(root)[1], [])
            self.assertEqual(workflow.changed_paths(root, base)[1], paths)
            for invalid in ['nonexistent', '--help']:
                with self.assertRaises(subprocess.CalledProcessError):
                    workflow.changed_paths(root, invalid)


class RunnerTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory(prefix='be2-runner-')
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        (self.root / 'tools').mkdir()
        for name in ['be2.py', 'workflow.py', 'upgrade.py']:
            shutil.copyfile(ROOT / 'tools' / name, self.root / 'tools' / name)

    def run_check(self, command, harness=None, timeout=None, later=True):
        plan = {'scope': 'iteration', 'feature': 'fixture', 'feature_mode': 'headless',
                'proves': 'Fixture only', 'remaining': 'Full verification remains',
                'test_harness': harness, 'commands': [command] +
                ([[sys.executable, '-c', 'print("later-command")']] if later else [])}
        # Drive the real CLI/exit path, replacing only the command plan. Temporary
        # copies keep fixture logs out of the engine checkout and need no Cargo deps.
        script = ('import sys,runpy; sys.path.insert(0,"tools"); import workflow; '
                  'workflow.validation_plan=lambda *args: ' + repr(plan) + '; '
                  'sys.argv=["be2.py","check"]' +
                  ('+["--timeout",' + repr(str(timeout)) + ']' if timeout else '') + '; '
                  'runpy.run_path("tools/be2.py",run_name="__main__")')
        result = subprocess.run([sys.executable, '-c', script], cwd=self.root,
                                capture_output=True, text=True,
                                env={**os.environ, 'CARGO_TARGET_DIR': str(self.root / 'target')},
                                **workflow.console_options())
        summary = json.loads(result.stdout)
        report = json.loads(Path(summary['report']).read_text())
        return result, summary, report

    def test_original_exit_status_and_complete_failure_log(self):
        result, summary, report = self.run_check(
            [sys.executable, '-c', 'import sys; print("evidence:"+"x"*12000); sys.exit(7)'])
        self.assertEqual(result.returncode, 7)
        self.assertFalse(summary['ok'])
        self.assertEqual(len(report['checks']), 1)
        self.assertEqual(summary['failure']['returncode'], 7)
        self.assertEqual(summary['failure']['category'], 'command_failure')
        self.assertGreater(Path(summary['failure']['log']).stat().st_size, 12000)
        self.assertEqual(summary['failure']['command'], summary['failure']['reproduction'])
        self.assertLess(len(result.stdout), 4000)

    def test_timeout_and_missing_tool_are_distinct_and_keep_logs(self):
        result, summary, _ = self.run_check(
            [sys.executable, '-c', 'import time; print("before timeout",flush=True); time.sleep(30)'], timeout=2)
        self.assertEqual(result.returncode, 124)
        self.assertEqual(summary['failure']['category'], 'timeout')
        self.assertIsNone(summary['failure']['returncode'])
        self.assertIn('before timeout', Path(summary['failure']['log']).read_text())
        result, summary, _ = self.run_check([str(self.root / 'missing-tool')])
        self.assertEqual(result.returncode, 127)
        self.assertEqual(summary['failure']['category'], 'missing_tool')
        self.assertTrue(Path(summary['failure']['log']).is_file())

    def test_empty_and_unrecognized_test_output_cannot_pass(self):
        for output, category in [('test result: ok. 0 passed; 0 failed; 0 ignored;', 'empty_test_selection'),
                                 ('unknown harness output', 'missing_test_evidence')]:
            result, summary, _ = self.run_check([sys.executable, '-c', 'print(' + repr(output) + ')'], 'rust')
            self.assertEqual(result.returncode, 3)
            self.assertEqual(summary['failure']['returncode'], 0)
            self.assertEqual(summary['failure']['category'], category)
        result, summary, _ = self.run_check([sys.executable, '-c',
            'print("Ran 1 test in 0.01s\\n\\nOK (skipped=1)")'], 'python')
        self.assertEqual(result.returncode, 3)
        self.assertEqual(summary['failure']['category'], 'empty_test_selection')

    @unittest.skipUnless(shutil.which('cargo'), 'Cargo needed for real diagnostic integration')
    def test_real_cargo_compiler_assertion_and_empty_selection(self):
        (self.root / 'Cargo.toml').write_text('[package]\nname="runner-fixture"\nversion="0.1.0"\nedition="2021"\n')
        (self.root / 'src').mkdir()
        source = self.root / 'src/lib.rs'
        source.write_text('pub fn value() -> u32 { "wrong type" }\n')
        cmd = ['cargo', 'test', '--offline', '--lib', '--message-format=json']
        result, summary, _ = self.run_check(cmd, 'rust')
        self.assertEqual(result.returncode, 101)
        self.assertEqual(summary['failure']['category'], 'compiler')
        self.assertEqual(summary['failure']['diagnostics'][0]['code'], 'E0308')
        self.assertEqual(summary['failure']['diagnostics'][0]['location']['line'], 1)
        source.write_text('#[test] fn regression() { assert_eq!(1, 2); }\n')
        result, summary, _ = self.run_check(cmd, 'rust')
        self.assertEqual(result.returncode, 101)
        self.assertEqual(summary['failure']['category'], 'test_failure')
        self.assertIn('assertion', json.dumps(summary['failure']['diagnostics']))
        self.assertEqual(summary['failure']['location']['line'], 1)
        source.write_text('#[test] fn regression() { assert_eq!(2, 2); }\n')
        # The runner must stop a misspelled test before the later command runs.
        result, summary, report = self.run_check(cmd + ['missing_test', '--', '--exact'], 'rust')
        self.assertEqual(result.returncode, 3)
        self.assertEqual(len(report['checks']), 1)
        result, summary, report = self.run_check(cmd + ['regression', '--', '--exact'], 'rust', later=False)
        self.assertEqual(report['checks'][0]['tests_executed'], 1)
        self.assertTrue(report['checks'][0]['ok'])
        self.assertEqual(result.returncode, 0)
        self.assertTrue(summary['ok'])
        self.assertEqual(summary['scope'], 'iteration')
        self.assertTrue(summary['remaining'])


class ConsoleTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform == 'win32', 'Windows console inheritance')
    def test_child_and_grandchild_consoles_stay_hidden(self):
        from tools.test_game_check import game_check
        probe = ('import ctypes; '
                 'k=ctypes.windll.kernel32; k.GetConsoleWindow.restype=ctypes.c_void_p; '
                 'u=ctypes.windll.user32; u.IsWindowVisible.argtypes=[ctypes.c_void_p]; '
                 'h=k.GetConsoleWindow(); print(bool(h), bool(u.IsWindowVisible(h)))')
        child = (probe + '; import subprocess,sys; '
                 'subprocess.run([sys.executable,"-c",' + repr(probe) + '],check=True)')
        for options in [workflow.console_options(), game_check.console_options()]:
            result = subprocess.run([sys.executable, '-c', child], capture_output=True,
                                    text=True, check=True, **options)
            self.assertEqual(result.stdout.splitlines(), ['True False', 'True False'])


if __name__ == '__main__':
    unittest.main()
