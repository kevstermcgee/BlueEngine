"""Bounded context lookup and conservative validation selection. No Cargo discovery."""
import json
from pathlib import Path
import re
import shutil
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


LOW_DISK_GB = 15


def disk_report(locations, usage=shutil.disk_usage):
    """Free space where Cargo writes (target, registry, temp).

    A cold engine or game build writes 10-13 GB; when the drive fills, Cargo fails late with os error 112
    (Windows) or ENOSPC. `locations` maps a label to a path (which need not exist yet); `usage` is
    injectable for tests. Returns the per-location numbers, plain-English warnings and, when any location is
    low, the exact environment variables that move Cargo to a drive with room.
    """
    found = {}
    warnings = []
    for label, path in locations.items():
        probe = Path(path)
        while not probe.exists() and probe != probe.parent:
            probe = probe.parent
        try:
            free = usage(probe).free / 1e9
        except OSError:
            continue
        found[label] = {'path': str(path), 'free_gb': round(free, 1)}
        if free < LOW_DISK_GB:
            warnings.append(f'{label} ({path}) has {free:.1f} GB free; a build needs about 10-13 GB')
    hint = None
    if warnings:
        hint = ('Point Cargo at a drive with room before building (a generated game inherits the same variables). '
                'PowerShell: $env:CARGO_TARGET_DIR="D:\\cargo-target"; $env:CARGO_HOME="D:\\cargo-home"; '
                '$env:TMP="D:\\tmp"; $env:TEMP="D:\\tmp". bash: export CARGO_TARGET_DIR=/d/cargo-target '
                'CARGO_HOME=/d/cargo-home TMPDIR=/d/tmp. Old target directories are safe to delete, but first check '
                'that no desktop shortcut or launcher points into them: shortcuts belong on a game\'s dist/ folder.')
    return {'locations': found, 'warnings': warnings, 'hint': hint}


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
    if not query.strip() or len(query) > 100:
        raise ValueError(f'The query must be 1..100 characters (this one is {len(query)}): shorten it to the '
                         f'few words that name the feature, not the whole task')
    if not 1 <= limit <= 5:
        raise ValueError(f'--limit must be 1..5 (got {limit})')
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
        for typecheck in (False, True):
            try:
                iteration_plan(root, name, typecheck=typecheck, _features=features)
            except ValueError:
                continue
            item['iterate'] = f'python tools/be2.py check --iterate {name}' + (' --typecheck' if typecheck else '') + ' --plan'
            break
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


# Cargo profile for `cargo test` in local checks. `itest` (Cargo.toml) is dev with optimized
# dependencies: same assertions, about 8x faster on the physics suites. CI runs cargo directly and is
# unaffected. `dev` restores the plain profile.
TEST_PROFILES = ('itest', 'dev')


def profile_flags(test_profile):
    if test_profile not in TEST_PROFILES:
        raise ValueError('Test profile must be one of: ' + ', '.join(TEST_PROFILES))
    return [] if test_profile == 'dev' else ['--profile', test_profile]


def full_commands(test_profile='itest'):
    # Group feature modes to avoid repeatedly rebuilding the same binary with a
    # different feature set. Keep every pre-existing engine gate.
    commands = [['cargo', 'fmt', '--check']]
    for features in ([], ['--no-default-features']):
        commands.extend([
            ['cargo', 'rustdoc', '--locked', '--lib', *features, '--', '-D', 'warnings'],
            ['cargo', 'test', '--locked', *profile_flags(test_profile), *features],
            ['cargo', 'clippy', '--all-targets', '--locked', *features, '--', '-D', 'warnings'],
        ])
    return commands + [
        [sys.executable, 'tools/check_headless.py'],
        [sys.executable, 'tools/check_authoring.py'],
        [sys.executable, '-m', 'unittest', 'tools.test_workflow',
         'tools.test_assets', 'scripts.test_publish_games',
         'tools.test_game_check', 'tools.test_game_ship', 'tools.test_media_tools',
         'tools.test_xcapture', 'tools.test_upgrade'],
    ]


WINDOWS_TARGET = 'x86_64-pc-windows-gnu'
WINDOWS_PROVES = (
    'Rust type-check of the engine library, every binary, example and test for ' + WINDOWS_TARGET +
    ' with default and headless features: cfg(windows) code (native key/focus readers, windows-sys '
    'calls, console handlers) compiles and type-checks. It does NOT prove anything about running on '
    'Windows: no linking, no execution, no real key/focus/console behavior, and the C parts of '
    'dependencies (ring) are not built for Windows. CI on windows-latest remains the real gate.')


def windows_plan(which=shutil.which, run=subprocess.run):
    """Plan for `check --windows`: a cross type-check for the Windows target without a Windows C toolchain.

    `cargo check` never links, but ring's build script still runs a C compiler for the target. With
    no mingw gcc we hand it the host `cc`/`ar` through cc-rs' per-target variables: it compiles the C as
    host objects that nothing links. That is fine for a type-check and says nothing about Windows C code.
    `which` and `run` are injectable for tests. Raises RuntimeError with the remedy when it cannot work.
    """
    if not which('cargo') or not which('rustc'):
        raise RuntimeError('Rust toolchain is missing; see tools/README.md')
    libdir = run(['rustc', '--print', 'target-libdir', '--target', WINDOWS_TARGET],
                 capture_output=True, text=True, **console_options())
    if libdir.returncode or not (libdir.stdout.strip() and Path(libdir.stdout.strip()).is_dir()):
        raise RuntimeError(f'Rust target {WINDOWS_TARGET} is not installed; run: rustup target add {WINDOWS_TARGET}')
    env, compiler = {}, 'x86_64-w64-mingw32-gcc'
    if not which(compiler):
        host_cc = which('cc') or which('gcc') or which('clang')
        host_ar = which('ar')
        if not host_cc or not host_ar:
            raise RuntimeError(f'No {compiler} and no host cc/ar to stand in for it; install a C compiler '
                               f'(mingw-w64 or build-essential) so ring\'s build script can run')
        env = {'CC_x86_64_pc_windows_gnu': host_cc, 'AR_x86_64_pc_windows_gnu': host_ar}
    base = ['cargo', 'check', '--locked', '--target', WINDOWS_TARGET, '--all-targets']
    return {'scope': 'windows_typecheck',
            'reason': 'Windows-only code is invisible to Linux builds; type-check it for ' + WINDOWS_TARGET + '.',
            'proves': WINDOWS_PROVES, 'env': env,
            'commands': [base, [*base, '--no-default-features']]}


def iteration_plan(root, feature_id, *, typecheck=False, test=None, feature_mode='default', _features=None,
                   test_profile='itest'):
    """Explicit iteration only. Derive targets from indexed evidence/files, never prose commands."""
    features = index(root) if _features is None else _features
    if feature_id not in features:
        raise ValueError('Unknown feature ID; use context to choose an exact indexed feature.')
    feature = features[feature_id]
    flags = ['--no-default-features'] if feature_mode == 'headless' else []
    if feature_mode not in {'headless', 'default'} or (typecheck and test):
        raise ValueError('Choose headless/default and either typecheck or a test, not both.')
    suites = sorted({item['suite'] for item in feature.get('evidence', [])} |
                    {path[6:-3] for path in feature['files']
                     if re.fullmatch(r'tests/[\w-]+\.rs', path)})
    if any(not re.fullmatch(r'[\w-]+', suite) for suite in suites):
        raise ValueError('Invalid indexed test suite name')
    harness = None
    if typecheck:
        if not any(path.startswith('src/') for path in feature['files']):
            raise ValueError('No engine source indexed here; use the feature checks from context.')
        commands = [['cargo', 'check', '--locked', '--lib', *flags, '--message-format=json']]
        proves = 'Engine library type-check only; no linking or behavior tests.'
    elif suites:
        selected, separator, name = test.partition('::') if test else ('', '', '')
        if test and (selected not in suites or (separator and not name)):
            raise ValueError('Select an indexed SUITE or SUITE::exact_test; suites: ' + ', '.join(suites))
        selected_suites = [selected] if test else suites
        command = ['cargo', 'test', '--locked', *profile_flags(test_profile), *flags, '--message-format=json']
        for suite in selected_suites:
            command += ['--test', suite]
        if separator:
            if not re.fullmatch(r'[\w:]+', name):
                raise ValueError('Exact test name must contain only letters, numbers, underscores or colons')
            command += [name, '--', '--exact']
        commands = [command]
        harness = 'rust'
        proves = 'Only requested integration tests in the selected feature configuration.'
    else:
        # Reuse the small existing Python unittest mappings; do not shell-execute
        # free-form checks, native command descriptions, or arbitrary index text.
        commands = []
        for check in feature['checks']:
            match = re.fullmatch(r'python -m unittest ((?:[\w]+\.[\w.]+)(?: [\w]+\.[\w.]+)*)', check)
            if match:
                commands.append([sys.executable, '-m', 'unittest', *match[1].split()])
        if not commands or test:
            raise ValueError('No indexed integration suite for this selection; use --typecheck or context checks.')
        harness = 'python'
        feature_mode = 'not_applicable'
        proves = 'Only indexed Python unittest modules; no engine compilation or runtime checks.'
    return {'scope': 'iteration', 'feature': feature_id, 'feature_mode': feature_mode,
            'proves': proves, 'remaining': 'Final check --changed (or check), Linux/Windows CI, and relevant manual checks.',
            'test_harness': harness, 'commands': commands}


def command_evidence(log, returncode, harness=None):
    """Read Cargo JSON compiler records and ordinary test text separately. No root-cause inference."""
    diagnostics, panics, tail, summaries = [], [], [], []
    artifacts = {'fresh': 0, 'built': 0}
    panic_lines = 0
    python_tests = None
    python_skips = 0
    location = None
    with log.open(encoding='utf-8', errors='replace') as stream:
        for line in stream:
            try:
                record = json.loads(line) if line.startswith('{') else None
            except ValueError:
                record = None
            if isinstance(record, dict) and record.get('reason') == 'compiler-artifact':
                artifacts['fresh' if record.get('fresh') else 'built'] += 1
                continue
            if isinstance(record, dict) and record.get('reason') == 'compiler-message':
                message = record.get('message', {})
                if message.get('level') == 'error' and len(diagnostics) < 3:
                    primary = next((span for span in message.get('spans', []) if span.get('is_primary')), None)
                    diagnostics.append({'message': message.get('message', '')[:1000],
                                        'code': (message.get('code') or {}).get('code'),
                                        'location': ({'file': primary['file_name'], 'line': primary['line_start'],
                                                      'column': primary['column_start']} if primary else None),
                                        'notes': [child['message'][:500] for child in message.get('children', [])
                                                  if len(child.get('message', '')) < 1000][-2:]})
                continue
            line = line.rstrip()
            if not line:
                continue
            match = re.search(r'panicked at (.+):(\d+):(\d+):$', line)
            if match and location is None:
                location = {'file': match[1], 'line': int(match[2]), 'column': int(match[3])}
            tail = (tail + [line[:500]])[-6:]
            match = re.search(r'test result: \w+\. (\d+) passed; (\d+) failed;', line)
            if match:
                summaries.append(int(match[1]) + int(match[2]))
            match = re.search(r'^Ran (\d+) tests? in ', line)
            if match:
                python_tests = int(match[1])
            match = re.search(r'^OK \(skipped=(\d+)\)', line)
            if match:
                python_skips = int(match[1])
            if (' panicked at ' in line or line.startswith(('FAIL:', 'ERROR:'))):
                panic_lines = 7
            if panic_lines and len(panics) < 18:
                panics.append(line[:500])
                panic_lines -= 1
    result = {'returncode': returncode, 'log_bytes': log.stat().st_size,
              'cargo_artifacts': artifacts}
    if summaries or python_tests is not None:
        result['tests_executed'] = sum(summaries) if summaries else python_tests - python_skips
    if returncode:
        result['category'] = 'compiler' if diagnostics else 'test_failure' if panics else 'command_failure'
        result['diagnostics'] = diagnostics or panics or tail
        if location:
            result['location'] = location
    elif harness:
        # AI-WARNING TEST-SELECTION-001: A zero-exit harness with no executed tests is not regression evidence.
        counts = summaries if harness == 'rust' else ([] if python_tests is None else [python_tests - python_skips])
        if not counts or any(count == 0 for count in counts):
            result.update(category='empty_test_selection' if counts else 'missing_test_evidence',
                          code='TEST-SELECTION-001',
                          diagnostics=['Requested tests did not prove a nonempty executed selection; inspect names and full log.'])
    return result


def validation_plan(paths=None, base=None, test_profile='itest'):
    full = full_commands(test_profile)
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
        'upgrade': ({'tools/upgrade.py', 'tools/test_upgrade.py', 'tools/upgrade_migrations.json'},
                   [[sys.executable, '-m', 'unittest', 'tools.test_upgrade']]),
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
