"""Behavioral gates for the IdeaForge supervisor, without paid AI calls or real publication."""
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
import argparse
from datetime import datetime, timezone
from unittest.mock import patch

from tools import idea_forge as forge


def idea():
    return {"dimension": "2d", "title": "Rule Relay", "slug": "rule-relay", "genre": "puzzle",
            "mechanic": "Move one physical rule between two machines; the donor loses it immediately.",
            "player_actions": ["Select a donor", "Select a recipient"],
            "rules": ["Only one machine owns each rule", "Transfers consume a turn", "Doors need two rules"],
            "win_condition": "Open the exit", "lose_condition": "Run out of transfers",
            "why_fun": "Each improvement creates a new weakness elsewhere.",
            "prototype": "Three machines, two rules and one exit.",
            "novelty_check": "Rule transfer differs from resource collection; prior art still needs comparison.",
            "playtest_risk": "Players must see which rule the donor lost.", "story": None}


def finding():
    return {"area": "ui", "severity": "medium", "observed": "A long rule label covered the transfer button.",
            "reproduction": "Select the machine with the longest rule label at the default window size.",
            "workaround": "Reserve the shared HUD margins and wrap the label.",
            "recommendation": "Provide a measured text layout helper for rule labels.",
            "keywords": ["rule", "label", "wrapping"]}


class SupervisorTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.engine = self.directory / "engine"
        self.engine.mkdir()
        subprocess.run(["git", "init", "-q", str(self.engine)], check=True)
        subprocess.run(["git", "-C", str(self.engine), "config", "user.name", "Fixture"], check=True)
        subprocess.run(["git", "-C", str(self.engine), "config", "user.email", "fixture@example.invalid"], check=True)
        (self.engine / ".gitignore").write_text(".blue-check/\ndist/\n")
        (self.engine / "source.rs").write_text("authoritative rules\n")
        (self.engine / "templates").mkdir()
        for name in ("ship", "check", "project"):
            (self.engine / "templates" / ("game_" + name + ".py")).write_text("# canonical " + name)
        subprocess.run(["git", "-C", str(self.engine), "add", "."], check=True)
        subprocess.run(["git", "-C", str(self.engine), "commit", "-qm", "Fixture"], check=True)
        self.state = {"id": "fixture", "created": "2026-10-08T00:00:00+00:00", "status": "running",
                      "source_root": str(self.engine), "target_dir": str(self.directory),
                      "timeout": 10, "wait_timeout": 1, "story": False, "brief": "test", "agent": "codex",
                      "repairs": 0, "publish": False, "min_free_gib": 0, "idea": idea()}
        self.worker = forge.Forge(self.directory, self.state)

    def native_game(self, smoke=True):
        game = self.worker.game
        (game / "scripts").mkdir(parents=True)
        (game / "src").mkdir()
        (self.engine / "templates").mkdir(exist_ok=True)
        for name in ("ship", "check", "project"):
            text = "# canonical " + name
            (game / "scripts" / (name + ".py")).write_text(text)
            (self.engine / "templates" / ("game_" + name + ".py")).write_text(text)
        for name in ("Cargo.toml", "src/lib.rs", "src/main.rs"):
            (game / name).write_text("fixture")
        forge.atomic_json(game / "game.project.json", {"targets": ["windows", "linux"], "presentation": "2d"})
        (game / "dist").mkdir()
        forge.atomic_json(game / "dist/ship.json", {"verified": {"smoke": smoke}})
        capture = game / ".blue-check/smoke/shot_00030.png"
        capture.parent.mkdir(parents=True)
        capture.write_bytes(b"\x89PNG\r\n\x1a\n" + b"\0\0\0\rIHDR" + struct.pack(">II", 800, 450) + b"\0" * 9)
        return game, capture

    def test_schema_rejects_path_traversal_and_missing_rules(self):
        concept = idea()
        forge.validate(concept, forge.IDEA_SCHEMA)
        concept["slug"] = "../engine"
        with self.assertRaises(forge.ForgeError):
            forge.validate(concept, forge.IDEA_SCHEMA)
        concept = idea()
        concept["rules"] = []
        with self.assertRaises(forge.ForgeError):
            forge.validate(concept, forge.IDEA_SCHEMA)

    def test_three_dimensional_requirement_blocks_a_flat_package(self):
        self.native_game()
        self.state["idea"]["dimension"] = "3d"
        with patch.object(self.worker, "command") as command:
            with self.assertRaisesRegex(forge.ForgeError, "requested dimension"):
                self.worker.verify_local()
            command.assert_not_called()

    def test_generation_cannot_substitute_a_2d_concept_for_a_3d_request(self):
        (self.engine / "games").mkdir()
        self.state["dimension"] = "3d"
        with patch.object(self.worker, "agent", return_value=idea()):
            with self.assertRaisesRegex(forge.ForgeError, "requested dimension"):
                self.worker.generate()
        self.assertFalse((self.directory / "idea.json").exists())

    def test_legacy_concept_input_still_builds_without_a_dimension_constraint(self):
        (self.engine / "games").mkdir()
        concept = idea()
        del concept["dimension"]
        supplied = self.directory / "supplied.json"
        forge.atomic_json(supplied, concept)
        self.state["idea_input"] = str(supplied)
        self.worker.generate()
        self.assertEqual(self.state["idea"], concept)

    def test_exact_mechanic_repetition_ignores_case_and_punctuation(self):
        first, second = idea(), idea()
        second["title"] = "A different title"
        second["mechanic"] = first["mechanic"].upper().replace(";", "!")
        self.assertEqual(forge.mechanic_key(first), forge.mechanic_key(second))

    def test_feedback_rejects_planted_credentials_without_echoing_them(self):
        record = finding()
        planted = "sk-proj-" + "x" * 70
        record["observed"] = "The terminal printed " + planted
        with self.assertRaises(forge.ForgeError) as raised:
            forge.public_findings([record])
        self.assertNotIn(planted, str(raised.exception))

    def test_a_second_supervisor_cannot_advance_the_same_run(self):
        with forge.run_lock(self.directory):
            with self.assertRaises(forge.ForgeError):
                with forge.run_lock(self.directory):
                    self.fail("A second supervisor acquired the run")
        self.assertFalse((self.directory / "supervisor.lock").exists())

    def test_code_binding_changes_for_rules_but_not_feedback(self):
        original = forge.code_digest(self.engine)
        path = self.engine / "docs/feedback/run.md"
        path.parent.mkdir(parents=True)
        path.write_text("delivery receipt")
        self.assertEqual(original, forge.code_digest(self.engine))
        (self.engine / "source.rs").write_text("different rules")
        self.assertNotEqual(original, forge.code_digest(self.engine))

    def test_old_capture_and_failed_smoke_cannot_certify_a_new_package(self):
        game, capture = self.native_game(smoke=False)
        with self.assertRaises(forge.ForgeError):
            forge.screenshot_from_package(game)
        forge.atomic_json(game / "dist/ship.json", {"verified": {"smoke": True}})
        os.utime(capture, (1, 1))
        with self.assertRaises(forge.ForgeError):
            forge.screenshot_from_package(game, not_before=10)

    def test_zero_executed_tests_cannot_reach_packaging(self):
        self.native_game()
        logfile = self.directory / "test.log"
        logfile.write_text("test result: ok. 0 passed; 0 failed; 3 ignored")
        with patch.object(self.worker, "command", return_value=logfile) as command:
            with self.assertRaisesRegex(forge.ForgeError, "no executed behavioral tests"):
                self.worker.verify_local()
        self.assertNotIn("native-package", [c.kwargs.get("label") for c in command.call_args_list])

    def test_modified_ship_script_cannot_fake_native_verification(self):
        game, _ = self.native_game()
        (game / "scripts/ship.py").write_text("print('all passed')")
        with patch.object(self.worker, "command") as command:
            with self.assertRaisesRegex(forge.ForgeError, "canonical"):
                self.worker.verify_local()
            command.assert_not_called()

    def test_untracked_workflow_changes_are_outside_the_worker_scope(self):
        self.native_game()
        workflow = self.engine / ".github/workflows/fake.yml"
        workflow.parent.mkdir(parents=True)
        workflow.write_text("fake gates")
        with self.assertRaisesRegex(forge.ForgeError, "outside its game scope"):
            self.worker.verify_local()

    def test_ai_findings_enter_feedback_and_the_validated_ledger_once(self):
        self.state["build_reports"] = [{"summary": "Built", "findings": [finding()]}]
        self.worker.feedback()
        self.worker.feedback()
        path = self.engine / self.state["feedback_path"]
        self.assertIn("Proposed engine improvement", path.read_text())
        ledger, malformed = forge.learn.load_ledger(self.engine / "docs/learning/ledger.jsonl")
        self.assertEqual(malformed, 0)
        self.assertEqual(len(ledger), 1)
        self.assertEqual(forge.learn.validate_entry(ledger[0]), [])
        self.assertEqual(ledger[0]["game"], "rule-relay")
        self.assertEqual(ledger[0]["tokens"], 0)

    def test_a_failed_build_retains_real_ai_feedback_and_does_not_publish(self):
        self.state["completed"] = ["setup", "generate"]
        self.state["build_reports"] = [{"summary": "Blocked", "findings": [finding()]}]
        with patch.object(self.worker, "build", side_effect=forge.ForgeError("Native input failed")):
            with self.assertRaises(forge.ForgeError):
                self.worker.run()
        saved = forge.load_json(self.directory / "state.json")
        self.assertEqual(saved["status"], "failed")
        self.assertNotIn("publish-games", saved["completed"])
        self.assertTrue((self.engine / saved["feedback_path"]).is_file())

    def test_changes_after_ci_prevent_a_publication_push(self):
        head = forge.git(self.engine, "rev-parse", "HEAD")
        self.state.update(engine_head=head, verified_code=forge.code_digest(self.engine))
        (self.engine / "source.rs").write_text("unreviewed change")
        with patch.object(self.worker, "command") as command:
            with self.assertRaisesRegex(forge.ForgeError, "Code changed"):
                self.worker.publish_games()
            command.assert_not_called()

    def test_a_successful_workflow_for_another_commit_is_rejected(self):
        head = forge.git(self.engine, "rev-parse", "HEAD")
        self.state.update(engine_head=head, games_head="companion", verified_code=forge.code_digest(self.engine),
                          engine_repository="owner/BlueEngine", games_repository="owner/BlueEngineGames")
        self.state["ci"] = {"engine-checks": {"run": 1, "head": head}}
        with patch.object(self.worker, "api", return_value={"head_sha": "old", "status": "completed", "conclusion": "success"}):
            with self.assertRaisesRegex(forge.ForgeError, "does not certify"):
                self.worker.assert_source_gates()

    def test_pages_wait_ignores_an_earlier_successful_deployment(self):
        older = {"head_sha": "head", "event": "push", "created_at": "2026-10-08T00:00:00Z",
                 "status": "completed", "conclusion": "success", "id": 1, "html_url": "old", "updated_at": "old"}
        current = dict(older, id=2, event="workflow_run", created_at="2026-10-08T00:10:01Z", html_url="new")
        with patch.object(self.worker, "api", return_value={"workflow_runs": [older, current]}):
            receipt = self.worker.wait_ci("owner/repo", "pages.yml", "main", "head", "download-page",
                                          after="2026-10-08T00:10:00Z")
        self.assertEqual(receipt["run"], 2)

    def test_agent_uses_stdin_schema_and_bounded_sandbox_without_a_shell(self):
        self.state["model"] = None
        def response(argv, **kwargs):
            self.assertEqual(argv[-1], "-")
            self.assertIn("read-only", argv)
            self.assertNotIn("--dangerously-bypass-approvals-and-sandbox", argv)
            self.assertEqual(kwargs["stdin"], "Concept please")
            output = Path(argv[argv.index("--output-last-message") + 1])
            forge.atomic_json(output, idea())
        with patch.object(self.worker, "command", side_effect=response):
            self.assertEqual(self.worker.agent("concept", "Concept please", forge.IDEA_SCHEMA), idea())

    def test_resume_keeps_completed_phases_and_retries_only_the_failed_build(self):
        self.state["completed"] = ["setup", "generate"]
        self.state["feedback_path"] = "docs/feedback/fixture.md"
        with patch.object(self.worker, "setup") as setup, patch.object(self.worker, "generate") as generate, \
                patch.object(self.worker, "build") as build, patch.object(self.worker, "feedback"), \
                patch.object(self.worker, "register_source"):
            self.worker.run()
        setup.assert_not_called()
        generate.assert_not_called()
        build.assert_called_once()
        self.assertEqual(self.state["status"], "built-locally")

    def test_actual_command_runner_rejects_exit_failure_and_preserves_private_log(self):
        with self.assertRaisesRegex(forge.ForgeError, "exit 7"):
            self.worker.command([sys.executable, "-c", "raise SystemExit(7)"], label="failure-fixture")
        self.assertEqual(self.state["commands"][-1]["exit_code"], 7)
        self.assertTrue((self.directory / self.state["commands"][-1]["log"]).exists())

    def test_rebuild_preserves_idea_and_findings_but_invalidates_old_delivery_gates(self):
        self.state.update(completed=["setup", "generate", "build-and-review", "feedback", "source-commit", "engine-ci"],
                          engine_head="old", games_head="old-games", verified_code="old-digest",
                          ci={"engine-checks": {"head": "old"}}, dispatched=["generated-game-linux-windows"],
                          feedback_path="docs/feedback/fixture.md",
                          build_reports=[{"summary": "Built", "findings": [finding()]}])
        self.worker.rebuild()
        self.assertEqual(self.state["completed"], ["setup", "generate"])
        self.assertEqual(self.state["idea"], idea())
        self.assertEqual(self.state["build_reports"][0]["findings"], [finding()])
        for key in ("ci", "engine_head", "games_head", "verified_code", "dispatched"):
            self.assertNotIn(key, self.state)
        self.assertEqual(self.state["verification_history"][0]["ci"]["engine-checks"]["head"], "old")
        with patch.object(self.worker, "setup") as setup, patch.object(self.worker, "generate") as generate, \
                patch.object(self.worker, "build") as build, patch.object(self.worker, "feedback"), \
                patch.object(self.worker, "register_source"):
            self.worker.run()
        setup.assert_not_called()
        generate.assert_not_called()
        build.assert_called_once()

    def test_rebuild_cannot_erase_an_already_published_run(self):
        self.state["completed"] = ["publish-games"]
        with self.assertRaisesRegex(forge.ForgeError, "already published"):
            self.worker.rebuild()
        self.assertEqual(self.state["completed"], ["publish-games"])

    def test_source_registration_recovers_before_and_after_manifest_write_and_commit(self):
        path = self.engine / "games-publish.json"
        forge.atomic_json(path, {"collections": {"games": []}})
        self.state["verified_code"] = forge.code_digest(self.engine)
        write = forge.atomic_json
        for after in (False, True):
            def interrupted(destination, value):
                if destination == path:
                    if after:
                        write(destination, value)
                    raise KeyboardInterrupt()
                write(destination, value)
            with patch.object(forge, "atomic_json", side_effect=interrupted):
                with self.assertRaises(KeyboardInterrupt):
                    self.worker.register_source()
            self.worker = forge.Forge(self.directory, forge.load_json(self.directory / "state.json"))
        commit = self.worker.commit
        def interrupted_commit(*args):
            commit(*args)
            raise KeyboardInterrupt()
        with patch.object(self.worker, "command"), patch.object(self.worker, "commit", side_effect=interrupted_commit):
            with self.assertRaises(KeyboardInterrupt):
                self.worker.register_source()
        self.worker = forge.Forge(self.directory, forge.load_json(self.directory / "state.json"))
        with patch.object(self.worker, "command"):
            self.worker.register_source()
        self.assertEqual(len(forge.load_json(path)["collections"]["games"]), 1)
        self.assertEqual(self.worker.state["engine_head"], forge.git(self.engine, "rev-parse", "HEAD"))

    def test_independent_export_cannot_claim_an_existing_slug(self):
        self.worker.games.mkdir(parents=True)
        catalog = self.worker.games / ".release-games.json"
        forge.atomic_json(catalog, {"native_playables": [{"slug": self.state["idea"]["slug"]}]})
        before = catalog.read_bytes()
        self.state["games_root"] = str(self.engine)
        with patch.object(self.worker, "command"), patch.object(forge, "github_repository", return_value="fixture/games"):
            with self.assertRaises(forge.TerminalError):
                self.worker.export()
        self.assertEqual(catalog.read_bytes(), before)
        self.assertNotIn("export_base", self.state)

    def test_local_review_defers_windows_delivery_to_the_supervisor_gates(self):
        _, capture = self.native_game()
        def agent(label, prompt, schema, **kwargs):
            if label.startswith("build"):
                return {"summary": "Built", "findings": []}
            self.assertIn("before\nWindows CI and installer publication", prompt)
            self.assertIn("actual\nportability defect in the source remains a blocker", prompt)
            self.assertEqual(kwargs["image"], capture)
            return {"approved": True, "mechanic_assessment": "Implemented",
                    "visual_assessment": "Readable", "blockers": [], "findings": []}
        with patch.object(self.worker, "agent", side_effect=agent), patch.object(self.worker, "verify_local"):
            self.worker.build()
        self.assertIn("verified_code", self.state)
        self.assertNotIn("ci", self.state)
        self.assertNotIn("publication", self.state)


class IntegrationRecoveryTests(unittest.TestCase):
    """Real bare remotes and worktrees; CI receipts alone are substituted."""
    def setUp(self):
        SupervisorTests.setUp(self)
        self.remote = self.directory / "remote.git"
        subprocess.run(["git", "init", "--bare", "-q", str(self.remote)], check=True)
        forge.git(self.engine, "branch", "-M", "main")
        forge.git(self.engine, "remote", "add", "origin", str(self.remote))
        forge.git(self.engine, "push", "-q", "origin", "HEAD:main")
        self.other = self.directory / "other"
        forge.git(self.engine, "worktree", "add", "-b", "other", str(self.other), "main")
        forge.git(self.engine, "checkout", "-b", "idea-forge/fixture")
        (self.engine / "game.rs").write_text("verified game")
        self.state["build_reports"] = [{"findings": [finding()]}]
        self.worker.feedback()
        forge.git(self.engine, "add", ".")
        forge.git(self.engine, "commit", "-qm", "Verified game")
        self.state.update(branch="idea-forge/fixture", publish=True,
                          engine_repository="fixture/engine", engine_head=forge.git(self.engine, "rev-parse", "HEAD"),
                          verified_code=forge.code_digest(self.engine),
                          publication={"page": "https://example.invalid/game", "release": "immutable",
                                       "installer_sha256": "installer", "zip_sha256": "zip"},
                          completed=["setup", "generate", "build-and-review", "feedback", "source-commit",
                                     "engine-ci", "export", "installer-review", "publish-games", "verify-publication"])
        self.worker.save()
        self.release_head = self.state["engine_head"]
        self.receipt = dict(self.state["publication"])

    def advance(self, name="main-change.rs"):
        (self.other / name).write_text("concurrent authoritative source change")
        forge.git(self.other, "add", ".")
        forge.git(self.other, "commit", "-qm", "Concurrent main")
        forge.git(self.other, "push", "-q", "origin", "HEAD:main")

    def ci(self, repository, workflow, branch, head, label, **kwargs):
        receipt = {"head": head, "run": 1, "url": "fixture"}
        self.worker.state.setdefault("ci", {})[label] = receipt
        self.worker.save()
        return receipt

    def api(self, endpoint):
        if "/actions/runs/" in endpoint:
            return {"head_sha": self.worker.state["integration_head"], "status": "completed", "conclusion": "success"}
        path = self.worker.state["feedback_path"]
        forge.git(self.other, "fetch", "-q", "origin", "main")
        return {"sha": forge.git(self.other, "rev-parse", "origin/main:" + path)}

    def finish(self):
        with patch.object(self.worker, "wait_ci", side_effect=self.ci), patch.object(self.worker, "api", side_effect=self.api):
            self.worker.run()
        self.assertTrue(forge.delivery_complete(self.worker.state))
        self.assertEqual(self.worker.state["publication"], self.receipt)
        self.assertEqual(self.worker.state["engine_head"], self.release_head)
        self.assertEqual(forge.git(self.engine, "rev-parse", "HEAD"), self.release_head)
        entries, malformed = forge.learn.load_ledger(self.worker.integration / "docs/learning/ledger.jsonl")
        self.assertEqual((len(entries), malformed), (1, 0))

    def test_normal_and_repeated_resume_preserve_release_and_feedback(self):
        self.finish()
        head = forge.git(self.worker.integration, "rev-parse", "HEAD")
        self.finish()
        self.assertEqual(forge.git(self.worker.integration, "rev-parse", "HEAD"), head)

    def test_main_advancing_before_catalog_publication_does_not_invalidate_release(self):
        self.advance()
        # Engine source did not move: exact source gates remain attached to this artifact.
        self.assertEqual(forge.code_digest(self.engine), self.state["verified_code"])
        self.finish()

    def test_publication_push_succeeded_before_journal_completion(self):
        self.worker.games = self.other
        self.state["games_head"] = forge.git(self.other, "rev-parse", "HEAD")
        self.state["publication_started"] = True
        (self.other / "dirty-after-push.rs").write_text("not release evidence")
        with patch.object(self.worker, "assert_source_gates", side_effect=AssertionError("must reconcile first")), patch.object(self.worker, "command", wraps=self.worker.command) as command:
            self.worker.publish_games()
        self.assertNotIn("publish-catalog", [c.kwargs.get("label") for c in command.call_args_list])

    def test_unchanged_source_reuses_game_receipt_but_checks_exact_integration_head(self):
        self.state["ci"] = {"generated-game-linux-windows": {"head": self.release_head, "run": 7, "url": "fixture"}}
        self.worker.integrate_engine()
        self.assertEqual(self.state["integration_code"], self.state["verified_code"])
        with patch.object(self.worker, "wait_ci", side_effect=self.ci) as waits:
            self.worker.integration_ci()
        self.assertEqual([call.args[4] for call in waits.call_args_list], ["integration-engine-checks"])
        self.assertEqual(self.state["ci"]["integration-game-linux-windows"]["run"], 7)
        def api(endpoint):
            if "/actions/runs/" in endpoint:
                head = self.release_head if endpoint.endswith("/7") else self.state["integration_head"]
                return {"head_sha": head, "status": "completed", "conclusion": "success"}
            return self.api(endpoint)
        with patch.object(self.worker, "api", side_effect=api):
            self.worker.publish_feedback()

    def test_changed_source_dispatches_game_ci_instead_of_reusing_artifact_receipt(self):
        self.state["ci"] = {"generated-game-linux-windows": {"head": self.release_head, "run": 7, "url": "fixture"}}
        self.advance()
        self.worker.integrate_engine()
        with patch.object(self.worker, "wait_ci", side_effect=self.ci) as waits:
            self.worker.integration_ci()
        self.assertEqual(len(waits.call_args_list), 2)
        self.assertEqual(self.state["ci"]["integration-game-linux-windows"]["head"], self.state["integration_head"])

    def test_main_advancing_after_publication_is_revalidated(self):
        self.advance()
        self.finish()
        self.assertNotEqual(self.worker.state["integration_code"], self.worker.state["verified_code"])
        self.assertTrue((self.worker.integration / "main-change.rs").exists())
        self.assertEqual(self.worker.state["ci"]["integration-engine-checks"]["head"], self.worker.state["integration_head"])

    def test_main_advancing_after_integration_ci_recovers_on_resume(self):
        self.worker.integrate_engine()
        with patch.object(self.worker, "wait_ci", side_effect=self.ci):
            self.worker.integration_ci()
        self.worker.state["completed"] += ["integrate-engine", "integration-ci"]
        self.advance()
        with patch.object(self.worker, "api", side_effect=self.api):
            with self.assertRaisesRegex(forge.RecoverableError, "main advanced"):
                self.worker.run()
        self.assertNotIn("integration-ci", self.worker.state["completed"])
        self.finish()

    def test_interruptions_after_each_integration_effect_are_idempotent(self):
        for phase in ("integrate_engine", "integration_ci", "publish_feedback"):
            with self.subTest(phase=phase):
                operation = getattr(self.worker, phase)
                def interrupted():
                    operation()
                    raise KeyboardInterrupt()
                with patch.object(self.worker, "wait_ci", side_effect=self.ci), \
                        patch.object(self.worker, "api", side_effect=self.api), \
                        patch.object(self.worker, phase, side_effect=interrupted):
                    with self.assertRaises(KeyboardInterrupt):
                        self.worker.run()
                self.worker = forge.Forge(self.directory, forge.load_json(self.directory / "state.json"))
        self.finish()

    def test_changed_integration_cannot_reuse_ci(self):
        self.worker.integrate_engine()
        with patch.object(self.worker, "wait_ci", side_effect=self.ci):
            self.worker.integration_ci()
        (self.worker.integration / "source.rs").write_text("unverified rules")
        with self.assertRaisesRegex(forge.RecoverableError, "source changed"):
            self.worker.publish_feedback()

    def test_failed_feedback_push_preserves_publication_then_recovers(self):
        command = self.worker.command
        def fail_push(argv, **kwargs):
            if kwargs.get("label") == "publish-engine-feedback":
                raise forge.RecoverableError("Git push failed; resume")
            return command(argv, **kwargs)
        with patch.object(self.worker, "command", side_effect=fail_push), \
                patch.object(self.worker, "wait_ci", side_effect=self.ci), patch.object(self.worker, "api", side_effect=self.api):
            with self.assertRaises(forge.RecoverableError):
                self.worker.run()
        self.finish()

    def test_overlapping_worker_ledger_appends_are_preserved(self):
        self.advance()
        ledger = self.other / "docs/learning/ledger.jsonl"
        ledger.parent.mkdir(parents=True)
        entry = {"game": "other-game", "area": "input", "tokens": 0, "note": "Other finding",
                 "status": "open", "ref": "docs/feedback/other.md", "keywords": ["input", "keys"]}
        other_entry = forge.learn.append_entry(entry, path=ledger)
        forge.git(self.other, "add", ".")
        forge.git(self.other, "commit", "-qm", "Other worker feedback")
        forge.git(self.other, "push", "-q", "origin", "HEAD:main")
        with patch.object(self.worker, "wait_ci", side_effect=self.ci), patch.object(self.worker, "api", side_effect=self.api):
            self.worker.run()
        entries, malformed = forge.learn.load_ledger(self.worker.integration / "docs/learning/ledger.jsonl")
        self.assertEqual((len(entries), malformed), (2, 0))
        self.assertEqual({e["game"] for e in entries}, {"rule-relay", "other-game"})
        self.assertEqual(len({e["id"] for e in entries}), 2)
        self.assertEqual(next(e for e in entries if e["game"] == "other-game")["id"], other_entry["id"])
        before = (self.worker.integration / "docs/learning/ledger.jsonl").read_bytes()
        with patch.object(self.worker, "wait_ci", side_effect=self.ci), patch.object(self.worker, "api", side_effect=self.api):
            self.worker.run()
        self.assertEqual((self.worker.integration / "docs/learning/ledger.jsonl").read_bytes(), before)


class DailyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.args = argparse.Namespace(engine_root=self.root, runs_root=self.root / "runs",
                                       daily_root=self.root / "daily", timezone="America/Los_Angeles")
        self.now = datetime(2026, 10, 8, 17, tzinfo=timezone.utc)
        self.built = []
        self.fail_next = False
        self.create = patch.object(forge, "create_run", side_effect=self.create_worker).start()
        self.run = patch.object(forge.Forge, "run", lambda worker: self.publish_worker(worker)).start()
        self.addCleanup(patch.stopall)
        try:
            forge.ZoneInfo(self.args.timezone)
            self.real_timezones = True
        except forge.ZoneInfoNotFoundError:
            self.real_timezones = False
            patch.object(forge, "ZoneInfo", return_value=timezone.utc).start()

    def create_worker(self, args, directory):
        directory.mkdir(parents=True, exist_ok=True)
        state = {"dimension": args.dimension, "publish": True, "target_dir": str(self.root),
                 "status": "created", "completed": []}
        worker = forge.Forge(directory, state)
        worker.save()
        return worker

    def publish_worker(self, worker):
        self.built.append((worker.state["dimension"], worker.directory))
        if self.fail_next:
            self.fail_next = False
            worker.state["status"] = "failed"
            worker.save()
            raise forge.ForgeError("A genuine publication gate failed")
        worker.state.update(status="published", completed=["publish-feedback"],
                            publication={"page": "https://example.invalid/game"})
        worker.save()

    def test_daily_publishes_exactly_one_of_each_and_repeated_wakeups_do_nothing(self):
        self.assertEqual(forge.daily_batch(self.args, self.now), 0)
        self.assertCountEqual([d for d, _ in self.built], ["2d", "3d"])
        forge.daily_batch(self.args, self.now)
        self.assertEqual(len(self.built), 2)
        self.assertEqual(self.create.call_count, 2)

    def test_failure_retries_the_same_run_then_builds_the_other_dimension(self):
        self.fail_next = True
        self.assertEqual(forge.daily_batch(self.args, self.now), 1)
        self.assertEqual(len(self.built), 1)
        first = self.built[0]
        self.assertEqual(forge.daily_batch(self.args, self.now), 0)
        self.assertEqual(self.built[1], first)
        self.assertNotEqual(self.built[2][0], first[0])
        self.assertEqual(self.create.call_count, 2)

    def test_reserved_run_recovers_after_exit_before_creation(self):
        batch = self.args.daily_root / "batches/2026-10-08/state.json"
        reserved = self.args.runs_root / "reserved"
        forge.atomic_json(batch, {"version": 1, "date": "2026-10-08", "timezone": self.args.timezone,
                                 "order": ["3d", "2d"], "status": "pending",
                                 "slots": {"3d": {"directory": str(reserved), "status": "pending"}}})
        forge.daily_batch(self.args, self.now)
        self.assertEqual(self.built[0], ("3d", reserved.resolve()))

    def test_published_label_without_completed_feedback_is_resumed(self):
        self.fail_next = True
        forge.daily_batch(self.args, self.now)
        directory = self.built[0][1]
        state = forge.load_json(directory / "state.json")
        state.update(status="published", publication={"page": "https://example.invalid/game"})
        forge.atomic_json(directory / "state.json", state)
        forge.daily_batch(self.args, self.now)
        self.assertEqual(self.built[0], self.built[1])

    def test_calendar_date_uses_pacific_midnight_and_handles_daylight_saving(self):
        if not self.real_timezones:
            self.skipTest("Host has no IANA timezone database; daily behavior is tested with UTC")
        forge.daily_batch(self.args, datetime(2026, 11, 1, 6, 30, tzinfo=timezone.utc))
        self.assertTrue((self.args.daily_root / "batches/2026-10-31/state.json").exists())
        forge.daily_batch(self.args, datetime(2026, 11, 1, 8, 30, tzinfo=timezone.utc))
        forge.daily_batch(self.args, datetime(2026, 11, 1, 9, 30, tzinfo=timezone.utc))
        self.assertEqual(len(self.built), 4)

    def test_overlapping_daily_supervisors_are_blocked(self):
        self.args.daily_root.mkdir()
        with forge.run_lock(self.args.daily_root):
            with self.assertRaises(forge.ForgeError):
                forge.daily_batch(self.args, self.now)
        self.create.assert_not_called()

    def test_old_unfinished_batch_is_completed_before_the_new_day(self):
        self.fail_next = True
        forge.daily_batch(self.args, self.now)
        first = self.built[0]
        forge.daily_batch(self.args, datetime(2026, 10, 9, 17, tzinfo=timezone.utc))
        self.assertEqual(self.built[1], first)
        self.assertEqual(len(self.built), 5)
        self.assertEqual(self.create.call_count, 4)

    def test_schedule_configuration_drives_daily_without_disabling_publication(self):
        config = self.root / "config.json"
        forge.atomic_json(config, {"engine_root": str(self.root), "games_root": str(self.root / "games"),
                                  "daily_root": str(self.args.daily_root), "timezone": "America/Los_Angeles"})
        with patch.object(forge, "daily_batch", return_value=0) as daily:
            self.assertEqual(forge.main(["daily", "--config", str(config)]), 0)
        settings = daily.call_args.args[0]
        self.assertEqual(settings.engine_root, self.root)
        self.assertTrue(settings.publish)

    def test_installer_persists_settings_and_enables_hourly_retry_timer(self):
        self.args.publish = True
        with patch.dict(os.environ, {"XDG_CONFIG_HOME": str(self.root / "config")}), \
                patch.object(forge.sys, "platform", "linux"), \
                patch.object(forge.shutil, "which", return_value="systemctl"), \
                patch.object(forge.subprocess, "run") as command:
            forge.install_schedule(self.args)
        units = self.root / "config/systemd/user"
        timer = (units / "ideaforge-daily.timer").read_text()
        self.assertIn("OnCalendar=*-*-* *:10:00 America/Los_Angeles", timer)
        self.assertIn("Persistent=true", timer)
        service = (units / "ideaforge-daily.service").read_text()
        self.assertIn('"daily" "--config"', service)
        self.assertIn(f"WorkingDirectory={self.root.resolve()}\n", service)
        saved = forge.load_json(self.root / "config/ideaforge/daily.json")
        self.assertNotIn("publish", saved)
        self.assertEqual(saved["engine_root"], str(self.root.resolve()))
        self.assertIn(["systemctl", "--user", "enable", "--now", "ideaforge-daily.timer"],
                      [call.args[0] for call in command.call_args_list])

    def test_scheduler_rejects_multiline_paths_and_escapes_expansions(self):
        with self.assertRaises(forge.ForgeError):
            forge.systemd_quote("path\nExecStart=another-command")
        self.assertEqual(forge.systemd_quote('/tmp/space % $ "'), '"/tmp/space %% $$ \\""')


if __name__ == "__main__":
    unittest.main()
