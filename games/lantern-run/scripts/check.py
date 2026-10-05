#!/usr/bin/env python3
"""Validate this game's content and Rust targets; never run the dependency's test suite."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

MANUAL = ('Inspect world/menu captures and exercise changed controls: no automated check can see or play the '
          'game. dist/ship.json records the launch and smoke evidence of the last "python scripts/ship.py ship" '
          '(window title, window icon, capture sizes); a human or agent still has to look at the captures and '
          'try the controls.')
ENGINE_CRATES = ('be2', 'vesper3d')


def console_options():
    """Hide a console tree without detaching Cargo from its test descendants."""
    if sys.platform != 'win32':
        return {}
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = subprocess.SW_HIDE
    return {'startupinfo': startup, 'creationflags': subprocess.CREATE_NEW_CONSOLE}


LOCK_COMMAND = ['cargo', 'metadata', '--format-version', '1', '--quiet']


def step_name(command):
    """The name a report gives a step: the native subcommand, or lock / test for the two cargo steps."""
    if command[0] == 'cargo':
        return {'metadata': 'lock', 'test': 'test'}.get(command[1], command[1])
    return command[1]


def commands(root, native, content_only=False, scenarios=()):
    game = root / 'game.json'
    main_map = (root / 'maps/main.json').resolve()
    if game.is_file():
        document = json.loads(game.read_text(encoding='utf-8'))
        # GameDocument paths are relative to its containing directory, not the tool.
        map_path = (game.parent / document['map']).resolve()
        # The generated static client loads maps/main.json independently of game.json.
        # Validate both when a stock-runtime game selects a different map.
        maps = dict.fromkeys([main_map, map_path])
    else:
        # Custom-simulation games have no GameDocument: validate the map only if there is one.
        document = None
        maps = dict.fromkeys([main_map] if main_map.is_file() else [])
    checks = []
    for path in maps:
        checks.extend([[str(native), 'audit', str(path)], [str(native), 'lint', str(path)]])
        # Run authored expectations when present. Without a checks block use the
        # native lint contract, not verify's implicit ten-warning policy (the
        # shipped two-room blueprint has advisory shared-wall warnings).
        if json.loads(path.read_text(encoding='utf-8')).get('checks') is not None:
            checks.append([str(native), 'verify', str(path)])
    if document is not None:
        checks.append([str(native), 'game-validate', str(game)])
    checks.extend([str(native), 'sim', str((root / scenario).resolve())] for scenario in scenarios)
    if not content_only:
        # A scaffold seeds Cargo.lock from the engine's own lock (the versions the engine was tested with).
        # That lock is stale for the game: its package entry is missing and it still lists crates only the
        # engine's dev-dependencies need, so `cargo test --locked` would fail on it. `cargo metadata` settles
        # the lock (creating one when there is none) and keeps the pinned versions. Never use
        # `cargo generate-lockfile` here: it discards the pins.
        checks.append(list(LOCK_COMMAND))
        # cargo test compiles these same targets. A preceding cargo check repeats
        # analysis without adding a guarantee. Do not test the engine dependency.
        checks.append(['cargo', 'test', '--locked'])
    return checks


def engine_path(root):
    """The engine checkout named by a path dependency on be2 (renamed vesper3d) in Cargo.toml, or None."""
    manifest = Path(root) / 'Cargo.toml'
    try:
        text = manifest.read_text(encoding='utf-8-sig')
    except OSError:
        return None
    candidates = []
    try:
        import tomllib
        document = tomllib.loads(text)
        tables = [document.get('dependencies') or {}, document.get('build-dependencies') or {},
                  document.get('dev-dependencies') or {}]
        tables.extend((target or {}).get('dependencies') or {} for target in (document.get('target') or {}).values())
        for table in tables:
            for key, spec in table.items():
                if isinstance(spec, dict) and isinstance(spec.get('path'), str) and (
                        key in ENGINE_CRATES or spec.get('package') in ENGINE_CRATES):
                    candidates.append(spec['path'])
    except (ImportError, ValueError):
        # Python 3.10 has no tomllib: read the inline-table form that `new-game` writes.
        for match in re.finditer(r'^\s*"?([\w-]+)"?\s*=\s*\{([^}\n]*)\}', text, re.M):
            body = match.group(2)
            path = re.search(r'\bpath\s*=\s*["\']([^"\']+)["\']', body)
            package = re.search(r'\bpackage\s*=\s*["\']([^"\']+)["\']', body)
            if path and (match.group(1) in ENGINE_CRATES or (package and package.group(1) in ENGINE_CRATES)):
                candidates.append(path.group(1))
    for candidate in candidates:
        found = (Path(root) / candidate).resolve()
        if found.is_dir():
            return found
    return None


def engine_warnings(root):
    """Warn (never fail) when assets/identity.json records another engine revision than the checkout is at."""
    try:
        recorded = json.loads((Path(root) / 'assets/identity.json').read_text(encoding='utf-8-sig')).get('engine_revision')
        engine = engine_path(root)
        if not isinstance(recorded, str) or not recorded.strip() or engine is None or not shutil.which('git'):
            return []
        done = subprocess.run(['git', '-C', str(engine), 'rev-parse', 'HEAD'], capture_output=True, text=True,
                              timeout=30)
        head = (done.stdout or '').strip().lower()
        if done.returncode != 0 or not re.fullmatch(r'[0-9a-f]{40,64}', head):
            return []
        if not head.startswith(recorded.strip().lower()):
            return [f'engine_revision {recorded.strip()} in assets/identity.json, but the engine checkout at {engine} '
                    f'is at {head[:12]}: rebuild against the recorded engine or update the identity']
    except (OSError, ValueError, AttributeError, subprocess.SubprocessError):
        pass
    return []


def parse_json_output(text):
    """The JSON object a command printed: the whole text, else its last line that parses."""
    for candidate in [text or ''] + list(reversed((text or '').splitlines())):
        candidate = candidate.strip()
        if candidate.startswith('{'):
            try:
                value = json.loads(candidate)
            except ValueError:
                continue
            if isinstance(value, dict):
                return value
    return None


def ship_stage(root, report, directory, index):
    """Run `scripts/ship.py verify --json`; the game is not done until its shortcut and package verify."""
    ship = root / 'scripts/ship.py'
    command = [sys.executable, str(ship), 'verify', '--json']
    log = directory / f'{index + 1}.log'
    item = {'name': 'ship', 'command': command, 'log': log.name, 'ok': False}
    report['checks'].append(item)
    result = subprocess.run(command, cwd=root, capture_output=True, text=True, encoding='utf-8', errors='replace',
                            timeout=600, **console_options())
    log.write_text((result.stdout or '') + '\n--- stderr ---\n' + (result.stderr or ''), encoding='utf-8')
    verdict = parse_json_output(result.stdout)
    fix = 'Run: python scripts/ship.py ship'
    if verdict is None:
        report['ship'], report['ship_status'] = 'fail', 'fail'
        raise ValueError(f'ship gate: scripts/ship.py verify printed no JSON (exit {result.returncode}); see {log}. {fix}')
    report['ship'] = verdict
    for name in verdict.get('skipped') or []:
        report['skipped'].append(f'ship.{name}')
    for check in verdict.get('checks') or []:
        if check.get('status') == 'warn':
            report['warnings'].append(f'ship.{check.get("name")}: {check.get("detail")}')
    item['ok'] = result.returncode == 0 and bool(verdict.get('ok'))
    if item['ok']:
        report['ship_status'] = 'pass'
        return
    report['ship_status'] = 'fail'
    failing = next((c for c in verdict.get('checks') or [] if c.get('status') == 'fail'), None)
    if failing is not None:
        raise ValueError(f'ship gate: {failing.get("name")}: {failing.get("detail")}. {fix}')
    raise ValueError(f'ship gate: {verdict.get("error") or "verify failed"} (exit {result.returncode}). {fix}')


def run(root, native, content_only=False, scenarios=(), skip_ship=False):
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
    directory = root / '.blue-check' / stamp
    directory.mkdir(parents=True)
    report = {'ok': False, 'scope': 'content' if content_only else 'project',
              'checks': [], 'warnings': [], 'skipped': [], 'manual': MANUAL}
    # The ship stage runs last; until it does, the report says why it did not (or could not) run.
    gate = not content_only and not skip_ship and (root / 'scripts/ship.py').is_file()
    if content_only:
        report['ship'], report['ship_status'] = 'not run: content-only', 'skipped: content-only'
    elif skip_ship:
        report['ship'], report['ship_status'] = 'skipped: --skip-ship', 'skipped: --skip-ship'
    elif not gate:
        report['ship'], report['ship_status'] = 'skipped: no scripts/ship.py', 'skipped: no scripts/ship.py'
    else:
        report['ship'], report['ship_status'] = 'not run: an earlier check failed', 'skipped: an earlier check failed'
    if isinstance(report['ship'], str):
        report['skipped'].append(f'ship: {report["ship"]}')
    started = time.monotonic()
    try:
        binary = Path(native).resolve()
        report['native_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
        report['warnings'].extend(engine_warnings(root))
        plan = commands(root, binary, content_only, scenarios)
        for index, command in enumerate(plan):
            log = directory / f'{index + 1}.log'
            name = step_name(command)
            item = {'name': name, 'command': command, 'log': log.name, 'ok': False}
            report['checks'].append(item)
            with log.open('w', encoding='utf-8') as output:
                # `cargo metadata` prints the whole dependency graph on stdout: only its errors belong in the log.
                stdout, stderr = (subprocess.DEVNULL, output) if name == 'lock' else (output, subprocess.STDOUT)
                result = subprocess.run(command, cwd=root, stdout=stdout, stderr=stderr, timeout=600,
                                        **console_options())
            item['ok'] = result.returncode == 0
            if not item['ok']:
                raise ValueError(f'Command failed ({result.returncode}); see {log}')
        if gate:
            report['skipped'].remove(f'ship: {report["ship"]}')
            ship_stage(root, report, directory, len(plan))
        report['ok'] = True
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        report['error'] = str(error)
    finally:
        report['elapsed_seconds'] = round(time.monotonic() - started, 3)
        destination = directory / 'report.json'
        destination.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
        print(json.dumps({'ok': report['ok'], 'scope': report['scope'], 'ship': report['ship_status'],
                          'report': str(destination),
                          **({'skipped': report['skipped']} if report['skipped'] else {}),
                          **({'warnings': report['warnings']} if report['warnings'] else {}),
                          **({'error': report['error']} if 'error' in report else {})}))
    return report['ok']


def find_tools(root):
    """A built be2-tools: BE2_TOOLS, then PATH, then the engine checkout this game depends on (its target
    directory, release/fast/debug). Returns None when there is none, so the caller can say how to build one."""
    found = os.environ.get('BE2_TOOLS') or shutil.which('be2-tools')
    if found:
        return found
    manifest = (Path(root) / 'Cargo.toml')
    if not manifest.is_file():
        return None
    match = re.search(r'^vesper3d\s*=\s*\{[^}]*?path\s*=\s*"([^"]+)"', manifest.read_text(encoding='utf-8'), re.M)
    if not match:
        return None
    engine = (Path(root) / match.group(1)).resolve()
    suffix = '.exe' if os.name == 'nt' else ''
    targets = [Path(os.environ['CARGO_TARGET_DIR'])] if os.environ.get('CARGO_TARGET_DIR') else []
    for base in targets + [engine / 'target']:
        for profile in ('release', 'fast', 'debug', 'be2-headless/release'):
            candidate = base / profile / ('be2-tools' + suffix)
            if candidate.is_file():
                return str(candidate)
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tools', default=find_tools(Path(__file__).resolve().parents[1]),
                        help='Matching be2-tools binary (default: BE2_TOOLS, PATH, then the engine checkout target dir)')
    parser.add_argument('--content-only', action='store_true',
                        help='Iteration check only; does not certify Rust changes')
    parser.add_argument('--scenario', action='append', default=[], help='Additional behavioral scenario')
    parser.add_argument('--skip-ship', action='store_true',
                        help='Full check without the ship gate (shortcut, package and icon verification)')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    if (root / 'game.project.json').exists():
        import importlib.util
        helper = root / 'scripts/project.py'
        if not helper.is_file():
            parser.error('game.project.json needs scripts/project.py; refresh generated project tooling')
        spec = importlib.util.spec_from_file_location('game_requirements', helper)
        requirements = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(requirements)
        try:
            requirements.validate_project(root)
        except (ValueError, OSError) as error:
            parser.error(str(error))
    if not args.tools:
        parser.error('No be2-tools found. Build one in the engine checkout, then rerun (it is found there '
                     'automatically): cargo build --profile fast --no-default-features --bin be2-tools '
                     '(or python tools/be2.py build tools). Or set BE2_TOOLS to its path.')
    return 0 if run(Path(__file__).resolve().parents[1], args.tools,
                    args.content_only, args.scenario, args.skip_ship) else 1


if __name__ == '__main__':
    sys.exit(main())
