"""Shared process budgets, evidence and replay; each game tests its own rules in Rust."""
import json
import contextlib
import io
from pathlib import Path
import sys
import tempfile
import time
import unittest
from tools import playtest

@unittest.skipUnless(sys.platform == 'linux', 'Linux address-space budget')
class BudgetTests(unittest.TestCase):
    def adapter(self, root):
        binary = root / 'adapter'
        binary.write_text('''#!/usr/bin/env python3
import json, sys, time
if sys.argv[1] == 'playtest':
    seed, ticks = map(int, sys.argv[2:4])
    time.sleep((3-seed%4)*.005)
    trace = {'seed':seed, 'turns':[seed, ticks]}
else:
    trace = json.load(open(sys.argv[2]))
    seed, ticks = trace['turns']
print(json.dumps({'ok':True, 'ticks':ticks, 'hash':str(seed), 'failure':None,
                  'trace':trace, 'checkpoints':[{'tick':4, 'original_hex':'424532', 'migrated_hex':'53415645'}]}))
''')
        binary.chmod(0o700)
        return binary

    def test_timeout_keeps_exact_seed_command_and_enforces_budget(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / 'slow'
            binary.write_text('#!/usr/bin/env python3\nimport time\ntime.sleep(10)\n')
            binary.chmod(0o700)
            row = playtest.run_one(binary, 42, 1000, 64, .1, time.monotonic()+10, root)
            self.assertFalse(row['ok'])
            self.assertIn('time budget', row['failure'])
            self.assertIn('42 1000', json.loads((root/'42/run.json').read_text())['run_command'])

    def test_memory_budget_is_applied_to_the_actual_child(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            binary=root/'large'
            binary.write_text('#!/usr/bin/env python3\na=bytearray(256*1024*1024)\n')
            binary.chmod(0o700)
            row=playtest.run_one(binary,7,1000,32,5,time.monotonic()+10,root)
            self.assertFalse(row['ok'])
            self.assertNotEqual(row['returncode'],0)
            self.assertTrue((root/'7/stderr.log').is_file())

    def test_expired_global_budget_never_launches_a_process(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            row=playtest.run_one(root/'missing',0,22,64,10,time.monotonic()-1,root)
            self.assertIn('total time budget',row['failure'])
            self.assertTrue((root/'0/run.json').is_file())

    def test_parallel_and_serial_compare_actions_and_keep_original_checkpoint_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = self.adapter(root)
            serial, parallel = root/'serial', root/'parallel'
            serial.mkdir()
            parallel.mkdir()
            left = playtest.run_batch(binary, [3, 0, 2, 1], 22, 64, 5, time.monotonic()+10, serial, 1)
            right = playtest.run_batch(binary, [0, 1, 2, 3], 22, 64, 5, time.monotonic()+10, parallel, 4)
            self.assertTrue(all(row['ok'] for row in left+right))
            self.assertEqual([playtest.gameplay_signature(row) for row in left],
                             [playtest.gameplay_signature(row) for row in right])
            self.assertEqual((parallel/'0/checkpoints/0/original.be2save').read_bytes(), b'BE2')
            self.assertEqual((parallel/'0/checkpoints/0/migrated.be2save').read_bytes(), b'SAVE')
            self.assertIn('original_hex', json.loads((parallel/'0/result.json').read_text())['checkpoints'][0])
            changed = {**right[0], 'trace_sha256':'different actions'}
            self.assertNotEqual(playtest.gameplay_signature(right[0]), playtest.gameplay_signature(changed))

    def test_replay_uses_same_budget_boundary_and_preserves_input(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = self.adapter(root)
            trace = root/'trace.json'
            playtest.write(trace, {'seed':42, 'turns':[42, 22]})
            output = root/'evidence'
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(playtest.main(['--binary', str(binary), '--replay-trace', str(trace),
                                                '--memory-mib', '64', '--out', str(output)]), 0)
            self.assertEqual((output/'replay/input-trace.json').read_bytes(), trace.read_bytes())
            result = json.loads((output/'report.json').read_text())
            self.assertTrue(result['ok'])
            self.assertEqual(result['runs'][0]['hash'], '42')
            self.assertIn('replay-playtest', result['runs'][0]['run_command'])

    def test_comparison_cli_retains_both_runs_and_checks_equivalence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = self.adapter(root)
            output = root/'comparison'
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(playtest.main(['--binary', str(binary), '--seeds', '0,1', '--ticks', '22',
                                                '--compare-serial', '--memory-mib', '64', '--out', str(output)]), 0)
            report = json.loads((output/'report.json').read_text())
            self.assertTrue(report['serial_parallel_equal'])
            self.assertTrue((output/'0/trace.json').is_file())
            self.assertTrue((output/'serial/0/trace.json').is_file())
            self.assertEqual(len(json.loads((output/'comparison.json').read_text())['serial_runs']), 2)

    def test_failure_keeps_trace_and_completed_checkpoint_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = self.adapter(root)
            binary.write_text(binary.read_text().replace("'ok':True", "'ok':False"))
            row = playtest.run_one(binary, 7, 22, 64, 5, time.monotonic()+10, root)
            self.assertFalse(row['ok'])
            self.assertTrue((root/'7/trace.json').is_file())
            self.assertEqual((root/'7/checkpoints/0/original.be2save').read_bytes(), b'BE2')
