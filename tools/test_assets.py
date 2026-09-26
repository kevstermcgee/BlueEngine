"""Contract tests for the unified asset catalog API."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tools" / "assets.py"


class AssetCatalogTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="be2-assets-test-")
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)

    def call(self, *args, ok=True):
        run = subprocess.run(
            [sys.executable, str(SCRIPT), *map(str, args)],
            cwd=self.directory,
            capture_output=True,
            text=True,
            timeout=30,
        )
        data = json.loads(run.stdout)
        self.assertEqual(run.returncode, 0 if ok else 1, data)
        self.assertEqual(data["ok"], ok, data)
        self.assertEqual(data["api_version"], 1)
        return data

    def fixture_pack(self):
        source = self.directory / "crate.json"
        source.write_text("{}\n", encoding="utf-8")
        manifest = self.directory / "pack.json"
        manifest.write_text(json.dumps({
            "schema_version": 1,
            "pack": {
                "id": "games/test", "name": "Test Game", "version": "0.1.0",
                "scope": "game-local", "license": "MIT"
            },
            "assets": [{
                "id": "crate", "label": "Supply crate",
                "description": "A reusable supply crate.", "type": "prefab",
                "source": {
                    "path": "crate.json", "format": "vesper-scene-v1",
                    "method": "modified", "derived_from": "core/interiors/carton"
                },
                "taxonomy": {
                    "categories": ["storage"], "tags": ["crate", "supply"],
                    "aliases": ["supply box"]
                },
                "geometry": {
                    "units": "meters", "origin": "bottom-center",
                    "half_extents": [0.3, 0.3, 0.3]
                },
                "physics": {"mobility": "static", "collision": "aabb"},
                "lifecycle": {"status": "candidate"}
            }]
        }), encoding="utf-8")
        return manifest

    def test_describe_validate_and_schema(self):
        description = self.call("describe")
        self.assertEqual(description["counts"], {"packs": 2, "assets": 70})
        self.assertTrue(self.call("validate")["valid"])
        self.assertEqual(
            self.call("schema", "pack")["schema"]["title"],
            "BlueEngine Asset Pack",
        )

    def test_search_filters_and_resolution(self):
        search = self.call("search", "small desk lamp", "--limit", 3)
        self.assertEqual(search["matches"][0]["id"], "core/native/table_lamp_1")
        filtered = self.call(
            "list", "--pack", "core/interiors", "--tag", "food-and-drink"
        )
        self.assertGreater(filtered["count"], 0)
        shown = self.call("show", "school desk")["asset"]
        self.assertEqual(shown["id"], "core/interiors/student-desk")
        self.call("search", "lamp", "--limit", 51, ok=False)
        self.call("show", "missing", ok=False)

    def test_include_and_promotion_proposal(self):
        manifest = self.fixture_pack()
        shown = self.call(
            "--include", manifest, "show", "games/test/crate"
        )["asset"]
        self.assertEqual(shown["source"]["method"], "modified")
        output = self.directory / "proposal.json"
        proposal = self.call("promote", manifest, "crate", output)
        self.assertEqual(proposal["proposal"]["status"], "review-required")
        self.assertTrue(output.is_file())
        self.call("promote", manifest, "crate", output, ok=False)

    def test_init_pack_is_non_destructive(self):
        manifest = self.directory / "game" / "assets.json"
        self.call(
            "init-pack", manifest, "--id", "games/example", "--name", "Example"
        )
        self.call("--include", manifest, "validate")
        self.call(
            "init-pack", manifest, "--id", "games/example", "--name", "Example",
            ok=False,
        )

    def test_rejects_missing_or_escaping_sources(self):
        manifest = self.fixture_pack()
        data = json.loads(manifest.read_text(encoding="utf-8"))
        data["assets"][0]["source"]["path"] = "../outside.json"
        manifest.write_text(json.dumps(data), encoding="utf-8")
        self.call("--include", manifest, "validate", ok=False)


if __name__ == "__main__":
    unittest.main()
