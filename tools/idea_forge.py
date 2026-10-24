#!/usr/bin/env python3
"""IdeaForge: generate a fresh game, have Codex build it in BlueEngine, and return engine feedback.

Only standard-library dependencies. Agent transcripts and resumable receipts stay
in .be2-work; authored games and sanitized learning records are repository content.
"""
from __future__ import annotations

import argparse
from contextlib import contextmanager
from copy import deepcopy
import csv
import hashlib
import io
import json
import os
from pathlib import Path
import re
import random
import shutil
import signal
import struct
import subprocess
import sys
import time
import unicodedata
import urllib.request
from datetime import datetime, timedelta, timezone
from uuid import uuid4
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

try:
    from tools import game_tooling, learn, player_feedback
except ModuleNotFoundError:
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
    from tools import game_tooling, learn, player_feedback

ROOT = Path(__file__).resolve().parent.parent
SLUG = r"[a-z0-9]+(?:-[a-z0-9]+)*"


class ForgeError(RuntimeError):
    """A failed gate; an idea or an agent's claim is never completion evidence."""


def object_schema(properties):
    return {"type": "object", "properties": properties, "required": list(properties),
            "additionalProperties": False}


def text_schema(limit=400):
    return {"type": "string", "minLength": 1, "maxLength": limit}


IDEA_SCHEMA = object_schema({
    "dimension": {"type": "string", "enum": ["2d", "3d"]},
    "title": text_schema(70), "slug": {"type": "string", "pattern": "^" + SLUG + "$", "maxLength": 60},
    "genre": text_schema(60), "mechanic": text_schema(1400),
    "player_actions": {"type": "array", "items": text_schema(180), "minItems": 1, "maxItems": 6},
    "rules": {"type": "array", "items": text_schema(240), "minItems": 3, "maxItems": 8},
    "win_condition": text_schema(300), "lose_condition": text_schema(300),
    "why_fun": text_schema(600), "prototype": text_schema(1200),
    "novelty_check": text_schema(1000), "playtest_risk": text_schema(500),
    "story": {"anyOf": [text_schema(800), {"type": "null"}]},
})
LEGACY_IDEA_SCHEMA = object_schema({k: v for k, v in IDEA_SCHEMA["properties"].items()
                                    if k != "dimension"})
FINDING_SCHEMA = object_schema({
    "area": {"type": "string", "enum": list(learn.AREAS)},
    "severity": {"type": "string", "enum": ["low", "medium", "high"]},
    "observed": text_schema(380), "reproduction": text_schema(800),
    "workaround": text_schema(380), "recommendation": text_schema(800),
    "keywords": {"type": "array", "items": {"type": "string", "pattern": "^[a-z][a-z0-9_-]*$"},
                 "minItems": 2, "maxItems": 12},
})
FINDINGS = {"type": "array", "items": FINDING_SCHEMA, "maxItems": 30}
BUILD_SCHEMA = object_schema({"summary": text_schema(1200), "findings": FINDINGS})
REVIEW_SCHEMA = object_schema({
    "approved": {"type": "boolean"}, "mechanic_assessment": text_schema(1200),
    "visual_assessment": text_schema(1200),
    "blockers": {"type": "array", "items": text_schema(800), "maxItems": 15},
    "findings": FINDINGS,
})


def validate(value, schema, path="$"):
    """Validate the constrained schemas locally as well as at the AI interface."""
    if "anyOf" in schema:
        for alternative in schema["anyOf"]:
            try:
                validate(value, alternative, path)
                return
            except ForgeError:
                pass
        raise ForgeError(f"Invalid structured response at {path}")
    kind = schema["type"]
    valid = {"object": isinstance(value, dict), "array": isinstance(value, list),
             "string": isinstance(value, str), "boolean": isinstance(value, bool),
             "null": value is None}[kind]
    if not valid or ("enum" in schema and value not in schema["enum"]):
        raise ForgeError(f"Invalid structured response at {path}")
    if kind == "object":
        if set(value) != set(schema["properties"]):
            raise ForgeError(f"Unexpected or missing structured fields at {path}")
        for key, child in schema["properties"].items():
            validate(value[key], child, path + "." + key)
    elif kind == "array":
        if not schema.get("minItems", 0) <= len(value) <= schema.get("maxItems", 1000):
            raise ForgeError(f"Invalid list size at {path}")
        for index, child in enumerate(value):
            validate(child, schema["items"], f"{path}[{index}]")
    elif kind == "string":
        if not schema.get("minLength", 0) <= len(value) <= schema.get("maxLength", 100000):
            raise ForgeError(f"Invalid text length at {path}")
        if schema.get("pattern") and not re.fullmatch(schema["pattern"], value):
            raise ForgeError(f"Invalid text format at {path}")


def atomic_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".partial")
    temporary.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)


def load_json(path):
    path = Path(path)
    if path.stat().st_size > 4_000_000:
        raise ForgeError("Structured response exceeds the size limit")
    return json.loads(path.read_text(encoding="utf-8"))


@contextmanager
def run_lock(directory):
    """Two supervisors must never advance or publish the same run concurrently."""
    path = Path(directory) / "supervisor.lock"
    try:
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    except FileExistsError:
        try:
            pid = int(path.read_text())
            if pid <= 0:
                raise ValueError("Invalid lock owner")
            if not process_alive(pid):
                path.unlink()
                with run_lock(directory):
                    yield
                return
        except (ValueError, OSError):
            raise ForgeError("Run lock needs inspection; another supervisor may be active") from None
        raise ForgeError("This run already has an active supervisor") from None
    try:
        with os.fdopen(fd, "w") as output:
            output.write(str(os.getpid()))
        yield
    finally:
        path.unlink(missing_ok=True)


def process_alive(pid):
    if os.name != "nt":
        try:
            os.kill(pid, 0)
            return True
        except ProcessLookupError:
            return False
    # os.kill(pid, 0) is not a portable liveness check on Windows.
    import ctypes
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    opener = kernel.OpenProcess
    opener.argtypes = [ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
    opener.restype = ctypes.c_void_p
    handle = opener(0x1000, False, pid)
    if not handle:
        return ctypes.get_last_error() != 87
    try:
        code = ctypes.c_ulong()
        getter = kernel.GetExitCodeProcess
        getter.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ulong)]
        if not getter(handle, ctypes.byref(code)):
            return True
        return code.value == 259
    finally:
        closer = kernel.CloseHandle
        closer.argtypes = [ctypes.c_void_p]
        closer(handle)


def git(root, *args):
    process = subprocess.run(["git", "-C", str(root), *args], capture_output=True, text=True)
    if process.returncode:
        raise ForgeError(f"Git operation failed: {args[0]} (exit {process.returncode})")
    return process.stdout.strip()


def git_ancestor(root, ancestor, descendant):
    result = subprocess.run(["git", "-C", str(root), "merge-base", "--is-ancestor",
                             ancestor, descendant], capture_output=True, text=True)
    if result.returncode not in (0, 1):
        raise ForgeError("Could not verify feedback integration ancestry")
    return result.returncode == 0


def github_repository(root):
    origin = git(root, "remote", "get-url", "origin")
    match = re.search(r"github\.com[:/]([\w.-]+/[\w.-]+?)(?:\.git)?$", origin)
    if not match:
        raise ForgeError("Publication requires a GitHub origin")
    return match[1]


def code_digest(root):
    """Bind verification to code/assets; feedback prose can be appended afterward."""
    paths = subprocess.check_output(["git", "-C", str(root), "ls-files", "-co",
                                     "--exclude-standard", "-z"]).decode().split("\0")
    digest = hashlib.sha256()
    for name in sorted(set(filter(None, paths))):
        if name.startswith(("docs/feedback/", "docs/learning/")):
            continue
        path = Path(root) / name
        digest.update(name.encode() + b"\0")
        if path.is_symlink():
            digest.update(os.readlink(path).encode())
        elif path.is_file():
            digest.update(path.read_bytes())
        else:
            digest.update(b"<deleted>")
    return digest.hexdigest()


def mechanic_key(idea):
    text = unicodedata.normalize("NFKC", idea["mechanic"]).casefold()
    return "".join(character for character in text if character.isalnum())


def public_findings(findings):
    for finding in findings:
        validate(finding, FINDING_SCHEMA)
        for key, value in finding.items():
            if isinstance(value, str) and learn.secret_like(value):
                raise ForgeError(f"Feedback field {key} needs a sanitized description")
    return findings


def screenshot_from_package(game, not_before=0):
    receipt = load_json(game / "dist/ship.json")
    if receipt.get("verified", {}).get("smoke") is not True:
        raise ForgeError("The packaged game has no successful isolated smoke receipt")
    captures = sorted((game / ".blue-check").rglob("shot_*.png"), key=lambda p: p.stat().st_mtime)
    for path in reversed(captures):
        if path.stat().st_mtime < not_before:
            continue
        header = path.read_bytes()[:33]
        if len(header) >= 24 and header[:8] == b"\x89PNG\r\n\x1a\n":
            width, height = struct.unpack(">II", header[16:24])
            if width >= 320 and height >= 180 and width > height:
                return path
    raise ForgeError("The isolated package needs a landscape native screenshot")


def validate_game_wiki(game, *, allow_human=False):
    """Gate authored publication facts; the independent review checks their truth."""
    try:
        wiki = load_json(game / "wiki.json")
        if not isinstance(wiki, dict) or type(wiki.get("version")) is not int or wiki["version"] != 1:
            raise ForgeError("Game wiki needs version 1")

        def text(value, field, minimum=1):
            if not isinstance(value, str) or len(value.strip()) < minimum:
                raise ForgeError(f"Game wiki needs substantive {field}")
            if learn.secret_like(value):
                raise ForgeError(f"Game wiki {field} needs a sanitized public description")

        def image(value, minimum, landscape=False):
            if not isinstance(value, str) or not re.fullmatch(r"assets/wiki/[a-zA-Z0-9_-]+\.png", value):
                raise ForgeError("Game wiki images must use game-relative assets/wiki/*.png paths")
            path = game / value
            if path.is_symlink() or not path.resolve().is_relative_to(game.resolve()):
                raise ForgeError("Game wiki screenshot escapes the game folder")
            data = path.read_bytes()
            if len(data) < 33 or data[:16] != b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR":
                raise ForgeError("Game wiki images must be PNG screenshots")
            width, height = struct.unpack(">II", data[16:24])
            if width < minimum[0] or height < minimum[1] or (landscape and width <= height):
                raise ForgeError("Game wiki screenshot is too small or has the wrong aspect ratio")
            return hashlib.sha256(data).hexdigest()

        text(wiki.get("overview"), "overview", 120)
        if wiki.get("story") is not None:
            text(wiki["story"], "implemented story")
        for field, count, minimum in (("rules", 3, 20), ("controls", 1, 8)):
            values = wiki.get(field)
            if not isinstance(values, list) or len(values) < count:
                raise ForgeError(f"Game wiki needs at least {count} {field}")
            for value in values:
                text(value, field, minimum)
        characters = wiki.get("characters", [])
        if not isinstance(characters, list):
            raise ForgeError("Game wiki needs a characters list")
        if wiki.get("characters_note"):
            text(wiki["characters_note"], "character note")
        names = set()
        for character in characters:
            if not isinstance(character, dict):
                raise ForgeError("Game wiki character must be an object")
            text(character.get("name"), "character name")
            if character["name"] in names:
                raise ForgeError("Game wiki character names must be distinct")
            names.add(character["name"])
            text(character.get("description"), "character description", 40)
            image(character.get("image"), (96, 96))
        gallery = wiki.get("gallery")
        if not isinstance(gallery, list) or len(gallery) < 3:
            raise ForgeError("Game wiki needs at least three distinct gameplay screenshots")
        digests = []
        for shot in gallery:
            if not isinstance(shot, dict):
                raise ForgeError("Game wiki gallery entry must be an object")
            text(shot.get("caption"), "screenshot caption", 8)
            text(shot.get("alt"), "screenshot alternative text", 8)
            digests.append(image(shot.get("image"), (320, 180), landscape=True))
        if len(set(digests)) != len(digests):
            raise ForgeError("Game wiki needs distinct screenshot bytes, not repeated images")
        design = wiki.get("design")
        if not isinstance(design, dict) or design.get("origin") not in ({"ai", "human"} if allow_human else {"ai"}):
            raise ForgeError("IdeaForge game wiki must declare AI design origin")
        if design["origin"] == "human":
            text(design.get("original_prompt"), "original human creation prompt")
        else:
            text(design.get("rationale"), "public design rationale explaining why this game was chosen", 120)
            if "original_prompt" not in design or design["original_prompt"] is not None:
                raise ForgeError("IdeaForge game wiki original_prompt must be null")
        return wiki
    except (OSError, ValueError, TypeError, KeyError) as error:
        raise ForgeError("Game wiki or one of its screenshot files is missing or malformed") from error


def codex_usage(path):
    """Keep only numeric turn.completed usage; never copy events/transcripts to feedback."""
    totals = {key: 0 for key in ("input_tokens", "cached_input_tokens", "output_tokens")}
    turns = 0
    for line in Path(path).read_text(encoding="utf-8", errors="replace").splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        if not isinstance(event, dict) or event.get("type") != "turn.completed":
            continue
        usage = event.get("usage")
        if not isinstance(usage, dict) or any(type(usage.get(k)) is not int or usage[k] < 0
                                               for k in ("input_tokens", "output_tokens")):
            continue
        for key in totals:
            value = usage.get(key, 0)
            if type(value) is int and value >= 0:
                totals[key] += value
        turns += 1
    return {"usage": totals if turns else None, "turns": turns,
            "usage_note": None if turns else "Codex CLI emitted no valid turn.completed usage; wall time only."}


class Forge:
    def __init__(self, directory, state):
        self.directory = Path(directory).resolve()
        self.state = state
        self.engine = self.directory / "engine"
        self.games = self.directory / "games-repository"
        self.env = os.environ.copy()
        self.env["CARGO_TARGET_DIR"] = state["target_dir"]
        self.env["RUSTC_WRAPPER"] = ""

    def save(self):
        atomic_json(self.directory / "state.json", self.state)

    def validate_wiki(self, game):
        return validate_game_wiki(game)

    def command(self, argv, cwd=None, label="command", stdin=None, timeout=None):
        """No shell interpolation; only this command's process group is stopped on timeout."""
        self.state.setdefault("commands", [])
        numbers = [int(p.name.split("-", 1)[0]) for p in self.directory.glob("*-*.log")
                   if p.name.split("-", 1)[0].isdigit()]
        number = max(numbers, default=0) + 1
        logfile = self.directory / f"{number:03d}-{label}.log"
        started = time.monotonic()
        with logfile.open("w", encoding="utf-8") as output:
            process = subprocess.Popen([str(a) for a in argv], cwd=cwd or self.engine,
                                       stdin=subprocess.PIPE if stdin is not None else subprocess.DEVNULL,
                                       stdout=output, stderr=subprocess.STDOUT, env=self.env,
                                       start_new_session=os.name != "nt")
            try:
                process.communicate(stdin.encode() if stdin is not None else None,
                                    timeout=timeout or self.state["timeout"])
            except (subprocess.TimeoutExpired, KeyboardInterrupt):
                if os.name == "nt":
                    subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                                   capture_output=True)
                else:
                    os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    if os.name != "nt":
                        os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
                self.state["commands"].append({"label": label, "exit_code": process.returncode,
                                               "interrupted": True, "log": logfile.name,
                                               "seconds": round(time.monotonic() - started, 3)})
                try:
                    self.save()
                except OSError:
                    pass
                raise ForgeError(f"{label} interrupted; private log: {logfile}") from None
        self.state["commands"].append({"label": label, "exit_code": process.returncode,
                                        "seconds": round(time.monotonic() - started, 3),
                                        "log": logfile.name})
        self.save()
        if process.returncode:
            raise ForgeError(f"{label} failed (exit {process.returncode}); private log: {logfile}")
        return logfile

    def api(self, endpoint):
        process = subprocess.run(["gh", "api", endpoint], capture_output=True, text=True)
        if process.returncode:
            raise ForgeError("GitHub API request failed; check gh authentication and repository access")
        return json.loads(process.stdout)

    def agent(self, label, prompt, schema, writable=False, image=None):
        schema_path = self.directory / (label + ".schema.json")
        answer_path = self.directory / (label + ".json")
        atomic_json(schema_path, schema)
        # Never accept a previous response after a failed new invocation.
        answer_path.unlink(missing_ok=True)
        argv = [self.state["agent"], "exec", "--sandbox", "workspace-write" if writable else "read-only",
                "-c", "approval_policy=\"never\"", "--ephemeral", "--color", "never", "--json",
                "--output-schema", str(schema_path), "--output-last-message", str(answer_path),
                "--cd", str(self.game if writable else self.engine)]
        if writable:
            self.ensure_game_scope()
            self.game.mkdir(parents=True, exist_ok=True)
            private = self.engine / ".be2-work"
            private.mkdir(exist_ok=True)
            feedback = self.engine / "docs/feedback"
            feedback.mkdir(parents=True, exist_ok=True)
            source_stamp = self.source_stamp()
            argv.extend(["--add-dir", self.state["target_dir"], "--add-dir", str(private),
                         "--add-dir", str(feedback)])
        if self.state.get("model"):
            argv.extend(["--model", self.state["model"]])
        if image:
            argv.extend(["--image", str(image)])
        argv.append("-")
        before = len(self.state.get("commands", []))
        try:
            self.command(argv, label=label, stdin=prompt)
        finally:
            completed = self.state.get("commands", [])[before:]
            for command in completed:
                measured = codex_usage(self.directory / command["log"])
                self.state.setdefault("agent_measurements", []).append({
                    "label": label, "seconds": command["seconds"], **measured})
            self.save()
        if not answer_path.exists():
            raise ForgeError(f"{label} produced no structured response")
        answer = load_json(answer_path)
        validate(answer, schema)
        if "findings" in answer:
            public_findings(answer["findings"])
        if writable:
            try:
                self.ensure_game_scope()
                if self.source_stamp() != source_stamp:
                    raise ForgeError("The worker touched engine src/; use the separate tools/engine_fix.py path")
            except ForgeError:
                if "findings" in answer:
                    self.state.setdefault("build_reports", []).append(answer)
                    self.save()
                raise
        return answer

    def setup(self):
        source = Path(self.state["source_root"])
        base = self.state["base"]
        if base == "origin/main":
            self.command(["git", "fetch", "origin", "main"], cwd=source, label="engine-fetch")
        if not self.engine.exists():
            self.command(["git", "worktree", "add", "-b", self.state["branch"], str(self.engine), base],
                         cwd=source, label="engine-worktree")
        self.state["base_commit"] = git(self.engine, "rev-parse", "HEAD")
        self.state["engine_repository"] = github_repository(source) if self.state["publish"] else None

    def player_experience(self, game=None):
        # Freeze the bounded public evidence in this run's private journal. Retries
        # retain the same inputs; a new worktree gets the latest committed reviews.
        key = game or "planning"
        snapshots = self.state.setdefault("player_feedback", {})
        if key not in snapshots:
            if (self.engine / player_feedback.DIRECTORY / "games.json").is_file():
                try:
                    snapshots[key] = player_feedback.context(self.engine, game=game)
                except (player_feedback.FeedbackError, OSError, ValueError) as error:
                    raise ForgeError("Committed player feedback is invalid; run player_feedback.py check") from error
            else:
                snapshots[key] = {"schema_version": 1, "total_reviews": 0, "reviews": []}
            atomic_json(self.directory / "player-feedback.json", snapshots)
            self.save()
        return player_feedback.prompt_context(snapshots[key])

    def generate(self):
        prior = []
        catalog = self.engine / "games/idea-forge/assets/ideas.json"
        if catalog.exists():
            prior.extend({"title": i["title"], "mechanic": i["mechanic"]} for i in load_json(catalog))
        for path in self.directory.parent.glob("*/idea.json"):
            if path.parent != self.directory:
                idea = load_json(path)
                prior.append({"title": idea["title"], "mechanic": idea["mechanic"]})
        known = [p.name for p in (self.engine / "games").iterdir() if p.is_dir()]
        prompt = f"""Generate one FRESH game concept centered on an unusual, engaging PLAYABLE MECHANIC.
This is an automatic game-building pipeline, not a list picker. Brainstorm several distinct
mechanical rules internally and select the most promising one that can become a polished
small offline BlueEngine native game in one build session. Prefer an elegant single rule
with surprising interactions, clear feedback and several escalating challenges. Cosmetic
reskins or another ordinary platformer/shooter with a narrative change are insufficient.
Describe exact inputs, rules, win/loss conditions, why decisions are interesting, and a
small prototype that tests the fun. Story is {'optional' if self.state['story'] else 'not requested; return null'}.
The requested direction is: {json.dumps(self.state['brief'])}.
Required dimensionality: {self.state.get('dimension', 'any')}. Return dimension as 2d or 3d.
For 2d, the playable space and presentation are two-dimensional. For 3d, the game
must have a real rendered 3D world with depth that matters to the central mechanic;
a flat puzzle with a tilted camera does not meet this requirement.
Read AGENTS.md and docs/PORTABLE_GAMES.md for what BlueEngine actually supports.
Compare the core rule to existing games and the prior mechanics below. Explain the closest
comparison and the concrete difference in novelty_check; worldwide originality and fun
cannot be certified. Do not reuse a previous mechanic or an existing slug. Do not edit files.
Existing game slugs: {json.dumps(known)}.
Prior mechanics: {json.dumps(prior)}.
{self.player_experience()}
Return only the schema-constrained concept. No credentials or environment identifiers."""
        if self.state.get("idea_input"):
            idea = load_json(self.state["idea_input"])
            validate(idea, IDEA_SCHEMA if "dimension" in idea else LEGACY_IDEA_SCHEMA)
        else:
            idea = self.agent("concept-candidate", prompt, IDEA_SCHEMA)
        required = self.state.get("dimension", "any")
        if required != "any" and idea.get("dimension") != required:
            raise ForgeError("The concept does not meet the requested dimension")
        if (self.engine / "games" / idea["slug"]).exists():
            raise ForgeError("The generated slug already exists; existing games were preserved")
        # Re-read completed concepts: another generation may have finished while
        # this model was thinking. Keep ideas from failed builds too.
        fresh_prior = [load_json(p) for p in self.directory.parent.glob("*/idea.json")
                       if p.parent != self.directory and str(p.resolve()) != self.state.get("idea_input")]
        compared = fresh_prior if self.state.get("idea_input") else prior + fresh_prior
        if mechanic_key(idea) in {mechanic_key(p) for p in compared}:
            raise ForgeError("The agent repeated a previous mechanic")
        if not self.state["story"] and not self.state.get("idea_input") and idea["story"] is not None:
            raise ForgeError("The concept included an unrequested story")
        self.state["idea"] = idea
        atomic_json(self.directory / "idea.json", idea)

    def build(self):
        threshold = self.state.get("min_free_gib", 8) * 1024**3
        if shutil.disk_usage(self.state["target_dir"]).free < threshold:
            raise ForgeError("Insufficient free space for native compilation; choose a larger target disk")
        idea = self.state["idea"]
        problem = self.state.get("rebuild_reason", "")
        for attempt in range(self.state["repairs"] + 1):
            prompt = f"""Build the actual game described below in this BlueEngine checkout.
Read root AGENTS.md, use tools/be2.py start and its selected documentation, and use the
fresh canonical map scaffold/icon tooling. The game lives at games/{idea['slug']}.
Implement the distinctive mechanic, real player input, a clear goal and loss/retry loop,
several escalating challenges, instructions, polished native visuals and its own icon.
Mandatory user direction and quality requirements: {json.dumps(self.state['brief'])}.
{self.player_experience(idea['slug'])}
Author a substantive mini-wiki at games/{idea['slug']}/wiki.json, alongside the game:
version: 1; overview: at least 120 characters explaining what players actually do;
rules: at least three concrete rules (20+ characters each), including goals, failure
and progression; controls: one or more exact input instructions (8+ characters each);
story and characters are optional publication facts. Omit them for storyless or
characterless games: no absence essay, story file or character sheet is required.
If implemented, document the actual story and named characters with name,
description and a genuine in-game image. Do not invent lore/assets to satisfy a gate.
gallery: at least THREE distinct real gameplay screenshots, each with image, caption
and alt (8+ characters each). All image paths must be game-relative assets/wiki/NAME.png.
Gallery images must be landscape PNGs >=320x180, showing genuinely different moments,
controls or challenges. Include an actual in-game screenshot of EACH named character
(>=96x96; crops of genuine captures are okay); icons or concept art do not count.
Use real native captures of the implemented game, never generated artwork, painted
mockups, duplicated images or the same image with tiny edits. Inspect every image.
design: origin 'ai', original_prompt null, rationale (120+ characters) explaining WHY
this particular game was chosen: the interesting player decisions, intended experience
and how it differs from prior games. Write a concise public design decision summary
grounded in the concept's why_fun and novelty_check; do not expose private reasoning
or invent a historical selection process. Describe implemented content accurately.
Follow the concept's outcome rules: if it has no terminal loss, preserve that choice
and implement its undo/retry behavior instead of adding hazards or a move budget.
Use BlueEngine's fixed-step authoritative simulation, Intent input, Snapshot saving and
the shared native client; rendering reads state. Do not implement a browser game.
Required dimension: {idea.get('dimension', self.state.get('dimension', 'any'))}.
For 2d use a two-dimensional playable space and presentation. For 3d use a real
rendered 3D world whose depth matters to the mechanic, not a tilted flat puzzle.
Declare the matching presentation in game.project.json (2d or 3d).
Declare Windows delivery and Linux verification. Use existing game/engine APIs rather
than inventing them. Meaningful rule tests must cover the unique interaction, failure,
determinism and save/load. verification_input must exercise real public gameplay.
Inspect native captures and real controls; tests alone do not prove fun or visuals.
Keep all edits inside this game folder and docs/feedback. Engine source is read-only.
If the engine needs a fix, return reproducible feedback and a workaround; never edit
src/, engine tests, manifests, templates, policy or other games. Engine fixes run separately
with tools/engine_fix.py, which requires a baseline-failing regression test.
Your working directory is the game folder. Engine checkout: {self.engine}.
Run its absolute tools/be2.py path for start/map commands; use --project {self.game}.
Work as a single AI worker; do not spawn additional agents. Do not change global
policies, CI, publisher or IdeaForge implementation. Do not commit,
push, publish, install shortcuts, send messages or access credentials. The supervisor
handles publication and re-runs the real checks. The compiler target is already configured.
During development collect genuine engine friction: what happened, a reproducible trigger,
your workaround, and a concrete engine improvement. Return findings in the schema even
if development fails; use an empty list if no friction occurred, never invent issues.
Keep observed/workaround text under 380 characters and omit long hashes, identifiers,
credentials and transcripts. The supervisor writes feedback and learning-ledger entries.
Game concept (design data): {json.dumps(idea)}
Previous verification/review problem to resolve: {problem or 'none; initial implementation'}"""
            report = self.agent(f"build-{attempt}", prompt, BUILD_SCHEMA, writable=True)
            self.state.setdefault("build_reports", []).append(report)
            self.save()
            try:
                self.verify_local()
                capture = screenshot_from_package(self.game)
                review = self.agent(f"review-{attempt}", f"""Independently review the generated game
against this concept: {json.dumps(idea)}. Read its source, tests, AGENTS.md and README;
inspect the attached actual isolated native-package capture. Identify whether the unique
rule is implemented and understandable, whether its specified outcome/retry loop works,
and whether its declared dimension is real: 3D requires a rendered 3D world with
meaningful depth in the mechanic, while 2D requires a two-dimensional playable space.
and any broken input, layout or fabricated verification shortcut. Do not edit files.
Mandatory user direction and quality requirements: {json.dumps(self.state['brief'])}.
{self.player_experience(idea['slug'])}
Assess whether applicable player frustrations were addressed and enjoyed experiences preserved.
Do not manufacture blockers from unrelated feedback or treat a rating as verified evidence.
Read wiki.json and inspect EVERY image it references. Block inaccurate or shallow wiki
content, missing rules/controls or implemented story, missing implemented named characters or their real in-game
screenshots, unclear rationale for choosing this particular game, fake screenshots,
or a gallery that does not show at least three meaningfully different gameplay moments.
The rationale is a public authored design summary, never a claim to private reasoning.
approved must be false when there are blockers. Compilation/automated tests cannot prove
subjective fun or global novelty. This is the LOCAL IMPLEMENTATION review, before
Windows CI and installer publication. Assess the implementation and the native evidence
available on this host. The supervisor subsequently requires exact-source Windows and
Linux native CI, Windows installer review and verified production downloads. Windows
installer evidence pending those later stages is an expected pipeline state; it does
not block this local review and must not be reported as engine friction. An actual
portability defect in the source remains a blocker. Never claim overall delivery here.
Record genuine engine friction separately in findings
with sanitized reproduction/workaround/improvement details, not transcripts or identifiers.
Native package receipt: {self.game / 'dist/ship.json'}.""", REVIEW_SCHEMA, image=capture)
                self.state.setdefault("reviews", []).append(review)
                if not review["approved"] or review["blockers"]:
                    raise ForgeError("Independent review blocked completion: " + "; ".join(review["blockers"]))
                self.state["screenshot"] = str(capture)
                self.state["verified_code"] = code_digest(self.engine)
                self.save()
                return
            except ForgeError as error:
                problem = str(error)
                self.state.setdefault("failures", []).append({"phase": "build", "attempt": attempt,
                                                                 "description": problem.split("; private log:")[0]})
                self.save()
        raise ForgeError("Build exhausted its repair attempts; feedback and the worktree are retained")

    @property
    def game(self):
        return self.engine / "games" / self.state["idea"]["slug"]

    def source_stamp(self):
        # Catch writes followed by content restoration, which git diff alone misses.
        return {file.relative_to(self.engine).as_posix():
                (file.stat().st_mtime_ns, hashlib.sha256(file.read_bytes()).hexdigest())
                for file in (self.engine / "src").rglob("*") if file.is_file()}

    def ensure_game_scope(self, supervisor=False):
        base = self.state.get("base_commit", "HEAD")
        changed = (git(self.engine, "diff", "--name-only", base).splitlines()
                   + git(self.engine, "ls-files", "--others", "--exclude-standard").splitlines())
        allowed = (f"games/{self.state['idea']['slug']}/", "docs/feedback/")
        if supervisor:
            allowed += ("docs/learning/",)
        def own_receipt(path):
            expected = self.state.get("supervisor_files", {}).get(path)
            full = self.engine / path
            return expected and full.is_file() and hashlib.sha256(full.read_bytes()).hexdigest() == expected
        if any(not p.startswith(allowed) and not own_receipt(p) and not (supervisor and p == "games-publish.json")
               for p in changed):
            raise ForgeError("The worker changed files outside its game scope; engine changes require "
                             "a separate tools/engine_fix.py run with a regression test")
        if self.game.is_symlink() or not self.game.resolve().is_relative_to(self.engine):
            raise ForgeError("The generated game directory escapes its engine worktree")

    def measurements(self):
        agents = self.state.get("agent_measurements", [])
        known = [agent["usage"] for agent in agents if agent.get("usage") is not None]
        usage = {key: sum(row.get(key, 0) for row in known)
                 for key in ("input_tokens", "cached_input_tokens", "output_tokens")}
        # Cached input is already part of input_tokens; do not count it twice.
        started = datetime.fromisoformat(self.state["created"]).timestamp()
        return {"run": self.state["id"], "game": self.state["idea"]["slug"],
                "tokens": usage["input_tokens"] + usage["output_tokens"],
                "usage": usage if known else None,
                "wall_seconds": round(max(0., time.time() - started), 3),
                "agent_seconds": round(sum(agent["seconds"] for agent in agents), 3),
                "usage_note": "Measured from Codex CLI turn.completed usage; cached input included once."
                    if agents and len(known) == len(agents) else
                    "Partial/unavailable CLI usage; tokens sum only exposed turns, wall time recorded."}

    def verify_local(self):
        self.ensure_game_scope()
        game = self.game
        if game.is_symlink() or not game.resolve().is_relative_to(self.engine):
            raise ForgeError("The generated game directory escapes its engine worktree")
        for name in ("Cargo.toml", "game.project.json", "scripts/ship.py", "src/lib.rs", "src/main.rs"):
            if not (game / name).is_file():
                raise ForgeError(f"Generated game is missing {name}")
        try:
            game_tooling.check(game, self.engine)
        except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
            raise ForgeError(
                f"Native project tooling must use the canonical pinned launchers: {error}. "
                "Review the tooling update and run python ENGINE/tools/game_tooling.py migrate GAME"
            ) from error
        project = load_json(game / "game.project.json")
        dimension = self.state["idea"].get("dimension", self.state.get("dimension", "any"))
        if dimension != "any" and project.get("presentation") != dimension:
            raise ForgeError("The game presentation does not meet the requested dimension")
        if not {"windows", "linux"}.issubset(project.get("targets", [])) or "web" in project.get("targets", []):
            raise ForgeError("The game must declare native Windows delivery and Linux verification")
        self.validate_wiki(game)
        self.ensure_game_scope()
        manifest = game / "Cargo.toml"
        self.command(["cargo", "fmt", "--manifest-path", manifest, "--check"], label="game-format")
        log = self.command(["cargo", "test", "--locked", "--manifest-path", manifest], label="game-tests")
        counts = re.findall(r"test result: ok\. (\d+) passed", log.read_text(errors="replace"))
        if not counts or sum(map(int, counts)) == 0:
            raise ForgeError("The game has no executed behavioral tests")
        self.command(["cargo", "clippy", "--locked", "--all-targets", "--manifest-path", manifest,
                      "--", "-D", "warnings"], label="game-clippy")
        ship = [sys.executable, str(game / "scripts/ship.py"), "ship", "--no-install"]
        if sys.platform.startswith("linux") and not os.environ.get("DISPLAY"):
            if not shutil.which("xvfb-run"):
                raise ForgeError("Linux package smoke needs DISPLAY or xvfb-run")
            ship = ["xvfb-run", "-a", *ship]
            self.env["LIBGL_ALWAYS_SOFTWARE"] = "1"
        smoke_started = time.time() - 1
        self.command(ship, label="native-package")
        screenshot_from_package(game, not_before=smoke_started)

    def feedback(self):
        idea = self.state.get("idea")
        if not idea:
            return
        reports = self.state.get("build_reports", []) + self.state.get("reviews", [])
        findings = public_findings([f for report in reports for f in report["findings"]])
        unique = list({json.dumps(f, sort_keys=True): f for f in findings}.values())
        relative = f"docs/feedback/{self.state['created'][:10]}-{idea['slug']}-{self.state['id']}.md"
        self.state["feedback_path"] = relative
        lines = [f"# {idea['title']}: AI development feedback", "", f"Run: `{self.state['id']}`.",
                 f"Project: `games/{idea['slug']}`. State: {self.state['status']}.", "",
                 "## Generated mechanic", "", idea["mechanic"], "", "## Novelty and playtesting", "",
                 idea["novelty_check"], "", idea["playtest_risk"], "",
                 "## Findings from the AI that developed and reviewed the game", ""]
        measured = self.measurements()
        cost_file = self.engine / "docs/learning/forge-runs.jsonl"
        cost_file.parent.mkdir(parents=True, exist_ok=True)
        costs = [json.loads(line) for line in cost_file.read_text().splitlines()] if cost_file.exists() else []
        costs = [row for row in costs if row["run"] != measured["run"]] + [measured]
        cost_file.write_text("".join(json.dumps(row) + "\n" for row in costs), encoding="utf-8")
        lines.extend(["## Measured game-run cost", "",
                      f"Tokens: {measured['tokens']}; wall seconds: {measured['wall_seconds']}; "
                      f"Codex seconds: {measured['agent_seconds']}. {measured['usage_note']}", ""])
        ledger = self.engine / "docs/learning/ledger.jsonl"
        entries, malformed = learn.load_ledger(ledger)
        if malformed:
            raise ForgeError("The engine learning ledger is malformed; preserved without modification")
        for index, finding in enumerate(unique, 1):
            lines.extend([f"### {index}. {finding['area']} ({finding['severity']})", "",
                          "Observed: " + finding["observed"], "", "Reproduce: " + finding["reproduction"], "",
                          "Workaround: " + finding["workaround"], "",
                          "Proposed engine improvement: " + finding["recommendation"], ""])
            entry = {"game": idea["slug"], "area": finding["area"], "tokens": measured["tokens"] if index == 1 else 0,
                     "wall_seconds": measured["wall_seconds"] if index == 1 else 0,
                     "measurement_ref": "docs/learning/forge-runs.jsonl",
                     "note": finding["observed"], "workaround": finding["workaround"],
                     "status": "open", "ref": relative, "keywords": finding["keywords"]}
            existing = next((e for e in entries if e.get("game") == entry["game"]
                             and e.get("note") == entry["note"] and e.get("ref") == relative), None)
            if existing is None:
                entries.append(learn.append_entry(entry, path=ledger))
            else:
                existing.update({key: entry[key] for key in ("tokens", "wall_seconds", "measurement_ref")})
        if unique:
            if any(learn.validate_entry(entry) for entry in entries):
                raise ForgeError("The measured learning ledger is invalid; preserved without replacement")
            temporary = ledger.with_name(".forge-ledger-" + uuid4().hex)
            temporary.write_text("".join(json.dumps(entry) + "\n" for entry in entries), encoding="utf-8")
            os.replace(temporary, ledger)
        if not unique:
            lines.extend(["The worker reported no engine friction. No findings were fabricated.", ""])
        lines.extend(["## Verification and delivery", "",
                      "Agent claims are separate from supervisor-executed checks. Virtual displays and",
                      "software rendering do not certify physical devices or audible hardware output.", ""])
        for command in self.state.get("commands", []):
            if command["label"] in ("game-format", "game-tests", "game-clippy", "native-package"):
                lines.append(f"- {command['label']}: exit {command['exit_code']}, {command['seconds']} seconds.")
        for label, receipt in self.state.get("ci", {}).items():
            lines.append(f"- {label}: {receipt['url']} (exact source `{receipt['head'][:12]}`).")
        if self.state.get("publication"):
            receipt = self.state["publication"]
            lines.extend(["", f"Published game: {receipt['page']}", f"Immutable release: {receipt['release']}",
                          f"Installer SHA-256: `{receipt['installer_sha256']}`.",
                          f"ZIP SHA-256: `{receipt['zip_sha256']}`."])
        if self.state.get("failures"):
            lines.extend(["", "## Supervisor-observed failed attempts", ""])
            for failure in self.state["failures"]:
                # Never include terminal transcripts or model/session events.
                description = failure["description"]
                if learn.secret_like(description):
                    description = "Gate failed; detailed evidence remains in the private run directory."
                lines.append(f"- {failure['phase']}: {description}")
        lines.extend(["", "Detailed command logs, AI responses and package captures stay in the ignored",
                      "run directory. This file contains authored findings and checked delivery receipts.", ""])
        path = self.engine / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("\n".join(lines), encoding="utf-8")
        self.state.setdefault("supervisor_files", {}).update({
            file.relative_to(self.engine).as_posix(): hashlib.sha256(file.read_bytes()).hexdigest()
            for file in (ledger, cost_file) if file.is_file()})
        self.save()

    def register_source(self):
        self.ensure_game_scope(supervisor=True)
        if code_digest(self.engine) != self.state["verified_code"]:
            raise ForgeError("Code changed after verification; rebuild before publishing")
        path = self.engine / "games-publish.json"
        manifest = load_json(path)
        entries = manifest["collections"]["games"]
        source = "games/" + self.state["idea"]["slug"]
        if not any(e["source"] == source for e in entries):
            entries.append({"source": source, "destination": self.state["idea"]["slug"]})
            atomic_json(path, manifest)
        # A later stage may fail or request a rebuild. On resume this exact
        # supervisor-authored registration remains allowed, never arbitrary edits.
        self.state.setdefault("supervisor_files", {})["games-publish.json"] = hashlib.sha256(path.read_bytes()).hexdigest()
        self.save()
        self.command([sys.executable, "scripts/publish_games.py", "check"], label="source-publication-check")
        git(self.engine, "add", "--all")
        self.commit(self.engine, "Build " + self.state["idea"]["title"] + " with IdeaForge and record AI feedback")
        self.state["engine_head"] = git(self.engine, "rev-parse", "HEAD")
        self.state["verified_code"] = code_digest(self.engine)

    def repair_source_registration_receipt(self):
        """Explicit legacy recovery: admit only base manifest plus this game."""
        slug = self.state["idea"]["slug"]
        if not re.fullmatch(SLUG, slug) or not self.state.get("base_commit"):
            raise ForgeError("Source receipt repair requires a valid game and recorded base")
        path = self.engine / "games-publish.json"
        if path.is_symlink() or not path.is_file():
            raise ForgeError("Source receipt repair requires a regular publication manifest")
        expected = json.loads(git(self.engine, "show", self.state["base_commit"] + ":games-publish.json"))
        entries = expected["collections"]["games"]
        own = {"source": "games/" + slug, "destination": slug}
        if own not in entries:
            entries.append(own)
        if load_json(path) != expected:
            raise ForgeError("Source manifest differs from the base plus this game's exact registration; preserved for review")
        original = dict(self.state["supervisor_files"]) if "supervisor_files" in self.state else None
        self.state.setdefault("supervisor_files", {})["games-publish.json"] = hashlib.sha256(path.read_bytes()).hexdigest()
        try:
            # Other supervisor files still need their prior exact receipts. This
            # repair cannot bless unrelated source or learning-ledger changes.
            self.ensure_game_scope()
        except (ForgeError, OSError):
            if original is None:
                self.state.pop("supervisor_files", None)
            else:
                self.state["supervisor_files"] = original
            raise
        self.state.setdefault("receipt_repairs", []).append({"kind": "source-registration-receipt",
            "base": self.state["base_commit"], "game": slug,
            "sha256": self.state["supervisor_files"]["games-publish.json"],
            "at": datetime.now(timezone.utc).isoformat()})
        self.save()

    def commit(self, root, message):
        if git(root, "diff", "--cached", "--name-only"):
            self.command(["git", "-c", "user.name=IdeaForge", "-c",
                          "user.email=idea-forge@users.noreply.github.com", "commit", "-m", message],
                         cwd=root, label="commit")

    def wait_ci(self, repository, workflow, branch, head, label, dispatch=False, fields=None, after=None):
        if dispatch and label not in self.state.setdefault("dispatched", []):
            argv = ["gh", "workflow", "run", workflow, "--repo", repository, "--ref", branch]
            for key, value in (fields or {}).items():
                argv.extend(["-f", f"{key}={value}"])
            self.command(argv, label="dispatch-" + label)
            self.state["dispatched"].append(label)
            self.save()
        deadline = time.monotonic() + self.state["wait_timeout"]
        while time.monotonic() < deadline:
            result = self.api(f"repos/{repository}/actions/workflows/{workflow}/runs?branch={branch}&per_page=20")
            matching = [r for r in result["workflow_runs"] if r["head_sha"] == head
                        and (not dispatch or r["event"] == "workflow_dispatch")
                        and (not after or r["created_at"] >= after)]
            if matching:
                run = matching[0]
                if run["status"] == "completed":
                    if run["conclusion"] != "success":
                        raise ForgeError(f"{label} did not pass: {run['html_url']}")
                    receipt = {"run": run["id"], "head": head, "url": run["html_url"],
                               "updated_at": run["updated_at"]}
                    self.state.setdefault("ci", {})[label] = receipt
                    self.save()
                    return receipt
            time.sleep(10)
        raise ForgeError(f"Timed out waiting for {label}; resume retains completed work")

    def engine_ci(self):
        repository = self.state["engine_repository"]
        self.command(["git", "push", "-u", "origin", "HEAD"], label="push-engine-review")
        head = self.state["engine_head"]
        # The normal engine workflow already runs from the push. Start the native
        # game lane immediately so both independent workflows overlap.
        self.wait_ci(repository, "forge-game-checks.yml", self.state["branch"], head,
                     "generated-game-linux-windows", dispatch=True, fields={"game_slug": self.state["idea"]["slug"]})
        self.wait_ci(repository, "ci.yml", self.state["branch"], head, "engine-checks")

    def update_native_definition(self, manifest, entry):
        entries = manifest["native_playables"]
        existing = next((item for item in entries if item["slug"] == entry["slug"]), None)
        if existing and existing != entry:
            raise ForgeError("An independently maintained download definition uses this slug")
        if existing:
            return False
        entries.append(entry)
        return True

    def assert_games_worktree(self):
        """Only this run's private branch in the configured repository is writable."""
        source = Path(self.state["games_root"])
        repository = github_repository(source)
        if self.state.get("games_repository", repository) != repository:
            raise ForgeError("Companion repository differs from the retained run")
        if self.games.is_symlink():
            raise ForgeError("Companion worktree must not be a symlink")
        common = lambda root: Path(git(root, "rev-parse", "--path-format=absolute", "--git-common-dir")).resolve()
        if (self.games.resolve() == source.resolve()
                or Path(git(self.games, "rev-parse", "--show-toplevel")).resolve() != self.games.resolve()
                or git(self.games, "branch", "--show-current") != self.state["branch"]
                or common(self.games) != common(source)
                or github_repository(self.games) != repository):
            raise ForgeError("Companion worktree, branch or repository does not match the retained run")

    def assert_games_merge_finished(self):
        merge = Path(git(self.games, "rev-parse", "--git-path", "MERGE_HEAD"))
        if not merge.is_absolute():
            merge = self.games / merge
        if merge.exists() or git(self.games, "ls-files", "-u"):
            raise ForgeError(f"Companion integration needs review in {self.games}; resolve conflicts and commit "
                             "the merge there, then resume. No source or receipts were discarded.")

    def games_changes(self):
        staged = set(filter(None, git(self.games, "diff", "--cached", "--name-only", "--no-renames", "-z").split("\0")))
        working = set(filter(None, git(self.games, "diff", "--name-only", "--no-renames", "-z").split("\0")))
        if staged & working:
            raise ForgeError(f"Companion index and worktree both edit the same paths in {self.games}; "
                             "reconcile them explicitly before resuming. Both versions were preserved.")
        return staged | working | set(filter(None, git(self.games, "ls-files", "--others", "--exclude-standard", "-z").split("\0")))

    def stage_games_changes(self, changed):
        # An export can exceed Windows' command-line limit. NUL-delimited literal
        # pathspecs also retain unusual filenames without shell/glob interpretation.
        self.command(["git", "add", "--all", "--pathspec-from-file=-", "--pathspec-file-nul"],
                     cwd=self.games, label="stage-games-export",
                     stdin="".join(":(literal)" + name + "\0" for name in sorted(changed)))

    def integrate_games(self):
        """Checkpoint a retained export, then merge main without changing engine evidence."""
        source = Path(self.state["games_root"])
        repository = github_repository(source)
        if self.state.get("games_repository", repository) != repository:
            raise ForgeError("Companion repository differs from the retained run")
        if self.games.is_symlink():
            raise ForgeError("Companion worktree must not be a symlink")
        if not self.games.exists():
            self.command(["git", "fetch", "origin", "main"], cwd=source, label="games-fetch")
            self.command(["git", "worktree", "add", "-b", self.state["branch"], str(self.games), "origin/main"],
                         cwd=source, label="games-worktree")
        self.assert_games_worktree()
        self.assert_games_merge_finished()
        slug = self.state["idea"]["slug"]
        if not re.fullmatch(SLUG, slug):
            raise ForgeError("Invalid retained game slug")
        owned = ["games/" + slug, ".games-catalog.json", ".release-games.json", "site/thumbs/" + slug + ".png"]
        changed = self.games_changes()
        if any(not any(name == path or name.startswith(path + "/") for path in owned) for name in changed):
            raise ForgeError(f"Unrelated companion edits in {self.games}; preserve and commit or move them "
                             "explicitly before resuming. Nothing was staged or merged.")
        self.state["games_repository"] = repository
        pending = self.state.setdefault("games_export_integration", {"source_head": git(self.games, "rev-parse", "HEAD")})
        if not git_ancestor(self.games, pending["source_head"], "HEAD"):
            raise ForgeError("Companion integration lost its retained export checkpoint")
        self.save()  # Journal ownership before checkpoint/merge; both operations can be interrupted.
        if changed:
            self.stage_games_changes(changed)
            self.commit(self.games, "Checkpoint retained " + slug + " export before companion integration")
            pending["source_head"] = git(self.games, "rev-parse", "HEAD")
            self.save()
        self.command(["git", "fetch", "origin", "main"], cwd=self.games, label="games-integration-fetch")
        head = git(self.games, "rev-parse", "HEAD")
        if not git_ancestor(self.games, "origin/main", head):
            try:
                self.command(["git", "-c", "user.name=IdeaForge", "-c",
                              "user.email=idea-forge@users.noreply.github.com", "merge", "--no-edit", "origin/main"],
                             cwd=self.games, label="games-merge-main")
            except ForgeError as error:
                raise ForgeError(f"Companion integration needs review in {self.games}; resolve any conflicts "
                                 "and commit the merge there, then resume. The checkpoint is retained.") from error
        self.state.pop("games_export_integration", None)
        self.save()

    def export(self):
        wiki = self.validate_wiki(self.game)
        self.integrate_games()
        catalog = load_json(self.games / ".games-catalog.json")
        previously_owned = {item["path"] for item in catalog["files"]}
        exported = catalog.get("source_revision")
        if (catalog.get("source_repository") != self.state["engine_repository"]
                or not isinstance(exported, str) or not re.fullmatch(r"[0-9a-f]{40}", exported)):
            raise ForgeError("Existing catalog lacks a trusted engine source pin; retain it for inspection")
        self.command(["git", "fetch", "origin", exported], label="catalog-source-fetch")
        if not git_ancestor(self.engine, exported, self.state["engine_head"]):
            raise ForgeError("Catalog contains newer or divergent engine source; integrate current main and reverify the retained game before exporting")
        self.command([sys.executable, "scripts/publish_games.py", "export", "--output", self.games,
                      "--revision", self.state["engine_head"]], label="export-game")
        slug = self.state["idea"]["slug"]
        exported_game = self.games / "games" / slug
        self.validate_wiki(exported_game)
        wiki_files = {"wiki.json", *(shot["image"] for shot in wiki["gallery"]),
                      *(character["image"] for character in wiki.get("characters", []))}
        if any((self.game / path).read_bytes() != (exported_game / path).read_bytes()
               for path in wiki_files):
            raise ForgeError("Export did not preserve the verified game wiki and screenshots")
        cargo = (self.game / "Cargo.toml").read_text()
        # Generated games may have helper binaries; release exactly the default playable.
        import tomllib
        package = tomllib.loads(cargo)
        binary = package["package"].get("default-run", package["package"]["name"])
        if binary not in [b["name"] for b in package.get("bin", [])] and package.get("bin"):
            raise ForgeError("The generated Cargo package has no unambiguous playable binary")
        path = self.games / ".release-games.json"
        manifest = load_json(path)
        entry = {"slug": slug, "name": self.state["idea"]["title"], "directory": "games/" + slug,
                 "kind": "cargo-package", "binary": binary}
        if self.update_native_definition(manifest, entry):
            atomic_json(path, manifest)
        shutil.copyfile(self.state["screenshot"], self.games / "site/thumbs" / (slug + ".png"))
        self.command([sys.executable, "distribution/check_catalog.py"], cwd=self.games, label="catalog-check")
        self.command([sys.executable, "-m", "unittest", "discover", "-s", "distribution", "-p", "test_*.py"],
                     cwd=self.games, label="distribution-tests")
        self.command([sys.executable, "-m", "unittest", "discover", "-s", "site", "-p", "test_*.py"],
                     cwd=self.games, label="site-tests")
        generated = {item["path"] for item in load_json(self.games / ".games-catalog.json")["files"]}
        generated |= previously_owned | {".games-catalog.json", ".release-games.json", "site/thumbs/" + slug + ".png"}
        changed = self.games_changes()
        if changed - generated:
            raise ForgeError(f"Unrelated companion edits appeared during export in {self.games}; retained without committing them")
        if changed:
            self.stage_games_changes(changed)
        self.commit(self.games, "Publish " + self.state["idea"]["title"] + " generated by IdeaForge")
        self.record_games_candidate(git(self.games, "rev-parse", "HEAD"))
        self.state["games_source_snapshot"] = self.games_snapshot(self.state["games_head"])
        self.state.pop("games_integration_pending", None)
        self.save()

    def record_games_candidate(self, head):
        """Candidate bytes changed: only companion/package evidence becomes stale."""
        recorded = self.state.get("games_head")
        if head != recorded:
            if self.state.get("publication"):
                raise ForgeError("A published candidate is frozen; create a new run for another release")
            if recorded:
                self.state.setdefault("games_verification_history", []).append({
                    "head": recorded, "installer": deepcopy(self.state.get("ci", {}).get("windows-installer-review"))})
            for label in ("windows-installer-review", "production-release", "download-page"):
                self.state.setdefault("ci", {}).pop(label, None)
            self.state["dispatched"] = [label for label in self.state.get("dispatched", [])
                                        if label != "windows-installer-review"]
            invalidated = {"installer-review", "publish-games", "verify-publication", "installer", "production", "public_receipt"}
            self.state["completed"] = [name for name in self.state.get("completed", []) if name not in invalidated]
            self.state["games_head"] = head
            self.save()

    def installer_review(self):
        self.assert_games_worktree()
        self.assert_games_merge_finished()
        if git(self.games, "rev-parse", "HEAD") != self.state["games_head"] or git(self.games, "status", "--porcelain"):
            raise ForgeError("Companion source changed before installer review; re-export or resolve the retained integration")
        self.command(["git", "push", "-u", "origin", "HEAD"], cwd=self.games, label="push-installer-review")
        self.wait_ci(self.state["games_repository"], "releases.yml", self.state["branch"],
                     self.state["games_head"], "windows-installer-review", dispatch=True,
                     fields={"game_slug": self.state["idea"]["slug"]})

    def assert_source_gates(self):
        if git(self.engine, "rev-parse", "HEAD") != self.state["engine_head"]:
            raise ForgeError("Engine commit changed after CI; old gates cannot authorize publication")
        if code_digest(self.engine) != self.state["verified_code"]:
            raise ForgeError("Code changed after verification; old gates cannot authorize publication")
        for label in ("engine-checks", "generated-game-linux-windows", "windows-installer-review"):
            receipt = self.state.get("ci", {}).get(label)
            expected = self.state["games_head"] if label == "windows-installer-review" else self.state["engine_head"]
            if not receipt or receipt["head"] != expected:
                raise ForgeError(f"Missing exact-source gate: {label}")
            repository = self.state["games_repository"] if label == "windows-installer-review" else self.state["engine_repository"]
            run = self.api(f"repos/{repository}/actions/runs/{receipt['run']}")
            if run["head_sha"] != expected or run["status"] != "completed" or run["conclusion"] != "success":
                raise ForgeError(f"The {label} gate does not certify this source")

    def assert_engine_gates(self):
        """Games metadata integration never authorizes different engine code."""
        if (git(self.engine, "rev-parse", "HEAD") != self.state["engine_head"]
                or code_digest(self.engine) != self.state["verified_code"]):
            raise ForgeError("Engine source changed after verification; retain it for fresh source review")
        for label in ("engine-checks", "generated-game-linux-windows"):
            receipt = self.state.get("ci", {}).get(label, {})
            if receipt.get("head") != self.state["engine_head"]:
                raise ForgeError("Missing exact engine source gate: " + label)
            run = self.api(f"repos/{self.state['engine_repository']}/actions/runs/{receipt['run']}")
            if (run.get("head_sha") != self.state["engine_head"]
                    or run.get("status") != "completed" or run.get("conclusion") != "success"):
                raise ForgeError("The engine source gate is no longer successful: " + label)

    def games_snapshot(self, head):
        slug = self.state["idea"]["slug"]
        selected = "games/" + slug
        catalog = json.loads(git(self.games, "show", head + ":.games-catalog.json"))
        if (catalog.get("source_repository") != self.state["engine_repository"]
                or catalog.get("target_repository") != self.state["games_repository"]
                or catalog.get("source_revision") != self.state["engine_head"]):
            raise ForgeError("Concurrent Games export changed the pinned engine source; retain it for a fresh native release")
        definitions = json.loads(git(self.games, "show", head + ":.release-games.json"))
        definitions = [item for item in definitions["native_playables"] if item.get("slug") == slug]
        if len(definitions) != 1:
            raise ForgeError("The released game needs its unique native definition")
        return {"game_tree": git(self.games, "rev-parse", head + ":" + selected),
                "definition": definitions[0],
                "thumbnail": git(self.games, "rev-parse", head + ":site/thumbs/" + slug + ".png"),
                "ownership": sorted([item["path"], item["sha256"]] for item in catalog["files"]
                                    if item["path"] == selected or item["path"].startswith(selected + "/")),
                "preserved_paths": sorted(path for path in catalog.get("preserved_paths", [])
                                          if path == selected or path.startswith(selected + "/") or selected.startswith(path + "/"))}

    def publish_games(self):
        """Reconcile unrelated main updates and recheck the exact installer head."""
        self.assert_engine_gates()
        self.assert_games_worktree()
        head = git(self.games, "rev-parse", "HEAD")
        self.assert_games_merge_finished()
        if git(self.games, "status", "--porcelain"):
            raise ForgeError(f"Retained Games integration needs a clean committed merge in {self.games}; preserve the exact released game")
        recorded = self.state["games_head"]
        if head != recorded and not self.state.get("games_integration_pending"):
            raise ForgeError("Games source changed outside its retained integration")
        frozen = self.state.setdefault("games_source_snapshot", self.games_snapshot(recorded))
        self.save()
        self.command(["git", "fetch", "origin", "main"], cwd=self.games, label="games-integration-fetch")
        # A confirmed original push remains valid when unrelated main commits
        # follow it; the original immutable production receipt remains the proof.
        if head == recorded and git_ancestor(self.games, recorded, "origin/main"):
            if self.state.get("ci", {}).get("windows-installer-review", {}).get("head") != recorded:
                self.installer_review()
            self.assert_source_gates()
            return
        if not git_ancestor(self.games, recorded, head):
            raise ForgeError("Games integration lost the exact exported source commit")
        if not git_ancestor(self.games, "origin/main", head):
            self.state["games_integration_pending"] = recorded
            self.save()
            try:
                self.command(["git", "-c", "user.name=IdeaForge", "-c", "user.email=idea-forge@users.noreply.github.com",
                              "merge", "--no-edit", "origin/main"], cwd=self.games, label="games-merge-main")
            except ForgeError as error:
                raise ForgeError(f"Companion integration needs review in {self.games}; resolve conflicts and commit "
                                 "the merge there, then resume. Installer evidence cannot certify a new candidate.") from error
            head = git(self.games, "rev-parse", "HEAD")
        if self.games_snapshot(head) != frozen:
            raise ForgeError("Concurrent Games main changed the released game, thumbnail, ownership or native definition; retain its exact source for a separate reviewed release")
        self.record_games_candidate(head)
        self.state.pop("games_integration_pending", None)
        self.save()
        if self.state.get("ci", {}).get("windows-installer-review", {}).get("head") != head:
            self.installer_review()
        self.assert_source_gates()
        if git(self.games, "rev-parse", "HEAD") != head or git(self.games, "status", "--porcelain"):
            raise ForgeError("Companion source changed after installer review; retain it for fresh review")
        self.command(["git", "fetch", "origin", "main"], cwd=self.games, label="games-publication-fetch")
        if git_ancestor(self.games, head, "origin/main"):
            return
        if not git_ancestor(self.games, "origin/main", head):
            raise ForgeError("Games main advanced during installer review; resume to integrate and repeat its exact-source gate")
        self.command(["git", "push", "origin", head + ":main"], cwd=self.games, label="publish-games")

    def verify_publication(self):
        repository = self.state["games_repository"]
        head = self.state["games_head"]
        production = self.wait_ci(repository, "releases.yml", "main", head, "production-release")
        self.wait_ci(repository, "pages.yml", "main", head, "download-page", after=production["updated_at"])
        release = next((r for r in self.api(f"repos/{repository}/releases?per_page=30")
                        if f"-games-{head[:12]}-" in r["tag_name"] and not r["draft"] and not r["prerelease"]), None)
        if not release:
            raise ForgeError("No immutable production release matches the reviewed companion source")
        assets = {a["name"]: a for a in release["assets"]}
        slug = self.state["idea"]["slug"]
        downloads = self.directory / "publication" / release["tag_name"]
        downloads.mkdir(parents=True, exist_ok=True)
        def fetch(url):
            request = urllib.request.Request(url, headers={"User-Agent": "IdeaForge", "Cache-Control": "no-cache"})
            with urllib.request.urlopen(request, timeout=60) as response:
                if response.status != 200:
                    raise ForgeError("Public download returned an unsuccessful HTTP response")
                return response.read()
        def asset(name):
            if name not in assets:
                raise ForgeError(f"Production release lacks {name}")
            data = fetch(assets[name]["browser_download_url"])
            (downloads / name).write_bytes(data)
            digest = hashlib.sha256(data).hexdigest()
            if assets[name].get("digest") and assets[name]["digest"] != "sha256:" + digest:
                raise ForgeError("Downloaded asset does not match its GitHub digest")
            return data, digest
        sums = {}
        for name in ("SHA256SUMS.txt", "INSTALLER-SHA256SUMS.txt"):
            if name in assets:
                data, _ = asset(name)
                for line in data.decode().splitlines():
                    parts = line.split(maxsplit=1)
                    if len(parts) == 2:
                        sums[parts[1].lstrip("*")] = parts[0]
        catalog, _ = asset("Games-catalog.tsv")
        rows = csv.DictReader(io.StringIO(catalog.decode()), delimiter="\t")
        entry = next((r for r in rows if r["slug"] == slug), None)
        installer_name, zip_name = slug + "-setup-windows-x64.exe", slug + "-windows-x64.zip"
        installer, installer_hash = asset(installer_name)
        _, zip_hash = asset(zip_name)
        if not entry or entry["release"] != release["tag_name"] or entry["sha256"] != zip_hash:
            raise ForgeError("The public catalog does not match the reviewed release")
        if installer[:2] != b"MZ" or sums.get(installer_name) != installer_hash or sums.get(zip_name) != zip_hash:
            raise ForgeError("Published installer/ZIP checksums do not match")
        owner, name = repository.split("/")
        base = f"https://{owner}.github.io/{name}/"
        page_url = base + "games/" + slug + "/"
        import html
        deadline = time.monotonic() + min(self.state["wait_timeout"], 300)
        while True:
            try:
                page = fetch(page_url).decode()
                if assets[installer_name]["browser_download_url"] not in html.unescape(page):
                    raise ForgeError("The public page still points to a different release")
                if f"games/{slug}/" not in fetch(base).decode():
                    raise ForgeError("The public catalog has not deployed the new game")
                thumbnail = fetch(base + "thumbs/" + slug + ".png")
                if hashlib.sha256(thumbnail).digest() != hashlib.sha256(Path(self.state["screenshot"]).read_bytes()).digest():
                    raise ForgeError("The live screenshot differs from the verified native capture")
                break
            except (ForgeError, OSError):
                if time.monotonic() >= deadline:
                    raise
                time.sleep(10)
        self.state["publication"] = {"page": page_url, "release": release["html_url"],
                                     "installer_sha256": installer_hash, "zip_sha256": zip_hash,
                                     "verified_at": datetime.now(timezone.utc).isoformat()}
        atomic_json(downloads / "receipt.json", self.state["publication"])

    def prepare_feedback_integration(self):
        """Retain conflicts and new checks separately from the released source."""
        integration = self.state["feedback_integration"]
        root = self.directory / "engine-feedback"
        branch, source = integration["branch"], integration["source_head"]
        integration["mode"] = "integration"
        self.save()  # Record ownership before creating or merging a worktree.
        if root.is_symlink():
            raise ForgeError("Feedback integration worktree must not be a symlink")
        if not root.exists():
            existing = git(self.engine, "branch", "--list", branch)
            argv = ["git", "worktree", "add"]
            argv += [str(root), branch] if existing else ["-b", branch, str(root), source]
            self.command(argv, cwd=self.engine, label="feedback-worktree")
        if (Path(git(root, "rev-parse", "--show-toplevel")).resolve() != root.resolve()
                or git(root, "branch", "--show-current") != branch):
            raise ForgeError("Feedback integration worktree or branch does not match its retained state")
        merge = Path(git(root, "rev-parse", "--git-path", "MERGE_HEAD"))
        if not merge.is_absolute():
            merge = root / merge
        if merge.exists() or git(root, "status", "--porcelain"):
            raise ForgeError(f"Feedback integration is unfinished in {root}; resolve conflicts and commit "
                             "the merge there, then resume. Preserve main's ledger and game registry entries.")
        head = git(root, "rev-parse", "HEAD")
        if not git_ancestor(root, source, head):
            raise ForgeError("Feedback integration no longer contains the frozen verified source")
        # A successful push followed by an API failure must reuse its commit, even
        # when another author has advanced main since that push.
        if not git_ancestor(root, head, "origin/main") and not git_ancestor(root, "origin/main", head):
            try:
                self.command(["git", "-c", "user.name=IdeaForge", "-c",
                              "user.email=idea-forge@users.noreply.github.com", "merge", "--no-edit", "origin/main"],
                             cwd=root, label="feedback-merge-main")
            except ForgeError as error:
                raise ForgeError(f"Feedback integration needs review in {root}; resolve conflicts and commit "
                                 "the merge there, then resume. Preserve main's ledger and game registry entries.") from error
        head = git(root, "rev-parse", "HEAD")
        if git(root, "status", "--porcelain") or not git_ancestor(root, source, head):
            raise ForgeError("Feedback integration must be clean and retain its frozen source")
        integration["head"] = head
        self.save()
        return root, head

    def publish_feedback(self):
        if code_digest(self.engine) != self.state["verified_code"]:
            raise ForgeError("Code changed after publication; unverified changes cannot enter engine main")
        integration = self.state.get("feedback_integration")
        if integration is None:
            self.state["status"] = "published"
            self.feedback()
            paths = [self.state["feedback_path"], "docs/learning/ledger.jsonl", "docs/learning/forge-runs.jsonl"]
            staged = git(self.engine, "diff", "--cached", "--name-only").splitlines()
            if set(staged) - set(paths):
                raise ForgeError("Unrelated staged changes must be preserved outside feedback publication")
            git(self.engine, "add", *paths)
            self.commit(self.engine, "Record verified delivery and AI feedback for " + self.state["idea"]["title"])
            integration = {"branch": self.state["branch"] + "-feedback",
                           "source_head": git(self.engine, "rev-parse", "HEAD"), "mode": "direct"}
            self.state["feedback_integration"] = integration
            self.save()  # Freeze the source before any fetch, worktree mutation, or push.
        source = integration["source_head"]
        if not git_ancestor(self.engine, self.state["engine_head"], source):
            raise ForgeError("Frozen feedback source does not contain the verified game commit")
        self.command(["git", "fetch", "origin", "main"], cwd=self.engine, label="feedback-fetch")
        direct = integration["mode"] == "direct" and (
            git_ancestor(self.engine, "origin/main", source) or git_ancestor(self.engine, source, "origin/main"))
        root, head = (self.engine, source) if direct else self.prepare_feedback_integration()
        if not direct:
            repository, branch = self.state["engine_repository"], integration["branch"]
            self.command(["git", "push", "-u", "origin", head + ":refs/heads/" + branch],
                         cwd=root, label="push-feedback-review")
            for workflow, label, dispatch in (
                    ("forge-game-checks.yml", "feedback-game-linux-windows-" + head, True),
                    ("ci.yml", "feedback-engine-checks-" + head, False)):
                receipt = self.wait_ci(repository, workflow, branch, head, label, dispatch=dispatch,
                                       fields={"game_slug": self.state["idea"]["slug"]} if dispatch else None)
                if not receipt or receipt.get("head") != head:
                    raise ForgeError("Feedback integration lacks an exact-source CI receipt")
            if git(root, "rev-parse", "HEAD") != head or git(root, "status", "--porcelain"):
                raise ForgeError("Feedback integration changed after CI; resume to recheck the retained source")
        self.command(["git", "fetch", "origin", "main"], cwd=root, label="feedback-publication-fetch")
        if not git_ancestor(root, head, "origin/main"):
            if not git_ancestor(root, "origin/main", head):
                raise ForgeError("Engine main advanced during feedback checks; resume to integrate and recheck it")
            if git(root, "rev-parse", "HEAD") != head or git(root, "status", "--porcelain"):
                raise ForgeError("Feedback worktree changed before publication; retained without a push")
            self.command(["git", "push", "origin", head + ":main"], cwd=root, label="publish-engine-feedback")
        blob = git(root, "rev-parse", head + ":" + self.state["feedback_path"])
        remote = self.api(f"repos/{self.state['engine_repository']}/contents/{self.state['feedback_path']}?ref=main")
        if remote["sha"] != blob:
            raise ForgeError("Final feedback was not verified on engine main")
        self.state["feedback_commit"] = head

    def rebuild(self):
        """Recheck integrated changes without regenerating the selected concept."""
        if "publish-games" in self.state.get("completed", []) or self.state.get("publication"):
            raise ForgeError("This run has already published; start a new run for another release")
        self.state.setdefault("verification_history", []).append({
            key: self.state.get(key) for key in
            ("completed", "engine_head", "games_head", "verified_code", "ci", "games_source_snapshot", "games_integration_pending")})
        self.state["completed"] = [name for name in self.state.get("completed", [])
                                   if name in ("setup", "generate")]
        for key in ("verified_code", "engine_head", "games_head", "screenshot", "ci",
                    "dispatched", "feedback_commit", "error", "games_source_snapshot", "games_integration_pending"):
            self.state.pop(key, None)
        self.state["rebuild_reason"] = (
            "The existing game worktree was updated. Preserve the selected concept and "
            "revalidate the implementation, tests, native visuals and controls against "
            "the current engine. Previous verification cannot certify these changes.")
        self.state.update(status="created", phase="build-and-review")
        self.save()

    def run(self):
        completed = self.state.get("completed", [])
        if ("build-and-review" in completed and "publish-games" not in completed
                and not self.state.get("publication") and self.game.is_dir()):
            try:
                self.validate_wiki(self.game)
            except ForgeError:
                self.rebuild()
                self.state["rebuild_reason"] += (
                    " The game needs the current mini-wiki contract, public design rationale "
                    "and at least three real gameplay captures before publication.")
        stages = [("setup", self.setup), ("generate", self.generate), ("build-and-review", self.build),
                  ("feedback", self.feedback), ("source-commit", self.register_source)]
        if self.state["publish"]:
            stages += [("engine-ci", self.engine_ci), ("export", self.export),
                       ("installer-review", self.installer_review), ("publish-games", self.publish_games),
                       ("verify-publication", self.verify_publication), ("publish-feedback", self.publish_feedback)]
        for name, function in stages:
            if name in self.state.setdefault("completed", []):
                continue
            self.state.update(status="running", phase=name)
            self.save()
            print(f"IdeaForge: {name} ({self.directory})", flush=True)
            try:
                function()
                self.state["completed"].append(name)
                self.save()
            except (ForgeError, OSError, ValueError, KeyError, KeyboardInterrupt) as error:
                self.state.update(status="failed", error=type(error).__name__)
                self.state.setdefault("failures", []).append({"phase": name, "description":
                    str(error).split("; private log:")[0] if isinstance(error, ForgeError)
                    else "Stage interrupted; detailed error remains in the private run evidence."})
                try:
                    self.save()
                except OSError:
                    pass  # Keep the original failure if the journal disk also filled.
                if not self.state.get("feedback_integration", {}).get("source_head"):
                    try:
                        self.feedback()
                    except (ForgeError, OSError, ValueError):
                        pass  # A feedback failure must not erase the original gate failure.
                raise
        self.state["status"] = "published" if self.state["publish"] else "built-locally"
        self.save()
        print(json.dumps({"status": self.state["status"], "game": str(self.game),
                          "page": self.state.get("publication", {}).get("page"),
                          "feedback": str(self.engine / self.state["feedback_path"]),
                          "resume": str(self.directory)}, indent=2))


def create_run(args, directory=None):
    source = args.engine_root.resolve()
    if not (source / "tools/be2.py").is_file() or not (source / "Cargo.toml").is_file():
        raise ForgeError("--engine-root must be a BlueEngine source checkout")
    if not shutil.which(args.agent):
        raise ForgeError("Codex CLI is required; install/sign in, or set --agent-executable")
    if args.publish and (not args.games_root or not args.games_root.is_dir()):
        raise ForgeError("Publication needs --games-root pointing to the BlueEngineGames checkout")
    directory = directory or new_run_directory(args)
    if (directory / "state.json").exists():
        raise ForgeError("A retained run must be resumed, never overwritten")
    directory.mkdir(parents=True, mode=0o700, exist_ok=True)
    target = Path(os.environ.get("CARGO_TARGET_DIR", source / ".be2-work/idea-forge/cargo-target")).resolve()
    target.mkdir(parents=True, exist_ok=True)
    if args.command == "run" and shutil.disk_usage(target).free < args.min_free_gib * 1024**3:
        raise ForgeError("Insufficient free space for native compilation; use a larger CARGO_TARGET_DIR")
    state = {"version": 1, "id": directory.name[-8:], "created": datetime.now(timezone.utc).isoformat(),
             "source_root": str(source), "games_root": str(args.games_root.resolve()) if args.games_root else None,
             "base": args.base, "branch": "idea-forge/" + directory.name, "target_dir": str(target),
             "agent": args.agent, "model": args.model, "brief": args.brief, "story": args.story,
             "dimension": args.dimension,
             "idea_input": str(args.idea.resolve()) if getattr(args, "idea", None) else None,
             "repairs": args.repairs, "timeout": args.agent_timeout, "wait_timeout": args.wait_timeout,
             "min_free_gib": args.min_free_gib,
             "publish": args.publish, "status": "created", "completed": []}
    forge = Forge(directory, state)
    forge.save()
    return forge


def new_run_directory(args):
    return (args.runs_root or args.engine_root / ".be2-work/idea-forge/runs").resolve() / (
        datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S") + "-" + uuid4().hex[:8])


def delivery_complete(state):
    return (state.get("status") == "published" and bool(state.get("publication"))
            and "publish-feedback" in state.get("completed", []))


def daily_batch(args, now=None):
    """Two persistent slots per local date; retries always resume the same games."""
    daily_root = (args.daily_root or args.engine_root / ".be2-work/idea-forge/daily").resolve()
    daily_root.mkdir(parents=True, exist_ok=True, mode=0o700)
    instant = now or datetime.now(timezone.utc)
    today = instant.astimezone(ZoneInfo(args.timezone)).date().isoformat()
    with run_lock(daily_root):
        batches = daily_root / "batches"
        paths = sorted(batches.glob("*/state.json"))
        current = batches / today / "state.json"
        if not current.exists():
            order = ["2d", "3d"]
            random.SystemRandom().shuffle(order)
            atomic_json(current, {"version": 1, "date": today, "timezone": args.timezone,
                                  "order": order, "slots": {}, "status": "pending"})
            paths.append(current)
        failed = False
        for path in sorted(paths):
            batch = load_json(path)
            if batch.get("version") != 1 or sorted(batch.get("order", [])) != ["2d", "3d"]:
                raise ForgeError("Invalid daily batch journal")
            if batch["status"] == "published":
                continue
            for dimension in batch["order"]:
                slot = batch["slots"].get(dimension)
                if slot is None:
                    # Reserve the identity BEFORE creating the run. A process exit
                    # between the two writes cannot create a replacement concept.
                    slot = {"directory": str(new_run_directory(args)), "status": "pending"}
                    batch["slots"][dimension] = slot
                    atomic_json(path, batch)
                directory = Path(slot["directory"])
                if slot.get("retry_after") and instant < datetime.fromisoformat(slot["retry_after"]):
                    print(f"IdeaForge daily: {batch['date']} {dimension} waiting until {slot['retry_after']}; retained run {directory}", flush=True)
                    break
                try:
                    if (directory / "state.json").exists():
                        state = load_json(directory / "state.json")
                        if state.get("dimension") != dimension or not state.get("publish"):
                            raise ForgeError("Daily slot does not match its retained publishing run")
                        worker = Forge(directory, state)
                    else:
                        settings = argparse.Namespace(**vars(args))
                        settings.command, settings.dimension = "run", dimension
                        worker = create_run(settings, directory)
                    with run_lock(directory):
                        if not delivery_complete(worker.state):
                            worker.run()
                    if not delivery_complete(worker.state):
                        raise ForgeError("Daily completion requires verified publication and feedback")
                    slot["status"] = "published"
                    for key in ("retry_after", "last_error", "attempts"):
                        slot.pop(key, None)
                except (ForgeError, OSError, ValueError, KeyError) as error:
                    failed = True
                    slot["status"] = "failed"
                    slot["attempts"] = min(int(slot.get("attempts", 0)) + 1, 24)
                    slot["retry_after"] = ((now or datetime.now(timezone.utc)) + timedelta(hours=min(2 ** (slot["attempts"] - 1), 8))).isoformat()
                    message = str(error).split("; private log:")[0][:500] if isinstance(error, ForgeError) else type(error).__name__
                    slot["last_error"] = message if not learn.secret_like(message) else type(error).__name__
                    print(f"IdeaForge daily: {batch['date']} {dimension} retained: {slot['last_error']}; retry after {slot['retry_after']}; run {directory}",
                          file=sys.stderr, flush=True)
                atomic_json(path, batch)
                if slot["status"] != "published":
                    break
            if all(batch["slots"].get(d, {}).get("status") == "published" for d in ("2d", "3d")):
                batch["status"] = "published"
                atomic_json(path, batch)
                print(f"IdeaForge daily: {batch['date']} published one 2D and one 3D game", flush=True)
            else:
                # Finish older retained work before allocating more days of games.
                break
        return 1 if failed else 0


def systemd_quote(value):
    value = str(value)
    if any(c in value for c in "\n\r\0"):
        raise ForgeError("Scheduler paths must be single-line values")
    return '"' + value.replace('\\', '\\\\').replace('"', '\\"').replace('%', '%%').replace('$', '$$') + '"'


def install_schedule(args):
    """Install an hourly wakeup: completed daily slots make later wakeups no-ops."""
    if not sys.platform.startswith("linux") or not shutil.which("systemctl"):
        raise ForgeError("Automatic installation needs Linux user systemd; schedule `ideaforge daily` on other hosts")
    config_root = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config"))
    config = config_root / "ideaforge/daily.json"
    values = {k: str(v.resolve()) if isinstance(v, Path) else v for k, v in vars(args).items()
              if k not in ("command", "config", "dimension", "publish")}
    config.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    atomic_json(config, values)
    config.chmod(0o600)
    units = config_root / "systemd/user"
    units.mkdir(parents=True, exist_ok=True)
    invocation = " ".join(systemd_quote(v) for v in
                          (sys.executable, Path(__file__).resolve(), "daily", "--config", config))
    working_directory = str(args.engine_root.resolve())
    systemd_quote(working_directory)  # Reject line breaks; this directive takes a literal path.
    (units / "ideaforge-daily.service").write_text(
        "[Unit]\nDescription=IdeaForge: one 2D and one 3D game per day\n"
        "After=network-online.target\n\n[Service]\nType=oneshot\n"
        f"WorkingDirectory={working_directory.replace('%', '%%')}\n"
        f"Environment={systemd_quote('PATH=' + os.environ.get('PATH', '/usr/local/bin:/usr/bin:/bin'))}\n"
        + (f"Environment={systemd_quote('CARGO_TARGET_DIR=' + os.environ['CARGO_TARGET_DIR'])}\n"
           if os.environ.get("CARGO_TARGET_DIR") else "")
        + f"ExecStart={invocation}\nTimeoutStartSec=infinity\nKillMode=control-group\nUMask=0077\n")
    (units / "ideaforge-daily.timer").write_text(
        "[Unit]\nDescription=Wake IdeaForge hourly to complete the daily pair\n\n[Timer]\n"
        f"OnCalendar=*-*-* *:10:00 {args.timezone}\nPersistent=true\nAccuracySec=1min\n"
        "Unit=ideaforge-daily.service\n\n[Install]\nWantedBy=timers.target\n")
    subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
    subprocess.run(["systemctl", "--user", "enable", "--now", "ideaforge-daily.timer"], check=True)
    subprocess.run(["systemctl", "--user", "start", "--no-block", "ideaforge-daily.service"], check=True)
    print(f"IdeaForge scheduled: one 2D + one 3D daily in {args.timezone}; random order; hourly retry.\nConfig: {config}")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("run", "generate", "daily", "schedule"):
        command = sub.add_parser(name, help={"run": "Build/publish a game", "generate": "Generate a fresh AI game concept",
                                            "daily": "Complete today's 2D and 3D games", "schedule": "Enable automatic daily games"}[name])
        command.add_argument("--engine-root", type=Path, default=ROOT)
        command.add_argument("--games-root", type=Path, default=ROOT.parent / "BlueEngineGames")
        command.add_argument("--runs-root", type=Path)
        command.add_argument("--base", default="origin/main", help="Committed source base; working edits are preserved")
        command.add_argument("--brief", default="A small, original game with a surprising and satisfying central rule")
        if name in ("run", "generate"):
            command.add_argument("--dimension", choices=("any", "2d", "3d"), default="any")
        else:
            command.add_argument("--timezone", default="America/Los_Angeles")
            command.add_argument("--daily-root", type=Path)
            command.set_defaults(dimension="any")
        if name == "daily":
            command.add_argument("--config", type=Path, help="Load a schedule's saved settings")
        if name == "run":
            command.add_argument("--idea", type=Path, help="Build a previously generated schema-valid concept JSON")
        command.add_argument("--story", action="store_true")
        command.add_argument("--agent-executable", dest="agent", default="codex")
        command.add_argument("--model", help="Optional Codex model override; otherwise inherit the configured model")
        command.add_argument("--repairs", type=int, choices=range(0, 6), default=2)
        command.add_argument("--agent-timeout", type=int, default=3600)
        command.add_argument("--wait-timeout", type=int, default=10800)
        command.add_argument("--min-free-gib", type=float, default=8)
        if name in ("run", "generate"):
            command.add_argument("--no-publish", dest="publish", action="store_false", default=name == "run")
        else:
            command.set_defaults(publish=True)
    resume = sub.add_parser("resume", help="Continue a retained run without repeating completed stages")
    resume.add_argument("directory", type=Path)
    resume.add_argument("--publish", action="store_true", help="Enable publication for a locally built run")
    resume.add_argument("--games-root", type=Path, help="BlueEngineGames checkout for publication")
    resume.add_argument("--rebuild", action="store_true",
                        help="Invalidate old gates and rebuild/review the same idea after worktree changes")
    status = sub.add_parser("status", help="Inspect a run's phase and verified delivery")
    status.add_argument("directory", type=Path)
    repair = sub.add_parser("repair-source-receipt", help="Repair a legacy run's exact supervisor source-registration receipt")
    repair.add_argument("directory", type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == "daily" and args.config:
            values = load_json(args.config)
            for key, value in values.items():
                if key not in vars(args) or key in ("command", "config", "dimension", "publish"):
                    raise ForgeError("Unsupported daily configuration field")
                setattr(args, key, Path(value) if key.endswith("_root") and value is not None else value)
        if args.command in ("daily", "schedule"):
            try:
                ZoneInfo(args.timezone)
            except (ZoneInfoNotFoundError, ValueError):
                raise ForgeError("Unknown daily timezone") from None
            if args.agent_timeout <= 0 or args.wait_timeout <= 0 or args.min_free_gib < 0:
                raise ForgeError("Timeouts must be positive and capacity threshold nonnegative")
            if args.command == "schedule":
                install_schedule(args)
                return 0
            return daily_batch(args)
        if args.command in ("resume", "status", "repair-source-receipt"):
            state = load_json(args.directory / "state.json")
            if state.get("version") != 1:
                raise ForgeError("Unsupported run state version")
            if args.command == "status":
                print(json.dumps({key: state.get(key) for key in
                                  ("status", "phase", "completed", "publication", "feedback_path")}, indent=2))
                return 0
            forge = Forge(args.directory, state)
        else:
            if args.agent_timeout <= 0 or args.wait_timeout <= 0 or args.min_free_gib < 0:
                raise ForgeError("Timeouts must be positive and capacity threshold nonnegative")
            forge = create_run(args)
            if args.command == "generate":
                forge.setup()
                forge.generate()
                forge.state.update(status="concept-generated", completed=["setup", "generate"])
                forge.save()
                print(json.dumps(forge.state["idea"], indent=2))
                print(f"Concept only. To build: ideaforge resume {forge.directory}", file=sys.stderr)
                return 0
        with run_lock(forge.directory):
            if args.command == "repair-source-receipt":
                forge.repair_source_registration_receipt()
                print("Recorded the exact source-registration receipt; no source edits or publication performed.")
                return 0
            if args.command == "resume":
                if args.rebuild:
                    forge.rebuild()
                if args.publish:
                    if args.games_root:
                        state["games_root"] = str(args.games_root.resolve())
                    if not state.get("games_root") or not Path(state["games_root"]).is_dir():
                        raise ForgeError("Publication needs a BlueEngineGames checkout")
                    state["publish"] = True
                    state["engine_repository"] = github_repository(Path(state["source_root"]))
                    forge.save()
            forge.run()
        return 0
    except (ForgeError, OSError, ValueError, KeyError) as error:
        message = str(error) if isinstance(error, ForgeError) else type(error).__name__
        print("IdeaForge: " + message, file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
