#!/usr/bin/env python3
"""Serve native game collaboration beside existing account-free player reviews.

Use a dedicated clean engine clone, behind HTTPS. GitHub tokens are request-scoped;
the service never logs headers, client addresses, review text or submitted code.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import socket
import ssl
import sys
import time

try:
    from tools import community_store as community, feedback_server as feedback
except ModuleNotFoundError:
    import community_store as community
    import feedback_server as feedback

PREFIX = "/community/v1"
DEFAULT_ORIGINS = (feedback.TRUSTED_ORIGIN, "tauri://localhost", "http://tauri.localhost", "https://tauri.localhost")


class CommunityServer(feedback.FeedbackServer):
    def __init__(self, address, publisher, *, origins=DEFAULT_ORIGINS, **kwargs):
        self.origins = frozenset(origins)
        if not self.origins or any(origin in ("null", "*") or not re.fullmatch(r"(?:https?://[A-Za-z0-9.:-]+|tauri://localhost)", origin)
                                   for origin in self.origins):
            raise ValueError("Configure explicit trusted website/launcher origins")
        super().__init__(address, publisher, **kwargs)
        self.RequestHandlerClass = CommunityHandler


class CommunityHandler(feedback.FeedbackHandler):
    server_version = "BlueEngineCommunity"

    def origin_allowed(self):
        # Keep the anonymous review endpoint's existing origin contract intact.
        if not self.path.startswith(PREFIX):
            return super().origin_allowed()
        origins = self.headers.get_all("Origin", [])
        return not origins or len(origins) == 1 and origins[0] in self.server.origins

    def respond(self, status, body, *, retry=False):
        if not getattr(self, "path", "").startswith(PREFIX):
            return super().respond(status, body, retry=retry)
        content = json.dumps(body, ensure_ascii=True, allow_nan=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(content)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Connection", "close")
        origins = self.headers.get_all("Origin", [])
        if len(origins) == 1 and origins[0] in self.server.origins:
            self.send_header("Access-Control-Allow-Origin", origins[0])
            self.send_header("Vary", "Origin")
        if retry:
            self.send_header("Retry-After", "5")
        self.end_headers()
        try:
            self.wfile.write(content)
        except OSError:
            pass

    def token(self):
        values = self.headers.get_all("Authorization", [])
        if len(values) != 1 or not values[0].startswith("Bearer "):
            raise community.AccessDenied("Supply a GitHub token in Authorization: Bearer")
        return values[0][7:]

    def route(self):
        suffix = self.path[len(PREFIX):]
        if suffix == "/games":
            return "games", None
        if suffix == "/me":
            return "me", None
        if suffix == "/feedback":
            return "feedback", None
        match = re.fullmatch(r"/games/([a-z0-9]+(?:-[a-z0-9]+)*)(?:/(history|forks|revisions|publish|releases))?", suffix)
        if not match or len(match[1]) > 80:
            raise community.NotFound("Unknown community endpoint")
        return match[2] or "game", match[1]

    def do_OPTIONS(self):
        if not self.path.startswith(PREFIX):
            return super().do_OPTIONS()
        requested = self.headers.get("Access-Control-Request-Headers", "").lower().split(",")
        if (not self.origin_allowed() or self.headers.get("Access-Control-Request-Method") not in ("GET", "POST")
                or any(item.strip() not in ("", "authorization", "content-type") for item in requested)):
            return self.respond(403, {"accepted": False, "error": "Preflight is not allowed"})
        try:
            self.route()
        except community.NotFound as error:
            return self.respond(404, {"accepted": False, "error": str(error)})
        self.send_response(204)
        origin = self.headers.get("Origin")
        if origin in self.server.origins:
            self.send_header("Access-Control-Allow-Origin", origin)
        self.send_header("Access-Control-Allow-Methods", "GET, POST")
        self.send_header("Access-Control-Allow-Headers", "Authorization, Content-Type")
        self.send_header("Access-Control-Max-Age", "600")
        self.send_header("Vary", "Origin")
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_GET(self):
        if not self.path.startswith(PREFIX):
            return super().do_GET()
        if not self.origin_allowed():
            return self.respond(403, {"accepted": False, "error": "Origin is not allowed"})
        try:
            route, target = self.route()
            if route == "me":
                _, actor = self.server.publisher.actor(self.token())
                result = {"actor": actor}
            elif route in ("games", "game", "history", "releases"):
                catalog = self.server.publisher.catalog(
                    refresh=route == "games" and self.headers.get("Cache-Control", "").strip().lower() == "no-cache")
                if route == "games":
                    result = {"reviewable_games": catalog.reviewable_games,
                              "games": [{key: game[key] for key in ("slug", "title", "owner", "root_game", "parent_game", "head_revision")}
                                        for game in catalog.games.values()]}
                elif route == "game":
                    result = {"game": catalog.public_game(target)}
                elif route == "releases":
                    result = {"game": target, "releases": catalog.require_game(target)["releases"]}
                else:
                    result = {"game": target, "events": catalog.history(target)}
            else:
                raise community.NotFound("Unknown community endpoint")
            self.respond(200, {"schema_version": 1, **result})
        except Exception as error:
            self.failure(error)

    def read_payload(self):
        types = self.headers.get_all("Content-Type", [])
        if len(types) != 1 or not re.fullmatch(r"application/json(?:\s*;\s*charset=utf-8)?", types[0], re.I):
            self.respond(415, {"accepted": False, "error": "Use application/json with UTF-8 encoding"})
            return None
        lengths = self.headers.get_all("Content-Length", [])
        if self.headers.get_all("Transfer-Encoding") or len(lengths) != 1 or not re.fullmatch(r"[0-9]{1,8}", lengths[0]):
            self.respond(400, {"accepted": False, "error": "One Content-Length is required"})
            return None
        length = int(lengths[0])
        if not 0 < length <= feedback.MAX_BODY:
            self.respond(413, {"accepted": False, "error": "Community request exceeds the size limit"})
            return None
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
        payload = json.loads(body.decode("utf-8"), object_pairs_hook=feedback.unique_fields, parse_constant=feedback.reject_constant)
        if not isinstance(payload, dict):
            raise ValueError("Expected a JSON object")
        return payload

    def do_POST(self):
        if not self.path.startswith(PREFIX):
            return super().do_POST()
        if not self.origin_allowed():
            return self.respond(403, {"accepted": False, "error": "Origin is not allowed"})
        if not self.server.rate_limit.accept():
            return self.respond(429, {"accepted": False, "error": "Publication rate limit reached; retry shortly"}, retry=True)
        try:
            route, target = self.route()
            action = {"games": "create", "forks": "fork", "revisions": "revision", "publish": "publish", "releases": "release", "feedback": "feedback"}.get(route)
            if action is None:
                raise community.NotFound("Unknown community endpoint")
            token = self.token()
            payload = self.read_payload()
            if payload is None:
                return
            event = self.server.publisher.publish_community(action, target, payload, token)
            result = {"accepted": True, "request_id": event["request_id"], "event": event}
            if "game" in event:
                result["game"] = {**event["game"], "revision": event["revision"]}
            self.respond(200, result)
        except Exception as error:
            self.failure(error)

    def failure(self, error):
        if isinstance(error, community.AccessDenied):
            status, message = 403, str(error)
        elif isinstance(error, community.Conflict):
            status, message = 409, str(error)
        elif isinstance(error, community.NotFound):
            status, message = 404, str(error)
        elif isinstance(error, (TimeoutError, socket.timeout)):
            status, message = 408, "Community request timed out"
        elif isinstance(error, community.feedback.FeedbackError):
            status, message = 400, str(error)
        elif isinstance(error, (ValueError, UnicodeError, RecursionError)):
            status, message = 400, "Invalid community JSON"
        else:
            # Never expose remote API errors, local paths, authorization headers
            # or Git exception details to a public client.
            status, message = 503, "Community publication unavailable; retry with the same request ID"
        self.respond(status, {"accepted": False, "error": message}, retry=status == 503)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True, help="Dedicated clean Git clone with writable origin")
    parser.add_argument("--repository", default=community.REPOSITORY)
    parser.add_argument("--games-repository", default=community.GAMES_REPOSITORY)
    parser.add_argument("--companion-sources", action="store_true", help="Include preserved game source from public companion Git history")
    parser.add_argument("--branch", default="main")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8091)
    parser.add_argument("--origin", action="append", help="Explicit trusted browser origin; repeat to replace defaults")
    parser.add_argument("--cert", type=Path)
    parser.add_argument("--key", type=Path)
    args = parser.parse_args(argv)
    if bool(args.cert) != bool(args.key):
        parser.error("--cert and --key must be supplied together")
    if args.host not in ("127.0.0.1", "::1", "localhost") and not args.cert:
        parser.error("Public listening requires --cert and --key; otherwise bind loopback behind HTTPS")
    publisher = None
    try:
        publisher = community.CommunityPublisher(args.root, repository=args.repository, games_repository=args.games_repository,
                                                companion_sources=args.companion_sources, branch=args.branch)
        with CommunityServer((args.host, args.port), publisher, origins=args.origin or DEFAULT_ORIGINS) as server:
            if args.cert:
                context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
                context.minimum_version = ssl.TLSVersion.TLSv1_2
                context.load_cert_chain(args.cert, args.key)
                server.socket = context.wrap_socket(server.socket, server_side=True, do_handshake_on_connect=False)
            print(f"BlueEngine community intake listening on {args.host}:{args.port}", flush=True)
            server.serve_forever()
    except KeyboardInterrupt:
        return 0
    except (feedback.PublicationError, community.CommunityError, OSError, ValueError) as error:
        print(f"Community intake unavailable: {error}", file=sys.stderr)
        return 1
    finally:
        if publisher:
            publisher.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
