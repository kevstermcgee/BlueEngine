#!/usr/bin/env python3
"""Git-backed collaborative game lineage. Submitted source is inspected, never run.

Every receipt is an append-only event confirmed by the publication remote. GitHub
identifies the caller; a repository's push/maintain/admin collaborators have equal
access. Other GitHub users can create and maintain their own forks.
"""
from __future__ import annotations

import argparse
import base64
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlencode
from urllib.request import HTTPRedirectHandler, Request, build_opener

try:
    from tools import feedback_server, player_feedback as feedback
except ModuleNotFoundError:
    import feedback_server
    import player_feedback as feedback

DIRECTORY = Path("docs/community/events")
REPOSITORY = "kevstermcgee/BlueEngine"
GAMES_REPOSITORY = "kevstermcgee/BlueEngineGames"
SHA = re.compile(r"[0-9a-f]{40}(?:[0-9a-f]{24})?")
REVISION = re.compile(r"[0-9a-f]{64}")
REPO = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,99}/[A-Za-z0-9][A-Za-z0-9_.-]{0,99}")
LOGIN = re.compile(r"[A-Za-z0-9][A-Za-z0-9-]{0,38}")
MAX_EVENTS = 10000
MAX_UPSTREAM_REVISIONS = 200
COMPANION_REF = "refs/blueengine-community/companion-main"
CATALOG_CACHE_SECONDS = 5
RELEASE_GATES = {"engine-checks": ("engine", "ci.yml"),
                 "generated-game-linux-windows": ("engine", "forge-game-checks.yml"),
                 "windows-installer-review": ("games", "releases.yml"),
                 "production-release": ("games", "releases.yml"),
                 "download-page": ("games", "pages.yml"),
                 "companion-native-checks": ("games", "native-game-checks.yml")}
ENGINE_GATES = frozenset(RELEASE_GATES) - {"companion-native-checks"}
COMPANION_GATES = frozenset(("companion-native-checks", "windows-installer-review", "production-release", "download-page"))


class CommunityError(feedback.FeedbackError):
    pass


class AccessDenied(CommunityError):
    pass


class Conflict(CommunityError):
    pass


class NotFound(CommunityError):
    pass


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, request, *args, **kwargs):
        # A renamed repository can redirect. Require its new canonical name
        # instead of forwarding a request-scoped token to a different location.
        raise CommunityError("Use the canonical GitHub repository name; redirects are disabled")


def slug(value):
    if not isinstance(value, str) or len(value) > 80 or not feedback.SLUG.fullmatch(value):
        raise CommunityError("Use a lowercase game slug with letters, numbers and hyphens")
    return value


def sha(value):
    if not isinstance(value, str) or not SHA.fullmatch(value):
        raise CommunityError("Source commit must be a full immutable Git SHA")
    return value


def revision_id(value):
    if not isinstance(value, str) or not REVISION.fullmatch(value):
        raise CommunityError("Invalid immutable revision ID")
    return value


def repository(value):
    if not isinstance(value, str) or not REPO.fullmatch(value) or any(part.endswith(".git") for part in value.split("/")):
        raise CommunityError("Source repository must be a GitHub owner/repository")
    return value


def canonical_request(action, payload):
    """Normalize and bound public data before performing any Git/network writes."""
    if not isinstance(payload, dict):
        raise CommunityError("Expected a JSON object")
    fields = {
        "create": ({"request_id", "slug", "title", "source_commit", "version", "note"},
                   {"source_repository", "source_game"}),
        "fork": ({"request_id", "slug", "title"}, {"source_revision"}),
        "revision": ({"request_id", "expected_revision", "source_commit", "version", "note"},
                     {"source_repository", "source_game"}),
        "publish": ({"request_id", "revision_id"}, set()),
        "release": ({"request_id", "revision_id", "source_commit", "games_commit", "receipt", "gates"}, set()),
        "feedback": ({"request_id", "target_type", "kind", "category", "note"}, {"target", "version"}),
    }
    if action not in fields:
        raise CommunityError("Unknown community action")
    required, optional = fields[action]
    if not required <= payload.keys() or payload.keys() - required - optional:
        raise CommunityError("Missing or unexpected community fields")
    result = {"request_id": feedback.submission_id(payload["request_id"])}
    for key in ("slug", "source_game"):
        if key in payload:
            result[key] = slug(payload[key])
    for key in ("source_revision", "expected_revision", "revision_id"):
        if key in payload:
            result[key] = revision_id(payload[key])
    if "source_commit" in payload:
        result["source_commit"] = sha(payload["source_commit"])
    if action == "release":
        result["games_commit"] = sha(payload["games_commit"])
        receipt = payload["receipt"]
        if (not isinstance(receipt, dict)
                or set(receipt) != {"page", "release", "installer_sha256", "zip_sha256", "verified_at"}):
            raise CommunityError("Supply the complete verified native publication receipt")
        result["receipt"] = {key: feedback.text(receipt[key], 600) for key in ("page", "release")}
        for key in ("installer_sha256", "zip_sha256"):
            result["receipt"][key] = revision_id(receipt[key])
        result["receipt"]["verified_at"] = feedback.timestamp(receipt["verified_at"])
        gates = payload["gates"]
        if (not isinstance(gates, dict) or set(gates) not in (ENGINE_GATES, COMPANION_GATES)
                or any(type(run) is not int or run < 1 for run in gates.values())):
            raise CommunityError("Supply the complete successful native release workflow run IDs")
        result["gates"] = dict(gates)
    if "source_repository" in payload:
        result["source_repository"] = repository(payload["source_repository"])
    for key, maximum in (("title", 160), ("note", 8000), ("version", 160), ("category", 80)):
        if key in payload:
            result[key] = feedback.text(payload[key], maximum, optional=key == "version")
    if action == "feedback":
        if payload["target_type"] not in ("engine", "launcher", "game") or payload["kind"] not in ("human", "ai"):
            raise CommunityError("Choose engine, launcher or game, and human or ai provenance")
        result.update(target_type=payload["target_type"], kind=payload["kind"])
        if payload["target_type"] == "game":
            result["target"] = slug(payload.get("target"))
        elif payload.get("target", ""):
            raise CommunityError("Only game feedback takes a target slug")
        else:
            result["target"] = ""
        result.setdefault("version", "")
    return result


def source_revision(source_repository, commit, game, *, version="", note="", actor=None,
                    created_at=None, kind="upstream", branch=None, game_path=None):
    source_repository, commit, game = repository(source_repository), sha(commit), slug(game)
    game_path = game_path if game_path is not None else f"games/{game}"
    if game_path not in (".", f"games/{game}", f"assets/games/{game}"):
        raise CommunityError("Invalid trusted source game path")
    version, note = feedback.text(version, 160, optional=True), feedback.text(note, 8000, optional=True)
    if kind not in ("upstream", "fork", "submitted"):
        raise CommunityError("Invalid source provenance kind")
    if created_at is not None:
        feedback.timestamp(created_at)
    if actor is not None and (not isinstance(actor, dict) or set(actor) != {"login", "role"}
                              or not isinstance(actor["login"], str) or not LOGIN.fullmatch(actor["login"])
                              or actor["role"] not in ("maintainer", "contributor")):
        raise CommunityError("Invalid source author provenance")
    if branch is not None and (not isinstance(branch, str) or not re.fullmatch(
            r"blueengine-community/[a-z0-9]+(?:-[a-z0-9]+)*/[0-9a-f]{64}", branch)):
        raise CommunityError("Invalid immutable source branch")
    identity = game if game_path == f"games/{game}" else game_path
    identifier = hashlib.sha256(f"{source_repository.lower()}\0{commit}\0{identity}".encode()).hexdigest()
    source_url = f"https://github.com/{source_repository}/tree/{commit}" + ("" if game_path == "." else "/" + game_path)
    return {"id": identifier, "source_repository": source_repository, "source_commit": commit,
            "source_game": game, "source": {"repository": source_repository, "commit": commit,
                                             "game_path": game_path},
            "source_url": source_url,
            "download_source_url": f"https://github.com/{source_repository}/archive/{commit}.tar.gz",
            "source_branch": branch, "version": version, "note": note, "actor": actor,
            "created_at": created_at, "kind": kind}


class GitHub:
    """A request-scoped token: never persisted, logged or sent to another host."""

    def __init__(self, token, *, timeout=12):
        if not isinstance(token, str) or not 10 <= len(token) <= 500 or not re.fullmatch(r"[A-Za-z0-9_]+", token):
            raise AccessDenied("Supply a GitHub token in Authorization: Bearer")
        self.token, self.timeout = token, timeout
        self.deadline = time.monotonic() + 60

    def api(self, path, *, method="GET", data=None, missing=False):
        if not path.startswith("/") or "\r" in path or "\n" in path:
            raise CommunityError("Invalid GitHub API path")
        request = Request("https://api.github.com" + path,
                          data=json.dumps(data).encode() if data is not None else None,
                          headers={"Authorization": f"Bearer {self.token}", "Accept": "application/vnd.github+json",
                                   "X-GitHub-Api-Version": "2022-11-28", "User-Agent": "BlueEngine-Community",
                                   "Content-Type": "application/json"}, method=method)
        try:
            remaining = self.deadline - time.monotonic()
            if remaining <= 0:
                raise feedback_server.PublicationError("GitHub verification exceeded its request deadline")
            with build_opener(NoRedirect()).open(request, timeout=min(self.timeout, remaining)) as response:
                body = response.read(1_000_001)
            if len(body) > 1_000_000:
                raise ValueError
            return json.loads(body) if body else {}
        except HTTPError as error:
            if error.code == 404 and missing:
                return None
            if error.code in (401, 403, 404):
                raise AccessDenied("GitHub did not authorize this account or repository action") from None
            if error.code == 422:
                raise Conflict("GitHub rejected this source branch or pull request; check the source commit and permissions") from None
            raise feedback_server.PublicationError("GitHub is temporarily unavailable") from None
        except (URLError, OSError, ValueError):
            raise feedback_server.PublicationError("GitHub is temporarily unavailable") from None

    def actor(self, engine_repository):
        user = self.api("/user")
        login = user.get("login")
        if not isinstance(login, str) or not LOGIN.fullmatch(login):
            raise AccessDenied("GitHub did not return an individual account")
        info = self.api(f"/repos/{engine_repository}")
        permissions = info.get("permissions", {})
        role = "maintainer" if any(permissions.get(key) is True for key in ("push", "maintain", "admin")) else "contributor"
        return {"login": login, "role": role}

    def source_upstream(self, engine_repository, source_repository, *, require_write=True):
        info = self.api(f"/repos/{source_repository}")
        network = {info.get("full_name", "").lower(),
                   info.get("source", {}).get("full_name", "").lower(),
                   info.get("parent", {}).get("full_name", "").lower()}
        upstream = next((candidate for candidate in getattr(self, "source_repositories", (engine_repository,))
                         if candidate.lower() in network), None)
        if upstream is None or (require_write and not info.get("permissions", {}).get("push")):
            raise AccessDenied("Submit source from an engine or companion repository fork where your account has write access")
        if info.get("private"):
            raise CommunityError("Collaborative game source must be public so other players can retrieve and fork it")
        return upstream

    def verify_source(self, engine_repository, source_repository, commit, game, *, require_write=True):
        self.source_upstream(engine_repository, source_repository, require_write=require_write)
        document = self.api(f"/repos/{source_repository}/commits/{commit}")
        if document.get("sha") != commit:
            raise CommunityError("GitHub did not confirm the exact source commit")
        content = self.api(f"/repos/{source_repository}/contents/games/{game}/game.project.json?ref={commit}", missing=True)
        if content is None:
            entries = self.api(f"/repos/{source_repository}/contents/games/{game}?ref={commit}")
            if isinstance(entries, list) and {item.get("name") for item in entries} & {"Cargo.toml", "game.json"}:
                # Legacy native source uses its existing publication gates. This
                # validates a retrievable pin, never certifies a playable build.
                return
            raise CommunityError("Source must contain a native game.project.json, Cargo.toml or stock game.json")
        try:
            if content.get("type") != "file" or content.get("encoding") != "base64" or content.get("size", 100001) > 100000:
                raise ValueError
            project = json.loads(base64.b64decode(content["content"]))
            targets = project["targets"]
            if (project.get("id") != game or not isinstance(targets, list)
                    or "windows" not in targets or "web" in targets or "wasm" in targets):
                raise ValueError
        except (ValueError, TypeError, KeyError):
            raise CommunityError("Source must contain the matching native Windows game.project.json; browser games are retired") from None

    def pin_source(self, source, game_slug):
        """Create a named Git ref once; never move or overwrite an existing pin."""
        branch = f"blueengine-community/{game_slug}/{source['id']}"
        path = f"/repos/{source['source_repository']}/git/ref/heads/{branch}"
        existing = self.api(path, missing=True)
        if existing is None:
            try:
                self.api(f"/repos/{source['source_repository']}/git/refs", method="POST",
                         data={"ref": f"refs/heads/{branch}", "sha": source["source_commit"]})
            except Conflict:
                existing = self.api(path)
                if existing.get("object", {}).get("sha") != source["source_commit"]:
                    raise Conflict("An existing source pin identifies another commit") from None
        elif existing.get("object", {}).get("sha") != source["source_commit"]:
            raise Conflict("An existing source pin identifies another commit")
        return branch

    def pull_request(self, engine_repository, game, revision, request_id):
        if revision["source_game"] != game["slug"]:
            raise CommunityError("Check out your fork into games/<your-slug>, update its identity, and submit that source before publishing")
        branch = self.pin_source(revision, game["slug"])
        target_repository = self.source_upstream(engine_repository, revision["source_repository"], require_write=False)
        source_kind = "engine" if target_repository == engine_repository else "companion"
        release_command = (f"python3 tools/community.py release {game['slug']} --source {source_kind} "
                           "--engine-root /path/to/BlueEngine --games-root /path/to/BlueEngineGames")
        owner = revision["source_repository"].split("/")[0]
        head = f"{owner}:{branch}"
        query = urlencode({"state": "all", "head": head, "base": "main", "per_page": 50})
        existing = self.api(f"/repos/{target_repository}/pulls?{query}")
        if not isinstance(existing, list):
            raise feedback_server.PublicationError("GitHub did not confirm the publication queue")
        if existing:
            pull = existing[0]
            if pull.get("state") == "closed" and not pull.get("merged_at"):
                raise Conflict("This source publication pull request was closed without merging; reopen it or submit a new revision before publishing again")
        else:
            body = (f"Publish **{game['title']}** (`{game['slug']}`).\n\n"
                    f"Source: `{revision['source_repository']}@{revision['source_commit']}`\n"
                    f"Game path: `games/{revision['source_game']}`\n"
                    f"Immutable revision: `{revision['id']}`\n"
                    f"Community request: `{request_id}`\n\n"
                    "This request is awaiting the existing Linux/Windows checks, Windows x64 installer review, "
                    "and production download receipt. A queued source revision is not a released game.\n\n"
                    f"Maintainer command: `{release_command}`\n")
            pull = self.api(f"/repos/{target_repository}/pulls", method="POST",
                            data={"title": f"Publish community game: {game['title']}", "head": head,
                                  "base": "main", "body": body, "maintainer_can_modify": True})
        number = pull.get("number")
        expected = f"https://github.com/{target_repository}/pull/{number}"
        if type(number) is not int or number < 1 or pull.get("html_url") != expected:
            raise feedback_server.PublicationError("GitHub did not confirm the publication pull request")
        return {"state": "awaiting_native_gates", "pull_request_url": expected, "pull_request_number": number,
                "target_repository": target_repository, "source_kind": source_kind,
                "pull_request_state": pull.get("state", "open"),
                "maintainer_command": release_command}

    def game_tree(self, source_repository, commit, game):
        document = self.api(f"/repos/{source_repository}/git/commits/{commit}")
        tree = sha(document.get("tree", {}).get("sha"))
        for name in ("games", game):
            entries = self.api(f"/repos/{source_repository}/git/trees/{tree}")
            if entries.get("truncated"):
                raise CommunityError("GitHub did not provide the complete source tree")
            entry = next((item for item in entries.get("tree", []) if item.get("path") == name), None)
            if entry is None or entry.get("type") != "tree" or entry.get("mode") != "040000":
                raise CommunityError("The released game tree is missing")
            tree = sha(entry.get("sha"))
        return tree

    def require_current_upstream(self, source, upstream):
        if self.game_tree(source["source_repository"], source["source_commit"], source["source_game"]) == self.game_tree(
                upstream["source_repository"], upstream["source_commit"], upstream["source_game"]):
            return
        comparison = self.api(f"/repos/{source['source_repository']}/compare/{upstream['source_commit']}...{source['source_commit']}", missing=True)
        if (not isinstance(comparison, dict) or comparison.get("status") not in ("ahead", "identical")
                or comparison.get("merge_base_commit", {}).get("sha") != upstream["source_commit"]):
            raise Conflict("Upstream game source changed since this submission; merge its latest immutable source and submit a new revision before publishing")

    def verify_release(self, engine_repository, games_repository, game, source, request):
        """Independent server checks of the supervisor's exact-source receipt."""
        if source["source_game"] != game["slug"]:
            raise CommunityError("A release receipt must identify this game's submitted source")
        upstream = self.source_upstream(engine_repository, source["source_repository"], require_write=False)
        companion = upstream == games_repository
        expected_gates = COMPANION_GATES if companion else ENGINE_GATES
        if set(request["gates"]) != expected_gates:
            raise CommunityError("This source repository requires its complete native release gate set")
        reviewed_repository = games_repository if companion else engine_repository
        reviewed_commit = request["games_commit"] if companion else request["source_commit"]
        original_tree = self.game_tree(source["source_repository"], source["source_commit"], source["source_game"])
        if original_tree != self.game_tree(reviewed_repository, reviewed_commit, game["slug"]):
            raise CommunityError("The released game source differs from the registered immutable revision")
        if not companion and original_tree != self.game_tree(games_repository, request["games_commit"], game["slug"]):
            raise CommunityError("The exported companion game differs from the registered engine source")
        content = self.api(f"/repos/{games_repository}/contents/.games-catalog.json?ref={request['games_commit']}")
        try:
            if (content.get("type") != "file" or content.get("encoding") != "base64"
                    or content.get("size", 800001) > 800000):
                raise ValueError
            catalog = json.loads(base64.b64decode(content["content"]))
            if catalog.get("source_revision") != request["source_commit"] or catalog.get("source_repository") != engine_repository:
                raise ValueError
        except (ValueError, TypeError, KeyError):
            raise CommunityError("The companion release does not pin the recorded engine repository and revision") from None
        receipt = request["receipt"]
        owner, name = games_repository.split("/")
        expected_page = f"https://{owner}.github.io/{name}/games/{game['slug']}/"
        prefix = f"https://github.com/{games_repository}/releases/tag/"
        tag = receipt["release"][len(prefix):] if receipt["release"].startswith(prefix) else ""
        if (receipt["page"] != expected_page or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,199}", tag)
                or f"-games-{request['games_commit'][:12]}-" not in tag):
            raise CommunityError("The native receipt must point to the exact production game release and download page")
        release = self.api(f"/repos/{games_repository}/releases/tags/{quote(tag, safe='')}")
        if release.get("draft") is not False or release.get("prerelease") is not False or release.get("html_url") != receipt["release"]:
            raise CommunityError("GitHub did not confirm a public production release")
        assets = {item.get("name"): item for item in release.get("assets", [])}
        for filename, key in ((game["slug"] + "-setup-windows-x64.exe", "installer_sha256"),
                              (game["slug"] + "-windows-x64.zip", "zip_sha256")):
            asset = assets.get(filename, {})
            if asset.get("state") != "uploaded" or asset.get("digest") != "sha256:" + receipt[key]:
                raise CommunityError("GitHub's production asset digest does not confirm the downloaded native receipt")
        verified_gates, review_branches = {}, {}
        for label, (scope, workflow) in RELEASE_GATES.items():
            if label not in expected_gates:
                continue
            gate_repository = engine_repository if scope == "engine" else games_repository
            expected_head = request["source_commit"] if scope == "engine" else request["games_commit"]
            run_id = request["gates"][label]
            run = self.api(f"/repos/{gate_repository}/actions/runs/{run_id}")
            if (run.get("head_sha") != expected_head or run.get("status") != "completed"
                    or run.get("conclusion") != "success"
                    or run.get("path", "").split("@")[0] != f".github/workflows/{workflow}"):
                raise CommunityError(f"The {label} workflow does not certify this exact source")
            allowed_events = ("push", "workflow_run") if label == "download-page" else ("push",)
            if label in ("production-release", "download-page") and (run.get("head_branch") != "main" or run.get("event") not in allowed_events):
                raise CommunityError("Production release and page receipts must come from the main production workflows")
            if label in ("windows-installer-review", "companion-native-checks"):
                if run.get("event") != "workflow_dispatch" or not isinstance(run.get("head_branch"), str) or not run["head_branch"]:
                    raise CommunityError("Native game and Windows installer review must use explicit review workflows")
                review_branches[label] = run["head_branch"]
            if label in ("generated-game-linux-windows", "companion-native-checks"):
                prefix = "forge" if label == "generated-game-linux-windows" else "native"
                expected_artifacts = {f"{prefix}-{game['slug']}-{system}" for system in ("Linux", "Windows")}
                document = self.api(f"/repos/{gate_repository}/actions/runs/{run_id}/artifacts?per_page=100")
                artifacts = {item.get("name") for item in document.get("artifacts", []) if item.get("expired") is False}
                if not expected_artifacts <= artifacts:
                    raise CommunityError("The native workflow does not retain Linux and Windows evidence for this game slug")
            verified_gates[label] = {"run": run_id, "head": expected_head,
                                     "url": f"https://github.com/{gate_repository}/actions/runs/{run_id}"}
        if companion and len(set(review_branches.values())) != 1:
            raise CommunityError("Native rules and Windows installer reviews must certify the same source branch")
        return {"state": "published", "source_kind": "companion" if companion else "engine", "game_tree": original_tree,
                "revision_id": source["id"], "source_commit": request["source_commit"],
                "games_commit": request["games_commit"], "receipt": receipt, "gates": verified_gates}


class Catalog:
    def __init__(self, publisher):
        self.publisher, self.games, self.events, self.upstream = publisher, {}, [], {}
        for game_slug, title in feedback.game_catalog(publisher.root).items():
            self.games[game_slug] = {"slug": game_slug, "title": title, "owner": publisher.repository.split("/")[0],
                                     "root_game": game_slug, "parent_game": None, "parent_revision": None,
                                     "head_revision": None, "revisions": [], "publication_requests": [], "releases": [],
                                     "source_unavailable": True,
                                     "source_unavailable_reason": "No recoverable source is registered for this published game. A maintainer can register recovered native source; reviews remain available."}
            source_repository, source_ref = publisher.repository, "HEAD"
            present = publisher.git("cat-file", "-t", f"HEAD:games/{game_slug}", check=False)
            if present.returncode or present.stdout.strip() != b"tree":
                if not getattr(publisher, "companion_sources", False):
                    continue
                present = publisher.git("cat-file", "-t", f"{COMPANION_REF}:games/{game_slug}", check=False)
                if present.returncode or present.stdout.strip() != b"tree":
                    continue
                source_repository, source_ref = publisher.games_repository, COMPANION_REF
            archive = publisher.git("cat-file", "-e", f"{source_ref}:games/{game_slug}/release-source.json", check=False)
            if archive.returncode == 0:
                names = publisher.git("ls-tree", "--name-only", f"{source_ref}:games/{game_slug}").stdout.decode().splitlines()
                if not set(names) & {"Cargo.toml", "game.project.json", "game.json"}:
                    continue  # A pinned executable archive is not editable source.
            commits = publisher.git("log", f"-{MAX_UPSTREAM_REVISIONS + 1}", "--format=%H%x00%cI", source_ref, "--", f"games/{game_slug}").stdout.decode().splitlines()
            if not commits:
                continue
            history = []
            for line in reversed(commits[:MAX_UPSTREAM_REVISIONS]):
                commit, created_at = line.split("\0", 1)
                source = source_revision(source_repository, commit, game_slug, created_at=feedback.timestamp(created_at))
                if history:
                    source["parent_revision"] = history[-1]["id"]
                history.append(source)
            source = history[-1]
            self.games[game_slug] = {"slug": game_slug, "title": title, "owner": source_repository.split("/")[0],
                                     "root_game": game_slug, "parent_game": None, "parent_revision": None,
                                     "head_revision": source["id"], "revisions": history, "publication_requests": [], "releases": [],
                                     "source_history_has_more": len(commits) > MAX_UPSTREAM_REVISIONS,
                                     "source_history_url": f"https://github.com/{source_repository}/commits/main/games/{game_slug}"}
            self.upstream[game_slug] = {"source": source, "revisions": history,
                                        "history_url": self.games[game_slug]["source_history_url"],
                                        "has_more": len(commits) > MAX_UPSTREAM_REVISIONS}
            self.games[game_slug]["submission_upstream_revision"] = source["id"]
        self.seed_published_source_pins()
        paths = sorted((publisher.root / DIRECTORY).glob("*.json"))
        if len(paths) > MAX_EVENTS:
            raise CommunityError("Community event limit requires operator archival")
        for path in paths:
            if path.is_symlink() or not path.is_file() or path.stat().st_size > 40000:
                raise CommunityError("Invalid community event storage")
            try:
                event = json.loads(path.read_text(encoding="utf-8"))
                if (event.get("schema_version") != 1 or event.get("request_id") != path.stem
                        or feedback.submission_id(event["request_id"]) != path.stem):
                    raise ValueError
                feedback.timestamp(event["created_at"])
                validate_event(event)
            except (ValueError, KeyError, TypeError):
                raise CommunityError("Invalid stored community event") from None
            self.events.append(event)
        # Remote commit order is authoritative, including operations in the same
        # timestamp tick; a client never supplies the receipt timestamp or order.
        self.events.sort(key=lambda event: (event["sequence"], event["request_id"]))
        if [event["sequence"] for event in self.events] != list(range(1, len(self.events) + 1)):
            raise CommunityError("Invalid community event sequence")
        for event in self.events:
            self.apply(event)
        self.reconcile_upstream()

    def seed_published_source_pins(self):
        """Recover historical/standalone roots from the trusted site's source pins.

        The public submission API still accepts games/<slug> only. A standalone
        root is a read-only parent that the creator CLI imports into a new fork.
        """
        if not getattr(self.publisher, "companion_sources", False):
            return
        result = self.publisher.git("show", f"{COMPANION_REF}:site/released-games.json", check=False)
        if result.returncode:
            return
        try:
            if len(result.stdout) > 1_000_000:
                raise ValueError
            document = json.loads(result.stdout)
            if document.get("version") != 1 or not isinstance(document.get("games"), list) or len(document["games"]) > 500:
                raise ValueError
            for entry in document["games"]:
                game_slug = slug(entry.get("slug"))
                if game_slug not in self.games or not self.games[game_slug].get("source_unavailable"):
                    continue
                match = re.fullmatch(r"https://github\.com/(" + REPO.pattern + r")/tree/(" + SHA.pattern + r")(?:/([A-Za-z0-9_./-]+))?", entry.get("source_url", ""))
                if not match:
                    raise ValueError
                source_repository, commit, path = match[1], match[2], match[3] or "."
                if any(part in ("", "..") for part in path.split("/")) or path.startswith("/"):
                    raise ValueError
                independent = source_repository.lower() not in (self.publisher.repository.lower(), self.publisher.games_repository.lower())
                game_path = "." if independent else path
                source = source_revision(source_repository, commit, game_slug, game_path=game_path,
                                         version=feedback.text(entry.get("game_version", ""), 160, optional=True))
                game = self.games[game_slug]
                game.pop("source_unavailable", None)
                game.pop("source_unavailable_reason", None)
                game.update(owner=source_repository.split("/")[0], head_revision=source["id"], revisions=[source],
                            source_kind="independent" if independent else "engine" if source_repository.lower() == self.publisher.repository.lower() else "companion",
                            submission_upstream_revision=source["id"], source_history_has_more=True,
                            source_history_url=f"https://github.com/{source_repository}/commits/{commit}" + ("" if game_path == "." else "/" + game_path))
                self.upstream[game_slug] = {"source": source, "revisions": [source], "history_url": game["source_history_url"], "has_more": True}
        except (ValueError, TypeError, KeyError):
            raise CommunityError("Invalid trusted published game source metadata") from None

    def local_tree(self, source):
        path = source["source"]["game_path"]
        target = f"{source['source_commit']}^{{tree}}" if path == "." else f"{source['source_commit']}:{path}"
        result = self.publisher.git("rev-parse", "--verify", target, check=False)
        value = result.stdout.decode().strip()
        return value if result.returncode == 0 and SHA.fullmatch(value) else None

    def reconcile_upstream(self):
        for game_slug, game in self.games.items():
            upstream = self.upstream.get(game_slug)
            game.update(upstream_revision=None, upstream_source=None, upstream_diverged=False)
            if upstream is None:
                continue
            source = upstream["source"]
            known = {item["id"] for item in game["revisions"]}
            game["revisions"].extend(item for item in upstream["revisions"] if item["id"] not in known)
            game.update(upstream_revision=source["id"], upstream_source=source,
                        source_history_url=upstream["history_url"], source_history_has_more=upstream["has_more"])
            if game["head_revision"] == source["id"] or game.get("submission_upstream_revision") == source["id"]:
                continue
            head = next(item for item in game["revisions"] if item["id"] == game["head_revision"])
            upstream_tree, head_tree = self.local_tree(source), self.local_tree(head)
            if upstream_tree is not None and upstream_tree == head_tree:
                continue
            released = next((item for item in reversed(game["releases"]) if item["revision_id"] == game["head_revision"]), None)
            if released and upstream_tree is not None and (released.get("game_tree") or head_tree) and upstream_tree != (released.get("game_tree") or head_tree):
                # The last creator head was released, so a subsequent trusted
                # source edit starts the next revision. Never replace a draft.
                game["head_revision"] = source["id"]
            else:
                game["upstream_diverged"] = True

    def apply(self, event):
        event_type = event["event_type"]
        if event_type in ("game_created", "game_forked"):
            game = event["game"]
            if game["slug"] in self.games and self.games[game["slug"]].get("community_created"):
                raise CommunityError("Duplicate community game identity")
            self.games[game["slug"]] = {**game, "community_created": True,
                                       "revisions": [event["revision"]], "publication_requests": [], "releases": [],
                                       "submission_upstream_revision": None}
        elif event_type == "revision_submitted":
            game = self.require_game(event["target"])
            game["head_revision"] = event["revision"]["id"]
            game["revisions"].append(event["revision"])
            game["submission_upstream_revision"] = event.get("upstream_revision", event["request"]["expected_revision"])
        elif event_type == "publication_requested":
            self.require_game(event["target"])["publication_requests"].append(
                {"request_id": event["request_id"], "revision_id": event["request"]["revision_id"],
                 "created_at": event["created_at"], "actor": event["actor"], **event["publication"]})
        elif event_type == "release_verified":
            self.require_game(event["target"])["releases"].append(
                {"request_id": event["request_id"], "created_at": event["created_at"], "actor": event["actor"], **event["release"]})
        elif event_type != "feedback":
            raise CommunityError("Unknown stored community event")

    def require_game(self, game_slug):
        if game_slug not in self.games:
            raise NotFound("Unknown game")
        return self.games[game_slug]

    def public_game(self, game_slug):
        game = self.require_game(game_slug)
        return {**game, "forks": [{key: child[key] for key in ("slug", "title", "owner", "head_revision")}
                                  for child in self.games.values() if child["parent_game"] == game_slug]}

    def history(self, game_slug):
        game = self.require_game(game_slug)
        lineage = set()
        while game:
            lineage.add(game["slug"])
            game = self.games.get(game["parent_game"])
        return [event for event in self.events if event.get("target") in lineage
                or event.get("game", {}).get("slug") in lineage]


class CommunityPublisher(feedback_server.Publisher):
    """One serialized clone serves both original reviews and community receipts."""

    def __init__(self, root, *, repository=REPOSITORY, games_repository=GAMES_REPOSITORY, companion_sources=False,
                 github_factory=GitHub, **kwargs):
        self.repository = globals()["repository"](repository)
        self.games_repository = globals()["repository"](games_repository)
        self.companion_sources, self.companion_synced_at = companion_sources, 0
        self._catalog_cache = None
        self.github_factory = github_factory
        self.community_journal = Path(root).resolve() / ".git/blueengine-community-inflight.json"
        super().__init__(root, **kwargs)
        Catalog(self)

    def safe_directories(self):
        super().safe_directories()
        for relative in ("docs/community", DIRECTORY):
            path = self.root / relative
            if path.is_symlink() or not path.resolve().is_relative_to(self.root) or (path.exists() and not path.is_dir()):
                raise feedback_server.PublicationError("Unsafe community storage directory")

    def save_community_journal(self, document):
        path = self.community_journal
        temporary = path.with_suffix(".tmp")
        if path.is_symlink() or temporary.is_symlink():
            raise feedback_server.PublicationError("Unsafe community publication journal")
        with temporary.open("wb") as handle:
            handle.write(json.dumps(document, allow_nan=False).encode())
            handle.flush()
            os.fsync(handle.fileno())
        temporary.replace(path)
        self.sync_git_directory()

    def clear_community_journal(self):
        self.community_journal.unlink(missing_ok=True)
        self.sync_git_directory()

    def recover_interrupted(self):
        super().recover_interrupted()
        path = self.community_journal
        if not path.exists() and not path.is_symlink():
            return
        try:
            if path.is_symlink() or not path.is_file() or path.stat().st_size > 2000:
                raise ValueError
            document = json.loads(path.read_text(encoding="utf-8"))
            if set(document) != {"request_id", "base", "commit"}:
                raise ValueError
            identifier = feedback.submission_id(document["request_id"])
            base = sha(document["base"])
            commit = sha(document["commit"]) if document["commit"] else None
            allowed = {f"{DIRECTORY.as_posix()}/{identifier}.json"}
            if self.git("rev-parse", "HEAD").stdout.decode().strip() not in (base, commit):
                raise ValueError
            if commit:
                if (self.git("rev-parse", f"{commit}^").stdout.decode().strip() != base
                        or self.git("log", "-1", "--format=%s", commit).stdout.decode().strip()
                        != self.community_message(identifier)):
                    raise ValueError
                changed = self.git("diff-tree", "--no-commit-id", "--name-only", "-r", "-z", base, commit).stdout
                if {item.decode() for item in changed.split(b"\0") if item} != allowed:
                    raise ValueError
            self.rollback(base, allowed)
            self.clear_community_journal()
        except (ValueError, OSError, KeyError, TypeError, CommunityError):
            raise feedback_server.PublicationError("Interrupted community publication requires operator inspection") from None

    @staticmethod
    def community_message(identifier):
        return f"Record community event {identifier} [skip ci]"

    def actor(self, token):
        client = self.github_factory(token)
        client.source_repositories = (self.repository, self.games_repository)
        return client, client.actor(self.repository)

    def sync(self):
        self.git("fetch", "--quiet", "--no-tags", "origin", f"refs/heads/{self.branch}")
        self.git("reset", "--hard", "FETCH_HEAD")
        if self.companion_sources and time.monotonic() - self.companion_synced_at > 300:
            # Only public Git objects enter a namespaced ref. Never check out or
            # run companion source inside the intake service.
            self.git("fetch", "--quiet", "--no-tags", "--no-write-fetch-head", f"https://github.com/{self.games_repository}.git",
                     f"refs/heads/main:{COMPANION_REF}")
            self.companion_synced_at = time.monotonic()

    def catalog(self):
        cached = self._catalog_cache
        if cached is not None and time.monotonic() - cached[0] < CATALOG_CACHE_SECONDS:
            # This immutable snapshot was confirmed against origin. Serving it
            # does not contend with Git writes or trigger fetches per page view.
            return cached[1]
        if not self.lock.acquire(timeout=0.25):
            raise feedback_server.PublicationError("Another publication is active; retry shortly")
        try:
            self.recover_interrupted()
            self.require_clean()
            self.sync()
            self.safe_directories()
            catalog = Catalog(self)
            self._catalog_cache = (time.monotonic(), catalog)
            return catalog
        finally:
            self.lock.release()

    def prepare(self, action, target, request, actor, client, catalog, received):
        event = {"schema_version": 1, "request_id": request["request_id"], "sequence": len(catalog.events) + 1,
                 "created_at": received, "actor": actor, "request": request, "target": target}
        if action in ("create", "fork"):
            game_slug = request["slug"]
            if game_slug in catalog.games:
                existing = catalog.games[game_slug]
                if not (action == "create" and existing.get("source_unavailable") and actor["role"] == "maintainer"):
                    raise Conflict("This game slug already exists; choose your own unique slug")
            game = {"slug": game_slug, "title": request["title"], "owner": actor["login"],
                    "root_game": game_slug, "parent_game": None, "parent_revision": None}
            if action == "fork":
                parent = catalog.require_game(target)
                if parent.get("source_unavailable"):
                    raise Conflict("This published game has no recoverable source pin yet; a maintainer must register native source before it can be forked")
                parent_revision = request.get("source_revision", parent["head_revision"])
                source = next((item for item in parent["revisions"] if item["id"] == parent_revision), None)
                if source is None:
                    raise NotFound("The parent revision does not belong to this game")
                source = {**source, "kind": "fork", "actor": actor, "created_at": received}
                game.update(root_game=parent["root_game"], parent_game=target, parent_revision=parent_revision)
                event["event_type"] = "game_forked"
            else:
                source = self.submitted_source(request, game_slug, actor, client, received)
                event["event_type"] = "game_created"
            game["head_revision"] = source["id"]
            event.update(game=game, revision=source)
        elif action in ("revision", "publish", "release"):
            game = catalog.require_game(target)
            if actor["role"] != "maintainer" and game["owner"].lower() != actor["login"].lower():
                raise AccessDenied("Only this game's owner or an engine maintainer can update or publish it")
            if action == "revision":
                if request["expected_revision"] != game["head_revision"]:
                    raise Conflict("Game changed since your checkout; fetch its latest history before updating")
                source = self.submitted_source(request, target, actor, client, received)
                if game.get("upstream_source"):
                    client.require_current_upstream(source, game["upstream_source"])
                if any(item["id"] == source["id"] for item in game["revisions"]):
                    raise Conflict("This source revision is already in the game's history")
                source["parent_revision"] = game["head_revision"]
                event.update(event_type="revision_submitted", revision=source, upstream_revision=game.get("upstream_revision"))
            elif action == "publish":
                if request["revision_id"] != game["head_revision"]:
                    raise Conflict("Only the current revision can be queued; fetch the latest game history")
                source = next(item for item in game["revisions"] if item["id"] == request["revision_id"])
                if source["source"]["game_path"] != f"games/{target}":
                    raise Conflict("Import this standalone or historical source into games/<your-fork-slug> and submit that native fork before publishing")
                # Re-verify ownership and the immutable source, even if the caller
                # obtained the revision through a different session or account.
                client.verify_source(self.repository, source["source_repository"], source["source_commit"], source["source_game"],
                                     require_write=actor["role"] != "maintainer")
                if game.get("upstream_source"):
                    client.require_current_upstream(source, game["upstream_source"])
                event.update(event_type="publication_requested",
                             publication=client.pull_request(self.repository, game, source, request["request_id"]))
            else:
                if actor["role"] != "maintainer":
                    raise AccessDenied("Only engine maintainers can certify a production native release")
                source = next((item for item in game["revisions"] if item["id"] == request["revision_id"]), None)
                if source is None:
                    raise NotFound("This release revision does not belong to the game")
                event.update(event_type="release_verified", release=client.verify_release(
                    self.repository, self.games_repository, game, source, request))
        else:
            if request["target_type"] == "game":
                catalog.require_game(request["target"])
            event.update(event_type="feedback", feedback={key: value for key, value in request.items() if key != "request_id"},
                         target=request["target"] if request["target_type"] == "game" else request["target_type"])
        return event

    def submitted_source(self, request, game_slug, actor, client, received):
        source_repository = request.get("source_repository", self.repository)
        game = request.get("source_game", game_slug)
        client.verify_source(self.repository, source_repository, request["source_commit"], game)
        source = source_revision(source_repository, request["source_commit"], game, version=request["version"],
                                 note=request["note"], actor=actor, created_at=received, kind="submitted")
        source["source_branch"] = client.pin_source(source, game_slug)
        return source

    def publish_community(self, action, target, payload, token):
        request = canonical_request(action, payload)
        if target is not None:
            slug(target)
        client, actor = self.actor(token)
        if not self.lock.acquire(timeout=0.25):
            raise feedback_server.PublicationError("Another publication is active; retry with the same request ID")
        try:
            self.recover_interrupted()
            self.require_clean()
            received = datetime.now(timezone.utc).isoformat()
            for _ in range(self.attempts):
                self.sync()
                self.safe_directories()
                catalog = Catalog(self)
                previous = next((item for item in catalog.events if item["request_id"] == request["request_id"]), None)
                if previous:
                    expected_type = {"create": "game_created", "fork": "game_forked", "revision": "revision_submitted",
                                     "publish": "publication_requested", "release": "release_verified", "feedback": "feedback"}[action]
                    # The event target for feedback derives from its target type.
                    expected_target = request.get("target") or request.get("target_type") if action == "feedback" else target
                    if (previous["request"] != request or previous["actor"]["login"] != actor["login"]
                            or previous["event_type"] != expected_type or previous["target"] != expected_target):
                        raise Conflict("Request ID already identifies a different operation")
                    self._catalog_cache = None
                    return previous
                event = self.prepare(action, target, request, actor, client, catalog, received)
                relative = f"{DIRECTORY.as_posix()}/{request['request_id']}.json"
                base = self.git("rev-parse", "HEAD").stdout.decode().strip()
                journal = {"request_id": request["request_id"], "base": base, "commit": None}
                try:
                    self.save_community_journal(journal)
                    feedback.atomic_json(self.root / relative, event)
                    if self.changed_paths() != {relative}:
                        raise feedback_server.PublicationError("Unexpected community publication output")
                    self.git("add", "--", relative)
                    tree = self.git("write-tree").stdout.decode().strip()
                    commit = self.git("commit-tree", tree, "-p", base, "-m", self.community_message(request["request_id"])).stdout.decode().strip()
                    journal["commit"] = commit
                    self.save_community_journal(journal)
                    self.git("reset", "--hard", commit)
                    pushed = self.git("push", "--quiet", "origin", f"HEAD:refs/heads/{self.branch}", check=False)
                    if pushed.returncode == 0:
                        self.clear_community_journal()
                        self._catalog_cache = None
                        return event
                except Exception:
                    self.rollback(base, {relative})
                    self.clear_community_journal()
                    raise
                self.rollback(base, {relative})
                self.clear_community_journal()
            raise feedback_server.PublicationError("Community event could not be published; retry with the same request ID")
        finally:
            self.lock.release()


def validate_event(event):
    """Validate the authenticated receipt without making a network request."""
    actions = {"game_created": "create", "game_forked": "fork", "revision_submitted": "revision",
               "publication_requested": "publish", "release_verified": "release", "feedback": "feedback"}
    action = actions.get(event.get("event_type"))
    if action is None or canonical_request(action, event.get("request")) != event["request"]:
        raise CommunityError("Invalid stored community request")
    if event["request"]["request_id"] != event["request_id"] or type(event.get("sequence")) is not int or event["sequence"] < 1:
        raise CommunityError("Invalid stored receipt identity")
    actor = event.get("actor")
    if (not isinstance(actor, dict) or set(actor) != {"login", "role"} or not isinstance(actor["login"], str)
            or not LOGIN.fullmatch(actor["login"]) or actor["role"] not in ("maintainer", "contributor")):
        raise CommunityError("Invalid stored authenticated provenance")
    common = {"schema_version", "request_id", "sequence", "created_at", "actor", "request", "target", "event_type"}
    if action == "revision" and "upstream_revision" in event:
        common.add("upstream_revision")
        if event["upstream_revision"] is not None:
            revision_id(event["upstream_revision"])
    additional = {"create": {"game", "revision"}, "fork": {"game", "revision"}, "revision": {"revision"},
                  "publish": {"publication"}, "release": {"release"}, "feedback": {"feedback"}}[action]
    if set(event) != common | additional:
        raise CommunityError("Unexpected stored community receipt fields")
    if "revision" in event:
        source = event["revision"]
        expected = source_revision(source["source_repository"], source["source_commit"], source["source_game"],
                                   version=source["version"], note=source["note"], actor=source["actor"],
                                   created_at=source["created_at"], kind=source["kind"], branch=source["source_branch"],
                                   game_path=source["source"]["game_path"])
        if "parent_revision" in source:
            expected["parent_revision"] = revision_id(source["parent_revision"])
        if expected != source:
            raise CommunityError("Invalid stored immutable source pin")
        if source["actor"] != actor or source["created_at"] != event["created_at"]:
            raise CommunityError("Invalid source revision author or receipt time")
        if action in ("create", "revision"):
            request = event["request"]
            if (source["source_commit"] != request["source_commit"] or source["version"] != request["version"]
                    or source["note"] != request["note"] or source["kind"] != "submitted"
                    or ("source_repository" in request and source["source_repository"] != request["source_repository"])
                    or ("source_game" in request and source["source_game"] != request["source_game"])):
                raise CommunityError("Stored source pin differs from its submitted request")
        if action == "revision" and source.get("parent_revision") != event["request"]["expected_revision"]:
            raise CommunityError("Stored update lost its previous revision")
    if action in ("create", "fork"):
        game = event["game"]
        if (set(game) != {"slug", "title", "owner", "root_game", "parent_game", "parent_revision", "head_revision"}
                or game["slug"] != event["request"]["slug"] or game["title"] != event["request"]["title"]
                or game["owner"] != actor["login"] or game["head_revision"] != event["revision"]["id"]):
            raise CommunityError("Invalid stored game registration")
        slug(game["slug"])
        slug(game["root_game"])
        if action == "fork":
            if game["parent_game"] != event["target"] or game["parent_revision"] != event["revision"]["id"]:
                raise CommunityError("Invalid stored fork ancestry")
            slug(game["parent_game"])
        elif game["parent_game"] is not None or game["parent_revision"] is not None or event["target"] is not None:
            raise CommunityError("Invalid stored root game ancestry")
    elif action == "feedback":
        expected = {key: value for key, value in event["request"].items() if key != "request_id"}
        if expected != event["feedback"] or event["target"] != (expected["target"] or expected["target_type"]):
            raise CommunityError("Invalid stored feedback provenance")
    else:
        slug(event["target"])
        if action == "publish":
            publication = event["publication"]
            if (publication.get("state") != "awaiting_native_gates"
                    or type(publication.get("pull_request_number")) is not int
                    or not isinstance(publication.get("pull_request_url"), str)
                    or not re.fullmatch(r"https://github\.com/" + REPO.pattern + r"/pull/[1-9][0-9]*", publication["pull_request_url"])):
                raise CommunityError("Invalid stored native publication request")
        elif action == "release":
            release = event["release"]
            if "game_tree" in release:
                sha(release["game_tree"])
            if "source_kind" in release and (
                    release["source_kind"] not in ("engine", "companion")
                    or set(release.get("gates", {})) != (COMPANION_GATES if release["source_kind"] == "companion" else ENGINE_GATES)):
                raise CommunityError("Invalid native source ownership gate set")
            if (actor["role"] != "maintainer" or release.get("state") != "published"
                    or release.get("revision_id") != event["request"]["revision_id"]
                    or release.get("receipt") != event["request"]["receipt"]
                    or release.get("source_commit") != event["request"]["source_commit"]
                    or release.get("games_commit") != event["request"]["games_commit"]
                    or set(release.get("gates", {})) not in (ENGINE_GATES, COMPANION_GATES)):
                raise CommunityError("Invalid stored native production release receipt")


class RepositoryReader:
    """Read-only validator that also works in developer/CI linked worktrees."""

    def __init__(self, root, source_repository=REPOSITORY):
        self.root, self.repository = Path(root).resolve(), repository(source_repository)
        self.games_repository = GAMES_REPOSITORY
        self.companion_sources = self.git("show-ref", "--verify", "--quiet", COMPANION_REF, check=False).returncode == 0

    def git(self, *args, check=True):
        try:
            return subprocess.run(["git", "-c", "core.hooksPath=/dev/null", "-c", "core.fsmonitor=false", *args],
                                  cwd=self.root, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                  stderr=subprocess.DEVNULL, timeout=15, check=check)
        except (OSError, subprocess.SubprocessError):
            raise CommunityError("Cannot inspect source Git history") from None


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("check",))
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--repository", default=REPOSITORY)
    args = parser.parse_args(argv)
    try:
        catalog = Catalog(RepositoryReader(args.root, args.repository))
        print(json.dumps({"valid": True, "events": len(catalog.events), "games": len(catalog.games)}))
        return 0
    except (CommunityError, OSError, ValueError, KeyError, TypeError) as error:
        print(f"Community event validation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
