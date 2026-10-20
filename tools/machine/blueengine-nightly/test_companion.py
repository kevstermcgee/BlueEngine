"""Second-worktree scope, publication ordering and signed feed checks in scratch Git."""
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

import companion


def git(root, *args, **unused):
    return subprocess.run(['git', '-C', str(root), *map(str, args)], check=True,
                          capture_output=True, text=True).stdout.strip()


class CompanionMaintenanceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source, self.remote, self.run = [self.root / name for name in ('source', 'remote.git', 'run')]
        self.run.mkdir()
        git(self.root, 'init', '--bare', '--initial-branch=main', str(self.remote))
        git(self.root, 'clone', str(self.remote), str(self.source))
        git(self.source, 'config', 'user.name', 'Fixture')
        git(self.source, 'config', 'user.email', 'fixture@example.invalid')
        self.versions(self.source, '0.1.0')
        (self.source / 'site').mkdir()
        (self.source / 'site/view.mjs').write_text('original UI\n')
        self.definitions = {'native_playables': [{'slug': slug, 'directory': 'games/' + slug, 'kind': 'cargo', 'binary': slug}
                                                for slug in ('owned', 'other', 'engine-game')]}
        for slug in ('owned', 'other', 'engine-game'):
            directory = self.source / 'games' / slug
            directory.mkdir(parents=True)
            (directory / 'Cargo.toml').write_text('[package]\nname="' + slug + '"\nversion="0.1.0"\n')
        self.catalog = {'source_repository': 'fixture/engine', 'target_repository': 'fixture/games',
                        'files': [{'path': 'games/engine-game/Cargo.toml', 'sha256': 'a' * 64}],
                        'preserved_paths': ['games/owned', 'games/other']}
        git(self.source, 'add', '.')
        git(self.source, 'commit', '-qm', 'Baseline')
        git(self.source, 'push', 'origin', 'main')
        self.base = git(self.source, 'rev-parse', 'HEAD')
        self.catalog['source_revision'] = self.base
        # The actual user checkout must remain untouched, including unfinished edits.
        (self.source / 'site/view.mjs').write_text('valuable unfinished user work\n')
        self.work = self.run / 'worktree'
        git(self.source, 'worktree', 'add', '--detach', self.work, self.base)
        self.config = {'games': str(self.source), 'games_repo': 'fixture/games', 'engine_repo': 'fixture/engine',
                       'check_timeout_seconds': 10, 'ci_timeout_seconds': 1, 'ci_poll_seconds': 1}
        self.log = io.StringIO()
        self.maintenance = companion.Maintenance(self.config, self.run, self.work, self.base, self.catalog, self.definitions,
            command=self.command, git=git, write_json=lambda path, value: path.write_text(json.dumps(value)), log=self.log)
        self.maintenance.state['game_baselines'] = {slug: {'expected_revision': 'before-' + slug} for slug in ('owned', 'other')}

    @staticmethod
    def versions(root, version):
        for name, value in (('launcher/package.json', {'version': version}),
                            ('launcher/package-lock.json', {'version': version, 'packages': {'': {'version': version}}}),
                            ('launcher/src-tauri/tauri.conf.json', {'version': version, 'plugins': {'updater': {'pubkey': 'public-key', 'endpoints': ['https://example.invalid/update']}}})):
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(value))
        (root / 'launcher/src-tauri/Cargo.toml').write_text('[package]\nname="blueengine-games-launcher"\nversion="' + version + '"\n')
        (root / 'launcher/src-tauri/Cargo.lock').write_text('[[package]]\nname="blueengine-games-launcher"\nversion="' + version + '"\n')

    @staticmethod
    def command(argv, *, cwd=None, timeout=10, log=None):
        return subprocess.run(list(map(str, argv)), cwd=cwd, timeout=timeout, check=True,
                              capture_output=True, text=True).stdout

    def test_companion_scope_keeps_exported_engine_games_in_engine_and_blocks_automation(self):
        self.assertEqual(self.maintenance.games, {'owned', 'other'})
        (self.work / 'games/owned/Cargo.toml').write_text('permitted companion edit')
        self.assertEqual(self.maintenance.changes(), ['games/owned/Cargo.toml'])
        (self.work / 'games/engine-game/Cargo.toml').write_text('wrong owner')
        with self.assertRaisesRegex(RuntimeError, 'escaped'):
            self.maintenance.changes()
        git(self.work, 'restore', 'games/engine-game/Cargo.toml')
        path = self.work / '.github/workflows/launcher.yml'
        path.parent.mkdir(parents=True)
        path.write_text('unreviewed workflow')
        with self.assertRaisesRegex(RuntimeError, 'escaped'):
            self.maintenance.changes()
        path.unlink()
        for forbidden in ('launcher/publish.py', 'distribution/check_catalog.py', 'site/launcher_feed.py'):
            path = self.work / forbidden
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('unreviewed publication automation')
            with self.assertRaisesRegex(RuntimeError, 'escaped'):
                self.maintenance.changes()
            path.unlink()

    def test_defined_unowned_game_is_companion_owned_but_preserve_cannot_override_managed_source(self):
        self.catalog['preserved_paths'] = ['games/owned']
        other = companion.Maintenance(self.config, self.run, self.work, self.base, self.catalog, self.definitions,
            command=self.command, git=git, write_json=lambda path, value: path.write_text(json.dumps(value)), log=self.log)
        self.assertEqual(other.games, {'owned', 'other'})
        self.catalog['files'].append({'path': 'games/owned/Cargo.toml', 'sha256': 'b' * 64})
        conflicting = companion.Maintenance(self.config, self.run, self.work, self.base, self.catalog, self.definitions,
            command=self.command, git=git, write_json=lambda path, value: path.write_text(json.dumps(value)), log=self.log)
        self.assertEqual(conflicting.games, {'other'})

    def test_ordinary_engine_export_is_integrated_before_companion_checks_without_touching_user_work(self):
        (self.work / 'site/view.mjs').write_text('candidate fix\n')
        upstream = self.root / 'export'
        git(self.root, 'clone', str(self.remote), str(upstream))
        (upstream / 'engine-export.json').write_text('new engine catalog\n')
        git(upstream, '-c', 'user.name=Fixture', '-c', 'user.email=f@example.invalid', 'add', '.')
        git(upstream, '-c', 'user.name=Fixture', '-c', 'user.email=f@example.invalid', 'commit', '-qm', 'Engine export')
        git(upstream, 'push', 'origin', 'main')
        self.maintenance.check = Mock()
        self.maintenance.wait_ci = Mock()
        result = self.maintenance.publish(self.source, 'Fix view')
        self.assertEqual(result['status'], 'verified')
        self.assertEqual((self.work / 'engine-export.json').read_text(), 'new engine catalog\n')
        self.assertEqual((self.work / 'site/view.mjs').read_text(), 'candidate fix\n')
        self.assertEqual((self.source / 'site/view.mjs').read_text(), 'valuable unfinished user work\n')

    def test_publication_failure_resumes_same_source_and_rechecks_retained_cleanliness(self):
        (self.work / 'site/view.mjs').write_text('candidate fix\n')
        self.maintenance.check = Mock()
        self.maintenance.wait_ci = Mock(side_effect=[{}, RuntimeError('Pages pending')])
        with self.assertRaisesRegex(RuntimeError, 'Pages pending'):
            self.maintenance.publish(self.source, 'Fix view')
        restored = companion.Maintenance.resume(self.config, self.run, command=self.command, git=git,
            write_json=lambda path, value: path.write_text(json.dumps(value)), log=self.log)
        restored.wait_ci = Mock()
        self.assertEqual(restored.finish_publication(self.source)['status'], 'verified')
        self.assertEqual(restored.state['commit'], self.maintenance.state['commit'])
        (self.work / 'site/view.mjs').write_text('changed after review')
        with self.assertRaisesRegex(RuntimeError, 'source changed'):
            restored.finish_publication(self.source)

    def test_accepted_main_push_with_lost_ack_resumes_exact_checked_source(self):
        (self.work / 'site/view.mjs').write_text('candidate fix\n')
        self.maintenance.check = Mock()
        self.maintenance.wait_ci = Mock()
        def lost_ack(root, *args, **kwargs):
            output = git(root, *args, **kwargs)
            if args[:2] == ('push', 'origin') and args[-1].endswith(':main'):
                raise subprocess.CalledProcessError(128, ['git', 'push'])
            return output
        self.maintenance.git = lost_ack
        with self.assertRaises(subprocess.CalledProcessError):
            self.maintenance.publish(self.source, 'Fix view')
        self.assertEqual(self.maintenance.state['status'], 'publication-intent')
        delivered = git(self.root, '--git-dir', self.remote, 'rev-parse', 'main')
        restored = companion.Maintenance.resume(self.config, self.run, command=self.command, git=git,
            write_json=lambda path, value: path.write_text(json.dumps(value)), log=self.log)
        restored.wait_ci = Mock()
        self.assertEqual(restored.finish_publication(self.source)['status'], 'verified')
        self.assertEqual(restored.state['commit'], delivered)

    def test_launcher_requires_all_versions_and_preserves_updater_trust(self):
        self.versions(self.work, '0.1.1')
        self.assertEqual(companion.launcher_versions(self.work), '0.1.1')
        launcher, games = self.maintenance.validate_versions(self.maintenance.changes())
        self.assertTrue(launcher)
        self.assertEqual(games, [])
        path = self.work / 'launcher/src-tauri/tauri.conf.json'
        document = json.loads(path.read_text())
        document['plugins']['updater']['pubkey'] = 'different-key'
        path.write_text(json.dumps(document))
        with self.assertRaisesRegex(RuntimeError, 'signing trust'):
            self.maintenance.validate_versions(self.maintenance.changes())
        (self.work / 'launcher/package.json').write_text('{"version":"0.1.2"}')
        with self.assertRaisesRegex(RuntimeError, 'must match'):
            companion.launcher_versions(self.work)

    def test_local_checks_cannot_mutate_already_edited_content_and_then_publish_it(self):
        path = self.work / 'site/view.mjs'
        path.write_text('candidate fix\n')
        self.maintenance.check = Mock(side_effect=lambda *args, **kwargs: path.write_text('check modified the candidate\n'))
        with self.assertRaisesRegex(RuntimeError, 'changed edited source content'):
            self.maintenance.publish(self.source, 'Fix the view')
        self.assertIsNone(self.maintenance.state['commit'])
        self.assertEqual(git(self.root, '--git-dir', self.remote, 'rev-parse', 'main'), self.base)

    def test_failed_hosted_review_retains_private_source_without_publishing_or_touching_user_work(self):
        (self.work / 'site/view.mjs').write_text('candidate fix\n')
        self.maintenance.check = Mock()
        self.maintenance.wait_ci = Mock(side_effect=RuntimeError('hosted review failed'))
        with self.assertRaisesRegex(RuntimeError, 'hosted review failed'):
            self.maintenance.publish(self.source, 'Fix the view')
        self.assertEqual(git(self.root, '--git-dir', self.remote, 'rev-parse', 'main'), self.base)
        self.assertEqual((self.work / 'site/view.mjs').read_text(), 'candidate fix\n')
        self.assertEqual((self.source / 'site/view.mjs').read_text(), 'valuable unfinished user work\n')
        self.assertEqual(self.maintenance.state['status'], 'reviewing')

    def test_site_fix_publishes_only_after_review_and_requires_exact_source_pages(self):
        (self.work / 'site/view.mjs').write_text('candidate fix\n')
        self.maintenance.check = Mock()
        observations = []
        def ci(workflow, branch, sha, label, **kwargs):
            observations.append((label, git(self.root, '--git-dir', self.remote, 'rev-parse', 'main'), sha))
        self.maintenance.wait_ci = Mock(side_effect=ci)
        result = self.maintenance.publish(self.source, 'Fix the view')
        self.assertEqual(result['status'], 'verified')
        self.assertEqual(observations[0][0:2], ('distribution-review', self.base))
        self.assertEqual(observations[1], ('site-production', result['commit'], result['commit']))
        self.assertEqual((self.source / 'site/view.mjs').read_text(), 'valuable unfinished user work\n')

    def test_every_changed_game_is_reviewed_before_any_main_push(self):
        for slug in ('owned', 'other'):
            (self.work / 'games' / slug / 'Cargo.toml').write_text('[package]\nname="' + slug + '"\nversion="0.1.1"\n')
        self.maintenance.wait_ci = Mock()
        def check(argv, label, **kwargs):
            if label == 'other-native-review':
                (self.run / 'other-native-review.json').write_text('{"run":"/private/other"}')
            if label == 'owned-native-review':
                self.assertEqual(git(self.root, '--git-dir', self.remote, 'rev-parse', 'main'), self.base)
                raise RuntimeError('second game failed native checks')
        self.maintenance.check = Mock(side_effect=check)
        with self.assertRaisesRegex(RuntimeError, 'second game failed'):
            self.maintenance.publish(self.source, 'Fix both games')
        self.assertEqual(git(self.root, '--git-dir', self.remote, 'rev-parse', 'main'), self.base)

    def deliver_game(self, current_head):
        (self.work / 'games/owned/Cargo.toml').write_text('[package]\nname="owned"\nversion="0.1.1"\n')
        game_run = self.run / 'native-release'
        game_run.mkdir()
        (game_run / 'state.json').write_text(json.dumps({'status': 'published', 'publication': {'verified': True}}))
        def check(argv, label, **kwargs):
            if label == 'owned-native-review':
                (self.run / 'owned-native-review.json').write_text(json.dumps({'run': str(game_run)}))
        self.maintenance.check = Mock(side_effect=check)
        self.maintenance.wait_ci = Mock()
        def api(argv):
            if 'show' in argv:
                commit = self.maintenance.state['commit'] if current_head == 'delivered' else self.base
                return {'game': {'head_revision': current_head, 'revisions': [{'id': current_head,
                    'source': {'repository': self.config['games_repo'], 'commit': commit, 'game_path': 'games/owned'}}]}}
            if 'submit' in argv:
                self.fail('A concurrent or already-delivered creator head must never be overwritten')
            return {}
        self.maintenance.json_command = Mock(side_effect=api)
        return self.maintenance.publish(self.source, 'Correct owned game')

    def test_creator_draft_advanced_after_authoring_cannot_be_overwritten_by_release(self):
        with self.assertRaisesRegex(RuntimeError, 'Creator head advanced'):
            self.deliver_game('cousins-unmerged-draft')
        self.assertEqual(self.maintenance.state['status'], 'reviewing')
        self.assertEqual(self.maintenance.state['games']['owned']['lineage']['expected_revision'], 'before-owned')

    def test_auto_advanced_exact_delivered_creator_source_reuses_revision_without_duplicate_submit(self):
        result = self.deliver_game('delivered')
        self.assertEqual(result['status'], 'verified')
        self.assertEqual(result['games']['owned']['community_revision'], 'delivered')

    def test_failed_launcher_production_reruns_original_workflow_source_and_attempt(self):
        (self.work / 'site/view.mjs').write_text('fix\n')
        self.maintenance.check = Mock()
        self.maintenance.wait_ci = Mock()
        self.maintenance.publish(self.source, 'Fix view')
        self.maintenance.state.update(status='pushed-awaiting-publication', launcher_version='0.1.1')
        prior = {'id': 400, 'head_sha': self.base, 'conclusion': 'failure', 'run_attempt': 2}
        self.maintenance.state['ci']['launcher-production'] = prior
        self.maintenance.command = Mock()
        self.maintenance.verify_launcher_feed = Mock()
        self.maintenance.finish_publication(self.source)
        self.maintenance.command.assert_called_once_with(['gh', 'run', 'rerun', '400', '--repo', 'fixture/games'], timeout=180, log=self.log)
        self.maintenance.wait_ci.assert_called_with('launcher.yml', 'main', self.base, 'launcher-production', expected_id=400, min_attempt=3)

    def test_ci_receipts_require_exact_source_branch_event_path_and_new_run(self):
        valid = {'id': 8, 'head_sha': self.base, 'head_branch': 'review', 'event': 'workflow_dispatch',
                 'path': '.github/workflows/launcher.yml', 'status': 'completed', 'conclusion': 'success', 'html_url': 'https://example.invalid/8'}
        impostors = [dict(valid, id=99, head_sha='a' * 40), dict(valid, id=98, event='push'),
                     dict(valid, id=97, head_branch='main'), dict(valid, id=96, path='.github/workflows/other.yml')]
        self.maintenance.command = Mock(return_value=json.dumps({'workflow_runs': impostors + [valid]}))
        receipt = self.maintenance.wait_ci('launcher.yml', 'review', self.base, 'launcher-review', after_id=7)
        self.assertEqual(receipt['id'], 8)

    def test_launcher_metadata_uses_trusted_base_cargo_graph_and_exact_production_signing_config(self):
        helper = self.source / 'launcher/publish.py'
        helper.write_text('''import json
class GitHub:
    def __init__(self, repository): self.repository = repository
    def release(self, tag): return {"tag": tag}
def verify_signature(root, config, installer, signature, version):
    manifest = (root / "launcher/src-tauri/Cargo.toml").read_text()
    assert "untrusted-verifier-dependency" not in manifest
    return {"verifier_root": str(root), "manifest": manifest, "config": json.loads(config.read_text())}
def verify_release(github, release, source, version, directory, signature_check):
    metadata = signature_check(directory / "setup.exe", directory / "setup.exe.sig", version)
    return dict(metadata, source=source, version=version)
''')
        git(self.source, 'add', 'launcher/publish.py')
        git(self.source, 'commit', '-qm', 'Trusted publisher fixture')
        trusted_base = git(self.source, 'rev-parse', 'HEAD')
        self.maintenance.base = trusted_base
        git(self.work, 'reset', '--hard', trusted_base)
        trusted_manifest = (self.work / 'launcher/src-tauri/Cargo.toml').read_text()
        (self.work / 'launcher/publish.py').write_text('raise RuntimeError("AI-edited helper executed")')
        with (self.work / 'launcher/src-tauri/Cargo.toml').open('a') as manifest:
            manifest.write('\n# untrusted-verifier-dependency\n')
        config = self.work / 'launcher/src-tauri/tauri.conf.json'
        config.write_text(json.dumps({'version': '0.1.1', 'plugins': {'updater': {'pubkey': 'production-key'}}}))
        git(self.work, 'add', 'launcher')
        git(self.work, 'commit', '-qm', 'Production source with edited dependencies')
        production = git(self.work, 'rev-parse', 'HEAD')
        self.maintenance.state['ci']['launcher-production'] = {'head_sha': production}

        metadata, release = self.maintenance.launcher_metadata('0.1.1')

        self.assertEqual(metadata['manifest'], trusted_manifest)
        self.assertEqual(metadata['config']['plugins']['updater']['pubkey'], 'production-key')
        self.assertEqual(metadata['source'], production)
        self.assertEqual(release['tag'], 'launcher-v0.1.1')
        self.assertNotEqual(Path(metadata['verifier_root']), self.work)
        self.assertFalse(Path(metadata['verifier_root']).exists())
        self.assertNotIn('trusted-source', git(self.work, 'worktree', 'list', '--porcelain'))
        self.assertEqual((self.source / 'site/view.mjs').read_text(), 'valuable unfinished user work\n')

    def launcher_publication(self, altered=False):
        binary, signature = b'MZverified Windows installer', 'signed-asset-proof'
        digest = hashlib.sha256(binary).hexdigest()
        url = 'https://github.com/fixture/games/releases/download/launcher-v0.1.1/setup.exe'
        update = {'version': '0.1.1', 'platforms': {'windows-x86_64': {'url': url, 'signature': signature}}}
        metadata = {'version': '0.1.1', 'url': url, 'size': len(binary), 'sha256': digest, 'update': update}
        release = {'tag_name': 'launcher-v0.1.1', 'html_url': 'https://example.invalid/release', 'assets': [
            {'name': 'BlueEngine-Games-Setup-x64.exe', 'browser_download_url': url, 'size': len(binary), 'digest': 'sha256:' + digest},
            {'name': 'BlueEngine-Games-Setup-x64.exe.sig', 'browser_download_url': url + '.sig'}]}
        verified = dict(metadata)
        if altered:
            metadata['sha256'] = '0' * 64
        responses = {'https://fixture.github.io/games/launcher-release.json': json.dumps(metadata).encode(),
                     'https://fixture.github.io/games/launcher-update.json': json.dumps(update).encode(),
                     url + '.sig': signature.encode(), url: binary}
        self.maintenance.launcher_metadata = Mock(return_value=(verified, release))
        self.maintenance.state['ci']['launcher-production'] = {'head_sha': self.base}
        return responses, digest

    def test_signature_or_source_validation_failure_cannot_certify_publication(self):
        self.maintenance.launcher_metadata = Mock(side_effect=ValueError('Wrong source SHA or invalid signed version'))
        with self.assertRaisesRegex(ValueError, 'Wrong source SHA'):
            self.maintenance.verify_launcher_feed('0.1.1')
        self.assertNotIn('launcher_publication', self.maintenance.state)

    def test_public_launcher_feed_signature_size_and_download_hash_match_immutable_assets(self):
        responses, digest = self.launcher_publication()
        with patch.object(companion, 'urlopen', side_effect=lambda request, **kwargs: io.BytesIO(responses[request.full_url])):
            self.maintenance.verify_launcher_feed('0.1.1')
        self.assertEqual(self.maintenance.state['launcher_publication']['sha256'], digest)

    def test_forged_public_feed_cannot_certify_launcher_delivery(self):
        responses, _ = self.launcher_publication(altered=True)
        with patch.object(companion, 'urlopen', side_effect=lambda request, **kwargs: io.BytesIO(responses[request.full_url])), \
                patch.object(companion.time, 'monotonic', side_effect=[0, 2]), self.assertRaisesRegex(RuntimeError, 'immutable installer'):
            self.maintenance.verify_launcher_feed('0.1.1')
        self.assertNotIn('launcher_publication', self.maintenance.state)


if __name__ == '__main__':
    unittest.main()
