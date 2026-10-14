"""Manifest/staging/freshness tests; fake PE/compiler fixtures are not Windows evidence."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import types
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]


def module(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'templates' / (name + '.py'))
    value = importlib.util.module_from_spec(spec)
    sys.modules[name] = value
    spec.loader.exec_module(value)
    return value


ship = module('game_ship')
installer = module('game_installer')


def pe(machine=0x8664, magic=0x20b, dll=False):
    data = bytearray(64 + 24 + 240)
    data[:2] = b'MZ'
    struct.pack_into('<I', data, 60, 64)
    data[64:68] = b'PE\0\0'
    struct.pack_into('<H', data, 68, machine)
    struct.pack_into('<H', data, 84, 240)
    struct.pack_into('<H', data, 86, 0x2002 if dll else 0x0002)
    struct.pack_into('<H', data, 88, magic)
    return bytes(data)


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='installer test 星河 ')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'scripts').mkdir()
        (self.root / 'assets').mkdir()
        (self.root / 'Cargo.toml').write_text('[package]\nname = "distinct-game"\nversion = "0.1.0"\n', encoding='utf-8')
        self.identity = {'title': '星河; {#literal} #1', 'tagline': 'An original route',
                         'controls': 'WASD', 'exe': 'distinct-game', 'package': []}
        self.files = {'distinct-game.exe': pe(), 'distinct-game.ico': b'fixture ico',
                      'distinct-game.png': b'fixture png', 'assets/星河; {#literal}/score.txt': b'original music'}
        self.dist = self.root / 'dist'
        self.dist.mkdir()
        for name, data in self.files.items():
            path = self.dist / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        self.refresh()

    def refresh(self):
        (self.root / 'assets/identity.json').write_text(json.dumps(self.identity, ensure_ascii=False), encoding='utf-8')
        self.project = ship.Project(self.root, 'windows')
        self.project.write_stamp({'title': self.identity['title'], 'exe': 'distinct-game.exe',
                                 'files': sorted(self.files),
                                 'file_sha256': {n: ship.sha256_file(self.dist / n) for n in self.files},
                                 'exe_sha256': ship.sha256_file(self.dist / 'distinct-game.exe')})

    def config(self, value):
        (self.root / 'scripts/installer.json').write_text(json.dumps(value), encoding='utf-8')

    def test_exact_manifest_excludes_undeclared_player_files_and_preserves_custom_script(self):
        saves = self.dist / 'saves'
        saves.mkdir()
        (saves / 'my-slot.json').write_text('player data')
        (self.dist / 'settings.json').write_text('{}')
        authored = self.root / 'scripts/installer.iss'
        authored.write_text('custom Inno path')
        result = installer.prepare(self.project, ship)
        stage = Path(result['stage'])
        self.assertEqual(sorted(p.relative_to(stage / 'payload').as_posix() for p in (stage / 'payload').rglob('*') if p.is_file()), sorted(self.files))
        text = Path(result['script']).read_text(encoding='utf-8-sig')
        self.assertNotIn('recursesubdirs', text)
        self.assertNotIn('*', text)
        self.assertNotIn('[UninstallDelete]', text)
        self.assertNotIn('[InstallDelete]', text)
        self.assertEqual(authored.read_text(), 'custom Inno path')
        self.assertTrue(result['unverified'])
        self.assertIsNone(result['package_shipping_evidence'])

    def test_prepared_outputs_do_not_enter_whole_project_source_exports(self):
        from scripts import publish_games
        authored = self.root / 'scripts/installer.iss'
        authored.write_text('authored custom Inno configuration')
        self.config({'app_id': 'Stable.AuthoredId'})
        legacy = self.root / 'installers/authored-options.iss'
        legacy.parent.mkdir()
        legacy.write_text('preserved legacy delivery source')
        result = installer.prepare(self.project, ship)
        stage = Path(result['stage'])
        (stage / 'output').mkdir()
        (stage / 'output/distinct-game-setup-windows-x64.exe').write_bytes(pe(0x14c, 0x10b))
        self.assertTrue(stage.is_relative_to(self.root / '.blue-check/installers'))
        observed = {relative.as_posix() for _, relative in publish_games.files_under(self.root)}
        self.assertIn('scripts/installer.iss', observed)
        self.assertIn('scripts/installer.json', observed)
        self.assertIn('installers/authored-options.iss', observed)
        self.assertFalse(any(name.startswith('.blue-check/') for name in observed))
        manifest = {'source_repository': 'fixture', 'target_repository': 'fixture',
                    'collections': {category: [] for category in publish_games.CATEGORIES},
                    'playables': [{'entry': 'games/native/Cargo.toml', 'files': ['games/native/scripts/installer.iss']}]}
        manifest['collections']['games'] = [{'source': self.root.name, 'destination': 'native'}]
        with tempfile.TemporaryDirectory(prefix='installer-export-fixture-') as temp:
            catalog = publish_games.export_tree(self.root.parent, Path(temp), manifest, 'fixture-only-no-publication')
            exported = {entry['path'] for entry in catalog['files']}
            self.assertIn('games/native/scripts/installer.iss', exported)
            self.assertIn('games/native/scripts/installer.json', exported)
            self.assertIn('games/native/installers/authored-options.iss', exported)
            self.assertFalse(any('/.blue-check/' in name for name in exported))
            self.assertFalse(any('/prepared-' in name or name.endswith('.exe') for name in exported))

    def test_inno_compile_paths_and_runtime_literals_are_distinct(self):
        self.config({'publisher': 'Studio "星"; {#danger} #literal'})
        result = installer.prepare(self.project, ship)
        self.assertTrue(Path(result['script']).read_bytes().startswith(b'\xef\xbb\xbf'))
        text = Path(result['script']).read_text(encoding='utf-8-sig')
        self.assertTrue(text.startswith('#preproc builtin\n'))
        self.assertIn('Source: "payload\\assets\\星河; {#literal}\\score.txt"', text)
        self.assertIn('DestDir: "{app}\\assets\\星河; {{#literal}"', text)
        self.assertIn('AppName="星河; {{#literal} #1"', text)
        self.assertIn('VersionInfoProductName="星河; {#literal} #1"', text)
        self.assertIn('AppPublisher="Studio "星"; {{#danger} #literal"', text)
        self.assertEqual(installer.literal('"quoted"; {#literal}'), '"""quoted""; {{#literal}"')
        self.assertIn('PrivilegesRequired=lowest', text)
        self.assertIn('ArchitecturesAllowed=x64compatible', text)
        self.assertIn('UsePreviousAppDir=yes', text)

    def test_appid_survives_title_version_changes_and_explicit_override_survives_package_rename(self):
        original = installer.prepare(self.project, ship)['config']['app_id']
        self.identity['title'] = 'A new authored title'
        (self.root / 'Cargo.toml').write_text('[package]\nname="distinct-game"\nversion="0.2.0"\n')
        self.refresh()
        self.assertEqual(installer.prepare(self.project, ship)['config']['app_id'], original)
        self.config({'app_id': original})
        (self.root / 'Cargo.toml').write_text('[package]\nname="renamed-game"\nversion="0.3.0"\n')
        self.refresh()
        self.assertEqual(installer.prepare(self.project, ship)['config']['app_id'], original)

    def test_newline_control_and_unknown_settings_fail_before_outputs(self):
        for config in ({'publisher': 'bad\n[Files]\nSource: evil'}, {'publisher': 'bad\x00'}, {'publisher': 'bad\u2028[Files]'}, {'publisher': 'bad\x85[Files]'}, {'app_id': '{code:evil}'}, {'app_id': 'x' * 128}, {'publsher': 'typo'}):
            with self.subTest(config=config):
                self.config(config)
                with self.assertRaises(ship.ConfigError):
                    installer.prepare(self.project, ship)
                self.assertFalse((self.root / '.blue-check/installers').exists())

    def test_declared_player_files_fail_without_affecting_existing_outputs(self):
        prepared = installer.prepare(self.project, ship)
        original = Path(prepared['script']).read_bytes()
        self.files['settings.json'] = b'player settings'
        (self.dist / 'settings.json').write_bytes(self.files['settings.json'])
        self.refresh()
        with self.assertRaisesRegex(ship.ConfigError, 'player saves/settings/logs'):
            installer.prepare(self.project, ship)
        self.assertEqual(Path(prepared['script']).read_bytes(), original)

    def test_non_windows_wrong_arch_dll_and_damaged_package_fail(self):
        for data in (b'Linux executable', pe(0x14c, 0x10b), pe(dll=True)):
            with self.subTest(data=data[:4]):
                (self.dist / 'distinct-game.exe').write_bytes(data)
                self.refresh()
                with self.assertRaises(ship.ShipError):
                    installer.prepare(self.project, ship)
        (self.dist / 'distinct-game.exe').write_bytes(pe())
        self.refresh()
        (self.dist / 'distinct-game.png').write_bytes(b'changed')
        with self.assertRaisesRegex(ship.ShipError, 'modified or damaged'):
            installer.prepare(self.project, ship)

    def test_prepare_and_cli_work_outside_game_cwd_without_processes(self):
        previous = Path.cwd()
        try:
            os.chdir(ROOT)
            with mock.patch.object(ship, 'run_process', side_effect=AssertionError('no compiler/build/install permitted')), contextlib.redirect_stdout(io.StringIO()) as output:
                code = ship.main(['installer', 'prepare'], root=self.root)
            self.assertEqual(code, 0)
            self.assertEqual(json.loads(output.getvalue())['command'], 'installer prepare')
        finally:
            os.chdir(previous)

    def compiler(self):
        path = self.root / 'Inno Setup 星河' / 'ISCC.exe'
        path.parent.mkdir()
        path.write_bytes(pe(0x14c, 0x10b))  # Synthetic PE; subprocess execution remains mocked.
        return path

    def test_batch_or_script_compiler_overrides_fail_before_staging_or_processes(self):
        prior = installer.prepare(self.project, ship)
        prior_script = Path(prior['script']).read_bytes()
        for name, content in (('ISCC.cmd', b'@echo dangerous wrapper'), ('ISCC.bat', pe(0x14c, 0x10b)), ('ISCC.exe', b'@echo renamed wrapper')):
            compiler = self.root / name
            compiler.write_bytes(content)
            with self.subTest(name=name), mock.patch.object(ship, 'host_platform', return_value='windows'), mock.patch.object(ship, 'run_process', side_effect=AssertionError('must not invoke a shell/process')):
                with self.assertRaisesRegex(ship.ShipError, 'native Windows'):
                    installer.build(self.project, ship, compiler)
            self.assertEqual(Path(prior['script']).read_bytes(), prior_script)
            self.assertEqual(len(list((self.root / '.blue-check/installers').glob('prepared-*'))), 1)

    def compiled(self, command, **options):
        self.assertIsInstance(command, list)
        self.assertNotIn('shell', options)
        self.assertEqual(command[0], str(self.iscc))
        stage = Path(command[-1]).parent
        self.assertEqual(Path(options['cwd']), stage)
        (stage / 'output/distinct-game-setup-windows-x64.exe').write_bytes(pe(0x14c, 0x10b))
        return subprocess.CompletedProcess(command, 0, 'compiled fixture', '')

    def test_build_uses_explicit_compiler_argument_array_and_fresh_output(self):
        self.iscc = self.compiler()
        with mock.patch.object(ship, 'host_platform', return_value='windows'), mock.patch.object(ship, 'run_process', side_effect=self.compiled):
            result = installer.build(self.project, ship, self.iscc)
        self.assertEqual(result['compiler_sha256'], ship.sha256_file(self.iscc))
        self.assertEqual(result['installer_sha256'], ship.sha256_file(Path(result['installer'])))
        self.assertTrue((Path(result['stage']) / 'built.json').is_file())
        self.assertTrue(result['unverified'])

    def test_compiler_failure_and_missing_output_preserve_prior_generation(self):
        self.iscc = self.compiler()
        prior = installer.prepare(self.project, ship)
        prior_bytes = Path(prior['script']).read_bytes()
        for code in (1, 0):
            with self.subTest(code=code), mock.patch.object(ship, 'host_platform', return_value='windows'), mock.patch.object(ship, 'run_process', return_value=subprocess.CompletedProcess([], code, 'log', 'diagnostic')):
                with self.assertRaisesRegex(ship.ShipError, 'preserved'):
                    installer.build(self.project, ship, self.iscc)
        self.assertEqual(Path(prior['script']).read_bytes(), prior_bytes)
        self.assertEqual(len(list((self.root / '.blue-check/installers').glob('prepared-*'))), 3)

    def test_package_change_while_compiling_rejects_success(self):
        self.iscc = self.compiler()
        def changed(command, **options):
            done = self.compiled(command, **options)
            (self.dist / 'distinct-game.exe').write_bytes(pe() + b'changed')
            return done
        with mock.patch.object(ship, 'host_platform', return_value='windows'), mock.patch.object(ship, 'run_process', side_effect=changed):
            with self.assertRaisesRegex(ship.ShipError, 'modified or damaged'):
                installer.build(self.project, ship, self.iscc)
        self.assertFalse(list((self.root / '.blue-check/installers').glob('*/built.json')))

    def test_prepared_script_payload_and_config_freshness_are_checked(self):
        for kind in ('script', 'payload', 'config'):
            with self.subTest(kind=kind):
                result = installer.prepare(self.project, ship)
                stage = Path(result['stage'])
                if kind == 'script':
                    (stage / 'installer.iss').write_text('changed')
                elif kind == 'payload':
                    (stage / 'payload/distinct-game.exe').write_bytes(b'changed')
                else:
                    self.config({'publisher': 'changed studio'})
                with self.assertRaises(ship.ShipError):
                    installer._validate_stage(self.project, ship, stage)

    def built(self):
        self.iscc = self.compiler()
        with mock.patch.object(ship, 'host_platform', return_value='windows'), mock.patch.object(ship, 'run_process', side_effect=self.compiled):
            return installer.build(self.project, ship, self.iscc)

    def registry(self, registered=False):
        return types.SimpleNamespace(HKEY_CURRENT_USER='HKCU', HKEY_LOCAL_MACHINE='HKLM',
                                     KEY_READ=0x20019, KEY_WOW64_32KEY=0x200, KEY_WOW64_64KEY=0x100,
                                     OpenKey=mock.Mock(return_value=mock.MagicMock()) if registered else mock.Mock(side_effect=FileNotFoundError))

    def test_lifecycle_refuses_all_hives_and_registry_views_without_processes(self):
        result = self.built()
        for hive in ('HKCU', 'HKLM'):
            for view in (0x100, 0x200):
                registry = self.registry()
                def opened(actual_hive, path, reserved, access):
                    if actual_hive == hive and access == (registry.KEY_READ | view):
                        return mock.MagicMock()
                    raise FileNotFoundError
                registry.OpenKey.side_effect = opened
                with self.subTest(hive=hive, view=view), mock.patch.dict(sys.modules, {'winreg': registry}), mock.patch.object(ship, 'host_platform', return_value='windows'), mock.patch.object(ship, 'run_process', side_effect=AssertionError('must not install')):
                    with self.assertRaisesRegex(ship.ShipError, 'already registered'):
                        installer.test_install(self.project, ship, result['stage'])

    def test_timeout_logs_keep_partial_bytes_and_prior_generation(self):
        self.iscc = self.compiler()
        prior = installer.prepare(self.project, ship)
        prior_script = Path(prior['script']).read_bytes()
        timeout = subprocess.TimeoutExpired([str(self.iscc)], 600, output=b'partial output\xff', stderr=b'partial diagnostic')
        with mock.patch.object(ship, 'host_platform', return_value='windows'), mock.patch.object(ship, 'run_process', side_effect=timeout):
            with self.assertRaisesRegex(ship.ShipError, 'timed out after 600 seconds'):
                installer.build(self.project, ship, self.iscc)
        logs = list((self.root / '.blue-check/installers').glob('*/compiler.stdout'))
        self.assertEqual(len(logs), 1)
        self.assertEqual(logs[0].read_text(encoding='utf-8'), 'partial output\ufffd')
        self.assertEqual(logs[0].with_name('compiler.stderr').read_text(), 'partial diagnostic')
        self.assertEqual(Path(prior['script']).read_bytes(), prior_script)

    @unittest.skipUnless(hasattr(os, 'symlink'), 'requires symlink support')
    def test_stage_report_and_output_symlinks_are_rejected_before_install(self):
        result = self.built()
        stage = Path(result['stage'])
        alias = self.root / '.blue-check/installers/stage-alias'
        try:
            alias.symlink_to(stage, target_is_directory=True)
        except OSError as error:
            self.skipTest(str(error))
        with self.assertRaisesRegex(ship.ShipError, 'symlinks'):
            installer._validate_stage(self.project, ship, alias)
        parent_alias = self.root / '.blue-check/installers/parent-alias'
        parent_alias.symlink_to(stage.parent, target_is_directory=True)
        with self.assertRaisesRegex(ship.ShipError, 'symlinks'):
            installer._validate_stage(self.project, ship, parent_alias / stage.name)
        for name in ('prepared.json', 'built.json', 'output'):
            path = stage / name
            saved = stage / (name + '.saved')
            path.rename(saved)
            path.symlink_to(saved, target_is_directory=saved.is_dir())
            with self.subTest(name=name), mock.patch.object(ship, 'host_platform', return_value='windows'), mock.patch.object(ship, 'run_process', side_effect=AssertionError('must not install')):
                with self.assertRaisesRegex(ship.ShipError, 'symlinks'):
                    installer.test_install(self.project, ship, stage)
            path.unlink()
            saved.rename(path)

    def test_lifecycle_refuses_registered_app_or_existing_shortcut_before_installing(self):
        result = self.built()
        stage = Path(result['stage'])
        programs = self.root / 'Programs'
        programs.mkdir()
        for registered in (True, False):
            if not registered:
                (programs / (self.identity['title'] + '.lnk')).write_text('existing shortcut')
            with self.subTest(registered=registered), mock.patch.dict(sys.modules, {'winreg': self.registry(registered)}), mock.patch.object(ship, 'host_platform', return_value='windows'), mock.patch.object(ship, '_known_folder', return_value=str(programs)), mock.patch.object(ship, 'run_process', side_effect=AssertionError('must not install')):
                with self.assertRaisesRegex(ship.ShipError, 'isolated Windows account'):
                    installer.test_install(self.project, ship, stage)

    def test_lifecycle_verifies_exact_bytes_shortcut_and_player_retention(self):
        result = self.built()
        stage = Path(result['stage'])
        programs = self.root / 'Programs'
        programs.mkdir()
        shortcut = programs / (self.identity['title'] + '.lnk')
        installation = self.root / 'installation check 星河'
        installation.mkdir()
        calls = []
        def process(command, **options):
            calls.append(command)
            if '/DIR=' + str(installation) in command:
                for name in result['files']:
                    path = installation / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes((stage / 'payload' / name).read_bytes())
                shortcut.write_text('shortcut fixture')
                (installation / 'unins000.exe').write_bytes(pe(0x14c, 0x10b))
            else:
                self.assertEqual(command[0], str(installation / 'unins000.exe'))
                for name in result['files']:
                    (installation / name).unlink()
                shortcut.unlink()
            return subprocess.CompletedProcess(command, 0, 'lifecycle fixture', '')
        with mock.patch.dict(sys.modules, {'winreg': self.registry()}), mock.patch.object(ship, 'host_platform', return_value='windows'), mock.patch.object(ship, '_known_folder', return_value=str(programs)), mock.patch.object(installer.tempfile, 'mkdtemp', return_value=str(installation)), mock.patch.object(ship, 'run_process', side_effect=process), mock.patch.object(ship, 'read_shortcut', return_value={'target': str(installation / 'distinct-game.exe'), 'working_dir': str(installation)}):
            tested = installer.test_install(self.project, ship, stage)
        self.assertTrue(tested['ok'])
        self.assertEqual(len(calls), 3)
        self.assertEqual((installation / 'saves/installer-retention.txt').read_text(), 'preserve player state')
        self.assertEqual((installation / 'settings.json').read_text(), '{"retain":true}')
        self.assertFalse((installation / 'distinct-game.exe').exists())
        self.assertEqual(len(list(stage.glob('lifecycle-*.stdout'))), 3)

    def test_lifecycle_rejects_output_from_another_package_even_with_same_script(self):
        result = self.built()
        stage = Path(result['stage'])
        built = json.loads((stage / 'built.json').read_text())
        built['package_binding']['ship_manifest_sha256'] = 'f' * 64
        (stage / 'built.json').write_text(json.dumps(built))
        with mock.patch.object(ship, 'host_platform', return_value='windows'), mock.patch.object(ship, 'run_process', side_effect=AssertionError('must not install')):
            with self.assertRaisesRegex(ship.ShipError, 'build binding'):
                installer.test_install(self.project, ship, stage)

    def test_build_and_install_test_require_windows_without_invoking_any_process(self):
        with mock.patch.object(ship, 'host_platform', return_value='linux'), mock.patch.object(ship, 'run_process', side_effect=AssertionError('no process')):
            with self.assertRaisesRegex(ship.ShipError, 'requires Windows'):
                installer.build(self.project, ship, 'ISCC.exe')
            with self.assertRaisesRegex(ship.ShipError, 'requires an isolated Windows'):
                installer.test_install(self.project, ship, self.root / '.blue-check/installers')


if __name__ == '__main__':
    unittest.main()
