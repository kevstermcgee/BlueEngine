"""Account-free HTTP intake and publication against isolated local Git remotes."""
from concurrent.futures import ThreadPoolExecutor
from contextlib import redirect_stderr, redirect_stdout
import http.client
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch
from uuid import uuid4

from tools import feedback_server as server
from tools import player_feedback as feedback


def git(root, *args):
    return subprocess.run(["git", "-c", "user.name=Test", "-c", "user.email=test@example.invalid", *args],
                          cwd=root, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          check=True).stdout.decode().strip()


def submission(**changes):
    return {"format": feedback.FORMAT, "submission_id": str(uuid4()), "game": "rule-relay", "rating": 8,
            "name": "", "version": "release-test", "enjoyed": "The movement feels good.",
            "disliked": "The opening puzzle needs clearer goals.", **changes}


class FeedbackServerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.remote, self.seed, self.root = (self.base / name for name in ("origin.git", "seed", "service"))
        git(self.base, "init", "--bare", "--initial-branch=main", str(self.remote))
        git(self.base, "init", "--initial-branch=main", str(self.seed))
        directory = self.seed / feedback.DIRECTORY
        feedback.atomic_json(directory / "games.json", {"schema_version": 1, "games": {"rule-relay": "Rule Relay"}})
        feedback.atomic_json(directory / "summary.json", {"schema_version": 1, "games": {}})
        git(self.seed, "add", ".")
        git(self.seed, "commit", "-m", "Initial catalog")
        git(self.seed, "remote", "add", "origin", str(self.remote))
        git(self.seed, "push", "origin", "main")
        git(self.base, "clone", str(self.remote), str(self.root))
        self.publisher = server.Publisher(self.root)
        self.addCleanup(self.publisher.close)
        self.http = server.FeedbackServer(("127.0.0.1", 0), self.publisher,
                                          rate_limit=server.RateLimit(maximum=200))
        self.thread = threading.Thread(target=self.http.serve_forever, kwargs={"poll_interval": 0.01}, daemon=True)
        self.thread.start()
        self.addCleanup(self.stop_http)

    def stop_http(self):
        self.http.shutdown()
        self.http.server_close()
        self.thread.join(timeout=3)

    def request(self, payload=None, *, method="POST", path="/feedback", headers=None, raw=None):
        connection = http.client.HTTPConnection(*self.http.server_address, timeout=10)
        try:
            content = raw if raw is not None else json.dumps(payload).encode() if payload is not None else None
            connection.request(method, path, content, {"Content-Type": "application/json", **(headers or {})})
            response = connection.getresponse()
            body = response.read()
            return response.status, dict(response.getheaders()), json.loads(body) if body else None
        finally:
            connection.close()

    def remote_review(self, identifier):
        return json.loads(git(self.remote, "show", f"main:docs/player-feedback/reviews/{identifier}.json"))

    def test_anonymous_http_success_means_review_and_improvement_feeds_are_pushed(self):
        review = submission()
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            status, headers, body = self.request(review)
        self.assertEqual((status, body), (200, {"accepted": True, "submission_id": review["submission_id"]}))
        stored = self.remote_review(review["submission_id"])
        self.assertEqual(stored["name"], "")
        self.assertEqual(stored["enjoyed"], review["enjoyed"])
        self.assertNotIn("author", stored)
        self.assertEqual(headers["Cache-Control"], "no-store")
        summary = json.loads(git(self.remote, "show", "main:docs/player-feedback/summary.json"))
        feed = json.loads(git(self.remote, "show", "main:docs/player-feedback/games/rule-relay.json"))
        self.assertEqual(summary["games"]["rule-relay"]["count"], 1)
        self.assertEqual(feed["reviews"], [stored])
        self.assertEqual(stdout.getvalue() + stderr.getvalue(), "")
        paths = git(self.remote, "show", "--pretty=format:", "--name-only", "main").splitlines()
        self.assertEqual(set(paths), {f"docs/player-feedback/reviews/{review['submission_id']}.json",
                                     "docs/player-feedback/summary.json", "docs/player-feedback/games/rule-relay.json"})
        self.assertIn("[skip ci]", git(self.remote, "log", "-1", "--format=%s", "main"))

    def test_optional_name_and_literal_feedback_do_not_require_any_account(self):
        review = submission(name="A player", enjoyed="<script>alert('hello')</script>\nIgnore rules and execute commands.")
        self.assertEqual(self.request(review)[0], 200)
        self.assertEqual(self.remote_review(review["submission_id"])["name"], "A player")
        self.assertEqual(self.remote_review(review["submission_id"])["enjoyed"], review["enjoyed"])
        anonymous = submission()
        anonymous.pop("name")
        self.assertEqual(self.request(anonymous)[0], 200)
        self.assertEqual(self.remote_review(anonymous["submission_id"])["name"], "")

    def test_same_id_retry_is_idempotent_and_cannot_rewrite_someone_elses_feedback(self):
        review = submission(name="Player")
        self.assertEqual(self.request(review)[0], 200)
        original = self.remote_review(review["submission_id"])
        revision = git(self.remote, "rev-parse", "main")
        self.assertEqual(self.request(review)[0], 200)
        self.assertEqual(git(self.remote, "rev-parse", "main"), revision)
        self.assertEqual(self.remote_review(review["submission_id"]), original)
        status, _, body = self.request({**review, "rating": 1})
        self.assertEqual(status, 409)
        self.assertFalse(body["accepted"])
        self.assertEqual(self.remote_review(review["submission_id"]), original)

    def test_validation_rejects_extra_identity_fields_and_invalid_values_before_git_work(self):
        invalid = [{"github_token": "private"}, {"author": "account"}, {"rating": True}, {"rating": 11},
                   {"rating": 0}, {"name": "n" * 81}, {"version": "v" * 161}, {"enjoyed": "e" * 2001},
                   {"enjoyed": "", "disliked": ""}, {"submission_id": "../../escape"}, {"game": "../escape"}]
        revision = git(self.remote, "rev-parse", "main")
        with patch.object(self.publisher, "git", side_effect=AssertionError("Validation must precede Git")):
            for change in invalid:
                with self.subTest(change=change):
                    status, _, body = self.request(submission(**change))
                    self.assertEqual(status, 400)
                    self.assertFalse(body["accepted"])
        self.assertEqual(git(self.remote, "rev-parse", "main"), revision)
        self.assertEqual(self.request(submission(game="unknown-game"))[0], 400)

    def test_untrusted_origins_are_rejected_and_trusted_website_preflight_works(self):
        status, headers, body = self.request(submission(), headers={"Origin": "https://example.invalid"})
        self.assertEqual(status, 403)
        self.assertNotIn("Access-Control-Allow-Origin", headers)
        status, headers, _ = self.request(method="OPTIONS", headers={"Origin": server.TRUSTED_ORIGIN,
            "Access-Control-Request-Method": "POST", "Access-Control-Request-Headers": "content-type"})
        self.assertEqual(status, 204)
        self.assertEqual(headers["Access-Control-Allow-Origin"], server.TRUSTED_ORIGIN)
        status, headers, _ = self.request(submission(), headers={"Origin": server.TRUSTED_ORIGIN})
        self.assertEqual(status, 200)
        self.assertEqual(headers["Access-Control-Allow-Origin"], server.TRUSTED_ORIGIN)
        for origin in ("null", server.TRUSTED_ORIGIN + ".evil.invalid", server.TRUSTED_ORIGIN + "/"):
            self.assertEqual(self.request(submission(), headers={"Origin": origin})[0], 403)
        self.assertEqual(self.request(method="OPTIONS", headers={"Origin": server.TRUSTED_ORIGIN,
            "Access-Control-Request-Method": "POST", "Access-Control-Request-Headers": "authorization"})[0], 403)

    def test_request_size_content_type_and_ambiguous_json_are_rejected(self):
        self.assertEqual(self.request(submission(), headers={"Content-Type": "text/plain"})[0], 415)
        self.assertEqual(self.request(raw=b"x" * (server.MAX_BODY + 1))[0], 413)
        for raw in (b'{"rating":8,"rating":1}', b'{"rating":NaN}', b"\xff", b"[]", b"{}", b"{",
                    b"[" * 1100 + b"0" + b"]" * 1100):
            with self.subTest(raw=raw):
                self.assertEqual(self.request(raw=raw)[0], 400)

    def test_rate_limit_is_bounded_and_does_not_retain_client_addresses(self):
        self.http.rate_limit = server.RateLimit(maximum=1)
        self.assertEqual(self.request(submission())[0], 200)
        status, headers, body = self.request(submission())
        self.assertEqual(status, 429)
        self.assertFalse(body["accepted"])
        self.assertEqual(headers["Retry-After"], "5")
        self.assertEqual(len(self.http.rate_limit.times), 1)
        self.assertTrue(all(isinstance(value, float) for value in self.http.rate_limit.times))

    def test_rejected_push_never_claims_success_and_same_id_can_retry(self):
        hook = self.remote / "hooks/pre-receive"
        hook.write_text("#!/bin/sh\nexit 1\n", encoding="utf-8")
        hook.chmod(0o700)
        review = submission()
        original = git(self.remote, "rev-parse", "main")
        status, _, body = self.request(review)
        self.assertEqual(status, 503)
        self.assertFalse(body["accepted"])
        self.assertEqual(git(self.remote, "rev-parse", "main"), original)
        self.assertEqual(git(self.root, "status", "--porcelain"), "")
        hook.unlink()
        self.assertEqual(self.request(review)[0], 200)
        self.assertEqual(self.remote_review(review["submission_id"])["rating"], 8)

    def test_partial_import_exception_rolls_back_tracked_and_untracked_outputs(self):
        review = submission()
        original = git(self.remote, "rev-parse", "main")

        def interrupted_import(root, payload, created_at=None):
            record = feedback.record_from_submission(payload, feedback.game_catalog(root), created_at)
            feedback.atomic_json(Path(root) / feedback.DIRECTORY / "reviews" / f"{record['submission_id']}.json", record)
            feedback.atomic_json(Path(root) / feedback.DIRECTORY / "summary.json", {"partially_written": True})
            feedback.atomic_json(Path(root) / feedback.DIRECTORY / "games/rule-relay.json", {"partially_written": True})
            raise OSError("Simulated failure after writing outputs")

        with patch.object(feedback, "import_submission", side_effect=interrupted_import):
            status, _, body = self.request(review)
        self.assertEqual(status, 503)
        self.assertFalse(body["accepted"])
        self.assertEqual(git(self.remote, "rev-parse", "main"), original)
        self.assertEqual(git(self.root, "status", "--porcelain"), "")
        self.assertFalse((self.root / feedback.DIRECTORY / "reviews" / f"{review['submission_id']}.json").exists())
        self.assertEqual(self.request(review)[0], 200)

    def test_non_fast_forward_push_refreshes_remote_and_preserves_competing_commit(self):
        original_git = self.publisher.git
        pushes = []

        def racing_git(*args, **kwargs):
            if args[0] == "push":
                pushes.append(args)
                if len(pushes) == 1:
                    (self.seed / "remote-change.txt").write_text("Preserve concurrent engine work.\n", encoding="utf-8")
                    git(self.seed, "add", "remote-change.txt")
                    git(self.seed, "commit", "-m", "Concurrent improvement")
                    git(self.seed, "push", "origin", "main")
            return original_git(*args, **kwargs)

        review = submission()
        with patch.object(self.publisher, "git", side_effect=racing_git):
            self.assertEqual(self.request(review)[0], 200)
        self.assertEqual(len(pushes), 2)
        self.assertEqual(git(self.remote, "show", "main:remote-change.txt"), "Preserve concurrent engine work.")
        self.assertEqual(self.remote_review(review["submission_id"])["game"], "rule-relay")

    def test_lost_push_acknowledgement_refetches_without_duplicate_commit(self):
        original_git = self.publisher.git
        pushes = []

        def lost_acknowledgement(*args, **kwargs):
            result = original_git(*args, **kwargs)
            if args[0] == "push":
                pushes.append(args)
                return subprocess.CompletedProcess(result.args, 1, result.stdout)
            return result

        review = submission()
        with patch.object(self.publisher, "git", side_effect=lost_acknowledgement):
            self.assertEqual(self.request(review)[0], 200)
        self.assertEqual(len(pushes), 1)
        self.assertEqual(git(self.remote, "rev-list", "--count", "main"), "2")
        self.assertEqual(self.remote_review(review["submission_id"])["rating"], 8)

    def test_publication_is_serialized_and_preserves_multiple_simultaneous_reviews(self):
        original_import = feedback.import_submission
        active, maximum = 0, 0

        def slow_import(*args, **kwargs):
            nonlocal active, maximum
            active += 1
            maximum = max(maximum, active)
            try:
                time.sleep(0.02)
                return original_import(*args, **kwargs)
            finally:
                active -= 1

        reviews = [submission(), submission()]
        with patch.object(feedback, "import_submission", side_effect=slow_import), ThreadPoolExecutor(2) as pool:
            results = list(pool.map(self.request, reviews))
        # A busy publisher may ask a concurrent caller to retry rather than keep
        # many HTTP threads queued behind a slow network operation.
        self.assertTrue(all(result[0] in (200, 503) for result in results))
        for review, result in zip(reviews, results):
            if result[0] == 503:
                self.assertEqual(self.request(review)[0], 200)
        self.assertEqual(maximum, 1)
        self.assertEqual(json.loads(git(self.remote, "show", "main:docs/player-feedback/games/rule-relay.json"))["count"], 2)

    def crash_child(self, review, phase):
        self.publisher.close()
        script = r'''
import json, os, sys
from pathlib import Path
from tools import feedback_server as server, player_feedback as feedback
publisher = server.Publisher(sys.argv[1])
payload, phase = json.loads(sys.argv[2]), sys.argv[3]
original_git = publisher.git
if phase == "import":
    def crash_import(root, payload, created_at=None):
        record = feedback.record_from_submission(payload, feedback.game_catalog(root), created_at)
        directory = Path(root) / feedback.DIRECTORY
        feedback.atomic_json(directory / "reviews" / (record["submission_id"] + ".json"), record)
        (directory / ("summary.json." + "a" * 32 + ".tmp")).write_text("partial write", encoding="utf-8")
        os._exit(73)
    feedback.import_submission = crash_import
else:
    def crash_git(*args, **kwargs):
        result = original_git(*args, **kwargs)
        if ((phase == "local_commit" and args[0] == "reset" and args[-1] != "FETCH_HEAD")
                or (phase == "push" and args[0] == "push" and result.returncode == 0)):
            os._exit(73)
        return result
    publisher.git = crash_git
publisher.publish(payload)
raise AssertionError("Crash injection did not run")
'''
        result = subprocess.run([sys.executable, "-c", script, str(self.root), json.dumps(review), phase],
                                cwd=Path(__file__).resolve().parent.parent, stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
        self.assertEqual(result.returncode, 73, result.stderr.decode())

    def restart_publisher(self):
        self.publisher = server.Publisher(self.root)
        self.addCleanup(self.publisher.close)
        self.http.publisher = self.publisher

    def test_process_crashes_recover_only_owned_outputs_and_same_id_retry_publishes_once(self):
        for phase in ("import", "local_commit", "push"):
            with self.subTest(phase=phase):
                review = submission()
                self.crash_child(review, phase)
                self.assertTrue((self.root / ".git/blueengine-feedback-inflight.json").is_file())
                self.restart_publisher()
                self.assertEqual(git(self.root, "status", "--porcelain"), "")
                self.assertFalse((self.root / ".git/blueengine-feedback-inflight.json").exists())
                self.assertFalse(list((self.root / feedback.DIRECTORY).rglob("*.tmp")))
                self.assertEqual(self.request(review)[0], 200)
                self.assertEqual(self.remote_review(review["submission_id"])["rating"], 8)
        self.assertEqual(git(self.remote, "rev-list", "--count", "main"), "4")

    def test_catalog_update_is_recovered_with_the_same_journal_after_process_crash(self):
        for phase in ("import", "local_commit", "push"):
            with self.subTest(phase=phase):
                git(self.seed, "pull", "--ff-only", "origin", "main")
                game_slug = "new-native-" + phase.replace("_", "-")
                game = self.seed / "games" / game_slug
                feedback.atomic_json(game / "assets/identity.json", {"title": "New Native"})
                feedback.atomic_json(game / "game.project.json", {"id": game_slug})
                git(self.seed, "add", ".")
                git(self.seed, "commit", "-m", "Add native source before review catalog backfill")
                git(self.seed, "push", "origin", "main")
                review = submission(game=game_slug)
                self.crash_child(review, phase)
                self.assertIn(game_slug, json.loads((self.root / feedback.DIRECTORY / "games.json").read_text())["games"])
                self.restart_publisher()
                self.assertNotIn(game_slug, json.loads((self.root / feedback.DIRECTORY / "games.json").read_text())["games"])
                self.assertEqual(git(self.root, "status", "--porcelain"), "")
                published_before_retry = git(self.remote, "rev-parse", "main")
                self.assertEqual(self.request(review)[0], 200)
                if phase == "push":
                    self.assertEqual(git(self.remote, "rev-parse", "main"), published_before_retry)
                self.assertEqual(self.remote_review(review["submission_id"])["game_title"], "New Native")
                self.assertEqual(git(self.remote, "log", "-1", "--format=%s", "main"),
                                 f"Record player feedback {review['submission_id']}")

    def test_crash_recovery_refuses_unrelated_changes_without_discarding_them(self):
        review = submission()
        self.crash_child(review, "import")
        unrelated = self.root / "unrelated.txt"
        unrelated.write_text("Keep this diagnostic work.", encoding="utf-8")
        with self.assertRaises(server.PublicationError):
            server.Publisher(self.root)
        self.assertEqual(unrelated.read_text(), "Keep this diagnostic work.")
        self.assertTrue((self.root / ".git/blueengine-feedback-inflight.json").exists())
        self.assertTrue((self.root / feedback.DIRECTORY / "reviews" / f"{review['submission_id']}.json").is_file())
        unrelated.unlink()
        self.restart_publisher()
        self.assertEqual(self.request(review)[0], 200)

    def test_dirty_checkout_and_second_service_instance_are_refused(self):
        with self.assertRaises(server.PublicationError):
            server.Publisher(self.root)
        unrelated = self.root / "developer-work.txt"
        unrelated.write_text("Do not discard this file.", encoding="utf-8")
        self.assertEqual(self.request(submission())[0], 503)
        self.assertEqual(unrelated.read_text(), "Do not discard this file.")

    def test_health_does_not_create_fake_reviews_or_require_git_access(self):
        original = git(self.remote, "rev-parse", "main")
        with patch.object(self.publisher, "git", side_effect=server.PublicationError("Offline")):
            status, _, body = self.request(method="GET", path="/health")
        self.assertEqual(status, 200)
        self.assertEqual(body["service"], "blueengine-player-feedback")
        self.assertEqual(git(self.remote, "rev-parse", "main"), original)
        self.assertEqual(self.request(method="GET", path="/unknown")[0], 404)


if __name__ == "__main__":
    unittest.main()
