"""Behavioral gates for the IdeaForge supervisor, without paid AI calls or real publication."""
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from tools import idea_forge as forge


def idea():
    return {"title": "Rule Relay", "slug": "rule-relay", "genre": "puzzle",
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
                      "repairs": 0, "publish": False, "idea": idea()}
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
        forge.atomic_json(game / "game.project.json", {"targets": ["windows", "linux"]})
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


if __name__ == "__main__":
    unittest.main()
