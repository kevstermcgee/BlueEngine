"""Native review keeps physical attestations separate from content/capture evidence."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


ship = load('review_test_ship', ROOT / 'templates/game_ship.py')
review = load('game_review', ROOT / 'templates/game_review.py')


class ReviewTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)
        self.engine = self.base / 'engine'
        (self.engine / 'src').mkdir(parents=True)
        (self.engine / 'assets').mkdir()
        (self.engine / 'templates').mkdir()
        (self.engine / 'Cargo.toml').write_text('[package]\nname="be2"\nversion="0.2.0"\n')
        (self.engine / 'src/lib.rs').write_text('// shared rendering-free rules\n')
        (self.engine / 'assets/font.bin').write_bytes(b'original-font')
        self.root = self.base / 'café 游戏'
        (self.root / 'src').mkdir(parents=True)
        (self.root / 'assets').mkdir()
        (self.root / 'dist/assets').mkdir(parents=True)
        (self.root / 'Cargo.toml').write_text('[package]\nname="review-fixture"\nversion="0.1.0"\n'
            '[dependencies]\nvesper3d={package="be2",path="../engine"}\n')
        (self.root / 'src/lib.rs').write_text('// game-owned rules, input and overhead camera\n')
        identity = {'title': 'Odd Controls', 'tagline': 'An original camera and sound',
                    'controls': 'Arrows turn; Space rings the bell', 'exe': 'review-fixture', 'package': ['assets']}
        (self.root / 'assets/identity.json').write_text(json.dumps(identity))
        (self.root / 'assets/icon.ico').write_bytes(b'fixture icon')
        (self.root / 'assets/icon.png').write_bytes(b'fixture png')
        self.spec = self.root / 'assets/native-review.json'
        self.spec.write_text(json.dumps({'schema_version': 1, 'checks': [
            {'id': 'turn', 'area': 'controls', 'instructions': 'Arrows rotate the player; release stops rotation.'},
            {'id': 'camera', 'area': 'camera', 'instructions': 'Overhead camera keeps the player in view.'},
            {'id': 'bell', 'area': 'audio', 'instructions': 'Press Space and hear the authored bell.'}]}))
        self.project = ship.Project(self.root, 'windows')
        for name in ('review-fixture.exe', 'review-fixture.ico', 'review-fixture.png'):
            (self.root / 'dist' / name).write_bytes(b'fixture bytes; not a physical-device result')
        for path in (self.root / 'assets').iterdir():
            shutil.copyfile(path, self.root / 'dist/assets' / path.name)
        names = sorted(p.relative_to(self.root / 'dist').as_posix() for p in (self.root / 'dist').rglob('*') if p.is_file())
        stamp = {'title': identity['title'], 'exe': 'review-fixture.exe', 'engine_revision': 'fixture-engine',
                 'exe_sha256': ship.sha256_file(self.root / 'dist/review-fixture.exe'), 'files': names,
                 'file_sha256': {n: ship.sha256_file(self.root / 'dist' / n) for n in names}}
        stamp['review_source_sha256'] = review.source_binding(self.project, ship)['sha256']
        self.project.write_stamp(stamp)
        self.clean_env = patch.dict(os.environ, {'LIBGL_ALWAYS_SOFTWARE': '0', 'GALLIUM_DRIVER': ''})
        self.clean_env.start()
        self.addCleanup(self.clean_env.stop)

    def prepare(self, **kwargs):
        return review.prepare(self.project, ship, **kwargs)

    def observations(self):
        receipt = json.loads((self.root / review.RECEIPT).read_text(encoding='utf-8'))
        observation = receipt['observation_template']
        observation.update(reviewer='Fixture reviewer', attested=True, context='Fresh package; overhead camera; authored bell',
                           launched_package=True, used_bound_settings=True)
        observation['environment'].update(mode='physical-manual', os_version='Windows fixture session',
            display='Fixture physical monitor', renderer='hardware', renderer_description='Fixture native GPU',
            input_devices=['Fixture physical keyboard'], audio_output='Fixture physical speakers', audio_output_kind='physical',
            physical_devices_attested=True)
        for check in observation['checks']:
            check.update(result='pass', observation='Explicit fixture observation of the authored expected outcome')
        return observation

    def record(self, observation, **kwargs):
        path = self.root / '.blue-check/observations.json'
        path.write_text(json.dumps(observation))
        with patch.object(ship, 'host_platform', return_value='windows'):
            return review.record(self.project, ship, path, **kwargs)

    def test_prepare_is_pending_and_does_not_choose_game_aesthetics(self):
        result = self.prepare()
        self.assertFalse(result['manual'])
        self.assertEqual(result['state'], 'pending')
        self.assertEqual(set(result['checks']), {'turn', 'camera', 'bell'})
        self.assertNotIn('crosshair', json.dumps(result))
        self.assertFalse(json.loads((self.root / review.RECEIPT).read_text(encoding='utf-8'))['observation_template']['attested'])

    def test_all_authored_physical_observations_can_pass_but_do_not_certify_other_gates(self):
        self.prepare()
        result = self.record(self.observations())
        self.assertTrue(result['manual'])
        self.assertEqual(result['state'], 'passed')
        self.assertIn('installer', ' '.join(result['limitations']))

    def test_partial_and_failed_observations_preserve_notes_and_can_be_completed(self):
        self.prepare()
        observation = self.observations()
        observation['checks'][1]['result'] = 'fail'
        observation['checks'][1]['observation'] = 'Camera crosses the authored wall'
        observation['notes'] = 'Keep the original overhead camera; repair the clipping.'
        result = self.record(observation)
        self.assertEqual(result['state'], 'failed')
        saved = json.loads((self.root / review.RECEIPT).read_text(encoding='utf-8'))
        self.assertEqual(saved['records'][0]['observations']['notes'], observation['notes'])
        self.assertTrue(self.record(self.observations())['manual'])
        self.assertEqual(len(json.loads((self.root / review.RECEIPT).read_text(encoding='utf-8'))['records']), 2)

    def test_empty_observation_or_missing_check_cannot_pass(self):
        self.prepare()
        observation = self.observations()
        observation['checks'][0]['observation'] = ' '
        result = self.record(observation)
        self.assertFalse(result['manual'])
        self.assertIn('nonempty observed outcome', ' '.join(result['diagnostics']))
        observation = self.observations()
        observation['checks'] = observation['checks'][:1]
        result = self.record(observation)
        self.assertFalse(result['manual'])
        self.assertIn('bell', result['pending'])
        self.assertEqual(len(json.loads((self.root / review.RECEIPT).read_text(encoding='utf-8'))['records']), 2)

    def test_partial_physical_checks_accumulate_and_software_failures_stay_visible(self):
        self.prepare()
        first = self.observations()
        first['checks'] = first['checks'][:1]
        self.assertFalse(self.record(first)['manual'])
        second = self.observations()
        second['checks'] = second['checks'][1:]
        self.assertTrue(self.record(second)['manual'])
        software = self.observations()
        software['environment']['mode'] = 'software-capture'
        software['checks'][1].update(result='fail', observation='Capture visibly clips the authored wall')
        self.assertEqual(self.record(software)['state'], 'failed')

    def test_software_unknown_and_scripted_evidence_never_promote_to_physical(self):
        self.prepare()
        for change in ({'mode': 'software-capture'}, {'renderer': 'unknown'}, {'scripted_input': True},
                       {'renderer_description': 'Mesa llvmpipe'}, {'renderer_description': 'Microsoft WARP'},
                       {'display': 'Xvfb :99'}, {'audio_output': 'ALSA null PCM'},
                       {'audio_output': 'PulseAudio Dummy Output'}, {'physical_devices_attested': False}):
            observation = self.observations()
            observation['environment'].update(change)
            with self.subTest(change=change):
                result = self.record(observation)
                self.assertFalse(result['manual'])
                self.assertEqual(result['state'], 'pending')
        with patch.dict(os.environ, {'LIBGL_ALWAYS_SOFTWARE': '1'}):
            self.assertFalse(self.record(self.observations())['manual'])
        self.assertTrue(self.record(self.observations())['manual'])
        self.assertGreater(len(json.loads((self.root / review.RECEIPT).read_text(encoding='utf-8'))['records']), 5)
        observation = self.observations()
        observation['environment']['audio_output'] = 'Fixture physical speakers with virtual surround'
        self.assertTrue(self.record(observation)['manual'])

    def test_wrong_actual_host_cannot_attest_windows_physical_review(self):
        self.prepare()
        path = self.root / '.blue-check/observations.json'
        path.write_text(json.dumps(self.observations()))
        with patch.object(ship, 'host_platform', return_value='linux'):
            result = review.record(self.project, ship, path)
        self.assertFalse(result['manual'])
        self.assertIn('actual recording host', ' '.join(result['diagnostics']))

    def test_source_engine_assets_and_package_changes_invalidate_the_current_receipt(self):
        self.prepare()
        self.assertTrue(self.record(self.observations())['manual'])
        for path in (self.root / 'src/lib.rs', self.engine / 'src/lib.rs', self.engine / 'assets/font.bin',
                     self.root / 'dist/review-fixture.exe'):
            before = path.read_bytes()
            path.write_bytes(before + b'changed')
            with self.subTest(path=path):
                result = review.status(self.project, ship)
                self.assertFalse(result['manual'])
                self.assertEqual(result['state'], 'stale')
            path.write_bytes(before)
        self.assertTrue(review.status(self.project, ship)['manual'])

    def test_changed_source_before_preparation_keeps_an_old_package_pending(self):
        (self.root / 'src/lib.rs').write_text('// genuinely changed authored input\n')
        result = self.prepare()
        self.assertFalse(result['manual'])
        self.assertIn('source-build binding', ' '.join(result['diagnostics']))
        self.assertFalse(self.record(self.observations())['manual'])

    def test_settings_and_capture_hashes_remain_bound_without_copying_private_settings(self):
        settings = self.root / 'dist/settings.json'
        settings.write_text('{"music":true,"camera":"overhead","private":"do not copy"}')
        capture = self.base / 'capture.png'
        capture.write_bytes(b'fixture software screenshot, not physical evidence')
        self.prepare(settings=settings, artifacts=[capture])
        self.assertTrue(self.record(self.observations())['manual'])
        receipt = (self.root / review.RECEIPT).read_text(encoding='utf-8')
        self.assertNotIn('do not copy', receipt)
        for path in (settings, capture):
            before = path.read_bytes()
            path.write_bytes(before + b'changed')
            self.assertEqual(review.status(self.project, ship)['state'], 'stale')
            path.write_bytes(before)

    def test_outputs_and_exact_copies_do_not_change_source_binding(self):
        before = review.source_binding(self.project, ship)
        for folder in ('target', 'dist', 'installers', '.blue-check'):
            (self.root / folder).mkdir(exist_ok=True)
            (self.root / folder / 'new-output').write_bytes(b'output/player data')
        self.assertEqual(before, review.source_binding(self.project, ship))
        copied = self.base / 'copied game'
        shutil.copytree(self.root, copied)
        self.assertEqual(before, review.source_binding(ship.Project(copied, 'windows'), ship))

    def test_receipt_locations_overlap_and_repeat_prepare_preserve_outputs(self):
        with self.assertRaisesRegex(review.ReviewError, 'under .blue-check'):
            self.prepare(receipt='src/lib.rs')
        original = (self.root / 'src/lib.rs').read_bytes()
        self.prepare()
        saved = (self.root / review.RECEIPT).read_bytes()
        with self.assertRaisesRegex(review.ReviewError, 'already exists'):
            self.prepare()
        self.assertEqual(saved, (self.root / review.RECEIPT).read_bytes())
        self.assertEqual(original, (self.root / 'src/lib.rs').read_bytes())
        with self.assertRaisesRegex(review.ReviewError, 'separate'):
            self.prepare(receipt='.blue-check/second.json', settings=self.spec)
        with self.assertRaisesRegex(review.ReviewError, 'separate'):
            review.record(self.project, ship, self.root / review.RECEIPT)
        with self.assertRaisesRegex(review.ReviewError, 'engine source'):
            self.prepare(receipt=self.engine / 'src/review.json')

    @unittest.skipIf(os.name == 'nt', 'POSIX fixture creates a symlink without Windows privilege')
    def test_symlink_traversal_in_evidence_is_rejected(self):
        settings = self.base / 'settings.json'
        settings.write_text('{}')
        link = self.root / '.blue-check'
        link.symlink_to(self.base, target_is_directory=True)
        with self.assertRaisesRegex(review.ReviewError, 'symlink'):
            self.prepare(settings=link / 'settings.json')
        self.assertFalse((self.base / 'native-review.json').exists())

    def test_altered_receipt_cannot_drop_an_authored_check(self):
        self.prepare()
        self.record(self.observations())
        path = self.root / review.RECEIPT
        value = json.loads(path.read_text(encoding='utf-8'))
        value['checks'] = value['checks'][:1]
        path.write_text(json.dumps(value))
        self.assertFalse(review.status(self.project, ship)['manual'])
        self.assertEqual(review.status(self.project, ship)['state'], 'stale')

    def assert_invalid_receipt_preserved(self, value, diagnostic):
        path = self.root / review.RECEIPT
        # Deliberate formatting verifies rejection preserves the exact input bytes.
        path.write_text(json.dumps(value, indent=3) + '\n')
        before = path.read_bytes()
        observations = self.root / '.blue-check/rejected-observations.json'
        observations.write_text(json.dumps({'schema_version': 1, 'notes': 'Retain these notes'}))
        for action in (lambda: review.status(self.project, ship),
                       lambda: review.record(self.project, ship, observations)):
            with self.assertRaisesRegex(review.ReviewError, diagnostic):
                action()
            self.assertEqual(path.read_bytes(), before)

    def test_nonobject_records_and_observations_are_actionable_and_preserved(self):
        self.prepare()
        saved = json.loads((self.root / review.RECEIPT).read_text(encoding='utf-8'))
        for records in ([0], [{'observations': 42}], [{'observations': []}]):
            with self.subTest(records=records):
                self.assert_invalid_receipt_preserved({**saved, 'records': records}, 'invalid receipt record 1')

    def test_malformed_declared_checks_are_actionable_and_preserved(self):
        self.prepare()
        saved = json.loads((self.root / review.RECEIPT).read_text(encoding='utf-8'))
        first = saved['checks'][0]
        for checks in ([None], [first, first], [{**first, 'id': 'Not stable!'}],
                       [{**first, 'area': []}], [{**first, 'area': 'unknown'}],
                       [{**first, 'instructions': ' '}], []):
            with self.subTest(checks=checks):
                self.assert_invalid_receipt_preserved({**saved, 'checks': checks}, 'invalid receipt checklist')

    def test_malformed_observed_environments_are_actionable_and_preserved(self):
        self.prepare()
        self.record(self.observations())
        saved = json.loads((self.root / review.RECEIPT).read_text(encoding='utf-8'))
        for environment in (42, None, [], 'Windows'):
            value = copy.deepcopy(saved)
            value['records'][0]['observed_environment'] = environment
            with self.subTest(environment=environment):
                self.assert_invalid_receipt_preserved(value, 'observed_environment must be JSON objects')
        value = copy.deepcopy(saved)
        del value['records'][0]['observed_environment']
        self.assert_invalid_receipt_preserved(value, 'observed_environment must be JSON objects')

    def packaged(self, *, no_build=False, during_build=None):
        built = self.base / 'build/review-fixture.exe'
        built.parent.mkdir(exist_ok=True)
        if not built.exists():
            shutil.copyfile(self.project.dist_exe, built)
        def build(project):
            if during_build:
                during_build()
            return built
        with patch.object(ship, 'cargo_build_release', side_effect=build), \
                patch.object(ship, 'locate_built_exe', return_value=built):
            return ship.cmd_package(self.project, no_build=no_build)

    def test_package_build_records_stable_source_and_no_build_retains_only_that_exe(self):
        result = self.packaged()
        self.assertTrue(result['review_source_bound'])
        self.assertEqual(self.project.read_stamp()['review_source_sha256'], review.source_binding(self.project, ship)['sha256'])
        self.assertTrue(self.packaged(no_build=True)['review_source_bound'])
        (self.base / 'build/review-fixture.exe').write_bytes(b'different release executable')
        self.assertFalse(self.packaged(no_build=True)['review_source_bound'])
        self.assertNotIn('review_source_sha256', self.project.read_stamp())

    def test_source_edit_during_build_preserves_package_outputs_without_provenance(self):
        def edit():
            (self.root / 'src/lib.rs').write_text('// an actual edit while the build runs\n')
        with self.assertRaisesRegex(ship.ShipError, 'changed during the build'):
            self.packaged(during_build=edit)
        self.assertTrue(self.project.dist_exe.is_file())
        self.assertNotIn('review_source_sha256', self.project.read_stamp())

    def test_prebuilt_without_source_proof_and_no_engine_packages_remain_uncertified(self):
        stamp = self.project.read_stamp()
        stamp.pop('review_source_sha256')
        self.project.write_stamp(stamp)
        self.assertFalse(self.packaged(no_build=True)['review_source_bound'])
        with patch.object(self.project, 'engine', return_value=None):
            result = self.packaged()
        self.assertTrue(result['ok'])
        self.assertFalse(result['review_source_bound'])
        self.assertIn('provenance is unavailable', ' '.join(result['warnings']))

    def test_known_engine_scope_covers_actual_literal_embedded_rust_inputs(self):
        import re
        roots = [Path(path) for path in review.ENGINE_INPUTS]
        for source in (ROOT / 'src').rglob('*.rs'):
            for match in re.finditer(r'include_(?:str|bytes)!\s*\(\s*"([^"]+)"', source.read_text(encoding='utf-8')):
                embedded = (source.parent / match[1]).resolve().relative_to(ROOT)
                self.assertTrue(any(embedded == known or known in embedded.parents for known in roots),
                                f'{source}: unbound embedded input {embedded}')


if __name__ == '__main__':
    unittest.main()
