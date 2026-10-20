"""Publish scoped nightly companion fixes from a second private worktree."""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
import tomllib
from urllib.request import Request, urlopen
from uuid import uuid4

SHARED_LAUNCHER = {'site/feedback.mjs', 'site/feedback.css', 'site/collaboration.mjs',
                   'site/collaboration.css', 'site/launcher_feed.py'}
VERSIONS = ('launcher/package.json', 'launcher/package-lock.json', 'launcher/src-tauri/Cargo.toml',
            'launcher/src-tauri/Cargo.lock', 'launcher/src-tauri/tauri.conf.json')
AUTOMATION = {'launcher/publish.py', 'launcher/build.mjs', 'launcher/prepare-upgrade-user.ps1',
              'launcher/smoke-windows.mjs', 'launcher/smoke-upgrade.mjs', 'launcher/smoke-upgrade.ps1',
              'launcher/src-tauri/build.rs', 'launcher/src-tauri/tauri.ci.conf.json',
              'site/build.py', 'site/check.py', 'site/launcher_feed.py', 'site/released_games.py'}


def numeric_version(value):
    if not isinstance(value, str) or not re.fullmatch(r'\d+\.\d+\.\d+', value):
        raise RuntimeError('A stable numeric MAJOR.MINOR.PATCH version is required')
    return tuple(map(int, value.split('.')))


def launcher_versions(root):
    package, npm_lock = [json.loads((root / path).read_text()) for path in VERSIONS[:2]]
    cargo, cargo_lock = [tomllib.loads((root / path).read_text()) for path in VERSIONS[2:4]]
    tauri = json.loads((root / VERSIONS[4]).read_text())
    native = [item['version'] for item in cargo_lock['package'] if item['name'] == 'blueengine-games-launcher']
    values = [package['version'], npm_lock['version'], npm_lock['packages']['']['version'], cargo['package']['version'], tauri['version'], *native]
    if len(native) != 1 or len(set(values)) != 1:
        raise RuntimeError('All five launcher version files, including both npm lock versions, must match')
    numeric_version(values[0])
    return values[0]


class Maintenance:
    def __init__(self, config, run, work, base, catalog, definitions, *, command, git, write_json, log):
        self.config, self.run, self.work, self.base = config, run, work, base
        self.command, self.git, self.write_json, self.log = command, git, write_json, log
        self.definitions = {item['slug']: item for item in definitions.get('native_playables', [])}
        preserved = catalog.get('preserved_paths', [])
        self.games = {slug for slug in self.definitions if any(
            path == 'games/' + slug or path.startswith('games/' + slug + '/') for path in preserved)}
        self.state = {'base': base, 'worktree': str(work), 'status': 'prepared', 'checks': [], 'ci': {},
                      'branch': 'nightly-companion/' + run.name, 'games': {}, 'commit': None}
        self.save()

    def save(self):
        self.write_json(self.run / 'companion-receipt.json', self.state)

    @classmethod
    def prepare(cls, config, run, digest, *, command, git, write_json, log, engine_work=None):
        if not any(item.get('scope') in {'launcher', 'game'} for item in digest['selected']):
            return None
        source = Path(config['games'])
        if git(source, 'remote', 'get-url', '--push', 'origin').strip() != 'https://github.com/' + config['games_repo'] + '.git':
            raise RuntimeError('Companion origin does not match the configured publication destination')
        command(['git', '-C', str(source), 'fetch', 'origin', 'main'], timeout=180, log=log)
        base = git(source, 'rev-parse', 'FETCH_HEAD').strip()
        catalog = json.loads(git(source, 'show', base + ':.games-catalog.json'))
        definitions = json.loads(git(source, 'show', base + ':.release-games.json'))
        if config.get('worktrees_root'):
            work = Path(tempfile.mkdtemp(prefix='bc-', dir=config['worktrees_root']))
        else:
            work = run / 'companion-worktree'
        git(source, 'worktree', 'add', '--detach', str(work), base, log=log)
        instance = cls(config, run, work, base, catalog, definitions, command=command, git=git, write_json=write_json, log=log)
        instance.games.intersection_update(item.get('game') for item in digest['selected'] if item.get('scope') == 'game')
        instance.state['game_baselines'], instance.state['blocked_games'] = {}, {}
        for slug in sorted(instance.games):
            try:
                game = instance.json_command([sys.executable, str(Path(engine_work or config['engine']) / 'tools/community.py'), 'show', slug])
                game = game.get('game', game)
                revision = next(item for item in game['revisions'] if item['id'] == game['head_revision'])
                source_pin = revision['source']
                if source_pin['repository'] != config['games_repo'] or source_pin['game_path'] != 'games/' + slug:
                    raise RuntimeError('Creator head is not the trusted companion game source; integrate its reviewed revision first')
                git(source, 'fetch', 'origin', source_pin['commit'], timeout=180, log=log)
                if git(source, 'rev-parse', source_pin['commit'] + ':games/' + slug).strip() != git(work, 'rev-parse', base + ':games/' + slug).strip():
                    raise RuntimeError('Creator head has edits absent from main; integrate that exact revision before corrective maintenance')
                instance.state['game_baselines'][slug] = {'expected_revision': game['head_revision'], 'source': source_pin}
            except (KeyError, StopIteration, RuntimeError, subprocess.CalledProcessError) as exc:
                instance.state['blocked_games'][slug] = str(exc)
        instance.games.difference_update(instance.state['blocked_games'])
        instance.save()
        if not instance.state['blocked_games'] and not any(item.get('scope') == 'launcher' or item.get('game') in instance.games for item in digest['selected']):
            git(source, 'worktree', 'remove', str(work), timeout=180, log=log)
            return None
        return instance

    def changes(self):
        if self.git(self.work, 'rev-parse', 'HEAD').strip() != self.base:
            raise RuntimeError('AI changed companion HEAD; automated publication stopped')
        paths = set(self.git(self.work, 'diff', '--name-only', '--no-renames', self.base).splitlines())
        paths.update(self.git(self.work, 'ls-files', '--others', '--exclude-standard').splitlines())
        for path in paths:
            relative = Path(path)
            blocked = relative.suffix.lower() in {'.key', '.pem', '.p12'} or relative.name.startswith('.env')
            blocked = blocked or path in AUTOMATION or (path.startswith('distribution/') and relative.suffix.lower() in {'.py', '.ps1'})
            blocked = blocked or path.startswith('launcher/src-tauri/examples/')
            allowed = path.startswith(('launcher/', 'site/', 'distribution/')) or any(path.startswith('games/' + slug + '/') for slug in self.games)
            if (not allowed or blocked or path in {'site/launcher-release.json', 'site/launcher-update.json'}
                    or any((self.work / parent).is_symlink() for parent in [relative, *relative.parents] if str(parent) != '.')):
                raise RuntimeError('Companion edits escaped launcher/site/distribution or companion-owned game source; publication stopped')
        return sorted(paths)

    def validate_versions(self, paths):
        launcher = any(path.startswith('launcher/') or path in SHARED_LAUNCHER for path in paths)
        if launcher:
            version = launcher_versions(self.work)
            old = json.loads(self.git(self.work, 'show', self.base + ':launcher/package.json'))['version']
            if numeric_version(version) <= numeric_version(old):
                raise RuntimeError('Launcher fixes require a consistent version increase in all five version files')
            before = json.loads(self.git(self.work, 'show', self.base + ':launcher/src-tauri/tauri.conf.json'))
            after = json.loads((self.work / 'launcher/src-tauri/tauri.conf.json').read_text())
            if before.get('plugins', {}).get('updater') != after.get('plugins', {}).get('updater'):
                raise RuntimeError('Nightly maintenance cannot change updater signing trust or endpoints')
            old_package = json.loads(self.git(self.work, 'show', self.base + ':launcher/package.json'))
            if old_package.get('scripts') != json.loads((self.work / 'launcher/package.json').read_text()).get('scripts'):
                raise RuntimeError('Nightly maintenance cannot change launcher build or test commands')
            self.state['launcher_version'] = version
        changed_games = sorted({path.split('/')[1] for path in paths if path.startswith('games/')})
        for slug in changed_games:
            if slug not in self.state.get('game_baselines', {}):
                raise RuntimeError('Changed game lacks the creator head captured before authoring')
            path = self.work / 'games' / slug / 'Cargo.toml'
            after = tomllib.loads(path.read_text())['package']['version']
            before = tomllib.loads(self.git(self.work, 'show', self.base + ':games/' + slug + '/Cargo.toml'))['package']['version']
            if numeric_version(after) <= numeric_version(before):
                raise RuntimeError('Each changed companion game needs a Cargo package version increase')
        self.state['changed_games'] = changed_games
        return launcher, changed_games

    def check(self, argv, label, *, cwd=None):
        self.command(argv, cwd=cwd or self.work, timeout=self.config['check_timeout_seconds'], log=self.log)
        self.state['checks'].append(label)
        self.save()

    def integrate_main(self):
        """Integrate the engine's ordinary export before reviewing companion edits."""
        paths = self.changes()
        self.git(self.work, 'fetch', 'origin', 'main', timeout=180, log=self.log)
        latest = self.git(self.work, 'rev-parse', 'FETCH_HEAD').strip()
        if latest == self.base:
            return
        self.git(self.work, 'merge-base', '--is-ancestor', self.base, latest)
        games = {path.split('/')[1] for path in paths if path.startswith('games/')}
        for slug in games:
            if self.git(self.work, 'rev-parse', self.base + ':games/' + slug).strip() != self.git(self.work, 'rev-parse', latest + ':games/' + slug).strip():
                raise RuntimeError('Companion game source advanced during authoring; retained edits require explicit integration')
        self.git(self.work, 'add', '-A', log=self.log)
        patch = self.run / 'companion-source.patch'
        patch.write_text(self.git(self.work, 'diff', '--cached', '--binary', self.base).rstrip('\n') + '\n')
        self.state['integration'] = {'original_base': self.base, 'new_base': latest, 'patch': str(patch)}
        self.save()
        self.git(self.work, 'reset', '--hard', self.base, log=self.log)
        self.git(self.work, 'checkout', '--detach', latest, log=self.log)
        self.git(self.work, 'apply', '--index', '--3way', str(patch), log=self.log)
        self.git(self.work, 'reset', log=self.log)
        self.base = latest
        self.state['base'] = latest
        self.save()

    def wait_ci(self, workflow, branch, sha, label, *, fields=None, events=None, after_id=0,
                expected_id=None, min_attempt=1):
        if fields is not None:
            argv = ['gh', 'workflow', 'run', workflow, '--repo', self.config['games_repo'], '--ref', branch]
            for key, value in fields.items():
                argv.extend(['-f', key + '=' + value])
            self.command(argv, timeout=180, log=self.log)
        timeout = int(self.config.get('companion_ci_timeout_seconds', self.config.get('ci_timeout_seconds', 5400)))
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            endpoint = f'repos/{self.config["games_repo"]}/actions/workflows/{workflow}/runs?branch={branch}&per_page=100'
            data = json.loads(self.command(['gh', 'api', endpoint], timeout=120))
            matches = [item for item in data['workflow_runs'] if item.get('head_sha') == sha
                       and item.get('head_branch') == branch and item.get('event') in (events or {'workflow_dispatch'})
                       and item.get('path', '').split('@', 1)[0] == '.github/workflows/' + workflow and item['id'] > after_id
                       and (expected_id is None or item['id'] == expected_id) and item.get('run_attempt', 1) >= min_attempt]
            if matches:
                item = max(matches, key=lambda value: (value['id'], value.get('run_attempt', 1)))
                if item['status'] == 'completed':
                    evidence = {key: item.get(key) for key in ('id', 'head_sha', 'head_branch', 'event', 'path', 'conclusion', 'html_url', 'run_attempt')}
                    self.state['ci'][label] = evidence
                    self.save()
                    if item['conclusion'] != 'success':
                        raise RuntimeError(f'Companion {label} did not pass: {item["html_url"]}')
                    return evidence
            time.sleep(min(int(self.config.get('ci_poll_seconds', 30)), max(0, deadline - time.monotonic())))
        raise TimeoutError(f'Companion {label} lacks exact-source successful CI; work retained')

    def json_command(self, argv):
        return json.loads(self.command(argv, cwd=self.work, timeout=self.config['check_timeout_seconds']))

    def launcher_metadata(self, version):
        # Publishing/signing helpers cannot be edited by the nightly worker.
        spec = importlib.util.spec_from_file_location('trusted_launcher_publish', self.work / 'launcher/publish.py')
        publisher = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(publisher)
        github = publisher.GitHub(self.config['games_repo'])
        release = github.release('launcher-v' + version)
        source = self.state['ci']['launcher-production']['head_sha']
        with tempfile.TemporaryDirectory(prefix='nightly-launcher-proof-') as temporary:
            directory = Path(temporary)
            config = directory / 'tauri.conf.json'
            config.write_text(self.git(self.work, 'show', source + ':launcher/src-tauri/tauri.conf.json'))
            metadata = publisher.verify_release(github, release, source, version, directory,
                lambda exe, signature, signed_version: publisher.verify_signature(self.work, config, exe, signature, signed_version))
        return metadata, release

    def verify_launcher_feed(self, version):
        repo = self.config['games_repo']
        verified, release = self.launcher_metadata(version)
        owner, name = repo.split('/')
        public = f'https://{owner}.github.io/{name}/'

        def read(url, limit=1_000_000):
            with urlopen(Request(url, headers={'User-Agent': 'BlueEngine-nightly', 'Cache-Control': 'no-cache'}), timeout=60) as response:
                data = response.read(limit + 1)
            if len(data) > limit:
                raise RuntimeError('Launcher publication metadata exceeded its size bound')
            return data

        deadline = time.monotonic() + min(600, int(self.config.get('ci_timeout_seconds', 5400)))
        while True:
            try:
                metadata, update = [json.loads(read(public + path)) for path in ('launcher-release.json', 'launcher-update.json')]
                if metadata != verified or update != verified['update']:
                    raise RuntimeError('The live signed launcher feed does not match the immutable installer')
                self.state['launcher_publication'] = {'version': version, 'release': release['html_url'],
                    'source_commit': self.state['ci']['launcher-production']['head_sha'],
                    'sha256': verified['sha256'], 'size': verified['size'], 'feed': public + 'launcher-update.json'}
                self.save()
                return
            except (OSError, ValueError, KeyError, RuntimeError):
                if time.monotonic() >= deadline:
                    raise
                time.sleep(min(15, max(0, deadline - time.monotonic())))

    def publish(self, engine_work, summary):
        paths = self.changes()
        if not paths:
            return None
        self.integrate_main()
        paths = self.changes()
        content = {path: hashlib.sha256((self.work / path).read_bytes()).hexdigest()
                   if (self.work / path).is_file() else None for path in paths}
        launcher, games = self.validate_versions(paths)
        self.state['paths'] = paths
        self.check([sys.executable, '-m', 'unittest', 'discover', '-s', 'site', '-p', 'test_*.py'], 'site-tests')
        self.check([sys.executable, '-m', 'unittest', 'discover', '-s', 'distribution', '-p', 'test_*.py'], 'distribution-tests')
        self.check([sys.executable, 'distribution/check_catalog.py'], 'catalog-check')
        self.check(['node', '--test', 'site/feedback.test.mjs', 'site/collaboration.test.mjs'], 'community-js')
        if launcher:
            for argv, label in ((['npm', 'ci'], 'launcher-dependencies'), (['npm', 'test'], 'launcher-tests'),
                                (['npm', 'run', 'build'], 'launcher-build'),
                                (['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--no-default-features', '--locked'], 'launcher-core')):
                self.check(argv, label, cwd=self.work / 'launcher')
        after = {path: hashlib.sha256((self.work / path).read_bytes()).hexdigest()
                 if (self.work / path).is_file() else None for path in self.changes()}
        if after != content:
            raise RuntimeError('Companion checks changed edited source content; preserved without publication')
        self.git(self.work, 'add', '-A', log=self.log)
        self.git(self.work, 'diff', '--cached', '--check', log=self.log)
        message = self.run / 'companion-commit-message.txt'
        message.write_text('Nightly collaborative game and launcher maintenance\n\n' + summary + '\n')
        self.git(self.work, 'commit', '-F', str(message), log=self.log)
        sha = self.git(self.work, 'rev-parse', 'HEAD').strip()
        self.state.update(commit=sha, status='reviewing')
        self.save()
        self.git(self.work, 'push', 'origin', 'HEAD:refs/heads/' + self.state['branch'], timeout=180, log=self.log)
        self.wait_ci('releases.yml', self.state['branch'], sha, 'distribution-review', fields={'checks_only': 'true'})
        if launcher:
            self.wait_ci('launcher.yml', self.state['branch'], sha, 'launcher-review', fields={'publish': 'false'})
        for slug in games:
            receipt = self.run / (slug + '-native-review.json')
            self.check([sys.executable, str(Path(__file__).resolve()), 'review-game', '--engine-root', str(engine_work),
                        '--games-root', str(self.work), '--slug', slug, '--runs-root', str(self.run / 'game-releases'),
                        '--receipt', str(receipt)], slug + '-native-review')
            self.state['games'][slug] = json.loads(receipt.read_text())
            self.save()
        self.git(self.work, 'fetch', 'origin', 'main', timeout=180, log=self.log)
        if self.git(self.work, 'rev-parse', 'FETCH_HEAD').strip() != self.base:
            raise RuntimeError('Companion main advanced during review; exact checked work is retained without a force push')
        # Persist before contacting the remote: an accepted push and lost network
        # acknowledgment must retain the no-AI recovery boundary even offline.
        self.state['status'] = 'publication-intent'
        self.save()
        self.git(self.work, 'push', 'origin', sha + ':main', timeout=180, log=self.log)
        self.state['status'] = 'pushed-awaiting-publication'
        self.save()
        return self.finish_publication(engine_work)

    def finish_publication(self, engine_work):
        """Resume only the immutable source already reviewed and published to main."""
        sha = self.state['commit']
        if self.git(self.work, 'rev-parse', 'HEAD').strip() != sha or self.git(self.work, 'status', '--porcelain').strip():
            raise RuntimeError('Retained companion publication source changed; prior checks cannot authorize it')
        self.git(self.work, 'fetch', 'origin', 'main', timeout=180, log=self.log)
        if self.state['status'] == 'publication-intent':
            if self.git(self.work, 'rev-parse', 'FETCH_HEAD').strip() == self.base:
                self.git(self.work, 'push', 'origin', sha + ':main', timeout=180, log=self.log)
                self.git(self.work, 'fetch', 'origin', 'main', timeout=180, log=self.log)
            self.git(self.work, 'merge-base', '--is-ancestor', sha, 'FETCH_HEAD')
            self.state['status'] = 'pushed-awaiting-publication'
            self.save()
        self.git(self.work, 'merge-base', '--is-ancestor', sha, 'FETCH_HEAD')
        if self.state['status'] == 'verified':
            return self.state
        paths, games = self.state['paths'], self.state['changed_games']
        launcher = bool(self.state.get('launcher_version'))
        for slug in games:
            if self.state['games'][slug].get('community_revision'):
                continue
            game_run = self.state['games'][slug]['run']
            self.check([sys.executable, str(Path(__file__).resolve()), 'finish-game', '--directory', game_run,
                        '--engine-root', str(engine_work)], slug + '-public-release')
            state = json.loads((Path(game_run) / 'state.json').read_text())
            version = tomllib.loads((self.work / 'games' / slug / 'Cargo.toml').read_text())['package']['version']
            cli = str(Path(engine_work) / 'tools/community.py')
            game = self.json_command([sys.executable, cli, 'show', slug])
            game = game.get('game', game)
            pending = self.state['games'][slug].setdefault('lineage', {'request_id': str(uuid4()),
                'expected_revision': self.state['game_baselines'][slug]['expected_revision']})
            self.save()
            head = next(item for item in game['revisions'] if item['id'] == game['head_revision'])
            source = head['source']
            delivered = False
            if source['repository'] == self.config['games_repo'] and source['game_path'] == 'games/' + slug:
                self.git(self.work, 'fetch', 'origin', source['commit'], timeout=180, log=self.log)
                delivered = (self.git(self.work, 'rev-parse', source['commit'] + ':games/' + slug).strip()
                             == self.git(self.work, 'rev-parse', sha + ':games/' + slug).strip())
            if delivered:
                revision = head['id']
            else:
                if game['head_revision'] != pending['expected_revision']:
                    raise RuntimeError('Creator head advanced during maintenance; retained delivery cannot overwrite another revision')
                event = self.json_command([sys.executable, cli, '--request-id', pending['request_id'], 'submit', slug,
                                          '--commit', sha, '--source-repository', self.config['games_repo'], '--source-game', slug,
                                          '--expected-revision', pending['expected_revision'], '--version', version,
                                          '--note', 'Validated corrective update from daily collaborative feedback.'])
                revision = event.get('event', event).get('revision', event.get('revision', {}))['id']
            self.json_command([sys.executable, cli, 'record-release', game_run, '--revision', revision])
            self.state['games'][slug].update(publication=state['publication'], community_revision=revision)
            self.save()
        if launcher:
            prior = self.state['ci'].get('launcher-production')
            if prior and prior.get('conclusion') != 'success':
                self.command(['gh', 'run', 'rerun', str(prior['id']), '--repo', self.config['games_repo']], timeout=180, log=self.log)
                self.wait_ci('launcher.yml', 'main', prior['head_sha'], 'launcher-production',
                             expected_id=prior['id'], min_attempt=(prior.get('run_attempt') or 1) + 1)
            elif not prior:
                publication_head = self.git(self.work, 'rev-parse', 'FETCH_HEAD').strip()
                if publication_head != sha:
                    changed = self.git(self.work, 'diff', '--name-only', sha, publication_head).splitlines()
                    if any(path.startswith(('launcher/', '.github/workflows/launcher')) or path in SHARED_LAUNCHER for path in changed):
                        raise RuntimeError('Launcher inputs advanced after review; preserve the candidate for a fresh exact-source review')
                before = json.loads(self.command(['gh', 'api', f'repos/{self.config["games_repo"]}/actions/workflows/launcher.yml/runs?branch=main&per_page=1']))
                after_id = max((item['id'] for item in before['workflow_runs']), default=0)
                self.wait_ci('launcher.yml', 'main', publication_head, 'launcher-production', fields={'publish': 'true'}, after_id=after_id)
            self.verify_launcher_feed(self.state['launcher_version'])
        elif not games:
            if any(path.startswith('distribution/') for path in paths):
                self.wait_ci('releases.yml', 'main', sha, 'distribution-production', events={'push'})
            self.wait_ci('pages.yml', 'main', sha, 'site-production', events={'push', 'workflow_run'})
        self.state['status'] = 'verified'
        self.save()
        return self.state

    @classmethod
    def resume(cls, config, run, *, command, git, write_json, log):
        state = json.loads((run / 'companion-receipt.json').read_text())
        if state.get('status') not in {'publication-intent', 'pushed-awaiting-publication', 'verified'} or not state.get('commit'):
            raise RuntimeError('Only an already reviewed companion main publication can resume without AI')
        instance = cls.__new__(cls)
        instance.config, instance.run, instance.work, instance.base = config, run, Path(state['worktree']), state['base']
        instance.command, instance.git, instance.write_json, instance.log = command, git, write_json, log
        instance.state, instance.games, instance.definitions = state, set(state['changed_games']), {}
        return instance

    def cleanup(self):
        self.git(self.config['games'], 'worktree', 'remove', str(self.work), timeout=180, log=self.log)


def native_phase(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument('phase', choices=('review-game', 'finish-game'))
    parser.add_argument('--engine-root', type=Path, required=True)
    parser.add_argument('--games-root', type=Path)
    parser.add_argument('--slug')
    parser.add_argument('--runs-root', type=Path)
    parser.add_argument('--receipt', type=Path)
    parser.add_argument('--directory', type=Path)
    args = parser.parse_args(argv)
    sys.path.insert(0, str(args.engine_root))
    from tools import community_release_companion
    if args.phase == 'finish-game':
        community_release_companion.resume(args.directory)
        return
    release = community_release_companion.create(args.engine_root, args.games_root, args.slug, runs_root=args.runs_root)
    args.receipt.write_text(json.dumps({'run': str(release.directory)}) + '\n')
    with community_release_companion.forge.run_lock(release.directory):
        for phase, operation in (('prepare', release.prepare), ('local', release.verify_local),
                                 ('native', release.native_checks), ('installer', release.installer_review)):
            operation()
            release.state['completed'].append(phase)
            release.save()
        release.state['status'] = 'installer-reviewed'
        release.save()


if __name__ == '__main__':
    native_phase()
