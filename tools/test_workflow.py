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


class BuildDeliveryTests(unittest.TestCase):
    def namespace(self):
        with patch.dict(sys.modules, workflow=workflow), patch('sys.path', [str(ROOT / 'tools'), *sys.path]):
            return runpy.run_path(str(ROOT / 'tools/be2.py'))

    def test_client_and_explicit_cinematic_commands(self):
        ns = self.namespace()
        calls = []
        ns['build'].__globals__['invoke'] = lambda args, **kwargs: calls.append(args)
        ns['build']('client')
        self.assertEqual(calls[-1], ['cargo', 'build', '--release', '--locked', '--bin', 'be2'])
        ns['build']('cinematic')
        self.assertEqual(calls[-1], ['cargo', 'build', '--release', '--locked', '--no-default-features',
                                   '--features', 'offline', '--bin', 'vesper3d'])

    def test_default_package_excludes_even_a_stale_cinematic_executable(self):
        import zipfile
        ns = self.namespace()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for kind, names in [('client', ['be2', 'vesper3d']), ('headless', ['be2-headless']), ('tools', ['be2-tools'])]:
                folder = root / kind
                folder.mkdir()
                for name in names:
                    (folder / (name + ns['SUFFIX'])).write_bytes(b'fixture')
            globals_ = ns['package'].__globals__
            globals_['ROOT'] = root
            globals_['build'] = lambda kind: root / kind
            globals_['invoke'] = lambda argv, **kwargs: 'a' * 40 if argv[1] == 'rev-parse' else ''
            archive = root / 'engine.zip'
            ns['package'](archive)
            with zipfile.ZipFile(archive) as payload:
                names = payload.namelist()
            self.assertIn('be2/bin/BE2' + ns['SUFFIX'], names)
            self.assertFalse(any('vesper3d' in name for name in names), names)

    def test_defaults_exclude_cinematic_but_keep_public_library(self):
        import tomllib
        manifest = tomllib.loads((ROOT / 'Cargo.toml').read_text())
        self.assertEqual(manifest['features']['default'], ['client'])
        self.assertEqual(manifest['lib']['name'], 'vesper3d')
        cinematic = next(item for item in manifest['bin'] if item['name'] == 'vesper3d')
        self.assertEqual(cinematic['required-features'], ['offline'])


@unittest.skipUnless(sys.platform == 'win32', 'Windows compatibility launcher')
class WindowsLauncherTests(unittest.TestCase):
    def test_old_entry_routes_workbench_and_preserves_stock_arguments_without_building(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ('launch_bea.bat', 'BEA_Launcher.ps1'):
                shutil.copy2(ROOT / name, root / name)
            (root / 'cargo.bat').write_text('@echo off\necho ROUTE:%*\n', encoding='utf-8')
            env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ['PATH'])
            for args, expected in [([], '--bin blueengine-sandbox --'),
                                   (['--map', 'assets/maps/test.json', '--feta'], '--bin be2 -- --map assets/maps/test.json --feta')]:
                result = subprocess.run(['powershell.exe', '-NoProfile', '-ExecutionPolicy', 'Bypass',
                                         '-File', str(root / 'BEA_Launcher.ps1'), *args],
                                        cwd=root, env=env, capture_output=True, text=True, timeout=30,
                                        **workflow.console_options())
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn(expected, result.stdout)


class ContextTests(unittest.TestCase):
    def test_stock_audio_pipeline_and_bindings_are_discoverable(self):
        packet = workflow.context(ROOT, 'stock GameDocument audio pipeline cues and adaptive music')
        feature = packet['matches'][0]
        self.assertEqual(feature['id'], 'audio_authoring')
        self.assertIn('docs/AUDIO.md', feature['read_first'])
        self.assertIn('StockAudio', feature['public_api'])
        self.assertIn('AudioCursor', feature['public_api'])
        self.assertFalse(packet['engine_source_read'])

    def test_named_audio_authoring_is_discoverable_without_source_exploration(self):
        packet = workflow.context(ROOT, 'compose stereo music score with MIDI notes and preview named sound effects')
        feature = packet['matches'][0]
        self.assertEqual(feature['id'], 'audio_authoring')
        self.assertIn('docs/AUDIO.md', feature['read_first'])
        self.assertEqual(feature['canonical_example'], 'examples/audio_preview.rs')
        self.assertIn('AudioBank', feature['public_api'])
        self.assertFalse(packet['engine_source_read'])

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
        for query, limit, argument in [('', 3, 'query'), ('x' * 501, 3, 'query'), ('a', 0, 'limit'), ('a', 6, 'limit')]:
            with self.assertRaises(ValueError) as refused:
                workflow.context(ROOT, query, limit)
            self.assertIn(argument, str(refused.exception), 'the error names the argument at fault')
        with self.assertRaisesRegex(ValueError, '501'):
            workflow.context(ROOT, 'x' * 501, 3)
        task=('Create a portable 2D game with five artifact pickups, a locked exit, timer loss, '
              'restart and engine-owned save/load. Assert collisions and exact save continuation, '
              'inspect desktop/mobile captures and package it for browser.')
        packet=workflow.context(ROOT,task,level=1)
        self.assertTrue(packet['matches'])
        self.assertTrue(all(p.endswith('.md') for match in packet['matches'] for p in match['read_first']))
        self.assertLess(len(json.dumps(packet)),8000)

    def test_lookup_needs_only_feature_index(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'tools').mkdir()
            (root / 'tools/FEATURES.json').write_bytes((ROOT / 'tools/FEATURES.json').read_bytes())
            self.assertTrue(workflow.context(root, 'shared_gameplay')['matches'])


class AutomaticLoopTests(unittest.TestCase):
    def test_reproducible_microbenchmark_has_seven_tasks_and_no_builds_by_default(self):
        from tools import dev_bench
        result = dev_bench.measure(ROOT)
        self.assertEqual(len(result['rows']), 7)
        self.assertTrue(all(row['total_task_seconds'] is None for row in result['rows']))
        self.assertTrue(all('verification_seconds' not in row for row in result['rows']))
        self.assertTrue(all(row['context_level'] == 1 for row in result['rows']))

    def test_transitive_consumers_and_library_units_are_selected(self):
        plan = workflow.change_plan(ROOT, ['src/viewer/net/replication.rs'])
        self.assertEqual(plan['scope'], 'focused')
        command = plan['commands'][0]
        for arg in ('--lib', '--no-default-features', 'replication_budget', 'hub', 'netplay', 'shared_gameplay'):
            self.assertIn(arg, command)
        self.assertNotIn('audio_project', command)
        self.assertEqual(plan['requirements']['full_suite'], 'before merge')
        self.assertEqual(len(plan['commands']), len(plan['command_harnesses']))

    def test_integration_covers_both_feature_modes_and_portable_mode(self):
        plan = workflow.change_plan(ROOT, ['src/two_d/mod.rs'], loop='integration')
        rust = [c for c in plan['commands'] if c[:2] == ['cargo', 'test']]
        self.assertEqual(len(rust), 3)
        self.assertNotIn('--no-default-features', rust[0])
        self.assertIn('--no-default-features', rust[1])
        self.assertIn('two-d', rust[2])
        self.assertIn('packaging', plan['requirements'])

    def test_feature_guarded_schema_suite_runs_separately_from_normal_units(self):
        plan = workflow.change_plan(ROOT, ['src/viewer/game.rs'])
        rust = [command for command in plan['commands'] if command[:2] == ['cargo', 'test']]
        self.assertNotIn('schema-validation', rust[0])
        self.assertNotIn('authoring_schemas', rust[0])
        guarded = [command for command in rust if 'authoring_schemas' in command]
        self.assertEqual(len(guarded), 1)
        self.assertIn('schema-validation', guarded[0])
        self.assertIn('--no-default-features', guarded[0])
        self.assertNotIn('--lib', guarded[0])
        exact = workflow.iteration_plan(ROOT, 'game_documents', test='authoring_schemas')
        self.assertIn('schema-validation', exact['commands'][0])

    def test_unknown_build_and_content_inputs_fail_closed(self):
        for path in ('src/new.rs', 'Cargo.toml', 'tools/FEATURES.json', '.github/workflows/ci.yml',
                     'assets/games/observatory/content/game.json', 'tools/place_interior.py'):
            plan = workflow.change_plan(ROOT, [path])
            self.assertEqual(plan['scope'], 'full', path)
            self.assertEqual(plan['commands'], workflow.full_commands())
        mixed = workflow.change_plan(ROOT, ['tools/assets.py', 'unknown.py'])
        self.assertEqual(mixed['scope'], 'full')
        self.assertEqual(workflow.change_plan(ROOT, [])['commands'], [])

    def test_shipping_selection_is_unchanged_and_python_needs_no_cargo(self):
        self.assertEqual(workflow.change_plan(ROOT, ['tools/assets.py'], loop='shipping'),
                         workflow.validation_plan(['tools/assets.py']))
        plan = workflow.change_plan(ROOT, ['tools/assets.py'])
        self.assertFalse(any(c[0] == 'cargo' for c in plan['commands']))
        self.assertIn([sys.executable, 'tools/assets.py', 'validate'], plan['commands'])

    def test_game_checks_use_own_gates_and_never_the_engine_suite(self):
        game = ROOT / 'games/lantern-run'
        inner = workflow.game_plan(ROOT, game)
        self.assertEqual(len(inner['commands']), 1)
        self.assertIn(str(game / 'Cargo.toml'), inner['commands'][0])
        self.assertIn('--no-default-features', inner['commands'][0])
        self.assertEqual(inner['command_harnesses'], ['rust_project'])
        ship = workflow.game_plan(ROOT, game, 'shipping')
        self.assertEqual(ship['commands'], [
            [sys.executable, str(game / 'scripts/check.py'), '--skip-ship'],
            [sys.executable, str(game / 'scripts/ship.py'), 'ship', '--no-install']])
        self.assertEqual(ship['command_roles'], ['game_check', 'game_ship'])
        self.assertFalse(any('web' in command for command in ship['commands']))
        self.assertNotIn(['cargo', 'test', '--locked', '--profile', 'itest'], ship['commands'])

    def test_progressive_context_and_index_validation(self):
        small = workflow.context(ROOT, 'two_dimensional', level=1)
        full = workflow.context(ROOT, 'two_dimensional')
        self.assertLess(len(json.dumps(small)), len(json.dumps(full)))
        self.assertTrue(all(p.endswith('.md') for p in small['matches'][0]['read_first']))
        detailed = workflow.context(ROOT, 'two_dimensional', level=3)['matches'][0]['map']
        self.assertIn('src/two_d/mod.rs', detailed['implementation'])
        self.assertIn('portable_storage', detailed['tests'])
        self.assertTrue(workflow.validate_index(ROOT)['ok'])
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); (root / 'tools').mkdir()
            (root / 'tools/FEATURES.json').write_bytes((ROOT / 'tools/FEATURES.json').read_bytes())
            self.assertFalse(workflow.validate_index(root)['ok'])

    def test_scaffold_registers_its_root_once_before_locked_tests(self):
        with tempfile.TemporaryDirectory() as directory:
            game = Path(directory)
            shutil.copyfile(ROOT / 'games/lantern-run/game.project.json', game / 'game.project.json')
            (game / 'Cargo.toml').write_text('[package]\nname="new-root"\nversion="0.1.0"\n')
            first = workflow.game_plan(ROOT, game)
            self.assertEqual(first['commands'][0][1], 'metadata')
            self.assertIn('--locked', first['commands'][1])
            (game / 'Cargo.lock').write_text('[[package]]\nname="new-root"\nversion="0.1.0"\n')
            ready = workflow.game_plan(ROOT, game)
            self.assertEqual(len(ready['commands']), 1)
            self.assertEqual(ready['commands'][0][1], 'test')

    @unittest.skipUnless(shutil.which('cargo'), 'Cargo required for fresh-lock regression')
    def test_fresh_lock_setup_actually_allows_locked_cargo(self):
        with tempfile.TemporaryDirectory() as directory:
            game = Path(directory)
            shutil.copyfile(ROOT / 'games/lantern-run/game.project.json', game / 'game.project.json')
            (game / 'Cargo.toml').write_text('[package]\nname="new-root"\nversion="0.1.0"\nedition="2021"\n')
            (game / 'src').mkdir()
            (game / 'src/lib.rs').write_text('pub fn ready() {}\n')
            # Scaffold locks initially describe the engine, not the new root.
            (game / 'Cargo.lock').write_text('version = 3\n[[package]]\nname="old-root"\nversion="0.1.0"\n')
            setup = workflow.game_plan(ROOT, game)['commands'][0]
            result = subprocess.run(setup, capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stderr)
            locked = subprocess.run(['cargo', 'metadata', '--locked', '--offline', '--format-version', '1',
                                     '--manifest-path', str(game / 'Cargo.toml')],
                                    capture_output=True, text=True, timeout=30)
            self.assertEqual(locked.returncode, 0, locked.stderr)
            self.assertEqual(workflow.game_plan(ROOT, game)['commands'][0][1], 'test')

    def test_ci_keeps_canonical_shipping_and_aggregate_gates(self):
        ci = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertIn('run: python tools/be2.py check', ci)
        for gate in ('cargo build --release --locked', 'needs: [engine, leo, idea-forge]',
                     'Stock audio offscreen verification', 'test "$FORGE_RESULT" = success',
                     'Windows package resources and isolated smoke'):
            self.assertIn(gate, ci)
        self.assertNotIn('wasm32-unknown-unknown', ci)
        self.assertNotIn('npm ci', ci)
        # Both platform and native/headless/portable checks remain in full_commands.
        self.assertIn('os: [ubuntu-latest, windows-latest]', ci)
        self.assertEqual(ci.count('run: python tools/be2.py check'), 1)


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
            self.assertIn(['cargo', 'clippy', '--all-targets', '--locked', '--profile', 'itest', *features,
                           '--', '-D', 'warnings'], commands)
            self.assertIn(['cargo', 'rustdoc', '--locked', '--lib', '--profile', 'itest', *features,
                           '--', '-D', 'warnings'], commands)
        for script in ['tools/check_headless.py', 'tools/check_authoring.py']:
            self.assertIn([sys.executable, script], commands)
        self.assertIn([sys.executable, 'tools/check_authoring.py', '--profile', 'dev'],
                      workflow.full_commands('dev'))
        for command in workflow.full_commands('dev'):
            if command[0] == 'cargo':
                self.assertNotIn('--profile', command)
        plan = workflow.validation_plan()
        self.assertEqual(plan['independent_commands'], [len(commands) - 1])
        self.assertEqual(commands[-1][1:3], ['-m', 'unittest'])
        self.assertNotIn('tools.test_author', commands[-1])

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
        learning = workflow.validation_plan(['docs/learning/ledger.jsonl', 'tools/learn.py'])
        self.assertEqual(learning['scope'], 'learning')
        self.assertEqual(learning['commands'], [[sys.executable, '-m', 'unittest', 'tools.test_learn']])
        self.assertEqual(workflow.validation_plan(['docs/learning/ledger.jsonl', 'docs/adr/README.md'])['scope'], 'full')
        self.assertIn('tools.test_learn', workflow.full_commands()[-1])
        self.assertIn('tools.test_hub_deploy', workflow.full_commands()[-1])
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

    def run_check(self, command, harness=None, timeout=None, later=True, independent=None,
                  following=None, serial=False, fixed_stamp=False):
        plan = {'scope': 'iteration', 'feature': 'fixture', 'feature_mode': 'headless',
                'proves': 'Fixture only', 'remaining': 'Full verification remains',
                'test_harness': harness, 'commands': [command] +
                (following if following is not None else
                 [[sys.executable, '-c', 'print("later-command")']] if later else []),
                'independent_commands': independent or []}
        # Drive the real CLI/exit path, replacing only the command plan. Temporary
        # copies keep fixture logs out of the engine checkout and need no Cargo deps.
        script = ('import sys,runpy; sys.path.insert(0,"tools"); import workflow; '
                  'workflow.validation_plan=lambda *args: ' + repr(plan) + '; '
                  'sys.argv=["be2.py","check"]' +
                  ('+["--timeout",' + repr(str(timeout)) + ']' if timeout else '') + '; '
                  + ('sys.argv += ["--serial"]; ' if serial else '') +
                  ('import datetime; from unittest.mock import patch; '
                   'clock=patch.object(datetime,"datetime").start(); '
                   'clock.now.return_value.strftime.return_value="same-timestamp"; ' if fixed_stamp else '') +
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
        self.assertIn('[1/2]', result.stderr)
        self.assertIn('FAILED: command_failure', result.stderr)

    def test_game_manifests_share_cargo_outputs_without_moving_python_fixtures(self):
        from tools import upgrade
        with patch.dict(sys.modules, {'workflow': workflow, 'upgrade': upgrade}):
            namespace = runpy.run_path(str(ROOT / 'tools/be2.py'))
        check = namespace['check']; seen = []
        def invoke(command, **options):
            seen.append((command, options['env']))
            return {'returncode': 0, 'elapsed_seconds': 0, 'log_bytes': 0}
        plan = {'scope': 'full', 'commands': [
            ['cargo', 'test', '--manifest-path', 'games/a/Cargo.toml'],
            ['cargo', 'test', '--manifest-path', 'games/b/Cargo.toml'],
            [sys.executable, '-m', 'unittest', 'independent_fixture']]}
        with patch.dict(check.__globals__, {'ROOT': self.root, 'WORK': self.root / '.be2-work', 'invoke': invoke}), \
                patch.dict(os.environ, {'CARGO_TARGET_DIR': ''}):
            check(plan)
        self.assertEqual(seen[0][1]['CARGO_TARGET_DIR'], str((self.root / 'target').resolve()))
        self.assertEqual(seen[0][1]['CARGO_TARGET_DIR'], seen[1][1]['CARGO_TARGET_DIR'])
        self.assertIsNone(seen[2][1])

    def test_progress_preserves_json_and_stage_timings(self):
        result, summary, report = self.run_check([sys.executable, '-c', 'print("fixture")'])
        self.assertEqual(result.returncode, 0)
        self.assertTrue(summary['ok'])
        self.assertIn('[1/2]', result.stderr)
        self.assertIn('[2/2]', result.stderr)
        self.assertEqual(result.stderr.count('PASS ('), 2)
        self.assertGreaterEqual(report['elapsed_seconds'],
                                sum(item['elapsed_seconds'] for item in report['checks']) - .002)
        self.assertTrue(all(item['log_bytes'] > 0 for item in report['checks']))

    def barrier_command(self, own, other, ending='print("done")'):
        # File barriers prove actual overlap, without timing-speed assertions.
        return [sys.executable, '-c',
                'from pathlib import Path; import time; '
                f'Path({own!r}).touch(); end=time.monotonic()+5; '
                f'exec("while not Path({other!r}).exists() and time.monotonic()<end: time.sleep(.01)"); '
                f'assert Path({other!r}).exists(), "other lane did not start"; {ending}']

    def test_independent_lane_overlaps_and_report_waits_for_both(self):
        first = self.barrier_command('first', 'second')
        second = self.barrier_command('second', 'first')
        result, summary, report = self.run_check(first, following=[second], independent=[1])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(summary['ok'])
        self.assertEqual(report['independent_commands'], [1])
        self.assertEqual([item['step'] for item in report['checks']], [1, 2])
        self.assertTrue(all(item['ok'] for item in report['checks']))
        self.assertTrue(all('done' in (Path(summary['report']).parent / item['log']).read_text()
                            for item in report['checks']))

    def test_failure_joins_started_lane_and_does_not_start_pending_gate(self):
        first = self.barrier_command('first', 'second', 'import sys; sys.exit(7)')
        second = self.barrier_command('second', 'first', 'Path("joined").touch(); print("done")')
        pending = [sys.executable, '-c', 'from pathlib import Path; Path("pending").touch()']
        result, summary, report = self.run_check(first, following=[second, pending], independent=[1])
        self.assertEqual(result.returncode, 7)
        self.assertFalse(summary['ok'])
        self.assertEqual(report['commands_attempted'], 2)
        self.assertTrue((self.root / 'joined').exists())
        self.assertFalse((self.root / 'pending').exists())

    def test_independent_failure_cannot_certify_serial_success(self):
        first = self.barrier_command('first', 'second')
        second = self.barrier_command('second', 'first', 'import sys; sys.exit(7)')
        result, summary, report = self.run_check(first, following=[second], independent=[1])
        self.assertEqual(result.returncode, 7)
        self.assertFalse(summary['ok'])
        self.assertEqual(report['failed_commands'], 1)

    def test_serial_opt_out_retains_all_commands(self):
        result, summary, report = self.run_check([sys.executable, '-c', 'print("first")'],
                                                independent=[1], serial=True)
        self.assertEqual(result.returncode, 0)
        self.assertTrue(summary['ok'])
        self.assertEqual(report['independent_commands'], [])
        self.assertEqual(report['commands_attempted'], 2)

    def test_repeated_timestamp_keeps_original_engine_evidence(self):
        _, first, _ = self.run_check([sys.executable, '-c', 'import sys; print("failure"); sys.exit(7)'],
                                    fixed_stamp=True)
        _, second, _ = self.run_check([sys.executable, '-c', 'print("success")'], fixed_stamp=True)
        self.assertNotEqual(first['report'], second['report'])
        self.assertFalse(json.loads(Path(first['report']).read_text())['ok'])
        self.assertTrue(json.loads(Path(second['report']).read_text())['ok'])
        self.assertEqual((Path(first['report']).parent / '1.log').read_text().strip(), 'failure')

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

    def test_malformed_shipping_skips_preserve_structured_failure_and_log(self):
        host = {'linux': 'linux', 'win32': 'windows', 'darwin': 'macos'}[sys.platform]
        for field in ('game_check', 'game_ship', 'verify'):
            for invalid in (42, '', {}, [{'name': 'smoke'}]):
                with self.subTest(field=field, skipped=invalid):
                    if field == 'game_check':
                        payload = {'ok': True, 'ship': 'skipped: --skip-ship', 'skipped': invalid}
                        harness, arguments = 'game_check', ['--skip-ship']
                    else:
                        payload = {'ok': True, 'command': 'ship', 'package': {'ok': True},
                                   'verify': {'ok': True, 'platform': host, 'checks': [
                                       {'name': name, 'status': 'pass'} for name in
                                       ('identity', 'icon-files', 'icon-art', 'wiring',
                                        'package', 'exe-resources', 'smoke')]}}
                        (payload['verify'] if field == 'verify' else payload)['skipped'] = invalid
                        harness, arguments = 'game_ship', ['ship', '--no-install']
                    output = json.dumps(payload)
                    result, summary, report = self.run_check(
                        [sys.executable, '-c', 'print(' + repr(output) + ')', *arguments], harness)
                    self.assertEqual(result.returncode, 3, result.stderr)
                    self.assertNotIn('Traceback', result.stderr)
                    self.assertFalse(summary['ok'])
                    self.assertEqual(summary['failure']['category'], 'incomplete_shipping_evidence')
                    self.assertIn('unverified', summary['failure']['diagnostics'][0])
                    self.assertEqual(summary['failure']['returncode'], 0)
                    self.assertEqual(len(report['checks']), 1)
                    self.assertEqual(Path(summary['failure']['log']).read_text().strip(), output)

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

        # A full project includes empty docs/bin harnesses alongside real tests.
        result, summary, report = self.run_check(
            ['cargo', 'test', '--offline', '--message-format=json'], 'rust_project', later=False)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(report['checks'][0]['tests_executed'], 1)
        source.write_text('pub fn no_tests() {}\n')
        result, summary, _ = self.run_check(
            ['cargo', 'test', '--offline', '--message-format=json'], 'rust_project', later=False)
        self.assertEqual(result.returncode, 3)
        self.assertEqual(summary['failure']['category'], 'empty_test_selection')


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
