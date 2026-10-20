"""Bind an exported game's Cargo inputs to the catalog's exact engine checkout."""
import argparse
import json
from pathlib import Path
import re
import tomllib


SOURCE_KEYS = {'path', 'git', 'rev', 'branch', 'tag'}


def bind(manifest: Path, engine: Path) -> None:
    manifest, engine = Path(manifest), Path(engine).resolve()
    if not (engine / 'Cargo.toml').is_file():
        raise ValueError(f'Missing catalog engine: {engine}')
    text = manifest.read_bytes().decode('utf-8')
    document = tomllib.loads(text)
    dependency = document.get('dependencies', {}).get('vesper3d')
    if (not isinstance(dependency, dict) or dependency.get('package') != 'be2'
            or 'workspace' in dependency or 'registry' in dependency
            or ('path' in dependency) == ('git' in dependency)):
        raise ValueError(f'{manifest}: expected one path or git vesper3d dependency on be2')
    source_key = 'git' if 'git' in dependency else 'path'
    if not isinstance(dependency[source_key], str):
        raise ValueError(f'{manifest}: unsupported engine source')
    pattern = re.compile(r'(?m)^([ \t]*vesper3d[ \t]*=[ \t]*\{)([^}\r\n]*)(\}[ \t]*(?:#[^\r\n]*)?)(?=\r?$)')
    matches = list(pattern.finditer(text))
    if len(matches) != 1:
        raise ValueError(f'{manifest}: expected exactly one inline vesper3d declaration')
    match = matches[0]
    body = re.sub(r'(?<![\w-])(?:path|git|rev|branch|tag)\s*=\s*"(?:[^"\\]|\\.)*"\s*,?', '', match[2])
    body = body.strip().rstrip(',').strip()
    replacement = match[1] + ' path = ' + json.dumps(engine.as_posix()) + ', ' + body + ' ' + match[3]
    bound = text[:match.start()] + replacement + text[match.end():]
    expected = {k: v for k, v in dependency.items() if k not in SOURCE_KEYS}
    expected['path'] = engine.as_posix()
    parsed = tomllib.loads(bound)
    if parsed['dependencies']['vesper3d'] != expected:
        raise ValueError(f'{manifest}: rebinding changed non-source dependency settings')
    parsed['dependencies']['vesper3d'] = dependency
    if parsed != document:
        raise ValueError(f'{manifest}: rebinding changed other manifest settings')

    lock = manifest.with_name('Cargo.lock')
    lock_text = lock.read_bytes().decode('utf-8') if lock.is_file() else None
    bound_lock = lock_text
    if source_key == 'git':
        if lock_text is None:
            raise ValueError(f'{manifest}: git engine needs a committed Cargo.lock')
        packages = tomllib.loads(lock_text).get('package', [])
        engines = [p for p in packages if p.get('name') == 'be2']
        if len(engines) != 1 or 'checksum' in engines[0]:
            raise ValueError(f'{lock}: expected one git be2 lock entry without a registry checksum')
        source = engines[0].get('source', '')
        if source.split('?', 1)[0].split('#', 1)[0] != 'git+' + dependency['git']:
            raise ValueError(f'{lock}: be2 lock source differs from the manifest')
        blocks = list(re.finditer(r'(?ms)^\[\[package\]\]\r?\n.*?(?=^\[\[package\]\]|\Z)', lock_text))
        blocks = [b for b in blocks if tomllib.loads(b[0])['package'][0].get('name') == 'be2']
        if len(blocks) != 1:
            raise ValueError(f'{lock}: ambiguous be2 lock block')
        block = blocks[0]
        replacement, count = re.subn(r'(?m)^source[ \t]*=[ \t]*"[^"\r\n]*"[ \t]*(?:\r?\n|$)', '', block[0])
        if count != 1:
            raise ValueError(f'{lock}: expected one engine source line')
        bound_lock = lock_text[:block.start()] + replacement + lock_text[block.end():]
        original_lock = tomllib.loads(lock_text)
        del next(p for p in original_lock['package'] if p['name'] == 'be2')['source']
        if tomllib.loads(bound_lock) != original_lock:
            raise ValueError(f'{lock}: rebinding changed other locked packages')

    # Validate every input before writing either file. The caller restores the byte snapshots.
    manifest.write_bytes(bound.encode('utf-8'))
    if bound_lock != lock_text:
        lock.write_bytes(bound_lock.encode('utf-8'))
    print(f'Bound {manifest.parent.name} to catalog engine {engine}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--engine', type=Path, required=True)
    arguments = parser.parse_args()
    bind(arguments.manifest, arguments.engine)
