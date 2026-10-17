"""Idle conditions read scratch locks and never acquire, remove, or rewrite them."""
import contextlib
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import ideaforge_idle as idle


class IdeaForgeIdleTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.home = Path(self.temp.name)
        self.root = self.home / "custom-daily"
        self.root.mkdir()
        self.config = self.home / "daily.json"
        self.config.write_text(json.dumps({"daily_root": str(self.root)}))
        self.lock = self.root / "supervisor.lock"

    def write_lock(self, contents):
        self.lock.write_text(contents)
        self.lock.chmod(0o600)

    def test_missing_lock_allows_normal_runner_without_creating_it(self):
        with patch.object(os, "kill") as kill:
            self.assertEqual(idle.idle_condition(self.config, self.home), 0)
            kill.assert_not_called()
        self.assertFalse(self.lock.exists())

    def test_busy_supervisor_skips_without_changing_lock(self):
        self.write_lock("12345")
        for outcome in (None, PermissionError()):
            with patch.object(os, "kill", side_effect=outcome) as kill:
                self.assertEqual(idle.idle_condition(self.config, self.home), 1)
                kill.assert_called_once_with(12345, 0)
        self.assertEqual(self.lock.read_text(), "12345")

    def test_stale_lock_is_left_for_normal_runner_to_recover(self):
        self.write_lock("12345")
        with patch.object(os, "kill", side_effect=ProcessLookupError()):
            self.assertEqual(idle.idle_condition(self.config, self.home), 0)
        self.assertEqual(self.lock.read_text(), "12345")

    def test_malformed_lock_is_error_without_signaling_any_process(self):
        for contents in ("", "0", "-1", "123\njunk", "9999999999", "x" * 65):
            self.write_lock(contents)
            with self.subTest(contents=contents), patch.object(os, "kill") as kill, contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(idle.main(["--config", str(self.config)]), 255)
                kill.assert_not_called()
            self.assertEqual(self.lock.read_text(), contents)

    def test_unsafe_symlink_and_writable_lock_are_errors(self):
        elsewhere = self.home / "source"
        elsewhere.write_text("12345")
        self.lock.symlink_to(elsewhere)
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(idle.main(["--config", str(self.config)]), 255)
        self.assertEqual(elsewhere.read_text(), "12345")
        self.lock.unlink()
        self.lock.write_text("12345")
        self.lock.chmod(0o666)
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(idle.main(["--config", str(self.config)]), 255)

    def test_missing_daily_root_uses_configured_engine(self):
        engine = self.home / "engine"
        self.config.write_text(json.dumps({"engine_root": str(engine), "daily_root": None}))
        self.assertEqual(idle.daily_root(self.config, self.home), engine / ".be2-work/idea-forge/daily")
        self.assertEqual(idle.idle_condition(self.config, self.home), 0)
        self.assertFalse(engine.exists())


if __name__ == "__main__":
    unittest.main()
