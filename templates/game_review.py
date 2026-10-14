"""Content-bound manual native review, shared by the existing project shipping commands.

A receipt records reviewer attestations, not independently measured hardware behavior.
Package integrity, build provenance, automatic capture and physical review remain distinct.
"""
import hashlib
import json
import os
from pathlib import Path
import platform
import re

VERSION = 1
SPEC = 'assets/native-review.json'
RECEIPT = '.blue-check/native-review.json'
# Only root-level generated outputs are omitted. Authored assets/scripts/config stay inputs.
OUTPUTS = {'.git', 'target', 'dist', 'installers', '.blue-check', '.be2-work', '.learning'}
ENGINE_INPUTS = ('Cargo.toml', 'Cargo.lock', 'build.rs', '.cargo', 'src', 'assets', 'templates',
                 'tools/FEATURES.json', 'tools/game_tooling.py', 'tools/mcp.schemas.json',
                 'tools/game.schema.json', 'docs/archive/AI_REFERENCE.md',
                 'deploy/hub/hub.conf.example', 'PACKAGE-MANIFEST.json')
AREAS = {'controls', 'camera', 'ui', 'audio', 'gameplay', 'other'}
LIMITATIONS = [
    'Manual results are explicit reviewer attestations; this receipt is not a signature or independent hardware measurement.',
    'Software rendering, scripted input and numeric/audio counters cannot certify physical devices.',
    'Review does not replace simulation, package, installer or other declared platform gates.',
]


class ReviewError(ValueError):
    pass


def _json_hash(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':'),
                                     ensure_ascii=False).encode('utf-8')).hexdigest()


def _path(root, name):
    path = Path(name)
    candidate = path if path.is_absolute() else Path(root) / path
    if any(part.is_symlink() for part in (candidate, *candidate.parents)):
        raise ReviewError(f'{candidate}: review paths cannot traverse symlinks; use the regular file path')
    return candidate.resolve()


def _receipt_path(project, name):
    path = _path(project.root, name or RECEIPT)
    try:
        relative = path.relative_to(project.root)
    except ValueError:
        engine = project.engine()
        if engine:
            try:
                engine_path = path.relative_to(Path(engine).resolve())
            except ValueError:
                engine_path = None
            if engine_path and any(engine_path == Path(item) or Path(item) in engine_path.parents
                                   for item in ENGINE_INPUTS):
                raise ReviewError('Review JSON outputs must stay outside engine source inputs')
        return path
    if not relative.parts or relative.parts[0] != '.blue-check':
        raise ReviewError('In-project receipts belong under .blue-check; choose an output path or an external review directory')
    return path


def _read(path):
    try:
        value = json.loads(Path(path).read_text(encoding='utf-8'))
    except (OSError, UnicodeError, ValueError) as error:
        raise ReviewError(f'{path}: unreadable review JSON: {error}; completed outputs were preserved') from error
    if (not isinstance(value, dict) or type(value.get('schema_version')) is not int
            or value['schema_version'] != VERSION):
        raise ReviewError(f'{path}: expected a JSON object with schema_version {VERSION}')
    return value


def _text(value):
    return isinstance(value, str) and bool(value.strip())


def _inventory(root, ship, inputs=None):
    """Explicit source scope: exact relative names and bytes, without output/player-file drift."""
    root = Path(root).resolve()
    files = {}
    pending = [root / name for name in inputs] if inputs else [root]
    for entry in pending:
        if not entry.exists() and not entry.is_symlink():
            continue
        if entry.is_file():
            files[entry.relative_to(root).as_posix()] = ship.sha256_file(entry)
            continue
        for folder, directories, names in os.walk(entry):
            directory = Path(folder)
            relative = directory.relative_to(root).parts
            fixture_root = (len(relative) == 3 and relative[:2] == ('assets', 'games')
                            and (directory / 'Cargo.toml').is_file())
            directories[:] = sorted(d for d in directories if d != '__pycache__'
                                    and not ((directory == root or fixture_root) and d in OUTPUTS))
            for name in directories:
                if (directory / name).is_symlink():
                    raise ReviewError(f'{directory / name}: source directory symlink cannot be bound; use a regular checkout')
            for name in sorted(names):
                path = directory / name
                if name.endswith(('.pyc', '.pyo')):
                    continue
                key = path.relative_to(root).as_posix()
                if path.is_file():
                    files[key] = {'sha256': ship.sha256_file(path),
                                  **({'symlink': os.readlink(path)} if path.is_symlink() else {})}
    return {'sha256': _json_hash(files), 'files': len(files)}


def source_binding(project, ship):
    """Source hash for cmd_package to capture before building and compare after packaging.

    The engine path comes from the existing Project dependency resolver. Hashes do
    not claim that its Git revision is enforced or that a prebuilt exe is fresh.
    """
    engine = project.engine()
    if not engine or not (Path(engine) / 'Cargo.toml').is_file():
        raise ReviewError('Engine source inputs are unavailable; restore the pinned source checkout before recording build provenance')
    game = _inventory(project.root, ship)
    engine_files = _inventory(engine, ship, ENGINE_INPUTS)
    inputs = {'game': game, 'engine': engine_files}
    return {**inputs, 'sha256': _json_hash(inputs),
            'scope': {'game': 'all project files except root-level generated outputs and Python caches',
                      'engine': list(ENGINE_INPUTS), 'excluded_game_outputs': sorted(OUTPUTS)}}


def _checks(checks, path):
    if not isinstance(checks, list) or not checks:
        raise ReviewError(f'{path}: declare a nonempty checks list with game-owned id, area and instructions')
    identifiers = set()
    for check in checks:
        if (not isinstance(check, dict) or not isinstance(check.get('id'), str)
                or not re.fullmatch(r'[a-z][a-z0-9_-]{0,63}', check['id'])
                or check['id'] in identifiers or not isinstance(check.get('area'), str)
                or check['area'] not in AREAS
                or not _text(check.get('instructions'))):
            raise ReviewError(f'{path}: each check needs a unique stable id, a supported area and nonempty instructions')
        identifiers.add(check['id'])
    return checks


def _spec(path):
    spec = _read(path)
    _checks(spec.get('checks'), path)
    return spec


def _artifact(path, ship):
    path = _path(path.parent, path)
    if not path.is_file() or path.is_symlink():
        raise ReviewError(f'{path}: review artifact is missing or not a regular file; retain the capture/settings file and retry')
    return {'path': str(path), 'sha256': ship.sha256_file(path)}


def _binding(project, ship, spec, settings, artifacts):
    paths = [spec, *([settings] if settings else []), *artifacts]
    if len(set(paths)) != len(paths):
        raise ReviewError('Spec, settings and capture artifacts must be separate files')
    files = ship.package_integrity(project, project.platform)
    stamp = project.read_stamp()
    source = source_binding(project, ship)
    return {'target_os': project.platform, 'source': source,
            'package': {'exe': stamp['exe'], 'files': {name: stamp['file_sha256'][name] for name in sorted(files)},
                        'engine_revision': stamp.get('engine_revision'),
                        'build_source_sha256': stamp.get('review_source_sha256')},
            'spec': _artifact(spec, ship),
            'settings': _artifact(settings, ship) if settings else None,
            'artifacts': [_artifact(path, ship) for path in artifacts]}


def _environment(ship):
    software = os.environ.get('LIBGL_ALWAYS_SOFTWARE', '').lower() in ('1', 'true', 'yes')
    software = software or os.environ.get('GALLIUM_DRIVER', '').lower() in ('llvmpipe', 'softpipe')
    return {'os': ship.host_platform(), 'os_version': platform.platform(), 'software_renderer_requested': software}


def _template(binding, checks):
    return {'schema_version': VERSION, 'binding_sha256': _json_hash(binding),
            'reviewer': '', 'attested': False, 'context': '', 'launched_package': False,
            'used_bound_settings': False,
            'environment': {'mode': 'unknown', 'os': binding['target_os'], 'os_version': '',
                            'display': '', 'renderer': 'unknown', 'renderer_description': '',
                            'input_devices': [], 'audio_output': '', 'audio_output_kind': 'unknown',
                            'scripted_input': False, 'physical_devices_attested': False},
            'checks': [{'id': check['id'], 'result': 'not-run', 'observation': ''} for check in checks],
            'notes': ''}


def _load(project, receipt):
    path = _receipt_path(project, receipt)
    data = _read(path)
    if (not isinstance(data.get('binding'), dict) or not isinstance(data.get('records'), list)
            or data.get('binding_sha256') != _json_hash(data['binding'])):
        raise ReviewError(f'{path}: invalid receipt; preserve it and prepare a new receipt')
    try:
        _checks(data.get('checks'), path)
    except ReviewError as error:
        raise ReviewError(f'{error}; invalid receipt checklist, preserve it and prepare a new receipt') from error
    for number, record in enumerate(data['records'], 1):
        if (not isinstance(record, dict) or not isinstance(record.get('observations'), dict)
                or not isinstance(record.get('observed_environment'), dict)):
            raise ReviewError(f'{path}: invalid receipt record {number}; observations and observed_environment '
                              'must be JSON objects; preserve it and prepare a new receipt')
    return path, data


def _current(project, ship, data):
    bound = data['binding']
    return _binding(project, ship, Path(bound['spec']['path']),
                    Path(bound['settings']['path']) if bound.get('settings') else None,
                    [Path(item['path']) for item in bound['artifacts']])


def _record_problems(record, data):
    errors = []
    observation = record.get('observations', {})
    env = observation.get('environment')
    env = env if isinstance(env, dict) else {}
    observed = record.get('observed_environment', {})
    if observation.get('binding_sha256') != data['binding_sha256']:
        errors.append('Observation binding_sha256 differs; use the current prepared template')
    for key in ('reviewer', 'context'):
        if not _text(observation.get(key)):
            errors.append(f'Provide a nonempty {key} identifying the reviewer/session')
    for key in ('attested', 'launched_package', 'used_bound_settings'):
        if observation.get(key) is not True:
            errors.append(f'{key} must be explicitly true after observing this package/settings')
    if env.get('mode') != 'physical-manual' or env.get('renderer') != 'hardware':
        errors.append('Physical review requires mode physical-manual and a hardware renderer; software/unknown captures remain pending')
    if observed.get('software_renderer_requested') or env.get('scripted_input') is not False:
        errors.append('Software rendering or scripted/unknown input cannot certify physical devices')
    if env.get('physical_devices_attested') is not True:
        errors.append('Attest explicitly that the described physical devices were exercised by the reviewer')
    renderer = str(env.get('renderer_description', '')).lower()
    if any(name in renderer for name in ('llvmpipe', 'softpipe', 'swiftshader', 'swrast')) or re.search(r'\bwarp\b', renderer):
        errors.append('The named renderer is software; its results remain pending physical review')
    if re.search(r'\bxvfb\b', str(env.get('display', '')).lower()):
        errors.append('The named Xvfb display is software capture, not a physical display')
    if env.get('os') != data['binding']['target_os'] or env.get('os') != observed.get('os'):
        errors.append('Review OS must match the package target and actual recording host; run the review on that OS')
    for key in ('os_version', 'display', 'renderer_description'):
        if not _text(env.get(key)):
            errors.append(f'Provide the physical session {key}')
    devices = env.get('input_devices')
    if not isinstance(devices, list) or not devices or not all(_text(device) for device in devices):
        errors.append('Name the actual physical input devices used')
    if any(check['area'] == 'audio' for check in data['checks']):
        if env.get('audio_output_kind') != 'physical' or not _text(env.get('audio_output')):
            errors.append('Audio checks require the actual physical output device, not decoded buffers/submission counters')
        output = str(env.get('audio_output', '')).lower()
        if re.search(r'\b(?:null|dummy)\s+(?:pcm|sink|audio|output|device)\b|\bsnd[-_]dummy\b', output):
            errors.append('The named null/dummy audio output cannot certify audible physical playback')
    observations = observation.get('checks')
    if not isinstance(observations, list):
        observations = []
    expected = {check['id'] for check in data['checks']}
    found = [check['id'] for check in observations
             if isinstance(check, dict) and isinstance(check.get('id'), str)]
    if len(found) != len(observations) or len(found) != len(set(found)) or set(found) - expected:
        errors.append('Observation check IDs are duplicate or undeclared; use the authored checklist')
    for check in observations:
        if not isinstance(check, dict) or check.get('result') not in ('pass', 'fail', 'not-run'):
            errors.append('Each observation needs result pass, fail or not-run')
        elif check['result'] in ('pass', 'fail') and not _text(check.get('observation')):
            errors.append(f'{check.get("id")}: passing/failing claims need a nonempty observed outcome')
    return errors


def status(project, ship, receipt=None):
    path, data = _load(project, receipt)
    diagnostics, stale = [], False
    try:
        current = _current(project, ship, data)
        stale = _json_hash(current) != data.get('binding_sha256')
        if stale:
            diagnostics.append('Source/package/spec/settings/artifact content changed; preserve this receipt and prepare a new one')
        if current['package']['build_source_sha256'] != current['source']['sha256']:
            diagnostics.append('Package has no matching source-build binding; package from current inputs before physical review')
        if _spec(Path(current['spec']['path']))['checks'] != data['checks']:
            stale = True
            diagnostics.append('Receipt checklist differs from the authored spec; preserve it and prepare a new receipt')
    except (ValueError, OSError, KeyError, ship.ShipError) as error:
        stale = True
        diagnostics.append(f'Current inputs cannot be verified: {error}; completed review notes were preserved')
    results = {check['id']: 'not-run' for check in data['checks']}
    history_problems = []
    for number, record in enumerate(data['records']):
        problems = _record_problems(record, data)
        if number == len(data['records']) - 1:
            diagnostics.extend(problems)
        elif problems:
            history_problems.append({'record': number + 1, 'diagnostics': problems})
        if not problems:
            for check in record['observations']['checks']:
                if check['result'] != 'not-run':
                    results[check['id']] = check['result']
        else:
            # A software review can expose a defect. It still cannot certify a physical pass.
            observations = record.get('observations', {}).get('checks')
            for check in observations if isinstance(observations, list) else []:
                if (isinstance(check, dict) and isinstance(check.get('id'), str) and check.get('id') in results
                        and check.get('result') == 'fail' and _text(check.get('observation'))):
                    results[check['id']] = 'fail'
    failed = [key for key, value in results.items() if value == 'fail']
    pending = [key for key, value in results.items() if value != 'pass']
    manual = bool(results) and not stale and not diagnostics and not pending
    return {'ok': not stale and not failed, 'manual': manual,
            'state': 'stale' if stale else 'failed' if failed else 'passed' if manual else 'pending',
            'receipt': str(path), 'binding_sha256': data['binding_sha256'],
            'checks': results, 'pending': pending, 'diagnostics': list(dict.fromkeys(diagnostics)),
            'historical_incomplete_records': len(history_problems),
            'next': '' if manual else 'Complete authored checks on physical devices for the target OS; record JSON observations. Reprepare if inputs changed.',
            'limitations': LIMITATIONS}


def prepare(project, ship, spec=None, receipt=None, settings=None, artifacts=()):
    spec_path = _path(project.root, spec or SPEC)
    checks = _spec(spec_path)['checks']
    path = _receipt_path(project, receipt)
    if path.exists():
        raise ReviewError(f'{path}: receipt already exists; inspect review status or use --receipt NEW.json; existing observations were preserved')
    binding = _binding(project, ship, spec_path, _path(project.root, settings) if settings else None,
                       [_path(project.root, artifact) for artifact in artifacts])
    data = {'schema_version': VERSION, 'prepared_at': ship.iso_now(), 'binding': binding,
            'binding_sha256': _json_hash(binding), 'checks': checks, 'records': [],
            'observation_template': _template(binding, checks), 'limitations': LIMITATIONS}
    path.parent.mkdir(parents=True, exist_ok=True)
    ship.write_json_atomic(path, data)
    return status(project, ship, path)


def record(project, ship, observations, receipt=None):
    path, data = _load(project, receipt)
    observation_path = _path(project.root, observations)
    try:
        relative = observation_path.relative_to(project.root)
    except ValueError:
        relative = None
    if relative is not None and relative.parts[0] != '.blue-check':
        raise ReviewError('Save observation JSON under .blue-check or outside the source project; otherwise it changes the reviewed source')
    inputs = data['binding']
    bound_paths = [path, Path(inputs['spec']['path']),
                   *([Path(inputs['settings']['path'])] if inputs.get('settings') else []),
                   *[Path(item['path']) for item in inputs['artifacts']]]
    if observation_path in bound_paths:
        raise ReviewError('Observation JSON must be separate from the receipt, spec, settings and capture artifacts')
    observation = _read(observation_path)
    data['records'].append({'recorded_at': ship.iso_now(), 'observations': observation,
                            'observations_sha256': ship.sha256_file(observation_path),
                            'observed_environment': _environment(ship)})
    ship.write_json_atomic(path, data)
    # Invalid/partial observations are retained, and diagnostics explain why they cannot pass.
    return status(project, ship, path)
