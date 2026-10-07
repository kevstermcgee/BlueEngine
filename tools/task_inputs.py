"""Stateless input identity and read-only probes for canonical task/check reports.

No test-result cache. Game identity includes all project inputs (including tests and
Cargo target sources); engine identity hashes indexed/nonignored files, not just Git.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

try:
    from . import author, workflow
except ImportError:
    import author, workflow


def probe(argv, root, calls=None):
    if calls is not None:
        calls.append(list(map(str, argv)))
    try:
        result = subprocess.run(argv, cwd=root, capture_output=True, text=True, timeout=10,
                                env={**os.environ, 'RUSTUP_AUTO_INSTALL': '0'},
                                **workflow.console_options())
        return {'ok': result.returncode == 0, 'output': result.stdout if '-z' in argv else result.stdout.strip(),
                'error': result.stderr.strip()[-400:] if result.returncode else None}
    except (OSError, subprocess.SubprocessError) as error:
        return {'ok': False, 'output': '', 'error': str(error)}


def file_hash(path):
    path = Path(path)
    if not path.is_file():
        return None
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def executable(path):
    return {'path': str(path) if path else None,
            'sha256': file_hash(path) if path else None,
            'freshness': 'unverified; map uses Cargo freshness before authoring'}


def engine_identity(root, calls=None):
    result = probe(['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], root, calls)
    if not result['ok']:
        return {'sha256': None, 'error': 'Engine source identity needs Git file inventory; evidence remains unverified.'}
    digest = hashlib.sha256()
    count = 0
    for name in sorted(set(filter(None, result['output'].split('\0')))):
        path = root / name
        digest.update(name.encode('utf-8'))
        if path.is_symlink():
            digest.update(os.readlink(path).encode('utf-8'))
        value = file_hash(path)
        digest.update((value or 'missing').encode('ascii'))
        count += 1
    return {'sha256': digest.hexdigest(), 'files': count}


def game_source_hash(game, *, without_lock=False):
    """Content identity independent of retired browser tooling and Git initialization.

    Include tracked files even when subsequently ignored/deleted, plus local inputs
    in scratch games. Exclude only conventional generated outputs, never tests,
    examples, benches, root-level scripts or Cargo configuration. Hash path + content
    so deletion/rename and symlink changes invalidate evidence too.
    """
    game = Path(game).resolve()
    inventory = probe(['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z', '--', '.'], game)
    names = set(filter(None, inventory['output'].split('\0'))) if inventory['ok'] else set()
    outputs = {'.git', 'target', 'dist', '.blue-check', '.be2-work', '__pycache__'}
    for folder, directories, files in os.walk(game):
        directories[:] = sorted(d for d in directories if d not in outputs)
        for name in files:
            if not name.endswith(('.pyc', '.pyo')):
                names.add((Path(folder) / name).relative_to(game).as_posix())
    digest = hashlib.sha256()
    for name in sorted(names):
        if without_lock and name == 'Cargo.lock':
            continue
        path = game / name
        digest.update(name.encode('utf-8'))
        if path.is_symlink():
            digest.update(os.readlink(path).encode('utf-8'))
        digest.update((file_hash(path) or 'missing').encode('ascii'))
    return digest.hexdigest()


def capture(root, game=None, calls=None):
    root = Path(root).resolve()
    compiler = probe(['rustc', '--version', '--verbose'], root, calls)
    sysroot = probe(['rustc', '--print', 'sysroot'], root, calls)
    cargo = probe(['cargo', '--version'], root, calls)
    env_names = ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_TARGET', 'CARGO_TARGET_DIR',
                 'BE2_TOOLS', 'RUSTUP_TOOLCHAIN', 'CC', 'AR')
    result = {'schema_version': 2, 'engine': engine_identity(root, calls),
              'tools': {'python': executable(Path(os.sys.executable)),
                        'authoring': executable(author.native_binary(root)),
                        'compiler': compiler, 'sysroot': sysroot, 'cargo': cargo},
              'environment_sha256': hashlib.sha256(json.dumps({k: os.environ.get(k) for k in env_names},
                                                              sort_keys=True).encode()).hexdigest()}
    cargo_home = Path(os.environ.get('CARGO_HOME') or Path.home() / '.cargo')
    if not cargo_home.is_absolute():
        cargo_home = root / cargo_home
    result['configuration'] = {str(p): file_hash(p) for p in (cargo_home / 'config', cargo_home / 'config.toml')}
    if game and (Path(game) / 'Cargo.toml').is_file():
        game = Path(game)
        result['game'] = {'sha256': game_source_hash(game),
                          'without_lock': game_source_hash(game, without_lock=True),
                          'lock': file_hash(game / 'Cargo.lock')}
        result['configuration'].update({str(p): file_hash(p) for p in (game / '.cargo/config', game / '.cargo/config.toml')})
        # Output identity prevents a replaced binary/config/package from retaining a prior pass.
        files = []
        for directory in (game / 'dist',):
            if directory.exists():
                files.extend(p for p in directory.rglob('*') if p.is_file())
        result['artifacts'] = hashlib.sha256(json.dumps([(str(p.relative_to(game)), file_hash(p))
                                                       for p in sorted(files)], separators=(',', ':')).encode()).hexdigest()
    return result


def stable_sources(before, after, plan):
    if not before['engine'].get('sha256') or before['engine'] != after['engine']:
        return False
    if before['environment_sha256'] != after['environment_sha256']:
        return False
    if before['configuration'] != after['configuration']:
        return False
    for name in ('python', 'compiler', 'sysroot', 'cargo'):
        if before['tools'][name] != after['tools'][name]:
            return False
    if before['tools']['authoring'] != after['tools']['authoring'] and os.environ.get('BE2_TOOLS'):
        return False
    first, last = before.get('game'), after.get('game')
    if first != last:
        # Fresh scaffolds legitimately register their root in Cargo.lock before locked tests.
        metadata = any(c[:2] == ['cargo', 'metadata'] for c in plan['commands'])
        if not (metadata and first and last and first['without_lock'] == last['without_lock']):
            return False
    return True
