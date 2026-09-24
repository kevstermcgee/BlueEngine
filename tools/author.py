#!/usr/bin/env python3
"""Source-free BE2 map authoring. Python 3.10+, standard library only."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / 'tools' / 'authoring.json'
VERSION = 1


def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def native(args, timeout=60):
    binary = ROOT / 'bin' / ('be2-tools.exe' if os.name == 'nt' else 'be2-tools')
    if not binary.is_file():
        raise ValueError('Packaged native tool missing. Use python tools/be2.py build tools and copy its binary to bin/.')
    run = subprocess.run([str(binary), *map(str, args)], capture_output=True,
                         text=True, timeout=timeout, cwd=ROOT)
    try:
        result = json.loads(run.stdout)
    except ValueError:
        result = {'message': run.stdout[-4000:], 'stderr': run.stderr[-4000:]}
    if run.returncode:
        raise ValueError(json.dumps({'native_exit': run.returncode, 'detail': result}))
    return result


def vector(value):
    parts = [float(p) for p in value.split(',')]
    if len(parts) != 3 or not all(math.isfinite(p) and abs(p) <= 1000 for p in parts):
        raise ValueError('Expected three finite coordinates in -1000..1000')
    return parts


def identifier(value):
    if not re.fullmatch(r'[A-Za-z0-9_-]{1,64}', value):
        raise ValueError('Instance ID must contain 1..64 letters, digits, underscores or hyphens')
    return value


def save_new(path, data):
    # Serialize before exclusive creation; never overwrite a completed document.
    payload = json.dumps(data, indent=2, allow_nan=False) + '\n'
    with Path(path).open('x', encoding='utf-8') as file:
        file.write(payload)


def describe(data):
    catalog = native(['catalog'])
    expected = {asset['native_kind'] for asset in data['assets']}
    if not expected.issubset(set(catalog['props'])):
        raise ValueError('Authoring metadata does not match this native binary')
    return {'protocol_version': VERSION, 'native': catalog, 'contract': data['contract'],
            'commands': data['commands'], 'asset_count': len(data['assets']),
            'recipes': [item['id'] for item in data['recipes']],
            'native_sha256': digest(ROOT / 'bin' / ('be2-tools.exe' if os.name == 'nt' else 'be2-tools')),
            'metadata_sha256': digest(DATA)}


def query(data, text, limit):
    terms = set(re.findall(r'[a-z0-9]+', text.lower()))
    hits = []
    for group in ('assets', 'recipes', 'topics'):
        for item in data[group]:
            words = set(re.findall(r'[a-z0-9]+', json.dumps(item).lower()))
            score = len(terms & words)
            if score:
                hits.append({'type': group[:-1], 'score': score, **item})
    hits.sort(key=lambda hit: (-hit['score'], hit['id']))
    return {'matches': hits[:limit], 'total': len(hits), 'method': 'local curated token search',
            'engine_source_read': False,
            'note': 'No match means no indexed capability; do not invent an API.'}


def instantiate(data, name, instance, origin):
    identifier(instance)
    recipe = next((r for r in data['recipes'] if r['id'] == name), None)
    asset = next((a for a in data['assets'] if a['id'] == name), None)
    if recipe:
        ops = json.loads(json.dumps(recipe['patch']))
    elif asset:
        ops = [{'op': 'add_prop', 'id': 'prop', 'label': asset['label'],
                'kind': asset['native_kind'], 'origin': [0, 0, 0]}]
    else:
        raise ValueError('Unknown asset or recipe ID; use query or assets')
    for op in ops:
        op['id'] = instance + '/' + op['id']
        key = 'origin' if op['op'] == 'add_prop' else 'center'
        op[key] = [a + b for a, b in zip(op[key], origin)]
    return ops


def verify(args):
    directory = Path(args.output).resolve()
    directory.mkdir(parents=True, exist_ok=False)
    report = {'protocol_version': VERSION, 'ok': False, 'checks': [],
              'visual_status': 'not_requested', 'limitations':
              ['Routes are reachability checks, not pathfinding.',
               'Image hashes detect any pixel/file change; they do not judge appearance.']}
    try:
        map_file = Path(args.map).resolve()
        report['map_sha256'] = digest(map_file)
        report['native_sha256'] = digest(ROOT / 'bin' / ('be2-tools.exe' if os.name == 'nt' else 'be2-tools'))
        report['checks'].append({'name': 'audit', 'result': native(['audit', map_file])})
        for route in args.route:
            path = Path(route).resolve()
            report['checks'].append({'name': 'route', 'input': str(path),
                                     'sha256': digest(path), 'result': native(['route', map_file, path])})
        if args.capture:
            binary = ROOT / 'bin' / ('BE2.exe' if os.name == 'nt' else 'be2')
            images = directory / 'captures'
            with (directory / 'capture.log').open('w', encoding='utf-8') as log:
                run = subprocess.run([str(binary), '--capture-house', str(images), '--map', str(map_file)],
                                     cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, timeout=60)
            files = sorted(images.glob('blue-engine-*.png'))
            if run.returncode or len(files) != 12 or not (images / 'render-report.txt').is_file():
                raise ValueError('Capture incomplete; inspect capture.log')
            for path in files:
                if path.read_bytes()[:8] != b'\x89PNG\r\n\x1a\n':
                    raise ValueError('Capture is not PNG: ' + path.name)
            report['client_sha256'] = digest(binary)
            report['captures'] = {path.name: digest(path) for path in files}
            report['visual_status'] = 'review_required'
            if args.baseline:
                baseline = read(args.baseline)
                old = baseline.get('captures', {})
                if not baseline.get('ok') or not old:
                    raise ValueError('Baseline must be a successful verify report with captures')
                current = report['captures']
                report['changed_captures'] = sorted(k for k in set(old) | set(current) if old.get(k) != current.get(k))
                same_client = baseline.get('client_sha256') == report['client_sha256']
                report['same_client_as_baseline'] = same_client
                if not report['changed_captures'] and same_client:
                    report['visual_status'] = 'identical_to_baseline'
        report['ok'] = True
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        report['error'] = str(error)
    save_new(directory / 'report.json', report)
    return {'report': str(directory / 'report.json'), **report}


class Parser(argparse.ArgumentParser):
    def error(self, message):
        raise ValueError(message)


def main():
    parser = Parser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('describe')
    sub.add_parser('assets')
    sub.add_parser('recipes')
    q = sub.add_parser('query'); q.add_argument('text'); q.add_argument('--limit', type=int, default=5)
    s = sub.add_parser('schema'); s.add_argument('kind', choices=['patch'])
    n = sub.add_parser('new'); n.add_argument('output')
    a = sub.add_parser('add'); a.add_argument('map'); a.add_argument('template'); a.add_argument('output')
    a.add_argument('--id', required=True); a.add_argument('--at', default='0,0,0')
    v = sub.add_parser('verify'); v.add_argument('map'); v.add_argument('output')
    v.add_argument('--route', action='append', default=[]); v.add_argument('--capture', action='store_true')
    v.add_argument('--baseline')
    m = sub.add_parser('map'); m.add_argument('arguments', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    data = read(DATA)
    if args.command == 'describe':
        return describe(data)
    if args.command in ('assets', 'recipes'):
        return {args.command: data[args.command]}
    if args.command == 'query':
        if not 1 <= args.limit <= 20:
            raise ValueError('limit must be 1..20')
        return query(data, args.text, args.limit)
    if args.command == 'schema':
        return read(ROOT / 'tools' / 'patch.schema.json')
    if args.command == 'new':
        return native(['export-house', Path(args.output).resolve()])
    if args.command == 'add':
        ops = instantiate(data, args.template, args.id, vector(args.at))
        with tempfile.TemporaryDirectory(prefix='be2-author-') as temp:
            patch = Path(temp) / 'patch.json'
            save_new(patch, ops)
            return native(['apply', Path(args.map).resolve(), patch, Path(args.output).resolve()])
    if args.command == 'verify':
        if args.baseline and not args.capture:
            raise ValueError('--baseline requires --capture')
        return verify(args)
    if args.command == 'map':
        # Relative user paths have one consistent meaning: the caller's directory.
        # The native tool itself has no need for the engine working directory.
        allowed = {'audit', 'diff', 'select', 'near', 'ray', 'route', 'floorplan', 'export-scene', 'apply'}
        if not args.arguments or args.arguments[0] not in allowed:
            raise ValueError('Supported map operations: ' + ', '.join(sorted(allowed)))
        values = args.arguments[:]
        path_counts = {'diff': 2, 'route': 2, 'floorplan': 2, 'export-scene': 2, 'apply': 3}
        for i in range(1, min(len(values), 1 + path_counts.get(values[0], 1))):
            values[i] = str(Path(values[i]).resolve())
        return native(values)


if __name__ == '__main__':
    try:
        result = main()
        print(json.dumps({'protocol_version': VERSION, 'ok': True, **result}, allow_nan=False))
        sys.exit(0 if result.get('ok', True) else 1)
    except (OSError, ValueError, KeyError, subprocess.TimeoutExpired) as error:
        print(json.dumps({'protocol_version': VERSION, 'ok': False, 'error': str(error)}))
        sys.exit(1)
