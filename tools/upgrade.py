"""Version-aware upgrade planning and verification for games built on BlueEngine.

Plans move an external game to a chosen engine revision: baseline/target identity, runtime
classification, applicable migrations from tools/upgrade_migrations.json, and template-provenance
comparison. Planning is read-only: no file in the game or engine checkout is written, no checkout is
switched, no network access is attempted. Verification reuses the game's own scripts/check.py
(templates/game_check.py) rather than re-implementing it; it always executes fresh and never accepts a
prior report.json as current evidence.
"""
import datetime
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
import time

FORMAT = 1
HEX_SHORT = re.compile(r'[0-9a-f]{6,40}')


class UpgradeError(ValueError):
    """A planning/verification input could not be resolved; never guessed."""


def _git(repo, *args, timeout=30):
    try:
        result = subprocess.run(['git', '-C', str(repo), *args], capture_output=True, text=True,
                                encoding='utf-8', errors='replace', timeout=timeout)
    except (OSError, subprocess.SubprocessError) as error:
        return None, str(error)
    if result.returncode != 0:
        return None, (result.stderr or result.stdout or f'git {" ".join(args)} failed').strip()
    return result.stdout, None


def is_git_repo(path):
    out, _ = _git(path, 'rev-parse', '--git-dir')
    return out is not None


def git_head(repo):
    """(full 40-hex commit, dirty bool) for repo's current checkout, or (None, None) if not a git repo."""
    commit, error = _git(repo, 'rev-parse', 'HEAD')
    if commit is None:
        return None, None
    status, _ = _git(repo, 'status', '--short')
    return commit.strip(), bool(status and status.strip())


def git_head_kind(repo):
    """'detached', a branch name, or None (not a git repo / no commits)."""
    branch, error = _git(repo, 'rev-parse', '--abbrev-ref', 'HEAD')
    if branch is None:
        return None
    branch = branch.strip()
    return 'detached' if branch == 'HEAD' else branch


def resolve_commit(repo, ref):
    """Full 40-hex commit `ref` names in `repo`, resolved locally once. Never fetches."""
    full, error = _git(repo, 'rev-parse', '--verify', '--end-of-options', ref + '^{commit}')
    if full is None:
        raise UpgradeError(f'Could not resolve {ref!r} in {repo} ({error}). This tool never fetches; '
                           'update the checkout yourself first if the revision is remote-only.')
    return full.strip()


def is_ancestor(repo, maybe_ancestor, commit):
    """True/False if resolvable locally, else None (unknown: shallow history or bad revision)."""
    result = subprocess.run(['git', '-C', str(repo), 'merge-base', '--is-ancestor', maybe_ancestor, commit],
                            capture_output=True, timeout=30)
    if result.returncode in (0, 1):
        return result.returncode == 0
    return None


def resolve_target(engine_checkout, ref):
    commit = resolve_commit(engine_checkout, ref)
    _, dirty = git_head(engine_checkout)
    return {'ref': ref, 'commit': commit, 'short': commit[:12], 'checkout': str(engine_checkout),
            'checkout_dirty': dirty,
            'warning': (f'{ref!r} is a moving branch name; this plan pins the exact commit above. '
                       'Re-run planning if the branch advances before you apply this plan.')
                       if ref not in (commit, commit[:12]) and not HEX_SHORT.fullmatch(ref) else None}


def _import_module(path, label):
    """Imports `path` for its functions only. Planning must not write to the game: bytecode caching
    (which would otherwise create scripts/__pycache__/*.pyc next to an external game's check.py) is
    disabled for the duration of the import, not globally."""
    path = Path(path)
    if not path.is_file():
        return None
    spec = importlib.util.spec_from_file_location(f'_upgrade_{label}_{abs(hash(str(path)))}', path)
    if spec is None or spec.loader is None:
        return None
    module = importlib.util.module_from_spec(spec)
    previous = sys.dont_write_bytecode
    sys.dont_write_bytecode = True
    try:
        spec.loader.exec_module(module)
    except Exception as error:
        raise UpgradeError(f'{path} could not be imported as a Python module: {error}') from error
    finally:
        sys.dont_write_bytecode = previous
    return module


def game_check_module(game_root):
    """The game's own scripts/check.py (templates/game_check.py as copied/customized), or None."""
    return _import_module(Path(game_root) / 'scripts/check.py', 'game_check')


def template_check_module(engine_root):
    return _import_module(Path(engine_root) / 'templates/game_check.py', 'template_check')


def resolve_engine_dependency(game_root, engine_root):
    """(path or None, source) where source is 'game_scripts_check' (the game's own copy, possibly
    customized) or 'engine_template_fallback' (no scripts/check.py; this engine's current template used
    for best-effort analysis only)."""
    module = game_check_module(game_root)
    source = 'game_scripts_check'
    if module is None or not hasattr(module, 'engine_path'):
        module = template_check_module(engine_root)
        source = 'engine_template_fallback'
    if module is None:
        return None, 'unavailable'
    return module.engine_path(Path(game_root)), source


def resolve_tool_binary(game_root, engine_root):
    module = game_check_module(game_root)
    source = 'game_scripts_check'
    if module is None or not hasattr(module, 'find_tools'):
        module = template_check_module(engine_root)
        source = 'engine_template_fallback'
    if module is None:
        return None, 'unavailable'
    return module.find_tools(Path(game_root)), source


def sha256_of(path):
    try:
        return hashlib.sha256(Path(path).read_bytes()).hexdigest()
    except OSError:
        return None


def read_identity(game_root):
    path = Path(game_root) / 'assets/identity.json'
    try:
        text = path.read_text(encoding='utf-8-sig')
    except OSError:
        return None, 'assets/identity.json not found'
    try:
        document = json.loads(text)
    except ValueError as error:
        return None, f'assets/identity.json is not valid JSON: {error}'
    if not isinstance(document, dict):
        return None, 'assets/identity.json does not contain a JSON object'
    return document, None


RUNTIME_MARKERS = {
    'wire.rs': 'src/wire.rs', 'server.rs': 'src/server.rs',
    'client.rs': 'src/client.rs', 'transport.rs': 'src/transport.rs',
}


def classify_runtime(game_root):
    """kind in {stock, custom_sim, custom_sim_netplay, mixed_or_legacy, unknown}, with the evidence used."""
    root = Path(game_root)
    has_game_json = (root / 'game.json').is_file()
    has_lib_rs = (root / 'src/lib.rs').is_file()
    has_cargo = (root / 'Cargo.toml').is_file()
    evidence = {'game.json': has_game_json, 'src/lib.rs': has_lib_rs, 'Cargo.toml': has_cargo}
    own_net_files = sorted(name for name, rel in RUNTIME_MARKERS.items() if (root / rel).is_file())
    evidence['hand_rolled_network_files'] = own_net_files
    uses_netplay = False
    try:
        for rust_file in root.glob('src/**/*.rs'):
            text = rust_file.read_text(encoding='utf-8', errors='replace')
            if 'viewer::netplay' in text or re.search(r'\bNetGame\b', text) or re.search(r'\bClientView\b', text):
                uses_netplay = True
                break
    except OSError:
        pass
    evidence['uses_netplay_kit'] = uses_netplay
    if not has_cargo:
        return {'kind': 'unknown', 'reason': 'no Cargo.toml: not a Rust game project', 'evidence': evidence}
    if has_game_json and has_lib_rs:
        return {'kind': 'mixed_or_legacy',
                'reason': 'both game.json (stock GameDocument) and src/lib.rs (custom simulation) are present',
                'evidence': evidence}
    if has_game_json:
        return {'kind': 'stock', 'reason': 'game.json present, no src/lib.rs', 'evidence': evidence}
    if has_lib_rs:
        kind = 'custom_sim_netplay' if uses_netplay else 'custom_sim'
        return {'kind': kind, 'reason': 'src/lib.rs present, no game.json', 'evidence': evidence}
    return {'kind': 'unknown', 'reason': 'neither game.json nor src/lib.rs found', 'evidence': evidence}


def load_registry(engine_root):
    path = Path(engine_root) / 'tools/upgrade_migrations.json'
    document = json.loads(path.read_text(encoding='utf-8'))
    if document.get('format') != FORMAT:
        raise UpgradeError(f'{path} has an unsupported format {document.get("format")!r}')
    return document['migrations']


def _matches_glob(root, pattern, exclude_paths):
    excluded = {str(Path(p).as_posix()) for p in exclude_paths or []}
    for path in root.glob(pattern):
        if not path.is_file():
            continue
        relative = path.relative_to(root).as_posix()
        if relative in excluded:
            continue
        yield path, relative


def evaluate_detect(game_root, detect):
    """True/False whether a migration's trigger evidence is present in the game's own source."""
    if detect is None:
        return True
    root = Path(game_root)
    kind = detect['kind']
    if kind == 'grep':
        pattern = re.compile(detect['pattern'])
        for include in detect.get('include', ['src/**/*.rs']):
            for path, _ in _matches_glob(root, include, detect.get('exclude_paths')):
                try:
                    if pattern.search(path.read_text(encoding='utf-8', errors='replace')):
                        return True
                except OSError:
                    continue
        return False
    if kind == 'files_exist':
        names = detect.get('any', detect.get('all', []))
        found = [name for name in names if (root / name).is_file()]
        return bool(found) if 'any' in detect else len(found) == len(names)
    if kind == 'files_absent':
        return all(not (root / name).is_file() for name in detect.get('all', []))
    raise UpgradeError(f'Unknown detect kind {kind!r} in migration registry')


def select_migrations(registry, engine_checkout, runtime_kind, baseline_commit, target_commit, game_root):
    results = []
    for migration in registry:
        since = migration['since_commit']
        entry = {'id': migration['id'], 'title': migration['title'], 'category': migration['category'],
                 'required': migration['required'], 'reference': migration['reference']}
        if baseline_commit is None:
            entry.update(status='uncertain',
                         reason='no recorded baseline engine revision (assets/identity.json engine_revision '
                                'is missing or unrecoverable); cannot tell whether this change already applied')
            results.append(entry)
            continue
        if runtime_kind in ('unknown', 'mixed_or_legacy'):
            # An unrecognized or mixed layout must never read as a quiet "no migration needed": say so.
            entry.update(status='uncertain',
                         reason=f'runtime classification is {runtime_kind!r}; cannot confidently judge applicability')
            results.append(entry)
            continue
        base_has = is_ancestor(engine_checkout, since, baseline_commit)
        target_has = is_ancestor(engine_checkout, since, target_commit)
        if base_has is None or target_has is None:
            entry.update(status='uncertain',
                         reason=f'{since} is not resolvable against baseline/target in {engine_checkout} '
                                '(shallow history or unknown revision)')
        elif base_has and target_has:
            entry.update(status='already_applied', reason='baseline already includes this change')
        elif not base_has and not target_has:
            entry.update(status='not_applicable', reason='target predates this change')
        elif base_has and not target_has:
            entry.update(status='uncertain',
                         reason='baseline already has this change but target does not: target looks older '
                                'than baseline (downgrade?); resolve manually')
        else:
            if runtime_kind not in migration['runtime']:
                entry.update(status='not_applicable',
                             reason=f'runtime is {runtime_kind!r}; this change applies to {migration["runtime"]}')
            elif evaluate_detect(game_root, migration['detect']):
                entry.update(status=migration['category'],
                             reason='revision range and source evidence both match')
            else:
                entry.update(status='not_applicable',
                             reason='in revision range, but the triggering pattern was not found in this game')
        results.append(entry)
    return results


def provenance(identity, runtime_kind):
    if identity and isinstance(identity.get('provenance'), dict):
        recorded = identity['provenance']
        return {'source': 'identity.json:provenance', 'template': recorded.get('template', 'unknown'),
                'template_revision': recorded.get('template_revision', 'unknown'),
                'generation_options': recorded.get('generation_options', {})}
    guess = {'stock': 'stock', 'custom_sim': 'custom-sim', 'custom_sim_netplay': 'custom-sim'}.get(runtime_kind)
    if identity and identity.get('engine_revision') and guess:
        return {'source': 'inferred', 'template': guess,
                'template_revision': ('unknown: assets/identity.json records the generation-time engine commit, '
                                      'not a separate template revision; equating them is not confirmed'),
                'generation_options': {}}
    return {'source': 'none', 'template': 'unknown', 'template_revision': 'unknown', 'generation_options': {}}


def check_script_report(game_root, engine_root):
    game_copy = Path(game_root) / 'scripts/check.py'
    if not game_copy.is_file():
        return {'present': False}
    current_template = Path(engine_root) / 'templates/game_check.py'
    game_hash, template_hash = sha256_of(game_copy), sha256_of(current_template)
    matches = (game_hash == template_hash) if template_hash else None
    return {'present': True, 'path': str(game_copy), 'sha256': game_hash, 'matches_current_template': matches,
            'note': ('identical to this engine checkout\'s current templates/game_check.py' if matches else
                     'differs from the current template (customized, or an older/newer template revision); '
                     'this plan never overwrites it')}


def _fallback_parse_json_output(text):
    """Mirrors templates/game_check.py's parse_json_output, for the rare case a game's own copy lacks it."""
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


def parse_fix(text):
    if ':' not in text:
        raise UpgradeError(f'--fix {text!r} must be ID:DESCRIPTION')
    task_id, _, description = text.partition(':')
    if not task_id:
        raise UpgradeError(f'--fix {text!r} must be ID:DESCRIPTION')
    return {'id': task_id, 'description': description, 'status': 'requested',
            'acceptance': 'Fixed and verified with no regression in the Stage F verify run (see docs/UPGRADE.md).'}


def _now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='seconds')


def plan(game_root, engine_root, target_ref, *, engine_checkout=None, fixes=(), adopt=()):
    """Read-only: inspects `game_root` and resolves `target_ref` in the engine checkout; writes nothing."""
    game_root = Path(game_root).resolve()
    engine_root = Path(engine_root).resolve()
    if not game_root.is_dir():
        raise UpgradeError(f'{game_root} is not a directory')
    warnings = []

    identity, identity_error = read_identity(game_root)
    if identity_error:
        warnings.append(identity_error)
    runtime = classify_runtime(game_root)
    if runtime['kind'] in ('unknown', 'mixed_or_legacy'):
        warnings.append(f'Runtime classification is {runtime["kind"]!r} ({runtime["reason"]}): migrations are '
                        'reported uncertain rather than excluded, and this plan is conservative')

    dep_path, dep_source = resolve_engine_dependency(game_root, engine_root)
    dependency = None
    if dep_path is None:
        warnings.append('Could not resolve the engine dependency path from Cargo.toml; engine identity is unavailable')
    else:
        commit, dirty = git_head(dep_path)
        head_kind = git_head_kind(dep_path)
        if commit is None:
            warnings.append(f'{dep_path} is not a git checkout; cannot read its commit')
        pin_quality = 'pinned_detached' if head_kind == 'detached' else 'floating_branch' if head_kind else 'unknown'
        dependency = {'path': str(dep_path), 'source': dep_source, 'commit': commit,
                      'commit_short': commit[:12] if commit else None, 'dirty': dirty, 'head_kind': head_kind,
                      'pin_quality': pin_quality,
                      'note': (None if pin_quality == 'pinned_detached' else
                              f'A path dependency on a checkout tracking {head_kind!r} is not a version pin: it '
                              'moves under this game whenever someone updates that checkout.')}
        if dirty:
            warnings.append(f'{dep_path} has uncommitted changes; its current commit is not a reproducible revision')

    checkout_for_target = Path(engine_checkout).resolve() if engine_checkout else (dep_path or engine_root)
    if not is_git_repo(checkout_for_target):
        raise UpgradeError(f'{checkout_for_target} is not a git checkout; cannot resolve a target revision there')
    target = resolve_target(checkout_for_target, target_ref)
    if target['checkout_dirty']:
        warnings.append(f'{checkout_for_target} has uncommitted changes; its HEAD is not reproducible '
                        '(the pinned target commit above is exact regardless)')
    if target['warning']:
        warnings.append(target['warning'])

    baseline_commit = identity.get('engine_revision') if identity else None
    if baseline_commit is not None and not HEX_SHORT.fullmatch(str(baseline_commit)):
        warnings.append(f'assets/identity.json engine_revision {baseline_commit!r} is not a recognizable commit; '
                        'treating the baseline as unknown')
        baseline_commit = None
    if baseline_commit is None:
        warnings.append('No recoverable last-verified engine revision: producing a conservative plan '
                        '(every migration is reported uncertain rather than excluded)')

    registry = load_registry(engine_root)
    migrations = select_migrations(registry, checkout_for_target, runtime['kind'], baseline_commit,
                                   target['commit'], game_root)

    requested_fixes = [parse_fix(item) for item in fixes]
    migrations_by_id = {item['id']: item for item in migrations}
    for migration_id in adopt:
        entry = migrations_by_id.get(migration_id)
        if entry is None:
            warnings.append(f'--adopt {migration_id}: no such migration id in the registry')
        elif entry['category'] != 'optional_adoption':
            warnings.append(f'--adopt {migration_id}: category is {entry["category"]!r}, not optional_adoption')
        elif entry['status'] != 'optional_adoption':
            warnings.append(f'--adopt {migration_id}: status is {entry["status"]!r} (not currently applicable), nothing to adopt')
        else:
            entry['adopt_requested'] = True

    tool_path, tool_source = resolve_tool_binary(game_root, engine_root)
    tool_binary = None
    if tool_path:
        tool_binary = {'path': str(tool_path), 'source': tool_source, 'sha256': sha256_of(tool_path),
                       'provenance_note': 'be2-tools embeds no build-commit stamp; this path is not proof it '
                                          'was built from the engine dependency commit above'}
        if dep_path is not None:
            try:
                under_dependency = Path(tool_path).resolve().is_relative_to(dep_path.resolve())
            except AttributeError:
                under_dependency = str(Path(tool_path).resolve()).startswith(str(dep_path.resolve()))
            if not under_dependency:
                warnings.append(f'The resolved be2-tools binary ({tool_path}) is not under the resolved engine '
                                f'dependency ({dep_path}); it may be built from a different checkout')
    else:
        warnings.append('No be2-tools binary found (BE2_TOOLS, PATH, or the engine checkout target dir); '
                        'verification will not be able to run native checks')

    packet = {
        'format': FORMAT, 'generated_at': _now(), 'game_root': str(game_root),
        'target': target,
        'game_source': dict(zip(('revision', 'dirty'), git_head(game_root))) if is_git_repo(game_root)
                      else {'revision': None, 'dirty': None, 'note': 'not a git repository'},
        'runtime': runtime,
        'identity_error': identity_error,
        'last_verified_engine_revision': baseline_commit,
        'engine_dependency': dependency,
        'tool_binary': tool_binary,
        'template_provenance': provenance(identity, runtime['kind']),
        'check_script': check_script_report(game_root, engine_root),
        'migrations': migrations,
        'requested_fixes': requested_fixes,
        'warnings': warnings,
    }
    packet['human_summary'] = format_plan_human(packet)
    return packet


def format_plan_human(packet):
    lines = [f'Upgrade plan: {packet["game_root"]}',
            f'  runtime: {packet["runtime"]["kind"]} ({packet["runtime"]["reason"]})',
            f'  baseline (last verified): {packet["last_verified_engine_revision"] or "unknown"}',
            f'  target: {packet["target"]["ref"]} -> {packet["target"]["short"]}'
            f'{" [checkout dirty]" if packet["target"]["checkout_dirty"] else ""}']
    dependency = packet['engine_dependency']
    if dependency:
        lines.append(f'  dependency: {dependency["path"]} @ {dependency["commit_short"]} '
                    f'({dependency["pin_quality"]}{", dirty" if dependency["dirty"] else ""})')
    by_status = {}
    for migration in packet['migrations']:
        by_status.setdefault(migration['status'], []).append(migration['id'])
    for status, ids in sorted(by_status.items()):
        lines.append(f'  migrations {status}: {", ".join(ids)}')
    if packet['requested_fixes']:
        lines.append('  requested fixes: ' + ', '.join(f['id'] for f in packet['requested_fixes']))
    if packet['warnings']:
        lines.append(f'  warnings ({len(packet["warnings"])}):')
        lines.extend(f'    - {warning}' for warning in packet['warnings'])
    return '\n'.join(lines)


def verify(game_root, engine_root, *, skip_ship=False, content_only=False, scenarios=(), timeout=600):
    """Reuses the game's own scripts/check.py; always executes fresh, never reads a prior report.json
    as current evidence. Returns ok=False (not an exception) for every unavailable/skipped precondition."""
    game_root = Path(game_root).resolve()
    script = game_root / 'scripts/check.py'
    result = {'format': FORMAT, 'generated_at': _now(), 'game_root': str(game_root), 'available': script.is_file()}
    if not result['available']:
        result.update(ok=False, reason=f'{script} not found; nothing to reuse. Add scripts/check.py '
                                        '(templates/game_check.py; new-game scaffolds this already).')
        return result
    module = game_check_module(game_root)
    tool_path = module.find_tools(game_root) if hasattr(module, 'find_tools') else None
    dep_path = module.engine_path(game_root) if hasattr(module, 'engine_path') else None
    parse_json_output = getattr(module, 'parse_json_output', _fallback_parse_json_output)
    engine_commit, engine_dirty = git_head(dep_path) if dep_path else (None, None)
    result['engine_at_verification'] = {
        'dependency_path': str(dep_path) if dep_path else None, 'commit': engine_commit, 'dirty': engine_dirty,
        'tool_binary': {'path': str(tool_path), 'sha256': sha256_of(tool_path)} if tool_path else None}
    if not tool_path:
        result.update(ok=False, reason='No be2-tools binary found; scripts/check.py cannot run native checks.')
        return result
    args = [sys.executable, str(script), '--tools', str(tool_path)]
    if content_only:
        args.append('--content-only')
    if skip_ship:
        args.append('--skip-ship')
    for scenario in scenarios:
        args += ['--scenario', scenario]
    started = time.monotonic()
    try:
        completed = subprocess.run(args, cwd=game_root, capture_output=True, text=True, encoding='utf-8',
                                   errors='replace', timeout=timeout)
    except subprocess.TimeoutExpired:
        result.update(ok=False, reason=f'scripts/check.py exceeded {timeout}s', command=args)
        return result
    result.update(command=args, returncode=completed.returncode,
                  elapsed_seconds=round(time.monotonic() - started, 3))
    verdict = parse_json_output(completed.stdout)
    if verdict is None:
        result.update(ok=False, reason='scripts/check.py printed no parseable JSON summary',
                      stdout_tail=(completed.stdout or '')[-2000:], stderr_tail=(completed.stderr or '')[-2000:])
        return result
    result['game_check'] = verdict
    report_path = verdict.get('report')
    if report_path and Path(report_path).is_file():
        try:
            result['game_check_detail'] = json.loads(Path(report_path).read_text(encoding='utf-8'))
        except (OSError, ValueError):
            pass
    result['ok'] = completed.returncode == 0 and bool(verdict.get('ok'))
    result['human_summary'] = format_verify_human(result)
    return result


def format_verify_human(result):
    if not result.get('available', True):
        return f'Verify: {result["game_root"]}: unavailable - {result["reason"]}'
    if 'reason' in result and 'game_check' not in result:
        return f'Verify: {result["game_root"]}: FAIL (precondition) - {result["reason"]}'
    status = 'PASS' if result['ok'] else 'FAIL'
    engine_commit = result['engine_at_verification']['commit'] or 'unknown'
    lines = [f'Verify: {result["game_root"]}: {status} ({result["elapsed_seconds"]}s)',
            f'  engine: {result["engine_at_verification"]["dependency_path"]} @ {engine_commit[:12]}']
    game_check = result.get('game_check', {})
    if game_check:
        lines.append(f'  game_check: ok={game_check.get("ok")} ship={game_check.get("ship")} '
                    f'report={game_check.get("report")}')
        if game_check.get('warnings'):
            lines.extend(f'    warning: {w}' for w in game_check['warnings'])
    return '\n'.join(lines)
