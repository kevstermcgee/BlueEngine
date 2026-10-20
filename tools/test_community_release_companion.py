"""Private companion native release checks; no hosted builds or publication."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

from tools import community_release_companion as companion


class CompanionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.engine = self.repository('engine')
        (self.engine / 'Cargo.toml').write_text('[package]\nname="be2"\nversion="0.1.0"\n')
        self.commit(self.engine, 'Engine source')
        self.git(self.engine, 'push', 'origin', 'main')
        self.engine_head = self.git(self.engine, 'rev-parse', 'HEAD')
        self.games = self.repository('games')
        # Frozen first-party controller contract from BlueEngineGames3aabaf6.
        # The adapter must load the trusted Git blob, never creator-edited code.
        (self.games / 'distribution').mkdir()
        fixture = Path(__file__).with_name('testdata') / 'companion-bind-engine-3a.txt'
        (self.games / 'distribution/bind_engine.py').write_bytes(fixture.read_bytes())
        self.game = self.games / 'games/legacy-game'
        (self.game / 'src').mkdir(parents=True)
        (self.game / 'Cargo.toml').write_bytes(b'[package]\r\nname="legacy-game"\r\nversion="0.1.0"\r\n[dependencies]\r\nvesper3d = { package="be2", path="../../../BlueEngine" }\r\n')
        (self.game / 'Cargo.lock').write_text('# frozen lock\n')
        (self.game / 'src/lib.rs').write_text('original legacy rules\n')
        self.definition = {'slug': 'legacy-game', 'name': 'Legacy Game', 'directory': 'games/legacy-game',
                           'kind': 'cargo', 'binary': 'legacy-game'}
        (self.games / '.release-games.json').write_text(json.dumps({'version': 1, 'native_playables': [self.definition]}))
        (self.games / '.games-catalog.json').write_text(json.dumps({'source_repository': companion.ENGINE_REPOSITORY,
            'target_repository': companion.GAMES_REPOSITORY, 'source_revision': self.engine_head,
            'files': [], 'preserved_paths': []}))
        (self.games / '.gitignore').write_text('**/dist/\n**/target/\n**/.blue-check/\n.be2-work/\n')
        (self.games / 'site/thumbs').mkdir(parents=True)
        (self.games / 'site/thumbs/legacy-game.png').write_bytes(b'committed native screenshot')
        self.commit(self.games, 'Original companion game')
        self.git(self.games, 'push', 'origin', 'main')
        # The pending authored revision stays off main until all release gates pass.
        (self.game / 'src/lib.rs').write_text('reviewed updated legacy rules\n')
        self.commit(self.games, 'Authored revision awaiting native release')
        self.source_head = self.git(self.games, 'rev-parse', 'HEAD')
        with patch.object(companion.forge, 'github_repository', side_effect=lambda root:
                          companion.ENGINE_REPOSITORY if Path(root) == self.engine else companion.GAMES_REPOSITORY):
            self.release = companion.create(self.engine, self.games, 'legacy-game', runs_root=self.root / 'runs')
        self.release.prepare()
        command = self.release.command
        self.sdk_commands = []
        self.metadata_packages = None
        def command_with_metadata(argv, **options):
            if options.get('label') != 'native-sdk-provenance':
                return command(argv, **options)
            self.sdk_commands.append(argv)
            log = self.release.directory / ('sdk-metadata-' + str(len(self.sdk_commands)) + '.log')
            packages = self.metadata_packages if self.metadata_packages is not None else [
                {'name': 'be2', 'source': None, 'manifest_path': str(self.release.engine / 'Cargo.toml')}]
            log.write_text(json.dumps({'packages': packages}) + '\n')
            return log
        self.release.command = command_with_metadata

    @staticmethod
    def git(root, *args):
        return subprocess.run(['git', '-C', str(root), *args], check=True,
                              capture_output=True, text=True).stdout.strip()

    def repository(self, name):
        remote, checkout = self.root / (name + '.git'), self.root / name
        self.git(self.root, 'init', '--bare', '--initial-branch=main', str(remote))
        self.git(self.root, 'clone', str(remote), str(checkout))
        self.configure(checkout)
        return checkout

    def configure(self, root):
        self.git(root, 'config', 'user.name', 'Fixture')
        self.git(root, 'config', 'user.email', 'fixture@example.invalid')

    def commit(self, root, message):
        self.git(root, 'add', '.')
        self.git(root, 'commit', '-qm', message)

    def competing_main(self, name='community-feedback.txt'):
        writer = self.root / 'other-author'
        self.git(self.root, 'clone', str(self.root / 'games.git'), str(writer))
        self.configure(writer)
        (writer / name).write_text('another author changed main\n')
        self.commit(writer, 'Concurrent update')
        self.git(writer, 'push', 'origin', 'main')
        return self.git(writer, 'rev-parse', 'HEAD')

    def test_legacy_game_without_project_metadata_prepares_from_frozen_source_and_pinned_engine(self):
        self.assertFalse((self.release.game / 'game.project.json').exists())
        self.assertEqual(self.release.state['engine_head'], self.engine_head)
        self.assertEqual(self.git(self.release.engine, 'rev-parse', 'HEAD'), self.engine_head)
        self.assertEqual(self.git(self.release.games, 'rev-parse', 'HEAD'), self.source_head)
        self.assertEqual(self.git(self.games, 'status', '--porcelain'), '')
        self.assertEqual(self.git(self.engine, 'status', '--porcelain'), '')
        self.assertEqual(self.release.state['source_kind'], 'companion')

    def test_new_unpreserved_companion_source_is_safe_without_engine_mutation(self):
        catalog = json.loads((self.release.games / '.games-catalog.json').read_text())
        self.assertEqual(catalog['preserved_paths'], [])
        self.assertTrue(companion.companion_owned(catalog, 'games/legacy-game'))
        self.assertEqual(self.release.state['ownership']['source_head'], self.source_head)
        self.assertEqual(self.git(self.engine, 'ls-remote', 'origin', 'refs/heads/main').split()[0], self.engine_head)
        commands = self.release.state['commands']
        self.assertFalse(any('ownership' in item['label'] or 'preserve' in item['label'] for item in commands))

    def test_owned_files_block_companion_lane_even_with_contradictory_preserve(self):
        catalog = json.loads((self.release.games / '.games-catalog.json').read_text())
        catalog['files'] = [{'path': 'games/legacy-game/src/lib.rs', 'sha256': 'a' * 64}]
        catalog['preserved_paths'] = ['games/legacy-game']
        self.assertFalse(companion.companion_owned(catalog, 'games/legacy-game'))
        catalog['files'][0]['path'] = 'games/Legacy-Game/src/lib.rs'
        self.assertFalse(companion.companion_owned(catalog, 'games/legacy-game'))
        catalog['files'][0]['path'] = 'games/other-game/src/lib.rs'
        self.assertTrue(companion.companion_owned(catalog, 'games/legacy-game'))

    def test_malformed_catalog_cannot_establish_unowned_source(self):
        catalog = json.loads((self.release.games / '.games-catalog.json').read_text())
        for invalid in (None, [], {}, dict(catalog, files=None), dict(catalog, source_revision=7),
                        dict(catalog, files=[{'path': 'games//legacy-game/source', 'sha256': 'a' * 64}]),
                        dict(catalog, files=[{'path': 'games/legacy-game/source', 'sha256': 'bad'}])):
            with self.subTest(invalid=invalid), self.assertRaises(companion.forge.ForgeError):
                companion.companion_owned(invalid, 'games/legacy-game')

    def test_fast_forward_main_incorporates_metadata_without_changing_frozen_game(self):
        self.git(self.games, 'push', 'origin', self.source_head + ':main')
        other = self.competing_main()
        frozen = self.git(self.release.games, 'rev-parse', self.source_head + ':games/legacy-game')
        self.release.integrate_main()
        self.assertEqual(self.release.state['games_head'], other)
        self.assertEqual(self.git(self.release.games, 'rev-parse', 'HEAD:games/legacy-game'), frozen)
        self.assertEqual(self.git(self.games, 'rev-parse', 'HEAD'), self.source_head)

    def test_ownership_race_after_native_review_blocks_public_source_push(self):
        writer = self.root / 'ownership-writer'
        self.git(self.root, 'clone', str(self.root / 'games.git'), str(writer))
        self.configure(writer)
        def claim_ownership():
            catalog = json.loads((writer / '.games-catalog.json').read_text())
            catalog['files'].append({'path': 'games/legacy-game/src/lib.rs', 'sha256': 'a' * 64})
            (writer / '.games-catalog.json').write_text(json.dumps(catalog))
            self.commit(writer, 'Export now owns selected game')
            self.git(writer, 'push', 'origin', 'main')
            self.release.state.setdefault('ci', {})['windows-installer-review'] = {'head': self.release.state['games_head']}
        self.release.verify_local = Mock(side_effect=lambda: self.release.state.update(local_receipt={
            key: self.release.state[key] for key in ('games_head', 'engine_head')}))
        self.release.native_checks = Mock(side_effect=lambda: self.release.state.setdefault('ci', {}).update(
            {'companion-native-checks': {'head': self.release.state['games_head']}}))
        self.release.installer_review = Mock(side_effect=claim_ownership)
        self.release.assert_review = Mock()
        with self.assertRaisesRegex(companion.forge.ForgeError, 'Engine-managed'):
            self.release.publish_games()
        public = self.git(self.games, 'ls-remote', 'origin', 'refs/heads/main').split()[0]
        self.assertEqual(public, self.git(writer, 'rev-parse', 'HEAD'))
        self.assertFalse(companion.forge.git_ancestor(self.release.games, self.source_head, public))
        self.assertEqual((self.game / 'src/lib.rs').read_text(), 'reviewed updated legacy rules\n')

    def test_temporary_binding_and_failed_build_restore_manifest_and_lock_exactly(self):
        original = {name: (self.release.game / name).read_bytes() for name in ('Cargo.toml', 'Cargo.lock')}
        with self.assertRaisesRegex(companion.forge.ForgeError, 'native failure'):
            with self.release.bound_engine():
                self.assertIn(self.release.engine.as_posix(), (self.release.game / 'Cargo.toml').read_text())
                (self.release.game / 'Cargo.lock').write_text('generated incompatible lock\n')
                raise companion.forge.ForgeError('native failure')
        self.assertEqual(original, {name: (self.release.game / name).read_bytes() for name in original})
        self.assertEqual((self.game / 'Cargo.toml').read_bytes(), original['Cargo.toml'])
        self.assertTrue(list(self.release.directory.glob('build-lock-*.toml')))
        self.release.assert_clean()

    def committed_manifest(self, dependency):
        manifest = self.release.game / 'Cargo.toml'
        manifest.write_text('[package]\nname="legacy-game"\nversion="0.1.0"\n[dependencies]\n' + dependency)
        self.commit(self.release.games, 'Authored SDK source')
        self.release.state['games_head'] = self.git(self.release.games, 'rev-parse', 'HEAD')
        return manifest

    def test_old_git_sdk_is_safely_bound_with_locked_proof_and_exact_source_restoration(self):
        url, revision = 'https://github.com/kevstermcgee/BlueEngine.git', '1441a433a76aa3c8d0f507dfe370a5c9583882b1'
        manifest = self.committed_manifest('vesper3d={package="be2",git="' + url + '",rev="' + revision + '",features=["presentation"]}\n')
        lock = self.release.game / 'Cargo.lock'
        lock.write_text('version=4\n[[package]]\nname="be2"\nversion="0.1.0"\nsource="git+' + url + '?rev=' + revision + '#' + revision + '"\n')
        self.commit(self.release.games, 'Committed original git lock')
        self.release.state['games_head'] = self.git(self.release.games, 'rev-parse', 'HEAD')
        original = {path: path.read_bytes() for path in (manifest, lock)}
        with self.release.bound_engine():
            import tomllib
            dependency = tomllib.loads(manifest.read_text())['dependencies']['vesper3d']
            self.assertEqual(dependency['path'], self.release.engine.as_posix())
            self.assertEqual(dependency['features'], ['presentation'])
            self.assertNotIn('source', tomllib.loads(lock.read_text())['package'][0])
        self.assertEqual(original, {path: path.read_bytes() for path in original})
        self.assertIn('--locked', self.sdk_commands[-1])
        proof = self.release.state['sdk_provenance']
        evidence = Path(proof['evidence'])
        self.assertEqual((evidence / 'original-Cargo.lock').read_bytes(), original[lock])
        self.assertNotEqual((evidence / 'bound-Cargo.lock').read_bytes(), original[lock])
        self.assertEqual(proof['tooling']['repository'], companion.GAMES_REPOSITORY)

    def test_missing_and_unrecognized_sdk_fail_before_metadata_and_restore_source(self):
        for declaration in ('serde="1"\n', 'vesper3d="0.1"\n', 'vesper3d={package="be2",version="0.1"}\n',
                            '[dependencies.vesper3d]\npackage="be2"\npath="old"\n',
                            'vesper3d={package="be2",path="old"}\n[target.\'cfg(windows)\'.dependencies]\n'
                            'vesper3d={package="be2",path="other"}\n'):
            with self.subTest(declaration=declaration):
                manifest = self.committed_manifest(declaration)
                original = manifest.read_bytes()
                before = len(self.sdk_commands)
                with self.assertRaisesRegex(companion.forge.ForgeError, 'supported inline'):
                    with self.release.bound_engine():
                        self.fail('Unrecognized SDK reached the native build')
                self.assertEqual(len(self.sdk_commands), before)
                self.assertEqual(manifest.read_bytes(), original)

    def test_wrong_git_duplicate_or_foreign_metadata_sdk_cannot_authorize_build(self):
        original = (self.release.game / 'Cargo.toml').read_bytes()
        variants = ([{'name': 'be2', 'source': 'git+old', 'manifest_path': str(self.release.engine / 'Cargo.toml')}],
                    [{'name': 'be2', 'source': None, 'manifest_path': str(self.engine / 'Cargo.toml')}],
                    [{'name': 'be2', 'source': None, 'manifest_path': str(self.release.engine / 'Cargo.toml')}] * 2,
                    [])
        for packages in variants:
            with self.subTest(packages=packages):
                self.metadata_packages = packages
                with self.assertRaisesRegex(companion.forge.ForgeError, 'Resolved SDK'):
                    with self.release.bound_engine():
                        self.fail('Unproven SDK reached the native build')
                self.assertEqual((self.release.game / 'Cargo.toml').read_bytes(), original)

    def test_partial_binder_write_failure_restores_both_inputs(self):
        manifest, lock = self.release.game / 'Cargo.toml', self.release.game / 'Cargo.lock'
        original = {path: path.read_bytes() for path in (manifest, lock)}
        def broken(manifest, engine):
            manifest.write_text('partially bound manifest')
            raise OSError('lock write failed')
        with patch.object(self.release, 'sdk_binder', return_value=Mock(bind=broken)):
            with self.assertRaisesRegex(OSError, 'lock write failed'):
                with self.release.bound_engine():
                    self.fail('Partial binder failure reached native build')
        self.assertEqual(original, {path: path.read_bytes() for path in original})

    def test_creator_edited_binding_controller_is_never_executed(self):
        (self.release.games / 'distribution/bind_engine.py').write_text('raise RuntimeError("untrusted creator controller executed")\n')
        self.commit(self.release.games, 'Untrusted release controller change')
        self.release.state['games_head'] = self.git(self.release.games, 'rev-parse', 'HEAD')
        with self.assertRaisesRegex(companion.forge.ForgeError, 'Integrate latest trusted Games main'):
            with self.release.bound_engine():
                self.fail('Untrusted controller reached native build')

    def test_private_binding_controller_tampering_is_not_executed(self):
        self.release.sdk_binder()
        path = self.release.directory / 'sdk-tooling' / self.release.state['sdk_tooling']['blob'] / 'bind_engine.py'
        path.write_text('raise RuntimeError("altered private tooling executed")\n')
        with self.assertRaisesRegex(companion.forge.ForgeError, 'private trusted SDK controller changed'):
            with self.release.bound_engine():
                self.fail('Changed private controller reached native build')

    def test_stale_source_and_stale_windows_receipts_cannot_publish(self):
        self.release.state['ci'] = {'windows-installer-review': {'head': 'wrong-source', 'run': 1}}
        self.release.api = Mock()
        with self.assertRaisesRegex(companion.forge.ForgeError, 'does not certify'):
            self.release.assert_review()
        self.release.api.assert_not_called()
        (self.release.game / 'src/lib.rs').write_text('unreviewed source edit')
        with self.assertRaisesRegex(companion.forge.ForgeError, 'tracked edits'):
            self.release.assert_review()
        self.assertEqual(self.git(self.games, 'rev-parse', 'HEAD'), self.source_head)

    def test_legacy_native_checks_use_pinned_engine_and_never_export_or_install(self):
        calls = []
        original = (self.release.game / 'Cargo.toml').read_bytes()
        def command(argv, *, cwd=None, label):
            calls.append((argv, label, cwd))
            if label == 'native-build':
                self.assertIn(self.release.engine.as_posix(), (self.release.game / 'Cargo.toml').read_text())
            log = self.release.directory / (label + '.log')
            log.write_text(json.dumps({'packages': [{'name': 'be2', 'source': None, 'manifest_path': str(self.release.engine / 'Cargo.toml')}]})
                           if label == 'native-sdk-provenance' else 'test result: ok. 2 passed; 0 failed\n')
            return log
        with patch.object(self.release, 'command', side_effect=command):
            self.release.verify_local()
        labels = [label for _, label, _ in calls]
        self.assertEqual(labels, ['catalog-check', 'distribution-tests', 'site-tests', 'native-sdk-provenance', 'game-format', 'game-headless-tests', 'game-clippy', 'native-build'])
        self.assertIn('--no-default-features', calls[5][0])
        self.assertEqual((self.release.game / 'Cargo.toml').read_bytes(), original)
        self.assertEqual(self.release.state['local_receipt']['games_head'], self.source_head)
        self.assertEqual(self.git(self.games, 'status', '--porcelain'), '')
        self.assertFalse(any('publish_games.py' in str(argv) for argv, _, _ in calls))

    def test_zero_behavioral_tests_block_native_build_and_restore_source(self):
        calls = []
        original = (self.release.game / 'Cargo.toml').read_bytes()
        def command(argv, *, cwd=None, label):
            calls.append(label)
            log = self.release.directory / (label + '.log')
            log.write_text(json.dumps({'packages': [{'name': 'be2', 'source': None, 'manifest_path': str(self.release.engine / 'Cargo.toml')}]})
                           if label == 'native-sdk-provenance' else 'test result: ok. 0 passed; 0 failed\n')
            return log
        with patch.object(self.release, 'command', side_effect=command), self.assertRaisesRegex(companion.forge.ForgeError, 'executed behavioral tests'):
            self.release.verify_local()
        self.assertNotIn('native-build', calls)
        self.assertEqual((self.release.game / 'Cargo.toml').read_bytes(), original)
        self.assertNotIn('local_receipt', self.release.state)

    def test_ci_requires_exact_sha_branch_event_and_workflow(self):
        valid = {'id': 7, 'head_sha': self.source_head, 'head_branch': 'main', 'event': 'push',
                 'path': '.github/workflows/releases.yml', 'status': 'completed', 'conclusion': 'success',
                 'html_url': 'https://example.invalid/run/7', 'updated_at': '2026-10-20T00:00:00Z'}
        impostors = [dict(valid, id=99, event='workflow_dispatch'), dict(valid, id=98, head_sha='a' * 40),
                     dict(valid, id=97, head_branch='other'), dict(valid, id=96, path='.github/workflows/other.yml')]
        self.release.api = Mock(return_value={'workflow_runs': impostors + [valid]})
        receipt = self.release.wait_ci(companion.GAMES_REPOSITORY, 'releases.yml', 'main', self.source_head, 'production-release')
        self.assertEqual(receipt['run'], 7)

    def test_diverged_main_rechecks_merge_before_push_without_engine_or_input_source_changes(self):
        other = self.competing_main()
        checks = []
        def local():
            checks.append(('local', self.release.state['games_head']))
            self.release.state['local_receipt'] = {key: self.release.state[key] for key in ('games_head', 'engine_head')}
        def installer():
            checks.append(('windows', self.release.state['games_head']))
            self.release.state.setdefault('ci', {})['windows-installer-review'] = {'head': self.release.state['games_head']}
        def native():
            checks.append(('native', self.release.state['games_head']))
            self.release.state.setdefault('ci', {})['companion-native-checks'] = {'head': self.release.state['games_head']}
        self.release.verify_local = Mock(side_effect=local)
        self.release.native_checks = Mock(side_effect=native)
        self.release.installer_review = Mock(side_effect=installer)
        self.release.assert_review = Mock()
        self.release.publish_games()
        head = self.git(self.games, 'ls-remote', 'origin', 'refs/heads/main').split()[0]
        self.assertNotEqual(head, self.source_head)
        self.assertTrue(companion.forge.git_ancestor(self.release.games, other, head))
        self.assertTrue(companion.forge.git_ancestor(self.release.games, self.source_head, head))
        self.assertEqual(checks, [('local', head), ('native', head), ('windows', head)])
        self.assertEqual(self.git(self.games, 'rev-parse', 'HEAD'), self.source_head)
        self.assertEqual(self.git(self.engine, 'rev-parse', 'HEAD'), self.engine_head)
        self.assertEqual(self.git(self.games, 'status', '--porcelain'), '')
        self.assertEqual(self.git(self.engine, 'status', '--porcelain'), '')

    def test_failed_merge_installer_gate_retains_work_and_retries_that_gate(self):
        other = self.competing_main()
        def local():
            self.release.state['local_receipt'] = {key: self.release.state[key] for key in ('games_head', 'engine_head')}
        self.release.verify_local = Mock(side_effect=local)
        self.release.native_checks = Mock(side_effect=lambda: self.release.state.setdefault('ci', {}).update(
            {'companion-native-checks': {'head': self.release.state['games_head']}}))
        self.release.installer_review = Mock(side_effect=companion.forge.ForgeError('installer failed'))
        self.release.assert_review = Mock()
        with self.assertRaisesRegex(companion.forge.ForgeError, 'installer failed'):
            self.release.publish_games()
        self.assertEqual(self.git(self.games, 'ls-remote', 'origin', 'refs/heads/main').split()[0], other)
        self.release.installer_review.side_effect = lambda: self.release.state.setdefault('ci', {}).update(
            {'windows-installer-review': {'head': self.release.state['games_head']}})
        self.release.publish_games()
        self.assertEqual(self.release.verify_local.call_count, 1)
        self.assertEqual(self.release.installer_review.call_count, 2)

    def test_concurrent_game_changes_preserve_both_sources_without_privileged_build_or_push(self):
        other = self.competing_main('games/legacy-game/new-rule.rs')
        self.release.verify_local = Mock()
        with self.assertRaisesRegex(companion.forge.ForgeError, 'exact tree'):
            self.release.publish_games()
        self.release.verify_local.assert_not_called()
        self.assertEqual(self.git(self.games, 'ls-remote', 'origin', 'refs/heads/main').split()[0], other)
        self.assertTrue((self.release.game / 'new-rule.rs').is_file())
        self.assertEqual((self.game / 'src/lib.rs').read_text(), 'reviewed updated legacy rules\n')

    def test_failed_phase_is_not_marked_complete_or_skipped_on_resume(self):
        self.release.state['completed'] = []
        operations = ('prepare', 'verify_local', 'native_checks', 'installer_review', 'publish_games', 'verify_publication')
        for name in operations:
            setattr(self.release, name, Mock())
        self.release.installer_review.side_effect = companion.forge.ForgeError('installer failed')
        with self.assertRaises(companion.forge.ForgeError):
            self.release.advance()
        self.assertEqual(self.release.state['completed'], ['prepare', 'local', 'native'])
        self.release.publish_games.assert_not_called()
        self.release.installer_review.side_effect = None
        self.release.state['publication'] = {'release': 'verified release'}
        self.release.advance()
        self.assertEqual(self.release.verify_local.call_count, 1)
        self.assertEqual(self.release.installer_review.call_count, 2)
        self.assertEqual(self.release.publish_games.call_count, 1)


if __name__ == '__main__':
    unittest.main()
