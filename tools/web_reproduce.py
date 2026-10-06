"""Rebuild an expected browser package using anonymous source and an empty Cargo target."""
import json
import os
from pathlib import Path
import shutil
import tempfile
from tools import web_release as release


def reproduce(package, output=None):
    from tools.web_games import integrity, run
    package = package if (package / 'manifest.json').is_file() else package / 'dist/web'
    expected = integrity(package)
    if run(['rustc','--version']).strip()!=expected.get('reproduce',{}).get('rustc'):
        raise release.ReleaseError('Reproduction requires the recorded rustc toolchain; select it with RUSTUP_TOOLCHAIN')
    retrieval = release.retrieve_sources(expected)
    output = output or ((package.parent.parent / '.blue-check/reproduction') if package.name=='web' and package.parent.name=='dist' else package.parent / ('.blue-check-'+package.name) / 'reproduction')
    output.mkdir(parents=True, exist_ok=True)
    sources = expected['sources']
    with tempfile.TemporaryDirectory(prefix='be2-clean-reproduction-') as folder:
        workspace = Path(folder)
        if sources['engine']['repository'] == sources['game']['repository'] and sources['engine']['revision'] == sources['game']['revision']:
            engine_repo = workspace / 'source'
            clone(sources['engine'], engine_repo)
            game = engine_repo / sources['game']['path']
            engine = engine_repo / sources['engine']['path']
        else:
            # Preserve the committed relative dependency; never patch Cargo.toml to make a test pass.
            relative = expected['reproduce']['engine_relative_to_game']
            base = workspace / 'layout'
            for _ in range(relative.count('..') + 1):
                base = base / 'parent'
            game_repo = base / 'game-source'
            clone(sources['game'], game_repo)
            game = game_repo / sources['game']['path']
            engine = (game / relative).resolve()
            engine.relative_to(workspace)
            engine_repo = engine
            for _ in Path(sources['engine']['path']).parts if sources['engine']['path'] != '.' else []:
                engine_repo = engine_repo.parent
            clone(sources['engine'], engine_repo)
        run(['npm','ci','--ignore-scripts','--prefix',engine/'tools'])
        target = workspace / 'empty-target'
        env = {**os.environ, 'CARGO_TARGET_DIR': str(target), 'RUSTC_WRAPPER': '',
               'SOURCE_DATE_EPOCH': str(expected['built_at_epoch'])}
        # Dependency archives may be cached; compiled binaries/source-tree artifacts cannot be reused.
        run(['cargo', 'fetch', '--locked'], cwd=game, env=env)
        raw = run(['python3', engine / 'tools/be2.py', 'web', 'build', game], cwd=engine, env=env)
        actual = json.loads(raw)['manifest']
        fields = ('package_id', 'engine_revision', 'game_revision', 'game_source_sha256',
                  'sources', 'verification', 'file_sha256', 'compatibility', 'controls',
                  'required_capabilities', 'built_at_epoch', 'reproduce', 'browser_verification')
        differences = {key: {'expected': expected.get(key), 'actual': actual.get(key)}
                       for key in fields if expected.get(key) != actual.get(key)}
        shutil.copytree(game / '.blue-check/web', output / 'browser', dirs_exist_ok=True)
        result = {'ok': not differences, 'source_retrieval': retrieval,
                  'engine_revision': actual['engine_revision'], 'game_revision': actual['game_revision'],
                  'package_id': actual['package_id'], 'compared_fields': list(fields),
                  'differences': differences, 'empty_target': True,
                  'checkout': 'anonymous exact commit, no local Git objects or working files',
                  'dependency_archives': 'Cargo cache allowed; all binaries compiled afresh'}
        (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8', newline='\n')
        if differences:
            raise release.ReleaseError(f'Clean reproduction differs; inspect {output / "result.json"}')
        return result


def clone(source, destination):
    destination.mkdir(parents=True)
    release.git(['init', '.'], destination)
    release.git(['remote', 'add', 'origin', source['repository']], destination)
    release.git(['-c', 'credential.helper=', '-c', 'core.askPass=', 'fetch', '--depth=1', 'origin', source['revision']], destination)
    release.git(['checkout', '--detach', source['revision']], destination)
