#!/usr/bin/env python3
"""Collect feedback, run unattended Codex, verify and fast-forward BlueEngine."""
import argparse
from datetime import datetime, timedelta, timezone
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

CONFIG_DIR = Path.home() / '.config' / 'blueengine-nightly'


def command(args, *, cwd=None, timeout=120, log=None):
    if log:
        log.write('\n$ ' + ' '.join(map(str, args)) + '\n')
        log.flush()
    result = subprocess.run(list(map(str, args)), cwd=cwd, timeout=timeout,
                            text=True, stdout=log or subprocess.PIPE,
                            stderr=subprocess.STDOUT, check=True)
    return result.stdout or ''


def git(repo, *args, **kwargs):
    return command(['git', '-C', repo, *args], **kwargs)


def write_json(path, value):
    temp = path.with_suffix(path.suffix + '.tmp')
    with temp.open('w') as stream:
        stream.write(json.dumps(value, indent=2) + '\n')
        stream.flush()
        os.fsync(stream.fileno())
    temp.replace(path)
    directory = os.open(path.parent, os.O_DIRECTORY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def wait_for_hosted_ci(config, sha, run, log):
    """Require the exact main push's Engine checks before acknowledging feedback."""
    timeout = int(config.get('ci_timeout_seconds', 5400))
    poll = int(config.get('ci_poll_seconds', 30))
    if config['branch'] != 'main' or timeout <= 0 or not 1 <= poll <= 60:
        raise ValueError('Hosted CI requires main, a positive timeout, and a 1–60 second poll interval')
    endpoint = (f'repos/{config["engine_repo"]}/actions/workflows/ci.yml/runs'
                f'?branch=main&event=push&head_sha={sha}&per_page=100')
    evidence = {'head_sha': sha, 'head_branch': 'main', 'event': 'push',
                'workflow': '.github/workflows/ci.yml', 'status': 'not_found',
                'wait_outcome': 'waiting'}
    path = run / 'hosted-ci.json'
    deadline = time.monotonic() + timeout
    previous = None

    def save():
        nonlocal previous
        write_json(path, evidence)
        summary = (evidence.get('id'), evidence['status'], evidence.get('conclusion'), evidence['wait_outcome'])
        if summary != previous:
            log.write(f'Hosted Engine checks: {summary}; {evidence.get("url", sha)}\n')
            log.flush()
            previous = summary

    def expired():
        evidence['wait_outcome'] = 'timeout'
        save()
        raise TimeoutError(f'Hosted Engine checks did not succeed within {timeout}s for {sha}; '
                           f'last status {evidence["status"]}; {evidence.get("url", "no matching run found")}')

    save()
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            expired()
        try:
            data = json.loads(command(['gh', 'api', endpoint], timeout=min(120, max(1, remaining))))
            matches = [item for item in data['workflow_runs']
                       if item.get('head_sha') == sha and item.get('head_branch') == 'main'
                       and item.get('event') == 'push'
                       and item.get('path', '').split('@', 1)[0] == '.github/workflows/ci.yml']
        except (subprocess.SubprocessError, ValueError, KeyError, TypeError) as exc:
            evidence.update(wait_outcome='query_failed', error_type=type(exc).__name__)
            save()
            raise RuntimeError(f'Could not verify hosted Engine checks for {sha}: {type(exc).__name__}') from exc
        if matches:
            selected = max(matches, key=lambda item: (int(item['id']), int(item.get('run_attempt', 1))))
            evidence.update(id=selected['id'], url=selected['html_url'], status=selected['status'],
                            conclusion=selected.get('conclusion'), run_attempt=selected.get('run_attempt', 1))
            if selected['status'] == 'completed':
                evidence['wait_outcome'] = 'success' if selected.get('conclusion') == 'success' else 'failure'
                save()
                if evidence['wait_outcome'] != 'success':
                    raise RuntimeError(f'Hosted Engine checks concluded {selected.get("conclusion")} '
                                       f'for {sha}: {selected["html_url"]}')
                return evidence
        save()
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            expired()
        time.sleep(min(poll, remaining))


def verify_no_changes(config, work, sha, run, log):
    """Only generated review data may inherit a code ancestor's successful CI.

    Never infer verification from a commit message, author, or the AI's result.
    Walk single-parent commits, inspecting every changed path (including deletes).
    A merge, unknown path, or exhausted bound requires CI on that revision.
    """
    current, skipped, validators = sha, [], set()
    for _ in range(100):
        parents = git(work, 'rev-list', '--parents', '-n', '1', current).split()
        if len(parents) != 2:
            break
        names = git(work, 'diff-tree', '--no-commit-id', '--name-only', '-r',
                    '--no-renames', current).splitlines()
        if not names or not all(
            name == 'docs/player-feedback/summary.json'
            or (name.startswith('docs/player-feedback/reviews/') and name.endswith('.json'))
            or (name.startswith('docs/player-feedback/games/') and name.endswith('.json'))
            or (name.startswith('docs/community/events/') and name.endswith('.json'))
            for name in names
        ):
            break
        skipped.append(current)
        if any(name.startswith('docs/player-feedback/') for name in names):
            validators.add('player')
        if any(name.startswith('docs/community/events/') for name in names):
            validators.add('community')
        current = parents[1]
    if skipped:
        # Use the current checkout's validator; review bodies remain untrusted data.
        if 'player' in validators:
            command(['python3', 'tools/player_feedback.py', 'check'], cwd=work,
                    timeout=180, log=log)
        if 'community' in validators:
            command(['python3', 'tools/community_store.py', 'check', '--root', str(work)],
                    cwd=work, timeout=180, log=log)
    evidence = wait_for_hosted_ci(config, current, run, log)
    evidence.update(reviewed_sha=sha, feedback_only_commits=skipped,
                    policy='validated-feedback-data-on-verified-code')
    write_json(run / 'hosted-ci.json', evidence)
    return evidence


def feedback_path(name):
    path = Path(name)
    return (name.startswith('docs/community/events/') and path.suffix == '.json') or (
        'feedback' in name.lower() and path.suffix.lower() in {'.md', '.txt', '.json', '.jsonl'}) or name in {
        'docs/learning/ledger.jsonl', 'docs/learning/REPORT.md',
        'docs/learning/dupes.md', 'docs/learning/dupes.json'}


def feedback_signals(name, data):
    """Hash feedback content, excluding generated reports and administrative edits."""
    if name.startswith('docs/player-feedback/games/') or name in {
                'docs/player-feedback/summary.json', 'docs/learning/REPORT.md', 'docs/learning/dupes.md',
                'docs/learning/dupes.json', 'docs/AI_DEV_FEEDBACK.md'} or 'feedback_prompt' in name.lower():
        return []
    if name == 'docs/learning/ledger.jsonl':
        records = [json.loads(line) for line in data.decode().splitlines() if line.strip()]
        values = [{key: record.get(key) for key in ('game', 'area', 'note', 'trap', 'duplicated', 'tokens')}
                  for record in records]
    elif name.startswith('issues/'):
        issue = json.loads(data)
        values = [{'title': issue.get('title'), 'body': issue.get('body'),
                   'comments': [comment.get('body') for comment in issue.get('comments', [])],
                   'review_comments': [{'body': comment.get('body'), 'path': comment.get('path')}
                                       for comment in issue.get('review_comments', [])]}]
    elif name.startswith('docs/player-feedback/reviews/'):
        review = json.loads(data)
        # Names, timestamps and generated URLs are not new observations. Human
        # and AI reports of identical content share a topic without a trust boost.
        values = [{key: review.get(key) for key in
                   ('game', 'version', 'rating', 'enjoyed', 'disliked',
                    'scope', 'category', 'summary', 'description', 'reproduction', 'expected', 'actual')}]
    elif name.startswith('docs/community/events/'):
        event = json.loads(data)
        if event.get('event_type') == 'publication_requested':
            values = [{'event_type': event['event_type'], 'request': event.get('request')}]
        elif event.get('event_type') != 'feedback':
            return []
        else:
            values = [{key: event.get('feedback', {}).get(key) for key in
                       ('target_type', 'target', 'category', 'note', 'version')}]
    else:
        return [hashlib.sha256(data).hexdigest()]
    return [hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest() for value in values]


def feedback_observations(name, data):
    """Bounded discovery hints; the original snapshots remain the evidence."""
    signals = feedback_signals(name, data)
    if not signals:
        return []
    records = [None] * len(signals)
    if name == 'docs/learning/ledger.jsonl':
        records = [json.loads(line) for line in data.decode().splitlines() if line.strip()]
    elif name.startswith(('docs/player-feedback/reviews/', 'docs/community/events/', 'issues/')):
        records = [json.loads(data)]
    observations = []
    for signal, record in zip(signals, records):
        scope, game = 'engine', None
        if record is None:
            evidence = data.decode(errors='replace')[:1800]
        elif name == 'docs/learning/ledger.jsonl':
            game = record.get('game')
            evidence = '\n'.join(str(record.get(key) or '') for key in ('note', 'trap', 'workaround'))[:1800]
        elif name.startswith('docs/community/events/'):
            feedback = record.get('feedback', {})
            if record.get('event_type') == 'publication_requested':
                scope = 'game'
                game = record.get('request', {}).get('game')
                evidence = 'Publication requested: ' + json.dumps(record.get('request', {}), sort_keys=True)[:1600]
            else:
                scope, game = feedback.get('target_type'), feedback.get('target')
                evidence = str(feedback.get('note') or '')[:1800]
        elif name.startswith('docs/player-feedback/reviews/'):
            scope = record.get('scope') or 'game'
            game = record.get('game')
            evidence = '\n'.join(str(record.get(key) or '') for key in
                                 ('summary', 'description', 'enjoyed', 'disliked', 'reproduction', 'expected', 'actual'))[:1800]
        else:
            evidence = '\n'.join(str(record.get(key) or '') for key in ('title', 'body'))[:1000]
            comments = record.get('comments', []) + record.get('review_comments', [])
            evidence += '\n' + '\n'.join(str(item.get('body') or '') for item in comments[-4:])[-800:]
        if scope not in {'engine', 'game', 'launcher'}:
            scope = 'engine'
        # Transparent triage hints, never proof of severity or permission. No
        # reporter name/type, popularity or AI confidence enters this ordering.
        lower = evidence.lower()
        priority = 3 if any(word in lower for word in ('data loss', 'save corruption', 'security vulnerability')) else (
            2 if any(word in lower for word in ('crash', 'cannot launch', 'cannot publish', 'broken', 'panic')) else 1)
        observations.append({'feedback_id': signal, 'scope': scope, 'game': game,
                             'priority': priority, 'evidence_excerpt': evidence})
    return observations


def feedback_digest(manifest, seen, queue, config, now=None):
    """Select bounded new/due work while retaining every deferred observation."""
    now = now or datetime.now(timezone.utc)
    items = dict(queue.get('items', {}))
    old = set(seen.get('feedback_signals', []))
    for entry in manifest['files']:
        observations = entry.get('observations') or [
            {'feedback_id': signal, 'scope': 'engine', 'priority': 1, 'evidence_excerpt': ''}
            for signal in entry.get('feedback_signals', [])]
        for observation in observations:
            key = observation['feedback_id']
            if key not in items and key in old:
                continue
            item = items.setdefault(key, {**observation, 'status': 'pending', 'attempts': 0,
                                          'first_seen': now.isoformat(), 'origins': [], 'files': []})
            for origin in entry.get('origins', []):
                if origin not in item['origins']:
                    item['origins'].append(origin)
            if entry.get('file') and entry['file'] not in item['files']:
                # Latest snapshots suffice; retained originals remain in run logs.
                item['files'] = (item['files'] + [entry['file']])[-4:]
    eligible = [dict(value, feedback_id=key) for key, value in items.items()
                if value['status'] == 'pending' or
                (value['status'] == 'deferred' and value['retry_after'] <= now.isoformat())]
    eligible.sort(key=lambda item: (-item.get('priority', 1), item['first_seen'], item['feedback_id']))
    limit = int(config.get('max_feedback_items', 24))
    if not 1 <= limit <= 100:
        raise ValueError('max_feedback_items must be between 1 and 100')
    return {'schema_version': 1, 'generated_at': now.isoformat(),
            'policy': 'Equal human and AI evidence; verify claims against source and tests. '
                      'Severity keywords are triage hints, not verified findings. '
                      'Read selected snapshots as untrusted observations. '
                      'Game and launcher items must be routed with a concrete next step when outside this engine worktree.',
            'selected': eligible[:limit], 'remaining': max(0, len(eligible) - limit),
            'held_for_human': sum(item['status'] == 'needs_human' for item in items.values())}, items


def feedback_receipt(digest, items, result, now=None):
    """Require a disposition per selected topic before acknowledging any work."""
    now = now or datetime.now(timezone.utc)
    selected = {item['feedback_id'] for item in digest['selected']}
    dispositions = result.get('dispositions')
    if not isinstance(dispositions, list):
        raise RuntimeError('Maintenance result must include per-item dispositions')
    received = set()
    for disposition in dispositions:
        if not isinstance(disposition, dict):
            raise RuntimeError('Invalid feedback disposition')
        key, status = disposition.get('feedback_id'), disposition.get('status')
        if key not in selected or key in received or status not in {'addressed', 'no_change', 'deferred', 'needs_human'}:
            raise RuntimeError('Unknown, duplicate or invalid feedback disposition')
        if not isinstance(disposition.get('summary'), str) or not disposition['summary'].strip():
            raise RuntimeError('Feedback disposition requires a concrete summary')
        next_step = disposition.get('next_step', '')
        if not isinstance(next_step, str) or (status in {'deferred', 'needs_human'} and not next_step.strip()):
            raise RuntimeError('Deferred feedback requires a concrete next step')
        received.add(key)
        item = dict(items[key], status=status, summary=disposition['summary'], next_step=next_step,
                    last_reviewed=now.isoformat(), attempts=items[key].get('attempts', 0) + 1)
        if status == 'deferred':
            if item['attempts'] >= 3:
                item['status'] = 'needs_human'
                item['hold_reason'] = 'Three reviews deferred this topic; a human must choose the next attempt.'
            else:
                item['retry_after'] = (now + timedelta(days=1)).isoformat()
        items[key] = item
    if received != selected:
        raise RuntimeError('Maintenance omitted selected feedback dispositions')
    return {'schema_version': 1, 'updated_at': now.isoformat(), 'items': items}


def worktrees(repo):
    return [Path(line[9:]) for line in git(repo, 'worktree', 'list', '--porcelain').splitlines()
            if line.startswith('worktree ') and Path(line[9:]).is_dir()]


def collect_feedback(config, work, run, previous):
    folder = run / 'feedback'
    folder.mkdir()
    entries, dedup, warnings = [], {}, []
    old = set(previous.get('feedback_signals', []))

    def add(source, name, data, modified=None):
        digest = hashlib.sha256(data).hexdigest()
        key = (name, digest)
        origin = {'source': source, 'path': name, 'modified': modified}
        if key in dedup:
            dedup[key]['origins'].append(origin)
            return
        destination = folder / (f'{len(entries):04d}-' + Path(name).name)
        destination.write_bytes(data)
        try:
            signals = feedback_signals(name, data)
            observations = feedback_observations(name, data)
        except (ValueError, TypeError, KeyError, AttributeError) as exc:
            warnings.append(f'Malformed feedback omitted: {source}:{name} ({type(exc).__name__})')
            signals, observations = [], []
        entry = {'file': str(destination), 'sha256': digest, 'feedback_signals': signals,
                 'new_since_seen': any(signal not in old for signal in signals), 'origins': [origin],
                 'observations': observations}
        entries.append(entry)
        dedup[key] = entry

    # Current remote engine source first, then tracked/untracked feedback in all
    # existing engine/game worktrees, including unfinished development reports.
    private_roots = {(Path(config['state']) / 'runs').resolve()}
    for key in ('runs_root', 'worktrees_root'):
        if config.get(key):
            private_roots.add(Path(config[key]).resolve())
    current_work = work.resolve() if work else None
    engine_ref = git(config['engine'], 'rev-parse', 'FETCH_HEAD').strip()
    for name in git(config['engine'], 'ls-tree', '-r', '--name-only', engine_ref).splitlines():
        if feedback_path(name):
            add(f'{config["engine_repo"]}@{engine_ref}', name,
                git(config['engine'], 'show', f'{engine_ref}:{name}').encode())
    checkouts = ([work] if work else []) + worktrees(config['engine']) + worktrees(config['games'])
    for checkout in dict.fromkeys(checkouts):
        resolved = checkout.resolve()
        if resolved != current_work and any(resolved.is_relative_to(root) for root in private_roots):
            continue
        names = git(checkout, 'ls-files', '--cached', '--others', '--exclude-standard', '-z').split('\0')
        for name in sorted(set(filter(feedback_path, filter(None, names)))):
            path = checkout / name
            try:
                if path.is_symlink() or not path.is_file():
                    continue
                if path.stat().st_size > 2_000_000:
                    warnings.append(f'Oversized feedback file omitted: {path}')
                    continue
                add(str(checkout), name, path.read_bytes(),
                    datetime.fromtimestamp(path.stat().st_mtime, timezone.utc).isoformat())
            except OSError as exc:
                warnings.append(f'Could not read {path}: {exc}')

    # Read current GitHub games source without updating anyone's checkout.
    git(config['games'], 'fetch', 'origin', 'main', timeout=180)
    games_ref = git(config['games'], 'rev-parse', 'FETCH_HEAD').strip()
    for name in git(config['games'], 'ls-tree', '-r', '--name-only', games_ref).splitlines():
        if feedback_path(name):
            add(f'{config["games_repo"]}@{games_ref}', name,
                git(config['games'], 'show', f'{games_ref}:{name}').encode())

    since = previous.get('completed_at') or (datetime.now(timezone.utc) - timedelta(days=14)).isoformat()
    for repo in (config['engine_repo'], config['games_repo']):
        try:
            # Include all open issues/PRs plus recently updated closed ones.
            issues = {}
            for query in ('state=open', f'state=closed&since={since}'):
                raw = command(['gh', 'api', '--paginate', '--slurp',
                               f'repos/{repo}/issues?{query}&per_page=100'], timeout=180)
                for page in json.loads(raw):
                    for issue in page:
                        issues[issue['number']] = issue
            for issue in issues.values():
                item = {key: issue.get(key) for key in (
                    'number', 'title', 'body', 'state', 'html_url', 'updated_at', 'pull_request')}
                item['comments'] = []
                if issue.get('comments'):
                    pages = json.loads(command(['gh', 'api', '--paginate', '--slurp',
                                                f'repos/{repo}/issues/{issue["number"]}/comments?per_page=100'], timeout=180))
                    item['comments'] = [{key: comment.get(key) for key in ('body', 'html_url', 'updated_at')}
                                        for page in pages for comment in page]
                if issue.get('pull_request'):
                    pages = json.loads(command(['gh', 'api', '--paginate', '--slurp',
                                                f'repos/{repo}/pulls/{issue["number"]}/comments?per_page=100'], timeout=180))
                    item['review_comments'] = [{key: comment.get(key) for key in ('body', 'path', 'html_url', 'updated_at')}
                                              for page in pages for comment in page]
                add(f'GitHub {repo}', f'issues/{repo.replace("/", "-")}-{issue["number"]}.json',
                    json.dumps(item, indent=2).encode(), issue.get('updated_at'))
        except (subprocess.SubprocessError, ValueError) as exc:
            warnings.append(f'GitHub issue collection incomplete for {repo}: {type(exc).__name__}')
    manifest = {'generated_at': datetime.now(timezone.utc).isoformat(),
                'previous_observation': previous, 'warnings': warnings, 'files': entries}
    write_json(run / 'feedback-manifest.json', manifest)
    return manifest


def preflight(config):
    for name in ('git', 'gh', 'python3', 'cargo'):
        if not shutil.which(name):
            raise RuntimeError(f'Required executable missing: {name}')
    if not Path(config['codex']).is_file():
        raise RuntimeError('Codex executable is missing')
    help_text = command([config['codex'], 'exec', '--help'])
    if '--dangerously-bypass-approvals-and-sandbox' not in help_text:
        raise RuntimeError('Installed Codex lacks the requested YOLO mode')
    status = command([config['codex'], 'login', 'status'])
    if 'Logged in' not in status:
        raise RuntimeError('Codex is not logged in')
    details = json.loads(command(['gh', 'api', f'repos/{config["engine_repo"]}']))
    if not details.get('permissions', {}).get('push'):
        raise RuntimeError('GitHub account lacks push permission for BlueEngine')
    expected = f'https://github.com/{config["engine_repo"]}.git'
    if git(config['engine'], 'remote', 'get-url', '--push', 'origin').strip() != expected:
        raise RuntimeError('BlueEngine origin does not match the configured destination')
    command(['git', 'ls-remote', '--exit-code', expected, f'refs/heads/{config["branch"]}'])
    print(f'Preflight passed: Codex authenticated; GitHub push permission; {config["branch"]} exists.')


def main():
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--check', action='store_true', help='Check prerequisites; do not run maintenance')
    mode.add_argument('--collect-only', action='store_true', help='Exercise feedback collection without Codex or pushes')
    mode.add_argument('--baseline', action='store_true', help='Remember current feedback without running Codex')
    mode.add_argument('--retry-feedback', metavar='ID', help='Requeue one deferred/held topic; no agent or publication')
    args = parser.parse_args()
    config = json.loads((CONFIG_DIR / 'config.json').read_text())
    os.umask(0o077)
    os.environ['GIT_TERMINAL_PROMPT'] = '0'
    os.environ['GH_PROMPT_DISABLED'] = '1'
    state = Path(config['state'])
    state.mkdir(parents=True, exist_ok=True)
    with (state / 'run.lock').open('w') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            print('Another BlueEngine nightly run is active; skipping.')
            return 0
        if args.check:
            preflight(config)
            return 0
        if args.retry_feedback:
            queue_path = state / 'feedback-queue.json'
            if not queue_path.is_file():
                parser.error('No durable feedback queue exists yet')
            queue = json.loads(queue_path.read_text())
            item = queue.get('items', {}).get(args.retry_feedback)
            if not item or item['status'] not in {'deferred', 'needs_human'}:
                parser.error('--retry-feedback requires a known deferred or held feedback ID')
            item.setdefault('manual_retries', []).append({
                'at': datetime.now(timezone.utc).isoformat(), 'previous_attempts': item['attempts']})
            item.update(status='pending', attempts=0)
            write_json(queue_path, queue)
            print('Feedback requeued for the existing daily schedule; no agent or publication was run.')
            return 0
        stamp = datetime.now().astimezone().strftime('%Y-%m-%d_%H-%M-%S-%f')
        run = Path(config.get('runs_root') or state / 'runs') / stamp
        run.mkdir(parents=True)
        work = run / 'worktree'
        previous_path = state / 'last-success.json'
        previous = json.loads(previous_path.read_text()) if previous_path.exists() else {}
        seen_path = state / 'feedback-seen.json'
        seen = json.loads(seen_path.read_text()) if seen_path.exists() else {}
        queue_path = state / 'feedback-queue.json'
        queue = json.loads(queue_path.read_text()) if queue_path.exists() else {}
        result, commit, base, manifest, hosted_ci = {}, None, None, {}, None
        digest, receipt = {}, None
        publication_attempted, pushed_commit, confirmed_no_changes = False, None, False
        temp = None
        outcome, detail, exit_code = 'failed', '', 1
        with (run / 'runner.log').open('w') as log:
            try:
                git(config['engine'], 'fetch', 'origin', config['branch'], timeout=180, log=log)
                base = git(config['engine'], 'rev-parse', 'FETCH_HEAD').strip()
                manifest = collect_feedback(config, None, run, seen)
                print(f'Collected {len(manifest["files"])} feedback snapshots in {run}')
                observed = set(seen.get('feedback_signals', []))
                current = {signal for item in manifest['files'] for signal in item.get('feedback_signals', [])}
                digest, queue_items = feedback_digest(manifest, seen, queue, config)
                write_json(run / 'feedback-digest.json', digest)
                observation = {'completed_at': seen.get('completed_at'),
                               'feedback_signals': sorted(observed | current)}
                if not manifest['warnings']:
                    observation['completed_at'] = datetime.now(timezone.utc).isoformat()
                if args.collect_only:
                    outcome, detail, exit_code = 'collection-only', 'Feedback collection verified; Codex and publication were not run.', 0
                elif args.baseline:
                    if manifest['warnings']:
                        raise RuntimeError('Feedback collection incomplete; baseline not saved')
                    write_json(seen_path, observation)
                    result = {'summary': 'Current feedback saved as the baseline. Codex was not launched.'}
                    outcome, detail, exit_code = 'baseline', f'Remembered {len(current)} feedback fingerprints.', 0
                elif not digest['selected']:
                    write_json(seen_path, observation)
                    result = {'summary': 'No new or due feedback. Codex was not launched.'}
                    outcome, detail, exit_code = 'skipped', 'No engine edits, builds, commits, or pushes were performed.', 0
                else:
                    # Acknowledge feedback only after a successful result below.
                    # Failed or blocked attempts must remain retryable next night.
                    preflight(config)
                    if config.get('worktrees_root'):
                        # Keep authoring/check paths short independently of retained
                        # logs. Never reuse a failed run's private worktree directory.
                        work_root = Path(config['worktrees_root'])
                        work_root.mkdir(mode=0o700, parents=True, exist_ok=True)
                        work = Path(tempfile.mkdtemp(prefix='bn-', dir=work_root))
                    write_json(run / 'worktree.json', {'path': str(work)})
                    git(config['engine'], 'worktree', 'add', '--detach', work, base, log=log)
                    # Unix control sockets include TMPDIR in their pathname. Keep this
                    # private root short even when the retained run directory is deep.
                    temp = Path(tempfile.mkdtemp(prefix='be2n-', dir=os.environ.get('TMPDIR', '/tmp')))
                    os.environ['TMPDIR'] = str(temp)
                    os.environ['CARGO_TARGET_DIR'] = os.environ.get('CARGO_TARGET_DIR') or str(state / 'target')
                    prompt = (CONFIG_DIR / 'prompt.txt').read_text()
                    prompt += f'\nBASE: {base}\nFeedback manifest: {run / "feedback-manifest.json"}\n'
                    prompt += f'Feedback digest: {run / "feedback-digest.json"}\n'
                    prompt += ('Work only on selected digest topics, including due deferred topics. '
                               'Return dispositions for every selected feedback_id, with status addressed, no_change, deferred, or needs_human; '
                               'each requires summary and next_step (concrete and nonempty when deferred or needs_human). '
                               'Do not rank human or AI feedback by identity. Launcher/game feedback outside this engine checkout '
                               'requires a concrete routed next step; never claim it fixed by an engine-only patch.\n')
                    prompt += f'Last successful result: {json.dumps(previous.get("result", {}))}\n'
                    if (state / 'latest.md').exists():
                        prompt += f'Previous morning report (read-only): {state / "latest.md"}\n'
                    (run / 'prompt.txt').write_text(prompt)
                    decision_path = state / 'no-change-decision.json'
                    decision = json.loads(decision_path.read_text()) if decision_path.exists() else {}
                    selection = [[item['feedback_id'], item['attempts']] for item in digest['selected']]
                    if (decision.get('base') == base and decision.get('signals') == sorted(current)
                            and decision.get('selection') == selection):
                        result = decision['result']
                        log.write('Reusing clean no-change decision for identical revision and feedback; CI still required.\n')
                    else:
                        with (run / 'codex.jsonl').open('w') as events, (run / 'codex.stderr.log').open('w') as errors:
                            subprocess.run([config['codex'], 'exec', '--dangerously-bypass-approvals-and-sandbox',
                                            '--cd', str(work), '--color', 'never', '--json',
                                            '--output-schema', str(CONFIG_DIR / 'result.schema.json'),
                                            '--output-last-message', str(run / 'result.json'), '-'],
                                           input=prompt, text=True, stdout=events, stderr=errors,
                                           timeout=config['codex_timeout_seconds'], check=True)
                        result = json.loads((run / 'result.json').read_text())
                    if result.get('status') not in {'updated', 'no_changes', 'blocked'} or not isinstance(result.get('summary'), str):
                        raise RuntimeError('Codex returned an invalid maintenance result')
                    receipt = feedback_receipt(digest, queue_items, result)
                    if git(work, 'rev-parse', 'HEAD').strip() != base:
                        raise RuntimeError('Codex changed HEAD; automated publication stopped')
                    dirty = git(work, 'status', '--porcelain').strip()
                    if result['status'] == 'blocked':
                        raise RuntimeError('Codex reported incomplete or blocked work')
                    if result['status'] == 'no_changes':
                        if dirty:
                            raise RuntimeError('Codex reported no changes but left modified files')
                        confirmed_no_changes = True
                        write_json(decision_path, {'base': base, 'signals': sorted(current),
                                                   'selection': selection, 'result': result})
                        hosted_ci = verify_no_changes(config, work, base, run, log)
                        outcome, detail, exit_code = 'no_changes', 'No engine patch was needed; the maintenance harness did not push an engine change.', 0
                    else:
                        if not dirty:
                            raise RuntimeError('Codex reported an update but no source changes exist')
                        git(work, 'add', '-A', log=log)
                        git(work, 'diff', '--cached', '--check', log=log)
                        command(['python3', 'tools/be2.py', 'check', '--changed', '--base', base],
                                cwd=work, timeout=config['check_timeout_seconds'], log=log)
                        changed = git(work, 'diff', '--cached', '--name-only').splitlines()
                        if any(name.startswith(('games/', 'assets/games/')) for name in changed):
                            command(['python3', 'scripts/publish_games.py', 'check'], cwd=work, timeout=1800, log=log)
                        if git(work, 'diff', '--name-only').strip() or git(work, 'ls-files', '--others', '--exclude-standard').strip():
                            raise RuntimeError('Verification modified source files; retained for review without pushing')
                        git(work, 'diff', '--cached', '--check', log=log)
                        message = run / 'commit-message.txt'
                        message.write_text(f'Nightly engine maintenance ({stamp[:10]})\n\n{result["summary"]}\n')
                        git(work, 'commit', '-F', message, log=log)
                        commit = git(work, 'rev-parse', 'HEAD').strip()
                        # A concurrent change to main rejects this ordinary push.
                        # Never force-push or silently rebase already checked work.
                        publication_attempted = True
                        git(work, 'push', 'origin', f'HEAD:refs/heads/{config["branch"]}', timeout=180, log=log)
                        pushed_commit = commit
                        published = git(work, 'ls-remote', 'origin', f'refs/heads/{config["branch"]}').split()[0]
                        if published != commit:
                            git(work, 'fetch', 'origin', config['branch'], log=log)
                            git(work, 'merge-base', '--is-ancestor', commit, 'FETCH_HEAD')
                        hosted_ci = wait_for_hosted_ci(config, commit, run, log)
                        outcome, detail, exit_code = 'updated', f'Validated and pushed {commit} to {config["branch"]}.', 0
                    # Ignore feedback-document/ledger edits produced by maintenance
                    # itself so they cannot trigger a session the following night.
                    for name in git(work, 'ls-files').splitlines():
                        if feedback_path(name) and (work / name).is_file():
                            observation['feedback_signals'] = sorted(set(observation['feedback_signals']) |
                                                                     set(feedback_signals(name, (work / name).read_bytes())))
                    write_json(queue_path, receipt)
                    write_json(seen_path, observation)
                    write_json(previous_path, {'completed_at': datetime.now(timezone.utc).isoformat(),
                                              'commit': commit, 'base': base, 'result': result,
                                              'hosted_ci': hosted_ci,
                                              'feedback_hashes': [item['sha256'] for item in manifest['files']]})
            except Exception as exc:
                # Exception strings from command failures can contain paths but do
                # not contain captured output, authorization headers, or tokens.
                outcome, exit_code = 'failed', 1
                detail = f'{type(exc).__name__}: {exc}'
                log.write(detail + '\n')
            finally:
                if temp is not None:
                    shutil.rmtree(temp, ignore_errors=True)
                report = f'# BlueEngine nightly — {stamp}\n\nStatus: {outcome}\n\n'
                fallback = 'Feedback collection completed.' if args.collect_only else 'Maintenance did not complete.'
                report += result.get('summary', fallback) + '\n\n' + detail + '\n'
                if pushed_commit:
                    report += f'\nPushed commit: {pushed_commit}. Hosted CI evidence below determines acknowledgment.\n'
                ci_path = run / 'hosted-ci.json'
                if ci_path.exists():
                    evidence = json.loads(ci_path.read_text())
                    label = f'[{evidence["id"]}]({evidence["url"]})' if evidence.get('url') else 'No matching run found'
                    report += (f'\nHosted Engine checks: {label}; exact SHA {evidence["head_sha"]}; '
                               f'status {evidence["status"]}; conclusion {evidence.get("conclusion")}; '
                               f'wait {evidence["wait_outcome"]}.\n')
                if manifest.get('warnings'):
                    report += '\nFeedback collection warnings:\n\n' + ''.join(f'- {item}\n' for item in manifest['warnings'])
                if digest:
                    report += (f'\nFeedback triage: {len(digest["selected"])} selected; '
                               f'{digest["remaining"]} waiting; {digest["held_for_human"]} held for a human.\n')
                    report += f'\nDigest: {run / "feedback-digest.json"}\n'
                if receipt and exit_code == 0:
                    counts = {status: sum(item['status'] == status for item in receipt['items'].values())
                              for status in ('pending', 'deferred', 'needs_human', 'addressed', 'no_change')}
                    report += f'\nDurable feedback queue: {queue_path}; {json.dumps(counts, sort_keys=True)}.\n'
                    for item in receipt['items'].values():
                        if item['status'] in {'deferred', 'needs_human'}:
                            report += f'\n- {item["status"]}: {item.get("summary", "")}; next: {item.get("next_step", "")}\n'
                for key, label in [('feedback_addressed', 'Feedback addressed'), ('checks', 'Codex checks'), ('deferred', 'Deferred')]:
                    if result.get(key):
                        report += f'\n{label}:\n\n' + ''.join(f'- {item}\n' for item in result[key])
                report += f'\nLogs and feedback: {run}\n'
                if outcome == 'failed' and work.exists() and not confirmed_no_changes:
                    label = ('Pushed worktree retained for review' if pushed_commit else
                             'Unchanged worktree retained for review' if confirmed_no_changes else
                             'Worktree retained; push was not confirmed' if publication_attempted else
                             'Unpublished work retained')
                    report += f'\n{label}: {work}\n'
                if confirmed_no_changes:
                    report += '\nNo source work retained: clean no-change decision is cached; retries only recheck CI.\n'
                (run / 'report.md').write_text(report)
                if not args.collect_only:
                    latest = state / 'latest.md'
                    temporary = state / 'latest.md.tmp'
                    temporary.write_text(report)
                    temporary.replace(latest)
                if (exit_code == 0 or confirmed_no_changes) and work.exists():
                    try:
                        git(config['engine'], 'worktree', 'remove', work, timeout=180, log=log)
                    except Exception as exc:
                        log.write(f'Cleanup incomplete: {exc}\n')
                print(report)
        return exit_code


if __name__ == '__main__':
    def interrupted(signum, frame):
        raise RuntimeError(f'Maintenance interrupted by signal {signum}; no further publication')
    signal.signal(signal.SIGTERM, interrupted)
    sys.exit(main())
