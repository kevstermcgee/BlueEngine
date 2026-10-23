"""Engine game delivery preserves optimistic lineage and resumable public proof."""
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock

import engine_games


def git(root, *args, **unused):
    return subprocess.run(['git', '-C', str(root), *map(str, args)], check=True,
                          capture_output=True, text=True).stdout.strip()


class EngineGameDeliveryTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.work, self.remote, self.run = [self.root / name for name in ('engine', 'origin.git', 'run')]
        self.run.mkdir()
        git(self.root, 'init', '--bare', '--initial-branch=main', self.remote)
        git(self.root, 'clone', self.remote, self.work)
        git(self.work, 'config', 'user.name', 'Fixture')
        git(self.work, 'config', 'user.email', 'fixture@example.invalid')
        game = self.work / 'games/owned'
        game.mkdir(parents=True)
        (game / 'game.project.json').write_text('{"id":"owned"}')
        (game / 'Cargo.toml').write_text('[package]\nname="owned"\nversion="0.1.0"\n')
        git(self.work, 'add', '.')
        git(self.work, 'commit', '-qm', 'Original game')
        git(self.work, 'push', 'origin', 'main')
        self.base = git(self.work, 'rev-parse', 'HEAD')
        self.config = {'engine_repo': 'fixture/engine', 'games': str(self.root / 'games'), 'check_timeout_seconds': 10}
        self.current = {'head_revision': 'before', 'revisions': [{'id': 'before', 'source': {
            'repository': 'fixture/engine', 'commit': self.base, 'game_path': 'games/owned'}}]}
        self.events, self.fail_metadata, self.advance_review = [], False, False
        self.command = Mock(side_effect=self.execute)
        self.callbacks = dict(command=self.command, git=git,
            write_json=lambda path, value: path.write_text(json.dumps(value)), log=io.StringIO())
        self.digest = {'selected': [{'feedback_id': 'feedback', 'game': 'owned', 'scope': 'game'}]}
        self.result = {'dispositions': [{'feedback_id': 'feedback', 'status': 'addressed'}]}
        self.delivery = engine_games.Delivery.prepare(self.config, self.run, self.digest, self.work, **self.callbacks)

    def execute(self, argv, **unused):
        if argv[1].endswith('community.py'):
            if 'show' in argv:
                return json.dumps({'game': self.current})
            if 'submit' in argv:
                self.events.append('submit')
                self.assertEqual(argv[argv.index('--expected-revision') + 1], 'before')
                source = argv[argv.index('--commit') + 1]
                self.current['head_revision'] = 'corrected'
                self.current['revisions'].append({'id': 'corrected', 'source': {
                    'repository': 'fixture/engine', 'commit': source, 'game_path': 'games/owned'}})
                return json.dumps({'event': {'revision': {'id': 'corrected'}}})
            if 'record-release' in argv:
                self.events.append('record-release')
                if self.fail_metadata:
                    raise RuntimeError('Community metadata unavailable')
                return '{"event":{"event_type":"release_verified"}}'
        phase = argv[2]
        self.events.append(phase)
        directory = self.run / 'private-release'
        if phase == 'create':
            directory.mkdir()
            (directory / 'state.json').write_text('{"status":"created","completed":[]}')
            Path(argv[argv.index('--receipt') + 1]).write_text(json.dumps({'run': str(directory)}))
        elif phase == 'review':
            self.assertIn('submit', self.events)
            if self.advance_review:
                self.current['head_revision'] = 'concurrent'
        elif phase == 'resume':
            self.assertIn('review', self.events)
            (directory / 'state.json').write_text(json.dumps({'status': 'published', 'publication': {
                'release': 'https://github.com/fixture/games/releases/tag/checked', 'sha256': 'a' * 64}}))
        return ''

    def source_update(self):
        (self.work / 'games/owned/Cargo.toml').write_text('[package]\nname="owned"\nversion="0.1.1"\n')
        git(self.work, 'add', '.')
        git(self.work, 'commit', '-qm', 'Corrected game')
        git(self.work, 'push', 'origin', 'main')
        self.delivery.select(self.digest, self.result, ['games/owned/Cargo.toml'])
        self.delivery.intend(git(self.work, 'rev-parse', 'HEAD'))

    def test_engine_game_release_registers_source_before_native_review_and_public_delivery(self):
        self.source_update()
        result = self.delivery.deliver(self.work)
        self.assertEqual(self.events, ['submit', 'create', 'review', 'resume', 'record-release'])
        self.assertEqual(result['status'], 'verified')
        self.assertEqual(result['games']['owned']['community_revision'], 'corrected')

    def test_missing_game_wiki_is_reported_before_authoring(self):
        self.assertIn('wiki.json', self.delivery.state['release_requirements']['owned'])
        self.assertIn('owned', self.delivery.state['baselines'])

    def test_concurrent_draft_before_delivery_stops_before_privileged_build(self):
        self.source_update()
        self.current['head_revision'] = 'cousins-draft'
        self.current['revisions'].append({'id': 'cousins-draft', 'source': self.current['revisions'][0]['source']})
        with self.assertRaisesRegex(RuntimeError, 'concurrent draft'):
            self.delivery.deliver(self.work)
        self.assertEqual(self.events, [])

    def test_creator_change_after_review_prevents_public_native_delivery(self):
        self.source_update()
        self.advance_review = True
        with self.assertRaisesRegex(RuntimeError, 'advanced during engine game review'):
            self.delivery.deliver(self.work)
        self.assertEqual(self.events, ['submit', 'create', 'review'])
        self.assertEqual(self.delivery.state['games']['owned']['revision_id'], 'corrected')

    def test_metadata_outage_reuses_completed_native_release_without_duplicate_publication(self):
        self.source_update()
        self.fail_metadata = True
        with self.assertRaisesRegex(RuntimeError, 'metadata unavailable'):
            self.delivery.deliver(self.work)
        resumed = engine_games.Delivery.resume(self.config, self.run, **self.callbacks)
        self.fail_metadata = False
        result = resumed.deliver(self.work)
        self.assertEqual(result['status'], 'verified')
        self.assertEqual(self.events.count('resume'), 1)
        self.assertEqual(self.events.count('create'), 1)
        self.assertEqual(self.events.count('submit'), 1)
        self.assertEqual(self.events.count('record-release'), 2)

    def test_unselected_or_unmigrated_game_has_no_authority_to_publish(self):
        with self.assertRaisesRegex(RuntimeError, 'pre-authoring creator baseline'):
            self.delivery.select(self.digest, self.result, ['games/unselected/src/main.rs'])

    def test_engine_feedback_from_game_needs_no_game_baseline_or_delivery(self):
        self.digest['selected'][0]['scope'] = 'engine'
        self.delivery.state['baselines'].clear()
        self.delivery.select(self.digest, self.result, ['tools/be2.py'])
        self.assertEqual(self.delivery.state['wanted'], [])
        self.assertIsNone(self.delivery.deliver(self.work))
        self.assertEqual(self.events, [])

    def test_addressed_game_feedback_still_needs_baseline_without_source_edits(self):
        self.delivery.state['baselines'].clear()
        with self.assertRaisesRegex(RuntimeError, 'pre-authoring creator baseline'):
            self.delivery.select(self.digest, self.result, [])

    def test_direct_game_edits_still_need_baseline_for_engine_feedback(self):
        self.digest['selected'][0]['scope'] = 'engine'
        self.delivery.state['baselines'].clear()
        with self.assertRaisesRegex(RuntimeError, 'pre-authoring creator baseline'):
            self.delivery.select(self.digest, self.result, ['games/owned/Cargo.toml'])

    def test_retained_tooling_or_source_change_cannot_resume_old_release_gates(self):
        self.source_update()
        (self.work / 'games/owned/Cargo.toml').write_text('changed after source intent')
        with self.assertRaisesRegex(RuntimeError, 'tooling/source changed'):
            self.delivery.deliver(self.work)
        self.assertEqual(self.events, [])


if __name__ == '__main__':
    unittest.main()
