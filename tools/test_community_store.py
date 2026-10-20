"""Source identity, authorization and native production receipt contracts."""
import base64
import json
import unittest
from unittest.mock import patch
from uuid import uuid4

from tools import community_store as store


class GitHubContractTests(unittest.TestCase):
    def client(self):
        return store.GitHub("github_test_token")

    def test_role_comes_from_engine_repository_permissions(self):
        for permissions in ({"push": True}, {"maintain": True}, {"admin": True}):
            with patch.object(store.GitHub, "api", side_effect=[{"login": "cousin"}, {"permissions": permissions}]):
                self.assertEqual(self.client().actor(store.REPOSITORY), {"login": "cousin", "role": "maintainer"})
        with patch.object(store.GitHub, "api", side_effect=[{"login": "alice"}, {"permissions": {"pull": True}}]):
            self.assertEqual(self.client().actor(store.REPOSITORY)["role"], "contributor")

    def test_upstream_updates_require_identical_source_or_confirmed_ancestry(self):
        source = store.source_revision("alice/BlueEngine", "a" * 40, "alice-game")
        upstream = store.source_revision(store.REPOSITORY, "b" * 40, "alice-game")
        client = self.client()
        with patch.object(client, "game_tree", return_value="f" * 40), patch.object(client, "api", side_effect=AssertionError("Identical trees need no compare")):
            client.require_current_upstream(source, upstream)
        with patch.object(client, "game_tree", side_effect=["1" * 40, "2" * 40]), \
                patch.object(client, "api", return_value={"status": "ahead", "merge_base_commit": {"sha": "b" * 40}}):
            client.require_current_upstream(source, upstream)
        for comparison in (None, {"status": "diverged", "merge_base_commit": {"sha": "b" * 40}},
                           {"status": "ahead", "merge_base_commit": {"sha": "c" * 40}}):
            with patch.object(client, "game_tree", side_effect=["1" * 40, "2" * 40]), patch.object(client, "api", return_value=comparison):
                with self.assertRaises(store.Conflict):
                    client.require_current_upstream(source, upstream)

    def test_maintainers_can_queue_a_verified_cousin_fork_without_personal_fork_write(self):
        info = {"full_name": "cousin/BlueEngine", "source": {"full_name": store.REPOSITORY},
                "permissions": {"push": False}, "private": False}
        client = self.client()
        with patch.object(client, "api", return_value=info):
            self.assertEqual(client.source_upstream(store.REPOSITORY, "cousin/BlueEngine", require_write=False), store.REPOSITORY)
            with self.assertRaises(store.AccessDenied):
                client.source_upstream(store.REPOSITORY, "cousin/BlueEngine")
        project = {"id": "cousin-game", "targets": ["windows", "linux"]}
        content = {"type": "file", "encoding": "base64", "size": 100,
                   "content": base64.b64encode(json.dumps(project).encode()).decode()}
        with patch.object(client, "api", side_effect=[info, {"sha": "a" * 40}, content]):
            client.verify_source(store.REPOSITORY, "cousin/BlueEngine", "a" * 40, "cousin-game", require_write=False)
        source = store.source_revision("cousin/BlueEngine", "a" * 40, "cousin-game")
        pull = {"number": 42, "html_url": f"https://github.com/{store.REPOSITORY}/pull/42", "state": "open"}
        # The named source pin was established by its author. Reading it and
        # opening an upstream PR do not require writing the cousin's repository.
        with patch.object(client, "api", side_effect=[{"object": {"sha": "a" * 40}}, info, [], pull]) as api:
            result = client.pull_request(store.REPOSITORY, {"slug": "cousin-game", "title": "Cousin Game"}, source, str(uuid4()))
            self.assertEqual(result["pull_request_url"], pull["html_url"])
            writes = [call for call in api.call_args_list if call.kwargs.get("method") == "POST"]
            self.assertEqual(len(writes), 1)
            self.assertEqual(writes[0].args[0], f"/repos/{store.REPOSITORY}/pulls")

    def test_source_requires_public_matching_fork_write_access_and_native_manifest(self):
        info = {"full_name": "alice/BlueEngine", "source": {"full_name": store.REPOSITORY}, "permissions": {"push": True}, "private": False}
        project = {"id": "alice-relay", "targets": ["windows", "linux"]}

        def content(project):
            return {"type": "file", "encoding": "base64", "size": 100,
                    "content": base64.b64encode(json.dumps(project).encode()).decode()}

        with patch.object(store.GitHub, "api", side_effect=[info, {"sha": "a" * 40}, content(project)]):
            self.client().verify_source(store.REPOSITORY, "alice/BlueEngine", "a" * 40, "alice-relay")
        for bad_info in ({**info, "private": True}, {**info, "source": {}}, {**info, "permissions": {"push": False}}):
            with patch.object(store.GitHub, "api", return_value=bad_info):
                with self.assertRaises(store.CommunityError):
                    self.client().verify_source(store.REPOSITORY, "alice/BlueEngine", "a" * 40, "alice-relay")
        for bad in ({**project, "targets": ["web"]}, {**project, "targets": ["windows", "web"]}, {**project, "id": "spoof"}):
            with patch.object(store.GitHub, "api", side_effect=[info, {"sha": "a" * 40}, content(bad)]):
                with self.assertRaises(store.CommunityError):
                    self.client().verify_source(store.REPOSITORY, "alice/BlueEngine", "a" * 40, "alice-relay")

    def test_source_pins_are_created_once_and_cannot_move(self):
        revision = store.source_revision("alice/BlueEngine", "a" * 40, "alice-game")
        with patch.object(store.GitHub, "api", side_effect=[None, {}]) as api:
            branch = self.client().pin_source(revision, "alice-game")
            self.assertEqual(api.call_args.kwargs["data"], {"ref": f"refs/heads/{branch}", "sha": "a" * 40})
        with patch.object(store.GitHub, "api", return_value={"object": {"sha": "b" * 40}}):
            with self.assertRaises(store.Conflict):
                self.client().pin_source(revision, "alice-game")

    def test_publish_pull_request_is_idempotent_and_keeps_native_gates_visible(self):
        revision = store.source_revision("alice/BlueEngine", "a" * 40, "alice-game")
        pull = {"number": 22, "html_url": f"https://github.com/{store.REPOSITORY}/pull/22", "state": "open"}
        game = {"slug": "alice-game", "title": "Alice Game"}
        with patch.object(store.GitHub, "pin_source", return_value="blueengine-community/alice-game/abc"), \
                patch.object(store.GitHub, "source_upstream", return_value=store.REPOSITORY), \
                patch.object(store.GitHub, "api", side_effect=[[], pull]) as api:
            result = self.client().pull_request(store.REPOSITORY, game, revision, str(uuid4()))
            self.assertEqual(result["state"], "awaiting_native_gates")
            body = api.call_args.kwargs["data"]["body"]
            self.assertIn("Windows x64 installer review", body)
            self.assertIn("tools/community.py release alice-game", body)
        with patch.object(store.GitHub, "pin_source", return_value="branch"), \
                patch.object(store.GitHub, "source_upstream", return_value=store.REPOSITORY), \
                patch.object(store.GitHub, "api", return_value=[pull]) as api:
            self.client().pull_request(store.REPOSITORY, game, revision, str(uuid4()))
            self.assertEqual(api.call_count, 1)
        companion_pull = {**pull, "html_url": f"https://github.com/{store.GAMES_REPOSITORY}/pull/22"}
        with patch.object(store.GitHub, "pin_source", return_value="branch"), \
                patch.object(store.GitHub, "source_upstream", return_value=store.GAMES_REPOSITORY), \
                patch.object(store.GitHub, "api", return_value=[companion_pull]):
            result = self.client().pull_request(store.REPOSITORY, game, revision, str(uuid4()))
            self.assertIn("--source companion", result["maintainer_command"])
            self.assertEqual(result["source_kind"], "companion")

    def test_release_receipt_requires_matching_source_assets_and_all_exact_workflow_gates(self):
        source = store.source_revision("alice/BlueEngine", "a" * 40, "alice-game")
        game = {"slug": "alice-game", "title": "Alice Game"}
        tag = "native-games-" + "c" * 12 + "-v1"
        receipt = {"page": "https://kevstermcgee.github.io/BlueEngineGames/games/alice-game/",
                   "release": f"https://github.com/{store.GAMES_REPOSITORY}/releases/tag/{tag}",
                   "installer_sha256": "d" * 64, "zip_sha256": "e" * 64, "verified_at": "2026-10-20T12:00:00+00:00"}
        request = {"request_id": str(uuid4()), "revision_id": source["id"], "source_commit": "b" * 40,
                   "games_commit": "c" * 40, "receipt": receipt,
                   "gates": {label: index for index, label in enumerate(store.RELEASE_GATES, 1) if label in store.ENGINE_GATES}}
        release = {"draft": False, "prerelease": False, "html_url": receipt["release"],
                   "assets": [{"name": "alice-game-setup-windows-x64.exe", "state": "uploaded", "digest": "sha256:" + "d" * 64},
                              {"name": "alice-game-windows-x64.zip", "state": "uploaded", "digest": "sha256:" + "e" * 64}]}
        runs = {}
        for label, (scope, workflow) in store.RELEASE_GATES.items():
            event = "workflow_dispatch" if label in ("windows-installer-review", "companion-native-checks") else "workflow_run" if label == "download-page" else "push"
            runs[label] = {"head_sha": "b" * 40 if scope == "engine" else "c" * 40,
                         "status": "completed", "conclusion": "success", "head_branch": "main", "event": event,
                         "path": ".github/workflows/" + workflow + "@main"}
        engine_runs = [run for label, run in runs.items() if label in store.ENGINE_GATES]
        client = self.client()
        client.source_repositories = (store.REPOSITORY, store.GAMES_REPOSITORY)
        catalog = {"type": "file", "encoding": "base64", "size": 100,
                   "content": base64.b64encode(json.dumps({"source_repository": store.REPOSITORY, "source_revision": "b" * 40}).encode()).decode()}

        def responses(gates, overrides=None, artifact_slug="alice-game"):
            result = [catalog, release]
            for label in store.RELEASE_GATES:
                if label not in gates:
                    continue
                result.append((overrides or {}).get(label, runs[label]))
                if label in ("generated-game-linux-windows", "companion-native-checks"):
                    prefix = "forge" if label == "generated-game-linux-windows" else "native"
                    result.append({"artifacts": [{"name": f"{prefix}-{artifact_slug}-{system}", "expired": False}
                                                  for system in ("Linux", "Windows")]})
            return result

        with patch.object(client, "source_upstream", return_value=store.REPOSITORY), \
                patch.object(client, "game_tree", return_value="f" * 40), patch.object(client, "api", side_effect=responses(store.ENGINE_GATES)):
            result = client.verify_release(store.REPOSITORY, store.GAMES_REPOSITORY, game, source, request)
            self.assertEqual(result["state"], "published")
            self.assertEqual(set(result["gates"]), store.ENGINE_GATES)
        with patch.object(client, "source_upstream", return_value=store.REPOSITORY), patch.object(client, "game_tree", side_effect=["f" * 40, "1" * 40]):
            with self.assertRaises(store.CommunityError):
                client.verify_release(store.REPOSITORY, store.GAMES_REPOSITORY, game, source, request)
        with patch.object(client, "source_upstream", return_value=store.REPOSITORY), \
                patch.object(client, "game_tree", side_effect=["f" * 40, "f" * 40, "2" * 40]):
            with self.assertRaises(store.CommunityError):
                client.verify_release(store.REPOSITORY, store.GAMES_REPOSITORY, game, source, request)
        for bad in ({**release, "draft": True}, {**release, "assets": []}):
            with patch.object(client, "source_upstream", return_value=store.REPOSITORY), \
                    patch.object(client, "game_tree", return_value="f" * 40), patch.object(client, "api", return_value=bad):
                with self.assertRaises(store.CommunityError):
                    client.verify_release(store.REPOSITORY, store.GAMES_REPOSITORY, game, source, request)
        wrong = {"engine-checks": {**runs["engine-checks"], "head_sha": "9" * 40}}
        with patch.object(client, "source_upstream", return_value=store.REPOSITORY), \
                patch.object(client, "game_tree", return_value="f" * 40), patch.object(client, "api", side_effect=responses(store.ENGINE_GATES, wrong)):
            with self.assertRaises(store.CommunityError):
                client.verify_release(store.REPOSITORY, store.GAMES_REPOSITORY, game, source, request)
        companion_source = store.source_revision("alice/BlueEngineGames", "a" * 40, "alice-game")
        companion_request = {**request, "gates": {label: index for index, label in enumerate(store.RELEASE_GATES, 1) if label in store.COMPANION_GATES}}
        companion_runs = [run for label, run in runs.items() if label in store.COMPANION_GATES]
        with patch.object(client, "source_upstream", return_value=store.GAMES_REPOSITORY), \
                patch.object(client, "game_tree", return_value="f" * 40), patch.object(client, "api", side_effect=responses(store.COMPANION_GATES)):
            result = client.verify_release(store.REPOSITORY, store.GAMES_REPOSITORY, game, companion_source, companion_request)
            self.assertEqual(set(result["gates"]), store.COMPANION_GATES)
        missing_native = {**companion_request, "gates": {label: run for label, run in companion_request["gates"].items() if label != "companion-native-checks"}}
        with patch.object(client, "source_upstream", return_value=store.GAMES_REPOSITORY):
            with self.assertRaises(store.CommunityError):
                client.verify_release(store.REPOSITORY, store.GAMES_REPOSITORY, game, companion_source, missing_native)
        mismatched_review = {"companion-native-checks": {**runs["companion-native-checks"], "head_branch": "unrelated-branch"}}
        with patch.object(client, "source_upstream", return_value=store.GAMES_REPOSITORY), \
                patch.object(client, "game_tree", return_value="f" * 40), patch.object(client, "api", side_effect=responses(store.COMPANION_GATES, mismatched_review)):
            with self.assertRaises(store.CommunityError):
                client.verify_release(store.REPOSITORY, store.GAMES_REPOSITORY, game, companion_source, companion_request)
        for upstream, pinned_source, submitted, gates in ((store.REPOSITORY, source, request, store.ENGINE_GATES),
                                                          (store.GAMES_REPOSITORY, companion_source, companion_request, store.COMPANION_GATES)):
            with patch.object(client, "source_upstream", return_value=upstream), \
                    patch.object(client, "game_tree", return_value="f" * 40), patch.object(client, "api", side_effect=responses(gates, artifact_slug="other-game")):
                with self.assertRaises(store.CommunityError):
                    client.verify_release(store.REPOSITORY, store.GAMES_REPOSITORY, game, pinned_source, submitted)
        for wrong_pin in ({"source_repository": store.REPOSITORY, "source_revision": "9" * 40},
                          {"source_repository": "attacker/Engine", "source_revision": "b" * 40}):
            wrong_catalog = {**catalog, "content": base64.b64encode(json.dumps(wrong_pin).encode()).decode()}
            for upstream, pinned_source, submitted in ((store.REPOSITORY, source, request),
                                                        (store.GAMES_REPOSITORY, companion_source, companion_request)):
                with patch.object(client, "source_upstream", return_value=upstream), \
                        patch.object(client, "game_tree", return_value="f" * 40), patch.object(client, "api", return_value=wrong_catalog):
                    with self.assertRaises(store.CommunityError):
                        client.verify_release(store.REPOSITORY, store.GAMES_REPOSITORY, game, pinned_source, submitted)


if __name__ == "__main__":
    unittest.main()
