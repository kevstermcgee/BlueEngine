#!/usr/bin/env python3
"""Generate/check committed authoring schemas through opt-in native Rust tooling."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import uuid
try:
    from .task_inputs import engine_identity
except ImportError:
    from task_inputs import engine_identity

ROOT = Path(__file__).resolve().parents[1]
FILES = {'game': 'tools/game.schema.json', 'patch': 'tools/patch.schema.json',
         'asset-pack': 'assets/asset-pack.schema.json', 'mcp': 'tools/mcp.schemas.json'}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    operation = parser.add_mutually_exclusive_group(required=True)
    operation.add_argument('--write', action='store_true', help='Generate all outputs; write only changed bytes')
    operation.add_argument('--check', action='store_true', help='Run generation parity and valid/invalid fixture tests')
    parser.add_argument('--plan', action='store_true', help='Print commands without running/installing/building')
    args = parser.parse_args(argv)
    command = (['cargo', 'test', '--locked', '--profile', 'itest', '--no-default-features',
                '--features', 'schema-validation', '--test', 'authoring_schemas', '-j', '2'] if args.check else
               ['cargo', 'build', '--locked', '--message-format=json-render-diagnostics', '--profile', 'itest', '--no-default-features',
                '--features', 'schema-generation', '--bin', 'be2-tools', '-j', '2'])
    packet = {'schema_version': 1, 'action': 'check' if args.check else 'write',
              'cwd': str(ROOT), 'argv': command, 'files': list(FILES.values()),
              'semantic_gate': 'Schema validity does not replace game-validate, asset validation, scenarios or shipping checks.'}
    if args.plan:
        print(json.dumps({**packet, 'status': 'planned', 'builds_triggered': 0}, indent=2))
        return 0
    started = time.monotonic()
    log = ROOT / '.be2-work' / 'schemas' / (packet['action'] + '-' + uuid.uuid4().hex + '.log')
    packet['log'] = str(log)
    def run(argv):
        result = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True, encoding='utf-8')
        with log.open('a', encoding='utf-8') as output:
            output.write(json.dumps({'argv': argv, 'returncode': result.returncode}) + '\n')
            output.write((result.stdout or '') + (result.stderr or ''))
        result.check_returncode()
        return result
    try:
        log.parent.mkdir(parents=True, exist_ok=True)
        before = engine_identity(ROOT)
        if not before.get('sha256'):
            raise ValueError(before.get('error', 'Cannot identify current source inputs'))
        # Cargo owns freshness and dependency resolution; no independent cache or installation.
        result = run(command)
        binary = None
        if args.write:
            # Use this Cargo invocation's artifact, including configured target directories.
            # Never guess a binary path and accidentally execute a stale host binary.
            for line in result.stdout.splitlines():
                artifact = json.loads(line)
                if (artifact.get('reason') == 'compiler-artifact' and
                        artifact.get('target', {}).get('name') == 'be2-tools' and
                        'bin' in artifact['target']['kind'] and artifact.get('executable')):
                    binary = artifact['executable']
            if not binary:
                raise ValueError('Cargo did not report the be2-tools executable; use a native host target.')
            generated = {}
            for kind, path in FILES.items():
                result = run([str(binary), 'schema-generate', kind])
                # Parse everything before touching any previous output.
                json.loads(result.stdout)
                generated[path] = result.stdout.encode('utf-8')
        after = engine_identity(ROOT)
        if before != after:
            raise ValueError('Source inputs changed during generation/checking. Outputs were preserved; retry on stable inputs.')
        packet['input_identity'] = before
        if args.write:
            changed, unchanged = [], []
            for name, data in generated.items():
                path = ROOT / name
                if path.is_file() and path.read_bytes() == data:
                    unchanged.append(name)
                    continue
                with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as output:
                    output.write(data)
                    temp = Path(output.name)
                try:
                    temp.chmod(path.stat().st_mode & 0o777 if path.exists() else 0o644)
                    os.replace(temp, path)
                finally:
                    if temp.exists():
                        temp.unlink()
                changed.append(name)
            packet.update(changed=changed, unchanged=unchanged)
        packet.update(status='passed', seconds=time.monotonic() - started)
    except (OSError, ValueError, subprocess.CalledProcessError) as exc:
        packet.update(status='failed', error=str(exc),
                      stderr=(getattr(exc, 'stderr', None) or '')[-2000:],
                      next='Inspect the failing Rust type/constraint or fixture; use a native host target and retry on stable inputs. Generation failures preserve previous schema outputs.')
        print(json.dumps(packet, indent=2))
        return 1
    print(json.dumps(packet, indent=2))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
