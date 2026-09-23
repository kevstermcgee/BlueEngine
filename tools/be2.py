#!/usr/bin/env python3
"""BE2 development entry point. Python 3.10+, standard library only. No shell evaluation."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / '.be2-work'
SUFFIX = '.exe' if os.name == 'nt' else ''


def invoke(args, *, env=None, log=None, capture=False, timeout=None):
    print('+ ' + ' '.join(map(str, args)), file=sys.stderr)
    result = subprocess.run(list(map(str, args)), cwd=ROOT, env=env,
                            stdout=subprocess.PIPE if capture or log else None,
                            stderr=subprocess.STDOUT if capture or log else None,
                            text=True, timeout=timeout)
    if log:
        Path(log).write_text(result.stdout, encoding='utf-8')
    if result.returncode:
        if result.stdout:
            print(result.stdout, file=sys.stderr)
        raise RuntimeError(f'Command failed ({result.returncode}); log: {log or "console"}')
    return result.stdout


def environment(kind):
    env = os.environ.copy()
    # Keep feature-isolated outputs; never mix a graphics-enabled headless binary into a package.
    base = Path(env.get('CARGO_TARGET_DIR', ROOT / 'target')).resolve()
    target = base / ('be2-' + kind)
    env['CARGO_TARGET_DIR'] = str(target)
    return env, target


def build(kind):
    env, target = environment(kind)
    args = ['cargo', 'build', '--release', '--locked']
    if kind == 'client':
        args += ['--bin', 'be2', '--bin', 'vesper3d']
    else:
        args += ['--no-default-features', '--bin', 'be2-headless' if kind == 'headless' else 'be2-tools']
    invoke(args, env=env)
    return target / 'release'


def tool(args):
    binary = build('tools') / ('be2-tools' + SUFFIX)
    invoke([binary, *args])


def doctor():
    result = {'root': str(ROOT), 'python': sys.version.split()[0],
              'programs': {}, 'entry_points': ['AGENTS.md', 'tools/README.md', 'tools/FEATURES.json'],
              'note': 'No dependencies are installed or settings changed by doctor.'}
    for name in ['cargo', 'rustc', 'git', 'ffmpeg']:
        path = shutil.which(name)
        result['programs'][name] = path
    result['git_status'] = invoke(['git', 'status', '--short'], capture=True) if shutil.which('git') else 'unavailable'
    if shutil.which('cargo'):
        result['cargo_version'] = invoke(['cargo', '--version'], capture=True).strip()
    result['ready_to_build'] = all(result['programs'][p] for p in ['cargo', 'rustc'])
    print(json.dumps(result, indent=2))
    if not result['ready_to_build']:
        raise RuntimeError('Rust toolchain is missing; see tools/README.md')


def check():
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
    directory = WORK / ('check-' + stamp)
    directory.mkdir(parents=True)
    commands = [
        ['cargo', 'fmt', '--check'],
        ['cargo', 'test', '--locked'],
        ['cargo', 'clippy', '--all-targets', '--locked', '--', '-D', 'warnings'],
        ['cargo', 'test', '--locked', '--no-default-features'],
        ['cargo', 'clippy', '--all-targets', '--locked', '--no-default-features', '--', '-D', 'warnings'],
    ]
    report = {'ok': False, 'checks': []}
    try:
        for i, cmd in enumerate(commands):
            log = directory / f'{i+1}.log'
            item = {'command': cmd, 'log': log.name, 'ok': False}
            report['checks'].append(item)
            invoke(cmd, log=log)
            item['ok'] = True
        report['ok'] = True
    finally:
        (directory / 'report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
        print(json.dumps({'report': str(directory / 'report.json'), 'ok': report['ok']}))


def capture(destination, map_file):
    destination = Path(destination).resolve()
    if destination.exists():
        raise RuntimeError('Capture destination must be new')
    if map_file:
        tool(['audit', str(Path(map_file).resolve())])
    binary = build('client') / ('be2' + SUFFIX)
    args = [binary, '--capture-house', destination]
    if map_file:
        args += ['--map', Path(map_file).resolve()]
    invoke(args, timeout=60)
    report = destination / 'render-report.txt'
    if not report.is_file() or len(list(destination.glob('blue-engine-*.png'))) != 12:
        raise RuntimeError('Capture did not complete; preserve output for diagnosis')
    print(json.dumps({'ok': True, 'report': str(report),
                      'next': 'Inspect PNGs including menu; camera positions are house-specific.'}))


def package(destination):
    destination = Path(destination).resolve()
    if destination.exists():
        raise RuntimeError('Package destination must be new')
    client, headless, tooling = build('client'), build('headless'), build('tools')
    tracked = invoke(['git', 'ls-files', '-z'], capture=True).split('\0')
    members = {}
    for rel in filter(None, tracked):
        path = ROOT / rel
        if path.is_symlink():
            raise RuntimeError(f'Package refuses symlink: {rel}')
        if path.is_file() and not rel.startswith('bin/'):
            members['be2/' + rel] = path
    for folder, name, output in [(client, 'be2', 'BE2'), (client, 'vesper3d', 'vesper3d'),
                                 (headless, 'be2-headless', 'be2-headless'), (tooling, 'be2-tools', 'be2-tools')]:
        members['be2/bin/' + output + SUFFIX] = folder / (name + SUFFIX)
    hashes = {name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in members.items()}
    manifest = {'format': 1, 'platform': sys.platform,
                'commit': invoke(['git', 'rev-parse', 'HEAD'], capture=True).strip(),
                'working_tree_status': invoke(['git', 'status', '--short'], capture=True),
                'sha256': hashes,
                'note': 'Includes current tracked working files; inspect dirty status. Run check separately.'}
    # Zip exclusive creation prevents overwriting a previous release, including races.
    with zipfile.ZipFile(destination, 'x', zipfile.ZIP_DEFLATED) as z:
        for name, path in members.items():
            z.write(path, name)
        z.writestr('be2/PACKAGE-MANIFEST.json', json.dumps(manifest, indent=2))
    print(json.dumps({'ok': True, 'package': str(destination), 'files': len(members)}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('doctor')
    sub.add_parser('check')
    b = sub.add_parser('build'); b.add_argument('kind', choices=['client', 'headless', 'tools', 'all'])
    c = sub.add_parser('capture'); c.add_argument('destination'); c.add_argument('--map')
    p = sub.add_parser('package'); p.add_argument('destination')
    t = sub.add_parser('map'); t.add_argument('arguments', nargs=argparse.REMAINDER)
    sub.add_parser('features')
    args = parser.parse_args()
    if args.command == 'doctor': doctor()
    elif args.command == 'check': check()
    elif args.command == 'build':
        for kind in ['client', 'headless', 'tools'] if args.kind == 'all' else [args.kind]:
            print(build(kind))
    elif args.command == 'capture': capture(args.destination, args.map)
    elif args.command == 'package': package(args.destination)
    elif args.command == 'map': tool(args.arguments)
    elif args.command == 'features': print((ROOT / 'tools/FEATURES.json').read_text(encoding='utf-8'))


if __name__ == '__main__':
    try:
        main()
    except (RuntimeError, OSError, ValueError, subprocess.TimeoutExpired) as error:
        print(json.dumps({'ok': False, 'error': str(error)}), file=sys.stderr)
        sys.exit(1)
