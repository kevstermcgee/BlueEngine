#!/usr/bin/env python3
"""BE2 context and canonical local/CI checks with timestamp-safe report directories, stage timings and failure logs."""
import argparse
import datetime
from concurrent.futures import ThreadPoolExecutor
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
try:
    import resource
except ImportError:  # Windows does not expose POSIX child usage.
    resource = None

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
            evidence['recovery'] = workflow.recovery(evidence)
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


def target_directory():
    path = Path(os.environ.get('CARGO_TARGET_DIR') or ROOT / 'target')
    return (path if path.is_absolute() else ROOT / path).resolve()


def environment(kind):
    env = os.environ.copy()
    # Keep feature-isolated outputs; never mix a graphics-enabled headless binary into a package.
    base = target_directory()
    target = base / ('be2-' + kind)
    env['CARGO_TARGET_DIR'] = str(target)
    return env, target


def build(kind):
    env, target = environment(kind)
    args = ['cargo', 'build', '--release', '--locked']
    if kind == 'client':
        args += ['--bin', 'be2']
    elif kind == 'cinematic':
        args += ['--no-default-features', '--features', 'offline', '--bin', 'vesper3d']
    else:
        args += ['--no-default-features', '--bin', 'be2-headless' if kind == 'headless' else 'be2-tools']
    invoke(args, env=env)
    return target / 'release'


def tool(args):
    # Share fresh headless authoring output with check_authoring; Cargo owns invalidation.
    # Isolated release builds remain the distribution/packaging path.
    output = invoke(['cargo', 'build', '--locked', '--profile', 'itest', '--no-default-features', '--bin', 'be2-tools'], capture=True)
    if output:
        print(output, end='', file=sys.stderr)
    target = target_directory()
    binary = target / 'itest' / ('be2-tools' + SUFFIX)
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


def check(plan, timeout=None, task=None):
    started = time.monotonic()
    usage_before = resource.getrusage(resource.RUSAGE_CHILDREN) if resource else None
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
    WORK.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix='check-' + stamp + '-', dir=WORK))
    report = {'ok': False, 'plan': plan, 'checks': []}
    binding = None
    if task:
        import springboard
        binding = springboard.bind_check(ROOT, task, plan)
    env = {**os.environ, **plan['env']} if plan.get('env') else None
    report['target_directory'] = str(target_directory())

    def execute(i, original):
        cmd = list(original)
        if (cmd[0] == 'cargo' and cmd[1] in {'test', 'check', 'clippy', 'build', 'rustdoc'}
                and not any(arg.startswith('--message-format') for arg in cmd)):
            cmd.insert(cmd.index('--') if '--' in cmd else len(cmd), '--message-format=json')
        log = directory / f'{i+1}.log'
        label = f'[{i+1}/{len(plan["commands"])}] ' + ' '.join(map(str, original))
        print(label, file=sys.stderr, flush=True)
        item = {'step': i + 1, 'command': cmd, 'log': log.name, 'ok': False,
                'started_seconds': round(time.monotonic() - started, 3)}
        report['checks'].append(item)
        try:
            harness = (plan['command_harnesses'][i] if 'command_harnesses' in plan else plan.get('test_harness'))
            # Separate game manifests otherwise create separate target directories,
            # rebuilding the same engine/dependencies once per game. Keep their
            # original profiles/features: Cargo fingerprints decide actual reuse.
            # The independent Python fixtures retain their own output directories.
            command_env = env
            if cmd[0] == 'cargo' or cmd[1:2] == ['tools/check_authoring.py']:
                command_env = {**(env or os.environ), 'CARGO_TARGET_DIR': str(target_directory())}
            item.update(invoke(cmd, log=log, env=command_env, timeout=timeout, harness=harness))
        except CommandFailure as error:
            item.update(error.packet)
            print(f'[{i+1}/{len(plan["commands"])}] FAILED: {error.packet["category"]}; log: {log}',
                  file=sys.stderr, flush=True)
            raise
        item['ok'] = True
        print(f'[{i+1}/{len(plan["commands"])}] PASS ({item["elapsed_seconds"]:.3f}s)',
              file=sys.stderr, flush=True)

    # One reviewed independent Python batch overlaps the serial Cargo lane.
    # Caller-supplied native binaries can opt into integration within that batch;
    # keep those calls serial to avoid touching a tool while Cargo replaces it.
    independent = plan.get('independent_commands', []) if not os.environ.get('BE2_TOOLS') else []
    if len(independent) > 1 or any(i not in range(len(plan['commands'])) for i in independent):
        raise ValueError('Check plans support at most one valid independent command.')
    report['independent_commands'] = independent
    try:
        # Context exit joins in-flight work even on failure; reports are written
        # only after all started processes have exited, with no detached checks.
        with ThreadPoolExecutor(max_workers=1) as pool:
            future = (pool.submit(execute, independent[0], plan['commands'][independent[0]])
                      if independent else None)
            for i, original in enumerate(plan['commands']):
                if i in independent:
                    continue
                if future is not None and future.done():
                    future.result() # Stop pending gates if the independent lane failed.
                execute(i, original)
            if future is not None:
                future.result()
        report['ok'] = True
    except CommandFailure as error:
        report['failure'] = error.packet
        report['exit_code'] = error.exit_code
        error.reported = True
        raise
    finally:
        report['checks'].sort(key=lambda item: item['step'])
        report['elapsed_seconds'] = round(time.monotonic() - started, 3)
        report['commands_attempted'] = len(report['checks'])
        report['failed_commands'] = sum(not item['ok'] for item in report['checks'])
        report['cargo_artifacts'] = {key: sum(item.get('cargo_artifacts', {}).get(key, 0) for item in report['checks'])
                                     for key in ('fresh', 'built')}
        if resource:
            usage = resource.getrusage(resource.RUSAGE_CHILDREN)
            report['resources'] = {'child_cpu_seconds': round(usage.ru_utime + usage.ru_stime -
                                                             usage_before.ru_utime - usage_before.ru_stime, 3),
                                   'max_child_rss_bytes': int(usage.ru_maxrss * (1 if sys.platform == 'darwin' else 1024)),
                                   'rss_basis': 'OS child high-water mark; not simultaneous whole-machine memory'}
        report['successful_steps'] = [item['step'] for item in report['checks'] if item['ok']]
        if binding:
            try:
                springboard.finish_check(ROOT, binding, plan, report, directory / 'report.json')
            except (OSError, ValueError, KeyError) as error:
                # Identity/state failures must never hide completed checks or their failure logs.
                report['task_evidence'] = {'task': task, 'stable_sources': False, 'error': str(error)}
                print('Task evidence unverified: ' + str(error), file=sys.stderr)
        (directory / 'report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
        summary = {'report': str(directory / 'report.json'), 'ok': report['ok'],
                   'scope': plan['scope'], 'seconds': report['elapsed_seconds'],
                   'commands': report['commands_attempted']}
        summary['cargo_artifacts'] = report['cargo_artifacts']
        if 'resources' in report:
            summary['resources'] = report['resources']
        if plan['scope'] in ('focused', 'game_shipping'):
            summary.update(loop=plan['loop'], proves=plan['proves'] if report['ok'] else None,
                           remaining=plan['remaining'], tests_executed=sum(item.get('tests_executed', 0) for item in report['checks']))
        if plan['scope'] == 'iteration':
            summary.update(feature=plan['feature'], feature_mode=plan['feature_mode'],
                           proves=plan['proves'] if report['ok'] else None, remaining=plan['remaining'])
            summary['tests_executed'] = sum(item.get('tests_executed', 0) for item in report['checks']) if plan.get('test_harness') else None
        if plan['scope'] == 'windows_typecheck':
            summary['proves'] = plan['proves'] if report['ok'] else None
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
    for folder, name, output in [(client, 'be2', 'BE2'),
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
    for command in ('start', 'next', 'resume'):
        s = sub.add_parser(command, help='Build-free task routing/progress around canonical tools')
        s.add_argument('--json', action='store_true', help='Versioned structured packet')
        s.add_argument('--compact', action='store_true', help='Compact JSON packet')
        s.add_argument('--detail', action='store_true', help='JSON including ownership, complete evidence and identity')
        if command == 'start':
            s.add_argument('objective')
            s.add_argument('--kind', choices=['new-game', 'change-game', 'engine', 'diagnose', 'upgrade'])
            s.add_argument('--project', '--game', dest='project')
            s.add_argument('--target', action='append', default=[], help='Requested platform; repeatable')
            s.add_argument('--template', help='Explicit name from templates/starters.json')
            s.add_argument('--networking', choices=['offline', 'native-multiplayer'])
            s.add_argument('--name', help='Explicit portable game ID')
            s.add_argument('--path', action='append', default=[], help='Engine-relative change scope; repeatable')
            s.add_argument('--constraint', action='append', default=[])
            s.add_argument('--no-save', action='store_true', help='Read-only packet; no task metadata')
        else:
            s.add_argument('task', help='ID from start, in this engine checkout')
            s.add_argument('--note', help='Context only; never passing evidence')
            s.add_argument('--no-save', action='store_true', help='Refresh without modifying task metadata')
    sub.add_parser('doctor')
    c = sub.add_parser('check')
    c.add_argument('--changed', action='store_true', help='Select checks from the complete Git diff')
    c.add_argument('--loop', choices=['inner', 'integration', 'shipping'], default='shipping',
                   help='Automatic change checks; shipping remains the default')
    c.add_argument('--path', action='append', default=[], help='Explicit path to check (repeatable; inner/integration only)')
    c.add_argument('--game', help='Standalone game project; reuse its native tests/package gates, excluding dependency tests')
    c.add_argument('--task', help='Bind observed check report to a springboard task/current inputs')
    c.add_argument('--base', default='HEAD', help='Compare current files against this commit (default HEAD)')
    c.add_argument('--windows', action='store_true',
                   help='Type-check cfg(windows) code for x86_64-pc-windows-gnu without a Windows C toolchain; '
                        'not a Windows run (see docs/CHANGE_WORKFLOW.md)')
    c.add_argument('--plan', action='store_true', help='Print the plan without running checks')
    c.add_argument('--serial', action='store_true', help='Run full gates in sequence (debugging/comparison)')
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
    c.add_argument('--level', type=int, choices=[1, 2, 3], default=2,
                   help='1: commands/guides; 2: contracts; 3: ownership/implementation map')
    b = sub.add_parser('build'); b.add_argument('kind', choices=['client', 'headless', 'tools', 'cinematic', 'all'])
    c = sub.add_parser('capture'); c.add_argument('destination'); c.add_argument('--map')
    p = sub.add_parser('package'); p.add_argument('destination')
    t = sub.add_parser('map'); t.add_argument('arguments', nargs=argparse.REMAINDER)
    t = sub.add_parser('schemas', help='Opt-in Rust schema generation/parity and fixture verification')
    t.add_argument('arguments', nargs=argparse.REMAINDER)
    f = sub.add_parser('features')
    f.add_argument('--feature', help='Derived implementation/interface/schema/test/example map for one feature')
    f.add_argument('--validate', action='store_true', help='Check indexed paths, dependency edges and test suites without a build')
    web = sub.add_parser('web', help='Retired browser command: migration diagnostic only'); web.add_argument('arguments', nargs=argparse.REMAINDER)
    pub = sub.add_parser('publish', help='Retired browser publication: use native shipping'); pub.add_argument('arguments', nargs=argparse.REMAINDER)
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
    if len(sys.argv) > 1 and sys.argv[1] == 'schemas':
        import schemas
        raise SystemExit(schemas.main(sys.argv[2:]))
    args = parser.parse_args()
    if args.command in ('start', 'next', 'resume'):
        import springboard
        if args.command == 'start':
            packet = springboard.start(ROOT, args.objective, kind=args.kind, project=args.project,
                                       targets=args.target, template=args.template, networking=args.networking,
                                       paths=args.path, name=args.name, constraints=args.constraint,
                                       detail=args.detail, persist=not args.no_save)
        else:
            packet = springboard.resume(ROOT, args.task, args.note, args.detail, persist=not args.no_save)
        print(json.dumps(packet, separators=(',', ':')) if args.compact else
              json.dumps(packet, indent=2) if args.json or args.detail else springboard.readable(packet))
    elif args.command in ('web', 'publish'):
        from web_games import main as web_main
        raise SystemExit(web_main((['publish'] if args.command == 'publish' else []) + args.arguments))
    elif args.command == 'doctor': doctor()
    elif args.command == 'check':
        if args.timeout is not None and (not 0 < args.timeout < float('inf')):
            parser.error('--timeout must be a finite positive number')
        if args.windows:
            if args.iterate or args.changed or args.path or args.game or args.loop != 'shipping' or args.base != 'HEAD' or args.typecheck or args.test or args.feature_mode:
                parser.error('--windows is its own check; do not combine it with --iterate/--changed/--base/--typecheck/--test/--feature-mode')
            plan = workflow.windows_plan()
            if args.plan: print(json.dumps(plan, indent=2))
            else: check(plan, args.timeout, args.task)
            return
        if args.iterate:
            if args.changed or args.path or args.game or args.loop != 'shipping' or args.base != 'HEAD':
                parser.error('--iterate cannot replace --changed or --base final checks')
            plan = workflow.iteration_plan(ROOT, args.iterate, typecheck=args.typecheck,
                                           test=args.test, feature_mode=args.feature_mode or 'default',
                                           test_profile=args.test_profile)
            if args.plan: print(json.dumps(plan, indent=2))
            else: check(plan, args.timeout, args.task)
            return
        if args.typecheck or args.test or args.feature_mode:
            parser.error('--typecheck, --test and --feature-mode require --iterate')
        if args.game:
            if args.changed or args.path or args.base != 'HEAD':
                parser.error('--game checks that project only; do not combine with engine diff/path selections')
            plan = workflow.game_plan(ROOT, args.game, args.loop)
            if args.plan: print(json.dumps(plan, indent=2))
            else: check(plan, args.timeout, args.task)
            return
        if not args.changed and args.base != 'HEAD':
            parser.error('--base requires --changed')
        if args.path and (args.changed or args.loop == 'shipping'):
            parser.error('--path is an explicit inner/integration selection; use --changed for final shipping checks')
        if args.loop != 'shipping' and not (args.changed or args.path):
            parser.error('Automatic inner/integration checks require --changed or --path')
        revision, paths = workflow.changed_paths(ROOT, args.base) if args.changed else (None, None)
        if args.path:
            paths = args.path
        plan = (workflow.validation_plan(paths, revision, args.test_profile) if args.loop == 'shipping' else
                workflow.change_plan(ROOT, paths, revision, loop=args.loop, test_profile=args.test_profile))
        if args.serial:
            plan['independent_commands'] = []
        if paths is not None:
            plan['impact'] = workflow.impact(ROOT, paths)
        if args.plan: print(json.dumps(plan, indent=2))
        else: check(plan, args.timeout, args.task)
    elif args.command == 'context':
        started = time.monotonic()
        packet = workflow.context(ROOT, args.query, args.limit, args.level)
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
    elif args.command == 'features':
        if args.validate:
            result = workflow.validate_index(ROOT)
            print(json.dumps(result))
            if not result['ok']:
                sys.exit(1)
        elif args.feature:
            print(json.dumps(workflow.feature_map(ROOT, args.feature), indent=2))
        else:
            print((ROOT / 'tools/FEATURES.json').read_text(encoding='utf-8'))
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
