import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("publish_games.py")
SPEC = importlib.util.spec_from_file_location("publish_games", MODULE_PATH)
publish_games = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(publish_games)


class PublishGamesTests(unittest.TestCase):
    def fixture(self, directory: Path) -> Path:
        root = directory / "engine"
        root.mkdir()
        for category in publish_games.CATEGORIES:
            source = root / f"source-{category}"
            source.mkdir()
            (source / "item.txt").write_text(category, encoding="utf-8")
        manifest = {
            "version": 2,
            "source_repository": "example/engine",
            "target_repository": "example/games",
            "collections": {
                category: [{"source": f"source-{category}", "destination": "."}]
                for category in publish_games.CATEGORIES
            },
            "preserve": ["games/external"],
            "playables": [
                {
                    "slug": "sample",
                    "name": "Sample",
                    "entry": "games/item.txt",
                    "files": ["games/item.txt"],
                    "arguments": [],
                }
            ],
        }
        (root / publish_games.MANIFEST_NAME).write_text(
            json.dumps(manifest), encoding="utf-8"
        )
        return root

    # -- helpers --------------------------------------------------------------------------------------

    def setup_repos(self, temp):
        base = Path(temp)
        root = self.fixture(base)
        output = base / "games-repo"
        return root, output

    def manifest(self, root: Path) -> dict:
        return json.loads((root / publish_games.MANIFEST_NAME).read_text(encoding="utf-8"))

    def write_manifest(self, root: Path, manifest: dict) -> None:
        (root / publish_games.MANIFEST_NAME).write_text(json.dumps(manifest), encoding="utf-8")

    def snapshot(self, output: Path) -> dict:
        return {
            path.relative_to(output).as_posix(): path.read_bytes()
            for path in sorted(output.rglob("*"))
            if path.is_file()
        }

    def catalog_paths(self, output: Path) -> set:
        data = json.loads((output / publish_games.CATALOG_NAME).read_text(encoding="utf-8"))
        return {item["path"] for item in data["files"]}

    # -- ownership ------------------------------------------------------------------------------------

    def test_leo_generated_audio_does_not_change_source_exports(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'assets/games/leo'
            (source / 'assets/audio-source').mkdir(parents=True)
            (source / 'assets/audio-source/music.json').write_text('score')
            clean = {nested.as_posix() for _, nested in publish_games.files_under(source, root=root)}
            (source / 'assets/audio/music').mkdir(parents=True)
            (source / 'assets/audio/music/day.wav').write_text('rendered')
            (source / 'assets/audio-source/leaves.wav').write_text('generated')
            warm = {nested.as_posix() for _, nested in publish_games.files_under(source, root=root)}
            self.assertEqual(clean, warm)
            self.assertIn('assets/audio-source/music.json', warm)

    def test_maintained_native_projects_export_real_build_files(self):
        # The manifest's destination is a directory even for a single source file.
        # Checking the production manifest catches Cargo.toml/Cargo.toml exports.
        root = Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp)
            publish_games.export_tree(root, output, publish_games.load_manifest(root), 'test')
            for slug in ('signal-garden', 'lantern-run', 'pocket-breaker', 'orchard-watch', 'lantern-grove'):
                for name in ('Cargo.toml', 'Cargo.lock', 'build.rs', 'game.project.json' if slug != 'signal-garden' else 'README.md', 'scripts/ship.py'):
                    with self.subTest(game=slug, file=name):
                        self.assertTrue((output / 'games' / slug / name).is_file())

    def test_native_build_outputs_and_player_files_never_become_published_source(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            game = root / "source-games" / "native"
            game.mkdir()
            (game / "Cargo.toml").write_text('[package]\nname = "native"\n')
            for name in ("target/debug/Game.exe", "dist/saves/quick.be2save", ".blue-check/run/log.txt",
                         ".be2-work/capture.png", "__pycache__/module.pyc", ".git/config"):
                path = game / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("local output, never source")
            asset = game / "assets" / "target" / "model.json"
            asset.parent.mkdir(parents=True)
            asset.write_text("authored target model")
            publish_games.publish(root, output, "r1")
            paths = self.catalog_paths(output)
            self.assertIn("games/native/assets/target/model.json", paths)
            self.assertIn("games/native/Cargo.toml", paths)
            self.assertEqual(len(paths), 6)

    def test_an_unknown_hand_added_game_survives_without_a_preserve_entry(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            publish_games.publish(root, output, "r1")
            (output / "games" / "handmade").mkdir()
            (output / "games" / "handmade" / "game.txt").write_text("mine", encoding="utf-8")
            (output / "games" / "loose.txt").write_text("also mine", encoding="utf-8")
            publish_games.publish(root, output, "r2")
            self.assertEqual((output / "games" / "handmade" / "game.txt").read_text(encoding="utf-8"), "mine")
            self.assertEqual((output / "games" / "loose.txt").read_text(encoding="utf-8"), "also mine")

    def test_the_first_export_with_no_catalog_deletes_nothing(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            (output / "games").mkdir(parents=True)
            (output / "games" / "stale.txt").write_text("unknown provenance", encoding="utf-8")
            publish_games.publish(root, output, "r1")
            self.assertTrue((output / "games" / "stale.txt").exists())

    def test_explicit_preserve_entry_still_survives(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            (output / "games" / "external").mkdir(parents=True)
            (output / "games" / "external" / "keep.txt").write_text("independent game", encoding="utf-8")
            catalog = publish_games.publish(root, output, "abc123")
            self.assertEqual((output / "games" / "external" / "keep.txt").read_text(encoding="utf-8"), "independent game")
            self.assertEqual(catalog["preserved_paths"], ["games/external"])

    def test_stale_owned_files_are_removed_and_empty_directories_go_with_them(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            nested = root / "source-games" / "old" / "deep"
            nested.mkdir(parents=True)
            (nested / "gone.txt").write_text("soon stale", encoding="utf-8")
            publish_games.publish(root, output, "r1")
            self.assertTrue((output / "games" / "old" / "deep" / "gone.txt").exists())
            (nested / "gone.txt").unlink()
            nested.rmdir()
            (root / "source-games" / "old").rmdir()
            summary = publish_games.publish(root, output, "r2")["summary"]
            self.assertEqual(summary["removed"], 1)
            self.assertFalse((output / "games" / "old").exists())
            self.assertTrue((output / "games").is_dir(), "a collection root is never removed")
            self.assertNotIn("games/old/deep/gone.txt", self.catalog_paths(output))

    def test_a_removed_manifest_entry_removes_only_what_it_exported(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            manifest = self.manifest(root)
            extra = root / "extra"
            extra.mkdir()
            (extra / "extra.txt").write_text("extra", encoding="utf-8")
            manifest["collections"]["demos"].append({"source": "extra", "destination": "extra"})
            self.write_manifest(root, manifest)
            publish_games.publish(root, output, "r1")
            (output / "demos" / "hand-added.txt").write_text("keep me", encoding="utf-8")
            manifest["collections"]["demos"].pop()
            self.write_manifest(root, manifest)
            publish_games.publish(root, output, "r2")
            self.assertFalse((output / "demos" / "extra" / "extra.txt").exists())
            self.assertTrue((output / "demos" / "hand-added.txt").exists())
            self.assertTrue((output / "demos" / "item.txt").exists())

    def test_exports_are_idempotent(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            publish_games.publish(root, output, "r1")
            first = self.snapshot(output)
            summary = publish_games.publish(root, output, "r1")["summary"]
            self.assertEqual(self.snapshot(output), first)
            self.assertEqual((summary["created"], summary["updated"], summary["removed"]), (0, 0, 0))
            self.assertFalse(list(output.glob(".games-publish-*")), "no staging directory is left behind")

    def test_a_changed_source_updates_an_unmodified_exported_file(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            publish_games.publish(root, output, "r1")
            (root / "source-games" / "item.txt").write_text("new content", encoding="utf-8")
            summary = publish_games.publish(root, output, "r2")["summary"]
            self.assertEqual(summary["updated"], 1)
            self.assertEqual((output / "games" / "item.txt").read_text(encoding="utf-8"), "new content")

    # -- conflicts ------------------------------------------------------------------------------------

    def test_an_unowned_file_at_a_destination_is_refused_not_claimed(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            (output / "games").mkdir(parents=True)
            (output / "games" / "item.txt").write_text("someone else's", encoding="utf-8")
            with self.assertRaisesRegex(publish_games.PublishError, "unowned"):
                publish_games.publish(root, output, "r1")
            self.assertEqual((output / "games" / "item.txt").read_text(encoding="utf-8"), "someone else's")
            self.assertFalse((output / publish_games.CATALOG_NAME).exists())
            self.assertFalse((output / "prototypes").exists(), "nothing else is written either")

    def test_an_identical_unowned_file_is_not_a_conflict(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            (output / "games").mkdir(parents=True)
            (output / "games" / "item.txt").write_text("games", encoding="utf-8")
            publish_games.publish(root, output, "r1")
            self.assertIn("games/item.txt", self.catalog_paths(output))

    def test_an_edited_exported_file_is_a_conflict_and_nothing_else_changes(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            publish_games.publish(root, output, "r1")
            (output / "games" / "item.txt").write_text("edited in the games repo", encoding="utf-8")
            (root / "source-tests" / "item.txt").write_text("changed upstream", encoding="utf-8")
            before = self.snapshot(output)
            with self.assertRaisesRegex(publish_games.PublishError, "edited in the output"):
                publish_games.publish(root, output, "r2")
            self.assertEqual(self.snapshot(output), before)

    def test_a_stale_file_edited_in_the_output_is_kept_and_reported(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            manifest = self.manifest(root)
            extra = root / "extra"
            extra.mkdir()
            (extra / "x.txt").write_text("x", encoding="utf-8")
            manifest["collections"]["demos"].append({"source": "extra", "destination": "extra"})
            self.write_manifest(root, manifest)
            publish_games.publish(root, output, "r1")
            (output / "demos" / "extra" / "x.txt").write_text("my edit", encoding="utf-8")
            manifest["collections"]["demos"].pop()
            self.write_manifest(root, manifest)
            with self.assertRaisesRegex(publish_games.PublishError, "not deleting"):
                publish_games.publish(root, output, "r2")
            self.assertEqual((output / "demos" / "extra" / "x.txt").read_text(encoding="utf-8"), "my edit")

    def test_dry_run_reports_conflicts_and_changes_nothing(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            result = publish_games.publish(root, output, "r1", dry_run=True)
            self.assertEqual(result["summary"]["created"], 4)
            self.assertFalse((output / publish_games.CATALOG_NAME).exists())
            self.assertFalse((output / "games").exists())

    # -- malformed ownership metadata -----------------------------------------------------------------

    def test_malformed_ownership_metadata_stops_the_export_with_a_diagnostic(self):
        bad_catalogs = {
            "not json": "{oops",
            "no files list": json.dumps({"schema_version": 1}),
            "a non-object entry": json.dumps({"files": ["games/item.txt"]}),
            "a bad hash": json.dumps({"files": [{"path": "games/item.txt", "sha256": "abc"}]}),
            "a traversal path": json.dumps({"files": [{"path": "games/../../x", "sha256": "0" * 64}]}),
            "an unmanaged path": json.dumps({"files": [{"path": "README.md", "sha256": "0" * 64}]}),
            "a duplicate path": json.dumps(
                {"files": [{"path": "games/a", "sha256": "0" * 64}, {"path": "games/a", "sha256": "0" * 64}]}
            ),
        }
        for label, text in bad_catalogs.items():
            with self.subTest(label), tempfile.TemporaryDirectory() as temp:
                root, output = self.setup_repos(temp)
                (output / "games").mkdir(parents=True)
                (output / "games" / "precious.txt").write_text("keep", encoding="utf-8")
                (output / publish_games.CATALOG_NAME).write_text(text, encoding="utf-8")
                before = self.snapshot(output)
                with self.assertRaises(publish_games.PublishError) as caught:
                    publish_games.publish(root, output, "r1")
                self.assertIn("nothing was changed", str(caught.exception))
                self.assertEqual(self.snapshot(output), before)

    # -- containment ----------------------------------------------------------------------------------

    def test_a_symlinked_collection_directory_is_refused_before_anything_is_written(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            outside = Path(temp) / "outside"
            outside.mkdir()
            output.mkdir()
            (output / "games").symlink_to(outside, target_is_directory=True)
            with self.assertRaisesRegex(publish_games.PublishError, "symlink"):
                publish_games.publish(root, output, "r1")
            self.assertEqual(list(outside.iterdir()), [])
            self.assertFalse((output / "prototypes").exists())

    def test_a_symlinked_nested_parent_is_refused(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            nested = root / "source-games" / "sub"
            nested.mkdir()
            (nested / "f.txt").write_text("f", encoding="utf-8")
            outside = Path(temp) / "outside"
            outside.mkdir()
            (output / "games").mkdir(parents=True)
            (output / "games" / "sub").symlink_to(outside, target_is_directory=True)
            with self.assertRaisesRegex(publish_games.PublishError, "symlink"):
                publish_games.publish(root, output, "r1")
            self.assertEqual(list(outside.iterdir()), [])

    def test_a_catalog_path_cannot_delete_through_a_symlinked_parent(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            outside = Path(temp) / "outside"
            outside.mkdir()
            victim = outside / "victim.txt"
            victim.write_text("outside the repository", encoding="utf-8")
            digest = publish_games.sha256_file(victim)
            (output / "games").mkdir(parents=True)
            (output / "games" / "link").symlink_to(outside, target_is_directory=True)
            (output / publish_games.CATALOG_NAME).write_text(
                json.dumps({"files": [{"path": "games/link/victim.txt", "sha256": digest}]}), encoding="utf-8"
            )
            with self.assertRaises(publish_games.PublishError):
                publish_games.publish(root, output, "r1")
            self.assertTrue(victim.exists())

    def test_manifest_paths_cannot_escape_the_repository(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            manifest = self.manifest(root)
            for entry in ({"source": "../outside"}, {"source": "source-games", "destination": "../../escape"}):
                manifest["collections"]["games"] = [entry]
                self.write_manifest(root, manifest)
                with self.assertRaises(publish_games.PublishError):
                    publish_games.publish(root, output, "r1")
            self.assertFalse((Path(temp) / "escape").exists())

    # -- failure --------------------------------------------------------------------------------------

    def test_a_failure_part_way_never_leaves_a_partial_file_and_a_rerun_converges(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            publish_games.publish(root, output, "r1")
            for category in publish_games.CATEGORIES:
                (root / f"source-{category}" / "item.txt").write_text(f"{category} v2", encoding="utf-8")
            old_catalog = (output / publish_games.CATALOG_NAME).read_bytes()
            real = publish_games._replace
            calls = []

            def flaky(source, target):
                calls.append(target)
                if len(calls) == 3:
                    raise OSError("disk fell over")
                real(source, target)

            publish_games._replace = flaky
            try:
                with self.assertRaises(OSError):
                    publish_games.publish(root, output, "r2")
            finally:
                publish_games._replace = real
            contents = {c: (output / c / "item.txt").read_text(encoding="utf-8") for c in publish_games.CATEGORIES}
            for category, text in contents.items():
                self.assertIn(text, (category, f"{category} v2"), "every file is entirely old or entirely new")
            self.assertEqual(sum(1 for c, t in contents.items() if t.endswith("v2")), 2)
            self.assertEqual((output / publish_games.CATALOG_NAME).read_bytes(), old_catalog)
            self.assertFalse(list(output.glob(".games-publish-*")), "staging is cleaned up after a failure")
            publish_games.publish(root, output, "r2")
            for category in publish_games.CATEGORIES:
                self.assertEqual((output / category / "item.txt").read_text(encoding="utf-8"), f"{category} v2")

    def test_a_failure_while_staging_changes_nothing(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            publish_games.publish(root, output, "r1")
            (root / "source-games" / "item.txt").write_text("v2", encoding="utf-8")
            before = self.snapshot(output)
            real = publish_games.shutil.copy2

            def failing(source, target, **kwargs):
                if ".games-publish-" in str(target):
                    raise OSError("no space left on device")
                return real(source, target, **kwargs)

            publish_games.shutil.copy2 = failing
            try:
                with self.assertRaises(OSError):
                    publish_games.publish(root, output, "r2")
            finally:
                publish_games.shutil.copy2 = real
            self.assertEqual(self.snapshot(output), before)

    def test_preserve_rejects_collection_roots_and_traversal(self):
        with tempfile.TemporaryDirectory() as temp:
            root = self.fixture(Path(temp))
            manifest_path = root / publish_games.MANIFEST_NAME
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            for invalid in (["games"], ["../outside"]):
                manifest["preserve"] = invalid
                manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
                with self.assertRaises(publish_games.PublishError):
                    publish_games.load_manifest(root)

    def test_check_and_export_reject_overlapping_preservation_without_touching_output(self):
        with tempfile.TemporaryDirectory() as temp:
            root, output = self.setup_repos(temp)
            publish_games.publish(root, output, "r1")
            before = self.snapshot(output)
            manifest = self.manifest(root)
            manifest["preserve"] = ["games/item.txt"]
            self.write_manifest(root, manifest)
            with self.assertRaisesRegex(publish_games.PublishError, "preserved path overlaps"):
                publish_games.check(root, "r2")
            with self.assertRaisesRegex(publish_games.PublishError, "preserved path overlaps"):
                publish_games.publish(root, output, "r2")
            self.assertEqual(self.snapshot(output), before)


if __name__ == "__main__":
    unittest.main()
