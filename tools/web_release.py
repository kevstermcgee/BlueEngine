"""Browser release identity, source retrieval and runtime asset closure (stdlib only)."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile


class ReleaseError(ValueError):
    pass


def git(args, cwd):
    result = subprocess.run(['git', *args], cwd=cwd, capture_output=True, text=True, timeout=120, env={**os.environ,'GIT_TERMINAL_PROMPT':'0'})
    if result.returncode:
        raise ReleaseError(f"Source retrieval failed: git {' '.join(args)}\n{result.stderr[-2000:]}")
    return result.stdout.strip()


def source(folder, inputs):
    """Record the actual checkout; a content hash cannot substitute for retrievable Git source."""
    root = Path(git(['rev-parse', '--show-toplevel'], folder))
    revision = git(['rev-parse', 'HEAD'], root)
    repo = git(['remote', 'get-url', 'origin'], root)
    if repo.startswith('git@github.com:'):
        repo = 'https://github.com/' + repo.split(':', 1)[1]
    validate_source({'repository': repo, 'revision': revision, 'path': '.'})
    paths = [str((folder / p).relative_to(root)) for p in inputs]
    dirty = git(['status', '--porcelain', '--untracked-files=all', '--', *paths], root)
    return {'repository': repo, 'revision': revision, 'path': folder.relative_to(root).as_posix(),
            'clean': not dirty, 'dirty_paths': dirty.splitlines()}


def validate_source(value):
    if not isinstance(value, dict) or not re.fullmatch(r'[0-9a-f]{40}', str(value.get('revision', ''))):
        raise ReleaseError('Release sources require exact 40-character Git revisions')
    if not re.fullmatch(r'https://[A-Za-z0-9.-]+/[A-Za-z0-9_./-]+', str(value.get('repository', ''))):
        raise ReleaseError('Release sources require public HTTPS repository URLs without credentials')
    path = value.get('path')
    if not isinstance(path, str) or not path or path.startswith('/') or '\\' in path or (path != '.' and any(p in ('', '.', '..') for p in path.split('/'))):
        raise ReleaseError('Release source path must be relative to its repository')


def retrieve_sources(manifest):
    """Fetch each exact commit without credentials or local object reuse before any publication write."""
    sources = manifest.get('sources', {})
    if set(sources) != {'engine', 'game'}:
        raise ReleaseError('Publication requires engine and game source repositories/revisions; rebuild legacy packages')
    for value in sources.values():
        validate_source(value)
    fetched = {}
    proof = []
    for role, value in sources.items():
        validate_source(value)
        if value.get('clean') is not True:
            raise ReleaseError(f'{role} source has uncommitted inputs; commit and rebuild before publication')
        if value['revision'] != manifest.get(role + '_revision'):
            raise ReleaseError(f'{role} source revision contradicts manifest')
        key = (value['repository'], value['revision'])
        with tempfile.TemporaryDirectory(prefix='be2-public-source-') as folder:
            if key not in fetched:
                git(['init', '--bare', '.'], folder)
                # No gh/token credentials: private or missing source cannot meet the public-source contract.
                git(['-c', 'credential.helper=', '-c', 'core.askPass=', 'fetch', '--no-tags', '--depth=1', value['repository'], value['revision']], folder)
                actual = git(['rev-parse', 'FETCH_HEAD^{commit}'], folder)
                if actual != value['revision']:
                    raise ReleaseError('Retrieved source differs from claimed revision')
                tree = git(['ls-tree', '-r', '--name-only', actual], folder).splitlines()
                game = sources['game']
                game_hash = None
                if key == (game['repository'], game['revision']):
                    prefix = '' if game['path'] == '.' else game['path'] + '/'
                    names = sorted(p[len(prefix):] for p in tree if p.startswith(prefix)
                                   and (p[len(prefix):].split('/')[0] in ('src', 'assets', 'scripts')
                                        or p[len(prefix):] in ('build.rs', 'AUDIO.md', 'Cargo.toml', 'Cargo.lock', 'game.project.json')))
                    digest = hashlib.sha256()
                    for name in names:
                        blob = subprocess.run(['git', 'show', actual + ':' + prefix + name], cwd=folder,
                                              capture_output=True, timeout=120)
                        if blob.returncode:
                            raise ReleaseError('Unable to read retrieved game source')
                        digest.update(name.encode());digest.update(blob.stdout)
                    game_hash = digest.hexdigest()
                fetched[key] = (tree, game_hash)
            tree, game_hash = fetched[key]
            entry = 'Cargo.toml' if value['path'] == '.' else value['path'] + '/Cargo.toml'
            if entry not in tree:
                raise ReleaseError(f'{role} source path is absent from retrieved commit: {entry}')
            if role == 'game' and manifest.get('game_source_sha256') and manifest['game_source_sha256'] != game_hash:
                raise ReleaseError('Retrieved game source hash differs from build; rebuild using committed inputs')
        proof.append({'role': role, **value, 'retrieved': True})
    return {'ok': True, 'method': 'isolated anonymous exact-commit git fetch', 'sources': proof}


def package_id(hashes):
    # Self/manifest are excluded to avoid recursive identities. All executable/assets remain covered.
    content = {k: v for k, v in hashes.items() if k not in ('manifest.json', 'service-worker.js')}
    return hashlib.sha256(json.dumps(content, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def runtime_files(game, declarations, safe_file):
    """An AudioBank directory means bank.json + referenced PCM, never preview mixes/reports/scores."""
    selected = set()
    for declaration in declarations:
        source = safe_file(game, declaration)
        if not source.exists():
            raise ReleaseError(f'Runtime asset is missing: {declaration}')
        candidates = sorted(source.rglob('*')) if source.is_dir() else [source]
        banks = [p for p in candidates if p.name == 'bank.json']
        bank_dirs = {p.parent for p in banks}
        for bank in banks:
            value = json.loads(bank.read_text())
            selected.add(bank.relative_to(game).as_posix())
            for group in ('effects', 'music'):
                for spec in value.get(group, {}).values():
                    name = (bank.parent.relative_to(game) / spec['file']).as_posix()
                    required = safe_file(game, name)
                    if not required.is_file():
                        raise ReleaseError(f'Audio bank runtime file missing: {name}')
                    selected.add(name)
        for path in candidates:
            name = path.relative_to(game).as_posix()
            path = safe_file(game, name)
            if path.is_dir():
                continue
            if not path.is_file():
                raise ReleaseError(f'Runtime asset must be a regular file: {name}')
            if any(parent in bank_dirs for parent in (path.parent, *path.parents)):
                continue
            if any(part in ('audio-source', '.blue-check', '.be2-work', 'target') for part in path.parts):
                raise ReleaseError(f'Authoring/verification assets cannot be runtime directories: {name}')
            selected.add(name)
    return sorted(selected)


def sizes(package, manifest):
    paths = {name: (package / name).stat().st_size for name in manifest['file_sha256']}
    wasm = paths.get('game.wasm', 0)
    audio = sum(size for name, size in paths.items() if name.endswith(('.wav', '.ogg', '.mp3')))
    return {'wasm_bytes': wasm, 'audio_bytes': audio,
            'other_asset_bytes': sum(paths.values()) - wasm - audio,
            'manifest_bytes': (package / 'manifest.json').stat().st_size,
            'total_bytes': sum(paths.values()) + (package / 'manifest.json').stat().st_size,
            'files': len(paths) + 1, 'precache_bytes': sum(paths.values()) + (package / 'manifest.json').stat().st_size}
