"""Bounded context lookup and conservative validation selection. No Cargo discovery."""
import json
import re
import subprocess
import sys


def console_options():
    """Give console process trees a hidden console that descendants can inherit."""
    if sys.platform != 'win32':
        return {}
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = subprocess.SW_HIDE
    # CREATE_NO_WINDOW detaches Cargo: its Rust test children can then allocate
    # visible consoles. A hidden NEW_CONSOLE keeps descendants in the same console.
    return {'startupinfo': startup, 'creationflags': subprocess.CREATE_NEW_CONSOLE}


def index(root):
    return json.loads((root / 'tools/FEATURES.json').read_text(encoding='utf-8'))['features']


def closure(features, names, reverse=False):
    result = set(names)
    pending = list(names)
    while pending:
        name = pending.pop()
        adjacent = ([key for key, value in features.items() if name in value.get('depends_on', [])]
                    if reverse else features[name].get('depends_on', []))
        for key in adjacent:
            if key not in result:
                result.add(key)
                pending.append(key)
    return result


def impact(root, paths):
    features = index(root)
    owners = {name for name, feature in features.items()
              if any(path == owned or path.startswith(owned.rstrip('/') + '/')
                     for path in paths for owned in feature['files'])}
    covered = {path for path in paths if any(
        path == owned or path.startswith(owned.rstrip('/') + '/')
        for name in owners for owned in features[name]['files'])}
    affected = closure(features, owners, reverse=True)
    return {'owners': sorted(owners), 'affected': sorted(affected),
            'unmapped': sorted(set(paths) - covered),
            'confidence': 'partial: declared feature relationships, not a complete Rust dependency graph',
            'scope_warning': ('More than 10 files: re-run context; consider a shared lower layer.'
                              if len(paths) > 10 else
                              'Crosses simulation/network/presentation: consider a shared lower layer.'
                              if {'simulation_contract', 'multiplayer', 'game_presentation'} <= affected else None)}


def context(root, query, limit=3):
    if not query.strip() or len(query) > 100 or not 1 <= limit <= 5:
        raise ValueError('Use a 1..100 character query and a limit of 1..5')
    features = index(root)
    terms = set(re.findall(r'[a-z0-9]+', query.lower())) - {
        'add', 'fix', 'change', 'the', 'a', 'an', 'to', 'for', 'in', 'of', 'and'}
    ranked = []
    for name, feature in features.items():
        words = set(re.findall(r'[a-z0-9]+', json.dumps(feature).lower()))
        title = set(re.findall(r'[a-z0-9]+', name.lower()))
        keywords = set(re.findall(r'[a-z0-9]+', ' '.join(feature.get('keywords', [])).lower()))
        diagnostic = any(query.upper() == item['id'] for item in
                         feature.get('constraints', []) + feature.get('decisions', []))
        score = 1000 if query.lower() == name or diagnostic else 4 * len(terms & (title | keywords)) + len(terms & words)
        if score:
            ranked.append((score, name, feature))
    ranked.sort(key=lambda item: (-item[0], item[1]))
    selected = ranked[:1] if ranked and ranked[0][0] >= 1000 else ranked[:limit]
    matches = []
    for position, (_, name, feature) in enumerate(selected):
        allowance = max(1, 5 // len(selected) + (position < 5 % len(selected)))
        item = {'id': name, 'read_first': feature.get('read_first', feature['files'][:3])[:allowance],
                'contract': feature.get('contract', feature['note']),
                'tests': feature['checks'][:2]}
        for field in ('constraints', 'canonical_example', 'decisions', 'traps', 'depends_on', 'public_api'):
            if feature.get(field):
                item[field] = feature[field]
        item['used_by'] = sorted(key for key, value in features.items() if name in value.get('depends_on', []))
        matches.append(item)
    omitted = []
    # A future large record must not silently turn context into an index dump.
    while len(matches) > 1 and len(json.dumps(matches).encode('utf-8')) > 6000:
        omitted.insert(0, matches.pop()['id'])
    related = closure(features, [item['id'] for item in matches])
    candidates = ['graphics', 'offline_renderer', 'asset_library', 'map_geometry']
    return {'query': query, 'total': len(ranked), 'limit': limit,
            'matches': matches,
            'omitted_matches': omitted,
            'confidence': 'high' if selected and selected[0][0] >= 1000 else 'low: keyword ranking; confirm read_first before editing',
            'probably_unnecessary': [name for name in candidates if name not in related] if selected else [],
            'exclusion_basis': 'Declared dependencies only; advisory, never a reason to skip validation.',
            'expected_scope': 'Small task: 1-4 source files; subsystem: 3-8. Above 10, recheck impact.',
            'engine_source_read': False,
            'verify': 'python tools/be2.py check --changed --plan; then check --changed',
            'next': 'If uncertain, narrow by feature ID or use existing be2-tools src find/outline. Feature tests are iteration evidence, not final validation.',
            'no_match': 'No indexed capability; do not invent an API.'}


def changed_paths(root, base='HEAD'):
    def git(*args):
        result = subprocess.run(['git', *args], cwd=root, check=True,
                                capture_output=True, text=True, encoding='utf-8',
                                **console_options())
        return result.stdout
    # Resolve separately: refs beginning with '-' must never become diff options.
    revision = git('rev-parse', '--verify', '--end-of-options', base + '^{commit}').strip()
    tracked = git('diff', '--no-ext-diff', '--no-renames', '--name-only', '-z', revision, '--')
    untracked = git('ls-files', '--others', '--exclude-standard', '-z')
    return revision, sorted(set(filter(None, (tracked + untracked).split('\0'))))


def full_commands():
    # Group feature modes to avoid repeatedly rebuilding the same binary with a
    # different feature set. Keep every pre-existing engine gate.
    commands = [['cargo', 'fmt', '--check']]
    for features in ([], ['--no-default-features']):
        commands.extend([
            ['cargo', 'rustdoc', '--locked', '--lib', *features, '--', '-D', 'warnings'],
            ['cargo', 'test', '--locked', *features],
            ['cargo', 'clippy', '--all-targets', '--locked', *features, '--', '-D', 'warnings'],
        ])
    return commands + [
        [sys.executable, 'tools/check_headless.py'],
        [sys.executable, 'tools/check_authoring.py'],
        [sys.executable, '-m', 'unittest', 'tools.test_workflow',
         'tools.test_assets', 'scripts.test_publish_games'],
    ]


def validation_plan(paths=None, base=None):
    full = full_commands()
    if paths is None:
        return {'scope': 'full', 'reason': 'Full engine validation requested.', 'commands': full}
    # The declared feature graph is partial, not proof of independence.
    # Only independent Python entry points have reviewed verification scopes.
    scopes = {
        'author': ({'tools/author.py', 'tools/test_author.py'},
                   [[sys.executable, 'tools/check_authoring.py']]),
        'assets': ({'tools/assets.py', 'tools/test_assets.py'},
                   [[sys.executable, '-m', 'unittest', 'tools.test_assets'],
                    [sys.executable, 'tools/assets.py', 'validate']]),
        'publishing': ({'scripts/publish_games.py', 'scripts/test_publish_games.py'},
                       [[sys.executable, '-m', 'unittest', 'scripts.test_publish_games'],
                        [sys.executable, 'scripts/publish_games.py', 'check']]),
    }
    covered = set().union(*(files for files, _ in scopes.values()))
    unknown = sorted(set(paths) - covered)
    commands = []
    selected = []
    for name, (files, checks) in scopes.items():
        if files.intersection(paths):
            selected.append(name)
            commands.extend(checks)
    return {'scope': 'full' if unknown else ('+'.join(selected) or 'no_changes'),
            'base': base, 'changed_paths': paths, 'fallback_paths': unknown,
            'reason': 'Unclassified inputs require all engine gates.' if unknown else
                      'Reviewed independent Python inputs only; no engine contract changed.' if paths else
                      'No changes relative to base; this does not certify the baseline.',
            'commands': full if unknown else commands}
