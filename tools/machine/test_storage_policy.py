"""Storage policy regressions use scratch caches and never touch real services."""
import argparse
import contextlib
import io
import json
import os
from pathlib import Path
import tempfile
import time
import unittest
from types import SimpleNamespace
from unittest.mock import patch

import dev_tools as tools
import storage_policy as storage


class StoragePolicyTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.home = Path(self.temp.name)
        self.stack = contextlib.ExitStack()
        self.addCleanup(self.stack.close)
        for name, value in (("HOME", self.home), ("STATE", self.home / "state")):
            self.stack.enter_context(patch.object(tools, name, value))
        self.stack.enter_context(patch.object(tools, "cache_references", return_value=([], [])))
        self.stack.enter_context(patch.object(tools, "user_processes", return_value=([], [])))
        self.project = self.home / "completed"
        self.target = self.project / "target"
        self.profile = self.target / "debug"
        self.profile.mkdir(parents=True)
        (self.project / "Cargo.toml").write_text('[package]\nname="fixture"\nversion="0.1.0"\n')
        (self.project / "source.rs").write_text("valuable source")
        (self.target / ".rustc_info.json").write_text("{}")
        (self.profile / "artifact").write_text("rebuildable")
        (self.target / "release").mkdir()
        (self.target / "release/server").write_text("retain executable")
        self.entry = {"target": str(self.target), "manifest": str(self.project / "Cargo.toml"),
                      "profiles": ["dev"], "reviewed_commit": "a" * 40, "base": "origin/main"}
        self.policy = {"version": 1, "reserve_gib": 30, "warning_free_gib": 40,
                       "cache_budget_gib": 80, "idle_days": 14, "protected_paths": [],
                       "pinned_caches": [], "retired_caches": [self.entry],
                       "workloads": {"fixture": [str(self.target)]}, "monitor_paths": []}
        self.config = self.home / "policy.json"
        self.config.write_text(json.dumps(self.policy))
        def git(project, *args):
            return "a" * 40 if args[0] == "rev-parse" else ""
        self.stack.enter_context(patch.object(storage, "git", side_effect=git))

    def report(self, pressure=False):
        return {"storage": {"available_gib": 5 if pressure else 60},
                "internal_cache_gib": 1, "actions": []}

    def test_plan_never_cleans_or_registers_unknown_caches(self):
        with patch.object(storage, "inventory", return_value=self.report()), patch.object(tools, "cache_command") as clean, contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(storage.main(["--config", str(self.config), "plan"]), 0)
            clean.assert_not_called()
        self.assertEqual(json.loads(self.config.read_text()), self.policy)

    def test_policy_rejects_protected_and_pinned_scope(self):
        for protected in ("protected_paths", "pinned_caches"):
            policy = dict(self.policy, **{protected: [str(self.target)]})
            with self.assertRaisesRegex(ValueError, "protected or pinned"):
                storage.validate_policy(policy)
        forbidden = dict(self.entry, target=str(self.home / "blueengine/target"),
                         manifest=str(self.home / "blueengine/Cargo.toml"))
        (self.home / "blueengine").mkdir()
        with self.assertRaisesRegex(ValueError, "protected or pinned"):
            storage.validate_policy(dict(self.policy, retired_caches=[forbidden]))

    def test_policy_refuses_release_symlinks_and_invalid_thresholds(self):
        with self.assertRaisesRegex(ValueError, "Only reviewed"):
            storage.validate_policy(dict(self.policy, retired_caches=[dict(self.entry, profiles=["release"])]))
        alias = self.home / "alias"
        alias.symlink_to(self.target, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "symlink"):
            storage.exact_path(alias)
        for bad in (-1, float("nan"), float("inf"), True):
            with self.assertRaises(ValueError):
                storage.validate_policy(dict(self.policy, reserve_gib=bad))

    def test_resumed_or_dirty_checkout_is_retained(self):
        for outputs in (("b" * 40,), ("a" * 40, " M source.rs")):
            with patch.object(storage, "git", side_effect=outputs), patch.object(tools, "cache_command") as clean:
                report = self.report(pressure=True)
                storage.sweep(self.policy, report)
                self.assertEqual(report["actions"][0]["status"], "retained")
                clean.assert_not_called()
        self.assertEqual((self.profile / "artifact").read_text(), "rebuildable")

    def test_active_binary_or_checkout_blocks_automatic_cleanup(self):
        for name, result in (("cache_references", ([{"pid": 1}], [])),
                             ("user_processes", ([{"cwd": str(self.project)}], []))):
            with patch.object(tools, name, return_value=result), patch.object(tools, "cache_command") as clean:
                report = self.report(pressure=True)
                storage.sweep(self.policy, report)
                self.assertEqual(report["actions"][0]["status"], "retained")
                clean.assert_not_called()

    def test_normal_retention_keeps_recent_cache_but_pressure_can_retire_it(self):
        with patch.object(tools, "cache_command", return_value=0) as clean:
            report = self.report()
            storage.sweep(self.policy, report)
            clean.assert_not_called()
            self.assertEqual(report["actions"][0]["reason"], "Within the retention window")
            report = self.report(pressure=True)
            storage.sweep(self.policy, report)
            self.assertEqual([call.args[0].apply for call in clean.call_args_list], [True, True])
            self.assertEqual(report["actions"][0]["status"], "cleaned")

    def test_preview_failure_preserves_all_files(self):
        with patch.object(tools, "cache_command", return_value=1) as clean:
            report = self.report(pressure=True)
            storage.sweep(self.policy, report)
            self.assertEqual(clean.call_count, 1)
            self.assertFalse(clean.call_args.args[0].apply)
        self.assertTrue((self.profile / "artifact").exists())
        self.assertTrue((self.target / "release/server").exists())
        self.assertEqual((self.project / "source.rs").read_text(), "valuable source")

    def test_changed_checkout_after_preview_prevents_apply(self):
        with patch.object(storage, "eligible", side_effect=[[], ValueError("resumed")]), patch.object(tools, "cache_command", return_value=0) as clean:
            report = self.report(pressure=True)
            storage.sweep(self.policy, report)
            self.assertEqual(clean.call_count, 1)
            self.assertFalse(clean.call_args.args[0].apply)
            self.assertEqual(report["actions"][0]["status"], "retained")

    def test_profile_symlink_or_source_like_profile_is_never_cleaned(self):
        (self.profile / "Cargo.toml").write_text("source")
        with patch.object(tools, "cache_command") as clean:
            storage.sweep(self.policy, self.report(pressure=True))
            clean.assert_not_called()

    def test_age_uses_nested_artifacts_not_only_directory_mtime(self):
        old = time.time() - 30 * 86400
        os.utime(self.profile, (old, old))
        self.assertGreater(storage.latest_write(self.profile), old + 1)

    def test_admission_checks_actual_workload_filesystem_and_keeps_job(self):
        with patch.object(tools, "space", return_value={"available_gib": 20}) as space:
            with self.assertRaisesRegex(ValueError, "fixture retained"):
                storage.admit(self.policy, "fixture")
            self.assertEqual(space.call_args.args[0], str(self.target))
        with patch.object(tools, "space", return_value={"available_gib": 44}):
            self.assertEqual(storage.admit(self.policy, "fixture"), [{"available_gib": 44}])

    def test_healthy_admission_skips_global_inventory_and_cleanup(self):
        with patch.object(tools, "space", return_value={"available_gib": 44}), \
                patch.object(storage, "inventory") as inventory, patch.object(storage, "sweep") as sweep, \
                contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(storage.main(["--config", str(self.config), "admit", "--workload", "fixture"]), 0)
            inventory.assert_not_called()
            sweep.assert_not_called()
        report = json.loads((tools.STATE / "storage-admission-fixture.json").read_text())
        self.assertTrue(report["admitted"])
        self.assertEqual(report["admitted_storage"], [{"available_gib": 44}])
        self.assertNotIn("internal_cache_gib", report)
        self.assertEqual(report["actions"], [])

    def test_low_capacity_admission_uses_reviewed_cleanup_then_rechecks(self):
        with patch.object(storage, "admit", side_effect=[storage.InsufficientStorage("fixture low"), [{"available_gib": 44}]]) as admit, \
                patch.object(storage, "inventory", return_value=self.report(pressure=True)) as inventory, \
                patch.object(tools, "cache_command", return_value=0) as clean, contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(storage.main(["--config", str(self.config), "admit", "--workload", "fixture"]), 0)
        self.assertEqual(admit.call_count, 2)
        inventory.assert_called_once_with(self.policy)
        self.assertEqual(clean.call_count, 2)
        self.assertTrue(clean.call_args.args[0].apply)
        report = json.loads((tools.STATE / "storage-admission-fixture.json").read_text())
        self.assertTrue(report["admitted"])
        self.assertEqual(report["actions"][0]["target"], str(self.target))

    def test_mount_failure_during_admission_never_enters_cleanup_fallback(self):
        with patch.object(storage, "admit", side_effect=ValueError("Required storage changed")), \
                patch.object(storage, "inventory") as inventory, patch.object(storage, "sweep") as sweep, \
                contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(storage.main(["--config", str(self.config), "admit", "--workload", "fixture"]), 2)
        inventory.assert_not_called()
        sweep.assert_not_called()

    def test_unknown_workload_cannot_escape_report_directory(self):
        with patch.object(tools, "report_save") as save, contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(storage.main(["--config", str(self.config), "admit", "--workload", "../../escape"]), 2)
            save.assert_not_called()

    def mounted_policy(self):
        mountpoint = self.home / "external"
        mountpoint.mkdir()
        expected = {"mountpoint": str(mountpoint), "uuid": "fixture-uuid", "fstype": "ext4", "fsroot": "/"}
        device = mountpoint.stat().st_dev
        row = {"target": str(mountpoint), "uuid": "fixture-uuid", "fstype": "ext4", "fsroot": "/",
               "options": "rw,nosuid,nodev", "maj:min": f"{os.major(device)}:{os.minor(device)}"}
        policy = dict(self.policy, required_mounts=[expected],
                      workloads={"fixture": [str(mountpoint / "future/cache")]})
        return policy, row

    def test_required_mount_identity_precedes_cleanup_and_capacity_probe(self):
        policy, row = self.mounted_policy()
        self.config.write_text(json.dumps(policy))
        for broken in (None, dict(row, uuid="wrong-uuid"), dict(row, fsroot="/wrong-bind"),
                       dict(row, fstype="tmpfs"), dict(row, options="ro,nosuid"),
                       dict(row, **{"maj:min": "0:0"})):
            result = {"ok": broken is not None, "output": json.dumps({"filesystems": [broken]})}
            with self.subTest(row=broken), patch.object(tools, "probe", return_value=result), \
                    patch.object(storage, "inventory") as inventory, patch.object(tools, "cache_command") as clean, \
                    patch.object(tools, "space") as space, contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(storage.main(["--config", str(self.config), "admit", "--workload", "fixture"]), 2)
                inventory.assert_not_called()
                clean.assert_not_called()
                space.assert_not_called()
        self.assertTrue((self.profile / "artifact").exists())

    def test_required_mount_accepts_future_children_without_creating_them(self):
        policy, row = self.mounted_policy()
        future = Path(policy["workloads"]["fixture"][0])
        storage.validate_policy(policy)
        with patch.object(tools, "probe", return_value={"ok": True, "output": json.dumps({"filesystems": [row]})}):
            verified = storage.verify_required_mounts(policy)
        self.assertEqual(verified[0]["uuid"], "fixture-uuid")
        self.assertFalse(future.exists())

    def test_required_mount_rejects_wrong_device_below_valid_mount(self):
        policy, row = self.mounted_policy()
        cache = Path(row["target"]) / "nested/cache"
        cache.mkdir(parents=True)
        policy["workloads"] = {"fixture": [str(cache)]}
        original_stat = Path.stat
        def changed_stat(path, *args, **kwargs):
            if path == cache:
                return SimpleNamespace(st_dev=0)
            return original_stat(path, *args, **kwargs)
        with patch.object(tools, "probe", return_value={"ok": True, "output": json.dumps({"filesystems": [row]})}), \
                patch.object(Path, "stat", changed_stat):
            with self.assertRaisesRegex(ValueError, "backing .* differs"):
                storage.verify_required_mounts(policy)

    def test_mount_failure_blocks_direct_sweep_and_retirement(self):
        policy, _ = self.mounted_policy()
        with patch.object(tools, "probe", return_value={"ok": False}), patch.object(tools, "cache_command") as clean:
            with self.assertRaisesRegex(ValueError, "Required storage"):
                storage.sweep(policy, self.report(pressure=True))
            with self.assertRaisesRegex(ValueError, "Required storage"):
                storage.retire(policy, self.config, str(self.target), ["dev"])
            clean.assert_not_called()

    def test_mount_disappearing_after_native_preview_prevents_apply(self):
        with patch.object(storage, "verify_required_mounts", side_effect=[[], ValueError("Required storage disappeared")]), \
                patch.object(tools, "cache_command", return_value=0) as clean:
            report = self.report(pressure=True)
            storage.sweep(self.policy, report)
        self.assertEqual(clean.call_count, 1)
        self.assertFalse(clean.call_args.args[0].apply)
        self.assertEqual(report["actions"][0]["status"], "retained")
        self.assertTrue((self.profile / "artifact").exists())

    def test_required_mount_policy_rejects_ambiguous_identity(self):
        policy, _ = self.mounted_policy()
        mount = policy["required_mounts"][0]
        for values in ({"uuid": ""}, {"fstype": ""}, {"fsroot": "relative"}, {"fsroot": "/a/../b"}):
            with self.subTest(values=values), self.assertRaises(ValueError):
                storage.validate_policy(dict(policy, required_mounts=[dict(mount, **values)]))
        with self.assertRaisesRegex(ValueError, "unique"):
            storage.validate_policy(dict(policy, required_mounts=[mount, mount]))

    def test_cache_measurement_deduplicates_relocated_aliases(self):
        alias = self.home / "old-cache"
        alias.symlink_to(self.target, target_is_directory=True)
        with patch.object(tools, "probe", return_value={"ok": True, "output": f"123\t{self.target}"}) as probe:
            self.assertEqual(storage.measure([str(alias), str(self.target)]), {str(self.target): 123})
            self.assertEqual(probe.call_args.args[0], ["du", "-sx", "-B1", str(self.target)])

    def test_missing_retired_target_does_not_block_storage_policy(self):
        for p in self.target.rglob("*"):
            if p.is_file():
                p.unlink()
        for p in sorted(self.target.rglob("*"), reverse=True):
            if p.is_dir():
                p.rmdir()
        self.target.rmdir()
        storage.validate_policy(self.policy)
        report = self.report(pressure=True)
        storage.sweep(self.policy, report)
        self.assertEqual(report["actions"], [])

    def test_explicit_retirement_registers_only_after_native_preview(self):
        policy = dict(self.policy, retired_caches=[])
        self.config.write_text(json.dumps(policy))
        calls = []
        def clean(args):
            # Taking the same lock here proves registration did not nest it.
            with tools.coordination_lock():
                calls.append(args.apply)
            return 0
        with patch.object(tools, "cache_command", side_effect=clean):
            storage.retire(policy, self.config, str(self.target), ["dev"])
        self.assertEqual(calls, [False])
        self.assertEqual(json.loads(self.config.read_text())["retired_caches"], [self.entry])


if __name__ == "__main__":
    unittest.main()
