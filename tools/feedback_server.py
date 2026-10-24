#!/usr/bin/env python3
"""Account-free review intake; acknowledge only reviews published to the engine repo.

Run behind an HTTPS reverse proxy using a dedicated checkout and server-only Git
credentials. Request text and client addresses are never written to service logs.
"""
from __future__ import annotations

import argparse
from collections import deque
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re
import socket
import ssl
import subprocess
import sys
import threading
import time

try:
    import fcntl
except ImportError:  # Keep the repository's Python checks importable on Windows.
    fcntl = None
    import msvcrt

try:
    from tools import player_feedback as feedback
except ModuleNotFoundError:
    import player_feedback as feedback

MAX_BODY = 20_000
TRUSTED_ORIGIN = "https://kevstermcgee.github.io"


class PublicationError(RuntimeError):
    """The repository did not confirm publication; callers may safely retry."""


class SubmissionConflict(feedback.FeedbackError):
    """A published submission ID already identifies different feedback."""


class Publisher:
    """Own one disposable, clean clone; serialize fetch/import/commit/push."""

    def __init__(self, root, *, branch="main", attempts=3, git_timeout=15):
        self.root = Path(root).resolve()
        self.branch = branch
        self.attempts = attempts
        self.git_timeout = git_timeout
        self.lock = threading.Lock()
        self.checkout_lock = None
        self.journal = self.root / ".git/blueengine-feedback-inflight.json"
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._/-]{0,120}", branch) or attempts not in range(1, 6):
            raise PublicationError("Invalid publication configuration")
        # A worktree shares a developer's Git state: require a dedicated clone.
        if not (self.root / ".git").is_dir() or (self.root / ".git").is_symlink():
            raise PublicationError("A dedicated initialized Git clone is required")
        try:
            self.checkout_lock = (self.root / ".git/blueengine-feedback.lock").open("a+b")
            if fcntl is not None:
                fcntl.flock(self.checkout_lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            else:
                if not self.checkout_lock.tell():
                    self.checkout_lock.write(b"\0")
                    self.checkout_lock.flush()
                self.checkout_lock.seek(0)
                msvcrt.locking(self.checkout_lock.fileno(), msvcrt.LK_NBLCK, 1)
            if Path(self.git("rev-parse", "--show-toplevel").stdout.decode().strip()).resolve() != self.root:
                raise PublicationError("Unexpected Git checkout root")
            self.git("check-ref-format", "--branch", branch)
            if self.git("branch", "--show-current").stdout.decode().strip() != branch:
                raise PublicationError("Checkout must use the configured publication branch")
            if not self.git("remote", "get-url", "origin").stdout.strip():
                raise PublicationError("An origin remote is required")
            self.safe_directories()
            self.recover_interrupted()
            self.require_clean()
            feedback.game_catalog(self.root)
        except (OSError, feedback.FeedbackError, PublicationError) as error:
            self.close()
            if isinstance(error, PublicationError):
                raise
            raise PublicationError("Cannot initialize the dedicated publication checkout") from None

    def close(self):
        if self.checkout_lock is not None:
            self.checkout_lock.close()
            self.checkout_lock = None

    def git(self, *args, check=True):
        environment = dict(os.environ, GIT_TERMINAL_PROMPT="0")
        command = ["git", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false",
                   "-c", "core.fsmonitor=false", "-c", "user.name=BlueEngine Feedback",
                   "-c", "user.email=feedback@blueengine.invalid", *args]
        try:
            result = subprocess.run(command, cwd=self.root, env=environment, stdin=subprocess.DEVNULL,
                                    stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                    timeout=self.git_timeout, check=False)
        except (OSError, subprocess.TimeoutExpired):
            raise PublicationError("Repository publication is temporarily unavailable") from None
        if check and result.returncode:
            raise PublicationError("Repository publication is temporarily unavailable")
        return result

    def require_clean(self):
        if self.git("status", "--porcelain", "--untracked-files=all").stdout:
            raise PublicationError("Publication checkout contains unrelated changes")

    def sync(self):
        self.git("fetch", "--quiet", "--no-tags", "origin", f"refs/heads/{self.branch}")
        self.git("reset", "--hard", "FETCH_HEAD")

    def sync_for_review(self):
        self.sync()

    def game_catalog(self):
        return feedback.game_catalog(self.root)

    def safe_directories(self):
        for relative in ("docs", "docs/player-feedback", "docs/player-feedback/reviews", "docs/player-feedback/games"):
            path = self.root / relative
            if path.is_symlink() or not path.resolve().is_relative_to(self.root) or (path.exists() and not path.is_dir()):
                raise PublicationError("Unsafe feedback storage directory")

    def changed_paths(self):
        tracked = self.git("diff", "--name-only", "-z", "HEAD", "--").stdout
        untracked = self.git("ls-files", "--others", "--exclude-standard", "-z").stdout
        return {part.decode("utf-8") for part in (tracked + untracked).split(b"\0") if part}

    def sync_git_directory(self):
        if os.name == "posix":
            descriptor = os.open(self.root / ".git", os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
            try:
                os.fsync(descriptor)
            finally:
                os.close(descriptor)

    def save_journal(self, document):
        temporary = self.journal.with_suffix(".tmp")
        if self.journal.is_symlink() or temporary.is_symlink():
            raise PublicationError("Unsafe publication journal")
        with temporary.open("wb") as handle:
            handle.write(json.dumps(document, ensure_ascii=True, allow_nan=False).encode("ascii"))
            handle.flush()
            os.fsync(handle.fileno())
        temporary.replace(self.journal)
        self.sync_git_directory()

    def clear_journal(self):
        self.journal.unlink(missing_ok=True)
        self.sync_git_directory()

    def temporary_outputs(self, allowed):
        result = set()
        for relative in allowed:
            path = self.root / relative
            for candidate in path.parent.glob(path.name + ".*.tmp"):
                if re.fullmatch(re.escape(path.name) + r"\.[0-9a-f]{32}\.tmp", candidate.name):
                    result.add(candidate.relative_to(self.root).as_posix())
        return result

    def recover_interrupted(self):
        if not self.journal.exists() and not self.journal.is_symlink():
            return
        try:
            self.safe_directories()
            if self.journal.is_symlink() or not self.journal.is_file() or self.journal.stat().st_size > 100_000:
                raise ValueError
            document = json.loads(self.journal.read_text(encoding="ascii"))
            if (set(document) != {"schema_version", "submission_id", "base", "commit", "paths"}
                    or type(document["schema_version"]) is not int or document["schema_version"] != 1):
                raise ValueError
            identifier = feedback.submission_id(document["submission_id"])
            base, commit, paths = document["base"], document["commit"], document["paths"]
            if (not isinstance(base, str) or not re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", base)
                    or (commit is not None and (not isinstance(commit, str)
                        or not re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", commit)))
                    or not isinstance(paths, list) or not 2 <= len(paths) <= 1000
                    or any(not isinstance(path, str) for path in paths)):
                raise ValueError
            allowed = set(paths)
            own_review = f"docs/player-feedback/reviews/{identifier}.json"
            if not {own_review, "docs/player-feedback/summary.json"} <= allowed:
                raise ValueError
            if any(path not in (own_review, "docs/player-feedback/summary.json", "docs/player-feedback/games.json")
                   and not re.fullmatch(r"docs/player-feedback/games/[a-z0-9]+(?:-[a-z0-9]+)*\.json", path)
                   for path in allowed):
                raise ValueError
            head = self.git("rev-parse", "HEAD").stdout.decode().strip()
            if head not in (base, commit):
                raise ValueError
            if self.git("cat-file", "-t", base).stdout.strip() != b"commit":
                raise ValueError
            if commit is not None:
                if self.git("rev-parse", f"{commit}^").stdout.decode().strip() != base:
                    raise ValueError
                committed = self.git("diff-tree", "--no-commit-id", "--name-only", "-r", "-z", base, commit).stdout
                committed = {path.decode("utf-8") for path in committed.split(b"\0") if path}
                if not committed <= allowed:
                    raise ValueError
                expected = self.commit_message(identifier, catalog_changed="docs/player-feedback/games.json" in committed)
                if self.git("log", "-1", "--format=%s", commit).stdout.decode().strip() != expected:
                    raise ValueError
            self.rollback(base, allowed)
            self.clear_journal()
        except (OSError, ValueError, TypeError, KeyError, feedback.FeedbackError):
            raise PublicationError("Interrupted publication requires operator inspection") from None

    @staticmethod
    def commit_message(identifier, *, catalog_changed=False):
        # Allowlist changes fall outside nightly's review-only CI inheritance.
        # Their own CI must run before an improvement cycle can use this tip.
        return f"Record player feedback {identifier}" + ("" if catalog_changed else " [skip ci]")

    def rollback(self, base, allowed):
        # Never use git clean: delete only this import's untracked output paths.
        temporary = self.temporary_outputs(allowed)
        changed = self.changed_paths()
        if not changed <= allowed | temporary:
            raise PublicationError("Publication checkout contains unrelated changes")
        for relative in changed | temporary:
            path = self.root / relative
            if path.is_symlink() or not path.resolve().is_relative_to(self.root):
                raise PublicationError("Unsafe interrupted feedback output")
        self.git("reset", "--hard", base)
        for relative in allowed | temporary:
            path = self.root / relative
            if path.is_file() and self.git("ls-files", "--error-unmatch", "--", relative, check=False).returncode:
                path.unlink()

    def publish(self, payload):
        # Reject malformed public input before performing network/Git work. The
        # authoritative game allowlist is checked again after fetching origin.
        candidate = payload.get("game") if isinstance(payload, dict) else None
        feedback.record_from_submission(payload, {candidate: "Pending game validation"}
                                        if isinstance(candidate, str) else {})
        if not self.lock.acquire(timeout=0.25):
            raise PublicationError("Another review is publishing; retry shortly")
        try:
            self.recover_interrupted()
            self.require_clean()
            received = datetime.now(timezone.utc).isoformat()
            for attempt in range(self.attempts):
                # Fetch first even for an ID retry: a local commit alone is no receipt.
                self.sync_for_review()
                self.safe_directories()
                games = self.game_catalog()
                record = feedback.record_from_submission(payload, games, received)
                identifier = record["submission_id"]
                relative_review = f"docs/player-feedback/reviews/{identifier}.json"
                allowed = {relative_review, "docs/player-feedback/summary.json", "docs/player-feedback/games.json"}
                allowed.update(f"docs/player-feedback/games/{slug}.json" for slug in games)
                # Feed cleanup may remove an obsolete game's canonical JSON feed.
                allowed.update(path.relative_to(self.root).as_posix()
                               for path in feedback.public_feed_paths(self.root).values())
                existing = self.root / relative_review
                if existing.exists():
                    previous = feedback.validate_record(json.loads(existing.read_text(encoding="utf-8")), games)
                    if any(previous.get(key) != value for key, value in record.items()
                           if key not in ("created_at", "updated_at")):
                        raise SubmissionConflict("Submission ID already belongs to different feedback")
                base = self.git("rev-parse", "HEAD").stdout.decode().strip()
                journal = {"schema_version": 1, "submission_id": identifier, "base": base,
                           "commit": None, "paths": sorted(allowed)}
                try:
                    self.save_journal(journal)
                    # Keep trusted newly published game identities with the same
                    # journal/commit as their review and canonical public feeds.
                    feedback.atomic_json(self.root / feedback.DIRECTORY / "games.json",
                                         {"schema_version": 1, "games": dict(sorted(games.items()))})
                    feedback.import_submission(self.root, payload, created_at=received)
                    changed = self.changed_paths()
                    if not changed <= allowed:
                        raise PublicationError("Import attempted to modify an unexpected repository path")
                    if not changed:
                        self.clear_journal()
                        return identifier  # Already fetched from the publication remote.
                    for relative in changed:
                        path = self.root / relative
                        if path.is_symlink() or not path.resolve().is_relative_to(self.root):
                            raise PublicationError("Unsafe feedback output path")
                    self.git("add", "--", *sorted(changed))
                    tree = self.git("write-tree").stdout.decode().strip()
                    message = self.commit_message(identifier, catalog_changed="docs/player-feedback/games.json" in changed)
                    commit = self.git("commit-tree", tree, "-p", base, "-m", message).stdout.decode().strip()
                    journal["commit"] = commit
                    # Journal the new commit before moving HEAD, so interruption
                    # at any point leaves a recognizable, recoverable checkout.
                    self.save_journal(journal)
                    self.git("reset", "--hard", commit)
                    pushed = self.git("push", "--quiet", "origin", f"HEAD:refs/heads/{self.branch}", check=False)
                    if pushed.returncode == 0:
                        self.clear_journal()
                        return identifier
                except Exception:
                    self.rollback(base, allowed)
                    self.clear_journal()
                    raise
                # Includes non-fast-forward races and lost push acknowledgements.
                # Re-fetch and re-import instead of force pushing or duplicating IDs.
                self.rollback(base, allowed)
                self.clear_journal()
            raise PublicationError("Review could not be published; retry with the same submission ID")
        finally:
            self.lock.release()


class RateLimit:
    """Bound the total publication rate without retaining client addresses."""

    def __init__(self, maximum=20, period=60):
        self.maximum, self.period = maximum, period
        self.times = deque()
        self.lock = threading.Lock()

    def accept(self):
        now = time.monotonic()
        with self.lock:
            while self.times and self.times[0] <= now - self.period:
                self.times.popleft()
            if len(self.times) >= self.maximum:
                return False
            self.times.append(now)
            return True


class FeedbackServer(ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = True
    request_queue_size = 8

    def __init__(self, address, publisher, *, rate_limit=None, connections=8):
        self.publisher = publisher
        self.rate_limit = rate_limit or RateLimit()
        self.slots = threading.BoundedSemaphore(connections)
        super().__init__(address, FeedbackHandler)

    def process_request(self, request, client_address):
        if not self.slots.acquire(blocking=False):
            try:
                request.settimeout(1)
                request.sendall(b"HTTP/1.0 503 Service Unavailable\r\nContent-Length: 0\r\nRetry-After: 5\r\nConnection: close\r\n\r\n")
            except OSError:
                pass
            self.shutdown_request(request)
            return
        try:
            super().process_request(request, client_address)
        except Exception:
            self.slots.release()
            raise

    def process_request_thread(self, request, client_address):
        try:
            super().process_request_thread(request, client_address)
        finally:
            self.slots.release()

    def handle_error(self, request, client_address):
        # The default traceback includes client addresses; HTTP errors stay generic.
        pass


class FeedbackHandler(BaseHTTPRequestHandler):
    server_version = "BlueEngineFeedback"
    sys_version = ""
    protocol_version = "HTTP/1.0"

    def setup(self):
        self.request.settimeout(5)
        super().setup()
        # Absolute header deadline also bounds clients that drip-feed headers.
        self.header_timer = threading.Timer(10, self.expire_headers)
        self.header_timer.daemon = True
        self.header_timer.start()

    def expire_headers(self):
        try:
            self.connection.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass

    def parse_request(self):
        try:
            return super().parse_request()
        finally:
            self.header_timer.cancel()

    def finish(self):
        self.header_timer.cancel()
        super().finish()

    def log_message(self, format, *args):
        pass

    def send_error(self, code, message=None, explain=None):
        self.respond(code, {"accepted": False, "error": "Invalid HTTP request"})

    def origin_allowed(self):
        origins = self.headers.get_all("Origin", [])
        return not origins or origins == [TRUSTED_ORIGIN]

    def respond(self, status, body, *, retry=False):
        content = json.dumps(body, ensure_ascii=True, allow_nan=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(content)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Connection", "close")
        if hasattr(self, "headers") and self.headers.get_all("Origin", []) == [TRUSTED_ORIGIN]:
            self.send_header("Access-Control-Allow-Origin", TRUSTED_ORIGIN)
            self.send_header("Vary", "Origin")
        if retry:
            self.send_header("Retry-After", "5")
        self.end_headers()
        try:
            self.wfile.write(content)
        except OSError:
            pass

    def do_GET(self):
        if self.path != "/health":
            self.respond(404, {"accepted": False, "error": "Unknown endpoint"})
        elif not self.origin_allowed():
            self.respond(403, {"accepted": False, "error": "Origin is not allowed"})
        else:
            self.respond(200, {"ready": True, "service": "blueengine-player-feedback", "format": feedback.FORMAT})

    def do_OPTIONS(self):
        if self.path != "/feedback":
            self.respond(404, {"accepted": False, "error": "Unknown endpoint"})
            return
        requested = self.headers.get("Access-Control-Request-Headers", "").lower().split(",")
        if (not self.origin_allowed() or self.headers.get("Access-Control-Request-Method") != "POST"
                or any(header.strip() not in ("", "content-type") for header in requested)):
            self.respond(403, {"accepted": False, "error": "Preflight is not allowed"})
            return
        self.send_response(204)
        self.send_header("Access-Control-Allow-Origin", TRUSTED_ORIGIN)
        self.send_header("Access-Control-Allow-Methods", "POST")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        self.send_header("Access-Control-Max-Age", "600")
        self.send_header("Vary", "Origin")
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_POST(self):
        if self.path != "/feedback":
            self.respond(404, {"accepted": False, "error": "Unknown endpoint"})
            return
        if not self.origin_allowed():
            self.respond(403, {"accepted": False, "error": "Origin is not allowed"})
            return
        types = self.headers.get_all("Content-Type", [])
        if len(types) != 1 or not re.fullmatch(r"application/json(?:\s*;\s*charset=utf-8)?", types[0], re.I):
            self.respond(415, {"accepted": False, "error": "Use application/json with UTF-8 encoding"})
            return
        lengths = self.headers.get_all("Content-Length", [])
        if self.headers.get_all("Transfer-Encoding") or len(lengths) != 1 or not re.fullmatch(r"[0-9]{1,8}", lengths[0]):
            self.respond(400, {"accepted": False, "error": "One Content-Length is required"})
            return
        length = int(lengths[0])
        if not 0 < length <= MAX_BODY:
            self.respond(413, {"accepted": False, "error": "Review exceeds the request size limit"})
            return
        if not self.server.rate_limit.accept():
            self.respond(429, {"accepted": False, "error": "Review rate limit reached; retry shortly"}, retry=True)
            return
        try:
            deadline, body = time.monotonic() + 10, bytearray()
            while len(body) < length:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError
                self.connection.settimeout(min(remaining, 5))
                chunk = self.rfile.read1(min(4096, length - len(body)))
                if not chunk:
                    raise ValueError
                body.extend(chunk)
            payload = json.loads(body.decode("utf-8"), object_pairs_hook=unique_fields,
                                 parse_constant=reject_constant)
            identifier = self.server.publisher.publish(payload)
        except (TimeoutError, socket.timeout):
            self.respond(408, {"accepted": False, "error": "Review request timed out"})
        except SubmissionConflict as error:
            self.respond(409, {"accepted": False, "error": str(error)})
        except feedback.UnknownGame:
            self.respond(400, {"accepted": False, "code": "game_not_available", "error": "Unknown game slug"})
        except feedback.FeedbackError as error:
            self.respond(400, {"accepted": False, "error": str(error)})
        except (ValueError, UnicodeError, RecursionError):
            self.respond(400, {"accepted": False, "error": "Invalid review JSON"})
        except (PublicationError, OSError):
            self.respond(503, {"accepted": False, "error": "Review could not be published; retry with the same submission ID"}, retry=True)
        else:
            self.respond(200, {"accepted": True, "submission_id": identifier})


def unique_fields(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("Duplicate JSON field")
        result[key] = value
    return result


def reject_constant(value):
    raise ValueError("Non-finite JSON number")


def main(argv=None):
    arguments = list(sys.argv[1:] if argv is None else argv)
    if "--community" in arguments:
        # Both routes share one publisher, checkout lock and mutation lock. A
        # community transaction therefore cannot race an anonymous review reset.
        try:
            from tools import community_server
        except ModuleNotFoundError:
            import community_server
        arguments.remove("--community")
        return community_server.main(arguments)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path, help="Dedicated clean Git clone with a writable origin")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", default=8091, type=int)
    parser.add_argument("--branch", default="main")
    parser.add_argument("--cert", type=Path, help="TLS certificate; otherwise an HTTPS reverse proxy is required")
    parser.add_argument("--key", type=Path, help="TLS private key")
    parser.add_argument("--community", action="store_true", help="Also serve authenticated /community/v1 collaboration")
    args = parser.parse_args(arguments)
    if bool(args.cert) != bool(args.key):
        parser.error("--cert and --key must be supplied together")
    if args.host not in ("127.0.0.1", "::1", "localhost") and not args.cert:
        parser.error("Direct public listening requires --cert and --key; bind loopback behind an HTTPS proxy")
    publisher = None
    try:
        publisher = Publisher(args.root, branch=args.branch)
        with FeedbackServer((args.host, args.port), publisher) as server:
            if args.cert:
                context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
                context.minimum_version = ssl.TLSVersion.TLSv1_2
                context.load_cert_chain(args.cert, args.key)
                server.socket = context.wrap_socket(server.socket, server_side=True, do_handshake_on_connect=False)
            print(f"BlueEngine feedback intake listening on {args.host}:{args.port}", flush=True)
            server.serve_forever()
    except KeyboardInterrupt:
        return 0
    except (PublicationError, OSError, ValueError) as error:
        print(f"Feedback intake unavailable: {error}", file=sys.stderr)
        return 1
    finally:
        if publisher is not None:
            publisher.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
