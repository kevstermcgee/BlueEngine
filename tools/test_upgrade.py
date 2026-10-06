"""Fixture-based tests for the upgrade planner/verifier: read-only planning, stable repeat runs,
runtime classification, migration selection against this repo's real seeded history, dirty/mismatched
evidence, and truthful fresh verification."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from tools import upgrade, workflow

ROOT = Path(__file__).resolve().parents[1]
# Real, verified commits from this repository's own history (see tools/upgrade_migrations.json).
OLD_BASELINE = '79d82a2'  # predates every seeded migration


def run_git(repo, *args):
    return subprocess.run(['git', '-C', str(repo), *args], check=True, capture_output=True,
                          text=True, **workflow.console_options()).stdout


def init_repo(root):
    run_git(root, 'init', '-q')
    run_git(root, 'config', 'user.email', 'test@example.invalid')
    run_git(root, 'config', 'user.name', 'Upgrade Test')


def commit_all(root, message):
    run_git(root, 'add', '-A')
    run_git(root, 'commit', '-q', '-m', message)
    return run_git(root, 'rev-parse', 'HEAD').strip()


# Drift compares exact template bytes; do not normalize checkout line endings.
TEMPLATE_CHECK = (ROOT / 'templates/game_check.py').read_bytes().decode('utf-8')


def write_identity(root, engine_revision=OLD_BASELINE, extra=None):
    document = {'title': 'Fixture Game', 'tagline': 'x', 'controls': 'y'}
    if engine_revision is not None:
        document['engine_revision'] = engine_revision
    if extra:
        document.update(extra)
    (root / 'assets').mkdir(exist_ok=True)
    (root / 'assets/identity.json').write_text(json.dumps(document), encoding='utf-8')


def write_cargo_toml(root, engine_path):
    (root / 'Cargo.toml').write_text(
        f'[package]\nname = "fixture"\nversion = "0.1.0"\n\n[dependencies]\n'
        f'vesper3d = {{ package = "be2", path = "{engine_path}", default-features = false }}\n',
        encoding='utf-8')


class CustomSimFixture:
    """A minimal custom-sim game pointed at the real engine checkout (ROOT) as its path dependency."""

    def __init__(self, extra_src=None, check_script=TEMPLATE_CHECK, engine_revision=OLD_BASELINE):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root / 'src').mkdir()
        (self.root / 'src/lib.rs').write_text('pub struct Sim;\n', encoding='utf-8')
        for name, text in (extra_src or {}).items():
            path = self.root / 'src' / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding='utf-8')
        write_cargo_toml(self.root, ROOT)
        write_identity(self.root, engine_revision=engine_revision)
        (self.root / 'scripts').mkdir(exist_ok=True)
        if check_script is not None:
            (self.root / 'scripts/check.py').write_bytes(check_script.encode('utf-8'))

    def close(self):
        self.temp.cleanup()


class SnapshotMixin:
    def snapshot(self, root):
        return {str(p.relative_to(root)): (p.stat().st_mtime_ns, p.stat().st_size)
               for p in Path(root).rglob('*') if p.is_file()}


class PlanningIsReadOnlyTests(SnapshotMixin, unittest.TestCase):
    def test_plan_touches_nothing_and_repeats_identically(self):
        fixture = CustomSimFixture()
        self.addCleanup(fixture.close)
        before = self.snapshot(fixture.root)
        first = upgrade.plan(fixture.root, ROOT, 'HEAD')
        after = self.snapshot(fixture.root)
        self.assertEqual(before, after, 'planning must not write, touch or remove any file in the game')
        second = upgrade.plan(fixture.root, ROOT, 'HEAD')
        first.pop('generated_at'); second.pop('generated_at')
        first.pop('human_summary'); second.pop('human_summary')
        self.assertEqual(first, second, 'identical inputs must produce an identical plan')

    def test_plan_without_out_writes_no_report_file(self):
        fixture = CustomSimFixture()
        self.addCleanup(fixture.close)
        before = set(Path(tempfile.gettempdir()).glob('*upgrade*'))
        upgrade.plan(fixture.root, ROOT, 'HEAD')
        after = set(Path(tempfile.gettempdir()).glob('*upgrade*'))
        self.assertEqual(before, after)


class RuntimeClassificationTests(unittest.TestCase):
    def test_stock_custom_sim_netplay_mixed_and_unknown(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'Cargo.toml').write_text('[package]\nname="x"\n')
            (root / 'game.json').write_text('{}')
            self.assertEqual(upgrade.classify_runtime(root)['kind'], 'stock')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'Cargo.toml').write_text('[package]\nname="x"\n')
            (root / 'src').mkdir()
            (root / 'src/lib.rs').write_text('pub struct Sim;\n')
            self.assertEqual(upgrade.classify_runtime(root)['kind'], 'custom_sim')
            (root / 'game.project.json').write_text(json.dumps({'presentation':'2d','networking':'offline','targets':['web']}))
            self.assertEqual(upgrade.classify_runtime(root)['kind'], 'two_d')
            (root / 'game.project.json').unlink()
            (root / 'src/net.rs').write_text('use vesper3d::viewer::netplay::NetGame;\n')
            self.assertEqual(upgrade.classify_runtime(root)['kind'], 'custom_sim_netplay')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'Cargo.toml').write_text('[package]\nname="x"\n')
            (root / 'game.json').write_text('{}')
            (root / 'src').mkdir()
            (root / 'src/lib.rs').write_text('pub struct Sim;\n')
            self.assertEqual(upgrade.classify_runtime(root)['kind'], 'mixed_or_legacy')
        with tempfile.TemporaryDirectory() as directory:
            self.assertEqual(upgrade.classify_runtime(Path(directory))['kind'], 'unknown')


class MigrationSelectionTests(unittest.TestCase):
    """Exercises real seeded history: the mouse-look fix (required_repair) and the netplay kit
    (optional_adoption), both genuine BlueEngine commits (see tools/upgrade_migrations.json)."""

    def by_id(self, packet):
        return {m['id']: m for m in packet['migrations']}

    def test_old_baseline_to_head_applies_mouselook_when_pattern_present(self):
        fixture = CustomSimFixture(extra_src={'camera.rs': 'fn look() { let d = mouse_delta_position(); }\n'})
        self.addCleanup(fixture.close)
        packet = upgrade.plan(fixture.root, ROOT, 'HEAD')
        migrations = self.by_id(packet)
        self.assertEqual(migrations['MIG-0020-MOUSELOOK']['status'], 'required_repair')
        self.assertEqual(migrations['MIG-0022-RENDER-TRAPS']['status'], 'received_automatically')

    def test_pattern_absent_is_not_applicable_even_in_revision_range(self):
        fixture = CustomSimFixture()  # no mouse_delta_position() anywhere
        self.addCleanup(fixture.close)
        migrations = self.by_id(upgrade.plan(fixture.root, ROOT, 'HEAD'))
        self.assertEqual(migrations['MIG-0020-MOUSELOOK']['status'], 'not_applicable')

    def test_already_applied_when_baseline_equals_target(self):
        head = run_git(ROOT, 'rev-parse', 'HEAD').strip()
        fixture = CustomSimFixture(engine_revision=head[:12])
        self.addCleanup(fixture.close)
        migrations = self.by_id(upgrade.plan(fixture.root, ROOT, 'HEAD'))
        self.assertEqual(migrations['MIG-0020-MOUSELOOK']['status'], 'already_applied')
        self.assertEqual(migrations['MIG-0022-NETPLAY-ADOPTION']['status'], 'already_applied')

    def test_netplay_adoption_excluded_once_a_game_already_uses_the_kit(self):
        fixture = CustomSimFixture(extra_src={
            'wire.rs': '// codec\n', 'server.rs': '// server\n', 'client.rs': '// client\n',
            'net.rs': 'use vesper3d::viewer::netplay::ClientView;\n'})
        self.addCleanup(fixture.close)
        migrations = self.by_id(upgrade.plan(fixture.root, ROOT, 'HEAD'))
        self.assertEqual(migrations['MIG-0022-NETPLAY-ADOPTION']['status'], 'not_applicable')
        self.assertIn('runtime', migrations['MIG-0022-NETPLAY-ADOPTION']['reason'])

    def test_netplay_adoption_applicable_for_hand_rolled_networking(self):
        fixture = CustomSimFixture(extra_src={'wire.rs': '// codec\n', 'server.rs': '// server\n',
                                              'client.rs': '// client\n'})
        self.addCleanup(fixture.close)
        migrations = self.by_id(upgrade.plan(fixture.root, ROOT, 'HEAD'))
        self.assertEqual(migrations['MIG-0022-NETPLAY-ADOPTION']['status'], 'optional_adoption')

    def test_server_cli_adoption_is_offered_only_to_games_with_a_hand_written_server_main(self):
        netplay = 'use vesper3d::viewer::netplay::ClientView;\n'
        hand_rolled = CustomSimFixture(extra_src={
            'net.rs': netplay,
            'bin/server.rs': 'fn main() { let s = NetServer::<MyGame, _>::new(t, cfg); }\n'})
        self.addCleanup(hand_rolled.close)
        self.assertEqual(self.by_id(upgrade.plan(hand_rolled.root, ROOT, 'HEAD'))['MIG-0037-GAME-SERVER-CLI']['status'],
                         'optional_adoption')
        adopted = CustomSimFixture(extra_src={
            'net.rs': netplay, 'bin/server.rs': 'fn main() -> R { serve::<MyGame>(&SPEC) }\n'})
        self.addCleanup(adopted.close)
        self.assertEqual(self.by_id(upgrade.plan(adopted.root, ROOT, 'HEAD'))['MIG-0037-GAME-SERVER-CLI']['status'],
                         'not_applicable')
        # A NetServer in the library (not a bin) is not a server main.
        in_lib = CustomSimFixture(extra_src={'net.rs': netplay + 'fn t() { NetServer::<G, _>::new(a, b); }\n'})
        self.addCleanup(in_lib.close)
        self.assertEqual(self.by_id(upgrade.plan(in_lib.root, ROOT, 'HEAD'))['MIG-0037-GAME-SERVER-CLI']['status'],
                         'not_applicable')

    def test_native_key_table_fix_is_reported_as_received_automatically(self):
        fixture = CustomSimFixture()
        self.addCleanup(fixture.close)
        migration = self.by_id(upgrade.plan(fixture.root, ROOT, 'HEAD'))['MIG-0035-NATIVE-KEY-TABLE']
        self.assertEqual(migration['status'], 'received_automatically')

    def test_restart_helper_is_offered_only_to_games_that_bind_r(self):
        with_r = CustomSimFixture(extra_src={'main.rs': 'if sim.over && input.pressed(KeyCode::R) { reset(); }\n'})
        self.addCleanup(with_r.close)
        self.assertEqual(self.by_id(upgrade.plan(with_r.root, ROOT, 'HEAD'))['MIG-0035-RESTART-CONVENTION']['status'],
                         'optional_adoption')
        # KeyCode::Right must not look like KeyCode::R.
        without = CustomSimFixture(extra_src={'main.rs': 'let go = input.down(KeyCode::Right);\n'})
        self.addCleanup(without.close)
        self.assertEqual(self.by_id(upgrade.plan(without.root, ROOT, 'HEAD'))['MIG-0035-RESTART-CONVENTION']['status'],
                         'not_applicable')

    def test_every_registry_commit_is_a_real_ancestor_of_head(self):
        ids = [m['id'] for m in upgrade.load_registry(ROOT)]
        self.assertEqual(len(ids), len(set(ids)))
        for migration in upgrade.load_registry(ROOT):
            self.assertTrue(upgrade.is_ancestor(ROOT, migration['since_commit'], 'HEAD'), migration['id'])
            self.assertTrue((ROOT / migration['reference']).is_file(), migration['id'])

    def test_unknown_layout_is_uncertain_not_silently_clear(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write_cargo_toml(root, ROOT)
            write_identity(root)
            migrations = self.by_id(upgrade.plan(root, ROOT, 'HEAD'))
        for migration in migrations.values():
            self.assertEqual(migration['status'], 'uncertain')

    def test_missing_baseline_makes_every_migration_uncertain(self):
        fixture = CustomSimFixture(engine_revision=None)
        self.addCleanup(fixture.close)
        packet = upgrade.plan(fixture.root, ROOT, 'HEAD')
        self.assertIsNone(packet['last_verified_engine_revision'])
        for migration in packet['migrations']:
            self.assertEqual(migration['status'], 'uncertain')
        self.assertTrue(any('No recoverable' in w for w in packet['warnings']))


class ProvenanceAndTemplateDriftTests(unittest.TestCase):
    def test_explicit_provenance_is_preferred_over_inference(self):
        fixture = CustomSimFixture()
        self.addCleanup(fixture.close)
        write_identity(fixture.root, extra={'provenance': {'template': 'custom-sim', 'template_revision': 'abc123'}})
        packet = upgrade.plan(fixture.root, ROOT, 'HEAD')
        self.assertEqual(packet['template_provenance']['source'], 'identity.json:provenance')
        self.assertEqual(packet['template_provenance']['template_revision'], 'abc123')

    def test_provenance_is_inferred_then_unknown(self):
        fixture = CustomSimFixture()
        self.addCleanup(fixture.close)
        packet = upgrade.plan(fixture.root, ROOT, 'HEAD')
        self.assertEqual(packet['template_provenance']['source'], 'inferred')
        write_identity(fixture.root, engine_revision=None)
        packet = upgrade.plan(fixture.root, ROOT, 'HEAD')
        self.assertEqual(packet['template_provenance']['source'], 'none')

    def test_check_script_drift_is_detected(self):
        fixture = CustomSimFixture()
        self.addCleanup(fixture.close)
        packet = upgrade.plan(fixture.root, ROOT, 'HEAD')
        self.assertTrue(packet['check_script']['matches_current_template'])
        (fixture.root / 'scripts/check.py').write_text(TEMPLATE_CHECK + '\n# customized\n', encoding='utf-8')
        packet = upgrade.plan(fixture.root, ROOT, 'HEAD')
        self.assertFalse(packet['check_script']['matches_current_template'])

    def test_missing_check_script_is_reported_not_assumed(self):
        fixture = CustomSimFixture(check_script=None)
        self.addCleanup(fixture.close)
        packet = upgrade.plan(fixture.root, ROOT, 'HEAD')
        self.assertFalse(packet['check_script']['present'])


class DirtyAndMismatchedEvidenceTests(unittest.TestCase):
    def test_dirty_target_checkout_is_flagged_not_hidden(self):
        with tempfile.TemporaryDirectory() as directory:
            engine = Path(directory) / 'engine'
            engine.mkdir()
            init_repo(engine)
            (engine / 'templates').mkdir()
            (engine / 'templates/game_check.py').write_text(TEMPLATE_CHECK, encoding='utf-8')
            (engine / 'tools').mkdir()
            shutil.copy(ROOT / 'tools/upgrade_migrations.json', engine / 'tools/upgrade_migrations.json')
            (engine / 'tracked.txt').write_text('v1\n')
            commit_all(engine, 'base')
            (engine / 'tracked.txt').write_text('v2 uncommitted\n')
            fixture = CustomSimFixture()
            self.addCleanup(fixture.close)
            packet = upgrade.plan(fixture.root, engine, 'HEAD', engine_checkout=engine)
            self.assertTrue(packet['target']['checkout_dirty'])
            self.assertTrue(any('uncommitted changes' in w for w in packet['warnings']))

    def test_floating_branch_dependency_is_not_called_a_pin(self):
        fixture = CustomSimFixture()
        self.addCleanup(fixture.close)
        packet = upgrade.plan(fixture.root, ROOT, 'HEAD')
        dependency = packet['engine_dependency']
        self.assertIn(dependency['pin_quality'], ('floating_branch', 'pinned_detached'))
        if dependency['pin_quality'] == 'floating_branch':
            self.assertIn('not a version pin', dependency['note'])

    def test_tool_binary_outside_resolved_dependency_is_flagged(self):
        fixture = CustomSimFixture()
        self.addCleanup(fixture.close)
        with patch.dict(os.environ, {'BE2_TOOLS': str(Path(tempfile.gettempdir()) / 'elsewhere/be2-tools')}):
            packet = upgrade.plan(fixture.root, ROOT, 'HEAD')
        self.assertTrue(any('not under the resolved engine dependency' in w for w in packet['warnings']))


class RequestedFixesAndAdoptionTests(unittest.TestCase):
    def test_fix_tasks_are_tracked_and_adoption_is_cross_checked(self):
        fixture = CustomSimFixture(extra_src={'wire.rs': '// codec\n', 'server.rs': '// server\n',
                                              'client.rs': '// client\n'})
        self.addCleanup(fixture.close)
        packet = upgrade.plan(fixture.root, ROOT, 'HEAD', fixes=['TASK-1:restore glow effect'],
                              adopt=['MIG-0022-NETPLAY-ADOPTION'])
        self.assertEqual(packet['requested_fixes'][0]['id'], 'TASK-1')
        migrations = {m['id']: m for m in packet['migrations']}
        self.assertTrue(migrations['MIG-0022-NETPLAY-ADOPTION'].get('adopt_requested'))

    def test_adopt_unknown_id_warns_instead_of_crashing(self):
        fixture = CustomSimFixture()
        self.addCleanup(fixture.close)
        packet = upgrade.plan(fixture.root, ROOT, 'HEAD', adopt=['NOT-A-REAL-ID'])
        self.assertTrue(any('no such migration id' in w for w in packet['warnings']))

    def test_malformed_fix_argument_is_rejected(self):
        fixture = CustomSimFixture()
        self.addCleanup(fixture.close)
        with self.assertRaises(upgrade.UpgradeError):
            upgrade.plan(fixture.root, ROOT, 'HEAD', fixes=['no-colon-here'])


FAKE_CHECK_OK = """
import json, sys
def find_tools(root):
    return sys.executable
def engine_path(root):
    return None
if __name__ == '__main__':
    print(json.dumps({"ok": True, "scope": "project", "ship": "skipped: no desktop", "report": "missing-report.json"}))
    sys.exit(0)
"""

FAKE_CHECK_FAIL = """
import json, sys
def find_tools(root):
    return sys.executable
def engine_path(root):
    return None
if __name__ == '__main__':
    print(json.dumps({"ok": False, "scope": "project", "ship": "not run", "report": "missing-report.json",
                      "error": "fixture-induced failure"}))
    sys.exit(1)
"""


class VerificationIsTruthfulTests(unittest.TestCase):
    def test_2d_upgrade_uses_fresh_browser_gate_not_a_native_only_success(self):
        fixture=CustomSimFixture(check_script=FAKE_CHECK_OK)
        self.addCleanup(fixture.close)
        (fixture.root/'game.project.json').write_text(json.dumps({'presentation':'2d','networking':'offline','targets':['web']}))
        script=fixture.root/'scripts/web.py'
        script.write_text("import json\nprint(json.dumps({'ok':True,'browser':True}))\n")
        result=upgrade.verify(fixture.root,ROOT)
        self.assertTrue(result['ok']);self.assertEqual(result['command'][1],str(script))
        self.assertIn('web',result['verification_target'])
        script.write_text("import json,sys\nprint(json.dumps({'ok':False,'browser':False}))\nsys.exit(1)\n")
        self.assertFalse(upgrade.verify(fixture.root,ROOT)['ok'])
        self.assertFalse(upgrade.verify(fixture.root,ROOT,content_only=True)['ok'])

    def test_unavailable_when_no_check_script(self):
        fixture = CustomSimFixture(check_script=None)
        self.addCleanup(fixture.close)
        result = upgrade.verify(fixture.root, ROOT)
        self.assertFalse(result['available'])
        self.assertFalse(result['ok'])

    def test_fresh_success_is_reported(self):
        fixture = CustomSimFixture(check_script=FAKE_CHECK_OK)
        self.addCleanup(fixture.close)
        result = upgrade.verify(fixture.root, ROOT)
        self.assertTrue(result['ok'])
        self.assertEqual(result['returncode'], 0)
        self.assertNotIn('game_check_detail', result, 'a missing report path must not be guessed at')

    def test_fresh_failure_cannot_be_hidden_by_a_stale_passing_report(self):
        fixture = CustomSimFixture(check_script=FAKE_CHECK_OK)
        self.addCleanup(fixture.close)
        stale = fixture.root / '.blue-check'
        stale.mkdir()
        (stale / 'report.json').write_text(json.dumps({'ok': True, 'stale': True}), encoding='utf-8')
        (fixture.root / 'scripts/check.py').write_text(FAKE_CHECK_FAIL, encoding='utf-8')
        result = upgrade.verify(fixture.root, ROOT)
        self.assertFalse(result['ok'])
        self.assertEqual(result['returncode'], 1)

    def test_missing_tool_binary_is_unavailable_not_a_false_pass(self):
        fake = (
            "import json, sys\n"
            "def find_tools(root):\n    return None\n"
            "def engine_path(root):\n    return None\n"
            "if __name__ == '__main__':\n    print(json.dumps({'ok': True}))\n"
        )
        fixture = CustomSimFixture(check_script=fake)
        self.addCleanup(fixture.close)
        result = upgrade.verify(fixture.root, ROOT)
        self.assertFalse(result['ok'])
        self.assertIn('No be2-tools binary found', result['reason'])


class CliSmokeTest(unittest.TestCase):
    def test_plan_subcommand_prints_summary_and_honors_out(self):
        fixture = CustomSimFixture()
        self.addCleanup(fixture.close)
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / 'plan.json'
            result = subprocess.run([sys.executable, str(ROOT / 'tools/be2.py'), 'upgrade', 'plan',
                                     str(fixture.root), '--to', 'HEAD', '--out', str(out)],
                                    cwd=ROOT, capture_output=True, text=True, timeout=60)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('Upgrade plan:', result.stdout)
            self.assertTrue(out.is_file())
            json.loads(out.read_text(encoding='utf-8'))


if __name__ == '__main__':
    unittest.main()
