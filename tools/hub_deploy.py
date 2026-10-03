#!/usr/bin/env python3
"""Per-game build, validation, promotion and activation for the shared hub (ADR 0037, deploy/hub/update.sh).

update.sh keeps the one-run-at-a-time lock, the hub/helper installs and the command line; everything that has to
be right per game lives here, because it is a state machine with many failure edges and Python can be tested
with temporary repositories and fake executables (tools/test_hub_deploy.py). Standard library only, Python 3.10+.

For each game in sources.conf:

  1. IDENTITY   what the executable would be built from (see `compute_inputs`): the selected package's source and
                every local path dependency's source (the engine when a game depends on it by path), manifests,
                Cargo.lock, cargo config, the toolchain (`rustc -vV`), build arguments and the result-affecting
                CARGO_*/RUSTFLAGS environment. Not git state, not other packages, not docs, not build output.
  2. DECIDE     compare with the receipt in <state>/deployed/<game>.json: up to date (nothing runs, Cargo is not
                even started), resume an incomplete activation (no rebuild), or build.
  3. BUILD      cargo build --locked --release ... --bin BIN; the inputs are fingerprinted again afterwards and a
                build whose inputs changed while it ran is refused (its output cannot be named by any identity).
  4. VALIDATE   the executable is copied beside its destination as a hidden candidate file and checked by
                `be2-hub verify` (the registry's own rules on --info, plus an isolated start that must print a
                STATUS line). A bad candidate is deleted; the installed executable and the receipt are untouched.
  5. PROMOTE    known-good installed executable kept as BIN.previous, receipt written with phase "installing",
                the candidate renamed over the installed file (atomic: same directory), phase "installed".
  6. ACTIVATE   if the hub is running: `be2-hub reload GAME` (phase "activated" when the hub accepted it); if not,
                the game is "installed, activation pending" and the hub loads it when it starts.
  7. READY      `be2-hub status GAME --expect-build B --wait N`: phase "ready" only when the hub's own Public room
                process prints STATUS with the candidate's build. A reload acknowledgement is never reported as ready.

The receipt's phase says exactly how far a game got; every phase before "ready" is retried (never skipped) by the
next run, without rebuilding when the inputs are unchanged. `--rollback GAME` reinstalls BIN.previous.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import time

IDENTITY_VERSION = 1
LIB_KINDS = {'lib', 'rlib', 'dylib', 'cdylib', 'staticlib', 'proc-macro'}
SKIP_DIRS = {'.git', 'target'}
# Environment that changes what Cargo/rustc produce and that this workflow passes through to Cargo. Locations and
# scheduling (CARGO_HOME, CARGO_TARGET_DIR, CARGO_BUILD_JOBS, CARGO_INCREMENTAL, CARGO_TERM_*, CARGO_NET_*) are not.
RESULT_ENV = re.compile(
    r'^(RUSTFLAGS|RUSTC|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|RUSTUP_TOOLCHAIN|CARGO_ENCODED_RUSTFLAGS|'
    r'CARGO_BUILD_(RUSTFLAGS|TARGET|RUSTC|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER)|CARGO_PROFILE_.+|'
    r'CARGO_TARGET_(?!DIR$).+)$')
READY_WAIT_SECONDS = 30
VERIFY_TIMEOUT_SECONDS = 60
PHASES = ('installing', 'installed', 'activated', 'ready')


class DeployError(Exception):
    """A step failed in a way that is reported for this game and leaves the previous state in place."""


class InputError(DeployError):
    """The build inputs cannot be identified safely: nothing is built or promoted."""


# ---- small helpers ----------------------------------------------------------------------------------------------

def sha256_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for block in iter(lambda: f.read(1 << 20), b''):
            h.update(block)
    return h.hexdigest()


def sha256_text(text):
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


def now_iso():
    return time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())


def say(game, text):
    print(f'{game}: {text}', flush=True)


def write_json_atomic(path, document):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + '.tmp')
    with open(tmp, 'w', encoding='utf-8') as f:
        json.dump(document, f, indent=2, sort_keys=True)
        f.write('\n')
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)


def read_json(path):
    try:
        with open(path, encoding='utf-8') as f:
            value = json.load(f)
        return value if isinstance(value, dict) else None
    except (OSError, ValueError):
        return None


def expand_home(text, home):
    if text == '~' or text.startswith('~/'):
        return str(Path(home) / text[2:]) if text != '~' else str(home)
    return text


# ---- environment / configuration --------------------------------------------------------------------------------

class Settings:
    """Directories and executables, all derived from the BLUEENGINE_* environment (never from the real home when the
    variables are set, so a test cannot reach the live installation)."""

    def __init__(self, env=None):
        env = os.environ if env is None else env
        self.env = env
        home = env.get('HOME', str(Path.home()))
        self.user_home = home
        self.home = Path(env.get('BLUEENGINE_HOME') or Path(home) / 'blueengine')
        self.config = Path(env.get('BLUEENGINE_CONFIG') or Path(home) / '.config' / 'blueengine')
        self.state = Path(env.get('BLUEENGINE_STATE') or Path(home) / '.local' / 'share' / 'blueengine')
        self.systemctl = env.get('BLUEENGINE_SYSTEMCTL') or 'systemctl'
        self.unit = env.get('BLUEENGINE_UNIT') or 'blueengine-hub.service'
        self.cargo = env.get('CARGO') or 'cargo'
        self.rustc = env.get('RUSTC') or 'rustc'
        self.ready_wait = int(env.get('BLUEENGINE_READY_WAIT') or READY_WAIT_SECONDS)

    @property
    def hub_bin(self):
        return self.home / 'be2-hub'

    @property
    def hub_conf(self):
        return self.config / 'hub.conf'

    def receipt_path(self, game):
        return self.state / 'deployed' / f'{game}.json'


def parse_sources(path, user_home):
    """`GAME ROOT BIN [extra cargo build arguments...]` per line; `#` starts a comment line."""
    games = []
    for number, line in enumerate(Path(path).read_text(encoding='utf-8').splitlines(), 1):
        stripped = line.strip()
        if not stripped or stripped.startswith('#'):
            continue
        words = shlex.split(stripped)
        if len(words) < 3:
            raise DeployError(f'sources.conf line {number}: {words[0]} needs a source root and a cargo bin name')
        games.append({'game': words[0], 'root': Path(expand_home(words[1], user_home)), 'bin': words[2],
                      'extra': words[3:]})
    return games


# ---- identity ---------------------------------------------------------------------------------------------------

class Inputs:
    """The fingerprint of everything one build depends on."""

    def __init__(self):
        self.identity = ''
        self.components = {}      # name -> sha256 hex
        self.files = {}           # absolute path -> sha256 hex, or 'missing'
        self.mtimes = {}          # absolute path -> mtime_ns
        self.package_roots = {}   # package name -> directory (local packages only)
        self.target_directory = None
        self.selected = ''
        self.extras = {'files': [], 'env': []}   # read outside the source trees by the last build


def run_tool(command, cwd, env, what, timeout=120):
    try:
        done = subprocess.run(command, cwd=cwd, env=env, capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired) as e:
        raise InputError(f'cannot run {what}: {e}') from e
    if done.returncode != 0:
        tail = (done.stderr or done.stdout).strip().splitlines()[-3:]
        raise InputError(f'{what} failed: {" | ".join(tail)}')
    return done.stdout


def forwarded_metadata_args(extra):
    """The cargo build arguments that change which packages are in the dependency graph."""
    out, package = [], None
    it = iter(range(len(extra)))
    for i in it:
        a = extra[i]
        nxt = extra[i + 1] if i + 1 < len(extra) else None
        if a in ('--no-default-features', '--all-features'):
            out.append(a)
        elif a in ('--features', '-F', '--manifest-path') and nxt is not None:
            out += [a, nxt]
            next(it)
        elif a.startswith(('--features=', '--manifest-path=')):
            out.append(a)
        elif a == '--target' and nxt is not None:
            out += ['--filter-platform', nxt]
            next(it)
        elif a.startswith('--target='):
            out += ['--filter-platform', a.split('=', 1)[1]]
        elif a in ('-p', '--package') and nxt is not None:
            package = nxt
            next(it)
        elif a.startswith('--package='):
            package = a.split('=', 1)[1]
    return out, package


def cargo_metadata(settings, root, extra):
    args, package = forwarded_metadata_args(extra)
    out = run_tool([settings.cargo, 'metadata', '--locked', '--offline', '--format-version', '1', *args],
                   root, settings.env, 'cargo metadata --locked --offline')
    try:
        return json.loads(out), package
    except ValueError as e:
        raise InputError(f'cargo metadata printed something that is not JSON: {e}') from e


def select_package(meta, bin_name, wanted):
    members = set(meta.get('workspace_members') or [p['id'] for p in meta['packages']])
    candidates = []
    for p in meta['packages']:
        if p['id'] not in members:
            continue
        if wanted and p['name'] != wanted:
            continue
        if any(t['name'] == bin_name and 'bin' in t['kind'] for t in p['targets']):
            candidates.append(p)
    if len(candidates) != 1:
        what = 'no package' if not candidates else 'several packages'
        raise InputError(f'{what} in the workspace has a bin called {bin_name} '
                         f'(name the package with -p in sources.conf if there are several)')
    return candidates[0]


def local_closure(meta, start):
    """Local (path) packages reachable from `start` through normal and build dependencies."""
    by_id = {p['id']: p for p in meta['packages']}
    nodes = {n['id']: n for n in (meta.get('resolve') or {}).get('nodes', [])}
    seen, stack = {start['id']}, [start['id']]
    while stack:
        node = nodes.get(stack.pop())
        if not node:
            continue
        for dep in node.get('deps', []):
            kinds = [k.get('kind') for k in dep.get('dep_kinds', [])] or [None]
            if all(k == 'dev' for k in kinds):
                continue
            if dep['pkg'] not in seen:
                seen.add(dep['pkg'])
                stack.append(dep['pkg'])
    return [by_id[i] for i in sorted(seen) if i in by_id and by_id[i].get('source') is None]


def walk_files(directory, skip_names=()):
    for base, dirs, names in os.walk(directory):
        dirs[:] = sorted(d for d in dirs if d not in SKIP_DIRS and not (Path(base) == Path(directory) and d in skip_names))
        for name in sorted(names):
            if Path(base) == Path(directory) and name in skip_names:
                continue
            yield Path(base) / name


def package_source_files(pkg, selected, bin_name):
    """Files Cargo may compile for this package: its manifest, build script, and the source trees of the targets
    that go into the executable. The selected package contributes all of src/ (its bin may `mod` anything there);
    a dependency contributes its library tree (not src/bin or src/main.rs, which are other executables)."""
    root = Path(pkg['manifest_path']).parent
    files = {Path(pkg['manifest_path'])}
    for t in pkg['targets']:
        kinds, src = set(t['kind']), Path(t['src_path'])
        if 'custom-build' in kinds:
            files.add(src)
            continue
        wanted = bool(kinds & LIB_KINDS) or (selected and 'bin' in kinds and t['name'] == bin_name)
        if not wanted:
            continue
        tree = src.parent
        if tree == root / 'src':
            skip = () if selected else ('bin', 'main.rs')
            files.update(walk_files(tree, skip))
        elif src.parent.is_dir() and tree != root:
            files.update(walk_files(tree))
        else:
            files.add(src)
    return files


def split_make_words(text):
    """Split on unescaped spaces; `\\ ` is a space inside a name."""
    words, cur, i = [], '', 0
    while i < len(text):
        if text[i] == '\\' and i + 1 < len(text) and text[i + 1] == ' ':
            cur += ' '
            i += 2
            continue
        if text[i] == ' ':
            if cur:
                words.append(cur)
            cur = ''
        else:
            cur += text[i]
        i += 1
    if cur:
        words.append(cur)
    return words


def parse_dep_info(text):
    """The files a `.d` dependency-info file lists, and the environment variables it recorded. Handles rustc's
    form (one rule plus an empty `file:` rule per dependency) and Cargo's top-level `BIN.d` (one rule, absolute paths)."""
    files, envs = [], []
    for line in text.splitlines():
        if line.startswith('# env-dep:'):
            envs.append(line[len('# env-dep:'):].split('=', 1)[0])
        elif not line or line.startswith(('#', ' ', '\t')):
            continue
        elif line.endswith(':'):
            files.append(line[:-1].replace('\\ ', ' '))
        elif ': ' in line:
            files += split_make_words(line.split(': ', 1)[1])
    return list(dict.fromkeys(files)), envs


def dep_info_paths(messages):
    """Where Cargo wrote the dependency-info (`.d`) file of each local unit a build reported: next to a library's
    reported file (`deps/libfoo-HASH.rlib` -> `deps/foo-HASH.d`), `BIN.d` beside an executable. Taken from the build's
    own JSON, never by scanning a target directory, which other checkouts may share."""
    found = []
    for m in messages:
        if m.get('reason') != 'compiler-artifact' or not str(m.get('package_id', '')).startswith('path+file://'):
            continue
        if m.get('executable'):
            found.append(Path(m['executable'] + '.d'))
        elif m.get('filenames'):
            first = Path(m['filenames'][0])
            stem = first.name.rsplit('.', 1)[0]
            found.append(first.with_name((stem[3:] if stem.startswith('lib') else stem) + '.d'))
    return found


def cargo_config_files(root, env, user_home):
    files = []
    for directory in [Path(root).resolve(), *Path(root).resolve().parents]:
        for name in ('config.toml', 'config'):
            f = directory / '.cargo' / name
            if f.is_file():
                files.append(f)
    cargo_home = Path(env.get('CARGO_HOME') or Path(user_home) / '.cargo')
    for name in ('config.toml', 'config'):
        f = cargo_home / name
        if f.is_file():
            files.append(f)
    for directory in [Path(root).resolve(), *Path(root).resolve().parents]:
        pinned = [directory / n for n in ('rust-toolchain.toml', 'rust-toolchain') if (directory / n).is_file()]
        if pinned:
            files.append(pinned[0])
            break
    return files


def toolchain_text(settings, root):
    rustc = run_tool([settings.rustc, '-vV'], root, settings.env, 'rustc -vV')
    cargo = run_tool([settings.cargo, '-V'], root, settings.env, 'cargo -V')
    return rustc.strip() + '\n' + cargo.strip()


def compute_inputs(settings, root, bin_name, extra, known=None, dep_infos=None):
    """Fingerprint a build. Raises InputError when the inputs cannot be identified safely (cargo metadata fails, a
    manifest is missing, a source file cannot be read): the caller then builds and promotes nothing.

    `known` is what the last completed build recorded as read outside the source trees (`include_str!` and
    `include_bytes!` targets and `env!` variables); `dep_infos` are the `.d` files of a build that just ran, which
    replace it. Neither: only the source trees count (a first build)."""
    root = Path(root)
    if not (root / 'Cargo.toml').is_file() and '--manifest-path' not in ' '.join(extra):
        raise InputError(f'{root} has no Cargo.toml')
    meta, wanted = cargo_metadata(settings, root, extra)
    selected = select_package(meta, bin_name, wanted)
    local = local_closure(meta, selected)
    result = Inputs()
    result.selected = selected['name']
    result.target_directory = meta['target_directory']
    workspace = Path(meta['workspace_root'])
    result.package_roots = {p['name']: Path(p['manifest_path']).parent for p in local}

    def hash_file(path):
        path = Path(path)
        key = str(path)
        if key not in result.files:
            try:
                result.mtimes[key] = path.stat().st_mtime_ns
                result.files[key] = sha256_file(path)
            except FileNotFoundError:
                result.files[key] = 'missing'
            except OSError as e:
                raise InputError(f'cannot read {path}: {e}') from e
        return result.files[key]

    def digest(paths, base):
        lines = []
        for p in sorted({str(Path(q)) for q in paths}):
            try:
                shown = str(Path(p).relative_to(base))
            except ValueError:
                shown = p
            lines.append(f'{shown} {hash_file(p)}')
        return sha256_text('\n'.join(lines))

    # Per local package: manifest, build script, compiled source trees, and what the last build read besides.
    extras, env_names = {}, set()
    if dep_infos is not None:
        listed_files, listed_env = [], []
        for d in dep_infos:
            try:
                files_here, envs_here = parse_dep_info(Path(d).read_text(encoding='utf-8', errors='replace'))
            except OSError:
                continue
            listed_files += files_here
            listed_env += envs_here
    else:
        listed_files, listed_env = list((known or {}).get('files', [])), list((known or {}).get('env', []))
    env_names.update(e for e in listed_env if not e.startswith('CARGO_'))
    cargo_home = Path(settings.env.get('CARGO_HOME') or Path(settings.user_home) / '.cargo')
    third_party = (str(cargo_home / 'registry'), str(cargo_home / 'git'))
    for item in listed_files:
        candidate = Path(item)
        if not candidate.is_absolute():
            options = [workspace / item, *(r / item for r in result.package_roots.values())]
            candidate = next((o for o in options if o.exists()), workspace / item)
        candidate = Path(os.path.normpath(candidate))
        shown = str(candidate)
        # Build output, downloaded crates (the lockfile pins those) and the Rust toolchain's own sources are not inputs.
        if shown.startswith((str(Path(result.target_directory)), *third_party)) or '/lib/rustlib/' in shown:
            continue
        extras[shown] = candidate
    result.extras = {'files': sorted(extras), 'env': sorted(env_names)}
    owner_of = {}
    for pkg in local:
        root_dir = Path(pkg['manifest_path']).parent
        files = package_source_files(pkg, pkg is selected, bin_name)
        owner_of[pkg['name']] = (root_dir, files)
    external = []
    for path in extras.values():
        owners = [n for n, (r, _) in owner_of.items() if r in path.parents]
        if owners:
            best = max(owners, key=lambda n: len(str(owner_of[n][0])))
            owner_of[best][1].add(path)
        else:
            external.append(path)
    for name, (root_dir, files) in sorted(owner_of.items()):
        result.components[f'source:{name}'] = digest(files, root_dir)
    if external:
        result.components['source:external'] = digest(external, workspace)
    lock = workspace / 'Cargo.lock'
    if not lock.is_file():
        raise InputError(f'{lock} is missing (the build uses --locked)')
    result.components['lockfile'] = digest([lock], workspace)
    workspace_manifest = workspace / 'Cargo.toml'
    if workspace_manifest.is_file():
        result.components['workspace_manifest'] = digest([workspace_manifest], workspace)
    config = cargo_config_files(root, settings.env, settings.user_home)
    result.components['cargo_config'] = digest(config, root) if config else sha256_text('')
    result.components['toolchain'] = sha256_text(toolchain_text(settings, root))
    env = {k: v for k, v in sorted(settings.env.items()) if RESULT_ENV.match(k)}
    env.update({f'dep-info:{k}': settings.env.get(k) for k in sorted(env_names)})
    result.components['environment'] = sha256_text(json.dumps(env, sort_keys=True))
    result.components['build_args'] = sha256_text(json.dumps(
        {'version': IDENTITY_VERSION, 'bin': bin_name, 'package': selected['name'], 'extra': extra,
         'profile': 'release', 'locked': True}, sort_keys=True))
    result.identity = sha256_text(json.dumps(result.components, sort_keys=True))
    return result


def explain_difference(old_components, new_components):
    """Why the identity changed, in words an operator can act on."""
    if not old_components:
        return 'nothing recorded for this game yet'
    notes = []
    for key in sorted(set(old_components) | set(new_components)):
        if old_components.get(key) == new_components.get(key):
            continue
        if key.startswith('source:'):
            notes.append(f'source of {key[7:]} changed' if key in old_components and key in new_components
                         else f'{key[7:]} source is now part of the build' if key in new_components
                         else f'{key[7:]} is no longer a dependency')
        else:
            notes.append({'lockfile': 'Cargo.lock changed', 'toolchain': 'Rust toolchain changed',
                          'build_args': 'build arguments or features changed',
                          'environment': 'build environment (RUSTFLAGS/CARGO_*) changed',
                          'cargo_config': 'cargo config changed',
                          'workspace_manifest': 'workspace manifest changed'}.get(key, f'{key} changed'))
    return '; '.join(notes) or 'identity changed'


def git_revisions(inputs):
    """Provenance only (never part of the identity): HEAD of each local package's repository."""
    out = {}
    for name, root in sorted(inputs.package_roots.items()):
        try:
            done = subprocess.run(['git', '-C', str(root), 'rev-parse', '--short=12', 'HEAD'],
                                  capture_output=True, text=True, timeout=10)
            out[name] = done.stdout.strip() if done.returncode == 0 else 'not a git checkout'
        except (OSError, subprocess.TimeoutExpired):
            out[name] = 'unknown'
    return out


def changed_during_build(before, after, started_ns):
    """Names the first input that moved while the build ran, or None. A file the build's own dep-info newly lists is
    fine when it is older than the build's start."""
    for path, digest in before.files.items():
        if after.files.get(path) != digest:
            return path
    for path in after.files:
        if path not in before.files and after.mtimes.get(path, 0) >= started_ns:
            return path
    for key in before.components:
        if not key.startswith('source:') and before.components[key] != after.components.get(key):
            return key
    return None


# ---- the hub ----------------------------------------------------------------------------------------------------

def key_values(text):
    out = {}
    for line in text.splitlines():
        if '=' in line and not line.startswith(' '):
            k, v = line.split('=', 1)
            out[k.strip()] = v.strip()
    return out


def hub_has(settings, subcommand):
    try:
        done = subprocess.run([str(settings.hub_bin), '--help'], capture_output=True, text=True, timeout=20)
    except (OSError, subprocess.TimeoutExpired):
        return False
    return f'be2-hub {subcommand} ' in done.stdout


def hub_running(settings):
    try:
        done = subprocess.run([settings.systemctl, '--user', 'is-active', '--quiet', settings.unit],
                              capture_output=True, timeout=20)
    except (OSError, subprocess.TimeoutExpired):
        return False
    return done.returncode == 0


def hub_run(settings, args, timeout, what):
    try:
        done = subprocess.run([str(settings.hub_bin), *args], capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired as e:
        raise DeployError(f'{what} did not finish within {timeout} s') from e
    except OSError as e:
        raise DeployError(f'cannot run {settings.hub_bin}: {e}') from e
    return done


def hub_verify(settings, game, candidate, destination):
    """`be2-hub verify`: the registry's rules on --info plus an isolated start. Returns its key=value report."""
    if not settings.hub_conf.is_file():
        raise DeployError(f'no {settings.hub_conf} (copy deploy/hub/hub.conf.example there)')
    if not hub_has(settings, 'verify'):
        raise DeployError(f'{settings.hub_bin} is too old to verify candidates: run update.sh --hub first '
                          f'(restarting the hub afterwards is optional for verify, needed for status)')
    done = hub_run(settings, ['verify', game, '--server', str(candidate), '--config', str(settings.hub_conf),
                              '--installs-to', str(destination), '--start'], VERIFY_TIMEOUT_SECONDS, 'be2-hub verify')
    if done.returncode != 0:
        raise DeployError('candidate rejected: ' + (done.stderr.strip().replace('be2-hub: ', '') or 'be2-hub verify failed'))
    report = key_values(done.stdout)
    if not report.get('build') or report.get('startup') in (None, 'not-checked'):
        raise DeployError('be2-hub verify did not report a build and a successful start')
    return report


def hub_reload(settings, game):
    done = hub_run(settings, ['reload', game, '--config', str(settings.hub_conf)], 40, 'be2-hub reload')
    if done.returncode != 0:
        raise DeployError('the hub refused or did not answer the reload: ' + (done.stderr.strip() or 'no output'))
    return done.stdout.strip()


def hub_status(settings, game, build, wait):
    """Returns (state, detail). state is one of ready, registry-only, starting, missing, wrong-build, no-answer,
    unknown-game, error."""
    args = ['status', game, '--config', str(settings.hub_conf), '--expect-build', build]
    if wait:
        args += ['--wait', str(wait)]
    done = hub_run(settings, args, wait + 30, 'be2-hub status')
    values = key_values(done.stdout)
    state = values.get('state')
    if state:
        return state, values.get('detail') or done.stderr.strip()
    return 'error', done.stderr.strip() or 'be2-hub status printed nothing'


# ---- receipts ---------------------------------------------------------------------------------------------------

def binary_record(path):
    st = Path(path).stat()
    return {'path': str(path), 'sha256': sha256_file(path), 'size': st.st_size, 'mtime_ns': st.st_mtime_ns}


def installed_matches(receipt, path):
    """Is the file at `path` the one the receipt recorded? Compares size and mtime first; hashes only if they moved."""
    rec = receipt.get('binary') or {}
    try:
        st = Path(path).stat()
    except OSError:
        return False
    if st.st_size != rec.get('size'):
        return False
    if st.st_mtime_ns == rec.get('mtime_ns'):
        return True
    return sha256_file(path) == rec.get('sha256')


def is_complete(receipt):
    return bool(receipt) and receipt.get('complete') is True


# ---- one game ---------------------------------------------------------------------------------------------------

class GameResult:
    def __init__(self, game):
        self.game = game
        self.ok = True
        self.updated = 0
        self.summary = ''

    def fail(self, text):
        self.ok = False
        self.summary = text
        say(self.game, f'FAILED: {text}')
        return self


def run_cargo_build(settings, root, bin_name, extra):
    """Build and return the executable path Cargo reports (never guessed from the target directory layout)."""
    command = [settings.cargo, 'build', '--locked', '--release', *extra, '--bin', bin_name,
               '--message-format=json-render-diagnostics']
    try:
        process = subprocess.Popen(command, cwd=root, env=settings.env, stdout=subprocess.PIPE, text=True)
    except OSError as e:
        raise DeployError(f'cannot run cargo: {e}') from e
    executable = None
    messages = []
    for line in process.stdout:
        try:
            message = json.loads(line)
        except ValueError:
            continue
        messages.append(message)
        if message.get('reason') == 'compiler-artifact' and message.get('executable'):
            target = message.get('target') or {}
            if target.get('name') == bin_name and 'bin' in (target.get('kind') or []):
                executable = message['executable']
    if process.wait() != 0:
        raise DeployError('cargo build failed (the installed server and the receipt are unchanged)')
    if not executable or not Path(executable).is_file():
        raise DeployError(f'cargo did not report an executable for {bin_name}')
    return Path(executable), messages


def update_game(settings, entry, force=False, pull=False):
    game, root, bin_name, extra = entry['game'], entry['root'], entry['bin'], entry['extra']
    result = GameResult(game)
    destination = settings.home / bin_name
    candidate = settings.home / f'.{bin_name}.candidate'
    receipt_path = settings.receipt_path(game)
    try:
        if not root.is_dir():
            raise DeployError(f'source root {root} does not exist')
        if pull:
            done = subprocess.run(['git', '-C', str(root), 'pull', '--ff-only', '--quiet'], capture_output=True, text=True)
            if done.returncode != 0:
                raise DeployError('git pull --ff-only failed: ' + done.stderr.strip())
        receipt = read_json(receipt_path)
        before = compute_inputs(settings, root, bin_name, extra, known=(receipt or {}).get('extras'))
        decision = decide(before, receipt, destination, force)
        say(game, decision['why'])
        if decision['action'] == 'skip':
            return result
        if decision['action'] == 'resume':
            return resume(settings, result, receipt, destination)
        return build_and_promote(settings, result, entry, before, receipt, destination, candidate)
    except InputError as e:
        return result.fail(f'cannot identify the build inputs, so nothing was built or changed: {e}')
    except DeployError as e:
        return result.fail(str(e))
    finally:
        # A candidate left by an interrupted or failed run is never an installed file; remove it.
        if candidate.exists() and not candidate.is_dir():
            try:
                candidate.unlink()
            except OSError:
                pass


def decide(inputs, receipt, destination, force):
    """What this run does for one game, and why (one line for the operator)."""
    short = inputs.identity[:12]
    if force:
        return {'action': 'build', 'why': f'--force: rebuilding ({short})'}
    if not receipt:
        return {'action': 'build', 'why': f'building: no deployment receipt yet ({short})'}
    if receipt.get('rolled_back_from') == inputs.identity:
        return {'action': 'skip', 'why': f'skipped: this source ({short}) was rolled back; '
                                          f'change it or use --force to deploy it again'}
    if receipt.get('identity') != inputs.identity:
        return {'action': 'build',
                'why': 'rebuilding: ' + explain_difference(receipt.get('components'), inputs.components)}
    if not destination.is_file() or not installed_matches(receipt, destination):
        return {'action': 'build',
                'why': f'rebuilding: the installed file {destination} is not the one recorded in the receipt'}
    if is_complete(receipt):
        return {'action': 'skip', 'why': f'up to date ({short}), {describe_phase(receipt)}'}
    return {'action': 'resume',
            'why': f'resuming without a rebuild ({short}): earlier run stopped at phase "{receipt.get("phase")}"'}


def describe_phase(receipt):
    if receipt.get('readiness') == 'registry-only':
        return 'activated (no Public room, readiness not observable)'
    return f'phase {receipt.get("phase")}'


def build_and_promote(settings, result, entry, before, old_receipt, destination, candidate):
    game, root, bin_name, extra = entry['game'], entry['root'], entry['bin'], entry['extra']
    say(game, f'building {bin_name} from {root}')
    started_ns = time.time_ns()
    artifact, messages = run_cargo_build(settings, root, bin_name, extra)
    after = compute_inputs(settings, root, bin_name, extra, dep_infos=dep_info_paths(messages))
    moved = changed_during_build(before, after, started_ns)
    if moved:
        raise DeployError(f'a build input changed while cargo was running ({moved}); the executable cannot be tied '
                          f'to one identity, so nothing was installed. Run again when the tree is quiet')
    # Stage beside the destination (same filesystem: the final rename is atomic).
    settings.home.mkdir(parents=True, exist_ok=True)
    try:
        shutil.copyfile(artifact, candidate)
        os.chmod(candidate, 0o755)
    except OSError as e:
        raise DeployError(f'cannot stage the candidate in {settings.home}: {e}') from e
    report = hub_verify(settings, game, candidate, destination)
    say(game, f'candidate ok: build {report["build"]}, {report.get("settings", "?")} setting(s), '
              f'started on loopback and reported STATUS ({report.get("startup")})')
    for warning in [v for k, v in report.items() if k == 'warning']:
        say(game, f'warning: {warning}')
    candidate_record = binary_record(candidate)
    new_receipt = {
        'schema': 1, 'game': game, 'bin': bin_name, 'identity': after.identity, 'components': after.components, 'extras': after.extras,
        'revisions': git_revisions(after), 'built_at': now_iso(),
        'info': {'game': report.get('game'), 'build': report['build'], 'max_seats': report.get('max_seats'),
                 'settings': report.get('settings')},
        'binary': candidate_record, 'phase': 'installing', 'complete': False,
        'previous': previous_section(old_receipt, destination),
    }
    keep_previous(destination, settings, bin_name, old_receipt)
    receipt_path = settings.receipt_path(game)
    try:
        write_json_atomic(receipt_path, new_receipt)
        os.replace(candidate, destination)
    except OSError as e:
        # Installation failed: the destination is untouched (the rename is atomic); put the old record back.
        if old_receipt is not None:
            write_json_atomic(receipt_path, old_receipt)
        elif receipt_path.exists():
            receipt_path.unlink()
        raise DeployError(f'installation failed ({e}); the installed server and the receipt are unchanged') from e
    new_receipt['binary'] = binary_record(destination)
    new_receipt.update(phase='installed', installed_at=now_iso())
    write_json_atomic(receipt_path, new_receipt)
    say(game, f'installed {destination} (previous known-good kept as {destination.name}.previous)'
        if (settings.home / f'{bin_name}.previous').exists() else f'installed {destination}')
    result.updated = 1
    return activate(settings, result, new_receipt, destination, fresh=True)


def previous_section(old_receipt, destination):
    if not old_receipt:
        return None
    return {'identity': old_receipt.get('identity'), 'sha256': (old_receipt.get('binary') or {}).get('sha256'),
            'build': (old_receipt.get('info') or {}).get('build'), 'phase': old_receipt.get('phase'),
            'complete': bool(old_receipt.get('complete'))}


def keep_previous(destination, settings, bin_name, old_receipt):
    """Keep the installed executable as BIN.previous when it is known to work (its activation completed), or when
    there is no previous copy yet. A half-activated install never overwrites a known-good previous copy."""
    previous = settings.home / f'{bin_name}.previous'
    if not destination.is_file():
        return
    if is_complete(old_receipt) or not previous.exists():
        tmp = previous.with_name(previous.name + '.tmp')
        shutil.copy2(destination, tmp)
        os.replace(tmp, previous)


def resume(settings, result, receipt, destination):
    """Finish an incomplete activation. The executable is already installed and matches the receipt."""
    receipt = dict(receipt)
    if receipt.get('phase') == 'installing':
        # The rename happened (the installed file matches) but the crash came before "installed" was recorded.
        receipt.update(phase='installed', installed_at=now_iso(), binary=binary_record(destination))
        write_json_atomic(settings.receipt_path(receipt['game']), receipt)
        say(receipt['game'], 'recovered: the new executable was already in place; continuing with activation')
    return activate(settings, result, receipt, destination, fresh=False)


def record(settings, receipt, **fields):
    receipt.update(fields)
    receipt['updated_at'] = now_iso()
    write_json_atomic(settings.receipt_path(receipt['game']), receipt)


def activate(settings, result, receipt, destination, fresh):
    """Phases installed -> activated -> ready. `fresh` is a run that just installed; otherwise a resumed one."""
    game, build = receipt['game'], receipt['info']['build']
    if not hub_running(settings):
        record(settings, receipt, phase='installed', complete=False, note='hub not running')
        result.summary = 'installed, activation pending'
        say(game, f'installed, activation pending: {settings.unit} is not running; the hub loads this server when it '
                  f'starts (the next update.sh run records the result)')
        return result
    if not hub_has(settings, 'status'):
        raise DeployError(f'{settings.hub_bin} predates `status`; run update.sh --hub first')
    if not fresh:
        # An earlier run got further than it could record, or the hub noticed the replaced file by itself: look
        # before acting, so a game that is already running the new build is not reloaded (and its rooms retired) again.
        state, detail = hub_status(settings, game, build, 0)
        if state == 'starting':
            state, detail = hub_status(settings, game, build, settings.ready_wait)
        say(game, f'hub reports {state}')
        if state in ('ready', 'registry-only'):
            return finish(settings, result, receipt, state, detail)
        if receipt.get('phase') == 'activated' and state in ('no-answer', 'starting', 'error'):
            # The hub already accepted a reload for this build; asking again would only retire rooms again.
            return not_ready(settings, result, receipt, state, detail)
    try:
        text = hub_reload(settings, game)
    except DeployError as e:
        record(settings, receipt, phase='installed', complete=False, note=f'reload failed: {e}')
        raise DeployError(f'installed but NOT activated: {e}. The hub also notices a replaced server file '
                          f'within about 20 s by itself; run update.sh again to retry the activation '
                          f'(no rebuild)') from e
    record(settings, receipt, phase='activated', complete=False, activated_at=now_iso(), note='reload accepted')
    say(game, f'activated: the hub accepted the reload ("{text}"); this is not yet proof the room works')
    state, detail = hub_status(settings, game, build, settings.ready_wait)
    if state in ('ready', 'registry-only'):
        return finish(settings, result, receipt, state, detail)
    return not_ready(settings, result, receipt, state, detail)


def finish(settings, result, receipt, state, detail):
    game = receipt['game']
    if state == 'ready':
        record(settings, receipt, phase='ready', complete=True, readiness='ready', ready_at=now_iso(),
               note=detail)
        result.summary = 'ready'
        say(game, f'ready: the hub\'s Public room runs build {receipt["info"]["build"]} and reports STATUS')
    else:
        record(settings, receipt, phase='activated', complete=True, readiness='registry-only', note=detail)
        result.summary = 'activated, readiness not observable'
        say(game, f'activated: the hub holds build {receipt["info"]["build"]}; {game} has no Public room, so no '
                  f'running process can confirm it (new rooms will use it)')
    return result


def not_ready(settings, result, receipt, state, detail):
    game = receipt['game']
    record(settings, receipt, complete=False, readiness=state, note=detail)
    hints = {
        'no-answer': 'the hub did not answer the status query (a hub started before `status` existed never does: '
                     'restart it when nobody is playing)',
        'starting': f'the Public room started but printed no STATUS line within {settings.ready_wait} s',
        'missing': 'the hub has no running Public room for it (pool or process limit? see its journal)',
        'wrong-build': 'the hub or its Public room still runs a different build',
    }
    raise DeployError(f'activated but NOT ready ({state}): {hints.get(state, detail)}. '
                      f'Run update.sh again to re-check; `update.sh --rollback {game}` restores the previous server')


def rollback_game(settings, entry):
    game, bin_name = entry['game'], entry['bin']
    result = GameResult(game)
    destination = settings.home / bin_name
    previous = settings.home / f'{bin_name}.previous'
    candidate = settings.home / f'.{bin_name}.candidate'
    receipt_path = settings.receipt_path(game)
    try:
        if not previous.is_file():
            raise DeployError(f'there is no {previous} to roll back to')
        receipt = read_json(receipt_path) or {}
        shutil.copyfile(previous, candidate)
        os.chmod(candidate, 0o755)
        report = hub_verify(settings, game, candidate, destination)
        prior = receipt.get('previous') or {}
        new_receipt = {
            'schema': 1, 'game': game, 'bin': bin_name, 'identity': prior.get('identity') or 'rolled-back',
            'components': {}, 'extras': receipt.get('extras') or {}, 'rolled_back_from': receipt.get('identity'),
            'info': {'game': report.get('game'), 'build': report['build'], 'max_seats': report.get('max_seats'),
                     'settings': report.get('settings')},
            'binary': binary_record(candidate), 'phase': 'installing', 'complete': False, 'built_at': now_iso(),
            'note': 'rollback to the previous executable',
        }
        write_json_atomic(receipt_path, new_receipt)
        os.replace(candidate, destination)
        new_receipt['binary'] = binary_record(destination)
        record(settings, new_receipt, phase='installed', installed_at=now_iso())
        say(game, f'rolled back: {destination} is the previous executable again (build {report["build"]})')
        return activate(settings, result, new_receipt, destination, fresh=True)
    except DeployError as e:
        return result.fail(str(e))
    finally:
        if candidate.exists() and not candidate.is_dir():
            candidate.unlink()


# ---- the hub program itself -------------------------------------------------------------------------------------

def install_hub(settings, engine):
    """`update.sh --hub`: build be2-hub from the engine checkout, check the candidate, install it atomically (the
    old one kept as be2-hub.previous). The running hub is never restarted from here: it keeps running the old
    program until the operator restarts it."""
    engine = Path(engine)
    name = 'be2-hub'
    destination = settings.hub_bin
    candidate = settings.home / f'.{name}.candidate'
    say('hub', f'building {name} from {engine}')
    try:
        if not engine.is_dir():
            raise DeployError(f'engine checkout {engine} does not exist')
        artifact, _ = run_cargo_build(settings, engine, name, ['--no-default-features'])
        settings.home.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(artifact, candidate)
        os.chmod(candidate, 0o755)
        helped = subprocess.run([str(candidate), '--help'], capture_output=True, text=True, timeout=30)
        if helped.returncode != 0 or 'be2-hub verify ' not in helped.stdout:
            raise DeployError('the built be2-hub does not describe itself (--help); not installed')
        if settings.hub_conf.is_file():
            # The new program must at least accept the registry file the hub runs with (rules may have tightened).
            ports = subprocess.run([str(candidate), 'ports', '--config', str(settings.hub_conf)],
                                   capture_output=True, text=True, timeout=30)
            if ports.returncode != 0:
                raise DeployError('the new be2-hub rejects the current hub.conf, so it was not installed: '
                                  + ports.stderr.strip())
        if destination.is_file():
            tmp = destination.with_name(destination.name + '.previous.tmp')
            shutil.copy2(destination, tmp)
            os.replace(tmp, settings.home / f'{name}.previous')
        os.replace(candidate, destination)
    except (DeployError, OSError, subprocess.TimeoutExpired) as e:
        say('hub', f'FAILED: {e} (the installed be2-hub is unchanged)')
        return False
    finally:
        if candidate.exists() and not candidate.is_dir():
            candidate.unlink()
    say('hub', f'installed {destination}. Restart it when nobody is playing (this ends every room); until then the '
               f'running hub keeps the old program and cannot answer `status`:')
    print(f'  systemctl --user restart {settings.unit}')
    return True


# ---- command line -----------------------------------------------------------------------------------------------

def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    sub = parser.add_subparsers(dest='command', required=True)
    up = sub.add_parser('update', help='update every game in sources.conf (or the named ones)')
    up.add_argument('games', nargs='*')
    up.add_argument('--force', action='store_true')
    up.add_argument('--pull', action='store_true')
    rb = sub.add_parser('rollback', help='reinstall BIN.previous for one game and activate it')
    rb.add_argument('games', nargs=1)
    hub = sub.add_parser('hub', help='build and install be2-hub from the engine checkout')
    hub.add_argument('engine')
    ident = sub.add_parser('identity', help='print the build identity of a source root (no build)')
    ident.add_argument('root')
    ident.add_argument('bin')
    ident.add_argument('extra', nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    settings = Settings()
    if args.command == 'hub':
        return 0 if install_hub(settings, args.engine) else 1
    if args.command == 'identity':
        try:
            started = time.monotonic()
            inputs = compute_inputs(settings, Path(args.root), args.bin, args.extra)
        except DeployError as e:
            print(f'cannot identify: {e}', file=sys.stderr)
            return 1
        print(json.dumps({'identity': inputs.identity, 'components': inputs.components, 'files': len(inputs.files),
                          'seconds': round(time.monotonic() - started, 3)}, indent=2, sort_keys=True))
        return 0
    sources = settings.config / 'sources.conf'
    if not sources.is_file():
        print(f'no {sources} (copy deploy/hub/sources.conf.example there)', file=sys.stderr)
        return 1
    try:
        entries = parse_sources(sources, settings.user_home)
    except DeployError as e:
        print(e, file=sys.stderr)
        return 1
    wanted = args.games
    unknown = [g for g in wanted if g not in {e['game'] for e in entries}]
    if unknown:
        print(f'not in sources.conf: {", ".join(unknown)}', file=sys.stderr)
        return 1
    chosen = [e for e in entries if not wanted or e['game'] in wanted]
    results = []
    for entry in chosen:
        if args.command == 'rollback':
            results.append(rollback_game(settings, entry))
        else:
            results.append(update_game(settings, entry, force=args.force, pull=args.pull))
    updated = sum(r.updated for r in results)
    failed = [r for r in results if not r.ok]
    print(f'done: {updated} game(s) updated' + (f', {len(failed)} FAILED: {", ".join(r.game for r in failed)}'
                                                if failed else ''))
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
