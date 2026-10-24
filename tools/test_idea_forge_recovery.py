"""Local Git regression fixtures for retained companion publication recovery.

These fixtures never contact GitHub, publish releases, or change live schedules.
"""
from copy import deepcopy
import json
import shutil
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

from tools import idea_forge as forge


class RetainedGamesRecoveryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.directory = self.root / "run"
        self.directory.mkdir()
        self.remote = self.root / "games.git"
        self.writer = self.root / "creator"
        self.git(self.root, "init", "--bare", "--initial-branch=main", str(self.remote))
        self.git(self.root, "clone", str(self.remote), str(self.writer))
        self.configure(self.writer)
        self.write(self.writer, "games/test-game/rules.rs", "old game\n")
        self.write(self.writer, "site/thumbs/test-game.png", "original capture\n")
        self.definition = {"slug": "test-game", "name": "Test Game", "binary": "test-game",
                           "kind": "cargo-package", "directory": "games/test-game"}
        self.catalog = {"source_repository": "fixture/engine", "target_repository": "fixture/games",
                        "source_revision": "e" * 40, "preserved_paths": [],
                        "files": [{"path": "games/test-game/rules.rs", "sha256": "a" * 64}]}
        self.write(self.writer, ".games-catalog.json", json.dumps(self.catalog))
        self.write(self.writer, ".release-games.json", json.dumps({"native_playables": [self.definition]}))
        self.write(self.writer, "distribution/validator.py", "old validator\n")
        self.commit(self.writer, "Companion baseline")
        self.git(self.writer, "push", "origin", "main")
        self.baseline = self.git(self.writer, "rev-parse", "HEAD")
        self.state = {"target_dir": str(self.root), "timeout": 10, "wait_timeout": 0.05,
                      "idea": {"slug": "test-game", "title": "Test Game"},
                      "games_root": str(self.writer), "games_repository": "fixture/games",
                      "engine_repository": "fixture/engine", "engine_head": "e" * 40,
                      "verified_code": "verified-engine-digest", "branch": "idea-forge/fixture",
                      "completed": ["setup", "engine-ci"],
                      "ci": {"engine-checks": {"head": "e" * 40, "run": 10},
                             "generated-game-linux-windows": {"head": "e" * 40, "run": 11}},
                      "dispatched": ["generated-game-linux-windows"]}
        self.worker = forge.Forge(self.directory, self.state)
        self.games = self.worker.games
        self.git(self.writer, "worktree", "add", "-b", self.state["branch"], str(self.games), "origin/main")
        self.configure(self.games)
        self.write(self.games, "games/test-game/rules.rs", "verified generated game\n")
        self.commit(self.games, "Generated candidate")
        self.candidate = self.git(self.games, "rev-parse", "HEAD")
        self.state["games_head"] = self.candidate
        self.engine_evidence = deepcopy({key: self.state[key] for key in ("engine_head", "verified_code")})
        self.engine_ci = deepcopy(self.state["ci"])
        self.addCleanup(patch.stopall)
        patch.object(forge, "github_repository", return_value="fixture/games").start()
        patch.object(forge.time, "sleep", return_value=None).start()
        self.worker.save()

    @staticmethod
    def git(root, *args):
        return subprocess.run(["git", "-C", str(root), *args], check=True,
                              capture_output=True, text=True).stdout.strip()

    @staticmethod
    def write(root, relative, text):
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        return path

    def configure(self, root):
        self.git(root, "config", "user.name", "Fixture")
        self.git(root, "config", "user.email", "fixture@example.invalid")

    def commit(self, root, message):
        self.git(root, "add", "--all")
        self.git(root, "commit", "-qm", message)

    def advance_main(self, path="distribution/validator.py", text="fixed validator\n"):
        self.git(self.writer, "fetch", "origin", "main")
        self.git(self.writer, "merge", "--ff-only", "origin/main")
        self.write(self.writer, path, text)
        self.commit(self.writer, "Independent companion update")
        self.git(self.writer, "push", "origin", "main")
        return self.git(self.writer, "rev-parse", "HEAD")

    def public_head(self):
        return self.git(self.writer, "ls-remote", "origin", "refs/heads/main").split()[0]

    def assert_engine_evidence_preserved(self):
        for key, value in self.engine_evidence.items():
            self.assertEqual(self.state[key], value)
        for label, value in self.engine_ci.items():
            self.assertEqual(self.state["ci"][label], value)
        self.assertIn("generated-game-linux-windows", self.state["dispatched"])

    def test_clean_retained_export_integrates_new_baseline_and_is_repeatable(self):
        newer = self.advance_main()
        self.worker.integrate_games()
        head = self.git(self.games, "rev-parse", "HEAD")
        self.assertTrue(forge.git_ancestor(self.games, newer, head))
        self.assertTrue(forge.git_ancestor(self.games, self.candidate, head))
        self.assertEqual((self.games / "games/test-game/rules.rs").read_text(), "verified generated game\n")
        self.assertEqual((self.games / "distribution/validator.py").read_text(), "fixed validator\n")
        self.assertEqual(self.public_head(), newer)
        self.worker.integrate_games()
        self.assertEqual(self.git(self.games, "rev-parse", "HEAD"), head)
        self.assert_engine_evidence_preserved()

    def test_absent_private_worktree_is_created_at_fetched_main(self):
        self.git(self.writer, "worktree", "remove", str(self.games))
        self.git(self.writer, "branch", "-D", self.state["branch"])
        self.state.pop("games_head")
        newer = self.advance_main()
        self.worker.integrate_games()
        self.assertEqual(self.git(self.games, "rev-parse", "HEAD"), newer)
        self.assertEqual(self.git(self.games, "branch", "--show-current"), self.state["branch"])
        self.assert_engine_evidence_preserved()

    def test_export_uses_integration_before_reading_or_replacing_catalog(self):
        self.worker.validate_wiki = Mock(return_value={})
        self.worker.integrate_games = Mock(side_effect=forge.ForgeError("fixture integration blocker"))
        before = (self.games / ".games-catalog.json").read_bytes()
        with patch.object(self.worker, "command") as command:
            with self.assertRaisesRegex(forge.ForgeError, "fixture integration blocker"):
                self.worker.export()
        self.worker.integrate_games.assert_called_once()
        command.assert_not_called()
        self.assertEqual((self.games / ".games-catalog.json").read_bytes(), before)

    def test_dirty_owned_files_are_checkpointed_without_losing_bytes(self):
        owned = {"games/test-game/rules.rs": "generated repairs\n",
                 "games/test-game/new.rs": "new generated file\n",
                 ".games-catalog.json": json.dumps(dict(self.catalog, generated=True)),
                 ".release-games.json": json.dumps({"native_playables": [self.definition], "generated": True}),
                 "site/thumbs/test-game.png": "new generated capture\n"}
        for path, text in owned.items():
            self.write(self.games, path, text)
        self.git(self.games, "add", "--", "games/test-game/rules.rs")
        newer = self.advance_main()
        self.worker.integrate_games()
        self.assertTrue(forge.git_ancestor(self.games, newer, "HEAD"))
        self.assertEqual(self.git(self.games, "status", "--porcelain"), "")
        for path, text in owned.items():
            self.assertEqual((self.games / path).read_text(), text)
        self.assert_engine_evidence_preserved()

    def test_unrelated_staged_unstaged_and_untracked_changes_stop_before_checkpoint(self):
        self.write(self.games, "distribution/validator.py", "creator's unstaged validator\n")
        self.write(self.games, "creator-staged.txt", "creator's staged work\n")
        self.git(self.games, "add", "--", "creator-staged.txt")
        self.write(self.games, "creator-untracked.txt", "creator's untracked work\n")
        self.write(self.games, "games/test-game/rules.rs", "generated but not checkpointed\n")
        status = self.git(self.games, "status", "--porcelain")
        staged = self.git(self.games, "diff", "--cached")
        head = self.git(self.games, "rev-parse", "HEAD")
        self.advance_main()
        with self.assertRaises(forge.ForgeError):
            self.worker.integrate_games()
        self.assertEqual(self.git(self.games, "rev-parse", "HEAD"), head)
        self.assertEqual(self.git(self.games, "status", "--porcelain"), status)
        self.assertEqual(self.git(self.games, "diff", "--cached"), staged)
        self.assertEqual((self.games / "games/test-game/rules.rs").read_text(), "generated but not checkpointed\n")

    def test_unrelated_index_edit_hidden_by_restored_worktree_is_preserved(self):
        original = (self.games / "distribution/validator.py").read_text()
        self.write(self.games, "distribution/validator.py", "creator staged validator\n")
        self.git(self.games, "add", "--", "distribution/validator.py")
        self.write(self.games, "distribution/validator.py", original)
        # The total worktree-vs-HEAD diff is empty, but the staged author work exists.
        self.assertEqual(self.git(self.games, "diff", "HEAD", "--name-only"), "")
        staged = self.git(self.games, "diff", "--cached")
        status = self.git(self.games, "status", "--porcelain")
        with self.assertRaises(forge.ForgeError):
            self.worker.integrate_games()
        self.assertEqual(self.git(self.games, "status", "--porcelain"), status)
        self.assertEqual(self.git(self.games, "diff", "--cached"), staged)
        self.assertEqual((self.games / "distribution/validator.py").read_text(), original)

    def test_owned_staged_and_worktree_versions_require_explicit_reconciliation(self):
        self.write(self.games, "games/test-game/rules.rs", "generated staged version\n")
        self.git(self.games, "add", "--", "games/test-game/rules.rs")
        self.write(self.games, "games/test-game/rules.rs", "different generated working version\n")
        staged = self.git(self.games, "show", ":games/test-game/rules.rs")
        head = self.git(self.games, "rev-parse", "HEAD")
        with self.assertRaises(forge.ForgeError):
            self.worker.integrate_games()
        self.assertEqual(self.git(self.games, "rev-parse", "HEAD"), head)
        self.assertEqual(self.git(self.games, "show", ":games/test-game/rules.rs"), staged)
        self.assertEqual((self.games / "games/test-game/rules.rs").read_text(), "different generated working version\n")

    def test_merge_conflict_is_retained_and_explicit_committed_resolution_resumes(self):
        newer = self.advance_main("games/test-game/rules.rs", "independent conflicting game rules\n")
        with self.assertRaises(forge.ForgeError):
            self.worker.integrate_games()
        self.assertTrue(self.git(self.games, "diff", "--name-only", "--diff-filter=U"))
        self.assertTrue(self.state.get("games_export_integration"))
        conflict = (self.games / "games/test-game/rules.rs").read_text()
        with self.assertRaises(forge.ForgeError):
            self.worker.integrate_games()
        self.assertEqual((self.games / "games/test-game/rules.rs").read_text(), conflict)
        self.write(self.games, "games/test-game/rules.rs", "explicitly reconciled game rules\n")
        self.commit(self.games, "Resolve retained companion conflict")
        self.worker.integrate_games()
        self.assertTrue(forge.git_ancestor(self.games, newer, "HEAD"))
        self.assertTrue(forge.git_ancestor(self.games, self.candidate, "HEAD"))
        self.assertEqual(self.git(self.games, "status", "--porcelain"), "")
        self.assert_engine_evidence_preserved()

    def test_interruption_after_merge_resumes_from_serialized_state(self):
        newer = self.advance_main()
        command = self.worker.command
        interrupted = False
        def after_merge(argv, **options):
            nonlocal interrupted
            result = command(argv, **options)
            if "merge" in list(map(str, argv)) and not interrupted:
                interrupted = True
                raise forge.ForgeError("fixture interruption after successful merge")
            return result
        with patch.object(self.worker, "command", side_effect=after_merge):
            with self.assertRaises(forge.ForgeError):
                self.worker.integrate_games()
        head = self.git(self.games, "rev-parse", "HEAD")
        self.assertTrue(forge.git_ancestor(self.games, newer, head))
        self.assertTrue(forge.load_json(self.directory / "state.json").get("games_export_integration"))
        self.worker = forge.Forge(self.directory, forge.load_json(self.directory / "state.json"))
        self.state = self.worker.state
        self.worker.integrate_games()
        self.assertEqual(self.git(self.games, "rev-parse", "HEAD"), head)
        self.assert_engine_evidence_preserved()

    def test_interruption_after_checkpoint_does_not_repeat_checkpoint(self):
        self.write(self.games, "games/test-game/rules.rs", "dirty generated repairs\n")
        self.advance_main()
        command = self.worker.command
        def after_commit(argv, **options):
            result = command(argv, **options)
            if "commit" in list(map(str, argv)):
                raise forge.ForgeError("fixture interruption after successful checkpoint")
            return result
        with patch.object(self.worker, "command", side_effect=after_commit):
            with self.assertRaises(forge.ForgeError):
                self.worker.integrate_games()
        checkpoint = self.git(self.games, "rev-parse", "HEAD")
        self.assertNotEqual(checkpoint, self.candidate)
        self.worker = forge.Forge(self.directory, forge.load_json(self.directory / "state.json"))
        self.state = self.worker.state
        self.worker.integrate_games()
        self.assertTrue(forge.git_ancestor(self.games, checkpoint, "HEAD"))
        self.assertEqual(self.git(self.games, "rev-list", "--count", self.candidate + "..HEAD", "--", "games/test-game"), "1")
        self.assertEqual((self.games / "games/test-game/rules.rs").read_text(), "dirty generated repairs\n")
        self.assert_engine_evidence_preserved()

    def test_wrong_branch_and_wrong_repository_are_rejected_without_changes(self):
        self.git(self.games, "switch", "-c", "another-creators-branch")
        head = self.git(self.games, "rev-parse", "HEAD")
        with self.assertRaises(forge.ForgeError):
            self.worker.integrate_games()
        self.assertEqual(self.git(self.games, "rev-parse", "HEAD"), head)
        self.git(self.games, "switch", self.state["branch"])
        other = self.root / "other-repository"
        self.git(self.root, "init", "--initial-branch=main", str(other))
        self.configure(other)
        self.write(other, "unrelated.txt", "another creator's repository\n")
        self.commit(other, "Unrelated repository")
        self.state["games_root"] = str(other)
        with self.assertRaises(forge.ForgeError):
            self.worker.integrate_games()
        self.assertEqual(self.git(self.games, "rev-parse", "HEAD"), head)

    def test_origin_identity_mismatch_stops_before_git_mutation(self):
        self.advance_main()
        head = self.git(self.games, "rev-parse", "HEAD")
        def repository(root):
            return "fixture/another-repository" if Path(root) == self.games else "fixture/games"
        with patch.object(forge, "github_repository", side_effect=repository):
            with patch.object(self.worker, "command") as command:
                with self.assertRaises(forge.ForgeError):
                    self.worker.integrate_games()
                command.assert_not_called()
        self.assertEqual(self.git(self.games, "rev-parse", "HEAD"), head)

    def test_nested_directory_and_creator_checkout_substitution_are_rejected(self):
        for substitute in (self.games / "games", self.writer):
            with self.subTest(path=str(substitute)):
                self.worker.games = substitute
                with patch.object(self.worker, "command") as command:
                    with self.assertRaises(forge.ForgeError):
                        self.worker.integrate_games()
                    command.assert_not_called()
        self.worker.games = self.games
        self.assertEqual(self.git(self.games, "rev-parse", "HEAD"), self.candidate)

    def test_symlink_private_worktree_is_rejected_before_modification(self):
        if not hasattr(Path, "symlink_to"):
            self.skipTest("Host has no symlink support")
        alias = self.directory / "games-alias"
        try:
            alias.symlink_to(self.games, target_is_directory=True)
        except OSError:
            self.skipTest("Host does not permit symlinks")
        self.worker.games = alias
        head = self.git(self.games, "rev-parse", "HEAD")
        with self.assertRaises(forge.ForgeError):
            self.worker.integrate_games()
        self.assertEqual(self.git(self.games, "rev-parse", "HEAD"), head)

    def mock_publication_ci(self):
        self.worker.assert_engine_gates = Mock()
        self.worker.assert_source_gates = Mock()
        self.dispatches = []
        self.pushes = []
        original = self.worker.command
        def command(argv, **options):
            argv = list(map(str, argv))
            if argv[:3] == ["gh", "workflow", "run"]:
                self.dispatches.append(self.git(self.games, "rev-parse", "HEAD"))
                return self.directory / "mock-dispatch.log"
            if argv[:3] == ["git", "push", "origin"] and argv[-1].endswith(":main"):
                self.pushes.append(argv[-1])
            return original(argv, **options)
        def api(endpoint):
            head = self.dispatches[-1] if self.dispatches else self.candidate
            return {"workflow_runs": [{"head_sha": head, "event": "workflow_dispatch", "status": "completed",
                                        "conclusion": "success", "id": len(self.dispatches) + 20,
                                        "html_url": "https://example.invalid/review", "updated_at": "2026-10-23T00:00:00Z"}]}
        self.worker.command = Mock(side_effect=command)
        self.worker.api = Mock(side_effect=api)

    def test_changed_candidate_dispatches_new_installer_and_preserves_engine_gates(self):
        self.mock_publication_ci()
        self.worker.installer_review()
        self.assertEqual(self.dispatches, [self.candidate])
        self.advance_main()
        self.worker.publish_games()
        new_head = self.state["games_head"]
        self.assertNotEqual(new_head, self.candidate)
        self.assertEqual(self.dispatches, [self.candidate, new_head])
        self.assertEqual(self.state["ci"]["windows-installer-review"]["head"], new_head)
        self.assertEqual(self.public_head(), new_head)
        self.assert_engine_evidence_preserved()

    def test_successful_push_with_uncertain_response_and_later_main_does_not_republish(self):
        self.mock_publication_ci()
        self.worker.installer_review()
        self.advance_main()
        command = self.worker.command
        def lose_ack(argv, **options):
            result = command(argv, **options)
            values = list(map(str, argv))
            if values[:3] == ["git", "push", "origin"] and values[-1].endswith(":main"):
                raise forge.ForgeError("fixture lost push response")
            return result
        with patch.object(self.worker, "command", side_effect=lose_ack):
            with self.assertRaises(forge.ForgeError):
                self.worker.publish_games()
        candidate = self.state["games_head"]
        self.assertEqual(self.public_head(), candidate)
        newer = self.advance_main("another-game.txt", "subsequent creator publication\n")
        self.worker.publish_games()
        self.assertEqual(self.public_head(), newer)
        self.assertEqual(self.state["games_head"], candidate)
        self.assertEqual(len(self.pushes), 1)
        self.assertEqual(self.dispatches, [self.candidate, candidate])

    def test_publication_merge_interruption_reuses_serialized_candidate(self):
        self.mock_publication_ci()
        self.worker.installer_review()
        newer = self.advance_main()
        command = self.worker.command
        def after_merge(argv, **options):
            result = command(argv, **options)
            if "merge" in list(map(str, argv)):
                raise forge.ForgeError("fixture interrupted publication merge")
            return result
        with patch.object(self.worker, "command", side_effect=after_merge):
            with self.assertRaises(forge.ForgeError):
                self.worker.publish_games()
        head = self.git(self.games, "rev-parse", "HEAD")
        self.assertTrue(forge.git_ancestor(self.games, newer, head))
        self.worker.state = forge.load_json(self.directory / "state.json")
        self.state = self.worker.state
        self.worker.publish_games()
        self.assertEqual(self.public_head(), head)
        self.assertEqual(self.state["games_head"], head)
        self.assertEqual(self.dispatches, [self.candidate, head])
        self.assert_engine_evidence_preserved()

    def test_interrupted_installer_receipt_resumes_without_duplicate_dispatch(self):
        self.mock_publication_ci()
        self.worker.installer_review()
        newer = self.advance_main()
        api = self.worker.api
        with patch.object(self.worker, "api", side_effect=forge.ForgeError("fixture lost review response")):
            with self.assertRaises(forge.ForgeError):
                self.worker.publish_games()
        head = self.state["games_head"]
        self.assertNotEqual(head, self.candidate)
        self.assertEqual(self.dispatches, [self.candidate, head])
        self.assertEqual(self.public_head(), newer)
        self.worker.api = api
        self.worker.state = forge.load_json(self.directory / "state.json")
        self.state = self.worker.state
        self.worker.publish_games()
        self.assertEqual(self.dispatches, [self.candidate, head])
        self.assertEqual(self.public_head(), head)

    def test_main_advance_during_review_requires_fresh_candidate_and_review(self):
        self.mock_publication_ci()
        self.worker.installer_review()
        self.advance_main()
        command = self.worker.command
        race_head = None
        def advance_during_review(argv, **options):
            nonlocal race_head
            result = command(argv, **options)
            if list(map(str, argv))[:3] == ["gh", "workflow", "run"]:
                race_head = self.advance_main("another-game.txt", "concurrent game publication\n")
            return result
        with patch.object(self.worker, "command", side_effect=advance_during_review):
            with self.assertRaises(forge.ForgeError):
                self.worker.publish_games()
        reviewed = self.state["games_head"]
        self.assertEqual(self.public_head(), race_head)
        self.assertEqual(self.pushes, [])
        self.worker.publish_games()
        head = self.state["games_head"]
        self.assertNotEqual(head, reviewed)
        self.assertEqual(self.dispatches, [self.candidate, reviewed, head])
        self.assertEqual(self.public_head(), head)
        self.assert_engine_evidence_preserved()

    def test_changed_game_cannot_reuse_installer_evidence(self):
        self.mock_publication_ci()
        self.worker.installer_review()
        remote = self.advance_main("games/test-game/new.rs", "unreviewed changed game\n")
        with self.assertRaises(forge.ForgeError):
            self.worker.publish_games()
        self.assertEqual(self.public_head(), remote)
        self.assertEqual(self.dispatches, [self.candidate])
        self.assertEqual(self.pushes, [])

    def test_rebuild_archives_stale_publication_identity_but_retains_export_checkpoint(self):
        snapshot = self.worker.games_snapshot(self.candidate)
        pending = {"source_head": self.candidate}
        self.state.update(games_source_snapshot=deepcopy(snapshot), games_integration_pending=self.candidate,
                          games_export_integration=deepcopy(pending))
        idea = deepcopy(self.state["idea"])
        self.worker.rebuild()
        self.assertNotIn("games_source_snapshot", self.state)
        self.assertNotIn("games_integration_pending", self.state)
        self.assertEqual(self.state["games_export_integration"], pending)
        self.assertEqual(self.state["verification_history"][-1]["games_source_snapshot"], snapshot)
        self.assertEqual(self.state["verification_history"][-1]["games_integration_pending"], self.candidate)
        self.assertEqual(self.state["idea"], idea)
        self.assertEqual((self.games / "games/test-game/rules.rs").read_text(), "verified generated game\n")

    def test_full_export_recovery_uses_actual_exporter_and_publishes_exact_candidate(self):
        from tools.test_idea_forge import wiki_fixture

        engine_remote = self.root / "engine.git"
        self.git(self.root, "init", "--bare", "--initial-branch=main", str(engine_remote))
        self.git(self.root, "init", "--initial-branch=main", str(self.worker.engine))
        self.configure(self.worker.engine)
        self.git(self.worker.engine, "remote", "add", "origin", str(engine_remote))
        exporter = self.worker.engine / "scripts/publish_games.py"
        exporter.parent.mkdir()
        shutil.copyfile(forge.ROOT / "scripts/publish_games.py", exporter)
        game = self.worker.game
        self.write(game, "rules.rs", "verified generated game\n")
        self.write(game, "Cargo.toml", '[package]\nname = "test-game"\nversion = "0.1.0"\n')
        self.write(game, "game.project.json", '{"id":"test-game"}\n')
        wiki = wiki_fixture(game)
        collections = {"games": [{"source": "games/test-game", "destination": "test-game"}]}
        for category in ("prototypes", "tests", "demos"):
            self.write(self.worker.engine, "fixtures/" + category + ".txt", category + " fixture\n")
            collections[category] = [{"source": "fixtures/" + category + ".txt", "destination": "."}]
        forge.atomic_json(self.worker.engine / "games-publish.json", {
            "version": 2, "source_repository": "fixture/engine", "target_repository": "fixture/games",
            "collections": collections,
            "playables": [{"slug": "test-game", "name": "Test Game", "entry": "games/test-game/rules.rs",
                           "files": ["games/test-game"], "arguments": []}]})
        self.commit(self.worker.engine, "Frozen verified fixture source")
        source = self.git(self.worker.engine, "rev-parse", "HEAD")
        self.git(self.worker.engine, "push", "origin", "main")
        self.state.update(engine_head=source, verified_code=forge.code_digest(self.worker.engine),
                          screenshot=str(game / wiki["gallery"][0]["image"]))
        for receipt in self.state["ci"].values():
            receipt["head"] = source
        self.engine_evidence = deepcopy({key: self.state[key] for key in ("engine_head", "verified_code")})
        self.engine_ci = deepcopy(self.state["ci"])
        # Simulate a retained partial export. All copied bytes remain dirty until
        # integration checkpoints them; the actual exporter must accept them.
        for path in game.rglob("*"):
            if path.is_file():
                target = self.games / "games/test-game" / path.relative_to(game)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, target)
        catalog = dict(self.catalog, source_revision=source)
        forge.atomic_json(self.games / ".games-catalog.json", catalog)
        self.state["games_source_snapshot"] = {"stale": "prior publication identity"}
        self.state["games_integration_pending"] = self.candidate
        self.state["ci"]["windows-installer-review"] = {"head": self.candidate, "run": 19}
        self.state["dispatched"].append("windows-installer-review")
        # Companion validation fixtures execute through the ordinary export gate.
        # They require the newer baseline marker and check actual exported files.
        assertion = ("from pathlib import Path\n"
                     "import json\n"
                     "root = Path(__file__).resolve().parents[1]\n"
                     "assert (root / 'distribution/validator.py').read_text() == 'fixed validator\\n'\n"
                     "assert (root / 'games/test-game/rules.rs').read_text() == 'verified generated game\\n'\n"
                     "assert json.loads((root / '.games-catalog.json').read_text())['source_revision'] == "
                     + repr(source) + "\n")
        self.write(self.writer, "distribution/check_catalog.py", assertion)
        for category in ("distribution", "site"):
            self.write(self.writer, category + "/test_fixture.py", assertion +
                       "import unittest\nclass ExportTests(unittest.TestCase):\n"
                       "    def test_exported_wiki(self):\n"
                       "        self.assertEqual(json.loads((root / 'games/test-game/wiki.json').read_text())['version'], 1)\n")
        self.write(self.writer, ".gitignore", "__pycache__/\n")
        self.write(self.writer, "creator-notes.txt", "independent creator work survives export\n")
        newer = self.advance_main()
        original = {path.relative_to(game).as_posix(): path.read_bytes()
                    for path in game.rglob("*") if path.is_file()}
        self.worker.export()
        head = self.state["games_head"]
        self.assertTrue(forge.git_ancestor(self.games, newer, head))
        self.assertTrue(forge.git_ancestor(self.games, self.candidate, head))
        self.assertEqual(self.git(self.games, "status", "--porcelain"), "")
        for relative, data in original.items():
            self.assertEqual((self.games / "games/test-game" / relative).read_bytes(), data)
        self.assertEqual((self.games / "creator-notes.txt").read_text(), "independent creator work survives export\n")
        self.assertEqual(self.state["games_source_snapshot"], self.worker.games_snapshot(head))
        self.assertNotIn("games_integration_pending", self.state)
        self.assertNotIn("windows-installer-review", self.state["ci"])
        self.assertNotIn("windows-installer-review", self.state["dispatched"])
        self.assert_engine_evidence_preserved()
        labels = [item["label"] for item in self.state["commands"]]
        self.assertIn("export-game", labels)
        for label in ("catalog-check", "distribution-tests", "site-tests"):
            self.assertIn(label, labels)
        # Retain actual source-gate validation, with only the GitHub observation
        # mocked. Installer dispatch and main publication still use real local Git.
        self.mock_publication_ci()
        self.worker.assert_engine_gates = forge.Forge.assert_engine_gates.__get__(self.worker)
        self.worker.assert_source_gates = forge.Forge.assert_source_gates.__get__(self.worker)
        api = self.worker.api
        def exact_receipt(endpoint):
            if "/actions/runs/" in endpoint:
                run_id = int(endpoint.rsplit("/", 1)[1])
                receipt = next(value for value in self.state["ci"].values() if value["run"] == run_id)
                return {"head_sha": receipt["head"], "status": "completed", "conclusion": "success"}
            return api(endpoint)
        self.worker.api = Mock(side_effect=exact_receipt)
        self.worker.installer_review()
        self.worker.publish_games()
        self.assertEqual(self.dispatches, [head])
        self.assertEqual(self.public_head(), head)
        self.assertEqual(self.git(self.worker.engine, "rev-parse", "HEAD"), source)
        self.assert_engine_evidence_preserved()

    def test_large_literal_path_list_stages_only_selected_files(self):
        selected = {"games/test-game/file[" + str(index).zfill(4) + "]-" + "x" * 70 + ".txt"
                    for index in range(400)}
        self.assertGreater(sum(len(path) + 1 for path in selected), 32_768)
        for path in selected:
            self.write(self.games, path, "selected generated file\n")
        self.write(self.games, "games/test-game/file0.txt", "unselected glob-like filename\n")
        self.write(self.games, "creator-staged.txt", "creator staged bytes\n")
        self.git(self.games, "add", "--", "creator-staged.txt")
        self.write(self.games, "distribution/validator.py", "creator unstaged validator\n")
        staged = self.git(self.games, "show", ":creator-staged.txt")
        command = self.worker.command
        with patch.object(self.worker, "command", wraps=command) as observed:
            self.worker.stage_games_changes(selected)
        argv = list(map(str, observed.call_args.args[0]))
        self.assertIn("--pathspec-from-file=-", argv)
        self.assertIn("--pathspec-file-nul", argv)
        self.assertLess(sum(map(len, argv)), 1000)
        self.assertGreater(len(observed.call_args.kwargs["stdin"]), 32_768)
        staged_paths = set(filter(None, self.git(self.games, "diff", "--cached", "--name-only", "-z").split("\0")))
        self.assertEqual(staged_paths, selected | {"creator-staged.txt"})
        self.assertEqual(self.git(self.games, "show", ":creator-staged.txt"), staged)
        self.assertEqual((self.games / "creator-staged.txt").read_text(), "creator staged bytes\n")
        self.assertEqual((self.games / "distribution/validator.py").read_text(), "creator unstaged validator\n")
        self.assertEqual((self.games / "games/test-game/file0.txt").read_text(), "unselected glob-like filename\n")

    def test_run_lock_prevents_duplicate_attempts(self):
        with forge.run_lock(self.directory):
            with self.assertRaises(forge.ForgeError):
                with forge.run_lock(self.directory):
                    self.fail("A duplicate supervisor entered the retained run")
        self.assertFalse((self.directory / "supervisor.lock").exists())


if __name__ == "__main__":
    unittest.main()
