"""Integration checks using real local Git repositories; no production writes."""
import contextlib
from datetime import datetime, timedelta, timezone
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('nightly', Path(__file__).with_name('run.py'))
nightly = importlib.util.module_from_spec(spec)
spec.loader.exec_module(nightly)


def git(*args, cwd=None):
    return subprocess.check_output(['git', *map(str, args)], cwd=cwd, text=True, stderr=subprocess.DEVNULL).strip()


class PublicationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.remote, self.engine = self.root / 'remote.git', self.root / 'engine'
        git('init', '--bare', '--initial-branch=main', self.remote)
        git('clone', self.remote, self.engine)
        git('config', 'user.name', 'Test', cwd=self.engine)
        git('config', 'user.email', 'test@example.invalid', cwd=self.engine)
        (self.engine / 'source.txt').write_text('original\n')
        (self.engine / 'tools').mkdir()
        (self.engine / 'tools/be2.py').write_text('''import os, pathlib, socket, sys, tempfile
report = pathlib.Path(os.environ.get('TEST_CHECK_REPORT_DIR', '..'))
(report / 'check-tmpdir.txt').write_text(os.environ['TMPDIR'])
(report / 'check-target.txt').write_text(os.environ['CARGO_TARGET_DIR'])
(report / 'check-workdir.txt').write_text(str(pathlib.Path.cwd()))
with tempfile.TemporaryDirectory() as directory, socket.socket(socket.AF_UNIX) as control:
    control.bind(str(pathlib.Path(directory) / 'control.sock'))
sys.exit(int(os.environ.get('TEST_CHECK_FAIL', '0')))
''')
        git('add', '.', cwd=self.engine)
        git('commit', '-m', 'baseline', cwd=self.engine)
        git('push', 'origin', 'main', cwd=self.engine)
        self.base = git('rev-parse', 'HEAD', cwd=self.engine)
        # A developer's unrelated dirty source must remain untouched.
        (self.engine / 'source.txt').write_text('unfinished developer work\n')
        self.config_dir = self.root / 'config'
        self.config_dir.mkdir()
        self.fake = self.root / 'codex'
        self.fake.write_text('''#!/usr/bin/python3
import json, os, pathlib, re, sys
args = sys.argv
prompt = sys.stdin.read()
digest_path = re.search(r'Feedback digest: (.+)', prompt).group(1)
selected = json.loads(pathlib.Path(digest_path).read_text())['selected']
dispositions = [{'feedback_id': item['feedback_id'], 'status': os.environ.get('TEST_DISPOSITION', 'addressed'),
                 'summary': 'Verified observation', 'next_step': 'Reproduce the remaining case'} for item in selected]
if os.environ.get('TEST_OMIT_DISPOSITIONS'):
    dispositions = []
work = pathlib.Path(args[args.index('--cd') + 1])
report = pathlib.Path(args[args.index('--output-last-message') + 1]).parent
(report / 'agent-tmpdir.txt').write_text(os.environ['TMPDIR'])
(report / 'agent-target.txt').write_text(os.environ['CARGO_TARGET_DIR'])
(report / 'agent-tmpmode.txt').write_text(str(pathlib.Path(os.environ['TMPDIR']).stat().st_mode & 0o777))
(report / 'agent-workdir.txt').write_text(str(work))
(report / 'agent-workmode.txt').write_text(str(work.stat().st_mode & 0o777))
status = os.environ.get('TEST_STATUS', 'updated')
if status != 'no_changes':
    (work / 'source.txt').write_text(os.environ.get('TEST_SOURCE', 'nightly fix') + '\\n')
if os.environ.get('TEST_DIRTY_NO_CHANGE'):
    (work / 'source.txt').write_text('unexpected\\n')
pathlib.Path(args[args.index('--output-last-message') + 1]).write_text(json.dumps({
    'status': status, 'summary': 'Fixed feedback item.', 'feedback_addressed': [], 'checks': [], 'deferred': [],
    'dispositions': dispositions}))
''')
        self.fake.chmod(0o700)
        self.state = self.root / 'state'
        config = {'engine': str(self.engine), 'games': str(self.engine), 'branch': 'main',
                  'engine_repo': 'test/engine', 'games_repo': 'test/games', 'codex': str(self.fake),
                  'state': str(self.state), 'codex_timeout_seconds': 20, 'check_timeout_seconds': 20,
                  'ci_timeout_seconds': 3, 'ci_poll_seconds': 1}
        (self.config_dir / 'config.json').write_text(json.dumps(config))
        (self.config_dir / 'prompt.txt').write_text('Test task')
        self.ci_calls = []
        self.old_umask = os.umask(0o077)

    def tearDown(self):
        os.umask(self.old_umask)
        self.temp.cleanup()

    def execute(self, *options, ci_responses=None, **env):
        def feedback(*args):
            return {'files': [{'sha256': 'feedback-hash',
                               'feedback_signals': [os.environ.get('TEST_FEEDBACK', 'feedback-hash')]}], 'warnings': []}
        responses = list(ci_responses if ci_responses is not None else ['success'])
        clock = [0]
        original = nightly.command

        def ci_command(args, **kwargs):
            if args[:2] == ['gh', 'api'] and '/actions/workflows/ci.yml/runs?' in args[2]:
                sha = args[2].split('head_sha=', 1)[1].split('&', 1)[0]
                self.ci_calls.append({'sha': sha, 'seen_exists': (self.state / 'feedback-seen.json').exists(),
                                      'success_exists': (self.state / 'last-success.json').exists(),
                                      'prompts_disabled': os.environ.get('GH_PROMPT_DISABLED')})
                response = responses.pop(0) if len(responses) > 1 else responses[0]
                if response is None:
                    return json.dumps({'workflow_runs': []})
                run = {'id': 17, 'html_url': 'https://github.com/test/engine/actions/runs/17',
                       'head_sha': sha, 'head_branch': 'main', 'event': 'push',
                       'path': '.github/workflows/ci.yml', 'run_attempt': 1,
                       'status': 'completed', 'conclusion': 'success'}
                if isinstance(response, dict):
                    run.update(response)
                elif response in {'queued', 'in_progress'}:
                    run.update(status=response, conclusion=None)
                else:
                    run['conclusion'] = response
                return json.dumps({'workflow_runs': [run]})
            return original(args, **kwargs)

        def advance(seconds):
            clock[0] += seconds

        with patch.object(nightly, 'CONFIG_DIR', self.config_dir), patch.object(nightly, 'preflight'), \
             patch.object(nightly, 'collect_feedback', side_effect=feedback), \
             patch.object(nightly, 'command', side_effect=ci_command), \
             patch.object(nightly.os, 'fsync'), \
             patch.object(nightly.time, 'monotonic', side_effect=lambda: clock[0]), \
             patch.object(nightly.time, 'sleep', side_effect=advance), \
             patch.object(sys, 'argv', ['run.py', *options]), patch.dict(os.environ, env), \
             contextlib.redirect_stdout(io.StringIO()):
            return nightly.main()

    def remote_head(self):
        return git('--git-dir', self.remote, 'rev-parse', 'main')

    def test_receipt_flushes_file_and_directory_before_returning(self):
        destination = self.root / 'durable.json'
        with patch.object(nightly.os, 'fsync') as synced:
            nightly.write_json(destination, {'acknowledged': True})
        self.assertEqual(synced.call_count, 2)
        self.assertEqual(json.loads(destination.read_text()), {'acknowledged': True})
        self.assertFalse(destination.with_suffix('.json.tmp').exists())

    def test_validated_update_pushes_and_preserves_developer_work(self):
        self.assertEqual(self.execute(), 0)
        self.assertNotEqual(self.remote_head(), self.base)
        self.assertIn('Validated and pushed', (self.state / 'latest.md').read_text())
        self.assertEqual((self.engine / 'source.txt').read_text(), 'unfinished developer work\n')
        self.assertEqual(git('rev-parse', 'HEAD', cwd=self.engine), self.base)
        self.assertFalse(list((self.state / 'runs').glob('*/worktree')))

    def feedback_commit(self, extra=None, valid=True):
        # Match the importer's actual token/skip outcome: no CI at this SHA.
        path = self.engine / 'docs/player-feedback/reviews/1.json'
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('{"body":"untrusted observation"}')
        (self.engine / 'tools/player_feedback.py').write_text(f'import sys; sys.exit({0 if valid else 1})\n')
        git('add', 'tools/player_feedback.py', cwd=self.engine)
        git('commit', '-m', 'install fixture validator', cwd=self.engine)
        code = git('rev-parse', 'HEAD', cwd=self.engine)
        git('add', 'docs/player-feedback', cwd=self.engine)
        if extra:
            (self.engine / extra).write_text('unverified code')
            git('add', extra, cwd=self.engine)
        git('commit', '-m', 'Sync player feedback [skip ci]', cwd=self.engine)
        git('push', 'origin', 'main', cwd=self.engine)
        return code, git('rev-parse', 'HEAD', cwd=self.engine)

    def test_feedback_only_without_push_ci_uses_verified_code_and_finishes_once(self):
        code, tip = self.feedback_commit()
        self.assertEqual(self.execute(TEST_STATUS='no_changes'), 0)
        self.assertEqual([call['sha'] for call in self.ci_calls], [code])
        evidence = json.loads((self.state / 'last-success.json').read_text())['hosted_ci']
        self.assertEqual(evidence['reviewed_sha'], tip)
        self.assertEqual(evidence['feedback_only_commits'], [tip])
        self.assertFalse(list((self.state / 'runs').glob('*/worktree')))
        # Durable acknowledgment survives another runner invocation, without AI or CI.
        self.assertEqual(self.execute(), 0)
        self.assertEqual(len(self.ci_calls), 1)
        self.assertEqual(len(list((self.state / 'runs').glob('*/codex.jsonl'))), 1)

    def test_invalid_feedback_data_cannot_inherit_ci(self):
        self.feedback_commit(valid=False)
        self.assertEqual(self.execute(TEST_STATUS='no_changes'), 1)
        self.assertFalse(self.ci_calls)
        self.assertFalse((self.state / 'feedback-seen.json').exists())
        self.assertFalse(list((self.state / 'runs').glob('*/worktree')))

    def test_community_events_require_validation_before_inheriting_code_ci(self):
        validator = self.engine / 'tools/community_store.py'
        validator.write_text('import sys; sys.exit(0)\n')
        git('add', validator, cwd=self.engine)
        git('commit', '-m', 'Install fixture event validator', cwd=self.engine)
        code = git('rev-parse', 'HEAD', cwd=self.engine)
        event = self.engine / 'docs/community/events/feedback.json'
        event.parent.mkdir(parents=True)
        event.write_text('{"event_type":"feedback"}')
        git('add', event, cwd=self.engine)
        git('commit', '-m', 'Community feedback [skip ci]', cwd=self.engine)
        git('push', 'origin', 'main', cwd=self.engine)
        self.assertEqual(self.execute(TEST_STATUS='no_changes'), 0)
        self.assertEqual(self.ci_calls[-1]['sha'], code)
        # A broken validator is code and cannot inherit a previous receipt.
        validator.write_text('import sys; sys.exit(1)\n')
        git('add', validator, cwd=self.engine)
        git('commit', '-m', 'Invalid fixture event validator', cwd=self.engine)
        event.write_text('{"event_type":"feedback","note":"changed"}')
        git('add', event, cwd=self.engine)
        git('commit', '-m', 'More feedback [skip ci]', cwd=self.engine)
        git('push', 'origin', 'main', cwd=self.engine)
        before = len(self.ci_calls)
        self.assertEqual(self.execute(TEST_STATUS='no_changes', TEST_FEEDBACK='another-observation'), 1)
        self.assertEqual(len(self.ci_calls), before)

    def test_feedback_data_cannot_hide_unverified_code_or_failed_ancestor(self):
        _, tip = self.feedback_commit(extra='rules.rs')
        for response in (None, 'failure', 'cancelled', 'queued'):
            with self.subTest(response=response):
                self.assertEqual(self.execute(TEST_STATUS='no_changes', ci_responses=[response]), 1)
                self.assertEqual(self.ci_calls[-1]['sha'], tip)
                self.assertFalse((self.state / 'feedback-seen.json').exists())

    def test_feedback_only_requires_successful_ancestor_and_valid_data(self):
        code, _ = self.feedback_commit()
        for response in (None, 'failure', 'cancelled', 'queued'):
            with self.subTest(response=response):
                self.assertEqual(self.execute(TEST_STATUS='no_changes', ci_responses=[response]), 1)
                self.assertEqual(self.ci_calls[-1]['sha'], code)
                self.assertFalse((self.state / 'feedback-seen.json').exists())

    def test_delayed_exact_head_hosted_success_precedes_acknowledgment(self):
        self.assertEqual(self.execute(ci_responses=['queued', 'in_progress', 'success']), 0)
        self.assertEqual(len(self.ci_calls), 3)
        self.assertTrue(all(call['sha'] == self.remote_head() for call in self.ci_calls))
        self.assertTrue(all(not call['seen_exists'] and not call['success_exists'] for call in self.ci_calls))
        self.assertTrue(all(call['prompts_disabled'] == '1' for call in self.ci_calls))
        success = json.loads((self.state / 'last-success.json').read_text())
        self.assertEqual(success['hosted_ci']['head_sha'], self.remote_head())
        self.assertEqual(success['hosted_ci']['wait_outcome'], 'success')
        self.assertIn('https://github.com/test/engine/actions/runs/17', (self.state / 'latest.md').read_text())

    def test_hosted_failure_and_cancellation_do_not_consume_feedback_after_push(self):
        for index, conclusion in enumerate(('failure', 'cancelled', 'timed_out')):
            with self.subTest(conclusion=conclusion):
                before = self.remote_head()
                self.assertEqual(self.execute(ci_responses=[conclusion], TEST_SOURCE=f'fix {index}'), 1)
                self.assertNotEqual(self.remote_head(), before)
                self.assertFalse((self.state / 'feedback-seen.json').exists())
                self.assertFalse((self.state / 'last-success.json').exists())
                report = (self.state / 'latest.md').read_text()
                self.assertIn('Status: failed', report)
                self.assertIn('Pushed worktree retained for review', report)
                self.assertNotIn('Unpublished work retained', report)
                self.assertIn(conclusion, report)

    def test_hosted_timeout_or_absent_run_preserves_existing_acknowledgments(self):
        self.assertEqual(self.execute('--baseline'), 0)
        seen = (self.state / 'feedback-seen.json').read_bytes()
        for index, response in enumerate(('queued', None)):
            with self.subTest(response=response):
                self.assertEqual(self.execute(ci_responses=[response], TEST_SOURCE=f'fix {index}',
                                              TEST_FEEDBACK='new-feedback'), 1)
                self.assertEqual((self.state / 'feedback-seen.json').read_bytes(), seen)
                self.assertFalse((self.state / 'last-success.json').exists())
                report = (self.state / 'latest.md').read_text()
                self.assertIn('wait timeout', report)
                self.assertIn('Pushed worktree retained for review', report)

    def test_wrong_hosted_identity_cannot_acknowledge_feedback(self):
        overrides = ({'head_sha': '0' * 40}, {'path': '.github/workflows/other.yml'},
                     {'event': 'pull_request'}, {'head_branch': 'feature'})
        for index, response in enumerate(overrides):
            with self.subTest(response=response):
                self.assertEqual(self.execute(ci_responses=[response], TEST_SOURCE=f'fix {index}'), 1)
                self.assertFalse((self.state / 'feedback-seen.json').exists())
                self.assertFalse((self.state / 'last-success.json').exists())
                self.assertIn('No matching run found', (self.state / 'latest.md').read_text())

    def test_already_pushed_retry_can_acknowledge_no_changes_after_exact_ci_success(self):
        self.assertEqual(self.execute(ci_responses=['failure']), 1)
        pushed = self.remote_head()
        self.assertNotEqual(pushed, self.base)
        self.assertFalse((self.state / 'feedback-seen.json').exists())
        self.assertEqual(self.execute(ci_responses=['failure'], TEST_STATUS='no_changes'), 1)
        self.assertEqual(self.remote_head(), pushed)
        self.assertFalse((self.state / 'feedback-seen.json').exists())
        report = (self.state / 'latest.md').read_text()
        self.assertIn('No source work retained', report)
        self.assertNotIn('Unpublished work retained', report)
        self.assertEqual(self.execute(ci_responses=['queued', 'success'], TEST_STATUS='no_changes'), 0)
        self.assertEqual(self.remote_head(), pushed)
        self.assertEqual(len(list((self.state / 'runs').glob('*/codex.jsonl'))), 2)
        success = json.loads((self.state / 'last-success.json').read_text())
        self.assertEqual(success['base'], pushed)
        self.assertEqual(success['hosted_ci']['head_sha'], pushed)
        self.assertEqual(success['hosted_ci']['wait_outcome'], 'success')
        self.assertIn('Status: no_changes', (self.state / 'latest.md').read_text())
        calls = len(self.ci_calls)
        self.assertEqual(self.execute(), 0)
        self.assertEqual(len(self.ci_calls), calls)
        self.assertEqual(self.remote_head(), pushed)

    def test_no_changes_hosted_failure_preserves_previous_success_and_observations(self):
        self.state.mkdir()
        seen_path, success_path = self.state / 'feedback-seen.json', self.state / 'last-success.json'
        seen_path.write_text(json.dumps({'feedback_signals': ['previous-feedback'], 'completed_at': 'previous-time'}))
        success_path.write_text(json.dumps({'commit': 'previous-commit', 'result': {'summary': 'Earlier success'}}))
        seen, success = seen_path.read_bytes(), success_path.read_bytes()
        self.assertEqual(self.execute(ci_responses=['failure'], TEST_STATUS='no_changes'), 1)
        self.assertEqual(seen_path.read_bytes(), seen)
        self.assertEqual(success_path.read_bytes(), success)
        self.assertEqual(self.remote_head(), self.base)
        self.assertEqual(self.ci_calls[-1]['sha'], self.base)
        report = (self.state / 'latest.md').read_text()
        self.assertIn('Status: failed', report)
        self.assertIn('No source work retained', report)
        self.assertNotIn('Unpublished work retained', report)

    def test_deep_run_path_uses_short_private_temp_for_agent_and_checks(self):
        self.state = self.root / ('deep-state-' + 'x' * 100)
        config_path = self.config_dir / 'config.json'
        config = json.loads(config_path.read_text())
        config['state'] = str(self.state)
        config_path.write_text(json.dumps(config))
        self.assertEqual(self.execute(TMPDIR='/tmp'), 0)
        run = next((self.state / 'runs').iterdir())
        agent_temp = Path((run / 'agent-tmpdir.txt').read_text())
        check_temp = (run / 'check-tmpdir.txt').read_text()
        self.assertEqual(str(agent_temp), check_temp)
        self.assertEqual(agent_temp.parent, Path('/tmp'))
        self.assertEqual(int((run / 'agent-tmpmode.txt').read_text()), 0o700)
        self.assertFalse(agent_temp.exists())

    def test_configured_cargo_target_reaches_agent_and_verification(self):
        target = self.root / 'external-build-cache'
        self.assertEqual(self.execute(CARGO_TARGET_DIR=str(target)), 0)
        run = next((self.state / 'runs').iterdir())
        self.assertEqual((run / 'agent-target.txt').read_text(), str(target))
        self.assertEqual((run / 'check-target.txt').read_text(), str(target))
        self.assertFalse((self.state / 'target').exists())

    def test_unconfigured_cargo_target_preserves_state_cache_default(self):
        self.assertEqual(self.execute(CARGO_TARGET_DIR=''), 0)
        run = next((self.state / 'runs').iterdir())
        self.assertEqual((run / 'agent-target.txt').read_text(), str(self.state / 'target'))
        self.assertEqual((run / 'check-target.txt').read_text(), str(self.state / 'target'))

    def test_external_run_root_preserves_internal_state_and_morning_report(self):
        external = self.root / 'external-runs'
        config_path = self.config_dir / 'config.json'
        config = json.loads(config_path.read_text())
        config['runs_root'] = str(external)
        config_path.write_text(json.dumps(config))
        self.assertEqual(self.execute(), 0)
        run = next(external.iterdir())
        self.assertTrue((run / 'runner.log').is_file())
        self.assertTrue((run / 'report.md').is_file())
        self.assertIn(str(run), (self.state / 'latest.md').read_text())
        self.assertTrue((self.state / 'last-success.json').is_file())
        self.assertTrue((self.state / 'feedback-seen.json').is_file())
        self.assertTrue((self.state / 'run.lock').is_file())
        self.assertFalse((self.state / 'runs').exists())

    def configure_short_work_root(self):
        external = self.root / ('retained-receipts-' + 'x' * 100)
        work_root = self.root / 'w'
        check_report = self.root / 'check-report'
        check_report.mkdir()
        config_path = self.config_dir / 'config.json'
        config = json.loads(config_path.read_text())
        config.update(runs_root=str(external), worktrees_root=str(work_root))
        config_path.write_text(json.dumps(config))
        return external, work_root, check_report

    def test_short_work_root_reaches_agent_and_checks_then_removes_only_own_work(self):
        external, work_root, check_report = self.configure_short_work_root()
        sibling = work_root / 'retained-older-work'
        sibling.mkdir(parents=True, mode=0o700)
        (sibling / 'source.txt').write_text('Keep prior work')
        self.assertEqual(self.execute(TEST_CHECK_REPORT_DIR=str(check_report)), 0)
        run = next(external.iterdir())
        work = Path((run / 'agent-workdir.txt').read_text())
        self.assertEqual(work.parent, work_root)
        self.assertTrue(work.name.startswith('bn-'))
        self.assertLess(len(str(work)), len(str(run)))
        self.assertEqual((run / 'agent-workmode.txt').read_text(), str(0o700))
        self.assertEqual((check_report / 'check-workdir.txt').read_text(), str(work))
        self.assertEqual(json.loads((run / 'worktree.json').read_text()), {'path': str(work)})
        self.assertFalse(work.exists())
        self.assertNotIn(str(work), git('worktree', 'list', '--porcelain', cwd=self.engine))
        self.assertEqual((sibling / 'source.txt').read_text(), 'Keep prior work')
        self.assertTrue((run / 'report.md').is_file())
        self.assertFalse((self.state / 'runs').exists())

    def test_short_work_root_preserves_failed_work_and_links_exact_path(self):
        external, work_root, check_report = self.configure_short_work_root()
        self.assertEqual(self.execute(TEST_CHECK_REPORT_DIR=str(check_report), TEST_CHECK_FAIL='1'), 1)
        run = next(external.iterdir())
        work = Path((run / 'agent-workdir.txt').read_text())
        self.assertEqual(work.parent, work_root)
        self.assertEqual(work_root.stat().st_mode & 0o777, 0o700)
        self.assertEqual((check_report / 'check-workdir.txt').read_text(), str(work))
        self.assertEqual((work / 'source.txt').read_text(), 'nightly fix\n')
        self.assertIn(str(work), git('worktree', 'list', '--porcelain', cwd=self.engine))
        self.assertIn(f'Unpublished work retained: {work}', (run / 'report.md').read_text())
        self.assertEqual(self.remote_head(), self.base)
        self.assertFalse((self.state / 'feedback-seen.json').exists())

    def test_feedback_excludes_legacy_external_runs_and_short_work_root_but_includes_sibling(self):
        external = self.root / 'external-runs'
        work_root = self.root / 'w'
        config = json.loads((self.config_dir / 'config.json').read_text())
        config['runs_root'] = str(external)
        config['worktrees_root'] = str(work_root)
        checkouts = [self.state / 'runs' / 'legacy', external / 'retained',
                     work_root / 'retained', self.root / 'w-developer']
        for index, checkout in enumerate(checkouts):
            git('worktree', 'add', '--detach', checkout, self.base, cwd=self.engine)
            path = checkout / 'docs/feedback/retained.md'
            path.parent.mkdir(parents=True)
            path.write_text(f'Feedback from checkout {index}')
        run = self.root / 'collection'
        run.mkdir()
        git('fetch', 'origin', 'main', cwd=self.engine)
        original = nightly.command

        def local_command(args, **kwargs):
            if args[:2] == ['gh', 'api']:
                return '[[]]'
            return original(args, **kwargs)

        with patch.object(nightly, 'command', side_effect=local_command):
            manifest = nightly.collect_feedback(config, None, run, {})
        self.assertEqual(manifest['warnings'], [])
        self.assertEqual(len(manifest['files']), 1)
        self.assertEqual(manifest['files'][0]['origins'][0]['source'], str(checkouts[-1]))
        self.assertEqual(Path(manifest['files'][0]['file']).read_text(), 'Feedback from checkout 3')

    def test_malformed_local_event_is_retained_as_warning_without_blocking_other_feedback(self):
        directory = self.engine / 'docs/community/events'
        directory.mkdir(parents=True)
        (directory / 'bad.json').write_text('{unfinished draft')
        good = self.engine / 'docs/feedback/useful.md'
        good.parent.mkdir(parents=True)
        good.write_text('Reproduce a launch crash')
        run = self.root / 'collection'
        run.mkdir()
        git('fetch', 'origin', 'main', cwd=self.engine)
        original = nightly.command
        def local_command(args, **kwargs):
            return '[[]]' if args[:2] == ['gh', 'api'] else original(args, **kwargs)
        with patch.object(nightly, 'command', side_effect=local_command):
            manifest = nightly.collect_feedback(json.loads((self.config_dir / 'config.json').read_text()), None, run, {})
        self.assertEqual(len(manifest['warnings']), 1)
        self.assertIn('Malformed feedback omitted', manifest['warnings'][0])
        self.assertTrue(any(item['feedback_signals'] for item in manifest['files']))
        self.assertTrue(any(Path(item['file']).read_text() == '{unfinished draft' for item in manifest['files']))

    def test_failed_verification_never_pushes(self):
        self.assertEqual(self.execute(TEST_CHECK_FAIL='1'), 1)
        self.assertEqual(self.remote_head(), self.base)
        self.assertFalse((self.state / 'last-success.json').exists())
        self.assertTrue(list((self.state / 'runs').glob('*/worktree/source.txt')))
        run = next((self.state / 'runs').iterdir())
        self.assertFalse(Path((run / 'agent-tmpdir.txt').read_text()).exists())

    def test_omitted_dispositions_prevent_publication_or_acknowledgment(self):
        self.assertEqual(self.execute(TEST_OMIT_DISPOSITIONS='1'), 1)
        self.assertEqual(self.remote_head(), self.base)
        self.assertFalse((self.state / 'feedback-seen.json').exists())
        self.assertFalse((self.state / 'feedback-queue.json').exists())

    def test_deferred_feedback_retries_when_due_then_holds_after_three_attempts(self):
        for attempt in range(3):
            self.assertEqual(self.execute(TEST_STATUS='no_changes', TEST_DISPOSITION='deferred'), 0)
            path = self.state / 'feedback-queue.json'
            queue = json.loads(path.read_text())
            item = queue['items']['feedback-hash']
            self.assertEqual(item['attempts'], attempt + 1)
            self.assertEqual(item['status'], 'needs_human' if attempt == 2 else 'deferred')
            # Same-day wakeups launch no AI; successful deferrals remain visible.
            self.assertEqual(self.execute(TEST_STATUS='no_changes'), 0)
            self.assertEqual(len(list((self.state / 'runs').glob('*/codex.jsonl'))), attempt + 1)
            if attempt < 2:
                item['retry_after'] = '2000-01-01T00:00:00+00:00'
                path.write_text(json.dumps(queue))
        self.assertEqual(self.remote_head(), self.base)
        self.assertIn('1 held for a human', (self.state / 'latest.md').read_text())
        self.assertEqual(self.execute('--retry-feedback', 'feedback-hash'), 0)
        self.assertEqual(len(list((self.state / 'runs').glob('*/codex.jsonl'))), 3)
        queue = json.loads(path.read_text())
        self.assertEqual(queue['items']['feedback-hash']['manual_retries'][0]['previous_attempts'], 3)
        self.assertEqual(self.execute(TEST_STATUS='no_changes'), 0)
        self.assertEqual(len(list((self.state / 'runs').glob('*/codex.jsonl'))), 4)

    def test_failed_ci_does_not_consume_a_selected_queue_disposition(self):
        self.assertEqual(self.execute(ci_responses=['failure'], TEST_STATUS='no_changes'), 1)
        self.assertFalse((self.state / 'feedback-queue.json').exists())
        self.assertEqual(self.execute(TEST_STATUS='no_changes'), 0)
        item = json.loads((self.state / 'feedback-queue.json').read_text())['items']['feedback-hash']
        self.assertEqual(item['attempts'], 1)

    def test_blocked_agent_never_pushes(self):
        self.assertEqual(self.execute(TEST_STATUS='blocked'), 1)
        self.assertEqual(self.remote_head(), self.base)

    def test_no_changes_does_not_create_commit(self):
        self.assertEqual(self.execute(TEST_STATUS='no_changes'), 0)
        self.assertEqual(self.remote_head(), self.base)

    def test_dirty_no_changes_is_rejected(self):
        self.assertEqual(self.execute(TEST_STATUS='no_changes', TEST_DIRTY_NO_CHANGE='1'), 1)
        self.assertEqual(self.remote_head(), self.base)

    def test_preflight_failure_writes_morning_report(self):
        with patch.object(nightly, 'CONFIG_DIR', self.config_dir), \
             patch.object(nightly, 'preflight', side_effect=RuntimeError('not authenticated')), \
             patch.object(nightly, 'collect_feedback', return_value={
                 'files': [{'sha256': 'feedback', 'feedback_signals': ['new']}], 'warnings': []}), \
             patch.object(sys, 'argv', ['run.py']), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(nightly.main(), 1)
        self.assertIn('not authenticated', (self.state / 'latest.md').read_text())
        self.assertEqual(self.remote_head(), self.base)
        self.assertFalse((self.state / 'feedback-seen.json').exists())

    def test_concurrent_update_rejects_publication(self):
        competitor = self.root / 'competitor'
        git('clone', self.remote, competitor)
        git('config', 'user.name', 'Test', cwd=competitor)
        git('config', 'user.email', 'test@example.invalid', cwd=competitor)
        (competitor / 'other.txt').write_text('concurrent change\n')
        git('add', '.', cwd=competitor)
        git('commit', '-m', 'Concurrent update', cwd=competitor)
        competing_head = git('rev-parse', 'HEAD', cwd=competitor)
        original = nightly.command

        def concurrent(args, **kwargs):
            output = original(args, **kwargs)
            if args[:2] == ['python3', 'tools/be2.py']:
                git('push', 'origin', 'main', cwd=competitor)
            return output

        with patch.object(nightly, 'command', side_effect=concurrent):
            self.assertEqual(self.execute(), 1)
        self.assertEqual(self.remote_head(), competing_head)
        self.assertIn('Status: failed', (self.state / 'latest.md').read_text())


    def test_unchanged_feedback_skips_codex_and_preserves_deferred_work(self):
        self.state.mkdir()
        (self.state / 'feedback-seen.json').write_text(json.dumps({'feedback_signals': ['feedback-hash']}))
        (self.state / 'last-success.json').write_text(json.dumps({'result': {'deferred': ['Old unfinished task']}}))
        self.assertEqual(self.execute(), 0)
        self.assertEqual(self.remote_head(), self.base)
        self.assertIn('Status: skipped', (self.state / 'latest.md').read_text())
        self.assertFalse(list((self.state / 'runs').glob('*/codex.jsonl')))
        self.assertFalse(list((self.state / 'runs').glob('*/worktree')))
        self.assertFalse((self.state / 'target').exists())

    def test_new_feedback_runs_exactly_once(self):
        self.assertEqual(self.execute('--baseline'), 0)
        self.assertFalse(list((self.state / 'runs').glob('*/codex.jsonl')))
        self.assertEqual(self.execute(TEST_FEEDBACK='new-feedback'), 0)
        head = self.remote_head()
        self.assertNotEqual(head, self.base)
        self.assertEqual(self.execute(TEST_FEEDBACK='new-feedback'), 0)
        self.assertEqual(self.remote_head(), head)
        self.assertEqual(len(list((self.state / 'runs').glob('*/codex.jsonl'))), 1)
        self.assertIn('Status: skipped', (self.state / 'latest.md').read_text())

    def test_failed_feedback_retries_without_consuming_successful_observations(self):
        self.assertEqual(self.execute('--baseline'), 0)
        seen_path = self.state / 'feedback-seen.json'
        baseline = seen_path.read_text()
        self.assertEqual(self.execute(TEST_CHECK_FAIL='1', TEST_FEEDBACK='new-feedback'), 1)
        self.assertEqual(seen_path.read_text(), baseline)
        self.assertEqual(self.remote_head(), self.base)
        self.assertEqual(self.execute(TEST_FEEDBACK='new-feedback'), 0)
        published = self.remote_head()
        self.assertNotEqual(published, self.base)
        self.assertEqual(set(json.loads(seen_path.read_text())['feedback_signals']),
                         {'feedback-hash', 'new-feedback'})
        self.assertEqual(self.execute(TEST_FEEDBACK='new-feedback'), 0)
        self.assertEqual(self.remote_head(), published)
        self.assertEqual(len(list((self.state / 'runs').glob('*/codex.jsonl'))), 2)
        self.assertIn('Status: skipped', (self.state / 'latest.md').read_text())

    def test_blocked_feedback_retries_until_success_then_skips(self):
        self.assertEqual(self.execute(TEST_STATUS='blocked'), 1)
        self.assertFalse((self.state / 'feedback-seen.json').exists())
        self.assertEqual(self.remote_head(), self.base)
        self.assertEqual(self.execute(), 0)
        published = self.remote_head()
        self.assertNotEqual(published, self.base)
        self.assertEqual(self.execute(), 0)
        self.assertEqual(self.remote_head(), published)
        self.assertEqual(len(list((self.state / 'runs').glob('*/codex.jsonl'))), 2)
        self.assertIn('Status: skipped', (self.state / 'latest.md').read_text())

    def test_no_changes_acknowledges_feedback_without_repeating_agent(self):
        self.assertEqual(self.execute(TEST_STATUS='no_changes'), 0)
        self.assertEqual(self.execute(), 0)
        self.assertEqual(self.remote_head(), self.base)
        self.assertEqual(len(list((self.state / 'runs').glob('*/codex.jsonl'))), 1)
        self.assertIn('Status: skipped', (self.state / 'latest.md').read_text())

    def test_collect_only_preserves_acknowledgment_and_morning_report(self):
        self.assertEqual(self.execute('--baseline'), 0)
        seen = (self.state / 'feedback-seen.json').read_bytes()
        report = (self.state / 'latest.md').read_bytes()
        self.assertEqual(self.execute('--collect-only', TEST_FEEDBACK='new-feedback'), 0)
        self.assertEqual((self.state / 'feedback-seen.json').read_bytes(), seen)
        self.assertEqual((self.state / 'latest.md').read_bytes(), report)
        self.assertEqual(self.remote_head(), self.base)
        self.assertFalse(list((self.state / 'runs').glob('*/codex.jsonl')))


class FeedbackSignalTests(unittest.TestCase):
    def test_review_metadata_and_generated_feeds_do_not_trigger_work(self):
        review = {'game': 'test', 'version': 'v1', 'rating': 2, 'disliked': 'Cannot launch',
                  'name': 'Cousin', 'updated_at': 'yesterday'}
        updated = dict(review, name='AI assistant', updated_at='today')
        name = 'docs/player-feedback/reviews/review.json'
        self.assertEqual(nightly.feedback_signals(name, json.dumps(review).encode()),
                         nightly.feedback_signals(name, json.dumps(updated).encode()))
        updated['disliked'] = 'Controls jump unexpectedly'
        self.assertNotEqual(nightly.feedback_signals(name, json.dumps(review).encode()),
                            nightly.feedback_signals(name, json.dumps(updated).encode()))
        for path in ('docs/player-feedback/summary.json', 'docs/player-feedback/games/test.json'):
            self.assertEqual(nightly.feedback_signals(path, b'{"changed":true}'), [])

    def test_community_human_ai_feedback_have_equal_fingerprints_and_target_routing(self):
        event = {'event_type': 'feedback', 'actor': {'login': 'Owner'},
                 'feedback': {'target_type': 'launcher', 'target': '', 'kind': 'human',
                              'category': 'bug', 'note': 'Cannot publish an update', 'version': 'v1'}}
        other = json.loads(json.dumps(event))
        other['actor']['login'] = 'Cousin'
        other['feedback']['kind'] = 'ai'
        name = 'docs/community/events/receipt.json'
        self.assertTrue(nightly.feedback_path(name))
        self.assertEqual(nightly.feedback_signals(name, json.dumps(event).encode()),
                         nightly.feedback_signals(name, json.dumps(other).encode()))
        observation = nightly.feedback_observations(name, json.dumps(other).encode())[0]
        self.assertEqual(observation['scope'], 'launcher')
        self.assertEqual(observation['priority'], 2)
        self.assertEqual(nightly.feedback_signals(name, b'{"event_type":"revision_submitted"}'), [])
        request = {'event_type': 'publication_requested', 'request': {'game': 'test', 'revision': 'r1'}}
        self.assertTrue(nightly.feedback_signals(name, json.dumps(request).encode()))

    def test_digest_deduplicates_origins_bounds_work_and_retains_unselected_topics(self):
        now = datetime(2026, 10, 20, tzinfo=timezone.utc)
        manifest = {'files': [
            {'file': '/private/one', 'feedback_signals': ['same', 'other'],
             'origins': [{'source': 'engine', 'path': 'one'}]},
            {'file': '/private/two', 'feedback_signals': ['same'],
             'origins': [{'source': 'games', 'path': 'two'}]}]}
        digest, items = nightly.feedback_digest(manifest, {}, {}, {'max_feedback_items': 1}, now)
        self.assertEqual(len(digest['selected']), 1)
        self.assertEqual(digest['remaining'], 1)
        self.assertEqual(len(items['same']['origins']), 2)
        selected = digest['selected'][0]['feedback_id']
        receipt = nightly.feedback_receipt(digest, items, {'dispositions': [
            {'feedback_id': selected, 'status': 'no_change', 'summary': 'Already fixed', 'next_step': ''}]}, now)
        next_digest, _ = nightly.feedback_digest(manifest, {'feedback_signals': ['same', 'other']}, receipt,
                                                 {'max_feedback_items': 1}, now + timedelta(days=1))
        self.assertEqual(len(next_digest['selected']), 1)
        self.assertNotEqual(next_digest['selected'][0]['feedback_id'], selected)

    def test_disposition_rejects_unknown_duplicate_and_vague_deferred_items(self):
        now = datetime(2026, 10, 20, tzinfo=timezone.utc)
        digest, items = nightly.feedback_digest({'files': [{'feedback_signals': ['one']}]}, {}, {}, {}, now)
        valid = {'feedback_id': 'one', 'status': 'deferred', 'summary': 'Needs reproduction', 'next_step': 'Add a failing save test'}
        for dispositions in ([], [dict(valid, feedback_id='other')], [valid, valid], [dict(valid, next_step='')]):
            with self.subTest(dispositions=dispositions), self.assertRaises(RuntimeError):
                nightly.feedback_receipt(digest, items, {'dispositions': dispositions}, now)

    def test_promotion_and_discovery_edits_are_not_new_feedback(self):
        old = {'id': 'L-001', 'game': 'test', 'area': 'tooling', 'note': 'An authoring problem',
               'status': 'open', 'keywords': ['authoring'], 'tokens': 20}
        updated = dict(old, status='promoted', ref='fixed', workaround='Engine now fixes it', keywords=['new'])
        self.assertEqual(nightly.feedback_signals('docs/learning/ledger.jsonl', json.dumps(old).encode()),
                         nightly.feedback_signals('docs/learning/ledger.jsonl', json.dumps(updated).encode()))
        updated['note'] = 'A different authoring problem'
        self.assertNotEqual(nightly.feedback_signals('docs/learning/ledger.jsonl', json.dumps(old).encode()),
                            nightly.feedback_signals('docs/learning/ledger.jsonl', json.dumps(updated).encode()))

    def test_issue_metadata_does_not_trigger_but_new_comments_do(self):
        old = {'title': 'Game bug', 'body': 'Feedback', 'comments': [], 'state': 'open', 'updated_at': 'yesterday'}
        updated = dict(old, state='closed', updated_at='today')
        self.assertEqual(nightly.feedback_signals('issues/engine-1.json', json.dumps(old).encode()),
                         nightly.feedback_signals('issues/engine-1.json', json.dumps(updated).encode()))
        updated['comments'] = [{'body': 'New details', 'updated_at': 'today'}]
        self.assertNotEqual(nightly.feedback_signals('issues/engine-1.json', json.dumps(old).encode()),
                            nightly.feedback_signals('issues/engine-1.json', json.dumps(updated).encode()))

    def test_generated_reports_and_feedback_instructions_are_only_context(self):
        for name in ('docs/learning/REPORT.md', 'docs/learning/dupes.json',
                     'docs/AI_DEV_FEEDBACK.md', 'games/test/FEEDBACK_PROMPT.md'):
            self.assertEqual(nightly.feedback_signals(name, b'Changed report'), [])


if __name__ == '__main__':
    unittest.main()
