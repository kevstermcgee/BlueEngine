"""Thin, build-free task coordinator; existing commands remain the execution front door."""
import datetime
import json
import os
from pathlib import Path
import re
import shutil
import sys
import tempfile
import tomllib
import uuid

try:
    from . import task_inputs, workflow, web_games
except ImportError:
    import task_inputs, workflow, web_games

VERSION = 1
KINDS = ('new-game', 'change-game', 'engine', 'diagnose', 'upgrade')
INVARIANTS = [
    'Preserve authoritative fixed-step simulation and rendering-free headless operation.',
    'Preserve transport limits, acknowledgement contracts and public vesper3d compatibility.',
    'Keep custom simulation, presentation, shaders, physics and networking escape hatches.',
    'Focused checks are iteration evidence; complete required target/CI gates before delivery.',
    'Schema validity, behavior, visuals/input and packaging are separate evidence.',
    'Notes are context, not passing evidence. Discovery never installs, builds or launches.',
]


def catalog(root):
    data = json.loads((root / 'templates/starters.json').read_text(encoding='utf-8'))
    if data.get('schema_version') != VERSION or data['cli_default'] not in data['starters']:
        raise ValueError('Invalid starter catalog; inspect templates/starters.json')
    return data


def state_path(root, task):
    if not re.fullmatch('[a-f0-9]{12}', task):
        raise ValueError('Task ID must be the 12 hexadecimal characters returned by start')
    return root / '.be2-work/tasks' / (task + '.json')


def load(root, task):
    state = json.loads(state_path(root, task).read_text(encoding='utf-8'))
    if state.get('schema_version') != VERSION or state.get('id') != task:
        raise ValueError('Unsupported or mismatched task state; use the recorded engine checkout')
    if state['engine'] != str(root.resolve()):
        raise ValueError('Task belongs to another engine checkout; use its recorded be2.py')
    return state


def save(root, state):
    path = state_path(root, state['id'])
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode='w', encoding='utf-8', dir=path.parent, delete=False) as stream:
        json.dump(state, stream, indent=2)
        stream.write('\n')
        temporary = stream.name
    os.replace(temporary, path)


def action(root, *args, expected, kind='command', files=None):
    return {'kind': kind, 'cwd': str(root), 'argv': [sys.executable, str(root / 'tools/be2.py'), *args],
            'expected': expected, **({'files': files} if files else {})}


def git_state(root, calls, detail=False):
    rev = task_inputs.probe(['git', 'rev-parse', 'HEAD'], root, calls)
    status = task_inputs.probe(['git', 'status', '--porcelain', '--untracked-files=all'], root, calls)
    paths = status['output'].splitlines() if status['ok'] else []
    return {'revision': rev['output'] if rev['ok'] else None,
            'working_tree': {'state': 'dirty' if paths else 'clean' if status['ok'] else 'unknown',
                             'count': len(paths), 'paths': paths if detail else paths[:8],
                             'omitted': 0 if detail else max(0, len(paths) - 8)}}


def select(root, task, kind, project, targets, template, networking):
    text = task.lower()
    data = catalog(root)
    if template and template not in data['starters']:
        raise ValueError('Unknown template; use ' + ', '.join(data['starters']))
    uncertainty, gaps = [], []
    if kind is None:
        if re.search(r'\b(upgrade|migrate)\b', text):
            kind = 'upgrade'
        elif re.search(r'\b(diagnose|diagnosis|failure|failed|error)\b', text):
            kind = 'diagnose'
        elif re.search(r'\b(create|build|make|new)\b.*\bgame\b', text):
            kind = 'new-game'
        elif project:
            kind = 'change-game'
        elif re.search(r'\b(engine|crate|replication|renderer|tooling)\b', text):
            kind = 'engine'
        else:
            uncertainty.append('Task kind is ambiguous; specify --kind. No implementation route was guessed.')
    network = networking or ('native-multiplayer' if re.search(r'\b(multiplayer|quic|udp|online)\b', text) else 'offline')
    if not targets:
        if project and (project / 'game.project.json').is_file():
            try:
                targets = web_games.validate_project(project)['targets']
            except (ValueError, OSError) as error:
                uncertainty.append(str(error))
        if not targets:
            targets = ['web'] if re.search(r'\b(browser|web|mobile)\b', text) else []
    targets = list(dict.fromkeys(targets))
    host = {'linux': 'linux', 'win32': 'windows', 'darwin': 'macos'}.get(sys.platform)
    targets = [host if t == 'native' else 'web' if t == 'browser' else t for t in targets]
    if kind == 'new-game':
        if template is None:
            if 'web' not in targets and re.search(r'\b(gamedocument|declarative|counters|interactables|timers)\b', text):
                template = 'stock'
            elif 'web' in targets or network == 'offline' and not targets:
                template = 'two-d' if re.search(r'\b2d\b|two-d|two dimensional', text) else data['cli_default']
            elif re.search(r'\b(gamedocument|declarative|counters|interactables|timers)\b', text):
                template = 'stock'
            else:
                template = 'custom-sim' if network != 'offline' or re.search(r'\b(enemies|projectiles|physics|scoring|ai)\b', text) else 'stock'
        starter = data['starters'][template]
        targets = targets or starter.get('default_targets', [host])
        if set(targets) - set(starter['targets']) or network not in starter['networking']:
            gaps.append({'kind': 'workflow_not_coordinated' if 'headless' in targets else 'unsupported_combination',
                         'detail': f'{template} does not support targets {targets} with {network}. No requested feature was dropped.',
                         'extension': 'docs/NETPLAY.md' if network != 'offline' else 'docs/PORTABLE_GAMES.md',
                         'next': 'Choose an explicit supported native route or an engine engineering task for the missing combination.'})
        if 'headless' in targets:
            gaps[-1]['detail'] = 'The engine has rendering-free headless simulation/server APIs; this game starter does not scaffold a headless application. No capability was removed.'
            gaps[-1]['extension'] = 'docs/NETPLAY.md'
        if template == 'stock' and re.search(r'\b(enemies|projectiles)\b|per.frame physics|custom gameplay scripts', text):
            gaps.append({'kind': 'unsupported_combination', 'detail': 'Stock GameDocument counters/interactables/timers do not supply enemy/projectile/custom per-tick rules.',
                         'extension': 'docs/CUSTOM_SIM_CHEATSHEET.md', 'next': 'Select custom-sim explicitly or extend GameDocument through engine maintenance; retain the requested mechanics.'})
        if 'web' in targets and re.search(r'\brapier\b|native world|worker api', text):
            gaps.append({'kind': 'unsupported_combination', 'detail': 'Native Rapier/world/worker APIs are not available in the browser runtime.',
                         'extension': 'docs/PORTABLE_GAMES.md', 'next': 'Use supported portable primitives or request an engine extension without dropping the requested behavior.'})
        if 'web' in targets and re.search(r'\b(gamedocument|declarative)\b', text):
            gaps.append({'kind': 'unsupported_combination', 'detail': 'GameDocument authoring is native; the portable browser starter owns Rust rules.',
                         'extension': 'docs/GAME_QUICKSTART.md', 'next': 'Clarify authoring/runtime requirements or extend the engine; do not silently replace declarative rules.'})
        runtime, feature = starter['runtime'], starter['feature']
    else:
        runtime, feature = None, None
        if project and (project / 'game.project.json').is_file():
            try:
                p = web_games.validate_project(project)
                runtime = p.get('runtime', 'portable' if p['presentation'] == '2d' else 'legacy-native')
                feature = 'two_dimensional' if runtime == 'portable' else 'game_documents'
                if targets and set(targets) != set(p['targets']):
                    uncertainty.append('Requested targets differ from game.project.json; edit requirements deliberately before checks.')
                if networking or re.search(r'\b(multiplayer|quic|udp|online)\b', text):
                    if network != p['networking']:
                        uncertainty.append('Requested networking differs from game.project.json; preserve the request and select an engineering route explicitly.')
                else:
                    network = p['networking']
            except (ValueError, OSError) as error:
                uncertainty.append(str(error))
        targets = targets or (['headless'] if kind == 'engine' else [host])
    coordinated = kind in ('new-game', 'engine') or kind == 'change-game' and runtime == 'portable'
    limitations = [] if coordinated else ['This route delegates to existing tools; portable project and engine checks supply coordinated stages.']
    return {'kind': kind, 'template': template, 'runtime': runtime, 'targets': targets,
            'networking': network, 'feature': feature, 'uncertainty': uncertainty, 'gaps': gaps,
            'coordinated': coordinated, 'limitations': limitations}


def readiness(root, route, identity, calls):
    checks = []
    def add(name, passed, phase, next_command, detail=None):
        checks.append({'name': name, 'state': 'passed' if passed else 'unverified' if passed is None else 'failed',
                       'phase': phase, 'next': next_command, **({'detail': detail} if detail else {})})
    tools = identity['tools']
    for name in ('compiler', 'cargo'):
        add(name, tools[name]['ok'], 'inner', 'Install/select Rust toolchain explicitly; then be2.py doctor', tools[name].get('error'))
    version = re.search(r'rustc (\d+)\.(\d+)', tools['compiler']['output'])
    manifest = root / 'Cargo.toml'
    minimum = tomllib.loads(manifest.read_text(encoding='utf-8')).get('package', {}).get('rust-version', '1.87') if manifest.is_file() else '1.87'
    add('Rust >= ' + minimum, bool(version and tuple(map(int, version.groups())) >= tuple(map(int, minimum.split('.')[:2]))), 'inner', 'Select the engine-supported Rust toolchain')
    linker = bool(shutil.which('cc') or shutil.which('clang') or shutil.which('cl.exe'))
    add('host C linker', linker if linker or sys.platform != 'win32' else None, 'inner',
        'be2.py doctor; inspect/install host build tools explicitly', 'MSVC can be discovered by Cargo outside PATH; an unverified result does not certify it.' if not linker and sys.platform == 'win32' else None)
    targets = route['targets']
    if 'web' in targets:
        sysroot = tools['sysroot']['output'] if tools['sysroot']['ok'] else ''
        std = Path(sysroot) / 'lib/rustlib/wasm32-unknown-unknown/lib'
        add('wasm32 standard library', bool(sysroot and list(std.glob('libstd-*'))), 'shipping', 'be2.py web prepare GAME')
        add('Node', bool(shutil.which('node')), 'shipping', 'Install Node explicitly; see docs/BROWSER_WORKFLOW.md')
        browser = os.environ.get('BE2_CHROMIUM') or next((shutil.which(n) for n in ('chromium', 'chromium-browser', 'google-chrome', 'google-chrome-stable') if shutil.which(n)), None)
        add('Chromium', bool(browser and (Path(browser).is_file() or shutil.which(browser))), 'shipping', 'Set BE2_CHROMIUM or install Chromium explicitly')
        ws = task_inputs.probe(['node', '--input-type=module', '-e', 'console.log(import.meta.resolve("ws"))'], root / 'tools', calls)
        add('Node ws', ws['ok'], 'shipping', 'be2.py web prepare GAME', ws.get('error'))
    if sys.platform == 'linux' and ('linux' in targets or route['kind'] == 'engine'):
        headers = task_inputs.probe(['pkg-config', '--exists', 'alsa', 'libudev'], root, calls)
        add('Linux presentation headers', headers['ok'], 'shipping', 'See native prerequisites in tools/README.md', headers.get('error'))
    if sys.platform == 'win32' and 'windows' in targets:
        add('Windows SDK resource compiler', True if shutil.which('rc.exe') else None, 'shipping', 'Inspect/select the Windows SDK; project ship gates must still run')
    host = {'linux': 'linux', 'win32': 'windows', 'darwin': 'macos'}.get(sys.platform)
    for target in targets:
        if target not in ('web', 'headless', host):
            add(target + ' packaging/runtime', None, 'shipping', 'Run native project/CI gates on ' + str(target), 'Host readiness does not certify another OS.')
    return {'checks': checks, 'ready_to_attempt_inner': all(c['state'] != 'failed' for c in checks if c['phase'] == 'inner'),
            'unverified': ['Dependency cache, compilation and device behavior remain unverified until the existing gates execute.']}


def start(root, objective, *, kind=None, project=None, targets=None, template=None, networking=None,
          paths=None, name=None, constraints=None, detail=False, persist=True):
    if not objective.strip() or len(objective) > 8000 or any(ord(c) < 32 and c not in '\n\t' for c in objective):
        raise ValueError('Objective needs 1..8000 printable characters; essential constraints are never truncated')
    root = Path(root).resolve()
    project = Path(project).resolve() if project else None
    if paths and any(Path(p).is_absolute() or '..' in Path(p).parts for p in paths):
        raise ValueError('--path needs an engine-relative path without ..')
    route = select(root, objective, kind, project, targets or [], template, networking)
    task = uuid.uuid4().hex[:12]
    name = name or (project.name if project else 'task-' + task)
    if route['kind'] == 'new-game':
        if not re.fullmatch('[a-z][a-z0-9-]{0,47}', name):
            raise ValueError('Supply --name with a portable game ID: [a-z][a-z0-9-]{0,47}')
        project = project or root / '.be2-work/games' / name
    calls = []
    git = git_state(root, calls, detail)
    inputs = task_inputs.capture(root, project, calls)
    state = {'schema_version': VERSION, 'id': task, 'objective': objective,
             'constraints': INVARIANTS + (constraints or []), 'engine': str(root),
             'base_revision': git['revision'], 'project': str(project) if project else None,
             'name': name, 'route': route, 'paths': paths or [], 'initial_inputs': inputs,
             'reports': [], 'notes': [], 'created': datetime.datetime.now(datetime.timezone.utc).isoformat()}
    packet = refresh(root, state, detail=detail, observed=(calls, git, inputs))
    if persist:
        save(root, state)
        packet['state_file'] = str(state_path(root, task))
    return packet


def report_status(root, state, inputs):
    stages = {name: {'state': 'unverified'} for name in ('inner', 'integration', 'shipping')}
    history = []
    for path in state['reports']:
        try:
            report = json.loads(Path(path).read_text(encoding='utf-8'))
            binding = report.get('task_evidence', {})
            loop = report['plan'].get('loop', 'shipping' if report['plan']['scope'] == 'full' else None)
            status = 'passed' if report['ok'] else 'failed'
            current = binding.get('task') == state['id'] and binding.get('after') == inputs and binding.get('stable_sources') is True
            if not current:
                status = 'unverified'
            elif report.get('skipped'):
                status = 'skipped'
            elif report['ok']:
                if len(report['checks']) != len(report['plan']['commands']) or not all(c.get('ok') for c in report['checks']):
                    status = 'unverified'
                for i, check in enumerate(report['checks']):
                    harnesses = report['plan'].get('command_harnesses')
                    harness = harnesses[i] if harnesses else report['plan'].get('test_harness')
                    result = workflow.command_evidence(Path(path).parent / check['log'], check['returncode'], harness)
                    if result.get('category'):
                        status = 'unverified'
                if loop == 'shipping' and report['plan'].get('game'):
                    for check in report['checks']:
                        if check['command'][:2] == ['cargo', 'metadata']:
                            continue  # Dependency inventory is not a behavioral/shipping gate.
                        text = (Path(path).parent / check['log']).read_text(encoding='utf-8', errors='replace')
                        payload = None
                        for line in reversed(text.splitlines()):
                            try:
                                value = json.loads(line)
                            except ValueError:
                                continue
                            if isinstance(value, dict) and 'ok' in value:
                                payload = value
                                break
                        if payload is None or payload.get('ok') is not True:
                            status = 'unverified'
                        elif payload.get('skipped') or str(payload.get('ship', '')).startswith(('skipped', 'not run')):
                            status = 'skipped'
                        elif payload.get('browser') is False:
                            status = 'skipped'
            item = {'stage': loop, 'state': status, 'report': path, 'current_inputs': current,
                    'previous_result': 'passed' if report['ok'] else 'failed'}
            if report.get('failure'):
                item['failure'] = report['failure']
            history.append(item)
            if loop in stages:
                stages[loop] = item
        except (OSError, ValueError, KeyError, TypeError) as error:
            history.append({'state': 'unverified', 'report': path, 'detail': str(error)})
    return stages, history


def refresh(root, state, *, detail=False, observed=None):
    root = Path(root).resolve()
    calls, git, inputs = observed or ([], None, None)
    if git is None:
        git = git_state(root, calls, detail)
        inputs = task_inputs.capture(root, state['project'], calls)
    route = state['route']
    owners = workflow.impact(root, state['paths'])['owners'] if state['paths'] else []
    query = route.get('feature') or (owners[0] if len(owners) == 1 else ' '.join(state['objective'].split())[:500])
    context = workflow.context(root, query, limit=1, level=3 if detail else 1)
    refs = list(dict.fromkeys(p for m in context['matches'] for p in m.get('read_first', [])))
    if route.get('template'):
        refs.insert(0, catalog(root)['starters'][route['template']]['guide'])
    if route['networking'] != 'offline':
        refs.insert(0, 'docs/NETPLAY.md')
    refs = list(dict.fromkeys(refs))[:3]
    stages, history = report_status(root, state, inputs)
    ready = readiness(root, route, inputs, calls)
    project = Path(state['project']) if state['project'] else None
    blockers = list(route['uncertainty']) + [g['detail'] for g in route['gaps']]
    if context['confidence'].startswith('low') and route['kind'] not in ('diagnose', 'upgrade'):
        blockers.append('Context ranking has low confidence; confirm ownership with an exact feature ID or --path before editing.')
    schema = {'state': 'unverified'}
    if project and (project / 'game.project.json').exists():
        try:
            config = web_games.validate_project(project)
            schema = {'state': 'passed', 'proves': 'Project schema only'}
            if set(config['targets']) != set(route['targets']):
                blockers.append('Set game.project.json targets to ' + json.dumps(route['targets']) + ' deliberately before verification.')
        except (ValueError, OSError) as error:
            schema = {'state': 'failed', 'detail': str(error)}
            blockers.append(str(error))
    if project and (project / 'Cargo.toml').exists():
        try:
            manifest = tomllib.loads((project / 'Cargo.toml').read_text(encoding='utf-8'))
            dependencies = manifest.get('dependencies', {})
            engine = next((v.get('path') for k, v in dependencies.items() if isinstance(v, dict) and (k == 'vesper3d' or v.get('package') == 'be2')), None)
            if not isinstance(engine, str) or (project / engine).resolve() != root:
                blockers.append('Game dependency does not statically resolve to this engine checkout; use the existing upgrade plan before coordinating checks.')
        except (ValueError, OSError) as error:
            blockers.append('Inspect ' + str(project / 'Cargo.toml') + ': ' + str(error))
    next_action = action(root, 'context', query, '--level', '2', '--compact',
                         expected='Read the selected contract and implement the objective; use --kind/--path to resolve uncertainty.')
    iteration = None
    integration = None
    final = {'state': 'unverified', 'requirements': ['Complete applicable Linux/Windows CI when engine inputs change.',
             'Inspect changed visuals/controls; physical devices/audio are not certified by emulation.',
             'Execute every declared target gate; publication also requires public-source reproduction and deployment receipts.']}
    if route['kind'] == 'diagnose':
        next_action = action(root, 'doctor', expected='Environment inventory; use context DIAGNOSTIC_ID for a specific failure.')
    elif route['kind'] == 'upgrade' and project:
        next_action = action(root, 'upgrade', 'plan', str(project), '--to', git['revision'] or 'HEAD',
                             '--engine-checkout', str(root), '--json', expected='Existing read-only migration/compatibility plan; no automatic upgrade.')
    elif route['kind'] in ('new-game', 'change-game') and project:
        if not (project / 'Cargo.toml').exists():
            if route['kind'] == 'change-game':
                blockers.append('Existing game Cargo.toml is missing; select the correct --project or use --kind new-game deliberately.')
            elif project.exists() and any(project.iterdir()):
                blockers.append('Scaffold destination is nonempty; choose a new directory. Existing files will not be overwritten.')
            else:
                next_action = action(root, 'map', 'new-game', state['name'], str(project), str(root), route['template'] or 'portable',
                                     expected='Create explicit starter; this action builds fresh authoring tooling only when you execute it.')
        elif route['runtime'] == 'portable':
            iteration = action(root, 'check', '--game', str(project), '--loop', 'inner', '--task', state['id'],
                               expected='Execute game behavior tests with current input identity; no dependency engine suite.')
            final['command'] = action(root, 'check', '--game', str(project), '--loop', 'shipping', '--task', state['id'],
                                      expected='Complete browser and declared native package gates; other OSs need their own run.')
            next_action = iteration
            integration = action(root, 'check', '--game', str(project), '--loop', 'integration', '--task', state['id'],
                                 expected='Formatting/default-feature project verification.')
            if stages['inner']['state'] == 'passed':
                next_action = integration
            if stages['integration']['state'] == 'passed':
                next_action = final['command']
        else:
            next_action = {'kind': 'command', 'cwd': str(project), 'argv': [sys.executable, 'scripts/check.py'],
                           'expected': 'Existing native project check/ship gate; coordinator does not certify this delegated route.'}
    elif route['kind'] == 'engine':
        arguments = [arg for p in state['paths'] for arg in ('--path', p)] or ['--changed', '--base', state['base_revision'] or 'HEAD']
        iteration = action(root, 'check', *arguments, '--loop', 'inner', '--task', state['id'], expected='Focused owner/consumer evidence; unknown paths fail closed.')
        final['command'] = action(root, 'check', '--changed', '--base', state['base_revision'] or 'HEAD', '--task', state['id'],
                                  expected='Existing conservative final engine checks; CI/relevant target gates remain required.')
        changed = inputs['engine'] != state['initial_inputs']['engine']
        if changed:
            next_action = iteration
        integration = action(root, 'check', *arguments, '--loop', 'integration', '--task', state['id'], expected='Affected subsystem feature modes and headless boundary.')
        if stages['inner']['state'] == 'passed':
            next_action = integration
        if stages['integration']['state'] == 'passed':
            next_action = final['command']
    if stages['shipping']['state'] == 'passed':
        final['state'] = 'unverified'
        final['machine_gate'] = 'passed for recorded scope only; CI/manual/device obligations remain unverified'
        next_action = action(root, 'context', query, '--level', '2', '--compact', kind='review',
                             expected='Review objective, frames and remaining platform/CI requirements; notes cannot certify them.')
    for loop in ('inner', 'integration', 'shipping'):
        if stages[loop]['state'] in ('failed', 'skipped'):
            command = iteration if loop == 'inner' else final.get('command') if loop == 'shipping' else integration
            if command:
                next_action = {**command, 'kind': 'repair',
                               'expected': 'Inspect the retained failure/skip details, fix the cause, then execute this check. Prior failures remain recorded.'}
            break
    if blockers:
        next_action = action(root, 'context', query, '--level', '2', '--compact', kind='clarify',
                             expected='Resolve listed capability/configuration uncertainty before implementation or verification.',
                             files=['game.project.json'] if project else None)
    elif not ready['ready_to_attempt_inner'] and next_action['kind'] == 'command':
        next_action = action(root, 'doctor', expected='Diagnose missing prerequisites; install/select tools explicitly, then resume.')
    packet = {'schema_version': VERSION, 'task': state['id'], 'objective': state['objective'], 'constraints': state['constraints'],
              'engine': {**git, 'root': str(root), 'source_identity': inputs['engine'], 'executable': inputs['tools']['authoring']},
              'project': state['project'], 'workflow': route, 'readiness': ready,
              'capabilities': {'starter': catalog(root)['starters'].get(route.get('template')),
                               'browser': web_games.capabilities() if 'web' in route['targets'] else None,
                               'extension': 'Custom code/clients remain available; runtime/target gaps require engineering, not dropping requested mechanics.'},
              'context': {'references': refs, 'paths': state['paths'],
                          'project_files': ['AGENTS.md', 'game.project.json', 'src/lib.rs'] if project else [],
                          'matches': context['matches'],
                          'confidence': context['confidence'], 'traps': context.get('learned', [])},
              'next_action': next_action, 'iteration': iteration, 'completion': final,
              'evidence': {'schema': schema, **stages, 'history': history[-4:],
                           'inputs_changed': inputs != state['initial_inputs'], 'notes_are_evidence': False},
              'blockers': blockers, 'notes': state['notes'][-2:],
              'inspection': {'read_only': True, 'builds_triggered': 0}}
    if iteration:
        try:
            plan = (workflow.game_plan(root, project, 'inner') if project and route['kind'] != 'engine' else
                    workflow.change_plan(root, state['paths'], loop='inner') if state['paths'] else None)
            if plan:
                packet['iteration']['plan'] = {'state': 'planned', 'scope': plan['scope'],
                                               'commands': len(plan['commands']), 'requirements': plan.get('requirements'),
                                               'fallback_paths': plan.get('fallback_paths', [])}
        except (ValueError, OSError, KeyError) as error:
            packet['iteration']['plan'] = {'state': 'unverified', 'detail': str(error)}
    if detail:
        packet['input_identity'] = inputs
        packet['observed_commands'] = calls
        packet['evidence']['history'] = history
    state['next_action'] = next_action
    state['completed_stages'] = [k for k, v in stages.items() if v['state'] == 'passed']
    state['blockers'] = blockers
    return packet


def resume(root, task, note=None, detail=False, persist=True):
    state = load(root, task)
    if note:
        if len(note) > 2000:
            raise ValueError('Note exceeds 2000 characters; keep notes compact and link detailed context')
        state['notes'].append(note)
    packet = refresh(root, state, detail=detail)
    if persist:
        save(root, state)
    packet['state_file'] = str(state_path(root, task))
    return packet


def bind_check(root, task, plan):
    state = load(root, task)
    game = plan.get('game')
    if (game != state['project'] and state['route']['kind'] != 'engine') or (game and state['route']['kind'] == 'engine'):
        raise ValueError('Check scope does not match task project/engine route')
    if state['route']['gaps']:
        raise ValueError('Resolve the task capability/workflow gap before binding checks; use the direct tools for independent work')
    return {'task': task, 'project': state['project'], 'before': task_inputs.capture(root, state['project'])}


def finish_check(root, binding, plan, report, path):
    after = task_inputs.capture(root, binding['project'])
    report['task_evidence'] = {**binding, 'after': after,
                               'stable_sources': task_inputs.stable_sources(binding['before'], after, plan)}
    state = load(root, binding['task'])
    if str(path) not in state['reports']:
        state['reports'].append(str(path))
    save(root, state)


def readable(packet):
    action_ = packet['next_action']
    prerequisites = [c['name'] + ' (' + c['phase'] + ')' for c in packet['readiness']['checks'] if c['state'] != 'passed']
    return '\n'.join([f"Task {packet['task']}: {packet['objective']}",
                      f"Engine: {packet['engine']['revision']} / {packet['engine']['working_tree']['state']}; project {packet['project'] or 'engine checkout'}",
                      f"Authoring executable: {packet['engine']['executable']['path']} (freshness unverified)",
                      f"Route: {packet['workflow']['kind']} / {packet['workflow']['template'] or packet['workflow']['runtime'] or 'existing tools'}; targets {', '.join(packet['workflow']['targets'])}",
                      'Read: ' + ', '.join(packet['context']['references']),
                      'Blockers: ' + ('; '.join(packet['blockers']) or 'none in routing; see prerequisite/evidence states'),
                      'Missing/unverified prerequisites: ' + (', '.join(prerequisites) or 'none observed; compilation/dependencies still unverified'),
                      f"Next ({action_['kind']}) in {action_['cwd']}: {json.dumps(action_['argv'])}", action_['expected'],
                      'Evidence: ' + ', '.join(f"{k}={packet['evidence'][k]['state']}" for k in ('schema', 'inner', 'integration', 'shipping')),
                      'Required: ' + ' '.join(packet['constraints']),
                      'Final: ' + ' '.join(packet['completion']['requirements']),
                      'Context notes (not evidence): ' + ('; '.join(packet['notes']) or 'none'),
                      'No build/installation/launch occurred. Use --json or --detail for structured data.'])
