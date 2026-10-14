"""Shared Inno preparation/build for pinned native game tooling.

Preparation checks bytes and emits reviewable input on any host. Compilation and
the explicit install/upgrade/uninstall test require Windows; neither certifies
physical devices. Existing game-owned Inno scripts are not overwritten.
"""
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tempfile


UNVERIFIED = [
    'Installer installation/upgrade/uninstall has not run; use installer test on an isolated Windows account.',
    'Windows executable resources and native packaged-game smoke still require the existing Windows shipping gate.',
    'Physical GPU/input/audio devices and public release are not certified by installer preparation or compilation.',
]


def literal(value, *, constants=True):
    """Inno quoted value, with no user-controlled preprocessor or constants.

    The generated script starts with #preproc builtin, disabling ISPP inline
    expressions. Quotes are doubled and literal opening braces are escaped:
    https://jrsoftware.org/ishelp/topic_params.htm
    https://jrsoftware.org/ishelp/topic_consts.htm
    """
    if not isinstance(value, str) or not value or any(ord(c) < 32 or 127 <= ord(c) <= 159 or c in '\u2028\u2029' for c in value):
        raise ValueError('installer values must be non-empty single-line strings without control characters')
    escaped = value.replace('"', '""')
    if constants:
        escaped = escaped.replace('{', '{{')
    return '"' + escaped + '"'


def _directive(value, *, constants=True):
    # Setup directives strip their outer quotes but, unlike section parameters,
    # preserve embedded quotes rather than unescaping doubled quotes.
    # https://github.com/jrsoftware/issrc/blob/main/Projects/Src/Compiler.SetupCompiler.pas
    literal(value, constants=constants)  # common single-line validation
    return '"' + (value.replace('{', '{{') if constants else value) + '"'


def _constant_path(prefix, suffix):
    # Only this maintained code supplies the Inno constant. Suffix is game data.
    return '"' + prefix + '\\' + literal(suffix)[1:-1] + '"'


def _config(project, ship):
    path = project.root / 'scripts/installer.json'
    config = {}
    if path.exists():
        if path.is_symlink():
            raise ship.ConfigError(f'{path}: installer settings must be a regular file')
        config = ship.read_json_file(path)
        if not isinstance(config, dict) or set(config) - {'app_id', 'publisher'}:
            raise ship.ConfigError(f'{path}: use an object containing only app_id and optional publisher')
    package = project.cargo.get('package', {})
    name, version = package.get('name'), package.get('version')
    if not isinstance(name, str) or not re.fullmatch(r'[A-Za-z0-9_-]+', name):
        raise ship.ConfigError('installer needs a literal Cargo package.name; keep a stable scripts/installer.json app_id when renaming')
    if not isinstance(version, str) or not re.fullmatch(r'\d+\.\d+\.\d+(?:[-+][A-Za-z0-9.+-]+)?', version):
        raise ship.ConfigError('installer needs a literal semantic Cargo package.version, such as 0.1.0')
    app_id = config.get('app_id', 'BlueEngineGames.' + name)
    if not isinstance(app_id, str) or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]{0,126}', app_id):
        raise ship.ConfigError(f'{path}: app_id must be 1..127 ASCII letters/digits/dots/hyphens/underscores and remain stable across upgrades')
    publisher = config.get('publisher')
    try:
        if publisher is not None:
            literal(publisher)
        literal(project.identity.title)
        literal(project.exe_stem())
    except ValueError as error:
        raise ship.ConfigError(f'{path}: {error}') from error
    return {'app_id': app_id, 'publisher': publisher, 'version': version,
            'install_slug': name, 'title': project.identity.title, 'exe': project.exe_file('windows')}


def _pe_executable(path, ship, *, installer=False):
    # Read bounded PE headers without executing Windows code on the preparation host.
    with path.open('rb') as stream:
        dos = stream.read(64)
        if len(dos) != 64 or dos[:2] != b'MZ':
            raise ship.ShipError(f'{path}: expected a Windows PE executable; package/build on Windows first')
        offset = struct.unpack_from('<I', dos, 60)[0]
        if offset < 64 or offset > path.stat().st_size - 26:
            raise ship.ShipError(f'{path}: invalid Windows PE header offset; package on Windows first')
        stream.seek(offset)
        header = stream.read(26)
    machine = struct.unpack_from('<H', header, 4)[0]
    optional_size = struct.unpack_from('<H', header, 20)[0]
    flags = struct.unpack_from('<H', header, 22)[0]
    format_pair = (machine, header[24:26])
    accepted = {(0x8664, b'\x0b\x02')}
    if installer:  # Inno 6 commonly emits a 32-bit installer for the x64 game.
        accepted.add((0x14c, b'\x0b\x01'))
    if (header[:4] != b'PE\0\0' or format_pair not in accepted or optional_size < 2 or
            offset + 24 + optional_size > path.stat().st_size or not flags & 0x0002 or flags & 0x2000):
        target = 'Windows x86/x64 installer' if installer else 'AMD64 PE32+ game'
        raise ship.ShipError(f'{path}: requires a {target} executable, not another target, truncated header or DLL')


def _player_path(name):
    first = name.split('/')[0].casefold()
    return first in {'save', 'saves', 'logs', '.blue-check', '.be2-saves', 'settings.json', 'records.json'}


def _script(config, files):
    title, exe, slug = config['title'], config['exe'], config['install_slug']
    stem = exe[:-4]
    lines = [
        '#preproc builtin',
        '; Generated from verified manifest declarations. Review scripts/installer.json for identity changes.',
        '[Setup]', 'AppId=' + _directive(config['app_id']), 'AppName=' + _directive(title),
        'AppVersion=' + _directive(config['version']),
        'VersionInfoProductName=' + _directive(title, constants=False),
        'VersionInfoDescription=' + _directive(title, constants=False),
        'DefaultDirName=' + _constant_path('{localappdata}', 'BlueEngine\\Games\\' + slug),
        'DefaultGroupName=' + _directive(title), 'PrivilegesRequired=lowest',
        'ArchitecturesAllowed=x64compatible', 'ArchitecturesInstallIn64BitMode=x64compatible',
        'UsePreviousAppDir=yes', 'DisableProgramGroupPage=yes',
        'OutputDir=output', 'OutputBaseFilename=' + _directive(stem + '-setup-windows-x64', constants=False),
        'Compression=lzma2', 'SolidCompression=yes', 'WizardStyle=modern',
        'SetupIconFile=' + _directive('payload\\' + stem + '.ico', constants=False),
        'UninstallDisplayIcon=' + _constant_path('{app}', exe),
        'CloseApplications=yes', 'RestartApplications=no',
    ]
    if config['publisher'] is not None:
        lines.append('AppPublisher=' + _directive(config['publisher']))
        lines.append('VersionInfoCompany=' + _directive(config['publisher'], constants=False))
    lines += ['[Tasks]', 'Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked', '[Files]']
    for name in files:
        directory = str(Path(name).parent).replace('/', '\\')
        destination = '"{app}"' if directory == '.' else _constant_path('{app}', directory)
        # Inno does not translate constants in bundled Source paths. Preserve
        # braces there while escaping them in runtime DestDir/shortcut values.
        lines.append('Source: ' + literal('payload\\' + name.replace('/', '\\'), constants=False) + '; DestDir: ' + destination + '; Flags: ignoreversion')
    lines += [
        '[Icons]',
        'Name: ' + _constant_path('{userprograms}', title) + '; Filename: ' + _constant_path('{app}', exe) + '; WorkingDir: "{app}"',
        'Name: ' + _constant_path('{userdesktop}', title) + '; Filename: ' + _constant_path('{app}', exe) + '; WorkingDir: "{app}"; Tasks: desktopicon',
        '[Run]', 'Filename: ' + _constant_path('{app}', exe) + '; Description: ' + literal('Launch ' + title) + '; Flags: nowait postinstall skipifsilent',
    ]
    return '\n'.join(lines) + '\n'


def _binding(project, ship, config):
    return {'ship_manifest_sha256': ship.sha256_file(project.stamp_path),
            'identity_sha256': ship.sha256_file(project.identity_path),
            'cargo_sha256': ship.sha256_file(project.root / 'Cargo.toml'),
            'settings_sha256': ship.sha256_file(project.root / 'scripts/installer.json') if (project.root / 'scripts/installer.json').is_file() else None,
            'producer_sha256': {'game_installer.py': ship.sha256_file(Path(__file__)),
                                'game_ship.py': ship.sha256_file(Path(ship.__file__))},
            'config': config}


def prepare(project, ship):
    files = sorted(ship.package_integrity(project, 'windows'))
    _pe_executable(project.dist / project.exe_file('windows'), ship)
    config = _config(project, ship)
    for name in files:
        if _player_path(name):
            raise ship.ConfigError(f'{name}: player saves/settings/logs must not be installer-owned; remove from identity.package and package again')
    binding = _binding(project, ship, config)
    # The existing source exporter excludes .blue-check at Cargo roots. Keep
    # generated payloads/compiler outputs there, separate from authored Inno
    # sources and established game-owned delivery directories.
    parent = ship.package_file(project.root, '.blue-check/installers')
    parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix='prepared-', dir=parent))
    try:
        payload = stage / 'payload'
        payload.mkdir()
        hashes = project.read_stamp()['file_sha256']
        for name in files:
            source = ship.package_file(project.dist, name)
            destination = ship.package_file(payload, name)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
            if ship.sha256_file(destination) != hashes[name]:
                raise ship.ShipError(f'{source}: changed while preparing installer; package/prepare again')
        script = stage / 'installer.iss'
        # UTF-8 BOM selects Unicode decoding explicitly for supported Inno 6
        # versions, independent of the Windows machine's ANSI code page.
        ship.write_text_atomic(script, '\ufeff' + _script(config, files))
        # Keep every completed generation. Do not replace an earlier good installer.
        if _binding(project, ship, _config(project, ship)) != binding:
            raise ship.ShipError('package or installer configuration changed during preparation; prepare again')
        report = {'ok': True, 'command': 'installer prepare', 'stage': str(stage), 'script': str(script),
                  'script_sha256': ship.sha256_file(script), 'files': files, 'file_sha256': {n: hashes[n] for n in files},
                  'package_binding': binding, 'config': config,
                  'package_shipping_evidence': project.read_stamp().get('verified'),
                  'unverified': UNVERIFIED[:], 'next': 'On Windows: python scripts/ship.py installer build --iscc "PATH/ISCC.exe"'}
        ship.write_json_atomic(stage / 'prepared.json', report)
        return report
    except Exception as error:
        raise ship.ShipError(f'installer preparation failed in {stage}: {error}; completed outputs were preserved') from error


def _validate_stage(project, ship, stage):
    stage = Path(stage)
    if not stage.is_absolute():
        stage = project.root / stage
    parent = ship.package_file(project.root, '.blue-check/installers')
    if stage.is_symlink():
        raise ship.ShipError('installer stage/output selectors must be regular directories, not symlinks')
    try:
        relative = stage.relative_to(parent).as_posix()
    except ValueError as error:
        raise ship.ShipError('installer stage must be inside .blue-check/installers/; prepare/build again (older completed outputs remain untouched)') from error
    # Reuse the package path checker before resolving: an intermediate symlink
    # must not become invisible through Path.resolve().
    ship.package_file(parent, relative)
    stage = stage.resolve()
    if not ship.inside(stage, parent):
        raise ship.ShipError('installer stage must be a prepared directory inside this game\'s .blue-check/installers/')
    for path in (stage / 'prepared.json', stage / 'built.json', stage / 'output'):
        if path.is_symlink():
            raise ship.ShipError(f'{path}: installer reports/output must not be symlinks')
    report = ship.read_json_file(stage / 'prepared.json')
    if not isinstance(report, dict) or not report.get('ok'):
        raise ship.ShipError(f'{stage}/prepared.json is missing/invalid; run installer prepare again')
    current = _binding(project, ship, _config(project, ship))
    if current != report.get('package_binding'):
        raise ship.ShipError('package/configuration changed since preparation; run installer build again')
    files = ship.package_integrity(project, 'windows')
    if set(files) != set(report.get('files', [])):
        raise ship.ShipError('installer stage inventory differs from package; prepare again')
    for name in files:
        file = ship.package_file(stage / 'payload', name)
        if not file.is_file() or ship.sha256_file(file) != report['file_sha256'].get(name):
            raise ship.ShipError(f'{file}: prepared input changed; prepare again')
    script = stage / 'installer.iss'
    if script.is_symlink() or not script.is_file() or ship.sha256_file(script) != report.get('script_sha256'):
        raise ship.ShipError(f'{script}: generated script changed; prepare again (custom Inno scripts remain separate)')
    return stage, report


def _compiler_logs(stage, stdout, stderr):
    for name, value in (('compiler.stdout', stdout), ('compiler.stderr', stderr)):
        if isinstance(value, bytes):
            value = value.decode('utf-8', errors='replace')
        (stage / name).write_text(value or '', encoding='utf-8')


def build(project, ship, iscc):
    if ship.host_platform() != 'windows':
        raise ship.ShipError('installer build requires Windows and Inno Setup 6; installer prepare can run on Linux')
    selected = Path(iscc).expanduser()
    if selected.suffix.casefold() != '.exe':
        raise ship.ShipError('ISCC override must be a native Windows .exe, not a .bat/.cmd wrapper; pass the explicit Inno Setup compiler path')
    compiler = selected.resolve()
    if not compiler.is_file() or compiler.is_symlink():
        raise ship.ShipError(f'{compiler}: ISCC.exe is missing; pass --iscc with the explicit Inno Setup 6 compiler path')
    # Windows may invoke cmd.exe implicitly for batch files even with an argv
    # list. Require a native image before constructing any game/stage arguments.
    try:
        _pe_executable(compiler, ship, installer=True)
    except ship.ShipError as error:
        raise ship.ShipError(f'{compiler}: ISCC override must be a native Windows x86/x64 PE .exe ({error})') from error
    compiler_hash = ship.sha256_file(compiler)
    report = prepare(project, ship)
    stage = Path(report['stage'])
    output = stage / 'output'
    output.mkdir()
    command = [str(compiler), '/Qp', '/O' + str(output), str(stage / 'installer.iss')]
    try:
        done = ship.run_process(command, cwd=stage, timeout=600)
        _compiler_logs(stage, done.stdout, done.stderr)
        if done.returncode:
            raise ship.ShipError(f'ISCC failed (exit {done.returncode}); inspect {stage}/compiler.stderr and compiler.stdout; Inno Setup 6 with x64compatible support is required')
        _validate_stage(project, ship, stage)
        if ship.sha256_file(compiler) != compiler_hash:
            raise ship.ShipError('ISCC compiler changed during compilation; build again with a stable compiler')
        artifact = output / (project.exe_stem() + '-setup-windows-x64.exe')
        if not artifact.is_file() or artifact.is_symlink() or artifact.stat().st_size == 0:
            raise ship.ShipError(f'ISCC reported success without {artifact}; no installer result was accepted')
        _pe_executable(artifact, ship, installer=True)
        result = {**report, 'command': 'installer build', 'compiler': str(compiler),
                  'compiler_sha256': compiler_hash, 'compiler_command': command,
                  'installer': str(artifact), 'installer_sha256': ship.sha256_file(artifact),
                  'next': 'On an isolated Windows account: python scripts/ship.py installer test --stage "' + str(stage) + '"'}
        ship.write_json_atomic(stage / 'built.json', result)
        return result
    except subprocess.TimeoutExpired as error:
        _compiler_logs(stage, error.stdout, error.stderr)
        raise ship.ShipError(f'ISCC timed out after {error.timeout} seconds in {stage}; inspect compiler.stdout/compiler.stderr; prior installers and completed outputs were preserved') from error
    except (OSError, subprocess.SubprocessError, ship.ShipError) as error:
        raise ship.ShipError(f'installer build failed in {stage}: {error}; prior installers and completed outputs were preserved') from error


def test_install(project, ship, stage):
    """Explicit destructive test limited to a fresh temp directory and absent AppId.

    This is the shared form of Reverie's install/upgrade/uninstall check. It never
    runs during prepare/build and refuses to replace a registered installation.
    """
    if ship.host_platform() != 'windows':
        raise ship.ShipError('installer test requires an isolated Windows account; no installation was attempted')
    stage, prepared = _validate_stage(project, ship, stage)
    built = ship.read_json_file(stage / 'built.json')
    artifact = stage / 'output' / (project.exe_stem() + '-setup-windows-x64.exe')
    if (not isinstance(built, dict) or built.get('script_sha256') != prepared['script_sha256'] or
            built.get('package_binding') != prepared['package_binding'] or built.get('file_sha256') != prepared['file_sha256'] or
            not artifact.is_file() or artifact.is_symlink() or ship.sha256_file(artifact) != built.get('installer_sha256')):
        raise ship.ShipError('installer artifact or build binding is missing/changed; run installer build again')
    import winreg
    app_id = prepared['config']['app_id']
    registry = 'Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\' + app_id + '_is1'
    # Match both per-user/per-machine and 32-/64-bit registrations before any
    # installer runs. UsePreviousAppDir must never adopt an existing install.
    for hive in (winreg.HKEY_CURRENT_USER, winreg.HKEY_LOCAL_MACHINE):
        for view in (winreg.KEY_WOW64_32KEY, winreg.KEY_WOW64_64KEY):
            try:
                with winreg.OpenKey(hive, registry, 0, winreg.KEY_READ | view):
                    raise ship.ShipError(f'{app_id}: an installation is already registered; test on an isolated Windows account')
            except FileNotFoundError:
                pass
            except OSError as error:
                raise ship.ShipError(f'{app_id}: could not rule out an existing Windows installation ({error}); no installation was attempted') from error
    programs = ship._known_folder('A77F5D77-2E2B-44C3-A6A2-ABA601054A51')
    if not programs:
        raise ship.ShipError('cannot resolve the current user Programs folder; no installation was attempted')
    shortcut = Path(programs) / (prepared['config']['title'] + '.lnk')
    if shortcut.exists():
        raise ship.ShipError(f'{shortcut}: existing shortcut would be replaced; test on an isolated Windows account')
    install = Path(tempfile.mkdtemp(prefix='blueengine-installer-test-'))
    commands = []
    def run(command):
        commands.append(command)
        done = ship.run_process(command, cwd=stage, timeout=600)
        (stage / f'lifecycle-{len(commands):02}.stdout').write_text(done.stdout or '', encoding='utf-8')
        (stage / f'lifecycle-{len(commands):02}.stderr').write_text(done.stderr or '', encoding='utf-8')
        if done.returncode:
            raise ship.ShipError(f'installer lifecycle command failed (exit {done.returncode}): {command[0]}; inspect installation at {install}')
    try:
        command = [str(artifact), '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', '/DIR=' + str(install)]
        run(command)
        for name in prepared['files']:
            file = ship.package_file(install, name)
            if not file.is_file() or ship.sha256_file(file) != prepared['file_sha256'][name]:
                raise ship.ShipError(f'{file}: installed package differs from prepared bytes')
        link = ship.read_shortcut(shortcut)
        if not link or not ship.same_file(link.get('target', ''), install / project.exe_file('windows')) or not ship.same_file(link.get('working_dir', ''), install):
            raise ship.ShipError(f'{shortcut}: installed shortcut target/working directory differs')
        saves = install / 'saves'
        saves.mkdir()
        note = saves / 'installer-retention.txt'
        note.write_text('preserve player state', encoding='utf-8')
        settings = install / 'settings.json'
        settings.write_text('{"retain":true}', encoding='utf-8')
        run(command)
        if note.read_text(encoding='utf-8') != 'preserve player state' or settings.read_text(encoding='utf-8') != '{"retain":true}':
            raise ship.ShipError('installer upgrade modified player saves/settings')
        run([str(install / 'unins000.exe'), '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART'])
        if any((install / name).exists() for name in prepared['files']) or shortcut.exists():
            raise ship.ShipError('uninstaller retained declared package files or its Start Menu shortcut')
        if note.read_text(encoding='utf-8') != 'preserve player state' or settings.read_text(encoding='utf-8') != '{"retain":true}':
            raise ship.ShipError('uninstaller removed/modified player saves/settings')
        result = {'ok': True, 'command': 'installer test', 'installer_sha256': built['installer_sha256'],
                  'stage': str(stage), 'installation': str(install), 'commands': commands,
                  'checks': ['exact installed bytes', 'shortcut target/working directory', 'upgrade retains saves/settings', 'uninstall removes package/shortcut and retains saves/settings'],
                  'unverified': ['Native installed-game launch/smoke and physical devices require separate Windows shipping/device checks.']}
        ship.write_json_atomic(stage / 'tested.json', result)
        return result
    except (OSError, subprocess.SubprocessError, ship.ShipError) as error:
        raise ship.ShipError(f'installer lifecycle test failed; installation/saves/logs preserved at {install}: {error}') from error
