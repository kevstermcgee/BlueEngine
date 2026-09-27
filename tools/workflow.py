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


def context(root, query, limit=3):
    if not query.strip() or len(query) > 100 or not 1 <= limit <= 5:
        raise ValueError('Use a 1..100 character query and a limit of 1..5')
    features = json.loads((root / 'tools/FEATURES.json').read_text(encoding='utf-8'))['features']
    terms = set(re.findall(r'[a-z0-9]+', query.lower()))
    ranked = []
    for name, feature in features.items():
        words = set(re.findall(r'[a-z0-9]+', json.dumps(feature).lower()))
        title = set(re.findall(r'[a-z0-9]+', name.lower()))
        score = 1000 if query.lower() == name else 4 * len(terms & title) + len(terms & words)
        if score:
            ranked.append((score, name, feature))
    ranked.sort(key=lambda item: (-item[0], item[1]))
    return {'query': query, 'total': len(ranked), 'limit': limit,
            'matches': [{'id': name, **feature} for _, name, feature in ranked[:limit]],
            'engine_source_read': False,
            'next': 'Read only matching contract/source/test paths. Checks here are evidence, '
                    'not a complete validation plan. Use check --changed --plan for that.',
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
    # Deliberately NOT inferred from FEATURES.json: it is a discovery index, not a
    # dependency graph. Only independent Python entry points have reviewed scopes.
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
