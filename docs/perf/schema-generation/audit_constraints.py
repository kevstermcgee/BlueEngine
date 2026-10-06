"""Reproduce the migration's structural constraint audit, not formal equivalence.

Run at repository root with --before cb5c8f1. Runtime fixtures and semantic
validation remain separate; annotation differences and extra typed bounds are allowed.
"""
import argparse
import json
from pathlib import Path
import subprocess


def audit(old, new):
    differences = []
    compared = 0
    visited = set()

    def resolve(value, root):
        if '$ref' not in value:
            return value
        target = root
        for part in value['$ref'][2:].split('/'):
            target = target[part]
        return {**target, **{k:v for k,v in value.items() if k != '$ref'}}

    def types(value, root):
        value = resolve(value, root)
        kind = value.get('type')
        if kind:
            return set(kind if isinstance(kind, list) else [kind])
        return set().union(*(types(v, root) for v in value.get('anyOf', [])))

    def nonnull(value, root):
        value = resolve(value, root)
        variants = [v for v in value.get('anyOf', []) if v.get('type') != 'null']
        if len(variants) == 1:
            return resolve({**variants[0], **{k:v for k,v in value.items() if k != 'anyOf'}}, root)
        return value

    def literals(value):
        if 'const' in value:
            return [value['const']]
        if 'enum' in value:
            return value['enum']
        variants = value.get('oneOf', [])
        result = [item for v in variants for item in literals(v)]
        return result if variants and len(result) == len(variants) else []

    def signature(value):
        return tuple((k, json.dumps(v.get('const'))) for k,v in value.get('properties', {}).items() if 'const' in v)

    def visit(a, b, path):
        nonlocal compared
        refs = (a.get('$ref'), b.get('$ref'))
        if all(refs):
            if refs in visited:
                return
            visited.add(refs)
        a, b = resolve(a, old), resolve(b, new)
        ta, tb = types(a, old), types(b, new)
        if ta and tb and not ta.issubset(tb):
            differences.append([path, 'type removed', sorted(ta), sorted(tb)])
        a, b = nonnull(a, old), nonnull(b, new)
        for key in ['minimum','maximum','exclusiveMinimum','exclusiveMaximum','minLength',
                    'maxLength','minItems','maxItems','maxProperties','pattern','format',
                    'additionalProperties','required','enum','const']:
            if key not in a:
                continue
            av, bv = a[key], b.get(key)
            if key == 'format' and av != 'uri':
                continue
            if key == 'additionalProperties' and isinstance(av, dict):
                visit(av, bv or {}, path+'/*')
                continue
            compared += 1
            if key == 'required':
                equal = isinstance(bv, list) and set(av) == set(bv)
            elif key in ('enum','const'):
                equal = set(map(json.dumps, literals(a))) == set(map(json.dumps, literals(b)))
            else:
                equal = av == bv
            if not equal:
                differences.append([path, key, av, bv])
        for name, value in a.get('properties', {}).items():
            visit(value, b.get('properties', {}).get(name, {}), path+'/'+name)
        if isinstance(a.get('items'), dict):
            visit(a['items'], b.get('items', {}), path+'[]')
        for key in ['oneOf','anyOf']:
            if key not in a or key not in b or literals(a) and literals(b):
                continue
            tagged = {signature(v):v for v in b[key] if signature(v)}
            for i, value in enumerate(a[key]):
                counterpart = tagged.get(signature(value)) if signature(value) else (b[key][i] if i<len(b[key]) else {})
                visit(value, counterpart or {}, path+'/'+key+'/'+str(i))
        if 'propertyNames' in a:
            visit(a['propertyNames'], b.get('propertyNames', {}), path+'/keys')

    visit(old, new, '')
    return {'constraints_compared': compared, 'differences': differences}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--before', required=True)
    args = parser.parse_args()
    reports = {}
    for path in ['tools/game.schema.json','tools/patch.schema.json','assets/asset-pack.schema.json']:
        old = json.loads(subprocess.check_output(['git','show',args.before+':'+path]))
        reports[path] = audit(old, json.loads(Path(path).read_text()))
    print(json.dumps({'before':args.before,'results':reports}, indent=2))
    raise SystemExit(any(report['differences'] for report in reports.values()))
