#!/usr/bin/env python3
"""Release companion-owned native source without exporting or editing engine source.

All builds use private worktrees and the catalog's immutable engine revision.
Existing companion Cargo games keep their source layout and release definitions.
"""
from __future__ import annotations

from contextlib import contextmanager
from copy import deepcopy
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import time
from uuid import uuid4

try:
    from tools import idea_forge as forge
except ModuleNotFoundError:
    import idea_forge as forge

ENGINE_REPOSITORY = "kevstermcgee/BlueEngine"
GAMES_REPOSITORY = "kevstermcgee/BlueEngineGames"


def companion_owned(catalog, directory):
    """Unowned native source is safe: the exporter never removes unknown files.

    Read the immutable file ownership record, rather than treating an absent
    preserve entry as permission for the engine to overwrite a creator's game.
    A contradictory preserve entry cannot override recorded Engine ownership.
    """
    if (not isinstance(catalog, dict) or catalog.get("source_repository") != ENGINE_REPOSITORY
            or catalog.get("target_repository") != GAMES_REPOSITORY
            or not isinstance(catalog.get("source_revision"), str)
            or not re.fullmatch(r"[0-9a-f]{40}(?:[0-9a-f]{24})?", catalog["source_revision"])
            or not isinstance(catalog.get("files"), list)):
        raise forge.ForgeError("Companion source ownership needs the trusted Engine export catalog with its complete files record")
    owned = False
    for item in catalog["files"]:
        path = item.get("path") if isinstance(item, dict) else None
        digest = item.get("sha256") if isinstance(item, dict) else None
        if (not isinstance(path, str) or "\\" in path or PurePosixPath(path).is_absolute()
                or ".." in PurePosixPath(path).parts or len(PurePosixPath(path).parts) < 2
                or PurePosixPath(path).as_posix() != path
                or PurePosixPath(path).parts[0] not in {"games", "prototypes", "tests", "demos"}
                or not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest)):
            raise forge.ForgeError("The Engine export catalog has an invalid file ownership record")
        if path.casefold() == directory.casefold() or path.casefold().startswith(directory.casefold() + "/"):
            owned = True
    return not owned


class CompanionRelease(forge.Forge):
    def __init__(self, directory, state):
        super().__init__(directory, state)
        self.games = self.directory / "BlueEngineGames"
        self.engine = self.directory / "engines" / state["engine_head"]
        self.env["BE2_TOOLING_ROOT"] = str(self.engine)

    @property
    def game(self):
        return self.games / self.state["definition"]["directory"]

    def assert_clean(self):
        if forge.git(self.games, "rev-parse", "HEAD") != self.state["games_head"]:
            raise forge.ForgeError("Companion source changed after its recorded checks; retain it and resume source integration")
        if forge.git(self.games, "diff", "--name-only", "HEAD"):
            raise forge.ForgeError("Companion source has tracked edits; preserved without publication")
        allowed = (self.state["definition"]["directory"] + "/dist/",
                   self.state["definition"]["directory"] + "/.blue-check/",
                   self.state["definition"]["directory"] + "/target/", "dist/")
        untracked = forge.git(self.games, "ls-files", "--others", "--exclude-standard").splitlines()
        if any(not path.startswith(allowed) for path in untracked):
            raise forge.ForgeError("Companion source has unrelated untracked files; preserved without publication")

    def source_definition(self):
        manifest = forge.load_json(self.games / ".release-games.json")
        matches = [entry for entry in manifest["native_playables"] if entry.get("slug") == self.state["idea"]["slug"]]
        if len(matches) != 1:
            raise forge.ForgeError("Add exactly one native_playables definition for this fork to .release-games.json, then commit a new source revision")
        entry = matches[0]
        if entry.get("kind") not in {"cargo", "cargo-package"}:
            raise forge.ForgeError("This source needs migration: engine-sandbox uses the engine release lane; release-asset requires recoverable Cargo source and a new cargo/cargo-package definition")
        relative = PurePosixPath(entry.get("directory", ""))
        if (relative.is_absolute() or len(relative.parts) != 2 or relative.parts[0] != "games"
                or relative.parts[1] != self.state["idea"]["slug"] or "\\" in str(relative)):
            raise forge.ForgeError("The native definition must use games/SLUG as its source directory")
        if not isinstance(entry.get("binary"), str) or not re.fullmatch(r"[A-Za-z0-9_-]+", entry["binary"]):
            raise forge.ForgeError("The native definition needs a valid Cargo playable binary")
        return entry

    def pin_engine(self):
        catalog = forge.load_json(self.games / ".games-catalog.json")
        revision = catalog.get("source_revision", "")
        if (catalog.get("source_repository") != ENGINE_REPOSITORY
                or catalog.get("target_repository") != GAMES_REPOSITORY
                or not re.fullmatch(r"[0-9a-f]{40}(?:[0-9a-f]{24})?", revision)):
            raise forge.ForgeError("The companion catalog needs the trusted immutable BlueEngine source pin")
        self.state["engine_head"] = revision
        self.engine = self.directory / "engines" / revision
        self.env["BE2_TOOLING_ROOT"] = str(self.engine)
        if not self.engine.exists():
            self.engine.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
            self.command(["git", "fetch", "origin", revision], cwd=Path(self.state["engine_root"]), label="pinned-engine-fetch")
            self.command(["git", "worktree", "add", "--detach", str(self.engine), revision],
                         cwd=Path(self.state["engine_root"]), label="pinned-engine-worktree")
        if (self.engine.is_symlink() or forge.git(self.engine, "rev-parse", "HEAD") != revision
                or forge.git(self.engine, "status", "--porcelain")):
            raise forge.ForgeError("The private pinned engine worktree changed; preserve it for review")
        self.save()

    def assert_companion_ownership(self):
        """Check both authored and public-main ownership before source delivery."""
        selected = self.state["definition"]["directory"]
        catalogs = (forge.load_json(self.games / ".games-catalog.json"),
                    json.loads(forge.git(self.games, "show", "origin/main:.games-catalog.json")))
        if not all(companion_owned(catalog, selected) for catalog in catalogs):
            raise forge.ForgeError("This game has Engine-managed files; use the Engine release lane or choose a new companion fork slug")
        self.state["ownership"] = {"directory": selected, "public_main": forge.git(self.games, "rev-parse", "origin/main"),
                                   "source_head": self.state["games_head"], "kind": "companion"}
        self.save()

    def prepare(self):
        if not self.games.exists():
            self.command(["git", "worktree", "add", "-b", self.state["branch"], str(self.games), self.state["base"]],
                         cwd=Path(self.state["source_root"]), label="companion-worktree")
        self.assert_clean()
        if self.source_definition() != self.state["definition"]:
            raise forge.ForgeError("The source's selected native definition differs from its frozen receipt")
        if self.game.is_symlink() or not self.game.is_dir() or any(path.is_symlink() for path in self.game.rglob("*")):
            raise forge.ForgeError("Companion game source must be a real directory without symlinks")
        for name in ("Cargo.toml", "Cargo.lock"):
            if not (self.game / name).is_file():
                raise forge.ForgeError("Commit the native game's Cargo.toml and Cargo.lock before publication")
        if self.state["definition"]["kind"] == "cargo-package" and not (self.game / "scripts/ship.py").is_file():
            raise forge.ForgeError("A cargo-package definition needs its committed native scripts/ship.py")
        self.integrate_main()
        self.assert_companion_ownership()
        self.pin_engine()
        screenshot = self.games / "site/thumbs" / (self.state["idea"]["slug"] + ".png")
        if screenshot.is_symlink() or not screenshot.is_file():
            raise forge.ForgeError("Commit the real native gameplay thumbnail and complete game guide before release")
        self.state["screenshot"] = str(screenshot)
        self.save()

    @contextmanager
    def bound_engine(self):
        """Use Windows' trusted SDK binder and prove the resolved immutable SDK."""
        manifest, lock = self.game / "Cargo.toml", self.game / "Cargo.lock"
        if any(path.is_symlink() or not path.is_file() for path in (manifest, lock)):
            raise forge.ForgeError("Native SDK binding needs committed real Cargo.toml and Cargo.lock files")
        originals = {path: path.read_bytes() for path in (manifest, lock)}
        evidence = self.directory / "sdk-evidence" / uuid4().hex
        evidence.mkdir(mode=0o700, parents=True)
        for path, data in originals.items():
            (evidence / ("original-" + path.name)).write_bytes(data)
        try:
            binder = self.sdk_binder()
            # This includes binder invocation: a partial manifest write followed
            # by a lock error must restore both original byte snapshots as well.
            try:
                binder.bind(manifest, self.engine)
            except ValueError as error:
                raise forge.ForgeError("Native SDK binding failed; use one supported inline vesper3d dependency on package be2 with a compatible committed Cargo.lock: " + str(error)) from None
            for path in originals:
                if path.is_symlink() or not path.is_file():
                    raise forge.ForgeError("The SDK binder replaced a source input with an unsafe path")
                (evidence / ("bound-" + path.name)).write_bytes(path.read_bytes())
            bound_inputs = {path: path.read_bytes() for path in originals}
            log = self.command(["cargo", "metadata", "--quiet", "--locked", "--format-version", "1",
                                "--manifest-path", str(manifest)], cwd=self.game, label="native-sdk-provenance")
            documents = []
            for line in log.read_text(encoding="utf-8").splitlines():
                try:
                    candidate = json.loads(line)
                except ValueError:
                    continue
                if isinstance(candidate, dict) and isinstance(candidate.get("packages"), list):
                    documents.append(candidate)
            if len(documents) != 1:
                raise forge.ForgeError("Cargo did not produce one complete locked SDK metadata proof")
            if any(path.is_symlink() or path.read_bytes() != data for path, data in bound_inputs.items()):
                raise forge.ForgeError("Locked SDK metadata changed its bound Cargo inputs; preserve the evidence and update the committed lock")
            packages = [item for item in documents[0]["packages"] if item.get("name") == "be2"]
            expected = (self.engine / "Cargo.toml").resolve()
            if (len(packages) != 1 or packages[0].get("source") is not None
                    or not isinstance(packages[0].get("manifest_path"), str)
                    or Path(packages[0]["manifest_path"]).resolve() != expected):
                raise forge.ForgeError("Resolved SDK does not match the frozen catalog Engine; integrate current Games main and update the game's dependency/lock before native publication")
            proof = {"inputs": self.sdk_inputs(originals), "tooling": deepcopy(self.state.get("sdk_tooling")),
                     "evidence": str(evidence), "metadata_log": log.name,
                     "bound_manifest_sha256": hashlib.sha256(manifest.read_bytes()).hexdigest(),
                     "bound_lock_sha256": hashlib.sha256(lock.read_bytes()).hexdigest()}
            forge.atomic_json(evidence / "proof.json", proof)
            self.state["sdk_provenance"] = proof
            self.save()
            yield
        finally:
            unsafe = []
            for path, original in originals.items():
                if path.is_symlink() or path.exists() and not path.is_file():
                    unsafe.append(path.name)
                    continue
                current = path.read_bytes() if path.exists() else None
                if current != original:
                    # Keep generated lock evidence without letting a build alter
                    # the immutable source snapshot that authorizes publication.
                    if path == lock and current is not None:
                        saved = self.directory / ("build-lock-" + uuid4().hex[:8] + ".toml")
                        saved.write_bytes(current)
                    path.write_bytes(original)
            if unsafe:
                raise forge.ForgeError("Native checks replaced source inputs with unsafe paths; retained without following them: " + ", ".join(unsafe))

    def sdk_inputs(self, originals=None):
        originals = originals or {path: path.read_bytes() for path in (self.game / "Cargo.toml", self.game / "Cargo.lock")}
        try:
            controller = forge.git(self.games, "rev-parse", "HEAD:distribution/bind_engine.py")
        except forge.ForgeError:
            raise forge.ForgeError("Integrate latest Games main before native review; the frozen source needs its trusted SDK binding controller") from None
        return {"engine_head": self.state["engine_head"],
                "game_tree": forge.git(self.games, "rev-parse", "HEAD:" + self.state["definition"]["directory"]),
                "controller_blob": controller,
                "manifest_sha256": hashlib.sha256(originals[self.game / "Cargo.toml"]).hexdigest(),
                "lock_sha256": hashlib.sha256(originals[self.game / "Cargo.lock"]).hexdigest()}

    def sdk_binder(self):
        """Execute only a frozen public-main controller matching reviewed source."""
        relative = "distribution/bind_engine.py"
        commit = self.state.get("sdk_tooling_base") or forge.git(self.games, "rev-parse", "origin/main")
        try:
            trusted = forge.git(self.games, "ls-tree", commit, "--", relative).split()
            source = forge.git(self.games, "ls-tree", "HEAD", "--", relative).split()
            if (len(trusted) != 4 or trusted[0] not in {"100644", "100755"}
                    or len(source) != 4 or source[:3] != trusted[:3]):
                raise forge.ForgeError("mismatched controller")
            process = subprocess.run(["git", "-C", str(self.games), "show", commit + ":" + relative], capture_output=True, timeout=60)
            if process.returncode:
                raise forge.ForgeError("missing controller")
            code = process.stdout
        except forge.ForgeError:
            raise forge.ForgeError("Integrate latest trusted Games main before native review: frozen source must include the same distribution/bind_engine.py controller as its trusted baseline") from None
        tooling = {"repository": GAMES_REPOSITORY, "commit": commit, "blob": trusted[2],
                   "sha256": hashlib.sha256(code).hexdigest()}
        path = self.directory / "sdk-tooling" / trusted[2] / "bind_engine.py"
        path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        if path.is_symlink() or path.exists() and path.read_bytes() != code:
            raise forge.ForgeError("The private trusted SDK controller changed; preserve it for review")
        if not path.exists():
            path.write_bytes(code)
        self.state["sdk_tooling"] = tooling
        self.state["sdk_tooling_base"] = commit
        self.save()
        spec = importlib.util.spec_from_file_location("blueengine_sdk_" + trusted[2], path)
        module = importlib.util.module_from_spec(spec)
        # Compile the verified Git bytes directly; an old private __pycache__
        # cannot substitute a different controller behind an unchanged source.
        exec(compile(code, str(path), "exec"), module.__dict__)
        return module

    def assert_sdk_provenance(self):
        if self.state.get("sdk_provenance", {}).get("inputs") != self.sdk_inputs():
            with self.bound_engine():
                pass

    def verify_local(self):
        self.assert_clean()
        self.pin_engine()
        for argv, label in (([sys.executable, "distribution/check_catalog.py"], "catalog-check"),
                            ([sys.executable, "-m", "unittest", "discover", "-s", "distribution", "-p", "test_*.py"], "distribution-tests"),
                            ([sys.executable, "-m", "unittest", "discover", "-s", "site", "-p", "test_*.py"], "site-tests")):
            self.command(argv, cwd=self.games, label=label)
        self.assert_clean()
        with self.bound_engine():
            manifest = self.game / "Cargo.toml"
            self.command(["cargo", "fmt", "--manifest-path", str(manifest), "--check"], cwd=self.game, label="game-format")
            log = self.command(["cargo", "test", "--locked", "--no-default-features", "--manifest-path", str(manifest)],
                               cwd=self.game, label="game-headless-tests")
            counts = re.findall(r"test result: ok\. (\d+) passed", log.read_text(errors="replace"))
            if not counts or not sum(map(int, counts)):
                raise forge.ForgeError("The native game needs executed behavioral tests before publication")
            self.command(["cargo", "clippy", "--locked", "--all-targets", "--manifest-path", str(manifest),
                          "--", "-D", "warnings"], cwd=self.game, label="game-clippy")
            self.command(["cargo", "build", "--locked", "--manifest-path", str(manifest),
                          "--bin", self.state["definition"]["binary"]], cwd=self.game, label="native-build")
            if self.state["definition"]["kind"] == "cargo-package":
                ship = str(self.game / "scripts/ship.py")
                self.command([sys.executable, ship, "package"], cwd=self.game, label="native-package")
                shortcuts = self.directory / "private-shortcuts"
                self.command([sys.executable, ship, "shortcut", "--folder", str(shortcuts)],
                             cwd=self.game, label="private-shortcut")
                verify = [sys.executable, ship, "verify", "--folder", str(shortcuts), "--smoke", "--json"]
                if sys.platform.startswith("linux") and not os.environ.get("DISPLAY"):
                    if not shutil.which("xvfb-run"):
                        raise forge.ForgeError("Native package smoke needs DISPLAY or xvfb-run")
                    verify = ["xvfb-run", "-a", *verify]
                    self.env["LIBGL_ALWAYS_SOFTWARE"] = "1"
                self.command(verify, cwd=self.game, label="isolated-native-smoke")
        self.assert_clean()
        if forge.git(self.engine, "status", "--porcelain"):
            raise forge.ForgeError("Native checks changed the pinned engine source; preserved without publication")
        self.state["local_receipt"] = {"games_head": self.state["games_head"], "engine_head": self.state["engine_head"]}
        self.save()

    def wait_ci(self, repository, workflow, branch, head, label, dispatch=False, fields=None, after=None):
        dispatch_key = label + "@" + head
        if dispatch and dispatch_key not in self.state.setdefault("dispatched", []):
            argv = ["gh", "workflow", "run", workflow, "--repo", repository, "--ref", branch]
            for key, value in (fields or {}).items():
                argv.extend(["-f", key + "=" + value])
            self.command(argv, cwd=self.games, label="dispatch-" + label)
            self.state["dispatched"].append(dispatch_key)
            self.save()
        events = {"workflow_dispatch"} if dispatch else ({"push", "workflow_run"} if label == "download-page" else {"push"})
        deadline = time.monotonic() + self.state["wait_timeout"]
        while time.monotonic() < deadline:
            data = self.api(f"repos/{repository}/actions/workflows/{workflow}/runs?branch={branch}&per_page=100")
            matches = [item for item in data["workflow_runs"] if item.get("head_sha") == head
                       and item.get("head_branch") == branch and item.get("event") in events
                       and item.get("path", "").split("@", 1)[0] == ".github/workflows/" + workflow
                       and (not after or item["created_at"] >= after)]
            if matches:
                run = max(matches, key=lambda item: (int(item["id"]), int(item.get("run_attempt", 1))))
                if run["status"] == "completed":
                    if run["conclusion"] != "success":
                        raise forge.ForgeError(f"{label} did not pass: {run['html_url']}")
                    receipt = {"run": run["id"], "head": head, "url": run["html_url"], "updated_at": run["updated_at"],
                               "workflow": workflow, "event": run["event"], "branch": branch}
                    self.state.setdefault("ci", {})[label] = receipt
                    self.save()
                    return receipt
            time.sleep(min(10, max(0, deadline - time.monotonic())))
        raise forge.ForgeError(f"Timed out waiting for exact-source {label}; resume the retained release")

    def native_checks(self):
        self.assert_clean()
        expected = {"games_head": self.state["games_head"], "engine_head": self.state["engine_head"]}
        if self.state.get("local_receipt") != expected:
            raise forge.ForgeError("Native source changed after local checks; repeat local gates before hosted checks")
        self.assert_sdk_provenance()
        self.command(["git", "push", "-u", "origin", "HEAD"], cwd=self.games, label="push-native-review")
        self.wait_ci(self.state["games_repository"], "native-game-checks.yml", self.state["branch"], self.state["games_head"],
                     "companion-native-checks", dispatch=True, fields={"game_slug": self.state["idea"]["slug"]})

    def installer_review(self):
        self.assert_clean()
        expected = {"games_head": self.state["games_head"], "engine_head": self.state["engine_head"]}
        if self.state.get("local_receipt") != expected:
            raise forge.ForgeError("Native source changed after local checks; repeat the exact-source local gates")
        if self.state.get("ci", {}).get("companion-native-checks", {}).get("head") != self.state["games_head"]:
            raise forge.ForgeError("Linux/Windows native checks must pass for this exact source before installer review")
        self.assert_sdk_provenance()
        self.command(["git", "push", "-u", "origin", "HEAD"], cwd=self.games, label="push-installer-review")
        self.wait_ci(self.state["games_repository"], "releases.yml", self.state["branch"], self.state["games_head"],
                     "windows-installer-review", dispatch=True, fields={"game_slug": self.state["idea"]["slug"]})
        self.save()

    def assert_review(self):
        self.assert_clean()
        for label, workflow in (("companion-native-checks", "native-game-checks.yml"),
                                ("windows-installer-review", "releases.yml")):
            receipt = self.state.get("ci", {}).get(label, {})
            if receipt.get("head") != self.state["games_head"]:
                raise forge.ForgeError(f"The {label} review does not certify this companion source")
            run = self.api(f"repos/{self.state['games_repository']}/actions/runs/{receipt['run']}")
            if (run.get("head_sha") != self.state["games_head"] or run.get("head_branch") != self.state["branch"]
                    or run.get("event") != "workflow_dispatch" or run.get("path", "").split("@", 1)[0] != ".github/workflows/" + workflow
                    or run.get("status") != "completed" or run.get("conclusion") != "success"):
                raise forge.ForgeError(f"The exact-source {label} gate is no longer successful")
        self.assert_sdk_provenance()

    def integrate_main(self):
        self.command(["git", "fetch", "origin", "main"], cwd=self.games, label="companion-main-fetch")
        merge = Path(forge.git(self.games, "rev-parse", "--git-path", "MERGE_HEAD"))
        if not merge.is_absolute():
            merge = self.games / merge
        if merge.exists() or forge.git(self.games, "diff", "--name-only", "HEAD"):
            raise forge.ForgeError(f"Retained companion integration needs conflict resolution and a committed merge in {self.games}; preserve the requested game's tree")
        head = forge.git(self.games, "rev-parse", "HEAD")
        if head != self.state["games_head"] and not self.state.get("integration_pending"):
            raise forge.ForgeError("Companion HEAD changed outside the retained integration; prior checks cannot authorize it")
        if not forge.git_ancestor(self.games, self.state["base"], head):
            raise forge.ForgeError("Companion integration lost its frozen source commit")
        if head != forge.git(self.games, "rev-parse", "origin/main") and forge.git_ancestor(self.games, head, "origin/main"):
            self.state["integration_pending"] = head
            self.save()
            self.command(["git", "merge", "--ff-only", "origin/main"], cwd=self.games, label="companion-fast-forward-main")
            head = forge.git(self.games, "rev-parse", "HEAD")
        elif not forge.git_ancestor(self.games, "origin/main", head):
            self.state["integration_pending"] = head
            self.save()
            self.command(["git", "-c", "user.name=BlueEngine Community", "-c", "user.email=community@users.noreply.github.com",
                          "merge", "--no-edit", "origin/main"], cwd=self.games, label="companion-merge-main")
            head = forge.git(self.games, "rev-parse", "HEAD")
        game = self.state["definition"]["directory"]
        if forge.git(self.games, "rev-parse", self.state["base"] + ":" + game) != forge.git(self.games, "rev-parse", head + ":" + game):
            raise forge.ForgeError("Concurrent main changed this game's source; keep the requested exact tree or submit a new reviewed revision")
        if self.source_definition() != self.state["definition"]:
            raise forge.ForgeError("Concurrent main changed this game's release definition; submit a new reviewed revision")
        if head != self.state["games_head"]:
            self.state.setdefault("verification_history", []).append(deepcopy({key: self.state.get(key) for key in ("games_head", "engine_head", "local_receipt", "ci")}))
            self.state["games_head"] = head
            self.state.pop("local_receipt", None)
            self.state.setdefault("ci", {}).pop("windows-installer-review", None)
            self.state["ci"].pop("companion-native-checks", None)
        self.state.pop("integration_pending", None)
        self.state["sdk_tooling_base"] = forge.git(self.games, "rev-parse", "origin/main")
        self.pin_engine()
        self.save()

    def publish_games(self):
        self.integrate_main()
        self.assert_companion_ownership()
        if self.state.get("local_receipt") != {"games_head": self.state["games_head"], "engine_head": self.state["engine_head"]}:
            self.verify_local()
        if self.state.get("ci", {}).get("companion-native-checks", {}).get("head") != self.state["games_head"]:
            self.native_checks()
        if self.state.get("ci", {}).get("windows-installer-review", {}).get("head") != self.state["games_head"]:
            self.installer_review()
        self.assert_review()
        self.command(["git", "fetch", "origin", "main"], cwd=self.games, label="companion-publication-fetch")
        self.assert_companion_ownership()
        head = self.state["games_head"]
        if not forge.git_ancestor(self.games, head, "origin/main"):
            if not forge.git_ancestor(self.games, "origin/main", head):
                raise forge.ForgeError("Companion main advanced during review; resume to integrate and repeat the exact-source gates")
            self.command(["git", "push", "origin", head + ":main"], cwd=self.games, label="publish-companion")

    def advance(self):
        phases = (("prepare", self.prepare), ("local", self.verify_local), ("native", self.native_checks), ("installer", self.installer_review),
                  ("production", self.publish_games), ("public_receipt", self.verify_publication))
        for name, operation in phases:
            if name in self.state["completed"]:
                continue
            self.state["status"] = name
            self.save()
            operation()
            self.state["completed"].append(name)
            self.save()
        self.state["status"] = "published"
        self.save()
        return {"status": "published", "game": self.state["idea"]["slug"], "publication": self.state["publication"], "run": str(self.directory)}


def create(engine_root, games_root, slug, *, runs_root=None, wait_timeout=7200):
    engine_root, games_root = Path(engine_root).resolve(), Path(games_root).resolve()
    if not re.fullmatch(forge.SLUG, slug):
        raise forge.ForgeError("Invalid game slug")
    if forge.github_repository(engine_root) != ENGINE_REPOSITORY or forge.github_repository(games_root) != GAMES_REPOSITORY:
        raise forge.ForgeError("Companion releases need the trusted BlueEngine and BlueEngineGames upstream origins; submit other forks for maintainer review first")
    if forge.git(games_root, "status", "--porcelain", "--untracked-files=all"):
        raise forge.ForgeError("Commit companion source changes before release; existing work is preserved")
    head = forge.git(games_root, "rev-parse", "HEAD")
    catalog = json.loads(forge.git(games_root, "show", head + ":.games-catalog.json"))
    manifest = json.loads(forge.git(games_root, "show", head + ":.release-games.json"))
    matches = [entry for entry in manifest["native_playables"] if entry.get("slug") == slug]
    if len(matches) != 1 or matches[0].get("kind") not in {"cargo", "cargo-package"}:
        raise forge.ForgeError("Commit a cargo/cargo-package native definition with recoverable source; use the engine lane for engine-sandbox and migrate release-asset games to authored source")
    if not re.fullmatch(r"[0-9a-f]{40}(?:[0-9a-f]{24})?", catalog.get("source_revision", "")):
        raise forge.ForgeError("The companion catalog needs an immutable engine revision")
    directory = Path(runs_root or games_root / ".be2-work/community/releases").resolve() / (
        datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S") + "-" + uuid4().hex[:8])
    directory.mkdir(mode=0o700, parents=True)
    target = Path(os.environ.get("CARGO_TARGET_DIR", directory / "cargo-target")).resolve()
    target.mkdir(parents=True, exist_ok=True)
    state = {"version": 1, "workflow": "community-release", "source_kind": "companion", "id": directory.name[-8:],
             "created": datetime.now(timezone.utc).isoformat(), "source_root": str(games_root), "games_root": str(games_root),
             "engine_root": str(engine_root), "engine_repository": ENGINE_REPOSITORY, "games_repository": GAMES_REPOSITORY,
             "base": head, "games_head": head, "engine_head": catalog["source_revision"], "definition": matches[0],
             "branch": "community-companion-release/" + directory.name, "target_dir": str(target), "timeout": 7200,
             "wait_timeout": wait_timeout, "idea": {"slug": slug, "title": matches[0]["name"]}, "completed": [], "status": "created"}
    release = CompanionRelease(directory, state)
    release.save()
    return release


def run(engine_root, games_root, slug, **options):
    return resume(create(engine_root, games_root, slug, **options).directory)


def resume(directory):
    directory = Path(directory).resolve()
    with forge.run_lock(directory):
        state = forge.load_json(directory / "state.json")
        if state.get("workflow") != "community-release" or state.get("source_kind") != "companion":
            raise forge.ForgeError("Not a companion-source release journal")
        release = CompanionRelease(directory, state)
        try:
            return release.advance()
        except (forge.ForgeError, OSError, ValueError, KeyError) as error:
            release.state["last_error"] = str(error)
            release.save()
            raise forge.ForgeError(f"Companion release retained at {directory}: {error}") from None
