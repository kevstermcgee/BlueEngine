"""Behavioral tests run in scratch directories; no real caches or services touched."""
import argparse
import contextlib
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import dev_tools as tools


class UtilitiesTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.home = Path(self.temp.name)
        self.stack = contextlib.ExitStack()
        self.addCleanup(self.stack.close)
        for name, value in [("HOME", self.home), ("STATE", self.home / "state"), ("BIN", self.home / "bin")]:
            self.stack.enter_context(patch.object(tools, name, value))

    def cache(self, name="project"):
        project = self.home / name
        target = project / "target"
        target.mkdir(parents=True)
        (project / "Cargo.toml").write_text('[package]\nname="fixture"\nversion="0.1.0"\n')
        (target / ".rustc_info.json").write_text("{}")
        (target / "debug").mkdir()
        return project, target

    def args(self, **values):
        defaults = dict(plan=False, save=False, cache=None, profile=None, apply=False)
        return argparse.Namespace(**(defaults | values))

    def test_discovery_excludes_unmarked_and_source_directories(self):
        _, good = self.cache()
        _, bad = self.cache("source-like")
        (bad / "Cargo.toml").write_text("source")
        _, unmarked = self.cache("unmarked")
        (unmarked / ".rustc_info.json").unlink()
        self.assertEqual([c["target"] for c in tools.discover_caches()], [str(good)])

    def test_discovery_excludes_target_symlinks(self):
        _, good = self.cache()
        other = self.home / "linked"
        other.mkdir()
        (other / "Cargo.toml").write_text("source")
        (other / "target").symlink_to(good, target_is_directory=True)
        self.assertEqual(len(tools.discover_caches()), 1)

    def test_discovery_recognizes_cargo_tag_without_rustc_info(self):
        _, target = self.cache()
        (target / ".rustc_info.json").unlink()
        (target / "CACHEDIR.TAG").write_text("Signature: 8a477f597d28d172789f06886806bc55\n")
        (target / "debug/.fingerprint").mkdir()
        self.assertEqual(len(tools.discover_caches()), 1)

    def test_discovery_rejects_invalid_cache_tag(self):
        _, target = self.cache()
        (target / ".rustc_info.json").unlink()
        (target / "CACHEDIR.TAG").write_text("not a cache signature\n")
        (target / "debug/.fingerprint").mkdir()
        self.assertEqual(tools.discover_caches(), [])

    def test_symlink_configuration_directory_refused(self):
        actual = self.home / "actual"
        actual.mkdir()
        link = self.home / "link"
        link.symlink_to(actual, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "symlink"):
            tools.private_dir(link)

    def test_state_lock_excludes_second_owner(self):
        with tools.coordination_lock():
            with self.assertRaisesRegex(ValueError, "Another coordinated"):
                with tools.coordination_lock():
                    self.fail("second lock acquired")
        with tools.coordination_lock():
            pass

    def test_lock_file_symlink_refused(self):
        tools.private_dir(tools.STATE)
        victim = self.home / "victim"
        victim.write_text("unchanged")
        (tools.STATE / "build-maintenance.lock").symlink_to(victim)
        with self.assertRaises(OSError):
            with tools.coordination_lock():
                pass
        self.assertEqual(victim.read_text(), "unchanged")

    def test_clean_refuses_active_build(self):
        _, target = self.cache()
        with patch.object(tools, "cache_references", return_value=([{"name": "rustc"}], [])), patch.object(tools.subprocess, "run") as run, contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(tools.cache_command(self.args(cache=str(target), profile="dev", apply=True)), 2)
            run.assert_not_called()

    def test_clean_records_visibility_gaps_and_uses_native_cargo(self):
        _, target = self.cache()
        with patch.object(tools, "cache_references", return_value=([], [99])), patch.object(tools.subprocess, "run", return_value=argparse.Namespace(returncode=0)) as run, contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(tools.cache_command(self.args(cache=str(target), profile="dev", apply=True)), 0)
            self.assertIn("clean", run.call_args.args[0])
            self.assertNotIn("--dry-run", run.call_args.args[0])
        report = json.loads((tools.STATE / "last-cleanup.json").read_text())
        self.assertEqual(report["uninspectable_user_processes"], [99])

    def test_clean_dry_run_remains_available_with_unknown_process(self):
        _, target = self.cache()
        with patch.object(tools, "cache_references", return_value=([], [99])), patch.object(tools.subprocess, "run", return_value=argparse.Namespace(returncode=0)) as run:
            self.assertEqual(tools.cache_command(self.args(cache=str(target), profile="dev")), 0)
            self.assertIn("--dry-run", run.call_args.args[0])

    def test_clean_dry_run_keeps_source_and_uses_native_cargo(self):
        project, target = self.cache()
        source = project / "source.txt"
        source.write_text("keep")
        with patch.object(tools, "cache_references", return_value=([], [])), patch.object(tools.subprocess, "run", return_value=argparse.Namespace(returncode=0)) as run:
            self.assertEqual(tools.cache_command(self.args(cache=str(target), profile="dev")), 0)
            cmd = run.call_args.args[0]
            self.assertIn("--dry-run", cmd)
            self.assertIn("--frozen", cmd)
            self.assertEqual(cmd[cmd.index("--target-dir") + 1], str(target))
        self.assertEqual(source.read_text(), "keep")

    def test_clean_apply_requires_exact_target(self):
        with self.assertRaisesRegex(ValueError, "exact"):
            tools.cache_command(self.args(apply=True))

    def test_plan_cannot_apply(self):
        with self.assertRaisesRegex(ValueError, "cannot be combined"):
            tools.cache_command(self.args(plan=True, apply=True))

    def test_clean_refuses_unregistered_directory(self):
        with self.assertRaisesRegex(ValueError, "not a discovered"):
            tools.cache_command(self.args(cache=str(self.home), profile="dev", apply=True))

    def test_clean_refuses_symlink_profile(self):
        _, target = self.cache()
        (target / "debug").rmdir()
        (target / "debug").symlink_to(self.home, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "symlink"):
            tools.cache_command(self.args(cache=str(target), profile="dev", apply=True))

    def test_report_atomic_replace_does_not_follow_symlink(self):
        tools.private_dir(tools.STATE)
        victim = self.home / "victim"
        victim.write_text("unchanged")
        (tools.STATE / "example.json").symlink_to(victim)
        p = tools.report_save("example", {"ok": True})
        self.assertFalse(p.is_symlink())
        self.assertEqual(p.stat().st_mode & 0o777, 0o600)
        self.assertEqual(victim.read_text(), "unchanged")

    def build_args(self, command, **overrides):
        values = dict(command=command, cwd=str(self.home), target_dir=None, jobs=2, min_free_gib=0, wait=0)
        return argparse.Namespace(**(values | overrides))

    def test_build_preserves_exit_status_and_omits_child_credentials(self):
        secret = "fixture-secret-never-store"
        code = tools.build_command(self.build_args([sys.executable, "-c", "raise SystemExit(7)", secret]))
        self.assertEqual(code, 7)
        contents = (tools.STATE / "last-build.json").read_text()
        self.assertNotIn(secret, contents)
        self.assertEqual(json.loads(contents)["exit_code"], 7)

    def test_build_passes_cargo_jobs_and_target_without_shell_expansion(self):
        selected = self.home / "chosen target"
        out = self.home / "environment.json"
        code = "import json,os,sys;open(sys.argv[1],'w').write(json.dumps([os.environ['CARGO_BUILD_JOBS'],os.environ['CARGO_TARGET_DIR'],sys.argv[2]]))"
        literal = "$(touch should-not-exist)"
        status = tools.build_command(self.build_args([sys.executable, "-c", code, str(out), literal], target_dir=str(selected)))
        self.assertEqual(status, 0)
        self.assertEqual(json.loads(out.read_text()), ["2", str(selected), literal])
        self.assertFalse((self.home / "should-not-exist").exists())

    def test_build_low_disk_refuses_child(self):
        with patch.object(tools, "space", return_value={"available_gib": 1}), patch.object(tools.subprocess, "Popen") as child:
            with self.assertRaisesRegex(ValueError, "Insufficient build headroom"):
                tools.build_command(self.build_args([sys.executable], min_free_gib=20))
            child.assert_not_called()

    def test_build_signal_exit_propagates(self):
        code = tools.build_command(self.build_args([sys.executable, "-c", "import os,signal;os.kill(os.getpid(),signal.SIGTERM)"]))
        self.assertEqual(code, 128 + signal.SIGTERM)

    def test_worktree_parser_preserves_paths_with_spaces(self):
        data = "worktree /tmp/a tree\0HEAD abc\0detached\0\0worktree /tmp/gone\0prunable missing gitdir\0\0"
        rows = tools.parse_worktrees(data)
        self.assertEqual(rows[0]["path"], "/tmp/a tree")
        self.assertTrue(rows[0]["detached"])
        self.assertEqual(rows[1]["prunable"], "missing gitdir")

    def test_package_installer_refuses_unmanaged_command(self):
        tools.BIN.mkdir()
        (tools.BIN / "strace").write_text("my utility")
        with self.assertRaisesRegex(ValueError, "unmanaged"):
            tools.install_package({"package": "strace", "version": "fixture", "commands": ["strace"]})

    def test_bootstrap_checksum_failure_never_installs(self):
        package = {"package": "strace", "version": "fixture", "commands": ["strace"], "url": "https://example.invalid/package", "size": 3, "sha256": "0" * 64}
        response = io.BytesIO(b"bad")
        with patch.object(tools.urllib.request, "urlopen", return_value=response), patch.object(tools.subprocess, "run") as extract:
            with self.assertRaisesRegex(ValueError, "checksum"):
                tools.install_package(package)
            extract.assert_not_called()
        self.assertFalse((tools.BIN / "strace").exists())

    def test_nonfinite_storage_threshold_refused(self):
        for value in ["nan", "inf", "-1"]:
            with self.assertRaises(argparse.ArgumentTypeError):
                tools.nonnegative(value)

    def test_server_health_checks_deployed_build_without_restarting(self):
        receipt = self.home / ".local/share/blueengine/deployed/fixture.json"
        receipt.parent.mkdir(parents=True)
        receipt.write_text(json.dumps({"info": {"build": "abcdef12"}, "phase": "ready"}))
        calls = []
        def fake_probe(command, **kwargs):
            calls.append(command)
            output = "LoadState=loaded\nActiveState=active\nResult=success\nExecMainStatus=0"
            return {"ok": True, "output": output, "exit_code": 0, "error": ""}
        with patch.object(tools, "probe", side_effect=fake_probe), contextlib.redirect_stdout(io.StringIO()):
            code = tools.server_command(argparse.Namespace(game=["fixture"], save=False))
        self.assertEqual(code, 0)
        query = calls[-1]
        self.assertIn("--expect-build", query)
        self.assertEqual(query[query.index("--expect-build") + 1], "abcdef12")
        self.assertEqual(query[query.index("--wait") + 1], "1")
        self.assertFalse(any("restart" in cmd or "reload" in cmd for cmd in calls))

    def test_server_health_rejects_invalid_receipt(self):
        receipt = self.home / ".local/share/blueengine/deployed/fixture.json"
        receipt.parent.mkdir(parents=True)
        receipt.write_text("{}"); capture = io.StringIO()
        with patch.object(tools, "probe", return_value={"ok": True, "output": "LoadState=loaded\nActiveState=active\nResult=success\nExecMainStatus=0"}), contextlib.redirect_stdout(capture):
            self.assertEqual(tools.server_command(argparse.Namespace(game=["fixture"], save=False)), 2)
        self.assertEqual(json.loads(capture.getvalue())["games"][0]["deployment"]["receipt"], "unreadable or invalid")

    def test_report_inventory_reads_sizes_and_skips_symlinks_without_touching_contents(self):
        root = self.home / "reports"
        room = root / "fixture/port-4101"; room.mkdir(parents=True)
        archive = room / "matches.jsonl"; archive.write_bytes(b"original archive")
        linked = root / "fixture/port-4102"; linked.mkdir()
        (linked / "matches.jsonl").symlink_to(archive)
        (root / "fixture/port-4103").symlink_to(room, target_is_directory=True)
        config = self.home / "hub.conf"; config.write_text(f"[hub]\nreport_dir={root}\n")
        result = tools.report_archive_usage(config, ["fixture"])
        self.assertEqual(result["total_bytes"], len(b"original archive"))
        self.assertEqual(len(result["files"]), 1)
        self.assertEqual(archive.read_bytes(), b"original archive")

    def test_report_inventory_is_bounded_and_does_not_guess_relative_roots(self):
        root = self.home / "reports/fixture"; root.mkdir(parents=True)
        for n in range(129): (root / f"port-{n}").mkdir()
        config = self.home / "hub.conf"; config.write_text(f"[hub]\nreport_dir={root.parent}\n")
        self.assertTrue(tools.report_archive_usage(config, ["fixture"])["truncated"])
        config.write_text("[hub]\nreport_dir=relative\n")
        self.assertIn("unavailable", tools.report_archive_usage(config, ["fixture"])["inventory"])


if __name__ == "__main__":
    unittest.main()
