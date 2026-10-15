"""Black-box checks for README capability-scope maintenance receipts."""

from contextlib import contextmanager
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from tools import readme_scope


class ReadmeScopeTests(unittest.TestCase):
    POLICIES = (
        "docs/adr/0045-windows-only-game-distribution.md",
        "docs/adr/0049-retire-browser-gameplay.md",
    )

    def write(self, root, relative, text):
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def write_json(self, root, relative, value):
        self.write(root, relative, json.dumps(value, indent=2) + "\n")

    def features(self, root):
        return json.loads((root / "tools/FEATURES.json").read_text(encoding="utf-8"))

    @contextmanager
    def repository(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.write(root, "README.md", "# BlueEngine\n\nNative game authoring.\n")
            self.write_json(root, "tools/FEATURES.json", {
                "format": 1,
                "features": {
                    "movement": {
                        "note": "Authoritative movement with collision.",
                        "public_api": ["Controller::look"],
                        "files": ["src/viewer/controller.rs"],
                        "checks": ["cargo test --lib controller"],
                        "tests": ["movement_is_fixed_step"],
                        "evidence": [{"suite": "movement", "test": "walk"}],
                        "canonical_example": "examples/walk.rs",
                        "read_first": ["docs/CONTROLLERS.md"],
                        "examples": ["examples/walk.rs"],
                        "iterate": "cargo test --lib controller",
                    },
                },
            })
            self.write(root, "Cargo.toml", '[package]\nname = "fixture"\n'
                       'version = "0.1.0"\nrust-version = "1.87"\n\n'
                       '[features]\ndefault = ["native"]\nnative = []\nnarrative = []\n')
            self.write_json(root, "templates/starters.json", {
                "schema_version": 1,
                "cli_default": "stock",
                "library_default": "stock",
                "starters": {"stock": {"runtime": "stock", "targets": ["windows"]}},
            })
            for policy in self.POLICIES:
                self.write(root, policy, "# Distribution policy\n\nNative Windows games.\n")
            yield root

    def assert_guarded(self, root):
        errors = readme_scope.errors(root)
        self.assertIsInstance(errors, list)
        self.assertTrue(errors, "Changed public scope must require review")
        self.assertTrue(all(isinstance(error, str) and error for error in errors))

    def test_missing_receipt_requires_an_initial_baseline(self):
        with self.repository() as root:
            self.assert_guarded(root)
            self.assertFalse((root / "docs/README_SCOPE.json").exists())

    def test_initial_update_records_current_readme_without_rewriting_it(self):
        with self.repository() as root:
            before = (root / "README.md").read_bytes()
            receipt = readme_scope.update(root)
            saved = json.loads((root / "docs/README_SCOPE.json").read_text(encoding="utf-8"))
            self.assertEqual(receipt, saved)
            self.assertEqual(receipt["format"], 1)
            self.assertEqual(receipt["readme_sha256"], hashlib.sha256(before).hexdigest())
            self.assertEqual((root / "README.md").read_bytes(), before)
            self.assertEqual(readme_scope.errors(root), [])

    def test_registered_feature_addition_removal_and_public_note_are_guarded(self):
        for change in ("add", "remove", "note", "public_api"):
            with self.subTest(change=change), self.repository() as root:
                readme_scope.update(root)
                document = self.features(root)
                if change == "add":
                    document["features"]["resources"] = {"note": "Bounded resource transactions."}
                elif change == "remove":
                    del document["features"]["movement"]
                elif change == "note":
                    document["features"]["movement"]["note"] = "Movement now supports flight."
                else:
                    document["features"]["movement"]["public_api"].append("Controller::fly")
                self.write_json(root, "tools/FEATURES.json", document)
                self.assert_guarded(root)

    def test_verification_and_source_location_edits_do_not_change_public_scope(self):
        with self.repository() as root:
            readme_scope.update(root)
            original_readme = (root / "README.md").read_bytes()
            document = self.features(root)
            feature = document["features"]["movement"]
            for field in ("files", "checks", "tests", "evidence", "canonical_example",
                          "read_first", "examples", "iterate"):
                feature[field] = ["relocated implementation or revised test"]
            self.write_json(root, "tools/FEATURES.json", document)
            self.assertEqual(readme_scope.errors(root), [])
            readme_scope.update(root)
            self.assertEqual(readme_scope.errors(root), [])
            self.assertEqual((root / "README.md").read_bytes(), original_readme)

    def test_cargo_default_named_feature_and_minimum_rust_changes_are_guarded(self):
        changes = (
            ('default = ["native"]', 'default = ["native", "narrative"]'),
            ('narrative = []', 'narrative = ["native"]'),
            ('rust-version = "1.87"', 'rust-version = "1.88"'),
        )
        for before, after in changes:
            with self.subTest(change=after), self.repository() as root:
                readme_scope.update(root)
                manifest = (root / "Cargo.toml").read_text(encoding="utf-8")
                self.write(root, "Cargo.toml", manifest.replace(before, after))
                self.assert_guarded(root)

    def test_starter_contract_changes_are_guarded(self):
        with self.repository() as root:
            readme_scope.update(root)
            path = root / "templates/starters.json"
            starters = json.loads(path.read_text(encoding="utf-8"))
            starters["starters"]["stock"]["targets"].append("linux")
            self.write_json(root, "templates/starters.json", starters)
            self.assert_guarded(root)

    def test_each_distribution_policy_is_part_of_the_scope(self):
        for policy in self.POLICIES:
            with self.subTest(policy=policy), self.repository() as root:
                readme_scope.update(root)
                text = (root / policy).read_text(encoding="utf-8")
                self.write(root, policy, text + "\nA new distribution rule.\n")
                self.assert_guarded(root)

    def test_readme_edit_is_detected_and_can_be_recorded_without_an_exception(self):
        with self.repository() as root:
            readme_scope.update(root)
            revised = b"# BlueEngine\n\nAuthor native games with optional contextual dialogue.\n"
            (root / "README.md").write_bytes(revised)
            self.assert_guarded(root)
            receipt = readme_scope.update(root)
            self.assertEqual(receipt["readme_sha256"], hashlib.sha256(revised).hexdigest())
            self.assertEqual((root / "README.md").read_bytes(), revised)
            self.assertEqual(readme_scope.errors(root), [])

    def test_scope_change_cannot_silently_refresh_an_unchanged_readme(self):
        with self.repository() as root:
            readme_scope.update(root)
            receipt_path = root / "docs/README_SCOPE.json"
            receipt_before = receipt_path.read_bytes()
            readme_before = (root / "README.md").read_bytes()
            document = self.features(root)
            document["features"]["movement"]["note"] = "Movement supports another public mode."
            self.write_json(root, "tools/FEATURES.json", document)
            with self.assertRaises(ValueError):
                readme_scope.update(root)
            self.assertEqual(receipt_path.read_bytes(), receipt_before)
            self.assertEqual((root / "README.md").read_bytes(), readme_before)
            self.assert_guarded(root)

    def test_explicit_nonempty_unchanged_reason_is_retained(self):
        with self.repository() as root:
            readme_scope.update(root)
            readme_before = (root / "README.md").read_bytes()
            document = self.features(root)
            document["features"]["movement"]["public_api"].append("Controller::turn")
            self.write_json(root, "tools/FEATURES.json", document)
            with self.assertRaises(ValueError):
                readme_scope.update(root, unchanged_reason="   \n")
            reason = "README already describes this controller API at the intended level."
            receipt = readme_scope.update(root, unchanged_reason=reason)
            self.assertEqual(receipt["unchanged_reason"], reason)
            self.assertEqual((root / "README.md").read_bytes(), readme_before)
            self.assertEqual(readme_scope.errors(root), [])

    def test_noop_update_preserves_the_applicable_unchanged_reason(self):
        with self.repository() as root:
            readme_scope.update(root)
            document = self.features(root)
            document["features"]["movement"]["public_api"].append("Controller::turn")
            self.write_json(root, "tools/FEATURES.json", document)
            reason = "The README already covers the controller's movement API."
            approved = readme_scope.update(root, unchanged_reason=reason)
            readme_before = (root / "README.md").read_bytes()

            refreshed = readme_scope.update(root)

            saved = json.loads((root / "docs/README_SCOPE.json").read_text(encoding="utf-8"))
            self.assertEqual(refreshed["unchanged_reason"], reason)
            self.assertEqual(saved["unchanged_reason"], reason)
            self.assertEqual(refreshed["readme_sha256"], approved["readme_sha256"])
            self.assertEqual((root / "README.md").read_bytes(), readme_before)
            self.assertEqual(readme_scope.errors(root), [])

    def test_malformed_receipt_and_unsupported_format_are_reported(self):
        for malformed in ("not JSON", "[]", "{}", "unsupported format"):
            with self.subTest(receipt=malformed), self.repository() as root:
                receipt = readme_scope.update(root)
                if malformed == "unsupported format":
                    receipt["format"] = 999
                    self.write_json(root, "docs/README_SCOPE.json", receipt)
                else:
                    self.write(root, "docs/README_SCOPE.json", malformed)
                self.assert_guarded(root)

    def test_malformed_capability_inputs_fail_without_rewriting_receipt_or_readme(self):
        malformed_inputs = (
            ("tools/FEATURES.json", '{"features": []}'),
            ("Cargo.toml", "[features\n"),
            ("templates/starters.json", "not JSON"),
        )
        for path, malformed in malformed_inputs:
            with self.subTest(path=path), self.repository() as root:
                readme_scope.update(root)
                receipt_before = (root / "docs/README_SCOPE.json").read_bytes()
                readme_before = (root / "README.md").read_bytes()
                self.write(root, path, malformed)
                self.assert_guarded(root)
                with self.assertRaises(ValueError):
                    readme_scope.update(root)
                self.assertEqual((root / "docs/README_SCOPE.json").read_bytes(), receipt_before)
                self.assertEqual((root / "README.md").read_bytes(), readme_before)

    def test_malformed_cargo_table_types_are_diagnosed_without_losing_receipt(self):
        for table in ("features", "package"):
            for value in ("[]", '"not a table"'):
                with self.subTest(table=table, value=value), self.repository() as root:
                    readme_scope.update(root)
                    receipt_before = (root / "docs/README_SCOPE.json").read_bytes()
                    readme_before = (root / "README.md").read_bytes()
                    valid_other_table = ('[package]\nname = "fixture"\nversion = "0.1.0"\n'
                                         'rust-version = "1.87"\n' if table == "features" else
                                         '[features]\ndefault = []\n')
                    self.write(root, "Cargo.toml", f"{table} = {value}\n" + valid_other_table)

                    errors = readme_scope.errors(root)

                    self.assertTrue(errors)
                    self.assertTrue(any("Cargo.toml" in error and table in error.lower()
                                        for error in errors), errors)
                    with self.assertRaises(ValueError):
                        readme_scope.update(root)
                    self.assertEqual((root / "docs/README_SCOPE.json").read_bytes(), receipt_before)
                    self.assertEqual((root / "README.md").read_bytes(), readme_before)


if __name__ == "__main__":
    unittest.main()
