"""Load-driver safety tests use disposable fixture processes, never live games."""
import argparse
import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import dev_tools
import server_load


class LoadTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.state = self.root / "state"
        self.stack = contextlib.ExitStack()
        self.addCleanup(self.stack.close)
        self.stack.enter_context(patch.object(dev_tools, "STATE", self.state))

    def script(self, name, source):
        path = self.root / name
        path.write_text("#!/usr/bin/python3\n" + source)
        path.chmod(0o700)
        return path

    def fixtures(self, bot_output=None):
        match = {"report": {"race_seconds": 1}, "net": {
            "server": {"ticks": 60, "tick_us_mean": 100, "tick_us_max": 200, "bad_datagrams": 0},
            "peers": [{"inputs_skipped": 0, "ticks_repeated": 0, "left_early": False,
                       "bytes_out": 2048, "bytes_in": 1024}]}}
        server = self.script("fixture-server", """import json, os, sys, time
from pathlib import Path
if '--info' in sys.argv:
 print('game=spooky-kart\\nbuild=00000001');raise SystemExit(0)
path=Path(sys.argv[sys.argv.index('--report-dir')+1])
(path/'pid').write_text(str(os.getpid()))
(path/'matches.jsonl').write_text(%r+'\\n')
print('STATUS game=spooky-kart players=1 max=8 stage=match build=00000001',flush=True)
while True:time.sleep(0.1)
""" % json.dumps(match))
        bot = bot_output or json.dumps({"clients": 1, "races_completed": 1,
            "per_client": [{"corrections": 0, "snaps": 0, "max_error_m": 0}]})
        bots = self.script("fixture-bots", "print(%r)\n" % bot)
        return server, bots

    def args(self, server, bots, **values):
        return argparse.Namespace(**(dict(server=str(server), bots=str(bots), label="baseline",
            rooms=[1, 2], clients=1, races=1, seconds=2, min_free_gib=0,
            memory_reserve_gib=0, run=True) | values))

    def assert_no_servers(self):
        for path in self.state.glob("server-load-*/rooms-*/room-*/pid"):
            with self.assertRaises(ProcessLookupError):
                os.kill(int(path.read_text()), 0)

    def test_plan_launches_no_rooms_and_creates_no_state(self):
        server, bots = self.fixtures()
        with contextlib.redirect_stdout(io.StringIO()), patch.object(server_load, "run_level") as level:
            self.assertEqual(server_load.run(self.args(server, bots, run=False)), 0)
            level.assert_not_called()
        self.assertFalse(self.state.exists())

    def test_concurrent_fixture_rooms_are_reported_and_reaped(self):
        server, bots = self.fixtures()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(server_load.run(self.args(server, bots)), 0)
        report = json.loads((self.state / "server-load-benchmark.json").read_text())
        self.assertTrue(report["ok"])
        self.assertEqual([len(r["room_results"]) for r in report["levels"]], [1, 2])
        self.assertEqual((self.state / "server-load-benchmark.json").stat().st_mode & 0o777, 0o600)
        for path in self.state.glob("server-load-*/rooms-*/room-*/*"):
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        self.assert_no_servers()

    def test_incomplete_race_fails_and_stops_servers(self):
        server, bots = self.fixtures(json.dumps({"clients": 1, "races_completed": 0}))
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(server_load.run(self.args(server, bots)), 2)
        report = json.loads((self.state / "server-load-benchmark.json").read_text())
        self.assertEqual(len(report["levels"]), 1)
        self.assertIn("did not complete", report["levels"][0]["error"])
        self.assert_no_servers()

    def test_memory_reserve_refuses_launch(self):
        server, bots = self.fixtures()
        with patch.object(server_load, "memory_available", return_value=0), patch.object(server_load, "run_level") as level:
            with self.assertRaisesRegex(ValueError, "reserve"):
                server_load.run(self.args(server, bots, memory_reserve_gib=2))
            level.assert_not_called()

    def test_invalid_driver_json_fails_and_cleans_up(self):
        server, bots = self.fixtures("not-json")
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(server_load.run(self.args(server, bots)), 2)
        self.assert_no_servers()

    def test_ports_are_loopback_below_client_ephemeral_range(self):
        held = server_load.reserve_ports(3)
        try:
            self.assertEqual(len(held), 3)
            self.assertEqual(len({s.getsockname()[1] for s in held}), 3)
            self.assertTrue(all(s.getsockname()[0] == "127.0.0.1" and s.getsockname()[1] < 32768 for s in held))
        finally:
            for sock in held:
                sock.close()

    def test_stop_process_reaps_its_own_session(self):
        process = subprocess.Popen(["/usr/bin/python3", "-c", "import time; time.sleep(60)"], start_new_session=True)
        try:
            server_load.stop_process(process)
            self.assertIsNotNone(process.returncode)
            with self.assertRaises(ProcessLookupError):
                os.kill(process.pid, 0)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()


if __name__ == "__main__":
    unittest.main()
