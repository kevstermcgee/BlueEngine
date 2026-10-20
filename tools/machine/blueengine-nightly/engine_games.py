"""Retain engine-owned game delivery until native releases and lineage are proven."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tomllib
from uuid import uuid4


class Delivery:
    def __init__(self, config, run, state, *, command, git, write_json, log):
        self.config, self.run, self.state = config, run, state
        self.command, self.git, self.write_json, self.log = command, git, write_json, log

    def save(self):
        self.write_json(self.run / 'engine-games-receipt.json', self.state)

    def api(self, work, *arguments):
        return json.loads(self.command([sys.executable, str(Path(work) / 'tools/community.py'), *arguments],
                                       timeout=self.config['check_timeout_seconds']))

    @classmethod
    def prepare(cls, config, run, digest, work, **callbacks):
        selected = {item.get('game') for item in digest['selected']}
        games = {slug for slug in selected if slug and (Path(work) / 'games' / slug / 'game.project.json').is_file()}
        registry = Path(work) / 'games-publish.json'
        preserved = json.loads(registry.read_text()).get('preserve', []) if registry.is_file() else []
        games = {slug for slug in games if not any(Path(value) == Path('games', slug)
            or Path(value) in Path('games', slug).parents or Path('games', slug) in Path(value).parents for value in preserved)}
        if not games:
            return None
        instance = cls(config, run, {'status': 'prepared', 'baselines': {}, 'blocked_games': {}, 'games': {},
                                    'selected_games': sorted(games), 'wanted': [], 'source_commit': None}, **callbacks)
        base = instance.git(work, 'rev-parse', 'HEAD').strip()
        for slug in sorted(games):
            try:
                game = instance.api(work, 'show', slug)
                game = game.get('game', game)
                revision = next(item for item in game['revisions'] if item['id'] == game['head_revision'])
                source = revision['source']
                if source['repository'] != config['engine_repo'] or source['game_path'] != 'games/' + slug:
                    raise RuntimeError('Creator source is not the trusted engine game; integrate its reviewed source first')
                instance.git(work, 'fetch', 'origin', source['commit'], timeout=180, log=instance.log)
                if instance.git(work, 'rev-parse', source['commit'] + ':games/' + slug).strip() != instance.git(work, 'rev-parse', base + ':games/' + slug).strip():
                    raise RuntimeError('Creator head contains edits absent from engine main; integrate that exact revision first')
                instance.state['baselines'][slug] = {'expected_revision': game['head_revision'], 'source': source}
            except (KeyError, StopIteration, RuntimeError, subprocess.CalledProcessError) as exc:
                instance.state['blocked_games'][slug] = str(exc)
        instance.save()
        return instance

    def select(self, digest, result, changed):
        dispositions = {item['feedback_id']: item['status'] for item in result['dispositions']}
        wanted = {item['game'] for item in digest['selected'] if item.get('game') in self.state['selected_games']
                  and dispositions[item['feedback_id']] == 'addressed'}
        wanted.update(path.split('/')[1] for path in changed if path.startswith('games/') and len(path.split('/')) > 2)
        if wanted - self.state['baselines'].keys():
            raise RuntimeError('Engine game changes lack a pre-authoring creator baseline; retain them for source integration')
        self.state['wanted'] = sorted(wanted)
        self.save()

    def intend(self, source):
        if self.state['wanted']:
            self.state.update(status='delivery-pending', source_commit=source)
            self.save()

    def deliver(self, work):
        if not self.state['wanted']:
            return None
        source = self.state['source_commit']
        if self.git(work, 'rev-parse', 'HEAD').strip() != source or self.git(work, 'status', '--porcelain').strip():
            raise RuntimeError('Engine game release tooling/source changed since the immutable delivery intent')
        self.git(work, 'fetch', 'origin', 'main', timeout=180, log=self.log)
        self.git(work, 'merge-base', '--is-ancestor', source, 'FETCH_HEAD')
        for slug in self.state['wanted']:
            record = self.state['games'].setdefault(slug, {})
            if record.get('community_receipt'):
                continue
            if not record.get('revision_id'):
                record['revision_id'] = self.register_revision(work, slug, record)
                self.save()
            if not record.get('run'):
                receipt = self.run / (slug + '-engine-release.json')
                self.command([sys.executable, str(Path(__file__).resolve()), 'create', '--engine-root', str(work),
                              '--games-root', self.config['games'], '--slug', slug,
                              '--runs-root', str(self.run / 'engine-game-releases'), '--receipt', str(receipt)],
                             timeout=self.config['check_timeout_seconds'], log=self.log)
                record.update(json.loads(receipt.read_text()))
                self.save()
            release_path = Path(record['run']) / 'state.json'
            existing = json.loads(release_path.read_text())
            if existing.get('status') != 'published':
                self.command([sys.executable, str(Path(__file__).resolve()), 'review', '--engine-root', str(work),
                              '--directory', record['run']], timeout=self.config['check_timeout_seconds'], log=self.log)
                current = self.api(work, 'show', slug)
                current = current.get('game', current)
                if current['head_revision'] != record['revision_id']:
                    raise RuntimeError('Creator head advanced during engine game review; retain its exact reviewed source without publishing')
                self.command([sys.executable, str(Path(__file__).resolve()), 'resume', '--engine-root', str(work),
                              '--directory', record['run']], timeout=self.config['check_timeout_seconds'], log=self.log)
            release = json.loads((Path(record['run']) / 'state.json').read_text())
            if release.get('status') != 'published' or not release.get('publication'):
                raise RuntimeError('Engine game lacks its native installer/public receipt')
            receipt = self.api(work, 'record-release', record['run'], '--revision', record['revision_id'])
            record.update(publication=release['publication'], community_revision=record['revision_id'], community_receipt=receipt)
            self.save()
        self.state['status'] = 'verified'
        self.save()
        return self.state

    def register_revision(self, work, slug, record):
        source = self.state['source_commit']
        game = self.api(work, 'show', slug)
        game = game.get('game', game)
        current = next(item for item in game['revisions'] if item['id'] == game['head_revision'])
        pin = current['source']
        delivered = False
        if pin['repository'] == self.config['engine_repo'] and pin['game_path'] == 'games/' + slug:
            self.git(work, 'fetch', 'origin', pin['commit'], timeout=180, log=self.log)
            delivered = self.git(work, 'rev-parse', pin['commit'] + ':games/' + slug).strip() == self.git(work, 'rev-parse', source + ':games/' + slug).strip()
        if delivered:
            revision = current['id']
        else:
            expected = self.state['baselines'][slug]['expected_revision']
            if game['head_revision'] != expected:
                raise RuntimeError('Creator head advanced during engine game delivery; never overwrite a concurrent draft')
            record.setdefault('request_id', str(uuid4()))
            self.save()
            version = tomllib.loads((Path(work) / 'games' / slug / 'Cargo.toml').read_text())['package']['version']
            event = self.api(work, '--request-id', record['request_id'], 'submit', slug, '--commit', source,
                '--source-repository', self.config['engine_repo'], '--source-game', slug,
                '--expected-revision', expected, '--version', version,
                '--note', 'Validated corrective update from daily collaborative feedback.')
            revision = event.get('event', event).get('revision', event.get('revision', {}))['id']
        return revision

    @classmethod
    def resume(cls, config, run, **callbacks):
        return cls(config, run, json.loads((run / 'engine-games-receipt.json').read_text()), **callbacks)


def native_phase():
    parser = argparse.ArgumentParser()
    parser.add_argument('phase', choices=('create', 'review', 'resume'))
    for name in ('engine-root', 'games-root', 'runs-root', 'directory', 'receipt'):
        parser.add_argument('--' + name, type=Path, required=name == 'engine-root')
    parser.add_argument('--slug')
    args = parser.parse_args()
    sys.path.insert(0, str(args.engine_root))
    from tools import community_release
    if args.phase == 'create':
        release = community_release.create(args.engine_root, args.games_root, args.slug, runs_root=args.runs_root)
        args.receipt.write_text(json.dumps({'run': str(release.directory)}) + '\n')
    elif args.phase == 'review':
        forge = community_release.forge
        with forge.run_lock(args.directory):
            state = forge.load_json(args.directory / 'state.json')
            release = community_release.Release(args.directory, state)
            for phase, operation in (('prepare', release.prepare), ('local', release.verify_local),
                                     ('engine_ci', release.engine_ci), ('export', release.export),
                                     ('installer', release.installer_review)):
                if phase in state['completed']:
                    continue
                operation()
                state['completed'].append(phase)
                release.save()
    else:
        community_release.resume(args.directory)


if __name__ == '__main__':
    native_phase()
