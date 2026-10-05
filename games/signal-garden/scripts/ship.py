#!/usr/bin/env python3
"""Ship this game: package it, give it its own desktop shortcut, and prove both work.

    python scripts/ship.py info [--json]
    python scripts/ship.py package [--no-build]
    python scripts/ship.py shortcut [--folder DIR] [--force] [--platform windows|linux|macos]
    python scripts/ship.py verify [--folder DIR] [--launch] [--smoke] [--skip-package] [--json]
    python scripts/ship.py ship [--folder DIR] [--force] [--no-build] [--no-launch] [--no-smoke]

The project root is the parent of this scripts/ folder. Everything the game is called comes from
assets/identity.json (title, tagline, controls, optional exe / package / smoke_args / engine_revision).
Every command prints ONE JSON object on stdout and human-readable lines on stderr. Exit codes: 0 ok,
1 something failed, 2 usage or configuration error. Standard library only; the Windows shell parts
(shortcuts, shell icon rendering, the launch check) run embedded PowerShell 5.1 helpers.
"""
import argparse
import dataclasses
import datetime
import hashlib
import json
import math
import os
import posixpath
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib
from pathlib import Path

# --------------------------------------------------------------------------------------------------
# Constants
# --------------------------------------------------------------------------------------------------

ICO_SIZES = (16, 20, 24, 32, 40, 48, 64, 96, 128, 256)
PNG_FRAME_FROM = 64  # ICO frames of this size and up are PNG, smaller ones are 32-bit BMP
RGBA_BLOBS = {'icon_16.rgba': 16, 'icon_32.rgba': 32, 'icon_64.rgba': 64}  # the window icon
PNG_SIGNATURE = b'\x89PNG\r\n\x1a\n'

TITLE_MAX = 60
CONTROLS_MAX = 200
DESCRIPTION_MAX = 255  # what we put in the shortcut tooltip
INVALID_TITLE_CHARS = '\\/:*?"<>|'
PLACEHOLDER_TITLES = frozenset({'play', 'game', 'blueengine game', 'blueengine', 'untitled', 'my game',
                                'new game'})
WINDOWS_RESERVED = frozenset({'con', 'prn', 'aux', 'nul', *(f'com{i}' for i in range(1, 10)),
                              *(f'lpt{i}' for i in range(1, 10))})
ENGINE_CRATES = ('be2', 'vesper3d')

DEFAULT_SMOKE_ARGS = ('--capture', '{dir}')
SMOKE_TIMEOUT = 60  # seconds
LAUNCH_TIMEOUT = 20  # seconds to wait for the process and its window
SCAN_MAX_FILES = 200  # shortcuts compared in shortcut-unique
SCAN_MAX_DEPTH = 2  # folders below the desktop that are searched
STAMP_NAME = 'ship.json'
STALE_SLACK = 2.0  # seconds of mtime slack before "dist exe is older than the build"

# Perceptual signature (checks icon-art, exe-resources, shortcut-icon, shortcut-unique, launch).
# An icon is composited over mid-grey and box-resampled to SIG_GRID x SIG_GRID. Its shape is the average
# hash of the HASH_GRID x HASH_GRID luminance blocks (also taken with the grid shifted by one pixel, so a
# 1 px shift is the same shape); its colour is a soft histogram with HIST_BINS bins per RGB channel over the
# opaque pixels. Two icons are TOO SIMILAR when both distances are small (a same-layout icon in another
# palette, or a same-palette icon with another glyph, is distinct); "the same picture" is one distance.
#
# Calibration (32 px shell renders of the 32 shortcuts on the author's real Desktop = 13 distinct pictures,
# 86 pairs of shortcuts that share one picture):
#   * shortcuts sharing a picture (12 x the generic app icon, 5 x the white rat, 5 x the red mouse):
#     distance 0.000 / 0.000, all 86 pairs flagged
#   * distinct pictures, 78 pairs: smallest shape distance 0.062 (same layout, other palette: colour 0.797),
#     smallest colour distance 0.311, closest pair overall shape 0.172 + colour 0.451 (two dark tiles with a
#     bright glyph): none flagged
#   * synthesised near-duplicates (hue +-10 deg, +-3% brightness noise or scale, 1 px shifts, bilinear
#     re-scales through 48 and 24 px, and hue + noise + shift combined; 13 icons x 12 variants): worst shape
#     0.094, worst colour 0.319 (the heavy 24 px blur), all 156 flagged
#   * the same art through the shell as .lnk, .exe and .ico: 0.000; the frames of one hand-tiered .ico
#     (16..256 px, small sizes drawn separately) differ by at most 0.109
# so shape 0.15 and colour 0.38 sit between the worst near-duplicate (0.094 / 0.319) and the closest distinct
# pair (0.172 / 0.451), and EQUAL_MAX 0.12 sits above the tiered-frame spread and far below any distinct pair.
SIG_GRID = 32
HASH_GRID = 8
HIST_BINS = 4
SIMILAR_SHAPE = 0.15  # shape distance (share of differing hash bits) at or below which shapes look alike
SIMILAR_COLOUR = 0.38  # colour distance (histogram total variation) at or below which palettes look alike
EQUAL_MAX = 0.12  # max(shape, colour) at or below which two renders are "the same picture"

# --------------------------------------------------------------------------------------------------
# Errors and small helpers
# --------------------------------------------------------------------------------------------------


class ShipError(Exception):
    """A command failed (exit code 1)."""

    exit_code = 1


class ConfigError(ShipError):
    """The project is not set up for shipping (exit code 2)."""

    exit_code = 2


class ImageError(ValueError):
    """An image or icon file could not be parsed."""


NO_WINDOW = getattr(subprocess, 'CREATE_NO_WINDOW', 0)


def log(message=''):
    """Human-readable progress goes to stderr; stdout is reserved for the JSON result."""
    print(message, file=sys.stderr)


def host_platform():
    if sys.platform == 'win32':
        return 'windows'
    if sys.platform == 'darwin':
        return 'macos'
    if sys.platform.startswith('linux'):
        return 'linux'
    return 'other'


def iso_now():
    return datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as handle:
        for block in iter(lambda: handle.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def write_text_atomic(path, text):
    """Write a UTF-8 text file by renaming a finished temp file over the destination."""
    path = Path(path)
    temp = path.with_name(f'.{path.name}.{os.getpid()}.tmp')
    temp.write_text(text, encoding='utf-8')
    os.replace(temp, path)


def write_json_atomic(path, value):
    write_text_atomic(path, json.dumps(value, indent=2, ensure_ascii=False) + '\n')


def read_json_file(path):
    """Parse a JSON file; returns None when it is missing or not valid JSON."""
    try:
        return json.loads(Path(path).read_text(encoding='utf-8-sig'))
    except (OSError, ValueError):
        return None


def norm_path(value):
    return os.path.normcase(os.path.normpath(os.path.abspath(str(value))))


def same_file(a, b):
    """True when both paths name the same file (case, slashes and links do not matter)."""
    if not a or not b:
        return False
    try:
        if os.path.exists(a) and os.path.exists(b):
            return os.path.samefile(a, b)
    except OSError:
        pass
    return norm_path(a) == norm_path(b)


def inside(path, folder):
    """True when `path` is `folder` or lies below it."""
    path, folder = norm_path(path), norm_path(folder)
    return path == folder or path.startswith(folder.rstrip(os.sep) + os.sep)


def human_size(count):
    for unit in ('B', 'KB', 'MB', 'GB'):
        if count < 1024 or unit == 'GB':
            return f'{count:.0f} {unit}' if unit == 'B' else f'{count:.1f} {unit}'
        count /= 1024
    return f'{count} B'


def shorten(text, limit=200):
    text = ' '.join(str(text).split())
    return text if len(text) <= limit else text[:limit - 3] + '...'


def run_process(command, cwd=None, timeout=None, env=None):
    """Run an argument list (never a shell string); returns CompletedProcess with text output."""
    return subprocess.run([str(part) for part in command], cwd=cwd, capture_output=True, text=True,
                          encoding='utf-8', errors='replace', timeout=timeout, env=env,
                          creationflags=NO_WINDOW)


# --------------------------------------------------------------------------------------------------
# Cargo.toml (tomllib when available, a small fallback parser for Python 3.10)
# --------------------------------------------------------------------------------------------------


class _MiniToml:
    """The subset of TOML that Cargo manifests use: tables, arrays of tables, dotted and quoted
    keys, strings, numbers, booleans, arrays (multi-line) and inline tables."""

    def __init__(self, text):
        self.s = text
        self.i = 0

    def fail(self, message):
        line = self.s.count('\n', 0, self.i) + 1
        raise ValueError(f'invalid TOML at line {line}: {message}')

    def peek(self):
        return self.s[self.i] if self.i < len(self.s) else ''

    def skip_inline_space(self):
        while self.peek() in (' ', '\t'):
            self.i += 1

    def skip_comment(self):
        if self.peek() == '#':
            while self.peek() not in ('', '\n'):
                self.i += 1

    def skip_blank(self):
        """Whitespace, newlines and comments."""
        while True:
            self.skip_inline_space()
            self.skip_comment()
            if self.peek() in ('\n', '\r'):
                self.i += 1
            else:
                return

    def end_of_line(self):
        self.skip_inline_space()
        self.skip_comment()
        if self.peek() not in ('', '\n', '\r'):
            self.fail(f'unexpected {self.peek()!r}')

    def key_path(self):
        parts = []
        while True:
            self.skip_inline_space()
            ch = self.peek()
            if ch in ('"', "'"):
                parts.append(self.string())
            else:
                match = re.compile(r'[A-Za-z0-9_-]+').match(self.s, self.i)
                if not match:
                    self.fail('expected a key')
                parts.append(match.group())
                self.i = match.end()
            self.skip_inline_space()
            if self.peek() == '.':
                self.i += 1
                continue
            return parts

    def string(self):
        quote = self.peek()
        literal = quote == "'"
        if self.s.startswith(quote * 3, self.i):
            end = self.s.find(quote * 3, self.i + 3)
            if end < 0:
                self.fail('unterminated string')
            body = self.s[self.i + 3:end]
            self.i = end + 3
            body = body[1:] if body.startswith('\n') else body
            return body if literal else self.unescape(body)
        self.i += 1
        start = self.i
        while True:
            ch = self.peek()
            if ch in ('', '\n'):
                self.fail('unterminated string')
            if ch == '\\' and not literal:
                self.i += 2
                continue
            if ch == quote:
                break
            self.i += 1
        body = self.s[start:self.i]
        self.i += 1
        return body if literal else self.unescape(body)

    def unescape(self, body):
        simple = {'b': '\b', 't': '\t', 'n': '\n', 'f': '\f', 'r': '\r', '"': '"', '\\': '\\'}
        out, i = [], 0
        while i < len(body):
            ch = body[i]
            if ch != '\\':
                out.append(ch)
                i += 1
                continue
            nxt = body[i + 1:i + 2]
            if nxt in simple:
                out.append(simple[nxt])
                i += 2
            elif nxt in ('u', 'U'):
                width = 4 if nxt == 'u' else 8
                out.append(chr(int(body[i + 2:i + 2 + width], 16)))
                i += 2 + width
            else:  # line-ending backslash in multi-line strings: skip the whitespace that follows
                i += 1
                while i < len(body) and body[i] in ' \t\r\n':
                    i += 1
        return ''.join(out)

    def value(self):
        self.skip_inline_space()
        ch = self.peek()
        if ch in ('"', "'"):
            return self.string()
        if ch == '[':
            return self.array()
        if ch == '{':
            return self.inline_table()
        match = re.compile(r'[^\s,\]}#]+').match(self.s, self.i)
        if not match:
            self.fail('expected a value')
        self.i = match.end()
        token = match.group()
        if token in ('true', 'false'):
            return token == 'true'
        try:
            return int(token.replace('_', ''), 0)
        except ValueError:
            pass
        try:
            return float(token.replace('_', ''))
        except ValueError:
            return token  # dates and other exotic scalars are kept as text

    def array(self):
        self.i += 1
        items = []
        while True:
            self.skip_blank()
            if self.peek() == ']':
                self.i += 1
                return items
            items.append(self.value())
            self.skip_blank()
            if self.peek() == ',':
                self.i += 1
            elif self.peek() != ']':
                self.fail('expected , or ] in an array')

    def inline_table(self):
        self.i += 1
        table = {}
        while True:
            self.skip_blank()
            if self.peek() == '}':
                self.i += 1
                return table
            path = self.key_path()
            self.skip_inline_space()
            if self.peek() != '=':
                self.fail('expected =')
            self.i += 1
            self.assign(table, path, self.value())
            self.skip_blank()
            if self.peek() == ',':
                self.i += 1
            elif self.peek() != '}':
                self.fail('expected , or } in an inline table')

    @staticmethod
    def assign(table, path, value):
        for part in path[:-1]:
            table = table.setdefault(part, {})
            if isinstance(table, list):
                table = table[-1]
        table[path[-1]] = value

    def parse(self):
        root = {}
        current = root
        while True:
            self.skip_blank()
            if self.i >= len(self.s):
                return root
            if self.peek() == '[':
                array = self.s.startswith('[[', self.i)
                self.i += 2 if array else 1
                path = self.key_path()
                closing = ']]' if array else ']'
                if not self.s.startswith(closing, self.i):
                    self.fail(f'expected {closing}')
                self.i += len(closing)
                self.end_of_line()
                table = root
                for part in path[:-1]:
                    table = table.setdefault(part, {})
                    if isinstance(table, list):
                        table = table[-1]
                if array:
                    current = {}
                    table.setdefault(path[-1], []).append(current)
                else:
                    current = table.setdefault(path[-1], {})
            else:
                path = self.key_path()
                self.skip_inline_space()
                if self.peek() != '=':
                    self.fail('expected =')
                self.i += 1
                self.assign(current, path, self.value())
                self.end_of_line()


def parse_toml(text):
    """Parse TOML text: `tomllib` when the interpreter has it (3.11+), else the fallback above."""
    try:
        import tomllib  # noqa: PLC0415 (only present on 3.11+)
    except ImportError:
        return _MiniToml(text).parse()
    return tomllib.loads(text)


def read_cargo(root):
    path = Path(root) / 'Cargo.toml'
    try:
        return parse_toml(path.read_text(encoding='utf-8-sig'))
    except OSError as error:
        raise ConfigError(f'cannot read {path}: {error}') from error
    except ValueError as error:
        raise ConfigError(f'{path} is not valid TOML: {error}') from error


def _dependency_tables(cargo):
    yield cargo.get('dependencies') or {}
    yield cargo.get('build-dependencies') or {}
    yield cargo.get('dev-dependencies') or {}
    yield (cargo.get('workspace') or {}).get('dependencies') or {}
    for target in (cargo.get('target') or {}).values():
        if isinstance(target, dict):
            yield target.get('dependencies') or {}


def find_engine_path(root, cargo):
    """The engine checkout named by a path dependency on `be2` (possibly renamed `vesper3d`)."""
    for table in _dependency_tables(cargo):
        for key, spec in table.items():
            if not isinstance(spec, dict) or not isinstance(spec.get('path'), str):
                continue
            if key in ENGINE_CRATES or spec.get('package') in ENGINE_CRATES:
                candidate = (Path(root) / spec['path']).resolve()
                return candidate if candidate.is_dir() else None
    return None


def engine_head(engine, length=12):
    """`git rev-parse HEAD` of the engine checkout (first `length` characters), or None."""
    if not engine or not shutil.which('git'):
        return None
    try:
        result = run_process(['git', '-C', engine, 'rev-parse', 'HEAD'], timeout=20)
    except (OSError, subprocess.SubprocessError):
        return None
    head = result.stdout.strip()
    return head[:length] if result.returncode == 0 and re.fullmatch(r'[0-9a-f]{40,64}', head) else None


def resolve_exe_stem(identity_exe, cargo):
    """The executable's file stem: identity `exe`, else the sole [[bin]], else the package name."""
    if identity_exe:
        return identity_exe
    package = cargo.get('package') or {}
    bins = [b['name'] for b in cargo.get('bin') or [] if isinstance(b, dict) and b.get('name')]
    if len(bins) == 1:
        return bins[0]
    if bins:
        for preferred in (package.get('default-run'), package.get('name')):
            if preferred in bins:
                return preferred
        raise ConfigError('Cargo.toml declares several [[bin]] targets: set "exe" in assets/identity.json')
    if package.get('name'):
        return package['name']
    raise ConfigError('Cargo.toml has no [package] name and no [[bin]]: set "exe" in assets/identity.json')


# --------------------------------------------------------------------------------------------------
# assets/identity.json
# --------------------------------------------------------------------------------------------------


def title_problems(title):
    """Everything wrong with a game title (empty list = fine). Rules: 1-60 characters after
    trimming, no path characters or control characters, not a placeholder, a valid file name."""
    if not isinstance(title, str):
        return ['title must be a string']
    text = title.strip()
    problems = []
    if not 1 <= len(text) <= TITLE_MAX:
        problems.append(f'title must be 1-{TITLE_MAX} characters after trimming (it has {len(text)})')
    bad = sorted({c for c in text if c in INVALID_TITLE_CHARS})
    if bad:
        problems.append('title must not contain ' + ' '.join(bad))
    if any(ord(c) < 32 or ord(c) == 127 for c in text):
        problems.append('title must not contain control characters')
    if text.endswith('.'):
        problems.append('title must not end with a dot (Windows drops it from the shortcut name)')
    if text.split('.')[0].strip().lower() in WINDOWS_RESERVED:
        problems.append('title is a reserved Windows device name')
    if ' '.join(text.lower().split()) in PLACEHOLDER_TITLES:
        problems.append(f'title "{text}" is a placeholder: a shipped game needs its own name')
    return problems


def _relative_entry_problem(entry):
    if not isinstance(entry, str) or not entry.strip():
        return 'must be a non-empty string'
    parts = entry.replace('\\', '/').split('/')
    if entry.startswith(('/', '\\')) or re.match(r'^[A-Za-z]:', entry) or '..' in parts:
        return 'must be a relative path inside the project'
    if entry.replace('\\', '/').strip('/') in ('', '.'):
        return 'must name a file or folder'
    if parts[0].lower() in ('dist', 'target', '.git', '.blue-check'):
        return f'"{parts[0]}" cannot be packaged'
    return None


def exe_problems(exe):
    """Problems with an identity "exe" value: it is a file stem (no folders, no .exe suffix)."""
    if not isinstance(exe, str) or not exe.strip():
        return ['"exe" must be a non-empty string']
    if re.search(r'[\\/:*?"<>|\x00-\x1f]', exe) or exe.lower().endswith('.exe') or exe != exe.strip():
        return ['"exe" is the file stem: no folders, no ".exe", no special characters']
    return []


def identity_problems(data):
    """Validate the decoded assets/identity.json. Returns a list of plain-English problems."""
    if not isinstance(data, dict):
        return ['assets/identity.json must contain a JSON object']
    problems = []
    if 'title' not in data:
        problems.append('missing "title"')
    else:
        problems.extend(title_problems(data['title']))
    for key in ('tagline', 'controls'):
        value = data.get(key)
        if not isinstance(value, str) or not value.strip():
            problems.append(f'"{key}" must be a non-empty string')
    controls = data.get('controls')
    if isinstance(controls, str) and len(controls.strip()) > CONTROLS_MAX:
        problems.append(f'"controls" must be at most {CONTROLS_MAX} characters')
    for key in ('tagline', 'controls'):
        value = data.get(key)
        if isinstance(value, str) and any(ord(c) < 32 for c in value):
            problems.append(f'"{key}" must be a single line without control characters')
    if data.get('exe') is not None:
        problems.extend(exe_problems(data['exe']))
    package = data.get('package')
    if package is not None:
        if not isinstance(package, list):
            problems.append('"package" must be a list of relative paths')
        else:
            for entry in package:
                reason = _relative_entry_problem(entry)
                if reason:
                    problems.append(f'"package" entry {entry!r} {reason}')
    smoke = data.get('smoke_args')
    if smoke is not None and (not isinstance(smoke, list) or not all(isinstance(a, str) for a in smoke)):
        problems.append('"smoke_args" must be a list of strings')
    revision = data.get('engine_revision')
    if revision is not None and not (isinstance(revision, str) and re.fullmatch(r'[0-9a-fA-F]{7,40}', revision)):
        problems.append('"engine_revision" must be 7-40 hexadecimal characters')
    return problems


@dataclasses.dataclass
class Identity:
    title: str
    tagline: str
    controls: str
    exe: str | None
    package: list
    smoke_args: list | None
    engine_revision: str | None

    @classmethod
    def from_dict(cls, data):
        problems = identity_problems(data)
        if problems:
            raise ConfigError('assets/identity.json: ' + '; '.join(problems))
        return cls(title=data['title'].strip(), tagline=data['tagline'].strip(),
                   controls=data['controls'].strip(), exe=(data.get('exe') or None),
                   package=[e.replace('\\', '/').strip('/') for e in data.get('package') or []],
                   smoke_args=data.get('smoke_args'), engine_revision=data.get('engine_revision'))

    def description(self):
        """The shortcut tooltip: tagline plus controls, at most 255 characters."""
        text = f'{self.tagline} {self.controls}'
        return text if len(text) <= DESCRIPTION_MAX else text[:DESCRIPTION_MAX - 3] + '...'

    def slug(self):
        slug = re.sub(r'[^a-z0-9]+', '-', self.title.lower()).strip('-')
        return slug or 'game'


# --------------------------------------------------------------------------------------------------
# PNG and ICO decoding (zlib + struct only)
# --------------------------------------------------------------------------------------------------

_CHANNELS = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}
_MAX_RAW_BYTES = 1 << 29


def png_header(data):
    """(width, height, bit_depth, colour_type, interlace) of a PNG, or ImageError."""
    if data[:8] != PNG_SIGNATURE:
        raise ImageError('not a PNG (bad signature)')
    if len(data) < 33 or data[12:16] != b'IHDR':
        raise ImageError('PNG has no IHDR chunk')
    width, height, depth, colour, _compression, _filter, interlace = struct.unpack('>IIBBBBB', data[16:29])
    if colour not in _CHANNELS or depth not in (1, 2, 4, 8, 16) or not width or not height:
        raise ImageError('PNG has an invalid IHDR')
    return width, height, depth, colour, interlace


def _unfilter(raw, height, stride, bpp):
    rows = []
    prior = bytearray(stride)
    pos = 0
    for _ in range(height):
        kind = raw[pos]
        row = bytearray(raw[pos + 1:pos + 1 + stride])
        pos += 1 + stride
        if kind == 1:
            for i in range(bpp, stride):
                row[i] = (row[i] + row[i - bpp]) & 255
        elif kind == 2:
            for i in range(stride):
                row[i] = (row[i] + prior[i]) & 255
        elif kind == 3:
            for i in range(min(bpp, stride)):
                row[i] = (row[i] + (prior[i] >> 1)) & 255
            for i in range(bpp, stride):
                row[i] = (row[i] + ((row[i - bpp] + prior[i]) >> 1)) & 255
        elif kind == 4:
            for i in range(min(bpp, stride)):
                row[i] = (row[i] + prior[i]) & 255
            for i in range(bpp, stride):
                a, b, c = row[i - bpp], prior[i], prior[i - bpp]
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                row[i] = (row[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        elif kind != 0:
            raise ImageError(f'PNG uses unknown filter type {kind}')
        rows.append(row)
        prior = row
    return rows


def _unpack_samples(row, depth, count):
    """Samples of a 1/2/4-bit row."""
    out = []
    mask = (1 << depth) - 1
    for byte in row:
        for shift in range(8 - depth, -1, -depth):
            out.append((byte >> shift) & mask)
    return out[:count]


def decode_png(data):
    """Decode a non-interlaced PNG (grey, grey+alpha, RGB, RGBA or palette; 1-16 bits) to
    (width, height, RGBA bytes). Chunk CRCs are verified. Raises ImageError on anything else."""
    width, height, depth, colour, interlace = png_header(data)
    if interlace:
        raise ImageError('interlaced PNGs are not supported')
    if width * height > 1 << 27:
        raise ImageError(f'PNG is too large ({width}x{height})')
    pos, idat, palette, trns, seen_end = 8, [], None, None, False
    while pos + 12 <= len(data) and not seen_end:
        length, kind = struct.unpack('>I4s', data[pos:pos + 8])
        end = pos + 8 + length
        if end + 4 > len(data):
            raise ImageError('PNG is truncated')
        if zlib.crc32(data[pos + 4:end]) != struct.unpack('>I', data[end:end + 4])[0]:
            raise ImageError(f'PNG chunk {kind.decode("latin-1")} has a bad CRC')
        body = data[pos + 8:end]
        if kind == b'IDAT':
            idat.append(body)
        elif kind == b'PLTE':
            palette = body
        elif kind == b'tRNS':
            trns = body
        elif kind == b'IEND':
            seen_end = True
        pos = end + 4
    if not idat:
        raise ImageError('PNG has no image data')
    channels = _CHANNELS[colour]
    stride = (width * channels * depth + 7) // 8
    bpp = max(1, channels * depth // 8)
    expected = height * (stride + 1)
    if expected > _MAX_RAW_BYTES:
        raise ImageError(f'PNG is too large ({width}x{height})')
    try:
        raw = zlib.decompressobj().decompress(b''.join(idat), expected + 1)
    except zlib.error as error:
        raise ImageError(f'PNG image data is corrupt: {error}') from error
    if len(raw) != expected:
        raise ImageError('PNG image data has the wrong length')
    rows = _unfilter(raw, height, stride, bpp)
    return width, height, _to_rgba(rows, width, height, depth, colour, palette, trns)


def _to_rgba(rows, width, height, depth, colour, palette, trns):
    out = bytearray(width * height * 4)
    if colour == 3:
        if palette is None:
            raise ImageError('palette PNG has no PLTE chunk')
        entries = len(palette) // 3
        tables = [bytearray(256) for _ in range(4)]
        for index in range(256):
            if index < entries:
                tables[0][index], tables[1][index], tables[2][index] = palette[index * 3:index * 3 + 3]
            tables[3][index] = trns[index] if trns and index < len(trns) else 255
    for y, row in enumerate(rows):
        base = y * width * 4
        if depth < 8:
            samples = bytes(_unpack_samples(row, depth, width))
            if colour == 3:
                pixels = samples
            else:
                scale = 255 // ((1 << depth) - 1)
                pixels = bytes(v * scale for v in samples)
        elif depth == 16:
            pixels = bytes(row[0::2])
        else:
            pixels = bytes(row)
        span = slice(base, base + width * 4)
        if colour == 6:
            out[span] = pixels
        elif colour == 2:
            out[base:base + width * 4:4] = pixels[0::3]
            out[base + 1:base + width * 4:4] = pixels[1::3]
            out[base + 2:base + width * 4:4] = pixels[2::3]
            out[base + 3:base + width * 4:4] = b'\xff' * width
        elif colour == 0:
            for channel in range(3):
                out[base + channel:base + width * 4:4] = pixels
            out[base + 3:base + width * 4:4] = b'\xff' * width
        elif colour == 4:
            for channel in range(3):
                out[base + channel:base + width * 4:4] = pixels[0::2]
            out[base + 3:base + width * 4:4] = pixels[1::2]
        else:
            for channel in range(4):
                out[base + channel:base + width * 4:4] = pixels.translate(bytes(tables[channel]))
    return bytes(out)


@dataclasses.dataclass
class IcoFrame:
    """One directory entry of an .ico plus what its image header says."""

    index: int
    width: int
    height: int
    planes: int
    bpp: int
    size: int
    offset: int
    kind: str  # 'png' or 'bmp'
    header_w: int  # width the image header declares
    header_h: int  # height the image header declares (a BMP counts the AND mask: twice the icon)
    header_bpp: int


def parse_ico(data):
    """Parse an .ico directory with bounds checks; raises ImageError for anything malformed."""
    if len(data) < 6:
        raise ImageError('ICO is shorter than its 6-byte header')
    reserved, kind, count = struct.unpack_from('<HHH', data, 0)
    if reserved != 0 or kind != 1:
        raise ImageError(f'not an ICO file (reserved={reserved}, type={kind})')
    if not 1 <= count <= 128:
        raise ImageError(f'ICO declares {count} images')
    table_end = 6 + 16 * count
    if len(data) < table_end:
        raise ImageError('ICO directory is truncated')
    frames = []
    for index in range(count):
        w, h, _colours, _reserved, planes, bpp, size, offset = struct.unpack_from('<BBBBHHII', data, 6 + 16 * index)
        w, h = w or 256, h or 256
        if size < 16 or offset < table_end or offset + size > len(data):
            raise ImageError(f'ICO image {index} ({w}x{h}) lies outside the file')
        blob = data[offset:offset + size]
        if blob[:8] == PNG_SIGNATURE:
            try:
                hw, hh, depth, colour, _interlace = png_header(blob)
            except (ImageError, struct.error) as error:
                raise ImageError(f'ICO image {index} ({w}x{h}) has a broken PNG header: {error}') from error
            kind_name, header_bpp = 'png', depth * _CHANNELS[colour]
        else:
            head_size, hw, hh, _planes, header_bpp = struct.unpack_from('<IiiHH', blob, 0)
            if head_size < 40 or head_size > size:
                raise ImageError(f'ICO image {index} ({w}x{h}) has neither a PNG nor a BMP header')
            kind_name = 'bmp'
        frames.append(IcoFrame(index, w, h, planes, bpp, size, offset, kind_name, hw, hh, header_bpp))
    return frames


def decode_ico_frame(data, frame):
    """RGBA pixels of one .ico frame as (width, height, bytes). PNG frames and BMP frames of
    1, 4, 8, 24 and 32 bits are supported (BMP: BI_RGB only)."""
    blob = data[frame.offset:frame.offset + frame.size]
    if frame.kind == 'png':
        return decode_png(blob)
    head_size, width, height2, _planes, bit_count, compression, _size_image, _x, _y, colours_used, _imp = \
        struct.unpack_from('<IiiHHIIiiII', blob, 0)
    if compression != 0:
        raise ImageError(f'BMP frame {frame.index} uses compression {compression}')
    if bit_count not in (1, 4, 8, 24, 32):
        raise ImageError(f'BMP frame {frame.index} has {bit_count} bits per pixel')
    top_down = height2 < 0
    height2 = abs(height2)
    height = height2 // 2 if height2 >= 2 * frame.height or height2 == 2 * width else height2
    if width <= 0 or height <= 0 or width > 1024 or height > 1024:
        raise ImageError(f'BMP frame {frame.index} has an invalid size {width}x{height}')
    palette = b''
    if bit_count <= 8:
        entries = colours_used or (1 << bit_count)
        palette = blob[head_size:head_size + 4 * entries]
    xor_offset = head_size + len(palette)
    xor_stride = ((width * bit_count + 31) // 32) * 4
    and_offset = xor_offset + xor_stride * height
    and_stride = ((width + 31) // 32) * 4
    if len(blob) < xor_offset + xor_stride * height:
        raise ImageError(f'BMP frame {frame.index} pixel data is truncated')
    have_mask = len(blob) >= and_offset + and_stride * height
    out = bytearray(width * height * 4)
    any_alpha = False
    for y in range(height):
        src_row = y if top_down else height - 1 - y
        row = blob[xor_offset + src_row * xor_stride:xor_offset + (src_row + 1) * xor_stride]
        base = y * width * 4
        if bit_count == 32:
            out[base:base + width * 4:4] = row[2:width * 4:4]
            out[base + 1:base + width * 4:4] = row[1:width * 4:4]
            out[base + 2:base + width * 4:4] = row[0:width * 4:4]
            out[base + 3:base + width * 4:4] = row[3:width * 4:4]
            any_alpha = any_alpha or any(row[3:width * 4:4])
        elif bit_count == 24:
            out[base:base + width * 4:4] = row[2:width * 3:3]
            out[base + 1:base + width * 4:4] = row[1:width * 3:3]
            out[base + 2:base + width * 4:4] = row[0:width * 3:3]
            out[base + 3:base + width * 4:4] = b'\xff' * width
        else:
            indexes = row[:width] if bit_count == 8 else _unpack_samples(row, bit_count, width)
            for x, index in enumerate(indexes):
                b, g, r = palette[index * 4:index * 4 + 3] if index * 4 + 3 <= len(palette) else (0, 0, 0)
                out[base + x * 4:base + x * 4 + 4] = bytes((r, g, b, 255))
    if bit_count == 32 and not any_alpha:  # the alpha channel is unused: the AND mask alone decides
        out[3::4] = b'\xff' * (width * height)
    if have_mask and not any_alpha:  # AND mask: a set bit is transparent
        for y in range(height):
            src_row = y if top_down else height - 1 - y
            mask = blob[and_offset + src_row * and_stride:and_offset + (src_row + 1) * and_stride]
            for x in range(width):
                if mask[x >> 3] & (0x80 >> (x & 7)):
                    out[(y * width + x) * 4 + 3] = 0
    return width, height, bytes(out)


# --------------------------------------------------------------------------------------------------
# Perceptual signature
# --------------------------------------------------------------------------------------------------


def _axis_weights(source, target):
    """Box-filter weights mapping `source` samples onto `target` cells: [(first index, weights)]."""
    scale = source / target
    cells = []
    for t in range(target):
        low, high = t * scale, (t + 1) * scale
        first = int(low)
        last = min(source, int(math.ceil(high)))
        weights = [min(high, i + 1) - max(low, i) for i in range(first, last)] or [1.0]
        total = sum(weights)
        cells.append((min(first, source - 1), [w / total for w in weights]))
    return cells


def box_resample(values, width, height, size):
    """Area-average one plane (flat list, row-major) down or up to size x size."""
    xs, ys = _axis_weights(width, size), _axis_weights(height, size)
    if width == size:
        rows = [values[y * width:(y + 1) * width] for y in range(height)]
    elif width % size == 0:  # whole boxes: plain slice sums run at C speed
        step = width // size
        rows = [[sum(values[start:start + step]) / step for start in range(y * width, (y + 1) * width, step)]
                for y in range(height)]
    else:
        rows = []
        for y in range(height):
            row = values[y * width:(y + 1) * width]
            rows.append([sum(w * row[i + k] for k, w in enumerate(ws)) for i, ws in xs])
    out = []
    if height == size:
        for row in rows:
            out.extend(row)
    elif height % size == 0:
        step = height // size
        for y in range(0, height, step):
            out.extend(sum(column) / step for column in zip(*rows[y:y + step]))
    else:
        for i, ws in ys:
            out.extend(sum(w * v for w, v in zip(ws, column)) for column in zip(*rows[i:i + len(ws)]))
    return [float(v) for v in out]


_SHIFTS = tuple((dx, dy) for dy in (-1, 0, 1) for dx in (-1, 0, 1))  # index 4 is (0, 0)


@dataclasses.dataclass
class Signature:
    """A tiny perceptual fingerprint of an icon: shape (luminance average hash, also taken with the
    32x32 grid shifted by up to one pixel so a 1 px shift is not a new shape), colour (soft RGB
    histogram of the opaque pixels) and how much of the square the art covers."""

    hashes: tuple  # 9 average hashes, one per grid shift in _SHIFTS
    hist: tuple
    coverage: float  # share of the 32x32 grid that is at least half opaque

    @property
    def hash(self):
        """The unshifted 64-bit average hash."""
        return self.hashes[4]

    def to_dict(self):
        return {'hashes': list(self.hashes), 'hist': [round(v, 5) for v in self.hist],
                'coverage': round(self.coverage, 4)}

    @classmethod
    def from_dict(cls, data):
        return cls(tuple(int(h) for h in data['hashes']), tuple(data['hist']), float(data['coverage']))


def _soft_histogram(pixels):
    bins = HIST_BINS
    top = bins - 1
    hist = [0.0] * bins ** 3
    for r, g, b in pixels:
        axes = []
        for value in (r, g, b):
            position = min(max(value, 0.0), 255.0) / 255.0 * top
            low = min(int(position), top - 1)
            frac = position - low
            axes.append(((low, 1.0 - frac), (low + 1, frac)))
        for ri, rw in axes[0]:
            for gi, gw in axes[1]:
                for bi, bw in axes[2]:
                    weight = rw * gw * bw
                    if weight:
                        hist[(ri * bins + gi) * bins + bi] += weight
    total = sum(hist)
    return tuple(v / total for v in hist) if total else tuple(hist)


def make_signature(width, height, rgba):
    """Composite over mid-grey, box-resample to 32x32; 8x8 luminance average hash + 4x4x4 soft
    RGB histogram over the opaque pixels + opaque coverage."""
    if width <= 0 or height <= 0 or len(rgba) != width * height * 4:
        raise ImageError('image size does not match its pixel data')
    grid = SIG_GRID
    alpha = list(rgba[3::4])
    planes = [[c * a / 255.0 for c, a in zip(rgba[k::4], alpha)] for k in range(3)]
    red, green, blue = (box_resample(plane, width, height, grid) for plane in planes)
    cover = box_resample([float(a) for a in alpha], width, height, grid)
    luma = []
    for i in range(grid * grid):
        grey = 128.0 * (1.0 - cover[i] / 255.0)
        luma.append(0.299 * (red[i] + grey) + 0.587 * (green[i] + grey) + 0.114 * (blue[i] + grey))
    hashes = tuple(_average_hash(luma, grid, dx, dy) for dx, dy in _SHIFTS)
    opaque = [i for i in range(grid * grid) if cover[i] >= 127.5]
    hist = _soft_histogram((red[i] * 255.0 / cover[i], green[i] * 255.0 / cover[i], blue[i] * 255.0 / cover[i])
                           for i in opaque)
    return Signature(hashes, hist, len(opaque) / (grid * grid))


def _average_hash(luma, grid, dx, dy):
    """64-bit average hash of the HASH_GRID x HASH_GRID block means of a luminance grid read at an
    offset of (dx, dy) pixels (edge pixels repeat). A bit is set when its block is above the mean."""
    cell = grid // HASH_GRID
    blocks = []
    for by in range(HASH_GRID):
        for bx in range(HASH_GRID):
            total = 0.0
            for y in range(cell):
                row = min(max(by * cell + y - dy, 0), grid - 1) * grid
                for x in range(cell):
                    total += luma[row + min(max(bx * cell + x - dx, 0), grid - 1)]
            blocks.append(total / (cell * cell))
    mean = sum(blocks) / len(blocks)
    bits = 0
    for index, value in enumerate(blocks):
        if value > mean + 1e-9:
            bits |= 1 << index
    return bits


def signature_distance(a, b):
    """(shape, colour), both 0..1. shape = share of differing hash bits, taking the best of the 3x3
    one-pixel alignments (so a shifted copy is still the same shape); colour = half the L1
    distance between the histograms (total variation), 1.0 when either has no opaque pixels."""
    bits = min(min(bin(a.hash ^ h).count('1') for h in b.hashes),
               min(bin(b.hash ^ h).count('1') for h in a.hashes))
    shape = bits / (HASH_GRID * HASH_GRID)
    if not any(a.hist) or not any(b.hist):
        return shape, 1.0
    colour = 0.5 * sum(abs(x - y) for x, y in zip(a.hist, b.hist))
    return shape, colour


def too_similar(a, b):
    """Two icons a person would confuse: alike in shape AND in colour."""
    shape, colour = signature_distance(a, b)
    return shape <= SIMILAR_SHAPE and colour <= SIMILAR_COLOUR


def same_picture(a, b):
    """The same artwork, allowing for resampling and shell rendering differences."""
    return max(signature_distance(a, b)) <= EQUAL_MAX


def luma_stddev(width, height, rgba, composite=False, max_samples=250000):
    """Standard deviation of the luminance (0-255); `composite` blends over mid-grey first."""
    step = max(1, int(math.ceil(math.sqrt(width * height / max_samples))))
    total = total_sq = count = 0
    for y in range(0, height, step):
        row = rgba[y * width * 4:(y + 1) * width * 4]
        reds, greens, blues, alphas = row[0::4], row[1::4], row[2::4], row[3::4]
        for x in range(0, width, step):
            r, g, b = reds[x], greens[x], blues[x]
            if composite:
                a = alphas[x] / 255.0
                r, g, b = r * a + 128 * (1 - a), g * a + 128 * (1 - a), b * a + 128 * (1 - a)
            value = 0.299 * r + 0.587 * g + 0.114 * b
            total += value
            total_sq += value * value
            count += 1
    mean = total / count
    return math.sqrt(max(0.0, total_sq / count - mean * mean))


def count_colours(rgba, min_alpha=128):
    """Number of distinct RGB colours among the mostly opaque pixels."""
    alphas = rgba[3::4]
    colours = set()
    for r, g, b, a in zip(rgba[0::4], rgba[1::4], rgba[2::4], alphas):
        if a >= min_alpha:
            colours.add((r, g, b))
    return len(colours)


def opaque_share(rgba, min_alpha=128):
    """Share of pixels that are at least half opaque."""
    alphas = rgba[3::4]
    return sum(1 for a in alphas if a >= min_alpha) / max(1, len(alphas))


# --------------------------------------------------------------------------------------------------
# Windows shell helper (Windows PowerShell 5.1 + C# through Add-Type; no System.Drawing needed)
# --------------------------------------------------------------------------------------------------

# One script, several actions chosen by the "action" member of the JSON arguments file. Results are
# written as JSON to the "result" path (UTF-8), pixels as raw RGBA files next to it. The C# must stay
# C# 5 (the compiler behind Add-Type in Windows PowerShell 5.1).
WINDOWS_HELPER = r'''
param([Parameter(Mandatory = $true)][string]$ArgsFile)
$ErrorActionPreference = 'Stop'
$utf8 = New-Object System.Text.UTF8Encoding($false)
$cfg = [IO.File]::ReadAllText($ArgsFile, $utf8) | ConvertFrom-Json

function Save-Result($value) {
    [IO.File]::WriteAllText($cfg.result, ($value | ConvertTo-Json -Depth 10 -Compress), $utf8)
}

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;

[ComImport, Guid("00021401-0000-0000-C000-000000000046")] class ShellLinkObject { }

[ComImport, InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("000214F9-0000-0000-C000-000000000046")]
interface IShellLinkW
{
    void GetPath([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder file, int max, IntPtr findData, int flags);
    void GetIDList(out IntPtr pidl);
    void SetIDList(IntPtr pidl);
    void GetDescription([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder name, int max);
    void SetDescription([MarshalAs(UnmanagedType.LPWStr)] string name);
    void GetWorkingDirectory([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder dir, int max);
    void SetWorkingDirectory([MarshalAs(UnmanagedType.LPWStr)] string dir);
    void GetArguments([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder args, int max);
    void SetArguments([MarshalAs(UnmanagedType.LPWStr)] string args);
    void GetHotkey(out short hotkey);
    void SetHotkey(short hotkey);
    void GetShowCmd(out int showCmd);
    void SetShowCmd(int showCmd);
    void GetIconLocation([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder path, int max, out int index);
    void SetIconLocation([MarshalAs(UnmanagedType.LPWStr)] string path, int index);
    void SetRelativePath([MarshalAs(UnmanagedType.LPWStr)] string path, int reserved);
    void Resolve(IntPtr window, int flags);
    void SetPath([MarshalAs(UnmanagedType.LPWStr)] string file);
}

[ComImport, InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("0000010b-0000-0000-C000-000000000046")]
interface IPersistFileW
{
    void GetClassID(out Guid classId);
    [PreserveSig] int IsDirty();
    void Load([MarshalAs(UnmanagedType.LPWStr)] string file, int mode);
    void Save([MarshalAs(UnmanagedType.LPWStr)] string file, [MarshalAs(UnmanagedType.Bool)] bool remember);
    void SaveCompleted([MarshalAs(UnmanagedType.LPWStr)] string file);
    void GetCurFile([MarshalAs(UnmanagedType.LPWStr)] out string file);
}

public static class ShipWin
{
    // A shortcut's fields as {target, start in, icon file, icon index, tooltip, arguments, show command}.
    public static string[] ReadLink(string path)
    {
        object link = new ShellLinkObject();
        try
        {
            ((IPersistFileW)link).Load(path, 0);
            IShellLinkW shell = (IShellLinkW)link;
            StringBuilder target = new StringBuilder(2048), dir = new StringBuilder(2048), icon = new StringBuilder(2048);
            StringBuilder text = new StringBuilder(2048), args = new StringBuilder(2048);
            int index, show;
            shell.GetPath(target, target.Capacity, IntPtr.Zero, 0);
            shell.GetWorkingDirectory(dir, dir.Capacity);
            shell.GetIconLocation(icon, icon.Capacity, out index);
            shell.GetDescription(text, text.Capacity);
            shell.GetArguments(args, args.Capacity);
            shell.GetShowCmd(out show);
            return new string[] { target.ToString(), dir.ToString(), icon.ToString(), index.ToString(), text.ToString(), args.ToString(), show.ToString() };
        }
        finally { Marshal.ReleaseComObject(link); }
    }

    // Writes a shortcut through IShellLinkW/IPersistFile: every string stays Unicode (WScript.Shell would
    // squeeze paths and tooltips through the ANSI code page and fail on other characters).
    public static void WriteLink(string path, string targetPath, string workingDir, string iconPath, int iconIndex, string description, int showCmd)
    {
        object link = new ShellLinkObject();
        try
        {
            IShellLinkW shell = (IShellLinkW)link;
            shell.SetPath(targetPath);
            shell.SetArguments("");
            shell.SetWorkingDirectory(workingDir);
            if (iconPath.Length > 0) shell.SetIconLocation(iconPath, iconIndex);
            shell.SetDescription(description);
            shell.SetShowCmd(showCmd);
            ((IPersistFileW)link).Save(path, true);
        }
        finally { Marshal.ReleaseComObject(link); }
    }

    [StructLayout(LayoutKind.Sequential)] struct SIZE { public int cx, cy; }
    [StructLayout(LayoutKind.Sequential)] struct BITMAP { public int bmType, bmWidth, bmHeight, bmWidthBytes; public short bmPlanes, bmBitsPixel; public IntPtr bmBits; }
    [StructLayout(LayoutKind.Sequential)] struct BITMAPINFOHEADER { public int biSize, biWidth, biHeight; public short biPlanes, biBitCount; public int biCompression, biSizeImage, biXPelsPerMeter, biYPelsPerMeter, biClrUsed, biClrImportant; }
    [StructLayout(LayoutKind.Sequential)] struct BITMAPINFO { public BITMAPINFOHEADER h; public int c0; }
    [StructLayout(LayoutKind.Sequential)] struct DIBSECTION { public BITMAP dsBm; public BITMAPINFOHEADER dsBmih; public int b0, b1, b2; public IntPtr hSection; public int offset; }
    [StructLayout(LayoutKind.Sequential)] struct ICONINFO { public int fIcon, xHotspot, yHotspot; public IntPtr hbmMask, hbmColor; }
    delegate bool EnumProc(IntPtr hwnd, IntPtr lparam);

    [ComImport, Guid("bcc18b79-ba16-442f-80c4-8a59c30c463b"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IShellItemImageFactory { [PreserveSig] int GetImage(SIZE size, int flags, out IntPtr phbm); }
    [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = false)]
    static extern void SHCreateItemFromParsingName(string path, IntPtr pbc, ref Guid riid, [MarshalAs(UnmanagedType.Interface)] out IShellItemImageFactory ppv);
    [DllImport("gdi32.dll")] static extern int GetObject(IntPtr h, int count, out DIBSECTION ds);
    [DllImport("gdi32.dll")] static extern int GetObject(IntPtr h, int count, out BITMAP bm);
    [DllImport("gdi32.dll")] static extern int GetDIBits(IntPtr hdc, IntPtr hbm, uint start, uint lines, byte[] bits, ref BITMAPINFO bmi, uint usage);
    [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr h);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr l);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowTextW(IntPtr hwnd, StringBuilder sb, int max);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] static extern bool PostMessageW(IntPtr hwnd, uint msg, IntPtr w, IntPtr l);
    [DllImport("user32.dll", SetLastError = true)] static extern IntPtr SendMessageTimeoutW(IntPtr hwnd, uint msg, IntPtr w, IntPtr l, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll", EntryPoint = "GetClassLongPtrW")] static extern IntPtr GetClassLongPtr64(IntPtr hwnd, int index);
    [DllImport("user32.dll", EntryPoint = "GetClassLongW")] static extern uint GetClassLong32(IntPtr hwnd, int index);
    [DllImport("user32.dll")] static extern bool GetIconInfo(IntPtr icon, out ICONINFO info);
    [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr hwnd);
    [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr hwnd, IntPtr dc);
    [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern bool QueryFullProcessImageNameW(IntPtr h, uint flags, StringBuilder sb, ref uint size);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);

    // The icon the shell (Explorer) shows for a file, as straight-alpha RGBA rows, top row first.
    public static byte[] ShellRender(string path, int size, out int width, out int height)
    {
        Guid iid = new Guid("bcc18b79-ba16-442f-80c4-8a59c30c463b");
        IShellItemImageFactory factory;
        SHCreateItemFromParsingName(path, IntPtr.Zero, ref iid, out factory);
        IntPtr hbm;
        SIZE want = new SIZE(); want.cx = size; want.cy = size;
        int hr = factory.GetImage(want, 0x4, out hbm); // SIIGBF_ICONONLY
        if (hr != 0) throw new COMException("GetImage failed", hr);
        try
        {
            DIBSECTION ds;
            if (GetObject(hbm, Marshal.SizeOf(typeof(DIBSECTION)), out ds) == 0) throw new InvalidOperationException("GetObject failed");
            int w = ds.dsBm.bmWidth, h = Math.Abs(ds.dsBm.bmHeight), stride = ds.dsBm.bmWidthBytes;
            if (ds.dsBm.bmBitsPixel != 32) throw new InvalidOperationException("unexpected bit depth " + ds.dsBm.bmBitsPixel);
            bool topDown = ds.dsBmih.biHeight < 0;
            byte[] result = new byte[w * h * 4];
            byte[] row = new byte[w * 4];
            for (int y = 0; y < h; y++)
            {
                int srcY = topDown ? y : (h - 1 - y);
                Marshal.Copy(new IntPtr(ds.dsBm.bmBits.ToInt64() + (long)srcY * stride), row, 0, w * 4);
                for (int x = 0; x < w; x++)
                {
                    int b = row[x * 4], g = row[x * 4 + 1], r = row[x * 4 + 2], a = row[x * 4 + 3];
                    if (a > 0 && a < 255) { r = Math.Min(255, r * 255 / a); g = Math.Min(255, g * 255 / a); b = Math.Min(255, b * 255 / a); } // the shell hands back premultiplied alpha
                    int o = (y * w + x) * 4;
                    result[o] = (byte)r; result[o + 1] = (byte)g; result[o + 2] = (byte)b; result[o + 3] = (byte)a;
                }
            }
            width = w; height = h;
            return result;
        }
        finally { DeleteObject(hbm); }
    }

    // Process ids whose executable is exactly this file (works across 32/64-bit and without admin rights).
    public static int[] PidsByImage(string image)
    {
        List<int> found = new List<int>();
        string want = System.IO.Path.GetFullPath(image);
        foreach (Process p in Process.GetProcesses())
        {
            IntPtr h = OpenProcess(0x1000, false, (uint)p.Id); // PROCESS_QUERY_LIMITED_INFORMATION
            if (h == IntPtr.Zero) continue;
            try
            {
                StringBuilder sb = new StringBuilder(1024); uint n = 1024;
                if (QueryFullProcessImageNameW(h, 0, sb, ref n) && string.Equals(System.IO.Path.GetFullPath(sb.ToString()), want, StringComparison.OrdinalIgnoreCase)) found.Add(p.Id);
            }
            catch (Exception) { }
            finally { CloseHandle(h); }
        }
        return found.ToArray();
    }

    public static bool IsAlive(int pid)
    {
        try { return !Process.GetProcessById(pid).HasExited; } catch (Exception) { return false; }
    }

    public static long[] WindowsOf(int pid)
    {
        List<long> list = new List<long>();
        EnumWindows(delegate(IntPtr h, IntPtr l)
        {
            uint owner; GetWindowThreadProcessId(h, out owner);
            if (owner == (uint)pid && IsWindowVisible(h)) list.Add(h.ToInt64());
            return true;
        }, IntPtr.Zero);
        return list.ToArray();
    }

    public static string TitleOf(long hwnd)
    {
        StringBuilder sb = new StringBuilder(512);
        GetWindowTextW(new IntPtr(hwnd), sb, 512);
        return sb.ToString();
    }

    public static void PostClose(long hwnd) { PostMessageW(new IntPtr(hwnd), 0x0010, IntPtr.Zero, IntPtr.Zero); } // WM_CLOSE

    // which: 0 ICON_SMALL, 1 ICON_BIG, 2 ICON_SMALL2 (WM_GETICON); 100 class icon, 101 class small icon.
    public static byte[] WindowIcon(long hwndValue, int which, out int width, out int height)
    {
        IntPtr hwnd = new IntPtr(hwndValue);
        IntPtr icon;
        if (which == 100) icon = IntPtr.Size == 8 ? GetClassLongPtr64(hwnd, -14) : new IntPtr((int)GetClassLong32(hwnd, -14));
        else if (which == 101) icon = IntPtr.Size == 8 ? GetClassLongPtr64(hwnd, -34) : new IntPtr((int)GetClassLong32(hwnd, -34));
        else { IntPtr result; SendMessageTimeoutW(hwnd, 0x7F, new IntPtr(which), IntPtr.Zero, 0x3, 2000, out result); icon = result; } // WM_GETICON, SMTO_ABORTIFHUNG|SMTO_BLOCK
        width = 0; height = 0;
        if (icon == IntPtr.Zero) return null;
        return IconPixels(icon, out width, out height);
    }

    static byte[] IconPixels(IntPtr icon, out int width, out int height)
    {
        ICONINFO info;
        if (!GetIconInfo(icon, out info)) throw new InvalidOperationException("GetIconInfo failed");
        IntPtr dc = GetDC(IntPtr.Zero);
        try
        {
            bool colour = info.hbmColor != IntPtr.Zero;
            BITMAP bm;
            if (GetObject(colour ? info.hbmColor : info.hbmMask, Marshal.SizeOf(typeof(BITMAP)), out bm) == 0) throw new InvalidOperationException("GetObject failed");
            int w = bm.bmWidth, h = colour ? bm.bmHeight : bm.bmHeight / 2; // a monochrome icon stacks its AND and XOR masks
            byte[] pixels = new byte[w * h * 4];
            byte[] mask = new byte[w * (colour ? h : h * 2) * 4];
            BITMAPINFO bmi = new BITMAPINFO();
            bmi.h.biSize = 40; bmi.h.biWidth = w; bmi.h.biHeight = -h; bmi.h.biPlanes = 1; bmi.h.biBitCount = 32; bmi.h.biCompression = 0;
            if (colour && GetDIBits(dc, info.hbmColor, 0, (uint)h, pixels, ref bmi, 0) == 0) throw new InvalidOperationException("GetDIBits (colour) failed");
            if (info.hbmMask != IntPtr.Zero)
            {
                int rows = colour ? h : h * 2;
                bmi.h.biHeight = -rows;
                if (GetDIBits(dc, info.hbmMask, 0, (uint)rows, mask, ref bmi, 0) == 0) throw new InvalidOperationException("GetDIBits (mask) failed");
                if (!colour) Array.Copy(mask, w * h * 4, pixels, 0, pixels.Length);
            }
            bool anyAlpha = false;
            for (int i = 3; i < pixels.Length; i += 4) if (pixels[i] != 0) { anyAlpha = true; break; }
            byte[] rgba = new byte[w * h * 4];
            for (int i = 0; i < w * h; i++)
            {
                rgba[i * 4] = pixels[i * 4 + 2]; rgba[i * 4 + 1] = pixels[i * 4 + 1]; rgba[i * 4 + 2] = pixels[i * 4];
                rgba[i * 4 + 3] = anyAlpha ? pixels[i * 4 + 3] : (byte)(mask[i * 4] > 127 ? 0 : 255); // AND mask: white is transparent
            }
            width = w; height = h;
            return rgba;
        }
        finally
        {
            ReleaseDC(IntPtr.Zero, dc);
            if (info.hbmColor != IntPtr.Zero) DeleteObject(info.hbmColor);
            if (info.hbmMask != IntPtr.Zero) DeleteObject(info.hbmMask);
        }
    }
}
'@

function Read-Shortcut([string]$path) {
    $f = [ShipWin]::ReadLink($path)
    [ordered]@{
        path = $path; exists = [bool](Test-Path -LiteralPath $path)
        target = $f[0]; working_dir = $f[1]; icon_location = ('{0},{1}' -f $f[2], $f[3]); description = $f[4]
        arguments = $f[5]; window_style = [int]$f[6]
    }
}

function Read-VersionInfo([string]$path) {
    $v = [Diagnostics.FileVersionInfo]::GetVersionInfo($path)
    [ordered]@{
        file_description = [string]$v.FileDescription; product_name = [string]$v.ProductName
        file_version = [string]$v.FileVersion; product_version = [string]$v.ProductVersion
        internal_name = [string]$v.InternalName; original_filename = [string]$v.OriginalFilename
        comments = [string]$v.Comments }
}

function Invoke-Render {
    $items = New-Object System.Collections.ArrayList
    foreach ($item in $cfg.items) {
        $entry = [ordered]@{ path = $item.path; ok = $false; error = $null; target = $null; icon_location = $null
                             shortcut = $null; version = $null; files = [ordered]@{} }
        if ($item.path -like '*.lnk') {
            try {
                $entry.shortcut = Read-Shortcut $item.path
                $entry.target = $entry.shortcut.target
                $entry.icon_location = $entry.shortcut.icon_location
            } catch { $entry.error = $_.Exception.Message }
        } elseif ($item.path -like '*.exe') {
            try { $entry.version = Read-VersionInfo $item.path } catch { }
        }
        foreach ($size in $item.sizes) {
            try {
                $w = 0; $h = 0
                $bytes = [ShipWin]::ShellRender([string]$item.path, [int]$size, [ref]$w, [ref]$h)
                $file = '{0}_{1}.rgba' -f $item.out, $size
                [IO.File]::WriteAllBytes($file, $bytes)
                $entry.files["$size"] = [ordered]@{ file = $file; width = $w; height = $h }
                $entry.ok = $true
                $entry.error = $null
            } catch { $entry.error = $_.Exception.Message }
        }
        [void]$items.Add($entry)
    }
    Save-Result @{ items = $items.ToArray() }
}

function Invoke-Launch {
    $exe = [IO.Path]::GetFullPath($cfg.exe)
    $result = [ordered]@{ started = $false; pids = @(); window = $null; icons = [ordered]@{}; closed = $null; error = $null }
    $before = New-Object 'System.Collections.Generic.HashSet[int]'
    foreach ($p in [ShipWin]::PidsByImage($exe)) { [void]$before.Add([int]$p) }
    $tracked = New-Object 'System.Collections.Generic.List[int]'
    $refresh = {
        foreach ($p in [ShipWin]::PidsByImage($exe)) {
            if (-not $before.Contains([int]$p) -and -not $tracked.Contains([int]$p)) {
                $tracked.Add([int]$p)
                [IO.File]::WriteAllText($cfg.state, (@{ pids = $tracked.ToArray() } | ConvertTo-Json -Compress), $utf8)
            }
        }
    }
    try {
        $start = New-Object System.Diagnostics.ProcessStartInfo
        $start.FileName = [string]$cfg.lnk
        $start.UseShellExecute = $true
        [void][System.Diagnostics.Process]::Start($start)
        $result.started = $true
        $deadline = [DateTime]::UtcNow.AddSeconds([double]$cfg.timeout)
        $window = $null; $candidate = $null; $candidateSince = $null
        while ([DateTime]::UtcNow -lt $deadline) {
            & $refresh
            foreach ($p in $tracked.ToArray()) {
                foreach ($h in [ShipWin]::WindowsOf([int]$p)) {
                    $title = [ShipWin]::TitleOf([long]$h)
                    if ($title.Length -eq 0) { continue }
                    if ($title -ceq [string]$cfg.title) { $window = @{ pid = $p; hwnd = [long]$h; title = $title }; break }
                    if ($null -eq $candidate) { $candidate = @{ pid = $p; hwnd = [long]$h; title = $title }; $candidateSince = [DateTime]::UtcNow }
                }
                if ($null -ne $window) { break }
            }
            if ($null -ne $window) { break }
            if ($null -ne $candidate -and ([DateTime]::UtcNow - $candidateSince).TotalSeconds -gt 3) { break }
            Start-Sleep -Milliseconds 100
        }
        if ($null -eq $window) { $window = $candidate }
        if ($null -eq $window) {
            $result.error = if ($tracked.Count -eq 0) { 'the game process never appeared' } else { 'the game process appeared but showed no titled window' }
        } else {
            $result.window = [ordered]@{ pid = $window.pid; hwnd = $window.hwnd; title = $window.title }
            $iconDeadline = [DateTime]::UtcNow.AddSeconds(3)
            foreach ($spec in @(@('big', 1), @('small', 0), @('class', 100))) {
                $which = [int]$spec[1]
                $w = 0; $h = 0
                do {
                    $bytes = [ShipWin]::WindowIcon([long]$window.hwnd, $which, [ref]$w, [ref]$h)
                    if ($null -ne $bytes -or $which -eq 100) { break }
                    Start-Sleep -Milliseconds 100
                } while ([DateTime]::UtcNow -lt $iconDeadline)
                if ($null -ne $bytes) {
                    $file = '{0}_{1}.rgba' -f $cfg.out, $spec[0]
                    [IO.File]::WriteAllBytes($file, $bytes)
                    $result.icons[[string]$spec[0]] = [ordered]@{ file = $file; width = $w; height = $h }
                }
            }
            [ShipWin]::PostClose([long]$window.hwnd)
            $closeDeadline = [DateTime]::UtcNow.AddSeconds(3)
            $alive = $true
            while ($alive -and [DateTime]::UtcNow -lt $closeDeadline) {
                $alive = $false
                foreach ($p in $tracked.ToArray()) { if ([ShipWin]::IsAlive([int]$p)) { $alive = $true } }
                if ($alive) { Start-Sleep -Milliseconds 100 }
            }
            $result.closed = if ($alive) { 'killed' } else { 'graceful' }
        }
    } catch {
        $result.error = $_.Exception.Message
    } finally {
        & $refresh
        foreach ($p in $tracked.ToArray()) {
            if ([ShipWin]::IsAlive([int]$p)) { try { Stop-Process -Id $p -Force -ErrorAction Stop } catch { } }
        }
        $result.pids = [int[]]$tracked.ToArray()
    }
    Save-Result $result
}

try {
    switch ($cfg.action) {
        'shortcut-read' { Save-Result (Read-Shortcut $cfg.path) }
        'shortcut-write' {
            $location = [string]$cfg.icon_location
            $comma = $location.LastIndexOf(',')
            $iconPath = if ($comma -ge 0) { $location.Substring(0, $comma) } else { $location }
            $iconIndex = if ($comma -ge 0) { [int]$location.Substring($comma + 1) } else { 0 }
            [ShipWin]::WriteLink([string]$cfg.path, [string]$cfg.target, [string]$cfg.working_dir, $iconPath, $iconIndex,
                                 [string]$cfg.description, [int]$cfg.window_style)
            Save-Result (Read-Shortcut $cfg.path)
        }
        'render' { Invoke-Render }
        'exe-info' { Save-Result (Read-VersionInfo $cfg.path) }
        'desktop' {
            Save-Result ([ordered]@{ desktop = [Environment]::GetFolderPath('Desktop'); common = [Environment]::GetFolderPath('CommonDesktopDirectory') })
        }
        'launch' { Invoke-Launch }
        default { throw "unknown action $($cfg.action)" }
    }
} catch {
    Save-Result ([ordered]@{ error = $_.Exception.Message })
    exit 1
}
'''


def powershell_exe():
    """Windows PowerShell 5.1 (powershell.exe, not pwsh: System.Drawing-free but COM/Add-Type safe)."""
    found = shutil.which('powershell.exe')
    if found:
        return found
    candidate = Path(os.environ.get('SystemRoot', r'C:\Windows')) / 'System32' / 'WindowsPowerShell' / 'v1.0' / 'powershell.exe'
    return str(candidate) if candidate.is_file() else None


def _kill_left_behind(state_file, image_name):
    """Best effort after a helper timeout: kill the game processes it recorded (pid AND image name)."""
    state = read_json_file(state_file) or {}
    pids = state.get('pids')
    for pid in ([pids] if isinstance(pids, int) else pids or []):
        try:
            run_process(['taskkill', '/F', '/T', '/FI', f'PID eq {int(pid)}', '/FI', f'IMAGENAME eq {image_name}'],
                        timeout=30)
        except (OSError, ValueError, subprocess.SubprocessError):
            pass


def run_windows_helper(action, arguments, timeout=180, image_name=None):
    """Run one action of the embedded PowerShell helper and return its JSON result (a dict).
    Parameters travel in a UTF-8 JSON file, never on the command line."""
    if host_platform() != 'windows':
        raise ShipError('this step needs Windows')
    exe = powershell_exe()
    if not exe:
        raise ShipError('powershell.exe (Windows PowerShell 5.1) was not found')
    with tempfile.TemporaryDirectory(prefix='blue-ship-', ignore_cleanup_errors=True) as directory:
        work = Path(directory)
        script, args_file, result_file, state_file = (work / n for n in ('ship.ps1', 'args.json', 'result.json',
                                                                        'state.json'))
        script.write_text(WINDOWS_HELPER, encoding='utf-8-sig')
        payload = dict(arguments, action=action, result=str(result_file), state=str(state_file), workdir=str(work))
        args_file.write_text(json.dumps(payload, ensure_ascii=False), encoding='utf-8')
        command = [exe, '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', str(script),
                   str(args_file)]
        try:
            done = subprocess.run(command, capture_output=True, text=True, encoding='utf-8', errors='replace',
                                  timeout=timeout, creationflags=NO_WINDOW)
        except subprocess.TimeoutExpired as error:
            if image_name:
                _kill_left_behind(state_file, image_name)
            raise ShipError(f'the Windows helper ({action}) timed out after {timeout} s') from error
        try:
            result = json.loads(result_file.read_text(encoding='utf-8-sig'))
        except (OSError, ValueError) as error:
            tail = shorten((done.stderr or done.stdout or '').strip(), 400)
            raise ShipError(f'the Windows helper ({action}) failed (exit {done.returncode}): {tail}') from error
        if isinstance(result, dict) and result.get('error') and done.returncode not in (0, None) and len(result) == 1:
            raise ShipError(f'the Windows helper ({action}) failed: {result["error"]}')
        # Load pixel files while the temp folder still exists.
        _load_pixels(result)
        return result


def _load_pixels(node):
    """Replace {"file": path, "width", "height"} members by {"width", "height", "rgba": bytes}."""
    if isinstance(node, dict):
        if 'file' in node and 'width' in node and 'height' in node:
            try:
                node['rgba'] = Path(node.pop('file')).read_bytes()
            except OSError:
                node['rgba'] = None
            return
        for value in node.values():
            _load_pixels(value)
    elif isinstance(node, list):
        for value in node:
            _load_pixels(value)


def as_list(value):
    """PowerShell's ConvertTo-Json turns one-element arrays into bare values."""
    if value is None:
        return []
    return value if isinstance(value, list) else [value]


def shell_render(paths, sizes=(32,)):
    """Render icons the way Explorer does (IShellItemImageFactory, icons only) for many files in
    one PowerShell run. The same run reads every .lnk back ("shortcut": target, start-in, icon,
    tooltip) and the version info of every .exe ("version"). Returns {path: {"ok", "error", "target",
    "icon_location", "shortcut", "version", "images": {size: (width, height, rgba)}}}. Never raises
    for a single bad file."""
    paths = [str(p) for p in paths]
    if not paths:
        return {}
    with tempfile.TemporaryDirectory(prefix='blue-render-', ignore_cleanup_errors=True) as directory:
        items = [{'path': p, 'sizes': list(sizes), 'out': str(Path(directory) / f'r{i}')} for i, p in enumerate(paths)]
        result = run_windows_helper('render', {'items': items}, timeout=max(120, 4 * len(paths) + 60))
    rendered = {}
    for entry in as_list(result.get('items')):
        images = {}
        for size, info in (entry.get('files') or {}).items():
            if info.get('rgba'):
                images[int(size)] = (info['width'], info['height'], info['rgba'])
        rendered[entry['path']] = {'ok': bool(entry.get('ok')) and bool(images), 'error': entry.get('error'),
                                   'target': entry.get('target'), 'icon_location': entry.get('icon_location'),
                                   'shortcut': entry.get('shortcut'), 'version': entry.get('version'),
                                   'images': images}
    return rendered


def read_shortcut(path):
    """Read a .lnk back through WScript.Shell (target, start-in, icon, tooltip)."""
    return run_windows_helper('shortcut-read', {'path': str(path)}, timeout=90)


def write_shortcut(path, target, working_dir, icon_location, description, window_style=1):
    return run_windows_helper('shortcut-write', {
        'path': str(path), 'target': str(target), 'working_dir': str(working_dir),
        'icon_location': str(icon_location), 'description': description, 'window_style': window_style}, timeout=90)


def exe_version_info(path):
    return run_windows_helper('exe-info', {'path': str(path)}, timeout=90)


def launch_through_shortcut(lnk, exe, title, timeout=None):
    """Start the game through its shortcut, find its window and read the window icon, close it.
    Returns the helper's dict; each icon is {"width", "height", "rgba"}."""
    timeout = LAUNCH_TIMEOUT if timeout is None else timeout
    image_name = Path(exe).name
    with tempfile.TemporaryDirectory(prefix='blue-launch-', ignore_cleanup_errors=True) as directory:
        result = run_windows_helper('launch', {'lnk': str(lnk), 'exe': str(exe), 'title': title, 'timeout': timeout,
                                               'out': str(Path(directory) / 'window')},
                                    timeout=timeout + 90, image_name=image_name)
    return result


# --------------------------------------------------------------------------------------------------
# Desktop folders
# --------------------------------------------------------------------------------------------------


def _known_folder(folder_id):
    """SHGetKnownFolderPath through ctypes: honours redirected desktops, no process spawn."""
    import ctypes  # noqa: PLC0415

    class Guid(ctypes.Structure):
        _fields_ = [('a', ctypes.c_uint32), ('b', ctypes.c_uint16), ('c', ctypes.c_uint16), ('d', ctypes.c_ubyte * 8)]

    import uuid  # noqa: PLC0415
    raw = uuid.UUID(folder_id)
    guid = Guid(raw.time_low, raw.time_mid, raw.time_hi_version,
                (ctypes.c_ubyte * 8)(*(raw.bytes[8:])))
    path = ctypes.c_void_p()
    shell32 = ctypes.WinDLL('shell32')
    shell32.SHGetKnownFolderPath.argtypes = [ctypes.POINTER(Guid), ctypes.c_uint32, ctypes.c_void_p,
                                             ctypes.POINTER(ctypes.c_void_p)]
    shell32.SHGetKnownFolderPath.restype = ctypes.c_long
    try:
        if shell32.SHGetKnownFolderPath(ctypes.byref(guid), 0, None, ctypes.byref(path)) != 0:
            return None
        return ctypes.wstring_at(path.value)
    finally:
        if path.value:
            ole32 = ctypes.WinDLL('ole32')
            ole32.CoTaskMemFree.argtypes = [ctypes.c_void_p]
            ole32.CoTaskMemFree(path)


def windows_desktops():
    """(user desktop, common desktop) as the OS reports them; either may be None."""
    user = common = None
    try:
        user = _known_folder('B4BFCC3A-DB2C-424C-B029-7FE99A87C641')
        common = _known_folder('C4AA340D-F20F-4863-AFEF-F87EF2E6BA25')
    except (OSError, ValueError, AttributeError):
        user = common = None
    if not user:  # ctypes failed: ask PowerShell the documented way
        try:
            answer = run_windows_helper('desktop', {}, timeout=60)
            user, common = answer.get('desktop') or None, answer.get('common') or None
        except ShipError:
            user = None
    return user, common


def _existing_folder(value):
    return Path(value) if value and Path(value).is_dir() else None


def desktop_folders(platform_name=None):
    """{"user": Path or None, "common": Path or None} for this machine's desktop, from the OS. Asking for
    another platform's desktop gives nothing: that never touches the OS (use --folder)."""
    platform_name = platform_name or host_platform()
    user = common = None
    if platform_name == host_platform():
        if platform_name == 'windows':
            user, common = windows_desktops()
        elif platform_name == 'macos':
            user = Path.home() / 'Desktop'
        elif platform_name == 'linux':
            user = _linux_desktop()
    return {'user': _existing_folder(user), 'common': _existing_folder(common)}


def _linux_desktop():
    if shutil.which('xdg-user-dir'):
        try:
            answer = run_process(['xdg-user-dir', 'DESKTOP'], timeout=10).stdout.strip()
        except (OSError, subprocess.SubprocessError):
            answer = ''
        if answer and Path(answer) != Path.home():
            return answer
    config = Path(os.environ.get('XDG_CONFIG_HOME') or Path.home() / '.config') / 'user-dirs.dirs'
    try:
        for line in config.read_text(encoding='utf-8').splitlines():
            match = re.match(r'\s*XDG_DESKTOP_DIR\s*=\s*"?([^"]*)"?', line)
            if match:
                value = match.group(1).replace('$HOME', str(Path.home()))
                return value if Path(value) != Path.home() else None
    except OSError:
        pass
    fallback = Path.home() / 'Desktop'
    return str(fallback) if fallback.is_dir() else None


def has_display():
    """True when a window can be opened (Windows: a screen exists; Linux: DISPLAY/WAYLAND_DISPLAY)."""
    name = host_platform()
    if name == 'windows':
        try:
            import ctypes  # noqa: PLC0415
            return ctypes.windll.user32.GetSystemMetrics(0) > 0
        except (OSError, AttributeError):
            return False
    if name == 'linux':
        return bool(os.environ.get('DISPLAY') or os.environ.get('WAYLAND_DISPLAY'))
    return name == 'macos'


# --------------------------------------------------------------------------------------------------
# The game project
# --------------------------------------------------------------------------------------------------

# miniquad 0.4's built-in window icon (its 32x32 frame, zlib + base64): what a window shows when the
# game forgot to pass its own icon. Used to recognise "still the default logo" in the launch check.
MINIQUAD_LOGO_32 = (
    'eNrtll9Mm1UYxqG00HYUKXRbOyjQ/6zSFdoVupaxjNoW0P1JXI1jAtNEss07M5aoV2Zu2Y3xwmVBExNjpotOk83tQsmMoFBGoSzG'
    'f4l6Y3bjACEuJl4+Pqf52iFbO/mqxotdPOnX9pzze877vuc936RGg8kHeqD/sVLULDVNTf3XfK0Wr5eX42xFBT5Wq/GF5CVNzVEz'
    'VPJf8iXWHddqYCktRUlJCR6inHyOlJVhWKXEGfo6T19j9DUteUpL/q7/A57Empe5tk7i30sqysj//QoFHlcqcUKlwjl6usR5E6u8'
    'yM37WXUFSvOwC0lH2RSl2EVfo/QzI4MvYjnCGJfI4K/WafLnZPDnWHsJ5roYtpp6n7lYbw5E7UyRHyiQ+7+jes6/JtXyemt/jDIW'
    'yQ9zfpL7WO9ZEPXyLuNWUWTu+5VlSGtl5J4SdVNs7b3A+k3LqL15aphnuRi2gnqTMUzJqL0Z5izOs1sMv5q5vyr1xvXW3gT5zXlq'
    'r6lSgx2b9fftS1s5X6yTlNF3xV1TtYavLVPg1HYnbvZ34/tEF6rLlQX5vYxfSkbti3yNsu8qVvcR9tIPIj788UwPlgej+GUgCn+t'
    'riD/KOtnXmbfPb6m777Uas+wbz31SEa3D8cxYDMV5J+R2XfFnTVC7wbGX8TAUKHCt4ldmX1n+b+Tf5K5yMcWfeOCjL67+gx8Rp1X'
    'qnDRVoeloXiOnd3/e8xHPn5dgb4r1r5ObzOMz/3OwbSqHD/6nFg8/Ff+EmPx9d4QjjRsQqTOAKtOC63yzl3lz9N3k+SOVVVhxL0V'
    'b20x8f1KumvyePiS/O/aHHfxhRYORXCbdbgyFMPPB7sxuz+Mc8YaPKsow0nmL6VR55hT0rPY89smIyI7w4h1hpDUV2JSLd6z7vYq'
    'vk9w/A2HGYtkrOXnfEjxWObnjc21+Iq1+4bZhg9rN5FXjks1Nfi8shJTUsxP2W3oJj8e3oErJj32NzTguMmElFqdY4v8TFOnrVZc'
    'NG/BrQNdWFhVf2v121AUV/fuxGB9HSY2bMAhXxeOeIK8wzU42roNr1qaMEtfYu3ntnky++8h/wmnBUZXM8LkXOO8pOThI50Ol2v0'
    '6OOYly0W/OCxZuK8cA/2Cn3dZC72tLXAarPhxUYb+oI9iAfjGK1vQt92H/YFO3BFr8c4GQc6ApnYC77b5YTD6YSVes1gQFq8zzJW'
    '8cZGHGt2IUqf/Zz/CWvm01gQi4MxLA/cYf/K558ORvB0uxcNdgfsDge8LW2IhXoRCfVh2NsOv9OBjvYAjjEO7zD3j4WCmdgLvmA7'
    'qUaXC/1mM+ZFzDduRCN/83u96JV8Pv+wG3s62nEi2omZRDeWyBX65sk49gWCMNvsmXWy6grsRiz8KKK+Trjsdng4X+w5wb33kt8j'
    '8bPj7YLHcReqq9HNeFv4vcXtzo0Tc4XnUDCIV2KdmZivkD+e6EFriw827ju7lsNhh8/jR5z7b/X4MjERv+8mOy7tJ6s/AQVIrRs='
)


class Project:
    """Everything the ship commands need to know about one game checkout."""

    def __init__(self, root, platform_name=None):
        self.root = Path(root).resolve()
        self.platform = platform_name or host_platform()
        self.cargo = read_cargo(self.root)
        self.assets = self.root / 'assets'
        self.dist = self.root / 'dist'
        self.identity_path = self.assets / 'identity.json'
        self._identity = None
        self._identity_error = None
        self._exe_stem = None

    # ---- identity ----
    def raw_identity(self):
        """The decoded identity.json (None when missing or not JSON)."""
        return read_json_file(self.identity_path)

    @property
    def identity(self):
        if self._identity is None:
            if self._identity_error:
                raise ConfigError(self._identity_error)
            if not self.identity_path.is_file():
                self._identity_error = 'assets/identity.json is missing (title, tagline, controls)'
                raise ConfigError(self._identity_error)
            data = self.raw_identity()
            if data is None:
                self._identity_error = 'assets/identity.json is not valid JSON'
                raise ConfigError(self._identity_error)
            try:
                self._identity = Identity.from_dict(data)
            except ConfigError as error:
                self._identity_error = str(error)
                raise
        return self._identity

    def exe_stem(self):
        """The executable's file stem: identity "exe" when it is valid, else from Cargo.toml."""
        if self._exe_stem is None:
            data = self.raw_identity()
            exe = data.get('exe') if isinstance(data, dict) else None
            self._exe_stem = resolve_exe_stem(exe if isinstance(exe, str) and not exe_problems(exe) else None,
                                              self.cargo)
        return self._exe_stem

    # ---- files ----
    def exe_file(self, platform_name=None):
        return self.exe_stem() + ('.exe' if (platform_name or self.platform) == 'windows' else '')

    @property
    def dist_exe(self):
        return self.dist / self.exe_file()

    @property
    def dist_ico(self):
        return self.dist / f'{self.exe_stem()}.ico'

    @property
    def dist_png(self):
        return self.dist / f'{self.exe_stem()}.png'

    @property
    def stamp_path(self):
        return self.dist / STAMP_NAME

    def read_stamp(self):
        stamp = read_json_file(self.stamp_path)
        return stamp if isinstance(stamp, dict) else None

    def write_stamp(self, stamp):
        self.dist.mkdir(parents=True, exist_ok=True)
        write_json_atomic(self.stamp_path, stamp)

    def engine(self):
        return find_engine_path(self.root, self.cargo)

    def display(self, path):
        """A path relative to the project when it lives inside it (forward slashes)."""
        try:
            return Path(path).resolve().relative_to(self.root).as_posix()
        except (ValueError, OSError):
            return str(path)


def load_project(root, platform_name=None):
    root = Path(root)
    if not (root / 'Cargo.toml').is_file():
        raise ConfigError(f'{root} is not a game project (no Cargo.toml); run scripts/ship.py from a game checkout')
    return Project(root, platform_name)


# --------------------------------------------------------------------------------------------------
# package
# --------------------------------------------------------------------------------------------------


def cargo_target_directory(root):
    """Where cargo puts build output: CARGO_TARGET_DIR, else `cargo metadata`, else ./target."""
    env = os.environ.get('CARGO_TARGET_DIR')
    if env:
        return (Path(root) / env).resolve()
    cargo = shutil.which('cargo')
    if cargo:
        try:
            done = run_process([cargo, 'metadata', '--format-version', '1', '--no-deps'], cwd=root, timeout=120)
            if done.returncode == 0:
                directory = json.loads(done.stdout).get('target_directory')
                if directory:
                    return Path(directory)
        except (OSError, ValueError, subprocess.SubprocessError):
            pass
    return Path(root) / 'target'


def locate_built_exe(project):
    exe = cargo_target_directory(project.root) / 'release' / project.exe_file()
    if not exe.is_file():
        raise ShipError(f'{exe} does not exist: build it first (drop --no-build): cargo build --release')
    return exe


def cargo_build_release(project):
    """`cargo build --release`; returns the built executable named by cargo itself, so a redirected
    CARGO_TARGET_DIR or build.target-dir needs no guessing."""
    cargo = shutil.which('cargo')
    if not cargo:
        raise ShipError('cargo was not found on PATH (use --no-build to package an existing release build)')
    log('cargo build --release ...')
    done = subprocess.run([cargo, 'build', '--release', '--message-format=json-render-diagnostics'],
                          cwd=project.root, stdout=subprocess.PIPE, text=True, encoding='utf-8', errors='replace',
                          creationflags=NO_WINDOW)
    if done.returncode != 0:
        raise ShipError(f'cargo build --release failed (exit {done.returncode}); the compiler messages are above')
    stem, built = project.exe_stem(), None
    for line in done.stdout.splitlines():
        try:
            message = json.loads(line)
        except ValueError:
            continue
        target = message.get('target') or {}
        if (message.get('reason') == 'compiler-artifact' and message.get('executable') and 'bin' in target.get('kind', [])
                and target.get('name') == stem):
            built = Path(message['executable'])
    if built is None or not built.is_file():
        built = locate_built_exe(project)
    return built


def _copy_file(source, destination):
    """Copy a file (with its modification time) by renaming a finished temp copy over the target."""
    destination.parent.mkdir(parents=True, exist_ok=True)
    temp = destination.with_name(f'.{destination.name}.{os.getpid()}.tmp')
    try:
        shutil.copy2(source, temp)
        os.replace(temp, destination)
    except PermissionError as error:
        temp.unlink(missing_ok=True)
        raise ShipError(f'cannot replace {destination}: it is in use (close the game and run again)') from error
    except OSError:
        temp.unlink(missing_ok=True)
        raise


def require_icon(project):
    icon = project.assets / 'icon.ico'
    if not icon.is_file():
        raise ConfigError('assets/icon.ico is missing: generate the icon first, e.g. '
                          f'be2-tools icon "{project.identity.title}" assets')
    return icon


def package_plan(project, built_exe):
    """[(source, destination relative to dist as a posix string)] for everything that ships."""
    icon = require_icon(project)
    stem = project.exe_stem()
    plan = [(built_exe, project.exe_file()), (icon, f'{stem}.ico')]
    png = project.assets / 'icon.png'
    if png.is_file():
        plan.append((png, f'{stem}.png'))
    for entry in project.identity.package:
        source = project.root / entry
        if source.is_symlink() or any(p.is_symlink() for p in source.parents if inside(p, project.root)):
            raise ConfigError(f'package entry {entry!r} is a symlink: ship regular files inside the project')
        source = source.resolve()
        if not inside(source, project.root):
            raise ConfigError(f'package entry {entry!r} leaves the project')
        if source.is_dir():
            children = sorted(p for p in source.rglob('*') if p.is_file())
            plan.extend((child, child.relative_to(project.root).as_posix()) for child in children)
        elif source.is_file():
            plan.append((source, entry))
        else:
            raise ConfigError(f'package entry {entry!r} does not exist in the project')
    seen = {STAMP_NAME.lower()}
    for source, name in plan:
        if source.is_symlink() or any(p.is_symlink() for p in source.parents if inside(p, project.root)):
            raise ConfigError(f'package source {source} is a symlink: ship regular files inside the project')
        package_file(project.dist, name)  # validate names and existing destination parents before copying
        if name.lower() in seen:
            raise ConfigError(f'{name} would be packaged twice or collides with a file this tool writes')
        seen.add(name.lower())
    return plan


def package_file(directory, name):
    """A portable regular-file path; never follow a package symlink, even one pointing inside dist."""
    if not isinstance(name, str) or not name or '\\' in name:
        raise ShipError(f'unsafe package file {name!r}: use a relative forward-slash path')
    parts = name.split('/')
    if any(p in ('', '.', '..') or p.rstrip(' .') != p or
           re.search(r'[:*?"<>|\x00-\x1f]', p) or p.split('.')[0].lower() in WINDOWS_RESERVED for p in parts):
        raise ShipError(f'unsafe package file {name!r}: paths must stay inside the package on every platform')
    target = Path(directory)
    if target.is_symlink():
        raise ShipError(f'unsafe package directory {target}: symlinks are not packaged')
    for part in parts:
        target = target / part
        if target.is_symlink():
            raise ShipError(f'unsafe package file {name!r}: symlinks are not packaged')
    return target


def package_integrity(project, plat=None):
    """Validate the shipping manifest, paths and every hash without a display or launching the game.

    Extra player files may remain in dist, but are never used by the isolated smoke run. A manifest
    is integrity evidence, not a signature: a party able to rewrite files AND hashes is trusted.
    """
    stamp = project.read_stamp()
    if not isinstance(stamp, dict):
        raise ShipError(f'dist/{STAMP_NAME} is missing or invalid: run python scripts/ship.py package')
    files, hashes = stamp.get('files'), stamp.get('file_sha256')
    if not isinstance(files, list) or not files or not all(isinstance(n, str) for n in files):
        raise ShipError('ship.json files must be a non-empty list of relative file paths: package again')
    if len({n.casefold() for n in files}) != len(files) or STAMP_NAME.casefold() in {n.casefold() for n in files}:
        raise ShipError('ship.json files contains duplicate paths or ship.json itself: package again')
    if not isinstance(hashes, dict) or set(hashes) != set(files):
        raise ShipError('ship.json file_sha256 must record exactly every packaged file: package again')
    exe = project.exe_file(plat)
    required = {exe, project.dist_ico.name, project.dist_png.name}
    if stamp.get('title') != project.identity.title or stamp.get('exe') != exe:
        raise ShipError('ship.json title/exe differs from assets/identity.json: package again')
    for entry in project.identity.package:
        # Exact file or a non-empty declared subtree. Source inventory, when available, also catches
        # removal from BOTH manifest and dist. Never fall back to the source for the actual bytes.
        if entry not in files and not any(n.startswith(entry.rstrip('/') + '/') for n in files):
            raise ShipError(f'package entry {entry} is missing from dist/ manifest: package again')
        source = project.root / entry
        if source.is_dir():
            required.update(p.relative_to(project.root).as_posix() for p in source.rglob('*') if p.is_file())
        elif source.is_file():
            required.add(entry)
    missing = required - set(files)
    if missing:
        raise ShipError(f'ship.json omits required packaged files: {", ".join(sorted(missing))}; package again')
    for name in files:
        path = package_file(project.dist, name)
        digest = hashes[name]
        if not isinstance(digest, str) or not re.fullmatch(r'[0-9a-f]{64}', digest):
            raise ShipError(f'ship.json has an invalid SHA-256 for {name}: package again')
        if not path.is_file():
            raise ShipError(f'dist/{name} is missing: package again (source files cannot satisfy this check)')
        if sha256_file(path) != digest:
            raise ShipError(f'dist/{name} does not match dist/ship.json (modified or damaged): package again')
    if stamp.get('exe_sha256') != hashes[exe]:
        raise ShipError('ship.json exe_sha256 disagrees with file_sha256: package again')
    # The stock runtime has a discoverable file dependency: GameDocument.map. Check it without
    # launching graphics. Custom code's arbitrary file reads still need the isolated runtime smoke.
    for name in files:
        if not name.endswith('.json'):
            continue
        try:
            document = json.loads((project.dist / name).read_text(encoding='utf-8-sig'))
        except (ValueError, UnicodeError):
            continue  # content validation belongs to the game's own check, not the integrity gate
        if isinstance(document, dict) and document.get('schema_version') == 1 and 'player_profile' in document:
            reference = document.get('map')
            if not isinstance(reference, str) or not reference:
                raise ShipError(f'dist/{name}: GameDocument.map must name a packaged map file')
            dependency = posixpath.normpath(posixpath.join(posixpath.dirname(name), reference))
            package_file(project.dist, dependency)
            if dependency not in hashes:
                raise ShipError(f'dist/{name} references undeclared map {reference!r}: add it to identity.package and package again')
    return files


def cmd_package(project, no_build=False):
    identity = project.identity
    require_icon(project)  # fail before a long build
    built = locate_built_exe(project) if no_build else cargo_build_release(project)
    plan = package_plan(project, built)
    # dist/ also holds what the game writes beside its exe (saves, settings, records, logs). Only the files
    # this tool installed are ever replaced or removed, and they are known from the stamp of the last run
    # (a missing or damaged stamp means: overwrite the package files, delete nothing).
    stamp = project.read_stamp() or {}
    previous = stamp.get('files') if isinstance(stamp.get('files'), list) else []
    previous_hashes = stamp.get('file_sha256') if isinstance(stamp.get('file_sha256'), dict) else {}
    project.dist.mkdir(parents=True, exist_ok=True)
    for source, name in plan:
        _copy_file(source, project.dist / name)
    wanted = {name for _source, name in plan}
    removed, kept = [], []
    for name in previous:  # installed earlier, no longer part of the package
        if not isinstance(name, str) or name in wanted:
            continue
        target = project.dist / name
        if not inside(target, project.dist) or not target.is_file():
            continue
        recorded = previous_hashes.get(name)
        if isinstance(recorded, str) and sha256_file(target) != recorded:
            kept.append(name)  # the player or the game changed it since: it is theirs now
            continue
        target.unlink()
        removed.append(name)
        parent = target.parent
        while inside(parent, project.dist) and norm_path(parent) != norm_path(project.dist) and not any(parent.iterdir()):
            parent.rmdir()
            parent = parent.parent
    stamp.update({'title': identity.title, 'exe': project.exe_file(), 'exe_sha256': sha256_file(project.dist_exe),
                  'ico_sha256': sha256_file(project.dist_ico), 'packaged_at': iso_now(),
                  'engine_revision': engine_head(project.engine()) or identity.engine_revision,
                  'files': sorted(wanted), 'file_sha256': {name: sha256_file(project.dist / name) for name in sorted(wanted)}})
    stamp.pop('verified', None)  # evidence about an older exe says nothing about this one
    project.write_stamp(stamp)
    log(f'packaged {project.exe_file()} ({human_size(project.dist_exe.stat().st_size)}) into {project.display(project.dist)}/'
        f' ({len(wanted)} file(s){f", removed {len(removed)}" if removed else ""}'
        f'{f", left {len(kept)} changed file(s) alone" if kept else ""})')
    return {'ok': True, 'command': 'package', 'dist': str(project.dist), 'exe': str(project.dist_exe),
            'built_from': str(built), 'files': sorted(wanted), 'removed': removed, 'kept_modified': kept,
            'stamp': str(project.stamp_path)}


# --------------------------------------------------------------------------------------------------
# shortcut
# --------------------------------------------------------------------------------------------------


def _desktop_escape(value):
    return value.replace('\\', '\\\\')


def _desktop_exec(path):
    """A quoted Exec= program: reserved characters escaped for the Exec grammar, then for the string."""
    quoted = '"' + re.sub(r'(["`$\\])', r'\\\1', str(path)) + '"'
    return _desktop_escape(quoted.replace('%', '%%'))


def desktop_entry_text(project):
    identity = project.identity
    lines = ['[Desktop Entry]', 'Type=Application', 'Version=1.0', f'Name={_desktop_escape(identity.title)}',
             f'Comment={_desktop_escape(identity.description())}', f'Exec={_desktop_exec(project.dist_exe)}',
             f'Path={_desktop_escape(str(project.dist))}', f'Icon={_desktop_escape(str(project.dist_png))}',
             'Terminal=false', 'Categories=Game;']
    return '\n'.join(lines) + '\n'


def command_launcher_text(project):
    import shlex  # noqa: PLC0415
    exe = project.exe_file('macos')
    return f'#!/bin/bash\ncd {shlex.quote(str(project.dist))} || exit 1\nexec ./{shlex.quote(exe)} "$@"\n'


def parse_desktop_entry(text):
    """The [Desktop Entry] keys of a .desktop file, with string escapes undone."""
    values, inside_entry = {}, False
    for line in text.splitlines():
        line = line.strip()
        if line.startswith('['):
            inside_entry = line == '[Desktop Entry]'
        elif inside_entry and '=' in line and not line.startswith('#'):
            key, _, value = line.partition('=')
            values[key.strip()] = re.sub(r'\\(.)', lambda m: {'s': ' ', 'n': '\n', 't': '\t', 'r': '\r'}.get(m.group(1), m.group(1)),
                                         value.strip())
    return values


def desktop_exec_program(exec_value):
    """The program named by an Exec= value (first word, quotes and escapes resolved)."""
    value = exec_value.strip()
    if not value:
        return ''
    if value[0] == '"':
        out, i = [], 1
        while i < len(value) and value[i] != '"':
            if value[i] == '\\' and i + 1 < len(value):
                i += 1
            out.append(value[i])
            i += 1
        return ''.join(out).replace('%%', '%')
    return value.split()[0].replace('%%', '%')


def _launcher_targets_project(path, project, kind):
    """Does an existing launcher file already start this game (any executable inside the project)?"""
    try:
        text = Path(path).read_text(encoding='utf-8', errors='replace')
    except OSError:
        return False
    if kind == 'desktop':
        program = desktop_exec_program(parse_desktop_entry(text).get('Exec', ''))
        return bool(program) and inside(program, project.root)
    return norm_path(project.dist) in norm_path(text) or str(project.dist) in text


def _write_launcher(path, text):
    """Write an executable text launcher (.desktop / .command) by renaming a finished temp file."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_name(f'.{path.name}.{os.getpid()}.tmp')
    try:
        temp.write_text(text, encoding='utf-8', newline='\n')
        temp.chmod(0o755)
        os.replace(temp, path)
    finally:
        temp.unlink(missing_ok=True)


def is_lnk_file(path):
    """True when the file starts with the Windows shell-link header (size 0x4C, then its CLSID)."""
    try:
        with open(path, 'rb') as handle:
            head = handle.read(20)
    except OSError:
        return False
    return head[:4] == b'\x4c\x00\x00\x00' and head[4:20] == bytes.fromhex('0114020000000000c000000000000046')


def cmd_shortcut(project, folder=None, force=False, platform_name=None):
    """Create or refresh the launcher for the packaged game."""
    project.identity  # noqa: B018 (validates assets/identity.json before anything is touched)
    plat = platform_name or project.platform
    if plat not in ('windows', 'linux', 'macos'):
        return {'ok': True, 'command': 'shortcut', 'skipped': f'unsupported platform {sys.platform}'}
    if plat != host_platform() and not folder:
        raise ConfigError(f'--platform {plat} on a {host_platform()} machine needs --folder (it never touches the OS)')
    exe, ico, png = project.dist / project.exe_file(plat), project.dist_ico, project.dist_png
    if plat == host_platform():  # generating another platform's launcher into --folder needs no real files
        for needed in (exe, ico):
            if not needed.is_file():
                raise ShipError(f'{project.display(needed)} is missing: run "python scripts/ship.py package" first')
        if plat == 'linux' and not png.is_file():
            raise ShipError(f'{project.display(png)} is missing: the .desktop icon is the packaged PNG '
                            '(add assets/icon.png and run package again)')
    if folder:
        desktop = Path(folder).resolve()
        desktop.mkdir(parents=True, exist_ok=True)
    else:
        desktop = desktop_folders(plat)['user']
        if desktop is None:
            log('no desktop folder here: nothing to create')
            return {'ok': True, 'command': 'shortcut', 'skipped': 'no desktop'}
    if plat == 'windows':
        result = _shortcut_windows(project, desktop, exe, ico, force)
    elif plat == 'linux':
        result = _shortcut_linux(project, desktop, force, home=not folder)
    else:
        result = _shortcut_macos(project, desktop, force)
    stamp = project.read_stamp()
    if stamp is not None:
        stamp['shortcut'] = {'path': result['paths'][0], 'created_at': iso_now()}
        project.write_stamp(stamp)
    log(f'{result["action"]} {result["paths"][0]}')
    return {'ok': True, 'command': 'shortcut', **result}


def _shortcut_windows(project, desktop, exe, ico, force):
    identity = project.identity
    path = desktop / f'{identity.title}.lnk'
    action = 'created'
    if path.exists():
        if not is_lnk_file(path):
            if not force:
                raise ShipError(f'{path} already exists and is not a shortcut; pass --force to replace it')
            action = 'replaced'
        else:
            target = read_shortcut(path).get('target') or ''
            ours = bool(target) and (same_file(target, exe) or inside(target, project.root))
            alive = bool(target) and Path(target).exists()
            if not ours and alive and not force:
                raise ShipError(f'{path} already exists and starts a different program ({target}), not this game. '
                                'Pick another title in assets/identity.json, or pass --force to replace it')
            # Refresh what is ours or dangling (its target is gone); anything else only happens with --force.
            action = 'replaced' if (alive and not ours) else 'refreshed'
    temp = desktop / f'.{identity.title}.blue-ship.{os.getpid()}.lnk'
    try:
        written = write_shortcut(temp, exe, project.dist, f'{ico},0', identity.description(), 1)
        os.replace(temp, path)
    finally:
        Path(temp).unlink(missing_ok=True)
    return {'action': action, 'kind': 'lnk', 'paths': [str(path)], 'target': written.get('target'),
            'working_dir': written.get('working_dir'), 'icon_location': written.get('icon_location'),
            'description': written.get('description')}


def _shortcut_linux(project, desktop, force, home):
    identity = project.identity
    text = desktop_entry_text(project)
    targets = [desktop / f'{identity.title}.desktop']
    if home:
        data = Path(os.environ.get('XDG_DATA_HOME') or Path.home() / '.local' / 'share')
        targets.append(data / 'applications' / f'{identity.slug()}.desktop')
    action = 'created'
    for path in targets:
        if path.exists():
            if _launcher_targets_project(path, project, 'desktop') or force:
                action = 'refreshed'
            else:
                raise ShipError(f'{path} already exists and starts a different program, not this game. '
                                'Pick another title in assets/identity.json, or pass --force to replace it')
    for path in targets:
        _write_launcher(path, text)
    return {'action': action, 'kind': 'desktop', 'paths': [str(p) for p in targets], 'target': str(project.dist_exe),
            'working_dir': str(project.dist), 'icon_location': str(project.dist_png)}


def _shortcut_macos(project, desktop, force):
    identity = project.identity
    path = desktop / f'{identity.title}.command'
    action = 'created'
    if path.exists():
        if _launcher_targets_project(path, project, 'command') or force:
            action = 'refreshed'
        else:
            raise ShipError(f'{path} already exists and starts a different program, not this game. '
                            'Pick another title in assets/identity.json, or pass --force to replace it')
    _write_launcher(path, command_launcher_text(project))
    return {'action': action, 'kind': 'command', 'paths': [str(path)], 'target': str(project.dist / project.exe_file('macos')),
            'working_dir': str(project.dist), 'icon_location': None}


# --------------------------------------------------------------------------------------------------
# verify
# --------------------------------------------------------------------------------------------------

PASS, FAIL, SKIP, WARN = 'pass', 'fail', 'skip', 'warn'
ART_MIN_STDDEV = 4.0  # luminance spread below which an icon counts as one flat colour
ART_MIN_COLOURS = 12
ART_COVERAGE = (0.25, 0.99)  # share of the square that is at least half opaque
SMOKE_MIN_STDDEV = 2.0  # luminance spread below which a captured frame counts as blank
REGENERATE = 'be2-tools icon "{title}" assets --variant N --replace'

NEXT_STEPS = {
    'identity': 'edit assets/identity.json (title, tagline, controls), then run: python scripts/ship.py verify',
    'icon-files': 'regenerate the icon set: be2-tools icon "{title}" assets --replace',
    'icon-art': 'draw or regenerate the icon: ' + REGENERATE,
    'wiring': 'wire the icon and title into the game: copy the engine templates/game_build.rs to build.rs, pass '
              'assets/icon_64.rgba to the window config, use the identity title as the window title, and put /dist/ in .gitignore',
    'package': 'python scripts/ship.py package',
    'exe-resources': 'make build.rs take the title and icon from assets/ (copy the engine templates/game_build.rs; it needs the '
                     'Windows SDK rc.exe), rebuild, then: python scripts/ship.py package',
    'shortcut-file': 'python scripts/ship.py shortcut',
    'shortcut-icon': 'python scripts/ship.py shortcut',
    'shortcut-unique': REGENERATE + ', rebuild, then: python scripts/ship.py ship',
    'launch': 'python scripts/ship.py verify --launch (read the detail: title, icon or startup problem)',
    'smoke': 'run the exe by hand with the smoke_args from assets/identity.json and read its output',
}


class Verifier:
    """Runs every check even after a failure and collects a JSON-able report."""

    def __init__(self, project, folder=None, platform_name=None, launch=False, smoke=False, skip_package=False):
        self.project = project
        self.folder = Path(folder).resolve() if folder else None
        self.plat = platform_name or project.platform
        self.want_launch = launch
        self.want_smoke = smoke
        self.skip_package = skip_package
        self.checks = []
        self.skipped = []
        self._ico = None  # (bytes, frames) once icon.ico parsed
        self._art = None
        self._desktop = False  # False = not looked up yet
        self._shell = None
        self._shell_error = None
        self.outcomes = {}

    # ---- plumbing ----
    def add(self, name, status, detail):
        self.checks.append({'name': name, 'status': status, 'detail': detail})
        self.outcomes[name] = status
        if status == SKIP:
            self.skipped.append(f'{name}: {detail}')

    def run_check(self, name, function):
        try:
            status, detail = function()
        except ShipError as error:
            status, detail = FAIL, str(error)
        except Exception as error:  # noqa: BLE001 (one broken check must not stop the others from running)
            status, detail = FAIL, f'internal error, {type(error).__name__}: {error}'
        self.add(name, status, detail)

    @property
    def title(self):
        return self.project.identity.title

    @property
    def native_windows(self):
        return self.plat == 'windows' and host_platform() == 'windows'

    def desktop(self):
        """The folder that holds this game's launcher: --folder, else the OS desktop (None: no desktop)."""
        if self._desktop is False:
            self._desktop = self.folder if self.folder else desktop_folders(self.plat)['user']
        return self._desktop

    def launcher_path(self):
        desktop = self.desktop()
        if desktop is None:
            return None
        suffix = {'windows': '.lnk', 'linux': '.desktop', 'macos': '.command'}[self.plat]
        return desktop / f'{self.title}{suffix}'

    def dist_exe(self):
        return self.project.dist / self.project.exe_file(self.plat)

    # ---- the report ----
    def run(self):
        self.run_check('identity', self.check_identity)
        self.run_check('icon-files', self.check_icon_files)
        self.run_check('icon-art', self.check_icon_art)
        self.run_check('wiring', self.check_wiring)
        self.run_check('package', self.check_package)
        self.run_check('exe-resources', self.check_exe_resources)
        self.run_check('shortcut-file', self.check_shortcut_file)
        self.run_check('shortcut-icon', self.check_shortcut_icon)
        self.run_check('shortcut-unique', self.check_shortcut_unique)
        self.run_check('launch', self.check_launch)
        self.run_check('smoke', self.check_smoke)
        failed = [c for c in self.checks if c['status'] == FAIL]
        report = {'ok': not failed, 'checks': self.checks, 'skipped': self.skipped, 'next': '',
                  'title': self.project.identity.title if self.outcomes.get('identity') == PASS else None,
                  'platform': self.plat}
        if failed:
            title = report['title'] or 'Title'
            report['next'] = NEXT_STEPS.get(failed[0]['name'], 'python scripts/ship.py ship').format(title=title)
        self.record_stamp()
        return report

    def record_stamp(self):
        """Write the `verified` block of dist/ship.json: launch/smoke are true/false when they ran in
        this verify, else the last recorded evidence (null when there is none)."""
        stamp = self.project.read_stamp()
        if stamp is None or not self.project.dist.is_dir():
            return
        previous = stamp.get('verified') if isinstance(stamp.get('verified'), dict) else {}
        now = iso_now()
        verified = {'at': now}
        for name in ('launch', 'smoke'):
            status = self.outcomes.get(name)
            if status in (PASS, FAIL):
                verified[name], verified[f'{name}_at'] = status == PASS, now
            else:  # not run this time: keep the last evidence (package clears it when the exe changes)
                verified[name], verified[f'{name}_at'] = previous.get(name), previous.get(f'{name}_at')
        stamp['verified'] = verified
        try:
            self.project.write_stamp(stamp)
        except OSError:
            pass

    def blocked(self, *names):
        """A skip reason when an earlier check this one builds on did not pass."""
        for name in names:
            if self.outcomes.get(name) not in (PASS, WARN):
                return f'{name} did not pass'
        return None

    # ---- 1 identity ----
    def check_identity(self):
        project = self.project
        if not project.identity_path.is_file():
            return FAIL, 'assets/identity.json is missing: create it with "title", "tagline" and "controls"'
        data = project.raw_identity()
        if data is None:
            return FAIL, 'assets/identity.json is not valid JSON'
        problems = identity_problems(data)
        stem = None
        try:
            stem = project.exe_stem()
        except ConfigError as error:
            problems.append(str(error))
        if problems:
            return FAIL, '; '.join(problems)
        identity = project.identity
        extras = len(identity.package)
        return PASS, f'title "{identity.title}", exe {stem}, {extras} extra package entr{"y" if extras == 1 else "ies"}'

    # ---- 2 icon-files ----
    def check_icon_files(self):
        path = self.project.assets / 'icon.ico'
        if not path.is_file():
            return FAIL, 'assets/icon.ico is missing'
        data = path.read_bytes()
        try:
            frames = parse_ico(data)
        except ImageError as error:
            return FAIL, f'assets/icon.ico does not parse: {error}'
        self._ico = (data, frames)
        problems = []
        sizes = sorted(frame.width for frame in frames)
        missing = [s for s in ICO_SIZES if s not in sizes]
        if missing:
            problems.append('icon.ico lacks the sizes ' + ', '.join(map(str, missing)))
        for frame in frames:
            label = f'{frame.width}x{frame.height} frame'
            if frame.width != frame.height:
                problems.append(f'{label} is not square')
            elif frame.width >= PNG_FRAME_FROM:
                if frame.kind != 'png':
                    problems.append(f'{label} must be a PNG frame')
                elif (frame.header_w, frame.header_h) != (frame.width, frame.height) or frame.header_bpp != 32:
                    problems.append(f'{label} is not a {frame.width}x{frame.height} 32-bit RGBA PNG')
            elif frame.kind != 'bmp' or frame.header_bpp != 32:
                problems.append(f'{label} must be a 32-bit BMP frame')
            elif frame.header_w != frame.width or frame.header_h != 2 * frame.height:
                problems.append(f'{label} has an inconsistent BMP header')
        for name, size in RGBA_BLOBS.items():
            blob = self.project.assets / name
            expected = size * size * 4
            if not blob.is_file():
                problems.append(f'assets/{name} is missing')
            elif blob.stat().st_size != expected:
                problems.append(f'assets/{name} is {blob.stat().st_size} bytes, expected {expected}')
        png = self.project.assets / 'icon.png'
        if not png.is_file():
            problems.append('assets/icon.png is missing')
        elif png.read_bytes()[:8] != PNG_SIGNATURE:
            problems.append('assets/icon.png is not a PNG')
        if problems:
            return FAIL, '; '.join(problems)
        return PASS, f'icon.ico has {len(frames)} sizes (16..256, BMP below {PNG_FRAME_FROM}, PNG from {PNG_FRAME_FROM}), ' \
                     'icon_16/32/64.rgba and icon.png are well-formed'

    def ico_pixels(self, size):
        """(width, height, rgba) of the icon.ico frame of exactly this size."""
        data, frames = self._ico
        for frame in frames:
            if frame.width == size and frame.height == size:
                return decode_ico_frame(data, frame)
        raise ImageError(f'icon.ico has no {size} px frame')

    def art_signatures(self):
        """[(label, Signature)] for every frame of icon.ico and every icon_*.rgba: our art."""
        if self._art is None:
            art = []
            data, frames = self._ico
            for frame in frames:
                try:
                    art.append((f'{frame.width} px frame', make_signature(*decode_ico_frame(data, frame))))
                except ImageError:
                    continue
            for name, size in RGBA_BLOBS.items():
                blob = self.project.assets / name
                if blob.is_file() and blob.stat().st_size == size * size * 4:
                    art.append((name, make_signature(size, size, blob.read_bytes())))
            self._art = art
        return self._art

    # ---- 3 icon-art ----
    def check_icon_art(self):
        if self._ico is None:
            return SKIP, 'icon-files did not parse icon.ico'
        problems, notes = [], []
        pixels = {}
        for size in (32, 256):
            try:
                pixels[size] = self.ico_pixels(size)
            except ImageError as error:
                problems.append(str(error))
                continue
            width, height, rgba = pixels[size]
            spread, colours, cover = luma_stddev(width, height, rgba, True), count_colours(rgba), opaque_share(rgba)
            if spread < ART_MIN_STDDEV:
                problems.append(f'the {size} px frame is one flat colour (luminance spread {spread:.1f})')
            if colours < ART_MIN_COLOURS:
                problems.append(f'the {size} px frame has only {colours} colours (need {ART_MIN_COLOURS})')
            if not ART_COVERAGE[0] <= cover <= ART_COVERAGE[1]:
                problems.append(f'the {size} px frame covers {cover:.0%} of the square '
                                f'(need {ART_COVERAGE[0]:.0%}-{ART_COVERAGE[1]:.0%}: transparent corners, real art)')
            notes.append(f'{size} px: {colours} colours, {cover:.0%} covered')
        # One art everywhere: the window icon blobs must be the frames of icon.ico.
        for name, size in RGBA_BLOBS.items():
            blob = self.project.assets / name
            if not blob.is_file() or blob.stat().st_size != size * size * 4:
                continue
            try:
                frame_sig = make_signature(*self.ico_pixels(size))
            except ImageError:
                continue
            shape, colour = signature_distance(make_signature(size, size, blob.read_bytes()), frame_sig)
            if max(shape, colour) > EQUAL_MAX:
                problems.append(f'assets/{name} is not the same art as the {size} px frame of icon.ico '
                                f'(shape {shape:.2f}, colour {colour:.2f}): regenerate the set together')
        if 32 in pixels:
            ours = make_signature(*pixels[32])
            logo = make_signature(32, 32, base64_logo())
            shape, colour = signature_distance(ours, logo)
            if too_similar(ours, logo):
                problems.append("the icon looks like miniquad's default window icon "
                                f'(shape {shape:.2f}, colour {colour:.2f}): draw the game its own art')
            notes.append(self.compare_with_engine_logo(pixels[32], problems))
        if problems:
            return FAIL, '; '.join(problems)
        return PASS, '; '.join(n for n in notes if n)

    def compare_with_engine_logo(self, pixels32, problems):
        """Fail (through `problems`) when the icon is the BlueEngine logo; returns a note."""
        engine = self.project.engine()
        logo_path = engine / 'assets' / 'branding' / 'blueengine.ico' if engine else None
        if not logo_path or not logo_path.is_file():
            return 'engine logo comparison skipped (no engine checkout with assets/branding/blueengine.ico)'
        logo_bytes = logo_path.read_bytes()
        if logo_bytes == self._ico[0]:
            problems.append("icon.ico is byte-identical to the BlueEngine logo: a game needs its own icon")
            return ''
        try:
            frames = parse_ico(logo_bytes)
            frame = min(frames, key=lambda f: abs(f.width - 32))
            width, height, rgba = decode_ico_frame(logo_bytes, frame)
        except ImageError as error:
            return f'engine logo comparison skipped ({error})'
        ours = make_signature(*pixels32)
        theirs = make_signature(width, height, rgba)
        shape, colour = signature_distance(ours, theirs)
        if ((width, height) == (32, 32) and rgba == pixels32[2]) or too_similar(ours, theirs):
            problems.append(f'the icon looks like the BlueEngine logo (shape {shape:.2f}, colour {colour:.2f}): '
                            'a game needs its own art')
            return ''
        return f'not the engine logo (shape {shape:.2f}, colour {colour:.2f})'

    # ---- 4 wiring ----
    def check_wiring(self):
        root = self.project.root
        problems, warnings = [], []
        build = root / 'build.rs'
        if not build.is_file():
            problems.append('build.rs is missing (it embeds assets/icon.ico in the exe: copy the engine templates/game_build.rs)')
        else:
            text = build.read_text(encoding='utf-8', errors='replace')
            if 'assets/icon.ico' not in text and not ('"assets"' in text and '"icon.ico"' in text):
                problems.append('build.rs does not mention assets/icon.ico')
        sources = []
        for path in sorted((root / 'src').rglob('*.rs'))[:2000]:
            try:
                sources.append(path.read_text(encoding='utf-8', errors='replace'))
            except OSError:
                continue
        if not any('icon_64.rgba' in s or 'icon_from_rgba' in s for s in sources):
            problems.append('no source under src/ sets the window icon (expected icon_64.rgba or icon_from_rgba)')
        title = self.project.identity.title if self.outcomes.get('identity') == PASS else None
        if title is None:
            warnings.append('title literal not checked (identity is invalid)')
        elif not any(f'"{title}"' in s or ('include_str!' in s and 'identity.json' in s) for s in sources):
            problems.append(f'the window title "{title}" does not appear in src/ '
                            '(use the identity title as the window title, or include_str! identity.json)')
        gitignore = root / '.gitignore'
        if not gitignore.is_file():
            warnings.append('.gitignore is missing (it should ignore /dist/)')
        elif not any(line.strip() in ('/dist/', '/dist', 'dist/', 'dist') for line in gitignore.read_text(
                encoding='utf-8', errors='replace').splitlines()):
            problems.append('.gitignore has no /dist/ line')
        if problems:
            return FAIL, '; '.join(problems)
        if warnings:
            return WARN, '; '.join(warnings)
        return PASS, 'build.rs embeds the icon, src/ sets the window icon and the title, /dist/ is ignored'

    # ---- 5 package ----
    def check_package(self):
        if self.blocked('identity'):
            return SKIP, 'identity is invalid'
        project, exe = self.project, self.dist_exe()
        if not exe.is_file():
            if self.skip_package:
                return SKIP, f'{project.display(exe)} is missing and --skip-package was given'
            return FAIL, f'{project.display(exe)} is missing: run python scripts/ship.py package'
        problems, warnings = [], []
        stem = project.exe_stem()
        icon = project.assets / 'icon.ico'
        if not project.dist_ico.is_file():
            problems.append(f'{project.display(project.dist_ico)} is missing')
        elif icon.is_file() and project.dist_ico.read_bytes() != icon.read_bytes():
            problems.append(f'dist/{stem}.ico differs from assets/icon.ico (packaged from another icon)')
        png = project.assets / 'icon.png'
        if png.is_file() and not project.dist_png.is_file():
            warnings.append(f'dist/{stem}.png is missing (Linux launchers need it)')
        for entry in project.identity.package:
            if not (project.dist / entry).exists():
                problems.append(f'package entry {entry} is missing from dist/')
        stamp = project.read_stamp()
        if stamp is None:
            problems.append(f'dist/{STAMP_NAME} is missing (created by package)')
        elif stamp.get('exe_sha256') != sha256_file(exe):
            problems.append(f'dist/{exe.name} does not match dist/{STAMP_NAME} (replaced or damaged after packaging)')
        try:
            files = package_integrity(project, self.plat)
        except ShipError as error:
            problems.append(str(error))
        built = self.freshest_release_build()
        if built and exe.stat().st_mtime + STALE_SLACK < built.stat().st_mtime:
            warnings.append(f'dist/{exe.name} is older than the release build {built}: package again')
        if problems:
            return FAIL, '; '.join(problems)
        if warnings:
            return WARN, '; '.join(warnings)
        return PASS, f'{len(files)} declared files: manifest, safe paths, existence and SHA-256 verified without a display'

    def freshest_release_build(self):
        """The newest release-profile exe we can find without running cargo, or None."""
        directories = [self.project.root / 'target']
        env = os.environ.get('CARGO_TARGET_DIR')
        if env:
            directories.insert(0, (self.project.root / env))
        found = [d / 'release' / self.project.exe_file(self.plat) for d in directories]
        found = [p for p in found if p.is_file()]
        return max(found, key=lambda p: p.stat().st_mtime) if found else None

    # ---- shell rendering (Windows) ----
    def render(self, path, size):
        """(width, height, rgba) of the shell icon of `path`; raises ShipError when it cannot be rendered."""
        self.prefetch_shell()
        entry = (self._shell or {}).get(str(path))
        if entry is None:
            raise ShipError(f'{path} was not rendered' + (f': {self._shell_error}' if self._shell_error else ''))
        if size not in entry['images']:
            raise ShipError(f'the shell could not render {path}: {entry.get("error") or "no image"}')
        return entry['images'][size]

    def other_shortcuts(self):
        """Other .lnk files on the desktop(s): recursive to SCAN_MAX_DEPTH folders, at most SCAN_MAX_FILES."""
        roots = [self.folder] if self.folder else [d for d in desktop_folders('windows').values() if d]
        found, seen = [], set()
        for base in roots:
            for directory, dirnames, filenames in os.walk(base):
                depth = len(Path(directory).relative_to(base).parts)
                dirnames.sort()
                if depth >= SCAN_MAX_DEPTH:
                    dirnames[:] = []
                for name in sorted(filenames):
                    path = Path(directory) / name
                    if name.lower().endswith('.lnk') and norm_path(path) not in seen:
                        seen.add(norm_path(path))
                        found.append(path)
        return found[:SCAN_MAX_FILES]

    def prefetch_shell(self):
        """Render every icon the Windows checks need in ONE PowerShell run (it is slow to start)."""
        if self._shell is not None or self._shell_error is not None:
            return
        wanted = []
        for path, sizes in ((self.dist_exe(), (32, 256)), (self.project.dist_ico, (32, 256))):
            if path.is_file():
                wanted.append((str(path), sizes))
        lnk = self.launcher_path()
        if lnk is not None and lnk.is_file():
            wanted.append((str(lnk), (32, 256)))
        if self.desktop() is not None or self.folder:
            wanted.extend((str(p), (32,)) for p in self.other_shortcuts() if lnk is None or norm_path(p) != norm_path(lnk))
        try:
            self._shell = self._render_batch(wanted)
        except ShipError as error:
            self._shell_error = str(error)
            self._shell = {}

    @staticmethod
    def _render_batch(wanted):
        merged = {}
        by_sizes = {}
        for path, sizes in wanted:
            by_sizes.setdefault(tuple(sizes), []).append(path)
        for sizes, paths in by_sizes.items():
            merged.update(shell_render(paths, sizes))
        return merged

    # ---- 6 exe-resources ----
    def check_exe_resources(self):
        if self.plat != 'windows':
            return SKIP, f'Windows executables only (platform is {self.plat})'
        if not self.native_windows:
            return SKIP, 'reading exe resources needs a Windows host'
        if self.blocked('identity'):
            return SKIP, 'identity is invalid'
        exe = self.dist_exe()
        if not exe.is_file():
            return SKIP, f'{self.project.display(exe)} is missing' + (' (--skip-package)' if self.skip_package else '')
        problems = []
        self.prefetch_shell()
        info = ((self._shell or {}).get(str(exe)) or {}).get('version') or exe_version_info(exe)
        for key, label in (('file_description', 'FileDescription'), ('product_name', 'ProductName')):
            if info.get(key) != self.title:
                problems.append(f'{label} is "{info.get(key) or ""}", expected "{self.title}"')
        note = ''
        if self.project.dist_ico.is_file():
            worst = (0.0, 0.0)
            for size in (32, 256):
                exe_sig = make_signature(*self.render(exe, size))
                ico_sig = make_signature(*self.render(self.project.dist_ico, size))
                shape, colour = signature_distance(exe_sig, ico_sig)
                worst = (max(worst[0], shape), max(worst[1], colour))
                if max(shape, colour) > EQUAL_MAX:
                    problems.append(f"the exe's icon is not the game's art at {size} px (shape {shape:.2f}, colour {colour:.2f}): "
                                    'build.rs embeds nothing without the Windows SDK (rc.exe) or the "client" feature')
                    break
            note = f', exe icon matches the .ico (shape {worst[0]:.2f}, colour {worst[1]:.2f})'
        if problems:
            return FAIL, '; '.join(problems)
        return PASS, f'FileDescription and ProductName are "{self.title}"{note}'

    # ---- 7 shortcut-file ----
    def no_desktop(self):
        return self.desktop() is None

    def check_shortcut_file(self):
        if self.blocked('identity'):
            return SKIP, 'identity is invalid'
        if self.no_desktop():
            return SKIP, 'no desktop'
        if self.unpackaged():
            return SKIP, self.unpackaged()
        path = self.launcher_path()
        if not path.is_file():
            return FAIL, f'{path} does not exist: run python scripts/ship.py shortcut'
        if self.plat == 'windows':
            return self.check_lnk(path)
        return self.check_text_launcher(path)

    def check_lnk(self, path):
        if not self.native_windows:
            return SKIP, 'reading .lnk files needs a Windows host'
        # exe-resources normally fetched every shell fact in one PowerShell run already
        info = ((self._shell or {}).get(str(path)) or {}).get('shortcut') or read_shortcut(path)
        project, identity = self.project, self.project.identity
        problems, warnings = [], []
        target = info.get('target') or ''
        if not same_file(target, self.dist_exe()):
            where = ' (a development build: shortcuts must start the packaged dist/ copy)' if inside(
                target, project.root) else ''
            problems.append(f'target is "{target}", expected "{self.dist_exe()}"{where}')
        if norm_path(info.get('working_dir') or '.') != norm_path(project.dist) or not info.get('working_dir'):
            problems.append(f'start in is "{info.get("working_dir")}", expected "{project.dist}"')
        icon_path, _, index = (info.get('icon_location') or '').rpartition(',')
        if not same_file(icon_path, project.dist_ico) or index.strip() != '0':
            problems.append(f'icon is "{info.get("icon_location")}", expected "{project.dist_ico},0"')
        description = info.get('description') or ''
        tagline = identity.tagline if len(identity.tagline) <= DESCRIPTION_MAX - 3 else identity.tagline[:DESCRIPTION_MAX - 3]
        if tagline not in description:
            problems.append('the tooltip does not contain the tagline')
        elif description != identity.description():
            warnings.append('the tooltip differs from assets/identity.json: run python scripts/ship.py shortcut to refresh it')
        if problems:
            return FAIL, f'{path.name}: ' + '; '.join(problems)
        if warnings:
            return WARN, f'{path.name}: ' + '; '.join(warnings)
        return PASS, f'{path} -> {project.display(self.dist_exe())}, start in dist/, icon {project.display(project.dist_ico)},0, tooltip has the tagline'

    def check_text_launcher(self, path):
        project, identity = self.project, self.project.identity
        text = path.read_text(encoding='utf-8', errors='replace')
        problems = []
        exe = self.dist_exe()
        if self.plat == 'linux':
            entry = parse_desktop_entry(text)
            if entry.get('Type') != 'Application':
                problems.append('Type is not Application')
            if entry.get('Name') != identity.title:
                problems.append(f'Name is "{entry.get("Name")}", expected "{identity.title}"')
            if not same_file(desktop_exec_program(entry.get('Exec', '')), exe):
                problems.append(f'Exec starts "{desktop_exec_program(entry.get("Exec", ""))}", expected "{exe}"')
            if not same_file(entry.get('Path', ''), project.dist):
                problems.append(f'Path is "{entry.get("Path")}", expected "{project.dist}"')
            if not same_file(entry.get('Icon', ''), project.dist_png):
                problems.append(f'Icon is "{entry.get("Icon")}", expected "{project.dist_png}"')
            elif host_platform() == 'linux' and not Path(entry['Icon']).is_file():
                problems.append('the Icon file does not exist')
            if entry.get('Terminal') != 'false':
                problems.append('Terminal is not false')
            if identity.tagline[:DESCRIPTION_MAX - 3] not in entry.get('Comment', ''):
                problems.append('Comment does not contain the tagline')
        else:
            if not text.startswith('#!/bin/bash'):
                problems.append('the launcher does not start with #!/bin/bash')
            if str(project.dist) not in text and norm_path(project.dist) not in norm_path(text):
                problems.append(f'the launcher does not cd into {project.dist}')
            if f'./{exe.name}' not in text.replace("'", ''):
                problems.append(f'the launcher does not exec ./{exe.name}')
        if host_platform() == self.plat and self.plat != 'windows' and not os.access(path, os.X_OK):
            problems.append('the launcher is not executable (chmod 755)')
        if problems:
            return FAIL, f'{path.name}: ' + '; '.join(problems)
        return PASS, f'{path} starts {project.display(exe)} from dist/'

    # ---- 8 shortcut-icon ----
    def unpackaged(self):
        """A skip reason for the shortcut checks when --skip-package was given and there is no package."""
        if self.skip_package and not self.dist_exe().is_file():
            return f'{self.project.display(self.dist_exe())} is missing (--skip-package)'
        return None

    def check_shortcut_icon(self):
        if self.blocked('identity'):
            return SKIP, 'identity is invalid'
        if self.no_desktop():
            return SKIP, 'no desktop'
        if self.unpackaged():
            return SKIP, self.unpackaged()
        path = self.launcher_path()
        if not path.is_file():
            return SKIP, 'the shortcut does not exist (see shortcut-file)'
        if self.plat == 'macos':
            return SKIP, 'a .command launcher carries no icon of its own'
        if self.plat == 'linux':
            return self.check_linux_icon(path)
        if not self.native_windows:
            return SKIP, 'shell icon rendering needs a Windows host'
        if not self.project.dist_ico.is_file():
            return SKIP, f'{self.project.display(self.project.dist_ico)} is missing'
        problems, notes = [], []
        for size in (32, 256):
            width, height, rgba = self.render(path, size)
            spread, colours = luma_stddev(width, height, rgba, True), count_colours(rgba)
            if spread < ART_MIN_STDDEV or colours < ART_MIN_COLOURS:
                problems.append(f'the shell shows a blank or flat icon at {size} px ({colours} colours)')
                continue
            shape, colour = signature_distance(make_signature(width, height, rgba),
                                               make_signature(*self.render(self.project.dist_ico, size)))
            notes.append(f'{size} px shape {shape:.2f} colour {colour:.2f}')
            if max(shape, colour) > EQUAL_MAX:
                problems.append(f'the shell icon of the shortcut is not the game icon at {size} px '
                                f'(shape {shape:.2f}, colour {colour:.2f}): check IconLocation')
        if problems:
            return FAIL, '; '.join(problems)
        return PASS, 'the shell renders the shortcut with the game icon (' + ', '.join(notes) + ')'

    def check_linux_icon(self, path):
        if self._ico is None:
            return SKIP, 'icon-files did not parse icon.ico'
        entry = parse_desktop_entry(path.read_text(encoding='utf-8', errors='replace'))
        icon = Path(entry.get('Icon', ''))
        if not icon.is_file():
            return FAIL, f'the Icon file "{icon}" does not exist'
        width, height, rgba = decode_png(icon.read_bytes())
        ours = make_signature(*self.ico_pixels(256)) if self._ico else None
        shape, colour = signature_distance(make_signature(width, height, rgba), ours) if ours else (0.0, 0.0)
        if max(shape, colour) > EQUAL_MAX:
            return FAIL, f'the Icon PNG is not the game icon (shape {shape:.2f}, colour {colour:.2f})'
        return PASS, f'Icon {icon.name} ({width}x{height}) is the game icon (no shell renderer on Linux)'

    # ---- 9 shortcut-unique ----
    def check_shortcut_unique(self):
        if self.blocked('identity'):
            return SKIP, 'identity is invalid'
        if self.no_desktop():
            return SKIP, 'no desktop'
        if self.unpackaged():
            return SKIP, self.unpackaged()
        path = self.launcher_path()
        if not path.is_file():
            return SKIP, 'the shortcut does not exist (see shortcut-file)'
        if self.plat == 'linux':
            return self.unique_linux(path)
        if self.plat != 'windows':
            return SKIP, 'a .command launcher carries no icon to compare'
        if not self.native_windows:
            return SKIP, 'shell icon rendering needs a Windows host'
        ours = make_signature(*self.render(path, 32))
        compared, nearest, clashes, twins = 0, None, [], []
        for other in self.other_shortcuts():
            if norm_path(other) == norm_path(path):
                continue
            entry = (self._shell or {}).get(str(other))
            if entry is None or 32 not in entry['images']:
                continue  # broken or unreadable shortcuts say nothing about looks
            target = entry.get('target') or ''
            if target and (same_file(target, self.dist_exe()) or inside(target, self.project.root)):
                continue  # another way to start this very game
            if other.stem.casefold() == self.title.casefold():
                twins.append(str(other))
            signature = make_signature(*entry['images'][32])
            shape, colour = signature_distance(ours, signature)
            compared += 1
            score = max(shape / SIMILAR_SHAPE, colour / SIMILAR_COLOUR)
            if nearest is None or score < nearest[0]:
                nearest = (score, other, shape, colour)
            if too_similar(ours, signature):
                clashes.append((score, other, shape, colour))
        if twins:
            return FAIL, f'another shortcut is also called "{self.title}" ({twins[0]}): choose a unique title'
        if clashes:
            _score, other, shape, colour = min(clashes, key=lambda c: c[0])
            return FAIL, (f'the icon is too similar to "{other.stem}" ({other}): shape {shape:.2f} <= {SIMILAR_SHAPE}, '
                          f'colour {colour:.2f} <= {SIMILAR_COLOUR}; give the game its own art with '
                          + REGENERATE.format(title=self.title) + ', rebuild and run python scripts/ship.py ship')
        if nearest is None:
            return PASS, 'no other shortcuts'
        _score, other, shape, colour = nearest
        return PASS, (f'distinct from {compared} other shortcut(s); nearest is "{other.stem}" '
                      f'(shape {shape:.2f}, colour {colour:.2f})')

    def unique_linux(self, path):
        if self._ico is None:
            return SKIP, 'icon-files did not parse icon.ico'
        folders = [self.folder] if self.folder else [d for d in (self.desktop(),) if d]
        if not self.folder:
            data = Path(os.environ.get('XDG_DATA_HOME') or Path.home() / '.local' / 'share') / 'applications'
            if data.is_dir():
                folders.append(data)
        ours = make_signature(*self.ico_pixels(256))
        compared, nearest = 0, None
        for base in folders:
            for other in sorted(base.rglob('*.desktop')):
                if norm_path(other) == norm_path(path) or other.name == f'{self.project.identity.slug()}.desktop':
                    continue
                entry = parse_desktop_entry(other.read_text(encoding='utf-8', errors='replace'))
                if entry.get('Name', '').casefold() == self.title.casefold() and not inside(
                        desktop_exec_program(entry.get('Exec', '')), self.project.root):
                    return FAIL, f'another launcher is also called "{self.title}" ({other}): choose a unique title'
                icon = Path(entry.get('Icon', ''))
                if not icon.is_absolute() or icon.suffix.lower() != '.png' or not icon.is_file():
                    continue
                try:
                    signature = make_signature(*decode_png(icon.read_bytes()))
                except ImageError:
                    continue
                compared += 1
                shape, colour = signature_distance(ours, signature)
                if too_similar(ours, signature):
                    return FAIL, (f'the icon is too similar to "{other.stem}" ({other}): shape {shape:.2f}, colour {colour:.2f}; '
                                  + REGENERATE.format(title=self.title))
                score = max(shape / SIMILAR_SHAPE, colour / SIMILAR_COLOUR)
                if nearest is None or score < nearest[0]:
                    nearest = (score, other, shape, colour)
        if nearest is None:
            return PASS, 'no other launchers with icon files'
        return PASS, f'distinct from {compared} other launcher(s); nearest is "{nearest[1].stem}"'

    # ---- 10 launch ----
    def dynamic_skip(self, wanted, flag, reason):
        """A skip reason shared by the launch and smoke checks, or None when they can run."""
        if not wanted:
            return reason or f'not requested (pass {flag})'
        if self.blocked('identity'):
            return 'identity is invalid'
        if not has_display():
            return 'no display'
        exe = self.dist_exe()
        if not exe.is_file():
            return f'{self.project.display(exe)} is missing' + (' (--skip-package)' if self.skip_package else '')
        return None

    def window_icon_verdict(self, label, icon):
        """(problem or None, note) for one window icon compared with our art."""
        signature = make_signature(icon['width'], icon['height'], icon['rgba'])
        best = None
        for _name, art in self.art_signatures():
            shape, colour = signature_distance(signature, art)
            if best is None or max(shape, colour) < max(best):
                best = (shape, colour)
        if best is not None and max(best) <= EQUAL_MAX:
            return None, f'{label} icon {icon["width"]}x{icon["height"]} is the game art (shape {best[0]:.2f}, colour {best[1]:.2f})'
        if too_similar(signature, make_signature(32, 32, base64_logo())):
            return (f"the window's {label} icon is miniquad's default logo: pass the game icon in the window config "
                    '(include_bytes!("../assets/icon_64.rgba") as miniquad_conf.icon)'), ''
        shown = f' (best match: shape {best[0]:.2f}, colour {best[1]:.2f})' if best else ''
        return f"the window's {label} icon is not the game art{shown}", ''

    def check_launch(self):
        reason = self.dynamic_skip(self.want_launch, '--launch', getattr(self, 'launch_reason', None))
        if reason:
            return SKIP, reason
        if self.plat != 'windows':
            return SKIP, f'the launch check drives Windows shortcuts and windows only (platform is {self.plat})'
        if not self.native_windows:
            return SKIP, 'the launch check needs a Windows host'
        if self.no_desktop():
            return SKIP, 'no desktop'
        lnk = self.launcher_path()
        if not lnk.is_file() or self.outcomes.get('shortcut-file') == FAIL:
            return SKIP, 'the shortcut is missing or wrong (see shortcut-file): launching it would not test this game'
        result = launch_through_shortcut(lnk, self.dist_exe(), self.title)
        window = result.get('window')
        if not window:
            return FAIL, f'started {lnk.name} but {result.get("error") or "no window appeared"}'
        problems, notes = [], []
        if window.get('title') != self.title:
            problems.append(f'the window title is "{window.get("title")}", expected "{self.title}"')
        else:
            notes.append(f'window "{window["title"]}" (pid {window.get("pid")})')
        icons = result.get('icons') or {}
        big = icons.get('big') or icons.get('class')
        if not big and not icons.get('small'):
            problems.append('the window has no icon (WM_GETICON returned nothing)')
        for label, icon in (('big', big), ('small', icons.get('small'))):
            if icon and icon.get('rgba'):
                problem, note = self.window_icon_verdict(label, icon)
                if problem:
                    problems.append(problem)
                elif note:
                    notes.append(note)
        if result.get('error'):
            problems.append(str(result['error']))
        notes.append('closed gracefully' if result.get('closed') == 'graceful' else
                     'did not close on WM_CLOSE within 3 s: ended by the checker')
        if problems:
            return FAIL, '; '.join(problems)
        return PASS, '; '.join(notes)

    # ---- 11 smoke ----
    def check_smoke(self):
        # Do not let the lack of a display hide an incomplete/tampered package.
        if self.want_smoke and not self.blocked('identity'):
            files = package_integrity(self.project, self.plat)
            if inside(tempfile.gettempdir(), self.project.root):
                return FAIL, 'isolated smoke needs a temp directory outside the project: set TMPDIR (Windows: TEMP) and retry'
        reason = self.dynamic_skip(self.want_smoke, '--smoke', getattr(self, 'smoke_reason', None))
        if reason:
            return SKIP, reason
        project, identity = self.project, self.project.identity
        exe = self.dist_exe()
        scratch = project.root / '.blue-check'
        scratch.mkdir(exist_ok=True)
        prune_smoke_folders(scratch, keep=1)
        capture = scratch / f'smoke-{datetime.datetime.now().strftime("%Y%m%dT%H%M%S%f")}'  # must not exist yet
        arguments = [a.replace('{dir}', str(capture)) for a in (identity.smoke_args or DEFAULT_SMOKE_ARGS)]
        try:
            # Copy ONLY verified declarations outside the checkout. No stale asset, player file or
            # settings in dist may satisfy a missing dependency. Saves/settings created here are disposable.
            with tempfile.TemporaryDirectory(prefix='blue-package-') as temporary:
                isolated = Path(temporary)
                for name in files:
                    destination = isolated / name
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(package_file(project.dist, name), destination)
                done = run_process([isolated / exe.name, *arguments], cwd=isolated, timeout=SMOKE_TIMEOUT)
        except subprocess.TimeoutExpired:
            return FAIL, (f'{exe.name} {" ".join(arguments)} did not finish within {SMOKE_TIMEOUT} s '
                          '(smoke_args in assets/identity.json should make the game exit by itself)')
        (scratch / f'{capture.name}.log').write_text((done.stdout or '') + '\n--- stderr ---\n' + (done.stderr or ''),
                                                    encoding='utf-8')
        if done.returncode != 0:
            tail = (done.stderr or done.stdout or '').strip()[-300:]
            return FAIL, f'{exe.name} exited with code {done.returncode}: {tail}'
        if not capture.is_dir():
            return FAIL, (f'the game exited without creating {project.display(capture)}: does it accept the smoke_args '
                          f'({" ".join(arguments)})?')
        pngs = sorted(capture.rglob('*.png'))
        if not pngs:
            return FAIL, f'{project.display(capture)} contains no PNG captures'
        described, blank = [], []
        for png in pngs:
            width, height, rgba = decode_png(png.read_bytes())
            spread = luma_stddev(width, height, rgba)
            described.append(f'{png.name} {width}x{height}')
            if spread < SMOKE_MIN_STDDEV:
                blank.append(f'{png.name} is blank (luminance spread {spread:.2f})')
        if blank:
            return FAIL, '; '.join(blank) + f' [{project.display(capture)}]'
        return PASS, f'isolated declared-file package: {len(pngs)} PNG capture(s) in {project.display(capture)}: ' + ', '.join(described)


def prune_smoke_folders(scratch, keep=2):
    """Keep only the newest few smoke capture folders under .blue-check/ (they are evidence, not archives)."""
    folders = sorted(p for p in scratch.glob('smoke-*') if p.is_dir() and re.fullmatch(r'smoke-\d{8}T\d{12}', p.name))
    for old in folders[:-keep] if keep else folders:
        shutil.rmtree(old, ignore_errors=True)
        old.with_suffix('.log').unlink(missing_ok=True)


def base64_logo():
    """RGBA pixels of miniquad's built-in 32x32 window icon."""
    import base64  # noqa: PLC0415
    return zlib.decompress(base64.b64decode(MINIQUAD_LOGO_32))


def cmd_verify(project, folder=None, platform_name=None, launch=False, smoke=False, skip_package=False, quiet=False,
               launch_reason=None, smoke_reason=None):
    verifier = Verifier(project, folder, platform_name, launch, smoke, skip_package)
    verifier.launch_reason, verifier.smoke_reason = launch_reason, smoke_reason
    report = verifier.run()
    if not quiet:
        for check in report['checks']:
            log(f'  [{check["status"]:4}] {check["name"]}: {check["detail"]}')
        log('verify: ' + ('ok' if report['ok'] else f'FAILED. Next: {report["next"]}'))
    return report


def cmd_ship(project, folder=None, force=False, no_build=False, no_launch=False, no_smoke=False, platform_name=None):
    """package, shortcut, then verify with the launch and smoke checks."""
    result = {'ok': False, 'command': 'ship'}
    result['package'] = cmd_package(project, no_build)
    result['shortcut'] = cmd_shortcut(project, folder, force, platform_name)
    report = cmd_verify(project, folder, platform_name, launch=not no_launch, smoke=not no_smoke,
                        launch_reason='--no-launch' if no_launch else None,
                        smoke_reason='--no-smoke' if no_smoke else None)
    result['verify'] = report
    result['ok'] = bool(report['ok'])
    return result


# --------------------------------------------------------------------------------------------------
# info
# --------------------------------------------------------------------------------------------------


def cmd_info(project):
    info = {'ok': True, 'command': 'info', 'root': str(project.root), 'platform': project.platform,
            'python': sys.version.split()[0]}
    try:
        identity = project.identity
        info['identity'] = dataclasses.asdict(identity)
        info['identity_error'] = None
    except ConfigError as error:
        info['identity'], info['identity_error'] = None, str(error)
    try:
        info['exe'] = project.exe_stem()
        info['exe_file'] = project.exe_file()
    except ConfigError as error:
        info['exe'], info['exe_file'], info['exe_error'] = None, None, str(error)
    info['dist'] = str(project.dist)
    if info.get('exe'):
        exe = project.dist_exe
        info['dist_exe'] = {'path': str(exe), 'exists': exe.is_file(), 'bytes': exe.stat().st_size if exe.is_file() else None}
        info['dist_ico'] = {'path': str(project.dist_ico), 'exists': project.dist_ico.is_file()}
    info['icon'] = {'path': str(project.assets / 'icon.ico'), 'exists': (project.assets / 'icon.ico').is_file()}
    folders = desktop_folders()
    info['desktop'] = {key: str(value) if value else None for key, value in folders.items()}
    if folders['user'] and info.get('identity'):
        suffix = {'windows': '.lnk', 'linux': '.desktop', 'macos': '.command'}.get(project.platform, '')
        shortcut = folders['user'] / f'{project.identity.title}{suffix}'
        info['shortcut'] = {'path': str(shortcut), 'exists': shortcut.exists()}
    engine = project.engine()
    info['engine'] = {'path': str(engine) if engine else None, 'revision': engine_head(engine)}
    stamp = project.read_stamp()
    info['stamp'] = stamp
    return info


def print_info(info):
    identity = info.get('identity') or {}
    lines = [f'project   {info["root"]}', f'platform  {info["platform"]}']
    if identity:
        lines += [f'title     {identity["title"]}', f'tagline   {identity["tagline"]}', f'controls  {identity["controls"]}']
    else:
        lines.append(f'identity  INVALID: {info["identity_error"]}')
    lines.append(f'exe       {info.get("exe_file") or info.get("exe_error")}')
    if 'dist_exe' in info:
        lines.append(f'dist exe  {info["dist_exe"]["path"]} ({"present" if info["dist_exe"]["exists"] else "missing"})')
    lines.append(f'icon      {info["icon"]["path"]} ({"present" if info["icon"]["exists"] else "missing"})')
    desktop = info['desktop']
    lines.append(f'desktop   {desktop["user"] or "none"} (common: {desktop["common"] or "none"})')
    if 'shortcut' in info:
        lines.append(f'shortcut  {info["shortcut"]["path"]} ({"present" if info["shortcut"]["exists"] else "missing"})')
    engine = info['engine']
    lines.append(f'engine    {engine["path"] or "not found"}' + (f' @ {engine["revision"]}' if engine['revision'] else ''))
    stamp = info.get('stamp')
    if stamp:
        verified = stamp.get('verified') or {}
        lines.append(f'stamp     packaged {stamp.get("packaged_at")}; verified {verified.get("at") or "never"} '
                     f'(launch {verified.get("launch")}, smoke {verified.get("smoke")})')
    print('\n'.join(lines))


# --------------------------------------------------------------------------------------------------
# command line
# --------------------------------------------------------------------------------------------------


def build_parser():
    parser = argparse.ArgumentParser(prog='ship.py', description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True, metavar='command')
    platforms = ('windows', 'linux', 'macos')
    info = sub.add_parser('info', help='show what this tool resolved for the project')
    info.add_argument('--json', action='store_true', help='print the information as JSON')
    package = sub.add_parser('package', help='build the release exe and copy the game into dist/')
    package.add_argument('--no-build', action='store_true', help='package the existing release build')
    shortcut = sub.add_parser('shortcut', help='create or refresh the desktop launcher')
    shortcut.add_argument('--folder', help='put the launcher here instead of the OS desktop (CI, tests)')
    shortcut.add_argument('--force', action='store_true', help='replace a same-named launcher of another program')
    shortcut.add_argument('--platform', choices=platforms, help="write another platform's launcher into --folder")
    verify = sub.add_parser('verify', help='check identity, icon, wiring, package and shortcut')
    verify.add_argument('--folder', help='look for the launcher here instead of the OS desktop')
    verify.add_argument('--launch', action='store_true', help='start the game through its shortcut (Windows)')
    verify.add_argument('--smoke', action='store_true', help='run the exe with its smoke_args and check the captures')
    verify.add_argument('--skip-package', action='store_true', help='do not fail when dist/ has not been built yet')
    verify.add_argument('--platform', choices=platforms, help='check the launcher of another platform in --folder')
    verify.add_argument('--json', action='store_true', help='print only the JSON (no summary lines on stderr)')
    ship = sub.add_parser('ship', help='package, create the shortcut and verify with launch and smoke')
    ship.add_argument('--folder', help='put the launcher here instead of the OS desktop')
    ship.add_argument('--force', action='store_true', help='replace a same-named launcher of another program')
    ship.add_argument('--no-build', action='store_true', help='package the existing release build')
    ship.add_argument('--no-launch', action='store_true', help='skip the launch check')
    ship.add_argument('--no-smoke', action='store_true', help='skip the smoke run')
    return parser


def emit(result):
    print(json.dumps(result))
    sys.stdout.flush()


def main(argv=None, root=None):
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(errors='replace')
        except (AttributeError, ValueError):
            pass
    args = build_parser().parse_args(argv)
    root = Path(root) if root else Path(__file__).resolve().parents[1]
    try:
        project = load_project(root, getattr(args, 'platform', None))
        if args.command == 'info':
            info = cmd_info(project)
            if args.json:
                emit(info)
            else:
                print_info(info)
            return 0
        if args.command == 'package':
            result = cmd_package(project, args.no_build)
        elif args.command == 'shortcut':
            result = cmd_shortcut(project, args.folder, args.force, args.platform)
        elif args.command == 'verify':
            result = cmd_verify(project, args.folder, args.platform, args.launch, args.smoke, args.skip_package,
                                quiet=args.json)
        else:
            result = cmd_ship(project, args.folder, args.force, args.no_build, args.no_launch, args.no_smoke)
        emit(result)
        return 0 if result.get('ok') else 1
    except ShipError as error:
        log(f'ship.py {args.command}: {error}')
        emit({'ok': False, 'command': args.command, 'error': str(error)})
        return error.exit_code
    except KeyboardInterrupt:
        log('interrupted')
        return 130
    except Exception as error:  # noqa: BLE001 (a bug in this tool must still end in one JSON object)
        import traceback  # noqa: PLC0415
        log(traceback.format_exc())
        emit({'ok': False, 'command': args.command, 'error': f'internal error, {type(error).__name__}: {error}'})
        return 1


if __name__ == '__main__':
    sys.exit(main())
