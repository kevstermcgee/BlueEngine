#!/usr/bin/env python3
"""Keep the human-written GitHub README in step with declared project scope."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
RECEIPT = Path('docs/README_SCOPE.json')
POLICIES = (
    'docs/adr/0045-windows-only-game-distribution.md',
    'docs/adr/0049-retire-browser-gameplay.md',
)
# Implementation navigation and verification edits do not change public scope.
INTERNAL_FIELDS = {
    'files', 'checks', 'tests', 'evidence', 'read_first', 'examples',
    'canonical_example', 'iterate',
}


def fingerprint(value):
    encoded = json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False)
    return hashlib.sha256(encoded.encode('utf-8')).hexdigest()


def read_json(path):
    try:
        return json.loads(path.read_text(encoding='utf-8'))
    except ValueError as error:
        raise ValueError(f'{path}: {error}') from error


def snapshot(root):
    root = Path(root)
    index = read_json(root / 'tools/FEATURES.json')
    features = index.get('features') if isinstance(index, dict) else None
    if not isinstance(features, dict) or any(not isinstance(v, dict) for v in features.values()):
        raise ValueError('tools/FEATURES.json: expected a mapping of capability records')
    try:
        manifest = tomllib.loads((root / 'Cargo.toml').read_text(encoding='utf-8'))
    except tomllib.TOMLDecodeError as error:
        raise ValueError(f'Cargo.toml: {error}') from error
    for table in ('package', 'features'):
        if not isinstance(manifest.get(table, {}), dict):
            raise ValueError(f'Cargo.toml: expected [{table}] to be a table')
    return {
        'format': 1,
        'readme_sha256': hashlib.sha256((root / 'README.md').read_bytes()).hexdigest(),
        'capabilities': {
            name: fingerprint({k: v for k, v in record.items() if k not in INTERNAL_FIELDS})
            for name, record in features.items()
        },
        'cargo_features': {name: fingerprint(value) for name, value in manifest.get('features', {}).items()},
        'rust_version': manifest.get('package', {}).get('rust-version'),
        'starters_sha256': fingerprint(read_json(root / 'templates/starters.json')),
        'distribution_policy': {
            name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in POLICIES
        },
    }


def previous(root):
    path = Path(root) / RECEIPT
    if not path.exists():
        return None
    value = read_json(path)
    if not isinstance(value, dict) or value.get('format') != 1:
        raise ValueError(f'{RECEIPT}: unsupported receipt format (expected 1)')
    for key in ('capabilities', 'cargo_features', 'distribution_policy'):
        if not isinstance(value.get(key), dict) or any(not isinstance(v, str) for v in value[key].values()):
            raise ValueError(f'{RECEIPT}: invalid {key} fingerprints')
    for key in ('readme_sha256', 'starters_sha256'):
        if not isinstance(value.get(key), str):
            raise ValueError(f'{RECEIPT}: missing {key}')
    return value


def changes(old, new, include_readme=True):
    found = []
    for key, label in (('capabilities', 'capability'), ('cargo_features', 'Cargo feature'),
                       ('distribution_policy', 'distribution policy')):
        before, after = old[key], new[key]
        found.extend(f'{label} added: {name}' for name in sorted(after.keys() - before.keys()))
        found.extend(f'{label} removed: {name}' for name in sorted(before.keys() - after.keys()))
        found.extend(f'{label} changed: {name}' for name in sorted(before.keys() & after.keys())
                     if before[name] != after[name])
    for key, label in (('rust_version', 'minimum Rust version'), ('starters_sha256', 'starter catalog')):
        if old.get(key) != new[key]:
            found.append(f'{label} changed')
    if include_readme and old['readme_sha256'] != new['readme_sha256']:
        found.append('README text changed since its last scope review')
    return found


def errors(root=ROOT):
    try:
        old, current = previous(root), snapshot(root)
        if old is None:
            return [f'README scope receipt missing: {RECEIPT}; run python3 tools/readme_scope.py update']
        drift = changes(old, current)
        if drift:
            return ['README scope review needed: ' + '; '.join(drift),
                    'Review README.md, then run python3 tools/readme_scope.py update. '
                    'If its description still fits, record why with --unchanged-reason TEXT.']
        return []
    except (OSError, ValueError) as error:
        return [f'README scope check: {error}']


def update(root=ROOT, unchanged_reason=None):
    root = Path(root)
    old, current = previous(root), snapshot(root)
    reason = unchanged_reason.strip() if unchanged_reason is not None else None
    if unchanged_reason is not None and not reason:
        raise ValueError('An unchanged reason must not be empty')
    if (old and changes(old, current, include_readme=False)
            and old['readme_sha256'] == current['readme_sha256'] and not reason):
        raise ValueError('Scope inputs changed but README.md is unchanged. Revise its description '
                         'or record a specific --unchanged-reason after reviewing it.')
    if old and unchanged_reason is None and not changes(old, current):
        reason = old.get('unchanged_reason')
    current['unchanged_reason'] = reason
    path = root / RECEIPT
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(current, indent=2, sort_keys=True) + '\n', encoding='utf-8')
    return current


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('check', 'update'), nargs='?', default='check')
    parser.add_argument('--unchanged-reason', help='Why the existing README still describes changed scope inputs')
    args = parser.parse_args()
    if args.action == 'check':
        problems = errors()
        print('\n'.join(problems) if problems else 'README scope: current')
        return bool(problems)
    try:
        update(unchanged_reason=args.unchanged_reason)
    except (OSError, ValueError) as error:
        print(f'README scope update: {error}', file=sys.stderr)
        return 1
    print(f'Recorded README scope review: {RECEIPT}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
