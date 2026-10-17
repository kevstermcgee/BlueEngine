"""Managed cache rotation touches only scratch targets, including a tiny Cargo fixture."""
import contextlib
import copy
import fcntl
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import dev_tools as tools
import storage_policy as storage


class ManagedCachesTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.home = Path(self.temp.name)
        self.stack = contextlib.ExitStack()
        self.addCleanup(self.stack.close)
        self.mount = self.home / "external"
        self.namespace = self.mount / "cache/cargo"
        self.target = self.namespace / "default"
        self.profile = self.target / "debug"
        self.profile.mkdir(parents=True)
        (self.profile / "artifact").write_bytes(b"rebuildable")
        (self.target / "CACHEDIR.TAG").write_text("Signature: 8a477f597d28d172789f06886806bc55\n")
        (self.target / "release").mkdir()
        (self.target / "release/server").write_bytes(b"preserve release")
        self.project = self.home / "project"
        (self.project / "src").mkdir(parents=True)
        self.manifest = self.project / "Cargo.toml"
        self.manifest.write_text('[package]\nname="cache-fixture"\nversion="0.1.0"\nedition="2021"\n'
                                 '[profile.itest]\ninherits="dev"\n[profile.fast]\ninherits="dev"\n')
        (self.project / "src/main.rs").write_text('fn main() { println!("fixture"); }\n')
        self.entry = {"target": str(self.target), "manifest": str(self.manifest),
                      "budget_gib": 0.000001, "profiles": ["dev", "itest", "fast"]}
        self.policy = {"version": 1, "reserve_gib": 30, "warning_free_gib": 40,
                       "cache_budget_gib": 80, "idle_days": 14, "protected_paths": [],
                       "pinned_caches": [str(self.target)], "retired_caches": [],
                       "workloads": {"fixture": [str(self.target)]}, "monitor_paths": [],
                       "required_mounts": [{"mountpoint": str(self.mount), "uuid": "fixture-uuid", "fstype": "ext4"}],
                       "managed_external_caches": [self.entry]}
        self.config = self.home / "policy.json"
        self.config.write_text(json.dumps(self.policy))
        self.nightly_lock = self.home / ".local/state/blueengine-nightly/run.lock"
        self.nightly_lock.parent.mkdir(parents=True)
        self.nightly_lock.touch(mode=0o600)
        self.device = self.mount.stat().st_dev
        self.wrong_mount = False
        self.unit_busy = False
        self.original_probe = tools.probe
        self.stack.enter_context(patch.object(tools, "HOME", self.home))
        self.stack.enter_context(patch.object(tools, "STATE", self.home / "state"))
        self.stack.enter_context(patch.object(storage, "MANAGED_NAMESPACE", self.namespace))
        self.stack.enter_context(patch.object(tools, "cache_references", return_value=([], [])))
        self.stack.enter_context(patch.object(tools, "user_processes", return_value=([], [])))
        self.stack.enter_context(patch.object(tools, "probe", side_effect=self.probe))
        storage.validate_policy(self.policy)

    def probe(self, command, **kwargs):
        if command[0] == "/usr/bin/findmnt":
            row = {"target": str(self.mount), "uuid": "wrong-uuid" if self.wrong_mount else "fixture-uuid",
                   "fstype": "ext4", "fsroot": "/", "options": "rw",
                   "maj:min": f"{os.major(self.device)}:{os.minor(self.device)}"}
            return {"ok": True, "output": json.dumps({"filesystems": [row]})}
        if command[0] == "systemctl":
            return {"ok": True, "output": "LoadState=loaded\nActiveState=" + ("activating" if self.unit_busy else "inactive")}
        return self.original_probe(command, **kwargs)

    def rotate(self):
        report = {"actions": []}
        storage.rotate_managed_caches(self.policy, report, self.config)
        return report["actions"][0]

    def assert_preserved(self):
        self.assertEqual((self.profile / "artifact").read_bytes(), b"rebuildable")
        self.assertEqual((self.target / "release/server").read_bytes(), b"preserve release")
        self.assertTrue((self.project / "src/main.rs").is_file())

    def test_registration_rejects_unreviewed_namespace_release_and_invalid_budget(self):
        for changes in ({"target": str(self.home / "arbitrary")}, {"profiles": ["release"]},
                        {"budget_gib": float("inf")}, {"budget_gib": 0}):
            policy = copy.deepcopy(self.policy)
            policy["managed_external_caches"][0].update(changes)
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                storage.validate_policy(policy)
        with self.assertRaises(ValueError):
            storage.validate_policy(dict(self.policy, required_mounts=[]))

    def test_active_unit_and_idea_supervisor_both_defer_between_cargo_stages(self):
        self.unit_busy = True
        with patch.object(storage, "managed_clean") as clean:
            self.assertEqual(self.rotate()["status"], "retained")
            clean.assert_not_called()
        self.unit_busy = False
        daily = self.home / "BlueEngine-idea-forge-native/.be2-work/idea-forge/daily"
        daily.mkdir(parents=True)
        (daily / "supervisor.lock").write_text(str(os.getpid()))
        (daily / "supervisor.lock").chmod(0o600)
        with patch.object(storage, "managed_clean") as clean:
            self.assertEqual(self.rotate()["status"], "retained")
            clean.assert_not_called()
        self.assert_preserved()

    def test_nightly_supervisor_and_cooperative_lock_defer_cleanup(self):
        with self.nightly_lock.open("r") as lock, patch.object(storage, "managed_clean") as clean:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.assertEqual(self.rotate()["status"], "retained")
            clean.assert_not_called()
        with tools.coordination_lock(), patch.object(storage, "managed_clean") as clean:
            self.assertEqual(self.rotate()["status"], "retained")
            clean.assert_not_called()
        self.assert_preserved()

    @unittest.skipUnless(shutil.which("flock"), "Scheduler condition fixture requires flock")
    def test_scheduler_shared_condition_defers_while_manager_holds_exclusive_lock(self):
        condition = [shutil.which("flock"), "--shared", "--nonblock", str(tools.STATE / "build-maintenance.lock"), "/usr/bin/true"]
        with tools.coordination_lock():
            self.assertEqual(subprocess.run(condition, timeout=5).returncode, 1)
        self.assertEqual(subprocess.run(condition, timeout=5).returncode, 0)

    def test_active_rust_or_cache_consumers_preserve_all_artifacts(self):
        for name in ("rustc", "running-game"):
            with patch.object(tools, "cache_references", return_value=([{"name": name}], [])), patch.object(storage, "managed_clean") as clean:
                self.assertEqual(self.rotate()["status"], "retained")
                clean.assert_not_called()
        self.assert_preserved()

    def test_mapped_cache_library_and_unknown_process_visibility_defer_cleanup(self):
        original_read = Path.read_text
        def mapped_read(path, *args, **kwargs):
            if str(path) == "/proc/987654/status":
                return "State:\tS (sleeping)\n"
            if str(path) == "/proc/987654/maps":
                return f"0-1 r-xp 0 0:0 1 {self.target}/debug/libfixture.so\n"
            if str(path) == "/proc/987654/comm":
                return "unknown-worker\n"
            return original_read(path, *args, **kwargs)
        with patch.object(Path, "read_text", mapped_read), \
                patch.object(tools, "user_processes", return_value=([{"pid": 987654, "name": "fixture"}], [])), \
                patch.object(storage, "managed_clean") as clean:
            self.assertEqual(self.rotate()["status"], "retained")
            clean.assert_not_called()
        with patch.object(Path, "read_text", mapped_read), \
                patch.object(tools, "cache_references", return_value=([], [987654])), \
                patch.object(storage, "managed_clean") as clean:
            self.assertEqual(self.rotate()["status"], "retained")
            clean.assert_not_called()
        self.assert_preserved()

    def test_confirmed_zombies_and_vanished_processes_do_not_block_but_live_codex_does(self):
        original_read = Path.read_text
        state = "Z"
        def process_read(path, *args, **kwargs):
            if str(path) == "/proc/987654/status":
                if state == "gone":
                    raise FileNotFoundError()
                return f"Name:\tcodex\nState:\t{state} (fixture)\n"
            if str(path) == "/proc/987654/comm":
                return "codex\n"
            if str(path) == "/proc/987654/maps":
                raise PermissionError()
            return original_read(path, *args, **kwargs)
        with patch.object(Path, "read_text", process_read), \
                patch.object(tools, "cache_references", return_value=([], [987654])), \
                patch.object(tools, "user_processes", return_value=([{"pid": 987654, "name": "codex"}], [])):
            for state in ("Z", "gone"):
                storage.managed_cache_idle(self.policy, self.entry)
            state = "S"
            with self.assertRaisesRegex(ValueError, "visibility is incomplete"):
                storage.managed_cache_idle(self.policy, self.entry)

    def test_wrong_mount_and_source_like_or_symlink_profile_never_clean(self):
        self.wrong_mount = True
        with patch.object(storage, "managed_clean") as clean:
            self.assertEqual(self.rotate()["status"], "retained")
            clean.assert_not_called()
        self.wrong_mount = False
        (self.profile / "Cargo.toml").write_text("source must survive")
        with patch.object(storage, "managed_clean") as clean:
            self.assertEqual(self.rotate()["status"], "retained")
            clean.assert_not_called()
        self.assertEqual((self.profile / "Cargo.toml").read_text(), "source must survive")
        (self.profile / "Cargo.toml").unlink()
        (self.profile / "artifact").unlink()
        self.profile.rmdir()
        self.profile.symlink_to(self.project, target_is_directory=True)
        with patch.object(storage, "managed_clean") as clean:
            self.assertEqual(self.rotate()["status"], "retained")
            clean.assert_not_called()
        self.assertTrue((self.project / "src/main.rs").is_file())

    def test_missing_provenance_or_failed_preview_preserves_source_and_release(self):
        marker = self.target / "CACHEDIR.TAG"
        marker.unlink()
        with patch.object(storage, "managed_clean") as clean:
            self.assertEqual(self.rotate()["status"], "retained")
            clean.assert_not_called()
        marker.write_text("Signature: 8a477f597d28d172789f06886806bc55\n")
        with patch.object(storage, "managed_clean", return_value=1) as clean:
            action = self.rotate()
            self.assertEqual(action["status"], "retained")
            self.assertEqual(clean.call_args_list[0].args, (self.entry, "dev", True))
            self.assertEqual(clean.call_count, 1)
        with patch.object(storage, "managed_clean", side_effect=[0, 1]) as clean:
            self.assertEqual(self.rotate()["status"], "failed")
            self.assertEqual(clean.call_args_list[-1].args, (self.entry, "dev", False))
        with patch.object(storage, "managed_clean", side_effect=subprocess.TimeoutExpired("cargo", 60)):
            self.assertEqual(self.rotate()["status"], "retained")
        self.assert_preserved()

    def test_new_job_or_changed_registration_after_preview_prevents_apply(self):
        def job_starts(*args):
            self.unit_busy = True
            return 0
        with patch.object(storage, "managed_clean", side_effect=job_starts) as clean:
            self.assertEqual(self.rotate()["status"], "retained")
            self.assertEqual(clean.call_count, 1)
        self.unit_busy = False
        def unregister(*args):
            self.config.write_text(json.dumps(dict(self.policy, managed_external_caches=[])))
            return 0
        with patch.object(storage, "managed_clean", side_effect=unregister) as clean:
            self.assertEqual(self.rotate()["status"], "retained")
            self.assertEqual(clean.call_count, 1)
        self.assert_preserved()

    def test_admission_never_rotates_external_profiles_even_when_capacity_is_low(self):
        with patch.object(storage, "admit", side_effect=[storage.InsufficientStorage("low"), [{"available_gib": 44}]]), \
                patch.object(storage, "inventory", return_value={"storage": {"available_gib": 5}, "internal_cache_gib": 1, "actions": []}), \
                patch.object(storage, "rotate_managed_caches") as rotate, contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(storage.main(["--config", str(self.config), "admit", "--workload", "fixture"]), 0)
            rotate.assert_not_called()

    @unittest.skipUnless(shutil.which("cargo"), "Native Cargo fixture requires Cargo")
    def test_native_cargo_rotation_keeps_release_binaries_and_all_source(self):
        env = {**os.environ, "RUSTC_WRAPPER": "", "CARGO_BUILD_RUSTC_WRAPPER": "", "CARGO_BUILD_JOBS": "1",
               "CARGO_TARGET_DIR": str(self.target), "CARGO_BUILD_TARGET_DIR": str(self.target),
               "CARGO_BUILD_BUILD_DIR": str(self.target)}
        cargo = shutil.which("cargo")
        subprocess.run([cargo, "generate-lockfile", "--offline", "--manifest-path", str(self.manifest)],
                       env=env, check=True, timeout=60, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for profile in ("dev", "itest", "fast", "release"):
            subprocess.run([cargo, "build", "--frozen", "--offline", "--manifest-path", str(self.manifest),
                            "--target-dir", str(self.target), "--profile", profile,
                            "--config", "build.build-dir=" + json.dumps(str(self.target))],
                           env=env, check=True, timeout=60, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        source = {p.relative_to(self.project): p.read_bytes() for p in self.project.rglob("*") if p.is_file()}
        release = {p.relative_to(self.target / "release"): p.read_bytes()
                   for p in (self.target / "release").rglob("*") if p.is_file()}
        action = self.rotate()
        self.assertEqual(action["status"], "cleaned", action)
        self.assertEqual(action["profiles"], ["dev", "itest", "fast"])
        self.assertEqual({p.relative_to(self.project): p.read_bytes() for p in self.project.rglob("*") if p.is_file()}, source)
        self.assertEqual({p.relative_to(self.target / "release"): p.read_bytes()
                          for p in (self.target / "release").rglob("*") if p.is_file()}, release)
        for name in ("debug", "itest", "fast"):
            self.assertFalse((self.target / name / "cache-fixture").exists())


if __name__ == "__main__":
    unittest.main()
