"""Process budgets and order independence; gameplay is tested against Workshop in Rust."""
import json
from pathlib import Path
import sys
import tempfile
import time
import unittest
from tools import playtest

@unittest.skipUnless(sys.platform == 'linux', 'Linux address-space budget')
class BudgetTests(unittest.TestCase):
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
