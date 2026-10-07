import subprocess
import unittest
from unittest.mock import patch

import server_isolation_test as probe


class IsolationTests(unittest.TestCase):
    def invoke(self, states, start_error=None):
        with patch.object(probe, "start", side_effect=start_error) as start, \
             patch.object(probe, "properties", side_effect=states), \
             patch.object(probe.subprocess, "run") as cleanup, \
             patch.object(probe.util, "report_save") as save, \
             patch.object(probe.util, "emit"):
            result = probe.run()
            return result, start, cleanup, save.call_args.args[1]

    def test_oom_of_one_service_requires_other_to_remain_active(self):
        good = {"ActiveState": "active", "MemoryMax": str(64 * 1024**2)}
        result, start, cleanup, report = self.invoke([good, {"ActiveState": "failed", "Result": "oom-kill"}, good])
        self.assertEqual(result, 0)
        self.assertNotEqual(start.call_args_list[0].args[0], start.call_args_list[1].args[0])
        self.assertTrue(all(c.args[0][-1] in report["units"] for c in cleanup.call_args_list))
        self.assertEqual(cleanup.call_count, 4)

    def test_an_ordinary_exit_does_not_certify_containment(self):
        good = {"ActiveState": "active", "MemoryMax": str(64 * 1024**2)}
        result, _, _, _ = self.invoke([good, {"ActiveState": "inactive", "Result": "success"}, good])
        self.assertEqual(result, 2)

    def test_sibling_failure_does_not_certify_containment(self):
        good = {"ActiveState": "active", "MemoryMax": str(64 * 1024**2)}
        result, _, _, _ = self.invoke([good, {"ActiveState": "failed", "Result": "oom-kill"}, {"ActiveState": "inactive"}])
        self.assertEqual(result, 2)

    def test_partial_startup_still_cleans_only_own_random_units(self):
        result, _, cleanup, report = self.invoke([], start_error=subprocess.TimeoutExpired("scratch", 10))
        self.assertEqual(result, 2)
        self.assertEqual(cleanup.call_count, 4)
        self.assertTrue(all(c.args[0][-1].startswith("dev-room-probe-") for c in cleanup.call_args_list))
        self.assertIn("error", report)


if __name__ == "__main__":
    unittest.main()
