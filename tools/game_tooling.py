#!/usr/bin/env python3
"""Shared project tooling. Explicitly migrate reviewed projects; check launcher compatibility."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def module_from(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def entry(command):
    return ('#!/usr/bin/env python3\n'
            '"""Compatibility entry point; shared implementation is pinned in tooling.json."""\n'
            'from pathlib import Path\nimport importlib.util\n'
            '_spec = importlib.util.spec_from_file_location("be2_bootstrap", Path(__file__).with_name("tooling.py"))\n'
            '_bootstrap = importlib.util.module_from_spec(_spec)\n_spec.loader.exec_module(_bootstrap)\n'
            f'_impl = _bootstrap.load({command!r}, Path(__file__).resolve().parents[1])\n'
            'globals().update({k: v for k, v in vars(_impl).items() if not k.startswith("__")})\n'
            'if __name__ == "__main__":\n'
            '    import sys\n'
            + ('    sys.exit(_impl.main(sys.argv[1:]))\n' if command == 'dev' else
               '    sys.exit(_impl.main(root=Path(__file__).resolve().parents[1]))\n' if command in ('check', 'ship') else
               '    print("Project requirements are checked by scripts/check.py")\n'))


def fresh_tools(engine, explicit=None):
    """Let Cargo prove source freshness, including shared target/config discovery.

    An override must be byte-identical to the just-verified build. A filename or
    executable on PATH is insufficient evidence of compatibility.
    """
    import hashlib
    packaged_manifest = engine / 'PACKAGE-MANIFEST.json'
    if packaged_manifest.is_file():
        manifest = json.loads(packaged_manifest.read_text(encoding='utf-8'))
        records = manifest.get('sha256', {})
        suffix = '.exe' if os.name == 'nt' else ''
        binary_key = 'be2/bin/be2-tools' + suffix
        if manifest.get('format') != 1 or binary_key not in records:
            raise ValueError('Packaged tooling has no verifiable binary; use a source checkout and map help')
        for name, expected in records.items():
            relative = Path(name.removeprefix('be2/'))
            if not name.startswith('be2/') or relative.is_absolute() or '..' in relative.parts:
                raise ValueError('Invalid packaged-tooling manifest path; restore the engine package')
            if hashlib.sha256((engine / relative).read_bytes()).hexdigest() != expected:
                raise ValueError(f'Packaged tooling input changed: {name}; use a source checkout and map help')
        binary = engine / 'bin' / ('be2-tools' + suffix)
        if explicit and hashlib.sha256(Path(explicit).read_bytes()).digest() != hashlib.sha256(binary.read_bytes()).digest():
            raise ValueError(f'{explicit} is incompatible or stale; use {binary} or remove BE2_TOOLS/--tools')
        return str(Path(explicit).resolve() if explicit else binary)
    profile = 'itest'
    if explicit:
        directory = Path(explicit).resolve().parent.name
        if directory in ('debug', 'release', 'fast', 'itest'):
            profile = 'dev' if directory == 'debug' else directory
    result = subprocess.run(['cargo', 'build', '--locked', '--profile', profile,
                             '--no-default-features', '--features', 'physics,networking', '--bin', 'be2-tools',
                             '--message-format=json-render-diagnostics'], cwd=engine,
                            capture_output=True, text=True, encoding='utf-8', errors='replace')
    binary = None
    for line in result.stdout.splitlines():
        try:
            message = json.loads(line)
        except ValueError:
            continue
        if message.get('reason') == 'compiler-message':
            print(message.get('message', {}).get('rendered', ''), end='', file=sys.stderr)
        if message.get('reason') == 'compiler-artifact' and message.get('target', {}).get('name') == 'be2-tools':
            binary = message.get('executable') or binary
    if result.stderr:
        print(result.stderr, end='', file=sys.stderr)
    if result.returncode or not binary:
        raise ValueError('Could not verify be2-tools freshness with Cargo; rerun '
                         'python ENGINE/tools/be2.py map help and inspect the compiler diagnostic')
    if explicit:
        candidate = Path(explicit).resolve()
        if hashlib.sha256(candidate.read_bytes()).digest() != hashlib.sha256(Path(binary).read_bytes()).digest():
            raise ValueError(f'{candidate} is incompatible or stale; use {binary} or remove BE2_TOOLS/--tools')
        return str(candidate)
    return binary


def load(command, game, engine):
    module = module_from(engine / f'templates/game_{command}.py', f'be2_shared_{command}')
    if command == 'dev':
        module.ROOT = game
    if command == 'check':
        # main resolves freshness after parsing so --help never compiles.
        module.ensure_tools = lambda explicit: fresh_tools(module.engine_path(game) or engine, explicit)
    return module


def pin(game, engine=ROOT):
    bootstrap = module_from(engine / 'templates/game_entry.py', 'be2_tooling_bootstrap')
    if (engine / 'PACKAGE-MANIFEST.json').is_file():
        revision = json.loads((engine / 'PACKAGE-MANIFEST.json').read_text(encoding='utf-8'))['commit']
    else:
        revision = subprocess.run(['git', '-C', str(engine), 'rev-parse', 'HEAD'], capture_output=True,
                                  text=True, check=True).stdout.strip()
    try:
        relative = os.path.relpath(engine, game)
    except ValueError:  # Windows volumes
        relative = str(engine)
    return {'schema_version': 1, 'engine': relative, 'revision': revision,
            'sha256': bootstrap.tooling_digest(engine)}


def check(game, engine=ROOT):
    """Recognize small game hooks; check only the engine-owned launchers and pin."""
    bootstrap = module_from(engine / 'templates/game_entry.py', 'be2_tooling_bootstrap')
    recorded = json.loads((game / 'scripts/tooling.json').read_text(encoding='utf-8'))
    if recorded.get('schema_version') != 1 or recorded.get('sha256') != bootstrap.tooling_digest(engine):
        raise ValueError(f'{game}: stale tooling pin; review changes and migrate explicitly')
    if (game / recorded['engine']).resolve() != engine.resolve():
        raise ValueError(f'{game}: tooling pin points to a different engine dependency')
    for command in ('check', 'ship', 'dev', 'project'):
        if (game / f'scripts/{command}.py').read_text(encoding='utf-8') != entry(command):
            raise ValueError(f'{game}: scripts/{command}.py drift; move custom preparation into tooling_hooks.py')
    if (game / 'scripts/tooling.py').read_bytes() != (engine / 'templates/game_entry.py').read_bytes():
        raise ValueError(f'{game}: bootstrap drift; review and migrate explicitly')


def migrate(game, engine=ROOT):
    """Only replace reviewed canonical copies, common historical drift, or launchers.

    Unknown customization and unextracted historical preparation are rejected
    before writing any project file.
    """
    for command in ('check', 'ship', 'dev', 'project'):
        destination = game / f'scripts/{command}.py'
        if not destination.exists() and command == 'project':
            continue
        existing = destination.read_text(encoding='utf-8')
        canonical = (engine / f'templates/game_{command}.py').read_text(encoding='utf-8')
        if existing == entry(command) or existing == canonical:
            continue
        # The inventory is an explicit approval of these source variants; unknown
        # project code is not silently overwritten by a synchronization command.
        import hashlib
        inventory = json.loads((engine / 'tools/game_tooling_inventory.json').read_text(encoding='utf-8'))
        digest = hashlib.sha256(existing.encode('utf-8')).hexdigest()
        if digest not in inventory['compatible'][command]:
            raise ValueError(f'{destination}: unreviewed customization; extract an explicit hook before migration')
        required = inventory.get('required_hooks', {}).get(command, {}).get(digest)
        if required is not None:
            hook = game / 'scripts/tooling_hooks.py'
            hook_digest = (hashlib.sha256(hook.read_text(encoding='utf-8').encode('utf-8')).hexdigest()
                           if hook.is_file() else None)
            if hook_digest != required['sha256']:
                raise ValueError(
                    f'{destination}: migration would remove {required["preparation"]}; '
                    'extract it into scripts/tooling_hooks.py:prepare_package before migration. '
                    f'Reviewed hook: {engine / required["reference"]} '
                    f'(SHA-256 {required["sha256"]}). Preserve helper scripts; '
                    'review a different hook explicitly instead of overwriting it. No files changed.')
    scripts = game / 'scripts'
    scripts.mkdir(parents=True, exist_ok=True)
    payloads = {f'{command}.py': entry(command).encode('utf-8') for command in ('check', 'ship', 'dev', 'project')}
    payloads['tooling.py'] = (engine / 'templates/game_entry.py').read_bytes()
    payloads['tooling.json'] = (json.dumps(pin(game, engine), indent=2) + '\n').encode('utf-8')
    for name, payload in payloads.items():
        destination = scripts / name
        temporary = scripts / ('.' + name + '.new')
        temporary.write_bytes(payload)
        os.replace(temporary, destination)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('check', 'migrate'))
    parser.add_argument('games', type=Path, nargs='+')
    args = parser.parse_args(argv)
    try:
        for game in args.games:
            {'check': check, 'migrate': migrate}[args.action](game.resolve())
        print(json.dumps({'ok': True, 'action': args.action, 'games': [str(g) for g in args.games]}))
        return 0
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(json.dumps({'ok': False, 'error': str(error)}))
        return 1


if __name__ == '__main__':
    sys.exit(main())
