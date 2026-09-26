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

    def test_publish_keeps_explicit_external_path_and_removes_other_stale_files(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            root = self.fixture(base)
            output = base / "games-repo"
            (output / "games" / "external").mkdir(parents=True)
            (output / "games" / "external" / "keep.txt").write_text(
                "independent game", encoding="utf-8"
            )
            (output / "games" / "stale.txt").write_text("stale", encoding="utf-8")

            catalog = publish_games.publish(root, output, "abc123")

            self.assertEqual(
                (output / "games" / "external" / "keep.txt").read_text(encoding="utf-8"),
                "independent game",
            )
            self.assertFalse((output / "games" / "stale.txt").exists())
            self.assertEqual(catalog["preserved_paths"], ["games/external"])

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


if __name__ == "__main__":
    unittest.main()
