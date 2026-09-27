#!/usr/bin/env python3
"""Validate this game's content and Rust targets; never run the dependency's test suite."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time


def console_options():
    """Hide a console tree without detaching Cargo from its test descendants."""
    if sys.platform != 'win32':
        return {}
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = subprocess.SW_HIDE
    return {'startupinfo': startup, 'creationflags': subprocess.CREATE_NEW_CONSOLE}


def commands(root, native, content_only=False, scenarios=()):
    game = root / 'game.json'
    document = json.loads(game.read_text(encoding='utf-8'))
    # GameDocument paths are relative to its containing directory, not the tool.
    map_path = (game.parent / document['map']).resolve()
    # The generated static client loads maps/main.json independently of game.json.
    # Validate both when a stock-runtime game selects a different map.
    maps = dict.fromkeys([(root / 'maps/main.json').resolve(), map_path])
    checks = []
    for path in maps:
        checks.extend([[str(native), 'audit', str(path)], [str(native), 'lint', str(path)]])
        # Run authored expectations when present. Without a checks block use the
        # native lint contract, not verify's implicit ten-warning policy (the
        # shipped two-room blueprint has advisory shared-wall warnings).
        if json.loads(path.read_text(encoding='utf-8')).get('checks') is not None:
            checks.append([str(native), 'verify', str(path)])
    checks.append([str(native), 'game-validate', str(game)])
    checks.extend([str(native), 'sim', str((root / scenario).resolve())] for scenario in scenarios)
    if not content_only:
        # cargo test compiles these same targets. A preceding cargo check repeats
        # analysis without adding a guarantee. Do not test the engine dependency.
        checks.append(['cargo', 'test', '--locked'])
    return checks


def run(root, native, content_only=False, scenarios=()):
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
    directory = root / '.blue-check' / stamp
    directory.mkdir(parents=True)
    report = {'ok': False, 'scope': 'content' if content_only else 'project',
              'checks': [], 'manual': 'Inspect world/menu captures and exercise changed controls.'}
    started = time.monotonic()
    try:
        binary = Path(native).resolve()
        report['native_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
        for index, command in enumerate(commands(root, binary, content_only, scenarios)):
            log = directory / f'{index + 1}.log'
            item = {'command': command, 'log': log.name, 'ok': False}
            report['checks'].append(item)
            with log.open('w', encoding='utf-8') as output:
                result = subprocess.run(command, cwd=root, stdout=output,
                                        stderr=subprocess.STDOUT, timeout=600,
                                        **console_options())
            item['ok'] = result.returncode == 0
            if not item['ok']:
                raise ValueError(f'Command failed ({result.returncode}); see {log}')
        report['ok'] = True
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        report['error'] = str(error)
    finally:
        report['elapsed_seconds'] = round(time.monotonic() - started, 3)
        destination = directory / 'report.json'
        destination.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
        print(json.dumps({'ok': report['ok'], 'scope': report['scope'], 'report': str(destination),
                          **({'error': report['error']} if 'error' in report else {})}))
    return report['ok']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tools', default=os.environ.get('BE2_TOOLS') or shutil.which('be2-tools'),
                        help='Matching be2-tools binary (or BE2_TOOLS / PATH)')
    parser.add_argument('--content-only', action='store_true',
                        help='Iteration check only; does not certify Rust changes')
    parser.add_argument('--scenario', action='append', default=[], help='Additional behavioral scenario')
    args = parser.parse_args()
    if not args.tools:
        parser.error('Set BE2_TOOLS to a matching native binary; build it once with '
                     'python tools/be2.py build tools in the engine checkout.')
    return 0 if run(Path(__file__).resolve().parents[1], args.tools,
                    args.content_only, args.scenario) else 1


if __name__ == '__main__':
    sys.exit(main())
