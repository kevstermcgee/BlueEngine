"""Tests for deploy/hub/update.sh and tools/hub_deploy.py: dependency-aware rebuild decisions, staged validation,
atomic promotion, and recovery at every failure boundary.

Everything runs in a temporary directory: BLUEENGINE_HOME/CONFIG/ENGINE/STATE/LOCK, HOME and PATH all point there,
cargo, rustc, systemctl and be2-hub are small fake executables (written below), and nothing binds a port. The fake
be2-hub speaks the same command-line contract as the real one (usage text in src/bin/be2-hub.rs; the real binary is
exercised against update.sh in tests/hub.rs, `update_sh_*`). The live hub, its files and its units are never touched.
"""
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import textwrap
import unittest
from unittest import mock

from tools import hub_deploy

ROOT = Path(__file__).resolve().parents[1]
UPDATE_SH = ROOT / 'deploy/hub/update.sh'
# update.sh needs bash and flock (the hub itself only runs on Linux boxes); the pure functions are tested everywhere.
HAVE_SHELL = os.name == 'posix' and bool(shutil.which('bash')) and bool(shutil.which('flock'))

FAKE_CARGO = '''#!/usr/bin/env python3
import json, os, sys
d = os.environ['FAKE_DIR']
def ctl():
    try:
        return json.load(open(d + '/ctl.json'))
    except OSError:
        return {}
args = sys.argv[1:]
with open(d + '/cargo.log', 'a') as log:
    log.write(' '.join(args) + '\\n')
c = ctl()
if args[:1] == ['-V']:
    print('cargo 9.9.9 (fake)')
elif args[:1] == ['metadata']:
    if c.get('metadata_fails'):
        print('error: failed to load manifest', file=sys.stderr)
        sys.exit(101)
    print(open(d + '/metadata.json').read())
elif args[:1] == ['build']:
    if c.get('build_fails'):
        print('error: could not compile `game` (fake)', file=sys.stderr)
        sys.exit(101)
    for path, text in c.get('write_during', {}).items():
        with open(path, 'w') as f:
            f.write(text)
    binary = args[args.index('--bin') + 1]
    out = d + '/target/release/' + binary
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, 'w') as f:
        f.write(c.get('artifacts', {}).get(binary, '#!/bin/sh\\n# BUILD=00000001 MODE=ok\\n'))
    os.chmod(out, 0o755)
    os.makedirs(d + '/target/release/deps', exist_ok=True)
    for crate, files in c.get('depinfo', {}).items():
        text = crate + ': ' + ' '.join(files) + '\\n' + ''.join(x + ':\\n' for x in files)
        if crate == binary:
            with open(out + '.d', 'w') as f:
                f.write(text)
            continue
        rlib = d + '/target/release/deps/lib' + crate + '-0123abcd.rlib'
        with open(d + '/target/release/deps/' + crate + '-0123abcd.d', 'w') as f:
            f.write(text)
        print(json.dumps({'reason': 'compiler-artifact', 'package_id': 'path+file:///fake/' + crate + '#0.1.0',
                          'target': {'name': crate, 'kind': ['lib']}, 'filenames': [rlib], 'executable': None}))
    print(json.dumps({'reason': 'compiler-artifact', 'package_id': 'path+file:///fake/game#0.1.0',
                      'target': {'name': binary, 'kind': ['bin']}, 'filenames': [out], 'executable': out}))
'''

FAKE_RUSTC = '''#!/usr/bin/env python3
import os, sys
sys.stdout.write(open(os.environ['FAKE_DIR'] + '/rustc.txt').read())
'''

FAKE_SYSTEMCTL = '''#!/usr/bin/env python3
import os, sys
with open(os.environ['FAKE_DIR'] + '/systemctl.log', 'a') as log:
    log.write(' '.join(sys.argv[1:]) + '\\n')
sys.exit(0 if os.path.exists(os.environ['FAKE_DIR'] + '/hub_active') else 3)
'''

# Same command line and output as the real be2-hub for verify/status/reload; behaviour is scripted through ctl.json
# and by `MODE=` / `BUILD=` markers inside the candidate executable's text.
FAKE_HUB = '''#!/usr/bin/env python3
import json, os, re, sys, time
d = os.environ['FAKE_DIR']
def ctl():
    try:
        return json.load(open(d + '/ctl.json'))
    except OSError:
        return {}
def log(line):
    with open(d + '/hub.log', 'a') as f:
        f.write(line + '\\n')
def take(name, default):
    """A scripted value, or the next of a list of them (the last one repeats)."""
    value = ctl().get(name, default)
    if isinstance(value, list):
        rest = value[1:] or value[-1:]
        c = ctl(); c[name] = rest
        json.dump(c, open(d + '/ctl.json', 'w'))
        return value[0]
    return value
def marker(path, key):
    m = re.search(key + r'=([\\w-]+)', open(path).read())
    return m.group(1) if m else ''
args = sys.argv[1:]
if args[:1] == ['--help']:
    print('be2-hub: fake\\n  be2-hub reload GAME [--config PATH]\\n  be2-hub status GAME [--config PATH]\\n'
          '  be2-hub verify GAME --server PATH')
elif args[:1] == ['ports']:
    pass
elif args[:1] == ['verify']:
    game = args[1]
    server = args[args.index('--server') + 1]
    flags = ' '.join(a for a in args if a in ('--start', '--installs-to'))
    log('verify %s %s %s' % (game, os.path.basename(server), flags))
    mode, build = marker(server, 'MODE'), marker(server, 'BUILD')
    if mode == 'bad-info':
        print('be2-hub: %s --info exited with exit status: 1' % server, file=sys.stderr); sys.exit(1)
    if mode == 'slow-info':
        print('be2-hub: %s --info did not finish in 5 s' % server, file=sys.stderr); sys.exit(1)
    if mode == 'bad-settings':
        print('be2-hub: public_set names the setting bots, which %s does not have' % game, file=sys.stderr); sys.exit(1)
    if mode == 'startup-fail':
        print('be2-hub: the candidate exited before reporting its first STATUS line', file=sys.stderr); sys.exit(1)
    print('game=%s\\nbuild=%s\\nfingerprint=%s\\nmax_seats=4\\nsettings=2' % (game, build, build))
    if mode == 'warn':
        print('warning=warning: [game %s] runs a server whose game is called other' % game)
    print('startup=ok players=0 max=4 build=%s' % build)
elif args[:1] == ['reload']:
    game = args[1]
    log('reload ' + game)
    mode = take('reload', 'ok')
    if mode == 'fail':
        print('be2-hub: no answer from a hub on 127.0.0.1:4100', file=sys.stderr); sys.exit(1)
    if mode == 'kill-parent':
        os.kill(os.getppid(), 9); time.sleep(5); sys.exit(1)
    installed = ctl()['paths'][game]
    c = ctl(); c.setdefault('held', {})[game] = marker(installed, 'BUILD')
    json.dump(c, open(d + '/ctl.json', 'w'))
    print('%s reloaded (build %s): 1 room(s) retired; Public room on port 4105, no status yet' % (game, c['held'][game]))
elif args[:1] == ['status']:
    game = args[1]
    expect = args[args.index('--expect-build') + 1] if '--expect-build' in args else ''
    log('status %s expect=%s' % (game, expect))
    state = take('status', 'real')
    if state == 'real':
        held = ctl().get('held', {}).get(game, ctl().get('initial_build', ''))
        state = 'ready' if held == expect else 'wrong-build'
    print('state=' + state)
    print('detail=game=%s registry_build=%s' % (game, expect))
    sys.exit(0 if state in ('ready', 'registry-only') else 1)
else:
    print('be2-hub: unexpected arguments %r' % args, file=sys.stderr); sys.exit(2)
'''


def write_exe(path, text):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(text, encoding='utf-8')
    os.chmod(path, 0o755)


def git(repo, *args):
    subprocess.run(['git', '-c', 'user.email=t@example.invalid', '-c', 'user.name=T', '-C', str(repo), *args],
                   check=True, capture_output=True)


class Fixture:
    """A workspace `ws/` holding two games (`game`, `other`), a path-dependency engine `engine/`, and a deployment
    directory tree, all inside one temporary directory."""

    GAME, BIN = 'game', 'game-server'

    def __init__(self, test):
        self.tmp = Path(tempfile.mkdtemp(prefix='hubdeploy-'))
        test.addCleanup(shutil.rmtree, self.tmp, True)
        t = self.tmp
        self.fake = t / 'fake'
        self.home_dir = t / 'home'            # HOME
        self.install = t / 'install'          # BLUEENGINE_HOME
        self.config = t / 'config'
        self.state = t / 'state'
        self.engine = t / 'engine'
        self.ws = t / 'ws'
        self.game = self.ws / 'games/game'
        self.other = self.ws / 'games/other'
        for d in (self.fake, self.home_dir, self.install, self.config, self.state / 'deployed'):
            d.mkdir(parents=True, exist_ok=True)
        self.write(self.engine / 'Cargo.toml', '[package]\nname = "be2"\n')
        self.write(self.engine / 'src/lib.rs', 'pub fn engine() -> u32 { 1 }\n')
        self.write(self.engine / 'src/netplay/mod.rs', '// netplay\n')
        self.write(self.engine / 'src/bin/be2-hub.rs', 'fn main() {}\n')
        self.write(self.engine / 'docs/GUIDE.md', 'docs\n')
        self.write(self.engine / 'assets/pic.png', 'png-v1')
        self.write(self.ws / 'Cargo.toml', '[workspace]\nmembers = ["games/*"]\n')
        self.write(self.ws / 'Cargo.lock', '# lock v1\n')
        self.write(self.game / 'Cargo.toml', '[package]\nname = "game"\n')
        self.write(self.game / 'src/lib.rs', 'pub fn rules() {}\n')
        self.write(self.game / 'src/bin/server.rs', 'fn main() {}\n')
        self.write(self.game / 'docs/NOTES.md', 'notes\n')
        self.write(self.other / 'Cargo.toml', '[package]\nname = "other"\n')
        self.write(self.other / 'src/lib.rs', 'pub fn other() {}\n')
        for repo in (self.engine, self.ws):
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            git(repo, 'add', '-A')
            git(repo, 'commit', '-q', '-m', 'start')
        write_exe(self.fake / 'cargo', FAKE_CARGO)
        write_exe(self.fake / 'rustc', FAKE_RUSTC)
        write_exe(self.fake / 'systemctl', FAKE_SYSTEMCTL)
        self.write(self.fake / 'rustc.txt', 'rustc 1.99.0 (fake)\nhost: x86_64-unknown-linux-gnu\n')
        write_exe(self.install / 'be2-hub', FAKE_HUB)
        self.write(self.config / 'hub.conf', f'[game {self.GAME}]\nserver = {self.install / self.BIN}\npublic = on\n')
        self.write(self.config / 'sources.conf', f'{self.GAME} {self.game} {self.BIN} --no-default-features\n')
        self.write_metadata()
        self.ctl(paths={self.GAME: str(self.install / self.BIN)}, initial_build='00000000')
        self.set_artifact('aaaa0001')
        self.hub_active(True)

    # -- files ---------------------------------------------------------------------------------------------------
    def write(self, path, text):
        Path(path).parent.mkdir(parents=True, exist_ok=True)
        Path(path).write_text(text, encoding='utf-8')

    def write_metadata(self, with_other_dep=False):
        def pkg(name, root, targets, source=None):
            return {'id': name, 'name': name, 'source': source, 'manifest_path': str(root / 'Cargo.toml'),
                    'targets': targets}
        game = pkg('game', self.game, [
            {'name': 'game', 'kind': ['lib'], 'src_path': str(self.game / 'src/lib.rs')},
            {'name': self.BIN, 'kind': ['bin'], 'src_path': str(self.game / 'src/bin/server.rs')}])
        engine = pkg('be2', self.engine, [
            {'name': 'vesper3d', 'kind': ['lib'], 'src_path': str(self.engine / 'src/lib.rs')},
            {'name': 'be2-hub', 'kind': ['bin'], 'src_path': str(self.engine / 'src/bin/be2-hub.rs')}])
        other = pkg('other', self.other, [{'name': 'other', 'kind': ['lib'], 'src_path': str(self.other / 'src/lib.rs')}])
        serde = pkg('serde', Path('/nonexistent/serde'), [], source='registry+https://example.invalid/index')
        deps = [{'pkg': 'be2', 'dep_kinds': [{'kind': None}]}, {'pkg': 'serde', 'dep_kinds': [{'kind': None}]},
                {'pkg': 'other', 'dep_kinds': [{'kind': 'dev'}]}]
        if with_other_dep:
            deps[2] = {'pkg': 'other', 'dep_kinds': [{'kind': None}]}
        meta = {'packages': [game, engine, other, serde], 'workspace_members': ['game', 'other'],
                'workspace_root': str(self.ws), 'target_directory': str(self.fake / 'target'),
                'resolve': {'nodes': [{'id': 'game', 'deps': deps},
                                      {'id': 'be2', 'deps': [{'pkg': 'serde', 'dep_kinds': [{'kind': None}]}]},
                                      {'id': 'other', 'deps': []}, {'id': 'serde', 'deps': []}]}}
        self.write(self.fake / 'metadata.json', json.dumps(meta))

    def ctl(self, **updates):
        path = self.fake / 'ctl.json'
        data = json.loads(path.read_text()) if path.exists() else {}
        data.update(updates)
        path.write_text(json.dumps(data))

    def set_artifact(self, build, mode='ok', binary=None):
        text = f'#!/bin/sh\n# BUILD={build} MODE={mode}\n'
        artifacts = json.loads((self.fake / 'ctl.json').read_text()).get('artifacts', {}) if (self.fake / 'ctl.json').exists() else {}
        artifacts[binary or self.BIN] = text
        self.ctl(artifacts=artifacts)

    def hub_active(self, active):
        flag = self.fake / 'hub_active'
        if active:
            flag.write_text('1')
        elif flag.exists():
            flag.unlink()

    # -- running -------------------------------------------------------------------------------------------------
    def env(self, **extra):
        env = {'PATH': f'{self.fake}:{Path(sys.executable).parent}:/usr/bin:/bin', 'HOME': str(self.home_dir),
               'LANG': 'C.UTF-8', 'FAKE_DIR': str(self.fake), 'BLUEENGINE_HOME': str(self.install),
               'BLUEENGINE_CONFIG': str(self.config), 'BLUEENGINE_ENGINE': str(self.engine),
               'BLUEENGINE_STATE': str(self.state), 'BLUEENGINE_LOCK': str(self.state / 'update.lock'),
               'BLUEENGINE_SYSTEMCTL': str(self.fake / 'systemctl'), 'BLUEENGINE_READY_WAIT': '1',
               'CARGO': str(self.fake / 'cargo'), 'RUSTC': str(self.fake / 'rustc')}
        env.update(extra)
        return env

    def run(self, *args, env=None):
        done = subprocess.run(['bash', str(UPDATE_SH), *args], env=env or self.env(), capture_output=True, text=True,
                              timeout=120)
        self.last = done
        return done

    # -- observations --------------------------------------------------------------------------------------------
    def builds(self):
        return self.lines('cargo.log', 'build')

    def lines(self, name, prefix=''):
        path = self.fake / name
        return [l for l in path.read_text().splitlines() if l.startswith(prefix)] if path.exists() else []

    def hub_calls(self, verb):
        return self.lines('hub.log', verb)

    def receipt(self, game=None):
        return hub_deploy.read_json(self.state / 'deployed' / f'{game or self.GAME}.json')

    def installed(self):
        path = self.install / self.BIN
        return path.read_text() if path.exists() else None

    def inputs(self, extra=('--no-default-features',), env=None):
        settings = hub_deploy.Settings(env or self.env())
        known = (self.receipt() or {}).get('extras')
        return hub_deploy.compute_inputs(settings, self.game, self.BIN, list(extra), known=known)

    def deploy(self):
        """A first, successful deployment: build aaaa0001, verified, installed, activated, ready."""
        done = self.run()
        assert done.returncode == 0, done.stdout + done.stderr
        assert self.receipt()['phase'] == 'ready'
        return done


@unittest.skipUnless(HAVE_SHELL, 'update.sh needs bash and flock')
class DeployTestCase(unittest.TestCase):
    def setUp(self):
        self.f = Fixture(self)

    def assertOk(self, done):
        self.assertEqual(done.returncode, 0, done.stdout + done.stderr)

    def assertFailed(self, done, needle=None):
        self.assertNotEqual(done.returncode, 0, done.stdout + done.stderr)
        if needle:
            self.assertIn(needle, done.stdout + done.stderr)


class InputMutationGuard(unittest.TestCase):
    def test_retired_inputs_are_reread_and_must_remain_unchanged(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'old-checkout-asset'
            path.write_bytes(b'original')
            before, after = hub_deploy.Inputs(), hub_deploy.Inputs()
            before.files[str(path)] = hub_deploy.sha256_file(path)
            self.assertIsNone(hub_deploy.changed_during_build(before, after, 0))
            path.write_bytes(b'changed')
            self.assertEqual(hub_deploy.changed_during_build(before, after, 0), str(path))
            path.unlink()
            self.assertEqual(hub_deploy.changed_during_build(before, after, 0), str(path))

    def test_unreadable_retired_input_fails_closed(self):
        before, after = hub_deploy.Inputs(), hub_deploy.Inputs()
        before.files['old-checkout-asset'] = 'digest'
        with mock.patch.object(hub_deploy, 'sha256_file', side_effect=PermissionError('denied')):
            self.assertEqual(hub_deploy.changed_during_build(before, after, 0), 'old-checkout-asset')


class FirstDeployment(DeployTestCase):
    def test_a_first_run_builds_verifies_installs_activates_and_only_then_says_ready(self):
        done = self.f.run()
        self.assertOk(done)
        out = done.stdout
        for step in ('building game-server', 'candidate ok: build aaaa0001', 'installed ', 'activated: the hub accepted the reload',
                     'not yet proof', 'ready: the hub'):
            self.assertIn(step, out)
        self.assertLess(out.index('activated:'), out.index('ready:'))
        self.assertIn('done: 1 game(s) updated', out)
        r = self.f.receipt()
        self.assertEqual((r['phase'], r['complete'], r['readiness']), ('ready', True, 'ready'))
        self.assertEqual(r['info']['build'], 'aaaa0001')
        self.assertEqual(r['binary']['path'], str(self.f.install / 'game-server'))
        self.assertEqual(r['binary']['sha256'], hub_deploy.sha256_file(self.f.install / 'game-server'))
        for stamp in ('built_at', 'installed_at', 'activated_at', 'ready_at'):
            self.assertTrue(r[stamp], stamp)
        self.assertIn('source:be2', r['components'])
        self.assertIn('source:game', r['components'])
        self.assertEqual(
            [c.split()[0] for c in self.f.builds()], ['build'])
        self.assertIn('--locked --release --no-default-features --bin game-server', self.f.builds()[0])
        # The candidate was checked in place beside the destination and checked to land where hub.conf looks.
        verify = self.f.hub_calls('verify')[0]
        self.assertIn('.game-server.candidate', verify)
        self.assertIn('--start', verify)
        self.assertIn('--installs-to', verify)
        self.assertFalse((self.f.install / '.game-server.candidate').exists(), 'no staged file is left behind')
        self.assertEqual(self.f.hub_calls('reload'), ['reload game'], 'only the affected game is reloaded')

    def test_the_run_uses_only_the_directories_it_was_given(self):
        # With every BLUEENGINE_* variable set, nothing is created under HOME; with only HOME set, the defaults
        # are derived from it (so a test or a second user can never reach somebody else's installation).
        self.f.deploy()
        self.assertEqual(list(self.f.home_dir.iterdir()), [], 'HOME untouched when the BLUEENGINE_* variables are set')
        self.assertTrue((self.f.state / 'update.lock').exists())
        home = Path(tempfile.mkdtemp(prefix='hubdeploy-home-'))
        self.addCleanup(shutil.rmtree, home, True)
        (home / '.config/blueengine').mkdir(parents=True)
        shutil.copy(self.f.config / 'sources.conf', home / '.config/blueengine/sources.conf')
        env = self.f.env(HOME=str(home))
        for k in ('BLUEENGINE_HOME', 'BLUEENGINE_CONFIG', 'BLUEENGINE_STATE', 'BLUEENGINE_LOCK', 'BLUEENGINE_ENGINE'):
            env.pop(k)
        done = self.f.run(env=env)
        self.assertTrue((home / '.local/share/blueengine/update.lock').exists(), done.stdout + done.stderr)
        self.assertTrue((home / 'blueengine').is_dir())


class RebuildDecisions(DeployTestCase):
    def test_a_run_with_nothing_relevant_changed_starts_no_cargo_build_and_no_hub_commands(self):
        self.f.deploy()
        builds, hub = len(self.f.builds()), len(self.f.hub_calls(''))
        done = self.f.run()
        self.assertOk(done)
        self.assertIn('up to date', done.stdout)
        self.assertIn('done: 0 game(s) updated', done.stdout)
        self.assertEqual(len(self.f.builds()), builds, 'zero cargo build invocations')
        self.assertEqual(len(self.f.hub_calls('')), hub, 'the hub was not asked anything')

    def test_an_engine_only_change_rebuilds_a_game_that_depends_on_the_engine_by_path(self):
        self.f.deploy()
        self.f.write(self.f.engine / 'src/netplay/mod.rs', '// netplay changed\n')
        self.f.set_artifact('aaaa0002')
        done = self.f.run()
        self.assertOk(done)
        self.assertIn('rebuilding: source of be2 changed', done.stdout)
        self.assertEqual(len(self.f.builds()), 2)
        self.assertEqual(self.f.receipt()['info']['build'], 'aaaa0002')

    def test_changes_that_cannot_reach_the_executable_do_not_rebuild(self):
        self.f.deploy()
        before = self.f.receipt()['identity']
        # Other games, the engine's docs, assets nothing includes, its other executables, the game's own docs and
        # build output, untracked clutter, and a package that is only a dev-dependency.
        self.f.write(self.f.other / 'src/lib.rs', 'pub fn other() { /* edited by another agent */ }\n')
        self.f.write(self.f.other / 'src/brand_new.rs', '// untracked, in another game\n')
        self.f.write(self.f.engine / 'docs/GUIDE.md', 'docs edited\n')
        self.f.write(self.f.engine / 'assets/pic.png', 'png-v2')
        self.f.write(self.f.engine / 'src/bin/be2-hub.rs', 'fn main() { /* another executable */ }\n')
        self.f.write(self.f.game / 'docs/NOTES.md', 'notes edited\n')
        self.f.write(self.f.game / 'scratch.txt', 'untracked and not source\n')
        self.f.write(self.f.game / 'target/debug/junk', 'build output\n')
        git(self.f.ws, 'commit', '-q', '--allow-empty', '-m', 'a commit changes no input')
        self.assertEqual(self.f.inputs().identity, before)
        done = self.f.run()
        self.assertIn('up to date', done.stdout)
        self.assertEqual(len(self.f.builds()), 1)

    def test_relevant_untracked_source_changes_the_identity(self):
        self.f.deploy()
        base = self.f.inputs().identity
        self.f.write(self.f.game / 'src/extra.rs', 'pub fn untracked() {}\n')     # not added to git
        status = subprocess.run(['git', '-C', str(self.f.ws), 'status', '--short'], capture_output=True, text=True).stdout
        self.assertIn('?? games/game/src/extra.rs', status)
        changed = self.f.inputs()
        self.assertNotEqual(changed.identity, base)
        self.f.set_artifact('aaaa0002')
        done = self.f.run()
        self.assertIn('rebuilding: source of game changed', done.stdout)
        # Untracked source in the engine counts too (the game compiles it through the path dependency).
        self.f.write(self.f.engine / 'src/netplay/new_untracked.rs', '// new\n')
        self.assertNotEqual(self.f.inputs().identity, self.f.receipt()['identity'])

    def test_a_file_the_compiler_read_outside_src_counts_once_the_dep_info_lists_it(self):
        asset = self.f.engine / 'assets/pic.png'
        # The first build's dep-info (the fake writes what rustc would) names the asset as an include_bytes! target.
        self.f.ctl(depinfo={'vesper3d': [str(asset)], 'game-server': ['src/bin/server.rs']})
        self.f.deploy()
        base = self.f.receipt()['identity']
        self.assertEqual(self.f.inputs().identity, base)
        # Dependency-info of some other checkout in a shared target directory is not consulted at all.
        stranger = self.f.tmp / 'elsewhere/secret.bin'
        self.f.write(stranger, 'one')
        self.f.write(self.f.fake / 'target/release/deps/vesper3d-ffff.d', f'out: {stranger}\n{stranger}:\n')
        self.f.write(stranger, 'two')
        self.assertEqual(self.f.inputs().identity, base, "another checkout's .d file in a shared target dir is ignored")
        self.f.write(asset, 'png-v3')
        self.assertNotEqual(self.f.inputs().identity, base, 'an include_bytes! target is a build input')
        self.f.set_artifact('aaaa0002')
        self.assertOk(self.f.run())
        self.assertEqual(self.f.receipt()['info']['build'], 'aaaa0002')

    def test_moving_engine_checkout_replaces_retired_dep_info_without_rejecting_the_build(self):
        old_asset = self.f.engine / 'assets/pic.png'
        self.f.ctl(depinfo={'vesper3d': [str(old_asset)]})
        self.f.deploy()
        old_engine = self.f.engine
        self.f.engine = self.f.tmp / 'new-engine'
        shutil.copytree(old_engine, self.f.engine)
        self.f.write_metadata()
        new_asset = self.f.engine / 'assets/pic.png'
        self.f.ctl(depinfo={'vesper3d': [str(new_asset)]})
        self.f.set_artifact('aaaa0002')
        self.assertOk(self.f.run())
        receipt = self.f.receipt()
        self.assertEqual(receipt['phase'], 'ready')
        self.assertEqual(receipt['info']['build'], 'aaaa0002')
        self.assertNotIn(str(old_asset), receipt['extras']['files'])
        self.assertIn(str(new_asset), receipt['extras']['files'])
        self.assertEqual(receipt['identity'], self.f.inputs().identity)
        self.assertIn('up to date', self.f.run().stdout)
        self.assertEqual(len(self.f.builds()), 2)

    def test_features_lockfile_toolchain_and_environment_invalidate_the_receipt(self):
        cases = ('a feature', 'the lockfile', 'the toolchain', 'RUSTFLAGS', 'a manifest', 'a cargo config')
        for label in cases:
            with self.subTest(label):
                f2 = Fixture(self)
                f2.deploy()
                base = f2.receipt()['identity']
                f2.set_artifact('aaaa0002')
                env = f2.env(RUSTFLAGS='-C target-cpu=native') if label == 'RUSTFLAGS' else None
                if label == 'a feature':
                    f2.write(f2.config / 'sources.conf', f'game {f2.game} game-server --no-default-features --features online\n')
                elif label == 'the lockfile':
                    f2.write(f2.ws / 'Cargo.lock', '# lock v2\n')
                elif label == 'the toolchain':
                    f2.write(f2.fake / 'rustc.txt', 'rustc 1.100.0 (fake)\n')
                elif label == 'a manifest':
                    f2.write(f2.engine / 'Cargo.toml', '[package]\nname = "be2"\n# edited\n')
                elif label == 'a cargo config':
                    f2.write(f2.game / '.cargo/config.toml', '[build]\nrustflags = ["-Cx"]\n')
                done = f2.run(env=env)
                self.assertOk(done)
                self.assertIn('rebuilding', done.stdout, label)
                self.assertEqual(len(f2.builds()), 2, label)
                self.assertNotEqual(f2.receipt()['identity'], base, label)
        # Locations and scheduling variables are not inputs.
        env = self.f.env(CARGO_TARGET_DIR='/elsewhere', CARGO_BUILD_JOBS='3', CARGO_HOME=str(self.f.tmp / 'ch'))
        self.assertEqual(self.f.inputs(env=env).components['environment'], self.f.inputs().components['environment'])

    def test_a_dependency_on_a_local_package_is_followed_and_dev_only_ones_are_not(self):
        self.f.deploy()
        base = self.f.inputs().identity
        self.f.write_metadata(with_other_dep=True)       # `other` becomes a normal path dependency of the game
        self.assertNotEqual(self.f.inputs().identity, base)
        self.f.write(self.f.other / 'src/lib.rs', 'pub fn other() { /* now it matters */ }\n')
        again = self.f.inputs().identity
        self.f.write(self.f.other / 'src/lib.rs', 'pub fn other() { /* and again */ }\n')
        self.assertNotEqual(self.f.inputs().identity, again)

    def test_inputs_that_cannot_be_identified_build_nothing_and_change_nothing(self):
        self.f.deploy()
        receipt, installed = self.f.receipt(), self.f.installed()
        self.f.ctl(metadata_fails=True)
        done = self.f.run()
        self.assertFailed(done, 'cannot identify the build inputs')
        self.assertEqual(len(self.f.builds()), 1)
        self.assertEqual((self.f.receipt(), self.f.installed()), (receipt, installed))
        self.f.ctl(metadata_fails=False)
        (self.f.ws / 'Cargo.lock').unlink()
        self.assertFailed(self.f.run(), 'Cargo.lock')
        self.assertEqual(len(self.f.builds()), 1)

    def test_force_rebuilds_and_reinstalls_unchanged_inputs(self):
        self.f.deploy()
        done = self.f.run('--force')
        self.assertOk(done)
        self.assertIn('--force', done.stdout)
        self.assertEqual(len(self.f.builds()), 2)


class FailureBoundaries(DeployTestCase):
    def snapshot(self):
        return {'installed': self.f.installed(), 'receipt': self.f.receipt(),
                'staged': (self.f.install / '.game-server.candidate').exists()}

    def deploy_then_break(self, mode, **kw):
        self.f.deploy()
        before = self.snapshot()
        self.f.write(self.f.game / 'src/lib.rs', 'pub fn rules() { /* v2 */ }\n')
        self.f.set_artifact('aaaa0002', mode=mode)
        return before

    def test_a_build_failure_changes_nothing(self):
        before = self.deploy_then_break('ok')
        self.f.ctl(build_fails=True)
        done = self.f.run()
        self.assertFailed(done, 'cargo build failed')
        self.assertEqual(self.snapshot(), before)
        self.assertEqual(self.f.hub_calls('reload'), ['reload game'], 'no second reload')

    def test_invalid_candidate_information_leaves_the_installed_server_and_the_deployment_record_unchanged(self):
        for mode in ('bad-info', 'slow-info', 'bad-settings'):
            with self.subTest(mode):
                self.f = Fixture(self)
                before = self.deploy_then_break(mode)
                done = self.f.run()
                self.assertFailed(done, 'candidate rejected')
                self.assertEqual(self.snapshot(), before)
                self.assertEqual(self.f.hub_calls('reload'), ['reload game'], 'the hub was not asked to reload')
                self.assertNotIn('installed ', done.stdout.split('building')[-1])

    def test_a_candidate_that_will_not_start_is_never_reported_as_activated(self):
        before = self.deploy_then_break('startup-fail')
        done = self.f.run()
        self.assertFailed(done, 'candidate rejected')
        self.assertIn('first STATUS line', done.stdout)
        self.assertNotIn('activated', done.stdout)
        self.assertNotIn('ready', done.stdout.replace('first STATUS', ''))
        self.assertEqual(self.snapshot(), before)
        self.assertEqual(self.f.hub_calls('reload'), ['reload game'])
        # The same unchanged, bad source is retried (and rejected again) on the next run: it is not recorded as done.
        self.assertFailed(self.f.run(), 'candidate rejected')

    def test_an_installation_failure_restores_the_record_and_keeps_the_old_server(self):
        self.f.deploy()
        before_receipt = self.f.receipt()
        self.f.write(self.f.game / 'src/lib.rs', 'pub fn rules() { /* v2 */ }\n')
        self.f.set_artifact('aaaa0002')
        # Make the final rename impossible: the destination becomes a non-empty directory.
        (self.f.install / 'game-server').unlink()
        (self.f.install / 'game-server').mkdir()
        (self.f.install / 'game-server' / 'x').write_text('x')
        done = self.f.run()
        self.assertFailed(done, 'installation failed')
        self.assertEqual(self.f.receipt(), before_receipt, 'the record of what is deployed is as it was')
        self.assertFalse((self.f.install / '.game-server.candidate').exists())
        self.assertEqual(self.f.hub_calls('reload'), ['reload game'])

    def test_a_source_change_while_cargo_runs_prevents_promotion_under_a_stale_identity(self):
        self.f.deploy()
        before = self.snapshot()
        self.f.write(self.f.game / 'src/lib.rs', 'pub fn rules() { /* v2 */ }\n')
        self.f.set_artifact('aaaa0002')
        # Another agent edits the engine while the build is running.
        self.f.ctl(write_during={str(self.f.engine / 'src/lib.rs'): 'pub fn engine() -> u32 { 2 }\n'})
        done = self.f.run()
        self.assertFailed(done, 'changed while cargo was running')
        self.assertEqual(self.snapshot(), before)
        self.assertEqual(self.f.hub_calls('verify'), self.f.hub_calls('verify')[:1], 'the stale output was never even checked')
        # Once the tree is quiet, the next run builds and promotes under the identity of what it really built from.
        self.f.ctl(write_during={})
        done = self.f.run()
        self.assertOk(done)
        self.assertEqual(self.f.receipt()['identity'], self.f.inputs().identity)

    def test_a_reload_failure_is_a_retryable_installed_state_not_a_success_stamp(self):
        self.f.deploy()
        self.f.write(self.f.game / 'src/lib.rs', 'pub fn rules() { /* v2 */ }\n')
        self.f.set_artifact('aaaa0002')
        self.f.ctl(reload=['fail', 'ok'])
        done = self.f.run()
        self.assertFailed(done, 'installed but NOT activated')
        r = self.f.receipt()
        self.assertEqual((r['phase'], r['complete']), ('installed', False))
        self.assertIn('aaaa0002', self.f.installed())
        builds = len(self.f.builds())
        done = self.f.run()                        # the next run: no rebuild, the activation is retried
        self.assertOk(done)
        self.assertIn('resuming without a rebuild', done.stdout)
        self.assertEqual(len(self.f.builds()), builds)
        self.assertEqual(self.f.receipt()['phase'], 'ready')
        self.assertIn('up to date', self.f.run().stdout)

    def test_a_readiness_failure_stays_recoverable_and_is_never_skipped_as_up_to_date(self):
        self.f.deploy()
        self.f.write(self.f.game / 'src/lib.rs', 'pub fn rules() { /* v2 */ }\n')
        self.f.set_artifact('aaaa0002')
        self.f.ctl(status=['starting', 'starting', 'starting', 'ready'])  # reload+wait, then no-wait + wait, then no-wait
        done = self.f.run()
        self.assertFailed(done, 'activated but NOT ready (starting)')
        r = self.f.receipt()
        self.assertEqual((r['phase'], r['complete'], r['readiness']), ('activated', False, 'starting'))
        reloads = len(self.f.hub_calls('reload'))
        done = self.f.run()                        # the hub is there but still silent: wait again, do not reload again
        self.assertFailed(done, 'NOT ready')
        self.assertEqual(len(self.f.hub_calls('reload')), reloads)
        done = self.f.run()                        # now the Public room reports the new build
        self.assertOk(done)
        self.assertEqual(self.f.receipt()['phase'], 'ready')
        self.assertEqual(len(self.f.hub_calls('reload')), reloads, 'recovered without another reload or a rebuild')
        self.assertEqual(len(self.f.builds()), 2)

    def test_a_hub_that_still_runs_the_old_build_gets_a_second_reload(self):
        self.f.deploy()
        self.f.write(self.f.game / 'src/lib.rs', 'pub fn rules() { /* v2 */ }\n')
        self.f.set_artifact('aaaa0002')
        self.f.ctl(status=['wrong-build'])
        self.assertFailed(self.f.run(), 'wrong-build')
        reloads = len(self.f.hub_calls('reload'))
        self.f.ctl(status=['wrong-build', 'ready'])
        done = self.f.run()
        self.assertOk(done)
        self.assertEqual(len(self.f.hub_calls('reload')), reloads + 1)
        self.assertEqual(self.f.receipt()['phase'], 'ready')

    def test_a_hub_that_cannot_answer_status_is_reported_honestly(self):
        self.f.deploy()
        self.f.write(self.f.game / 'src/lib.rs', 'pub fn rules() { /* v2 */ }\n')
        self.f.set_artifact('aaaa0002')
        self.f.ctl(status=['no-answer'])
        done = self.f.run()
        self.assertFailed(done, 'no-answer')
        self.assertIn('restart it', done.stdout)
        self.assertEqual(self.f.receipt()['complete'], False)
        reloads = len(self.f.hub_calls('reload'))
        self.assertFailed(self.f.run(), 'no-answer')
        self.assertEqual(len(self.f.hub_calls('reload')), reloads, 'an accepted reload is not repeated every run')

    def test_a_game_without_a_public_room_is_activated_but_not_claimed_ready(self):
        self.f.ctl(status='registry-only')
        done = self.f.run()
        self.assertOk(done)
        self.assertIn('has no Public room', done.stdout)
        self.assertNotIn('ready: the hub', done.stdout)
        r = self.f.receipt()
        self.assertEqual((r['phase'], r['readiness'], r['complete']), ('activated', 'registry-only', True))
        self.assertIn('up to date', self.f.run().stdout)


class HubStopped(DeployTestCase):
    def test_with_the_hub_stopped_the_result_is_installed_activation_pending_and_a_later_run_finishes_it(self):
        self.f.hub_active(False)
        done = self.f.run()
        self.assertOk(done)
        self.assertIn('installed, activation pending', done.stdout)
        self.assertNotIn('ready:', done.stdout)
        self.assertNotIn('activated:', done.stdout)
        r = self.f.receipt()
        self.assertEqual((r['phase'], r['complete']), ('installed', False))
        self.assertEqual(self.f.hub_calls('reload'), [], 'a stopped hub is not asked anything')
        again = self.f.run()
        self.assertIn('activation pending', again.stdout)
        self.assertEqual(len(self.f.builds()), 1)
        # The hub starts (on its own, it reads the installed file): the next run sees it running the new build and
        # records ready without reloading it.
        self.f.hub_active(True)
        self.f.ctl(status=['ready'])
        done = self.f.run()
        self.assertOk(done)
        self.assertEqual(self.f.receipt()['phase'], 'ready')
        self.assertEqual(self.f.hub_calls('reload'), [])
        self.assertEqual(len(self.f.builds()), 1)


class Interruption(DeployTestCase):
    def test_a_run_killed_after_installation_is_finished_by_the_next_run_without_a_rebuild(self):
        self.f.deploy()
        self.f.write(self.f.game / 'src/lib.rs', 'pub fn rules() { /* v2 */ }\n')
        self.f.set_artifact('aaaa0002')
        self.f.ctl(reload=['kill-parent', 'ok'])
        done = self.f.run()
        self.assertNotEqual(done.returncode, 0)                   # the updater was SIGKILLed mid-reload
        self.assertIn('aaaa0002', self.f.installed())
        r = self.f.receipt()
        self.assertEqual((r['phase'], r['complete']), ('installed', False), 'no success stamp was written')
        builds = len(self.f.builds())
        done = self.f.run()
        self.assertOk(done)
        self.assertEqual(len(self.f.builds()), builds)
        self.assertEqual(self.f.receipt()['phase'], 'ready')

    def test_a_crash_between_the_rename_and_the_installed_record_is_recovered(self):
        self.f.deploy()
        first = self.f.receipt()
        self.f.write(self.f.game / 'src/lib.rs', 'pub fn rules() { /* v2 */ }\n')
        self.f.set_artifact('aaaa0002')
        self.assertOk(self.f.run())
        done_receipt = self.f.receipt()
        # Reconstruct the crash window: the executable is in place, the receipt still says "installing".
        crashed = dict(done_receipt, phase='installing', complete=False)
        for key in ('installed_at', 'activated_at', 'ready_at', 'readiness'):
            crashed.pop(key, None)
        hub_deploy.write_json_atomic(self.f.state / 'deployed/game.json', crashed)
        builds = len(self.f.builds())
        self.f.ctl(status=['ready'])
        done = self.f.run()
        self.assertOk(done)
        self.assertIn('recovered', done.stdout)
        self.assertEqual(len(self.f.builds()), builds)
        self.assertEqual(self.f.receipt()['phase'], 'ready')
        self.assertNotEqual(first['identity'], self.f.receipt()['identity'])

    def test_a_crash_before_the_rename_leaves_the_old_server_and_the_next_run_redoes_the_install(self):
        self.f.deploy()
        old = self.f.installed()
        self.f.write(self.f.game / 'src/lib.rs', 'pub fn rules() { /* v2 */ }\n')
        self.f.set_artifact('aaaa0002')
        self.assertOk(self.f.run())
        done_receipt = self.f.receipt()
        # Crash window before the rename: the receipt names the new binary, the installed file is still the old one.
        (self.f.install / 'game-server').write_text(old)
        hub_deploy.write_json_atomic(self.f.state / 'deployed/game.json', dict(done_receipt, phase='installing', complete=False))
        done = self.f.run()
        self.assertOk(done)
        self.assertIn('is not the one recorded', done.stdout)
        self.assertIn('aaaa0002', self.f.installed())
        self.assertEqual(self.f.receipt()['phase'], 'ready')


class PreviousArtifactAndRollback(DeployTestCase):
    def prepare_rollback(self):
        self.f.deploy()
        first = self.f.installed()
        self.assertOk(self.update_to('aaaa0002'))
        return first

    def update_to(self, build, mode='ok'):
        self.f.write(self.f.game / 'src/lib.rs', f'pub fn rules() {{ /* {build} */ }}\n')
        self.f.set_artifact(build, mode)
        return self.f.run()

    def test_the_known_working_server_is_kept_and_an_unfinished_install_does_not_replace_it(self):
        self.f.deploy()
        first = self.f.installed()
        self.f.ctl(status=['starting'])
        self.assertFailed(self.update_to('aaaa0002'), 'NOT ready')
        self.assertEqual((self.f.install / 'game-server.previous').read_text(), first)
        # A third build replaces the unfinished second one: the previous known-good file is still the first.
        self.f.ctl(status=['ready'])
        self.assertOk(self.update_to('aaaa0003'))
        self.assertEqual((self.f.install / 'game-server.previous').read_text(), first)
        self.assertOk(self.update_to('aaaa0004'))
        self.assertIn('aaaa0003', (self.f.install / 'game-server.previous').read_text())

    def test_rollback_reinstalls_the_previous_server_validates_it_and_is_not_undone_by_the_next_run(self):
        self.f.deploy()
        self.assertOk(self.update_to('aaaa0002'))
        done = self.f.run('--rollback', 'game')
        self.assertOk(done)
        self.assertIn('rolled back', done.stdout)
        self.assertIn('aaaa0001', self.f.installed())
        self.assertEqual(self.f.receipt()['info']['build'], 'aaaa0001')
        self.assertEqual(self.f.receipt()['phase'], 'ready')
        self.assertEqual(self.f.hub_calls('reload')[-1], 'reload game')
        builds = len(self.f.builds())
        again = self.f.run()
        self.assertIn('was rolled back', again.stdout)
        self.assertEqual(len(self.f.builds()), builds)
        self.assertIn('aaaa0001', self.f.installed())
        # Changing the source (or --force) deploys again.
        self.assertOk(self.update_to('aaaa0005'))
        self.assertIn('aaaa0005', self.f.installed())

    def test_rollback_without_a_previous_server_says_so(self):
        done = self.f.run('--rollback', 'game')
        self.assertFailed(done, 'no ')
        self.assertIn('to roll back to', done.stdout)

    def test_stopped_hub_rollback_resumes_readiness_without_redeploying_rejected_source(self):
        first = self.prepare_rollback()
        self.f.hub_active(False)
        self.assertOk(self.f.run('--rollback', 'game'))
        self.assertEqual((self.f.receipt()['phase'], self.f.receipt()['complete']), ('installed', False))
        builds, statuses = len(self.f.builds()), len(self.f.hub_calls('status'))
        self.f.hub_active(True)
        self.assertOk(self.f.run())
        self.assertEqual(self.f.installed(), first)
        self.assertEqual(len(self.f.builds()), builds)
        self.assertGreater(len(self.f.hub_calls('status')), statuses)
        self.assertEqual((self.f.receipt()['phase'], self.f.receipt()['complete']), ('ready', True))
        self.assertIn('restored executable verified', self.f.run().stdout)

    def test_failed_rollback_activation_and_readiness_remain_retryable(self):
        self.prepare_rollback()
        self.f.ctl(reload=['fail', 'ok'], status=['real', 'starting', 'starting', 'ready'])
        self.assertFailed(self.f.run('--rollback', 'game'), 'NOT activated')
        builds = len(self.f.builds())
        self.assertEqual((self.f.receipt()['phase'], self.f.receipt()['complete']), ('installed', False))
        self.assertFailed(self.f.run(), 'NOT ready')
        self.assertEqual((self.f.receipt()['phase'], self.f.receipt()['complete']), ('activated', False))
        reloads = len(self.f.hub_calls('reload'))
        self.assertOk(self.f.run())
        self.assertEqual(len(self.f.hub_calls('reload')), reloads, 'readiness recovery does not retire rooms again')
        self.assertEqual(len(self.f.builds()), builds)
        self.assertTrue(self.f.receipt()['complete'])

    def test_interrupted_rollback_on_either_side_of_installation_finishes_without_a_build(self):
        first = self.prepare_rollback()
        rejected = self.f.installed()
        self.assertOk(self.f.run('--rollback', 'game'))
        complete = self.f.receipt()
        builds = len(self.f.builds())
        for before_rename in (True, False):
            with self.subTest(before_rename=before_rename):
                (self.f.install / self.f.BIN).write_text(rejected if before_rename else first)
                hub_deploy.write_json_atomic(self.f.state / 'deployed/game.json',
                                             dict(complete, phase='installing', complete=False))
                self.f.ctl(status=['ready'])
                self.assertOk(self.f.run())
                self.assertEqual(self.f.installed(), first)
                self.assertEqual(len(self.f.builds()), builds)
                self.assertEqual((self.f.receipt()['phase'], self.f.receipt()['complete']), ('ready', True))

    def test_failed_rollback_rename_retains_a_recoverable_receipt(self):
        first = self.prepare_rollback()
        builds = len(self.f.builds())
        destination = self.f.install / self.f.BIN
        destination.unlink()
        destination.mkdir()
        obstacle = destination / 'blocks-rename'
        obstacle.write_text('fixture')
        self.assertFailed(self.f.run('--rollback', 'game'), 'FAILED')
        self.assertEqual((self.f.receipt()['phase'], self.f.receipt()['complete']), ('installing', False))
        self.assertFailed(self.f.run(), 'rollback installation failed')
        self.assertEqual(len(self.f.builds()), builds)
        obstacle.unlink()
        destination.rmdir()
        self.assertOk(self.f.run())
        self.assertEqual(self.f.installed(), first)
        self.assertEqual(len(self.f.builds()), builds)
        self.assertTrue(self.f.receipt()['complete'])

    def test_missing_or_modified_restored_executable_is_recovered_from_verified_previous(self):
        first = self.prepare_rollback()
        self.assertOk(self.f.run('--rollback', 'game'))
        builds = len(self.f.builds())
        dest = self.f.install / self.f.BIN
        for missing in (True, False):
            with self.subTest(missing=missing):
                if missing:
                    dest.unlink()
                else:
                    recorded_mtime = dest.stat().st_mtime_ns
                    dest.write_text(first.replace('aaaa0001', 'ffff0001'))
                    os.utime(dest, ns=(recorded_mtime, recorded_mtime))
                self.assertOk(self.f.run())
                self.assertEqual(self.f.installed(), first)
                self.assertEqual(len(self.f.builds()), builds)
                self.assertTrue(self.f.receipt()['complete'])

    def test_missing_or_inconsistent_previous_fails_without_building_rejected_source(self):
        self.prepare_rollback()
        self.assertOk(self.f.run('--rollback', 'game'))
        builds = len(self.f.builds())
        receipt = self.f.receipt()
        (self.f.install / self.f.BIN).unlink()
        previous = self.f.install / (self.f.BIN + '.previous')
        for missing in (False, True):
            with self.subTest(missing=missing):
                if missing:
                    previous.unlink()
                else:
                    previous.write_text('inconsistent artifact')
                self.assertFailed(self.f.run(), 'rollback recovery failed')
                self.assertEqual(len(self.f.builds()), builds)
                self.assertEqual(self.f.receipt(), receipt)
                self.assertIsNone(self.f.installed())


class IsolationBetweenGames(DeployTestCase):
    def add_second_game(self):
        f = self.f
        f.write(f.ws / 'games/second/Cargo.toml', '[package]\nname = "second"\n')
        f.write(f.ws / 'games/second/src/lib.rs', 'pub fn two() {}\n')
        f.write(f.ws / 'games/second/src/bin/server.rs', 'fn main() {}\n')
        meta = json.loads((f.fake / 'metadata.json').read_text())
        second = {'id': 'second', 'name': 'second', 'source': None, 'manifest_path': str(f.ws / 'games/second/Cargo.toml'),
                  'targets': [{'name': 'second', 'kind': ['lib'], 'src_path': str(f.ws / 'games/second/src/lib.rs')},
                              {'name': 'second-server', 'kind': ['bin'], 'src_path': str(f.ws / 'games/second/src/bin/server.rs')}]}
        meta['packages'].append(second)
        meta['workspace_members'].append('second')
        meta['resolve']['nodes'].append({'id': 'second', 'deps': [{'pkg': 'be2', 'dep_kinds': [{'kind': None}]}]})
        f.write(f.fake / 'metadata.json', json.dumps(meta))
        f.write(f.config / 'sources.conf', f'game {f.game} game-server\nsecond {f.ws / "games/second"} second-server\n')
        f.write(f.config / 'hub.conf', f'[game game]\nserver = {f.install / "game-server"}\n[game second]\nserver = {f.install / "second-server"}\n')
        f.ctl(paths={'game': str(f.install / 'game-server'), 'second': str(f.install / 'second-server')})
        f.set_artifact('bbbb0001', binary='second-server')
        f.set_artifact('aaaa0001')

    def test_updating_one_game_reloads_only_that_game_and_leaves_the_others_receipt_and_server_alone(self):
        self.add_second_game()
        self.assertOk(self.f.run())
        second_before = (self.f.receipt('second'), (self.f.install / 'second-server').read_text())
        self.f.write(self.f.game / 'src/lib.rs', 'pub fn rules() { /* v2 */ }\n')
        self.f.set_artifact('aaaa0002')
        reloads = len(self.f.hub_calls('reload'))
        done = self.f.run('game')
        self.assertOk(done)
        self.assertEqual(self.f.hub_calls('reload')[reloads:], ['reload game'])
        self.assertEqual((self.f.receipt('second'), (self.f.install / 'second-server').read_text()), second_before)
        self.assertNotIn('second:', done.stdout)

    def test_rollback_recovery_leaves_the_other_games_artifact_and_receipt_unchanged(self):
        self.add_second_game()
        self.assertOk(self.f.run())
        second = (self.f.receipt('second'), (self.f.install / 'second-server').read_text())
        self.f.write(self.f.game / 'src/lib.rs', 'pub fn rules() { /* v2 */ }\n')
        self.f.set_artifact('aaaa0002')
        self.assertOk(self.f.run('game'))
        self.f.hub_active(False)
        self.assertOk(self.f.run('--rollback', 'game'))
        self.f.hub_active(True)
        reloads = len(self.f.hub_calls('reload'))
        self.assertOk(self.f.run())
        self.assertEqual((self.f.receipt('second'), (self.f.install / 'second-server').read_text()), second)
        self.assertEqual(self.f.hub_calls('reload')[reloads:], ['reload game'])

    def test_a_failing_game_does_not_stop_the_others_but_fails_the_run(self):
        self.add_second_game()
        self.f.set_artifact('aaaa0001', mode='startup-fail')
        done = self.f.run()
        self.assertFailed(done, 'game: FAILED')
        self.assertIn('FAILED: game', done.stdout.replace('game: FAILED', 'FAILED: game'))
        self.assertEqual(self.f.receipt('second')['phase'], 'ready', 'the other game was still deployed')
        self.assertIsNone(self.f.receipt('game'))
        self.assertIsNone(self.f.installed(), 'nothing of the failing game was installed')
        self.assertIn('1 FAILED: game', done.stdout)

    def test_both_games_update_when_the_engine_changes(self):
        self.add_second_game()
        self.assertOk(self.f.run())
        self.f.write(self.f.engine / 'src/lib.rs', 'pub fn engine() -> u32 { 9 }\n')
        self.f.set_artifact('aaaa0002')
        self.f.set_artifact('bbbb0002', binary='second-server')
        done = self.f.run()
        self.assertOk(done)
        self.assertIn('done: 2 game(s) updated', done.stdout)


class CommandLine(DeployTestCase):
    def test_help_unknown_options_and_games(self):
        done = self.f.run('--help')
        self.assertOk(done)
        self.assertIn('--rollback', done.stdout)
        self.assertEqual(self.f.run('--nonsense').returncode, 2)
        self.assertFailed(self.f.run('ghost'), 'not in sources.conf')
        self.assertEqual(self.f.run('--rollback').returncode, 2)

    def test_a_second_run_at_the_same_time_does_nothing(self):
        import fcntl
        with open(self.f.state / 'update.lock', 'w') as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            done = self.f.run()
        self.assertOk(done)
        self.assertIn('another update is running', done.stdout)
        self.assertEqual(self.f.builds(), [])

    def test_a_missing_hub_executable_or_one_without_verify_is_refused_with_the_remedy(self):
        write_exe(self.f.install / 'be2-hub', '#!/bin/sh\necho "be2-hub: old"\n')
        done = self.f.run()
        self.assertFailed(done, 'update.sh --hub first')
        self.assertIsNone(self.f.installed())

    def test_hub_option_builds_checks_and_installs_be2_hub_before_the_games(self):
        self.f.write(self.f.engine / 'Cargo.lock', '# engine lock\n')
        self.f.set_artifact('00000000', binary='be2-hub')
        # The fake "build" of be2-hub is the fake hub itself, so its --help mentions verify.
        self.f.ctl(artifacts={'be2-hub': FAKE_HUB, 'game-server': '#!/bin/sh\n# BUILD=aaaa0001 MODE=ok\n'})
        old = (self.f.install / 'be2-hub').read_text()
        done = self.f.run('--hub')
        self.assertOk(done)
        self.assertLess(done.stdout.index('hub: installed'), done.stdout.index('game: '))
        self.assertIn('Restart it when nobody is playing', done.stdout)
        self.assertEqual((self.f.install / 'be2-hub.previous').read_text(), old)
        self.assertNotIn('systemctl', ' '.join(self.f.lines('systemctl.log', 'restart')), 'never restarts the hub itself')
        # A built be2-hub that does not describe itself is not installed.
        self.f.ctl(artifacts={'be2-hub': '#!/bin/sh\nexit 1\n', 'game-server': '#!/bin/sh\n# BUILD=aaaa0001 MODE=ok\n'})
        installed = (self.f.install / 'be2-hub').read_text()
        done = self.f.run('--hub')
        self.assertFailed(done, 'does not describe itself')
        self.assertEqual((self.f.install / 'be2-hub').read_text(), installed)


class Units(unittest.TestCase):
    def test_dep_info_parsing_handles_escaped_spaces_env_deps_and_comments(self):
        text = ('out: a.rs b\\ c.rs\n\na.rs:\nb c.rs:\n# env-dep:FOO=bar\n# env-dep:CARGO_PKG_NAME=x\n')
        files, envs = hub_deploy.parse_dep_info(text)
        self.assertEqual(files, ['a.rs', 'b c.rs'])
        self.assertEqual(envs, ['FOO', 'CARGO_PKG_NAME'])
        # Cargo's own BIN.d: one rule, absolute paths, a space escaped.
        files, _ = hub_deploy.parse_dep_info('/t/release/game-server: /w/dep/data.bin /w/my\\ dir/lib.rs\n')
        self.assertEqual(files, ['/w/dep/data.bin', '/w/my dir/lib.rs'])

    def test_sources_conf_lines_need_a_root_and_a_bin_and_expand_the_home(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / 'sources.conf'
            path.write_text('# c\n\ng ~/src/g g-server --no-default-features -p g\n')
            games = hub_deploy.parse_sources(path, '/home/x')
            self.assertEqual(games, [{'game': 'g', 'root': Path('/home/x/src/g'), 'bin': 'g-server',
                                      'extra': ['--no-default-features', '-p', 'g']}])
            path.write_text('lonely\n')
            with self.assertRaises(hub_deploy.DeployError):
                hub_deploy.parse_sources(path, '/home/x')

    def test_result_affecting_environment_is_selected_and_locations_are_not(self):
        yes = ['RUSTFLAGS', 'RUSTC_WRAPPER', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_TARGET', 'CARGO_PROFILE_RELEASE_LTO',
               'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER', 'RUSTUP_TOOLCHAIN']
        no = ['CARGO_TARGET_DIR', 'CARGO_HOME', 'CARGO_BUILD_JOBS', 'CARGO_INCREMENTAL', 'CARGO_TERM_COLOR', 'PATH', 'HOME']
        self.assertTrue(all(hub_deploy.RESULT_ENV.match(k) for k in yes))
        self.assertFalse(any(hub_deploy.RESULT_ENV.match(k) for k in no))

    def test_metadata_arguments_forward_only_what_changes_the_graph(self):
        args, package = hub_deploy.forwarded_metadata_args(
            ['--no-default-features', '--features', 'a,b', '--target', 'x86_64-pc-windows-gnu', '-p', 'game', '--jobs', '2'])
        self.assertEqual(args, ['--no-default-features', '--features', 'a,b', '--filter-platform', 'x86_64-pc-windows-gnu'])
        self.assertEqual(package, 'game')


if __name__ == '__main__':
    unittest.main()
