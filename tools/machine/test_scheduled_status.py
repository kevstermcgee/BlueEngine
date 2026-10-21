"""Failure reports use scratch state and never consume feedback or start units."""
from datetime import datetime, timezone
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import dev_tools as tools
import scheduled_status as status


class ScheduledStatusTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.home = Path(self.temp.name)
        self.now = datetime(2026, 10, 17, 12, tzinfo=timezone.utc)
        self.failed = {"Result": "exit-code", "ActiveState": "failed", "SubState": "failed",
                       "ExecMainStatus": "0", "StateChangeTimestamp": f"@{int(self.now.timestamp())}"}
        self.reports = self.home / ".local/state/dev-tools"
        self.reports.mkdir(parents=True)

    def admission(self, workload="nightly", **changes):
        path = self.reports / f"storage-admission-{workload}.json"
        path.write_text(json.dumps({"time": self.now.isoformat(), "admitted": False,
                                   "reason": f"{workload} retained: need 30 GiB free on every build filesystem; lowest is 22.639 GiB",
                                   **changes}))

    def test_preflight_failure_replaces_latest_but_preserves_success_and_feedback(self):
        state = self.home / ".local/state/blueengine-nightly"
        state.mkdir()
        (state / "latest.md").write_text("Status: updated\nlast successful report\n")
        (state / "last-success.json").write_text('{"commit":"retained"}')
        (state / "feedback-seen.json").write_text('{"feedback":"retained"}')
        self.admission()
        path = status.record_failure("blueengine-nightly", self.failed, self.home, self.now)
        report = path.read_text()
        self.assertIn("Status: failed", report)
        self.assertIn("22.639 GiB free; 30 GiB required", report)
        self.assertIn(str(state / "last-success.json"), report)
        self.assertIn(str(state / "runs"), report)
        self.assertEqual((state / "last-success.json").read_text(), '{"commit":"retained"}')
        self.assertEqual((state / "feedback-seen.json").read_text(), '{"feedback":"retained"}')
        previous = list((state / "scheduled-status").glob("previous-*.md"))
        self.assertEqual(previous[0].read_text(), "Status: updated\nlast successful report\n")
        self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        self.assertEqual((state / "scheduled-status").stat().st_mode & 0o777, 0o700)

    def test_ideaforge_failure_has_local_status_and_retained_run_links(self):
        self.admission("ideaforge")
        path = status.record_failure("ideaforge-daily", self.failed, self.home, self.now)
        self.assertEqual(path, self.home / ".local/state/ideaforge/latest.md")
        report = path.read_text()
        self.assertIn("daily/batches", report)
        self.assertIn("idea-forge/runs", report)
        self.assertFalse((self.home / "BlueEngine-idea-forge-native").exists())

    def test_ideaforge_completion_replaces_failure_without_claiming_publication(self):
        config = self.home / ".config/ideaforge/daily.json"
        config.parent.mkdir(parents=True)
        config.write_text(json.dumps({"runs_root": "/srv/storage/artifacts/ideaforge/runs"}))
        self.admission("ideaforge")
        path = status.record_status("ideaforge-daily", self.failed, self.home, self.now)
        previous = path.read_text()
        path = status.record_status("ideaforge-daily", {**self.failed, "Result": "success", "ActiveState": "inactive", "SubState": "dead",
                                                      "InactiveExitTimestampMonotonic": "1000", "ExecMainExitTimestampMonotonic": "2000"}, self.home, self.now)
        report = path.read_text()
        self.assertIn("Status: completed", report)
        self.assertIn("Scheduled attempt completed. Publication details are recorded", report)
        self.assertNotIn("Storage admission refused", report)
        self.assertIn("/srv/storage/artifacts/ideaforge/runs", report)
        self.assertIn(str(self.home / "BlueEngine-idea-forge-native/.be2-work/idea-forge/runs"), report)
        self.assertTrue(any(p.read_text() == previous for p in (path.parent / "scheduled-status").glob("previous-*.md")))

    def test_skipped_ideaforge_condition_does_not_claim_completed_attempt(self):
        path = status.record_status("ideaforge-daily", self.failed, self.home, self.now)
        report = path.read_text()
        for last_exit in ("0", "1000"):
            self.assertIsNone(status.record_status("ideaforge-daily", {**self.failed, "Result": "success", "ActiveState": "inactive",
                                                                      "InactiveExitTimestampMonotonic": "2000", "ExecMainExitTimestampMonotonic": last_exit}, self.home, self.now))
        self.assertEqual(path.read_text(), report)

    def test_backoff_wakeup_reports_waiting_without_claiming_delivery(self):
        root = self.home / "BlueEngine-idea-forge-native/.be2-work/idea-forge/daily/batches/2026-10-17"
        root.mkdir(parents=True)
        (root / "state.json").write_text(json.dumps({"date": "2026-10-17", "status": "pending", "order": ["3d", "2d"],
            "slots": {"3d": {"status": "failed", "retry_after": "2026-10-17T20:00:00+00:00"}}}))
        service = {**self.failed, "Result": "success", "ActiveState": "inactive", "SubState": "dead",
                   "InactiveExitTimestampMonotonic": "1000", "ExecMainExitTimestampMonotonic": "2000"}
        report = status.record_status("ideaforge-daily", service, self.home, self.now).read_text()
        self.assertIn("Status: waiting", report)
        self.assertIn("2026-10-17T20:00:00+00:00", report)
        self.assertNotIn("Scheduled attempt completed", report)

    def test_observed_exec_condition_skip_preserves_status_but_condition_error_reports_failure(self):
        state = self.home / ".local/state/ideaforge"
        state.mkdir()
        latest = state / "latest.md"
        latest.write_text("Existing local status")
        skipped = {**self.failed, "Result": "exec-condition", "ActiveState": "inactive", "SubState": "dead"}
        for reporter in (status.record_status, status.record_failure):
            self.assertIsNone(reporter("ideaforge-daily", skipped, self.home, self.now))
        self.assertEqual(latest.read_text(), "Existing local status")
        self.assertFalse((state / "scheduled-status").exists())
        # An ExecCondition exit of 255 genuinely fails the service and has the
        # observed exit-code result, unlike an ordinary condition skip.
        path = status.record_status("ideaforge-daily", self.failed, self.home, self.now)
        self.assertIn("Status: failed", path.read_text())

    def test_nightly_success_callback_preserves_runner_report(self):
        state = self.home / ".local/state/blueengine-nightly"
        state.mkdir()
        latest = state / "latest.md"
        latest.write_text("Authoritative runner success report")
        config = self.home / ".config/blueengine-nightly/config.json"
        config.parent.mkdir(parents=True)
        config.write_text(json.dumps({"state": str(state), "runs_root": str(self.home / "external/runs")}))
        self.assertIsNone(status.record_status("blueengine-nightly", {**self.failed, "Result": "success", "ActiveState": "inactive"}, self.home, self.now))
        self.assertEqual(latest.read_text(), "Authoritative runner success report")

    def test_nightly_failure_links_external_future_runs_and_preserved_internal_history(self):
        config = self.home / ".config/blueengine-nightly/config.json"
        config.parent.mkdir(parents=True)
        state = self.home / ".local/state/blueengine-nightly"
        external = self.home / "external/runs"
        config.write_text(json.dumps({"state": str(state), "runs_root": str(external)}))
        path = status.record_status("blueengine-nightly", self.failed, self.home, self.now)
        report = path.read_text()
        self.assertIn(f"[Current run logs]({external})", report)
        self.assertIn(f"[Retained run logs]({state / 'runs'})", report)
        self.assertIn(f"[Last successful result]({state / 'last-success.json'})", report)
        self.assertFalse(external.exists())

    def test_nightly_links_fall_back_for_missing_or_malformed_config(self):
        default = self.home / ".local/state/blueengine-nightly"
        self.assertIn(str(default / "runs"), status.nightly_links(self.home))
        config = self.home / ".config/blueengine-nightly/config.json"
        config.parent.mkdir(parents=True)
        for content in ("{", "[]", '{"state": "relative", "runs_root": 42}'):
            config.write_text(content)
            with self.subTest(content=content):
                links = status.nightly_links(self.home)
                self.assertIn(str(default / "runs"), links)
                self.assertIn(str(default / "last-success.json"), links)
                self.assertNotIn("Retained run logs", links)

    def test_stale_admission_is_not_presented_as_current_failure_reason(self):
        self.admission(time="2026-10-16T12:00:00+00:00")
        path = status.record_failure("blueengine-nightly", self.failed, self.home, self.now)
        self.assertIn("too old to explain", path.read_text())
        self.assertNotIn("22.639 GiB", path.read_text())

    def test_recovered_or_running_service_does_not_replace_report(self):
        for values in ({"Result": "success"}, {"ActiveState": "activating"}, {"ActiveState": "active"}, {"ActiveState": "deactivating"}):
            self.assertIsNone(status.record_failure("blueengine-nightly", {**self.failed, **values}, self.home, self.now))
        self.assertFalse((self.home / ".local/state/blueengine-nightly").exists())

    def test_report_omits_arbitrary_status_and_admission_content(self):
        secret = "secret-value-never-show"
        self.admission(reason=secret, environment=secret)
        path = status.record_failure("blueengine-nightly", {**self.failed, "ExecMainStatus": secret + "\nextra", "Environment": secret}, self.home, self.now)
        self.assertNotIn(secret, path.read_text())

    def test_query_requests_only_allowlisted_properties(self):
        with patch.object(tools, "probe", return_value={"ok": True, "output": "Result=exit-code\nEnvironment=secret\n"}) as probe:
            self.assertEqual(status.service_status("blueengine-nightly"), {"Result": "exit-code"})
        args = probe.call_args.args[0]
        self.assertNotIn("--property=Environment", args)
        self.assertNotIn("status", args)
        self.assertIn("--timestamp=unix", args)


if __name__ == "__main__":
    unittest.main()
