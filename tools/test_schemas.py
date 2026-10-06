"""Schema coordinator safety; compilation and schema semantics are Rust-tested."""
import contextlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from tools import schemas


class SchemaToolTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for name in schemas.FILES.values():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b'{"previous":true}\n')
            path.chmod(0o644)
        self.calls = []
        self.binary = str(self.root / 'custom-target' / 'be2-tools')

    def run_command(self, argv, **kwargs):
        self.calls.append(argv)
        if argv[0] == 'cargo':
            artifact = {'reason': 'compiler-artifact', 'target': {'name': 'be2-tools', 'kind': ['bin']},
                        'executable': self.binary}
            return subprocess.CompletedProcess(argv, 0, json.dumps(artifact) + '\n', '')
        self.assertEqual(argv[0], self.binary)
        return subprocess.CompletedProcess(argv, 0, '{\n  "title": "' + argv[-1] + '"\n}\n', '')

    def invoke(self, args, *, identities=None, runner=None):
        output = io.StringIO()
        with patch.object(schemas, 'ROOT', self.root), \
                patch.object(schemas, 'engine_identity', side_effect=identities or [{'sha256': 'same'}, {'sha256': 'same'}]), \
                patch.object(schemas.subprocess, 'run', side_effect=runner or self.run_command), \
                contextlib.redirect_stdout(output):
            code = schemas.main(args)
        return code, json.loads(output.getvalue())

    def previous_outputs(self):
        return {name: (self.root / name).read_bytes() for name in schemas.FILES.values()}

    def test_plan_does_not_probe_install_or_build(self):
        code, packet = self.invoke(['--write', '--plan'], identities=AssertionError('probe forbidden'),
                                   runner=AssertionError('execution forbidden'))
        self.assertEqual(code, 0)
        self.assertEqual(packet['status'], 'planned')
        self.assertEqual(packet['builds_triggered'], 0)

    def test_cargo_artifact_selects_exact_executable_and_preserves_canonical_bytes(self):
        code, packet = self.invoke(['--write'])
        self.assertEqual(code, 0)
        self.assertEqual(packet['status'], 'passed')
        self.assertEqual(len(self.calls), 5)
        self.assertEqual((self.root / schemas.FILES['game']).read_bytes(), b'{\n  "title": "game"\n}\n')
        self.assertEqual(len(packet['changed']), 4)

    def test_unchanged_generation_keeps_mtime_and_permission(self):
        self.invoke(['--write'])
        before = {name: (self.root / name).stat().st_mtime_ns for name in schemas.FILES.values()}
        code, packet = self.invoke(['--write'])
        self.assertEqual(code, 0)
        self.assertEqual(packet['changed'], [])
        self.assertEqual(len(packet['unchanged']), 4)
        self.assertEqual(before, {name: (self.root / name).stat().st_mtime_ns for name in schemas.FILES.values()})
        if schemas.os.name != 'nt':
            self.assertEqual((self.root / schemas.FILES['game']).stat().st_mode & 0o777, 0o644)

    def test_changed_inputs_reject_results_before_writing(self):
        before = self.previous_outputs()
        code, packet = self.invoke(['--write'], identities=[{'sha256':'before'}, {'sha256':'after'}])
        self.assertEqual(code, 1)
        self.assertEqual(packet['status'], 'failed')
        self.assertIn('changed', packet['error'])
        self.assertEqual(before, self.previous_outputs())

    def test_source_inventory_failure_blocks_build(self):
        code, packet = self.invoke(['--write'], identities=[{'sha256':None,'error':'Git inventory unavailable'}])
        self.assertEqual(code, 1)
        self.assertEqual(self.calls, [])
        self.assertIn('Git', packet['error'])

    def test_last_generation_failure_preserves_all_previous_outputs(self):
        before = self.previous_outputs()
        def fail(argv, **kwargs):
            if argv[-1] == 'mcp':
                return subprocess.CompletedProcess(argv, 0, 'broken JSON', '')
            return self.run_command(argv, **kwargs)
        code, packet = self.invoke(['--write'], runner=fail)
        self.assertEqual(code, 1)
        self.assertEqual(packet['status'], 'failed')
        self.assertEqual(before, self.previous_outputs())

    def test_failed_check_remains_failed_with_diagnostics(self):
        code, packet = self.invoke(['--check'], runner=subprocess.CalledProcessError(1, ['cargo','test'], stderr='constraint fixture failed'))
        self.assertEqual(code, 1)
        self.assertEqual(packet['status'], 'failed')
        self.assertIn('fixture', packet['stderr'])

    def test_missing_artifact_never_runs_a_guessed_binary(self):
        def no_artifact(argv, **kwargs):
            self.calls.append(argv)
            return subprocess.CompletedProcess(argv, 0, '{"reason":"build-finished","success":true}\n', '')
        code, packet = self.invoke(['--write'], runner=no_artifact)
        self.assertEqual(code, 1)
        self.assertIn('native host', packet['error'])
        self.assertEqual(len(self.calls), 1)

    def test_check_uses_focused_fixture_suite(self):
        code, packet = self.invoke(['--check'])
        self.assertEqual(code, 0)
        self.assertEqual(packet['action'], 'check')
        self.assertEqual(len(self.calls), 1)
        self.assertIn('authoring_schemas', self.calls[0])


if __name__ == '__main__':
    unittest.main()
