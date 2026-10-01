#!/usr/bin/env python3
"""BE2 development entry point. Python 3.10+, standard library only. No shell evaluation."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import zipfile

import upgrade
import workflow

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / '.be2-work'
SUFFIX = '.exe' if os.name == 'nt' else ''


class CommandFailure(RuntimeError):
    def __init__(self, packet, exit_code):
        super().__init__(packet['category'])
        self.packet = packet
        self.exit_code = exit_code
        self.reported = False


def invoke(args, *, env=None, log=None, capture=False, timeout=None, harness=None):
    args = list(map(str, args))
    if log:
        started = time.monotonic()
        evidence = None
        # Stream directly to disk: even a timeout/crash preserves complete output.
        with Path(log).open('wb') as output:
            try:
                with subprocess.Popen(args, cwd=ROOT, env=env, stdout=output,
                                      stderr=subprocess.STDOUT, start_new_session=os.name != 'nt',
                                      **workflow.console_options()) as process:
                    try:
                        returncode = process.wait(timeout=timeout)
                    except subprocess.TimeoutExpired:
                        # Stop descendants too: Cargo/test children must not keep
                        # running against the checkout after a timed-out command.
                        if os.name == 'nt':
                            subprocess.run(['taskkill', '/PID', str(process.pid), '/T', '/F'],
                                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                           **workflow.console_options())
                        else:
                            try:
                                os.killpg(process.pid, signal.SIGKILL)
                            except ProcessLookupError:
                                pass
                        process.kill()
                        process.wait()
                        evidence = {'returncode': None, 'category': 'timeout',
                                    'diagnostics': [f'Command exceeded {timeout}s; requested process-tree termination.']}
                        exit_code = 124
            except OSError as error:
                evidence = {'returncode': None, 'category': 'missing_tool' if isinstance(error, FileNotFoundError) else 'launch_error',
                            'diagnostics': [str(error)]}
                exit_code = 127 if isinstance(error, FileNotFoundError) else 126
        if evidence is None:
            evidence = workflow.command_evidence(Path(log), returncode, harness)
            exit_code = (returncode if returncode > 0 else 128 - returncode) if returncode else 3
        evidence.update(elapsed_seconds=round(time.monotonic() - started, 3),
                        log_bytes=Path(log).stat().st_size)
        if 'category' in evidence:
            evidence.update(command=args, reproduction=args, log=str(log))
            raise CommandFailure(evidence, exit_code)
        return evidence
    print('+ ' + ' '.join(map(str, args)), file=sys.stderr)
    buffered = capture or os.name == 'nt'
    result = subprocess.run(list(map(str, args)), cwd=ROOT, env=env,
                            stdout=subprocess.PIPE if buffered else None,
                            stderr=subprocess.STDOUT if buffered else None,
                            text=True, timeout=timeout, **workflow.console_options())
    if result.returncode:
        if result.stdout:
            print(result.stdout, file=sys.stderr)
        raise CommandFailure({'category': 'command_failure', 'command': args,
                              'returncode': result.returncode, 'log': None, 'reproduction': args},
                             result.returncode if result.returncode > 0 else 128 - result.returncode)
    if buffered and not capture and result.stdout:
        print(result.stdout, end='')
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
    result['disk'] = workflow.disk_report({
        'target': Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')),
        'cargo_home': Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo')),
        'temp': Path(tempfile.gettempdir())})
    result['ready_to_build'] = all(result['programs'][p] for p in ['cargo', 'rustc'])
    print(json.dumps(result, indent=2))
    if not result['ready_to_build']:
        raise RuntimeError('Rust toolchain is missing; see tools/README.md')


def check(plan, timeout=None):
    started = time.monotonic()
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
    directory = WORK / ('check-' + stamp)
    directory.mkdir(parents=True)
    report = {'ok': False, 'plan': plan, 'checks': []}
    try:
        for i, original in enumerate(plan['commands']):
            cmd = list(original)
            if (cmd[0] == 'cargo' and cmd[1] in {'test', 'check', 'clippy', 'build', 'rustdoc'}
                    and not any(arg.startswith('--message-format') for arg in cmd)):
                cmd.insert(cmd.index('--') if '--' in cmd else len(cmd), '--message-format=json')
            log = directory / f'{i+1}.log'
            item = {'command': cmd, 'log': log.name, 'ok': False}
            report['checks'].append(item)
            try:
                item.update(invoke(cmd, log=log, timeout=timeout, harness=plan.get('test_harness')))
            except CommandFailure as error:
                item.update(error.packet)
                report['failure'] = error.packet
                report['exit_code'] = error.exit_code
                error.reported = True
                raise
            item['ok'] = True
        report['ok'] = True
    finally:
        report['elapsed_seconds'] = round(time.monotonic() - started, 3)
        report['commands_attempted'] = len(report['checks'])
        report['failed_commands'] = sum(not item['ok'] for item in report['checks'])
        (directory / 'report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
        summary = {'report': str(directory / 'report.json'), 'ok': report['ok'],
                   'scope': plan['scope'], 'seconds': report['elapsed_seconds'],
                   'commands': report['commands_attempted']}
        if plan['scope'] == 'iteration':
            summary.update(feature=plan['feature'], feature_mode=plan['feature_mode'],
                           proves=plan['proves'] if report['ok'] else None, remaining=plan['remaining'])
            summary['tests_executed'] = sum(item.get('tests_executed', 0) for item in report['checks']) if plan.get('test_harness') else None
        if 'failure' in report:
            summary.update(failure=report['failure'], exit_code=report['exit_code'])
        print(json.dumps(summary))


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
    c = sub.add_parser('check')
    c.add_argument('--changed', action='store_true', help='Select checks from the complete Git diff')
    c.add_argument('--base', default='HEAD', help='Compare current files against this commit (default HEAD)')
    c.add_argument('--plan', action='store_true', help='Print the plan without running checks')
    c.add_argument('--iterate', metavar='FEATURE', help='Focused iteration only; never final validation')
    c.add_argument('--typecheck', action='store_true', help='Iteration: engine library type-check only')
    c.add_argument('--test', metavar='SUITE[::EXACT_TEST]', help='Iteration: one indexed suite or exact regression')
    c.add_argument('--feature-mode', choices=['headless', 'default'], default=None,
                   help='Iteration Cargo features (default: normal Cargo features; headless is explicit)')
    c.add_argument('--profile', choices=workflow.TEST_PROFILES, default='itest', dest='test_profile',
                   help='Cargo profile for tests (default itest: optimized dependencies, much faster; dev: plain)')
    c.add_argument('--timeout', type=float, help='Per-command timeout in seconds; logs survive timeouts')
    c = sub.add_parser('context', help='Bounded feature context without a native build or source reads')
    c.add_argument('query'); c.add_argument('--limit', type=int, default=3)
    c.add_argument('--compact', action='store_true', help='Compact JSON; same bounded packet')
    c.add_argument('--record', action='store_true', help='Save packet size/timing locally for workflow measurement')
    b = sub.add_parser('build'); b.add_argument('kind', choices=['client', 'headless', 'tools', 'all'])
    c = sub.add_parser('capture'); c.add_argument('destination'); c.add_argument('--map')
    p = sub.add_parser('package'); p.add_argument('destination')
    t = sub.add_parser('map'); t.add_argument('arguments', nargs=argparse.REMAINDER)
    sub.add_parser('features')
    u = sub.add_parser('upgrade', help='Plan and verify moving an external game to a chosen engine revision')
    uc = u.add_subparsers(dest='upgrade_command', required=True)
    up = uc.add_parser('plan', help='Read-only: baseline/target identity, runtime, applicable migrations')
    up.add_argument('game', help='Path to the external game directory (never modified)')
    up.add_argument('--to', required=True, metavar='REF', help='Target engine revision (branch, tag or commit)')
    up.add_argument('--engine-checkout', help='Resolve --to here instead of the game-resolved engine dependency')
    up.add_argument('--fix', action='append', default=[], metavar='ID:DESCRIPTION',
                    help='Requested problem to track through the upgrade; repeatable')
    up.add_argument('--adopt', action='append', default=[], metavar='MIGRATION_ID',
                    help='Explicitly select an optional_adoption migration; repeatable')
    up.add_argument('--out', help='Write the full JSON packet here (never written without this flag)')
    up.add_argument('--json', action='store_true', help='Print the full JSON packet instead of the human summary')
    uv = uc.add_parser('verify', help='Reruns the game\'s own scripts/check.py fresh; never trusts a stale report')
    uv.add_argument('game', help='Path to the external game directory')
    uv.add_argument('--skip-ship', action='store_true')
    uv.add_argument('--content-only', action='store_true')
    uv.add_argument('--scenario', action='append', default=[], help='Additional behavioral scenario; repeatable')
    uv.add_argument('--timeout', type=float, default=600)
    uv.add_argument('--out', help='Write the full JSON result here (never written without this flag)')
    uv.add_argument('--json', action='store_true', help='Print the full JSON result instead of the human summary')
    args = parser.parse_args()
    if args.command == 'doctor': doctor()
    elif args.command == 'check':
        if args.timeout is not None and (not 0 < args.timeout < float('inf')):
            parser.error('--timeout must be a finite positive number')
        if args.iterate:
            if args.changed or args.base != 'HEAD':
                parser.error('--iterate cannot replace --changed or --base final checks')
            plan = workflow.iteration_plan(ROOT, args.iterate, typecheck=args.typecheck,
                                           test=args.test, feature_mode=args.feature_mode or 'default',
                                           test_profile=args.test_profile)
            if args.plan: print(json.dumps(plan, indent=2))
            else: check(plan, args.timeout)
            return
        if args.typecheck or args.test or args.feature_mode:
            parser.error('--typecheck, --test and --feature-mode require --iterate')
        if not args.changed and args.base != 'HEAD':
            parser.error('--base requires --changed')
        revision, paths = workflow.changed_paths(ROOT, args.base) if args.changed else (None, None)
        plan = workflow.validation_plan(paths, revision, args.test_profile)
        if paths is not None:
            plan['impact'] = workflow.impact(ROOT, paths)
        if args.plan: print(json.dumps(plan, indent=2))
        else: check(plan, args.timeout)
    elif args.command == 'context':
        started = time.monotonic()
        packet = workflow.context(ROOT, args.query, args.limit)
        output = json.dumps(packet, separators=(',', ':')) if args.compact else json.dumps(packet, indent=2)
        if args.record:
            WORK.mkdir(exist_ok=True)
            with (WORK / 'context-metrics.jsonl').open('a', encoding='utf-8') as stream:
                stream.write(json.dumps({'query': args.query, 'bytes': len(output.encode('utf-8')),
                                         'elapsed_seconds': round(time.monotonic() - started, 4),
                                         'engine_source_files_opened': 0,
                                         'documentation_files_opened': 0}) + '\n')
        print(output)
    elif args.command == 'build':
        for kind in ['client', 'headless', 'tools'] if args.kind == 'all' else [args.kind]:
            print(build(kind))
    elif args.command == 'capture': capture(args.destination, args.map)
    elif args.command == 'package': package(args.destination)
    elif args.command == 'map': tool(args.arguments)
    elif args.command == 'features': print((ROOT / 'tools/FEATURES.json').read_text(encoding='utf-8'))
    elif args.command == 'upgrade':
        if args.upgrade_command == 'plan':
            packet = upgrade.plan(args.game, ROOT, args.to, engine_checkout=args.engine_checkout,
                                  fixes=args.fix, adopt=args.adopt)
            if args.out:
                Path(args.out).write_text(json.dumps(packet, indent=2) + '\n', encoding='utf-8')
            print(json.dumps(packet, indent=2) if args.json else packet['human_summary'])
        elif args.upgrade_command == 'verify':
            result = upgrade.verify(args.game, ROOT, skip_ship=args.skip_ship, content_only=args.content_only,
                                    scenarios=args.scenario, timeout=args.timeout)
            if args.out:
                Path(args.out).write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
            print(json.dumps(result, indent=2) if args.json else result.get('human_summary', json.dumps(result)))
            if not result['ok']:
                sys.exit(1)


if __name__ == '__main__':
    try:
        main()
    except CommandFailure as error:
        if not error.reported:
            print(json.dumps({'ok': False, 'failure': error.packet, 'exit_code': error.exit_code}), file=sys.stderr)
        sys.exit(error.exit_code)
    except (RuntimeError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(json.dumps({'ok': False, 'error': str(error)}), file=sys.stderr)
        sys.exit(1)
