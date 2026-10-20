"""Creator checkout safety, durable retries, and native release orchestration."""
from copy import deepcopy
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

from tools import community, community_release


class CreatorTests(unittest.TestCase):
    def test_standalone_import_reads_exact_git_blobs_without_recursive_copy(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            community.command(["git", "init", "-q", str(root)])
            (root / "Cargo.toml").write_text('[package]\nname="original"\nversion="0.1.0"\n')
            (root / "assets").mkdir()
            (root / "assets/bytes.bin").write_bytes(b"\x00\xff\x01")
            (root / "dist").mkdir()
            (root / "dist/old.exe").write_bytes(b"excluded build")
            community.command(["git", "add", "."], root)
            community.command(["git", "-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "source"], root)
            pin = community.command(["git", "rev-parse", "HEAD"], root)
            target = root / "games/new-fork"
            original_command = community.command
            def commands(argv, cwd=None):
                return "" if "fetch" in argv else original_command(argv, cwd)
            with patch.object(community, "command", side_effect=commands):
                community.import_standalone(["git"], "owner/standalone", pin, root, target)
            self.assertEqual((target / "assets/bytes.bin").read_bytes(), b"\x00\xff\x01")
            self.assertFalse((target / "dist").exists())
            self.assertFalse((target / "games").exists())
            self.assertEqual((target / "Cargo.toml").read_bytes(), (root / "Cargo.toml").read_bytes())

    def test_standalone_pin_cannot_import_symlink_or_parent_escape(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for tree in (f"120000 blob {'a' * 40}\tCargo.toml\0", f"100644 blob {'a' * 40}\t../outside\0"):
                with patch.object(community, "command", side_effect=["", tree]):
                    with self.assertRaises(community.CommunityError):
                        community.import_standalone(["git"], "owner/standalone", "b" * 40, root, root / "game")
                self.assertFalse((root / "game").exists())

    def test_standalone_import_rejects_windows_git_metadata_aliases_and_streams(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in (".GIT/hooks/pre-commit", "sub/.Git/config", "git~1/config", "src/file:stream", "CON.txt", "src/trailing. "):
                tree = f"100644 blob {'a' * 40}\t{name}\0"
                with patch.object(community, "command", side_effect=["", tree]):
                    with self.assertRaisesRegex(community.CommunityError, "unsafe path"):
                        community.import_standalone(["git"], "owner/standalone", "b" * 40, root, root / "game")
            self.assertFalse((root / "game").exists())

    def test_standalone_import_preserves_source_by_rejecting_case_collisions(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tree = "".join(f"100644 blob {'a' * 40}\t{name}\0" for name in ("Cargo.toml", "Assets/one.png", "assets/two.png"))
            with patch.object(community, "command", side_effect=["", tree]):
                with self.assertRaisesRegex(community.CommunityError, "collide on Windows"):
                    community.import_standalone(["git"], "owner/standalone", "b" * 40, root, root / "game")
            self.assertFalse((root / "game").exists())

    def test_invalid_credential_cannot_reach_header_errors_or_diagnostics(self):
        with patch.dict(community.os.environ, {"BLUEENGINE_COMMUNITY_TOKEN": "private-test-value\n"}):
            with self.assertRaises(community.CommunityError) as caught:
                community.token()
        self.assertNotIn("private-test-value", str(caught.exception))

    def test_existing_game_checkout_cannot_follow_source_metadata_symlink(self):
        game = {"slug": "original", "source": {"repository": "owner/repo", "commit": "a" * 40,
                                                 "game_path": "games/original"}}
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "checkout"
            victim = Path(temporary) / "existing-user-file"
            victim.write_text("preserve this file")
            def git(argv, cwd=None):
                if "clone" in argv:
                    source = destination / "games/original"
                    source.mkdir(parents=True)
                    try:
                        (source / "community-source.json").symlink_to(victim)
                    except OSError:
                        self.skipTest("Host does not allow symlink fixtures")
                return ""
            with patch.object(community, "command", side_effect=git):
                with self.assertRaisesRegex(community.CommunityError, "symlinks"):
                    community.checkout(game, destination)
            self.assertEqual(victim.read_text(), "preserve this file")

    def test_fork_receipt_and_public_game_resolve_same_head_source(self):
        source = {"repository": "owner/repo", "commit": "a" * 40, "game_path": "games/original"}
        revision = {"id": "b" * 64, "source": source}
        game = {"slug": "fork-game", "head_revision": revision["id"]}
        fork = {"game": game, "event": {"revision": revision, "game": game}}
        public = {**game, "revisions": [revision]}
        self.assertEqual(community.source_document(community.game_document(fork)), community.source_document(public))

    def test_remote_credentials_require_https_and_no_redirects(self):
        for endpoint in ("http://example.com/community/v1", "https://user:token@example.com", "https://example.com?token=private"):
            with self.assertRaises(community.CommunityError):
                community.Client(endpoint)
        community.Client("http://127.0.0.1:8091/community/v1")
        with self.assertRaises(community.CommunityError):
            community.NoRedirect().redirect_request(None)

    def test_failed_submission_retry_keeps_identity_without_storing_token_or_note(self):
        with tempfile.TemporaryDirectory() as temporary:
            client = community.Client(state_directory=temporary)
            client.opener = Mock()
            responses = []
            def receive(request, **kwargs):
                responses.append(json.loads(request.data))
                if len(responses) == 1:
                    raise TimeoutError()
                answer = io.BytesIO(json.dumps({"accepted": True, "request_id": responses[-1]["request_id"]}).encode())
                return answer
            client.opener.open.side_effect = receive
            body = {"note": "Feedback text must not be stored in the local retry journal"}
            with patch.object(community, "token", return_value="private-test-value"):
                with self.assertRaises(community.CommunityError):
                    client.request("/feedback", body)
                self.assertTrue(client.request("/feedback", body)["accepted"])
            self.assertEqual(responses[0], responses[1])
            text = "".join(path.read_text() for path in Path(temporary).iterdir())
            self.assertNotIn("private-test-value", text)
            self.assertNotIn(body["note"], text)
            self.assertNotIn("request_id", body)

    def test_post_requires_exact_remote_acceptance_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            client = community.Client(state_directory=temporary)
            for result in ({"accepted": False}, {"accepted": True},
                           {"accepted": True, "request_id": "another-request"}):
                client.opener = Mock()
                client.opener.open.return_value = io.BytesIO(json.dumps(result).encode())
                with patch.object(community, "token", return_value="private-test-value"):
                    with self.assertRaisesRegex(community.CommunityError, "did not confirm"):
                        client.request("/feedback", {"note": "test"})

    def test_source_pin_rejects_mutable_refs_paths_and_non_github_repository(self):
        source = {"slug": "fork-game", "source": {"repository": "owner/repo", "commit": "a" * 40,
                                                    "game_path": "games/original"}}
        self.assertEqual(community.source_document(source)[1], "a" * 40)
        for field, value in (("commit", "main"), ("game_path", "../outside"),
                             ("game_path", "games/../../outside"), ("game_path", "games\\outside"),
                             ("repository", "owner/repo;touch /tmp/bad")):
            invalid = deepcopy(source)
            invalid["source"][field] = value
            with self.assertRaises(community.CommunityError):
                community.source_document(invalid)

    def test_checkout_preserves_original_and_rebinds_fork_identity_without_executing_source(self):
        game = {"slug": "my-fork", "title": "My Fork", "source": {"repository": "owner/repo",
                "commit": "a" * 40, "game_path": "games/original"}}
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "checkout"
            commands = []
            def git(argv, cwd=None):
                commands.append(argv)
                if "clone" in argv:
                    original = destination / "games/original"
                    (original / "assets").mkdir(parents=True)
                    (original / "game.project.json").write_text('{"id":"original"}')
                    (original / "assets/identity.json").write_text('{"title":"Original"}')
                    (original / "src").mkdir()
                    (original / "src/main.rs").write_text("unchanged source")
                return ""
            with patch.object(community, "command", side_effect=git):
                receipt = community.checkout(game, destination)
            self.assertEqual(json.loads((destination / "games/original/game.project.json").read_text())["id"], "original")
            self.assertEqual(json.loads((destination / "games/my-fork/game.project.json").read_text())["id"], "my-fork")
            self.assertEqual((destination / "games/my-fork/src/main.rs").read_text(), "unchanged source")
            self.assertEqual(receipt["source"]["commit"], "a" * 40)
            self.assertTrue(all(argv[0] == "git" for argv in commands))
            with self.assertRaisesRegex(community.CommunityError, "already exists"):
                community.checkout(game, destination)


class NativeReleaseTests(unittest.TestCase):
    def test_human_creation_credit_passes_release_wiki_gates_without_relaxing_ideaforge(self):
        from tools.test_idea_forge import wiki_fixture
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            game = root / 'game'
            wiki = wiki_fixture(game)
            wiki['design'] = {'origin': 'human', 'original_prompt': 'Build a rule-transfer puzzle with three machines.'}
            community_release.forge.atomic_json(game / 'wiki.json', wiki)
            release = community_release.Release(root, {'target_dir': temporary})
            self.assertEqual(release.validate_wiki(game)['design']['origin'], 'human')
            with self.assertRaises(community_release.forge.ForgeError):
                community_release.forge.validate_game_wiki(game)
            wiki['design']['original_prompt'] = ''
            community_release.forge.atomic_json(game / 'wiki.json', wiki)
            with self.assertRaises(community_release.forge.ForgeError):
                release.validate_wiki(game)

    def test_engine_owned_native_title_and_binary_can_update_but_preserved_games_cannot(self):
        release = community_release.Release('/tmp/release-test', {'target_dir': '/tmp', 'idea': {'slug': 'test-game'},
            'engine_repository': 'owner/engine', 'games_repository': 'owner/games'})
        old = {'slug': 'test-game', 'directory': 'games/test-game', 'kind': 'cargo-package', 'name': 'Old title', 'binary': 'old-bin'}
        new = dict(old, name='New title', binary='new-bin')
        catalog = {'source_repository': 'owner/engine', 'target_repository': 'owner/games', 'preserved_paths': [],
                   'files': [{'path': 'games/test-game/Cargo.toml'}, {'path': 'games/test-game/game.project.json'}]}
        manifest = {'native_playables': [old]}
        with patch.object(community_release.forge, 'git', return_value=json.dumps(catalog)):
            self.assertTrue(release.update_native_definition(manifest, new))
        self.assertEqual(manifest['native_playables'], [new])
        for override in ({'preserved_paths': ['games/test-game']}, {'files': []}, {'source_repository': 'other/engine'}):
            with self.subTest(override=override), patch.object(community_release.forge, 'git', return_value=json.dumps({**catalog, **override})):
                manifest = {'native_playables': [dict(old)]}
                with self.assertRaisesRegex(community_release.forge.ForgeError, 'ownership'):
                    release.update_native_definition(manifest, new)
                self.assertEqual(manifest['native_playables'], [old])

    def test_completed_release_records_five_gate_ids_without_republishing(self):
        with tempfile.TemporaryDirectory() as temporary:
            labels = ("engine-checks", "generated-game-linux-windows", "windows-installer-review", "production-release", "download-page")
            state = {"workflow": "community-release", "status": "published", "idea": {"slug": "test-game"},
                     "base": "a" * 40, "engine_head": "b" * 40, "games_head": "c" * 40,
                     "publication": {"release": "https://github.com/owner/repo/releases/tag/test"},
                     "ci": {label: {"run": i + 1} for i, label in enumerate(labels)}}
            Path(temporary, "state.json").write_text(json.dumps(state))
            client = Mock()
            client.request.side_effect = [{"game": {"revisions": [{"id": "d" * 64, "source_commit": state["base"]}]}},
                                          {"accepted": True}]
            self.assertTrue(community.record_release(client, temporary)["accepted"])
            path, body = client.request.call_args.args
            self.assertEqual(path, "/games/test-game/releases")
            self.assertEqual(set(body["gates"]), set(labels))
            self.assertEqual(body["revision_id"], "d" * 64)
            saved = json.loads(Path(temporary, "state.json").read_text())
            self.assertEqual(saved["community_revision"], "d" * 64)

    def test_failed_gate_is_retained_and_resume_does_not_skip_it_or_publish_early(self):
        with tempfile.TemporaryDirectory() as temporary:
            state = {"target_dir": temporary, "idea": {"slug": "test-game"}, "completed": []}
            release = community_release.Release(temporary, state)
            calls = []
            phases = {"prepare": "prepare", "verify_local": "local", "engine_ci": "engine_ci", "export": "export",
                      "installer_review": "installer", "publish_games": "production",
                      "verify_publication": "public_receipt", "publish_source": "source"}
            for name, phase in phases.items():
                setattr(release, name, Mock(side_effect=lambda phase=phase: calls.append(phase)))
            release.installer_review.side_effect = community_release.forge.ForgeError("installer failed")
            with self.assertRaisesRegex(community_release.forge.ForgeError, "installer failed"):
                release.advance()
            self.assertEqual(state["completed"], ["prepare", "local", "engine_ci", "export"])
            release.publish_games.assert_not_called()
            release.publish_source.assert_not_called()
            release.installer_review.side_effect = lambda: calls.append("installer")
            release.verify_publication.side_effect = lambda: state.update(publication={"installer_sha256": "verified"})
            result = release.advance()
            self.assertEqual(calls.count("local"), 1)
            self.assertEqual(calls.count("production"), 1)
            self.assertEqual(result["status"], "published")
            self.assertEqual(result["publication"], {"installer_sha256": "verified"})

    def test_release_source_requires_fresh_gate_assertion_and_fast_forward(self):
        release = community_release.Release("/tmp/release-test", {"target_dir": "/tmp"})
        release.assert_source_gates = Mock(side_effect=community_release.forge.ForgeError("stale CI"))
        release.command = Mock()
        with self.assertRaisesRegex(community_release.forge.ForgeError, "stale CI"):
            release.publish_source()
        release.command.assert_not_called()


class SourceReleaseIntegrationTests(unittest.TestCase):
    """Real Git divergence; hosted checks are stubbed without any network use."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.remote, self.writer, self.directory = [self.root / name for name in ('remote.git', 'writer', 'run')]
        self.directory.mkdir()
        self.engine = self.directory / 'engine'
        self.git(self.root, 'init', '--bare', '--initial-branch=main', str(self.remote))
        self.git(self.root, 'clone', str(self.remote), str(self.writer))
        self.configure(self.writer)
        game = self.writer / 'games/test-game'
        game.mkdir(parents=True)
        (game / 'rules.rs').write_text('original game\n')
        self.commit(self.writer, 'Baseline game')
        self.git(self.writer, 'push', 'origin', 'main')
        self.git(self.root, 'clone', str(self.remote), str(self.engine))
        self.configure(self.engine)
        self.git(self.engine, 'switch', '-c', 'community-release/test-game')
        (self.engine / 'games/test-game/rules.rs').write_text('reviewed released game\n')
        self.commit(self.engine, 'Reviewed immutable release source')
        self.source = self.git(self.engine, 'rev-parse', 'HEAD')
        self.state = {'target_dir': str(self.root), 'timeout': 10, 'idea': {'slug': 'test-game'},
                      'engine_head': self.source, 'branch': 'community-release/test-game',
                      'engine_repository': 'fixture/engine', 'publication': {'release': 'immutable-release', 'installer_sha256': 'reviewed-installer'}}
        self.release = community_release.Release(self.directory, self.state)
        self.release.assert_source_gates = Mock()
        self.release.wait_ci = Mock(side_effect=lambda repo, workflow, branch, head, label, **kwargs: {'head': head})

    @staticmethod
    def git(root, *args):
        return subprocess.run(['git', '-C', str(root), *args], check=True, capture_output=True, text=True).stdout.strip()

    def configure(self, root):
        self.git(root, 'config', 'user.name', 'Fixture')
        self.git(root, 'config', 'user.email', 'fixture@example.invalid')

    def commit(self, root, message):
        self.git(root, 'add', '.')
        self.git(root, 'commit', '-qm', message)

    def advance_main(self, name='feedback.txt'):
        (self.writer / name).write_text('new independent observation\n')
        self.commit(self.writer, 'Concurrent community observation')
        self.git(self.writer, 'push', 'origin', 'main')
        return self.git(self.writer, 'rev-parse', 'HEAD')

    def test_diverged_main_is_integrated_and_rechecked_without_changing_release_receipt(self):
        other = self.advance_main()
        receipt = deepcopy(self.state['publication'])
        self.release.publish_source()
        head = self.git(self.writer, 'ls-remote', 'origin', 'refs/heads/main').split()[0]
        self.assertNotEqual(head, self.source)
        integration = self.directory / 'engine-feedback'
        self.assertTrue(community_release.forge.git_ancestor(integration, self.source, head))
        self.assertTrue(community_release.forge.git_ancestor(integration, other, head))
        self.assertEqual([call.args[1] for call in self.release.wait_ci.call_args_list], ['ci.yml', 'forge-game-checks.yml'])
        self.assertTrue(all(call.args[3] == head for call in self.release.wait_ci.call_args_list))
        self.assertEqual(self.state['engine_head'], self.source)
        self.assertEqual(self.state['publication'], receipt)
        self.assertEqual(self.state['source_publication']['commit'], head)
        self.release.publish_source()  # A confirmed push is safe to resume.
        self.assertEqual(self.git(self.writer, 'ls-remote', 'origin', 'refs/heads/main').split()[0], head)

    def test_failed_merge_ci_retains_source_and_retries_without_publishing(self):
        other = self.advance_main()
        self.release.wait_ci.side_effect = community_release.forge.ForgeError('CI failed')
        with self.assertRaisesRegex(community_release.forge.ForgeError, 'CI failed'):
            self.release.publish_source()
        self.assertEqual(self.git(self.writer, 'ls-remote', 'origin', 'refs/heads/main').split()[0], other)
        self.assertTrue((self.directory / 'engine-feedback').is_dir())
        self.release.wait_ci.side_effect = lambda repo, workflow, branch, head, label, **kwargs: {'head': head}
        self.release.publish_source()
        self.assertNotEqual(self.git(self.writer, 'ls-remote', 'origin', 'refs/heads/main').split()[0], other)

    def test_concurrent_game_tree_change_needs_separate_release_and_is_not_silently_pushed(self):
        other = self.advance_main('games/test-game/new-feature.rs')
        with self.assertRaisesRegex(community_release.forge.ForgeError, 'released game'):
            self.release.publish_source()
        self.release.wait_ci.assert_not_called()
        self.assertEqual(self.git(self.writer, 'ls-remote', 'origin', 'refs/heads/main').split()[0], other)


if __name__ == "__main__":
    unittest.main()
