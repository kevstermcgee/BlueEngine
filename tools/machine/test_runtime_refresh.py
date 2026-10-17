"""Proposed refresh commands run against scratch Git repositories, never live source."""
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
import unittest


RUNTIME = "%h/BlueEngine-idea-forge-native"
DROPIN = Path(__file__).parent / "systemd/ideaforge-daily.service.d/zz-refresh-runtime.conf"


@unittest.skipUnless(shutil.which("git"), "Runtime refresh fixtures require Git")
class RuntimeRefreshTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.remote, self.author, self.runtime = [self.root / name for name in ("remote.git", "author", "runtime")]
        self.env = {**os.environ, "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_SYSTEM": os.devnull,
                    "GIT_TERMINAL_PROMPT": "0"}
        self.run_git("init", "--bare", "--initial-branch=main", str(self.remote))
        self.run_git("init", "--initial-branch=main", str(self.author))
        self.identity(self.author)
        (self.author / "tools").mkdir()
        (self.author / "tools/idea_forge.py").write_text("original supervisor\n")
        self.commit(self.author, "Initial supervisor")
        self.run_git("-C", str(self.author), "remote", "add", "origin", str(self.remote))
        self.run_git("-C", str(self.author), "push", "origin", "main")
        self.run_git("clone", "--branch", "main", str(self.remote), str(self.runtime))
        self.identity(self.runtime)
        self.base = self.head(self.runtime)
        (self.author / "tools/idea_forge.py").write_text("updated supervisor\n")
        (self.author / "future.py").write_text("upstream file\n")
        self.commit(self.author, "Update scheduled supervisor")
        self.run_git("-C", str(self.author), "push", "origin", "main")
        self.updated = self.head(self.author)

    def run_git(self, *args):
        return subprocess.run(["/usr/bin/git", *args], env=self.env, check=True,
                              text=True, capture_output=True, timeout=20).stdout.strip()

    def identity(self, repository):
        self.run_git("-C", str(repository), "config", "user.name", "Fixture")
        self.run_git("-C", str(repository), "config", "user.email", "fixture@example.invalid")

    def commit(self, repository, message):
        self.run_git("-C", str(repository), "add", "--all")
        self.run_git("-C", str(repository), "commit", "-m", message)

    def head(self, repository):
        return self.run_git("-C", str(repository), "rev-parse", "HEAD")

    def refresh(self):
        commands = [shlex.split(line.split("=", 1)[1]) for line in DROPIN.read_text().splitlines()
                    if line.startswith("ExecStartPre=")]
        # Verify scope before substituting scratch paths: tests can never run a
        # changed command against the actual runtime checkout accidentally.
        expected = [["/usr/bin/git", "-C", RUNTIME, "diff", "--quiet"],
                    ["/usr/bin/git", "-C", RUNTIME, "diff", "--cached", "--quiet"],
                    ["/usr/bin/timeout", "--kill-after=10s", "180s", "/usr/bin/git", "-C", RUNTIME, "fetch", "origin", "main"],
                    ["/usr/bin/git", "-C", RUNTIME, "merge", "--ff-only", "origin/main"]]
        self.assertEqual(commands, expected)
        for command in commands:
            command = [str(self.runtime) if value == RUNTIME else value for value in command]
            result = subprocess.run(command, env=self.env, text=True, capture_output=True, timeout=20)
            if result.returncode:
                return result.returncode
        return 0

    def test_clean_checkout_fast_forwards_to_updated_supervisor(self):
        self.assertEqual(self.refresh(), 0)
        self.assertEqual(self.head(self.runtime), self.updated)
        self.assertEqual((self.runtime / "tools/idea_forge.py").read_text(), "updated supervisor\n")
        self.assertEqual(self.run_git("-C", str(self.runtime), "status", "--porcelain"), "")

    def test_unstaged_work_blocks_refresh_without_overwrite_or_fetch(self):
        source = self.runtime / "tools/idea_forge.py"
        source.write_text("valuable unstaged work\n")
        self.assertNotEqual(self.refresh(), 0)
        self.assertEqual(source.read_text(), "valuable unstaged work\n")
        self.assertEqual(self.head(self.runtime), self.base)
        self.assertEqual(self.run_git("-C", str(self.runtime), "rev-parse", "origin/main"), self.base)

    def test_staged_work_blocks_refresh_and_preserves_index(self):
        source = self.runtime / "tools/idea_forge.py"
        source.write_text("valuable staged work\n")
        self.run_git("-C", str(self.runtime), "add", "tools/idea_forge.py")
        index = self.run_git("-C", str(self.runtime), "diff", "--cached")
        self.assertNotEqual(self.refresh(), 0)
        self.assertEqual(source.read_text(), "valuable staged work\n")
        self.assertEqual(self.run_git("-C", str(self.runtime), "diff", "--cached"), index)
        self.assertEqual(self.head(self.runtime), self.base)
        self.assertEqual(self.run_git("-C", str(self.runtime), "rev-parse", "origin/main"), self.base)

    def test_diverged_checkout_is_preserved_without_merge_or_reset(self):
        note = self.runtime / "local-note.txt"
        note.write_text("valuable committed work\n")
        self.commit(self.runtime, "Local development")
        local_head = self.head(self.runtime)
        self.assertNotEqual(self.refresh(), 0)
        self.assertEqual(self.head(self.runtime), local_head)
        self.assertEqual(note.read_text(), "valuable committed work\n")
        self.assertEqual((self.runtime / "tools/idea_forge.py").read_text(), "original supervisor\n")
        self.assertEqual(self.run_git("-C", str(self.runtime), "status", "--porcelain"), "")

    def test_git_preserves_untracked_file_that_overlaps_new_upstream_source(self):
        untracked = self.runtime / "future.py"
        untracked.write_text("valuable untracked work\n")
        self.assertNotEqual(self.refresh(), 0)
        self.assertEqual(untracked.read_text(), "valuable untracked work\n")
        self.assertEqual(self.head(self.runtime), self.base)
        self.assertEqual((self.runtime / "tools/idea_forge.py").read_text(), "original supervisor\n")


if __name__ == "__main__":
    unittest.main()
