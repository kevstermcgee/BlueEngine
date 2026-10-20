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
        self.root = Path(self.temp.name)
        self.engine = self.repository('engine')
        (self.engine / 'Cargo.toml').write_text('[package]\nname="be2"\nversion="0.1.0"\n')
        self.commit(self.engine, 'Engine source')
        self.git(self.engine, 'push', 'origin', 'main')
        self.engine_head = self.git(self.engine, 'rev-parse', 'HEAD')
        self.games = self.repository('games')
        self.game = self.games / 'games/legacy-game'
        (self.game / 'src').mkdir(parents=True)
        (self.game / 'Cargo.toml').write_bytes(b'[package]\r\nname="legacy-game"\r\nversion="0.1.0"\r\n[dependencies]\r\nvesper3d = { package="be2", path="../../../BlueEngine" }\r\n')
        (self.game / 'Cargo.lock').write_text('# frozen lock\n')
        (self.game / 'src/lib.rs').write_text('original legacy rules\n')
        self.definition = {'slug': 'legacy-game', 'name': 'Legacy Game', 'directory': 'games/legacy-game',
                           'kind': 'cargo', 'binary': 'legacy-game'}
        (self.games / '.release-games.json').write_text(json.dumps({'version': 1, 'native_playables': [self.definition]}))
        (self.games / '.games-catalog.json').write_text(json.dumps({'source_repository': companion.ENGINE_REPOSITORY,
            'target_repository': companion.GAMES_REPOSITORY, 'source_revision': self.engine_head}))
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
            log.write_text('test result: ok. 2 passed; 0 failed\n')
            return log
        with patch.object(self.release, 'command', side_effect=command):
            self.release.verify_local()
        labels = [label for _, label, _ in calls]
        self.assertEqual(labels, ['catalog-check', 'distribution-tests', 'site-tests', 'game-format', 'game-headless-tests', 'game-clippy', 'native-build'])
        self.assertIn('--no-default-features', calls[4][0])
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
            log.write_text('test result: ok. 0 passed; 0 failed\n')
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
