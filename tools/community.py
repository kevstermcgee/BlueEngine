#!/usr/bin/env python3
"""BlueEngine creator workflow: browse, fork, update, review, and publish native games."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
from urllib.error import HTTPError, URLError
from urllib.parse import urlsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener
from uuid import UUID, uuid4

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_ENDPOINT = "https://blue-engine.duckdns.org/community/v1"
SLUG = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*")
REPOSITORY = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+")


class CommunityError(ValueError):
    pass


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, request, *args, **kwargs):
        raise CommunityError("The community endpoint redirected; credentials were not forwarded")


def command(argv, cwd=None):
    result = subprocess.run(argv, cwd=cwd, capture_output=True, text=True,
                            env=dict(os.environ, GIT_TERMINAL_PROMPT="0"))
    if result.returncode:
        raise CommunityError(f"{argv[0]} failed; check authentication and repository access")
    return result.stdout.strip()


def token():
    value = os.environ.get("BLUEENGINE_COMMUNITY_TOKEN")
    if not value:
        try:
            value = command(["gh", "auth", "token"])
        except (CommunityError, OSError):
            raise CommunityError("Sign in with gh auth login, or set BLUEENGINE_COMMUNITY_TOKEN privately") from None
    if not value or len(value) > 1000 or any(ord(char) < 33 or ord(char) > 126 for char in value):
        raise CommunityError("Invalid GitHub credential format; sign in again")
    return value


class Client:
    def __init__(self, endpoint=DEFAULT_ENDPOINT, state_directory=None):
        parsed = urlsplit(endpoint)
        if parsed.username or parsed.password or parsed.query or parsed.fragment:
            raise CommunityError("The endpoint must not contain credentials, query parameters, or fragments")
        if parsed.scheme != "https" and not (parsed.scheme == "http" and parsed.hostname in ("127.0.0.1", "localhost", "::1")):
            raise CommunityError("The endpoint requires HTTPS (HTTP is allowed only for local development)")
        self.endpoint = endpoint.rstrip("/")
        self.state_directory = Path(state_directory or ROOT / ".be2-work/community/client")
        self.opener = build_opener(NoRedirect())

    def request(self, path, body=None, *, authenticated=False, request_id=None):
        headers = {"Accept": "application/json", "User-Agent": "BlueEngine-community"}
        if authenticated or body is not None:
            headers["Authorization"] = "Bearer " + token()
        if body is not None:
            body = dict(body)
            if request_id:
                if str(UUID(request_id, version=4)) != request_id:
                    raise CommunityError("--request-id must be a lowercase UUID v4")
            else:
                fingerprint = hashlib.sha256(json.dumps([self.endpoint, path, body], sort_keys=True).encode()).hexdigest()
                self.state_directory.mkdir(parents=True, exist_ok=True, mode=0o700)
                outbox = self.state_directory / (fingerprint + ".json")
                if outbox.exists():
                    request_id = json.loads(outbox.read_text())["request_id"]
                else:
                    request_id = str(uuid4())
                    # Retain the receipt identity before sending; identical retries cannot duplicate writes.
                    with outbox.open("x", encoding="utf-8") as output:
                        os.chmod(outbox, 0o600)
                        json.dump({"request_id": request_id}, output)
            body["request_id"] = request_id
            headers["Content-Type"] = "application/json"
        request = Request(self.endpoint + path, headers=headers,
                          data=json.dumps(body).encode() if body is not None else None)
        try:
            with self.opener.open(request, timeout=90) as response:
                result = json.load(response)
                if body is not None and (not isinstance(result, dict) or result.get("accepted") is not True
                                         or result.get("request_id") != request_id):
                    raise CommunityError("The service did not confirm this request; retry with the retained request ID")
                return result
        except HTTPError as error:
            # Never include arbitrary server response bodies or credentials in diagnostics.
            detail = {401: "Sign in again", 403: "This account lacks the required repository access",
                      409: "The game advanced or the request conflicts; inspect history before resubmitting",
                      404: "The game or community endpoint is unavailable",
                      429: "Rate limited; retry the same request later"}.get(error.code, "Request failed; retry later")
            raise CommunityError(f"HTTP {error.code}: {detail}" + (f" (request {request_id})" if body is not None else "")) from None
        except (URLError, TimeoutError, json.JSONDecodeError):
            raise CommunityError("Community service unavailable; source and pending requests are retained") from None


def game_document(value):
    if isinstance(value.get("game"), dict):
        revision = value.get("revision", value.get("event", {}).get("revision"))
        return {**value["game"], **({"revision": revision} if revision else {})}
    if isinstance(value.get("event"), dict):
        return game_document(value["event"])
    return value


def source_document(game):
    source = game.get("source", {})
    if not source and isinstance(game.get("revision"), dict):
        source = game["revision"].get("source", {})
    if not source:
        head = next((revision for revision in game.get("revisions", []) if revision.get("id") == game.get("head_revision")), {})
        source = head.get("source", {})
    repository = source.get("repository", game.get("source_repository"))
    commit = source.get("commit", game.get("source_commit"))
    game_path = source.get("game_path", "games/" + game.get("source_game", game.get("slug", "")))
    relative = PurePosixPath(game_path)
    standalone = game_path == "." and repository not in ("kevstermcgee/BlueEngine", "kevstermcgee/BlueEngineGames")
    if (not isinstance(repository, str) or not REPOSITORY.fullmatch(repository)
            or not isinstance(commit, str) or not re.fullmatch(r"[0-9a-f]{40}", commit)
            or relative.is_absolute() or ".." in relative.parts or "\\" in game_path
            or (not standalone and (len(relative.parts) < 2 or relative.parts[0] not in ("games", "assets")))):
        raise CommunityError("The game has no valid immutable source pin")
    return repository, commit, relative


def import_standalone(git, repository, commit, destination, target):
    """Import tracked blobs from a pinned standalone game into a fresh engine fork.

    Read Git objects directly; no archive extraction, checkout filters, hooks,
    symlink traversal, submodule initialization, or submitted code execution.
    """
    command([*git, "fetch", "--no-write-fetch-head", "--no-tags", "https://github.com/" + repository + ".git", commit], destination)
    entries = command([*git, "ls-tree", "-r", "-z", "--full-tree", commit], destination).split("\0")
    files = []
    windows_names = {}
    excluded = {".git", ".github", "target", "dist", ".be2-work", ".blue-check", "__pycache__"}
    for entry in filter(None, entries):
        description, name = entry.split("\t", 1)
        mode, kind, object_id = description.split(" ")
        relative = PurePosixPath(name)
        if (relative.is_absolute() or ".." in relative.parts or "\\" in name
                or any(ord(char) < 32 or char in '<>:"|?*' for char in name) or not relative.parts
                or any(part.endswith((" ", ".")) or part.casefold().rstrip(" .") == ".git"
                       or re.fullmatch(r"\.?git~[0-9]+", part, re.I)
                       or re.fullmatch(r"(?:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])", part.split(".", 1)[0], re.I)
                       for part in relative.parts)):
            raise CommunityError("Standalone source contains an unsafe path; checkout retained")
        if relative.parts[0] in excluded:
            continue
        for index in range(1, len(relative.parts) + 1):
            prefix = "/".join(relative.parts[:index])
            previous = windows_names.setdefault(prefix.casefold(), prefix)
            if previous != prefix:
                raise CommunityError("Standalone source paths collide on Windows; checkout retained")
        if mode not in ("100644", "100755") or kind != "blob" or not re.fullmatch(r"[0-9a-f]{40,64}", object_id):
            raise CommunityError("Standalone source contains symlinks or submodules; checkout retained for review")
        files.append((relative, object_id, mode))
    if not any(str(path) == "Cargo.toml" for path, _, _ in files):
        raise CommunityError("Standalone source needs a committed Cargo project before import")
    target.mkdir(parents=True)
    for relative, object_id, mode in files:
        result = subprocess.run([*git, "cat-file", "blob", object_id], cwd=destination, capture_output=True)
        if result.returncode:
            raise CommunityError("Could not retrieve a pinned source blob; checkout retained")
        path = target.joinpath(*relative.parts)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(result.stdout)
        if mode == "100755":
            path.chmod(0o755)


def checkout(game, destination):
    game = game_document(game)
    slug = game.get("slug", "")
    if not SLUG.fullmatch(slug):
        raise CommunityError("Invalid game slug")
    repository, commit, relative = source_document(game)
    destination = Path(destination).absolute()
    if destination.exists():
        raise CommunityError("Checkout destination already exists; choose a new directory")
    destination.parent.mkdir(parents=True, exist_ok=True)
    git = ["git", "-c", "core.hooksPath=/dev/null", "-c", "core.fsmonitor=false"]
    standalone = str(relative) == "."
    checkout_repository = "kevstermcgee/BlueEngine" if standalone else repository
    command([*git, "clone", "--no-checkout", "--filter=blob:none", "https://github.com/" + checkout_repository + ".git", str(destination)])
    if standalone:
        command([*git, "checkout", "--detach", "origin/main"], destination)
    else:
        command([*git, "fetch", "origin", commit], destination)
        command([*git, "checkout", "--detach", commit], destination)
    command([*git, "switch", "-c", "community/" + slug + "-" + uuid4().hex[:8]], destination)
    target = destination / "games" / slug
    if standalone:
        if target.exists() or (target.parent.exists() and (target.parent.is_symlink() or not target.parent.resolve().is_relative_to(destination.resolve()))):
            raise CommunityError("The fork slug collides with existing source; checkout retained")
        import_standalone(git, repository, commit, destination, target)
    original = target if standalone else destination.joinpath(*relative.parts)
    if (not original.resolve().is_relative_to(destination.resolve()) or not original.is_dir()
            or any(path.is_symlink() for path in [original, *original.parents] if path.is_relative_to(destination))):
        raise CommunityError("Pinned game source directory is missing or unsafe; checkout retained")
    if any(path.is_symlink() for path in original.rglob("*")):
        raise CommunityError("Game source contains symlinks; checkout retained for review")
    if target != original:
        if target.exists():
            raise CommunityError("The fork slug collides with existing source; checkout retained")
        shutil.copytree(original, target)
    if target != original or standalone:
        for path, key, value in ((target / "game.project.json", "id", slug),
                                 (target / "assets/identity.json", "title", game.get("title", slug))):
            if path.is_file():
                document = json.loads(path.read_text())
                document[key] = value
                path.write_text(json.dumps(document, indent=2) + "\n")
    metadata = {"schema_version": 1, "slug": slug, "source": {"repository": repository,
                "commit": commit, "game_path": str(relative)}, "revision_id": game.get("head_revision", game.get("revision_id"))}
    metadata_path = target / "community-source.json"
    descriptor = os.open(metadata_path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | getattr(os, "O_NOFOLLOW", 0), 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as output:
        output.write(json.dumps(metadata, indent=2) + "\n")
    return {"checkout": str(destination), "game": str(target), "source": metadata["source"],
            "creator_cli": str(Path(__file__).resolve()),
            "next": f"Read {target / 'AGENTS.md'}, make the game your own, and retain community-source.json. "
                    "Use a new icon and title. Keep the BlueEngine Games opening card centered and text-only; do not reuse retired Feta/white-rat artwork in new art or branding. Commit/push the source, submit a revision, then request publication. "
                    "Native Windows installer gates remain required. For companion-owned source, bind its engine "
                    "dependency using the project's documented tooling before checking it. Use creator_cli from "
                    "the current tools checkout for collaboration commands if the pinned source predates them. "
                    "Imported standalone games retain their original build files; migrate their engine/tooling "
                    "bindings and declare the new native release definition before publishing."}


def record_release(client, directory, revision=None):
    """Attach verified native gate receipts to immutable creator history."""
    directory = Path(directory).resolve()
    state_path = directory / "state.json"
    state = json.loads(state_path.read_text())
    if state.get("workflow") != "community-release" or state.get("status") != "published":
        raise CommunityError("Only a completed native community release can be recorded")
    game = game_document(client.request("/games/" + state["idea"]["slug"]))
    revision = revision or state.get("community_revision")
    if not revision:
        candidate = next((item for item in game.get("revisions", [])
                          if item.get("source_commit") in (state["base"], state["engine_head"])), None)
        if candidate:
            revision = candidate["id"]
        elif game.get("head_revision"):
            # Merges/registry commits may change the commit ID without changing
            # the game. The server independently proves exact tree equality.
            revision = game["head_revision"]
    if not revision:
        raise CommunityError("Register/submit the released source revision, then use record-release RUN --revision REVISION_ID")
    labels = ("windows-installer-review", "production-release", "download-page")
    if state.get("source_kind", "engine") == "engine":
        labels = ("engine-checks", "generated-game-linux-windows", *labels)
    else:
        labels = ("companion-native-checks", *labels)
    body = {"revision_id": revision, "source_commit": state["engine_head"], "games_commit": state["games_head"],
            "receipt": state["publication"], "gates": {label: state["ci"][label]["run"] for label in labels}}
    value = client.request("/games/" + state["idea"]["slug"] + "/releases", body)
    state["community_revision"] = revision
    state["community_receipt"] = value
    temporary = state_path.with_suffix(".tmp")
    temporary.write_text(json.dumps(state, indent=2) + "\n")
    temporary.replace(state_path)
    return value


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--endpoint", default=os.environ.get("BLUEENGINE_COMMUNITY_URL", DEFAULT_ENDPOINT))
    result.add_argument("--request-id", help="Retain across retries; otherwise an ID is journaled by request content")
    commands = result.add_subparsers(dest="command", required=True)
    for name in ("games", "me"):
        commands.add_parser(name)
    for name in ("show", "history", "checkout"):
        command_parser = commands.add_parser(name)
        command_parser.add_argument("slug")
        if name == "checkout":
            command_parser.add_argument("destination", type=Path)
    fork = commands.add_parser("fork")
    fork.add_argument("parent")
    fork.add_argument("--slug", required=True)
    fork.add_argument("--title", required=True)
    fork.add_argument("--checkout", type=Path)
    for name in ("register", "submit"):
        submit = commands.add_parser(name)
        submit.add_argument("slug")
        submit.add_argument("--commit", required=True)
        submit.add_argument("--source-repository", default="kevstermcgee/BlueEngine")
        submit.add_argument("--source-game")
        if name == "register":
            submit.add_argument("--title", required=True)
            submit.add_argument("--version", default="0.1.0")
            submit.add_argument("--note", default="Register authored native game source")
        else:
            submit.add_argument("--expected-revision", required=True)
            submit.add_argument("--version", required=True)
            submit.add_argument("--note", required=True)
    publish = commands.add_parser("publish")
    publish.add_argument("slug")
    publish.add_argument("--revision", required=True)
    feedback = commands.add_parser("feedback")
    feedback.add_argument("--target-type", choices=("engine", "launcher", "game"), required=True)
    feedback.add_argument("--target")
    feedback.add_argument("--kind", choices=("human", "ai"), default="ai")
    feedback.add_argument("--category", default="workflow")
    feedback.add_argument("--note", required=True)
    feedback.add_argument("--version", default="")
    release = commands.add_parser("release")
    release.add_argument("slug")
    release.add_argument("--engine-root", type=Path, default=ROOT)
    release.add_argument("--games-root", type=Path, required=True)
    release.add_argument("--runs-root", type=Path)
    release.add_argument("--revision", help="Creator revision ID to attach the verified release receipt")
    release.add_argument("--source", choices=("engine", "companion"), default="engine", help="Repository that owns this game's source")
    resume = commands.add_parser("resume-release")
    resume.add_argument("directory", type=Path)
    record = commands.add_parser("record-release")
    record.add_argument("directory", type=Path)
    record.add_argument("--revision")
    return result


def main(argv=None):
    args = parser().parse_args(argv)
    try:
        if args.command == "record-release":
            value = record_release(Client(args.endpoint), args.directory, args.revision)
        elif args.command in ("release", "resume-release"):
            source = args.source if args.command == "release" else json.loads((args.directory / "state.json").read_text()).get("source_kind", "engine")
            if source == "companion":
                try:
                    from tools import community_release_companion as community_release
                except ModuleNotFoundError:
                    import community_release_companion as community_release
            else:
                try:
                    from tools import community_release
                except ModuleNotFoundError:
                    import community_release
            if args.command == "release":
                value = community_release.run(args.engine_root, args.games_root, args.slug, runs_root=args.runs_root)
            else:
                value = community_release.resume(args.directory)
            try:
                value["community_receipt"] = record_release(Client(args.endpoint), value["run"], getattr(args, "revision", None))
            except (CommunityError, OSError, ValueError, KeyError) as error:
                # Delivery already passed all native/public gates. A metadata
                # outage must not cause a second installer release on retry.
                value["community_receipt"] = {"status": "pending", "reason": str(error),
                                               "retry": f"python3 tools/community.py record-release {value['run']}" +
                                                        (f" --revision {args.revision}" if getattr(args, "revision", None) else "")}
        else:
            client = Client(args.endpoint)
            if args.command in ("show", "history", "checkout", "submit", "register", "publish") and not SLUG.fullmatch(args.slug):
                raise CommunityError("Invalid game slug")
            if args.command == "games":
                value = client.request("/games")
            elif args.command == "me":
                value = client.request("/me", authenticated=True)
            elif args.command in ("show", "history", "checkout"):
                value = client.request("/games/" + args.slug + ("/history" if args.command == "history" else ""))
                if args.command == "checkout":
                    value = checkout(value, args.destination)
            else:
                if args.command == "fork":
                    if not SLUG.fullmatch(args.parent) or not SLUG.fullmatch(args.slug):
                        raise CommunityError("Invalid parent or fork slug")
                    path, body = "/games/" + args.parent + "/forks", {"slug": args.slug, "title": args.title}
                elif args.command in ("register", "submit"):
                    body = {"source_commit": args.commit, "source_repository": args.source_repository,
                            "source_game": args.source_game or args.slug}
                    if args.command == "register":
                        path = "/games"
                        body.update(slug=args.slug, title=args.title, version=args.version, note=args.note)
                    else:
                        path = "/games/" + args.slug + "/revisions"
                        body.update(expected_revision=args.expected_revision, version=args.version, note=args.note)
                elif args.command == "publish":
                    path, body = "/games/" + args.slug + "/publish", {"revision_id": args.revision}
                else:
                    path, body = "/feedback", {"target_type": args.target_type, "kind": args.kind,
                                              "category": args.category, "note": args.note, "version": args.version}
                    if args.target:
                        body["target"] = args.target
                value = client.request(path, body, request_id=args.request_id)
                if args.command == "fork" and args.checkout:
                    value = {"fork": value, **checkout(value, args.checkout)}
        print(json.dumps(value, indent=2))
        return 0
    except (CommunityError, OSError, ValueError, KeyError, RuntimeError) as error:
        print(f"community: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
