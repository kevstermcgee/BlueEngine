#!/usr/bin/env python3
"""Place data-only interior prefabs and validate with the packaged native engine."""
import argparse
import copy
import json
import math
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
LIBRARY = ROOT / 'assets' / 'props' / 'interiors'


def place(source, asset_id, output, instance_id, origin, angle):
    catalog = json.loads((LIBRARY / 'catalog.json').read_text(encoding='utf-8'))
    spec = next((a for a in catalog if a['id'] == asset_id), None)
    if spec is None:
        raise ValueError('Unknown interior asset; use --list')
    if not re.fullmatch(r'[A-Za-z0-9_/-]{1,80}', instance_id):
        raise ValueError('Instance ID must contain only letters, numbers, underscores, hyphens and slashes')
    if len(origin) != 3 or not all(math.isfinite(v) for v in origin):
        raise ValueError('Origin requires three finite coordinates')
    if output.exists():
        raise ValueError('Output must be a new file')
    doc = json.loads(source.read_text(encoding='utf-8'))
    ids = [n['id'] for n in doc['scene']['nodes']] + list(doc['colliders']) + [e['id'] for e in doc['entities']]
    if any(i == instance_id or i.startswith(instance_id + '/') for i in ids):
        raise ValueError('Instance ID is already occupied')
    scene = json.loads((LIBRARY / spec['scene']).read_text(encoding='utf-8'))
    for key, value in scene['materials'].items():
        if key in doc['scene']['materials'] and doc['scene']['materials'][key] != value:
            raise ValueError('Material conflict: ' + key)
        doc['scene']['materials'][key] = value
    rad = math.radians(angle)
    c, s = math.cos(rad), math.sin(rad)
    for i, original in enumerate(scene['nodes']):
        n = copy.deepcopy(original)
        if n['rot'][0] or n['rot'][2]:
            raise ValueError('This placement helper supports prefab parts with Y-only rotation')
        x, y, z = n['pos']
        n['pos'] = [origin[0] + x*c + z*s, origin[1] + y, origin[2] - x*s + z*c]
        n['rot'][1] += angle
        n['id'] = f'{instance_id}/{i}'
        doc['scene']['nodes'].append(n)
    x, y, z = spec['half_extents']
    ext = [abs(c)*x + abs(s)*z, y, abs(s)*x + abs(c)*z]
    center = [origin[0], origin[1] + y, origin[2]]
    bounds = {'min': [center[i] - ext[i] for i in range(3)],
              'max': [center[i] + ext[i] for i in range(3)]}
    doc['colliders'][instance_id] = bounds
    doc['entities'].append({'id': instance_id, 'label': spec['label'], 'bounds': bounds, 'action': 'inspect'})
    work = ROOT / '.be2-work'
    work.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='interior-', dir=work) as temp:
        draft = Path(temp) / 'draft.json'
        data = json.dumps(doc, indent=2)
        draft.write_text(data, encoding='utf-8')
        run = subprocess.run([str(ROOT / 'bin' / 'be2-tools.exe'), 'audit', str(draft)],
                             capture_output=True, text=True, timeout=60)
        audit = json.loads(run.stdout or run.stderr)
        if run.returncode or not audit.get('ok') or not audit.get('spawn_clear'):
            raise ValueError('Native audit failed: ' + json.dumps(audit))
        with output.open('x', encoding='utf-8') as file:
            file.write(data)
    return {'ok': True, 'output': str(output.resolve()), 'audit': audit}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--list', action='store_true')
    parser.add_argument('map', type=Path, nargs='?')
    parser.add_argument('asset', nargs='?')
    parser.add_argument('output', type=Path, nargs='?')
    parser.add_argument('--id')
    parser.add_argument('--at', default='0,0,0')
    parser.add_argument('--yaw', type=int, choices=[0, 90, 180, 270], default=0)
    args = parser.parse_args()
    try:
        if args.list:
            result = {'ok': True, 'assets': json.loads((LIBRARY / 'catalog.json').read_text(encoding='utf-8'))}
        else:
            if not all([args.map, args.asset, args.output, args.id]):
                raise ValueError('Supply MAP ASSET OUTPUT --id ID [--at X,Y,Z] [--yaw DEGREES]')
            result = place(args.map, args.asset, args.output, args.id, [float(v) for v in args.at.split(',')], args.yaw)
        print(json.dumps(result))
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        print(json.dumps({'ok': False, 'error': str(error)}))
        raise SystemExit(1)


if __name__ == '__main__':
    main()
