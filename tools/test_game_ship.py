"""Tests for templates/game_ship.py: identity rules, icon codecs, perceptual signatures, package,
shortcut and verify (platform independent through --platform/--folder, plus real Windows shell tests).

Run from the repo root: python -m unittest tools.test_game_ship
The Windows tests drive the real shell (WScript.Shell, IShellItemImageFactory) in temp folders only;
they never touch the real Desktop. They skip on other systems. The launch test opens a window for
about a second and skips when there is no display or no C# compiler.
"""
import colorsys
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import random
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import zlib

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('game_ship', ROOT / 'templates/game_ship.py')
game_ship = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(game_ship)

# The same few test icons are fingerprinted over and over by different tests: remember the answers
# (failures are not cached, so the error cases still reach the real function).
_make_signature = game_ship.make_signature
_SIGNATURES = {}


def _cached_signature(width, height, rgba):
    key = (width, height, bytes(rgba))
    if key not in _SIGNATURES:
        _SIGNATURES[key] = _make_signature(width, height, rgba)
    return _SIGNATURES[key]


game_ship.make_signature = _cached_signature

WINDOWS = sys.platform == 'win32'
POSIX = os.name == 'posix'
CSC = Path(os.environ.get('SystemRoot', r'C:\Windows')) / 'Microsoft.NET' / 'Framework64' / 'v4.0.30319' / 'csc.exe'
NO_WINDOW = getattr(subprocess, 'CREATE_NO_WINDOW', 0)


# --------------------------------------------------------------------------------------------------
# Fixtures: icons, PNG/ICO encoders, fake game projects
# --------------------------------------------------------------------------------------------------


def _hsv(h, s, v):
    r, g, b = colorsys.hsv_to_rgb(h % 1.0, s, v)
    return round(r * 255), round(g * 255), round(b * 255)


def draw_icon(seed, size):
    """A deterministic rounded-square tile whose colours and glyph depend on `seed` (straight RGBA)."""
    rnd = random.Random(seed * 7919 + 13)
    hue = rnd.random()
    top, bottom = _hsv(hue, 0.85, 0.7), _hsv(hue + 0.25 + rnd.random() * 0.4, 0.9, 0.95)
    glyph = seed % 5
    cx, cy = 0.5 + (rnd.random() - 0.5) * 0.2, 0.5 + (rnd.random() - 0.5) * 0.2
    ink = (255, 255, 255) if seed % 2 else (20, 20, 30)
    out = bytearray(size * size * 4)
    for y in range(size):
        v = (y + 0.5) / size
        for x in range(size):
            u = (x + 0.5) / size
            dx, dy = max(abs(u - 0.5) - 0.28, 0.0), max(abs(v - 0.5) - 0.28, 0.0)
            distance = (dx * dx + dy * dy) ** 0.5 - 0.22
            alpha = min(1.0, max(0.0, 0.5 - distance * size))
            if alpha <= 0:
                continue
            t = (u + v) / 2
            colour = [top[k] * (1 - t) + bottom[k] * t for k in range(3)]
            gu, gv = u - cx, v - cy
            radius = (gu * gu + gv * gv) ** 0.5
            if glyph == 0:
                on = radius < 0.27
            elif glyph == 1:
                on = abs(gu) + abs(gv) < 0.3
            elif glyph == 2:
                on = (abs(gu) < 0.08 and abs(gv) < 0.3) or (abs(gv) < 0.08 and abs(gu) < 0.3)
            elif glyph == 3:
                on = 0.18 < radius < 0.32
            else:
                on = abs(gu) < 0.3 and int(v * 8) % 2 == 0
            if on:
                colour = list(ink)
            base = (y * size + x) * 4
            out[base:base + 4] = bytes((round(colour[0]), round(colour[1]), round(colour[2]), round(alpha * 255)))
    return bytes(out)


def upscale(rgba, size, target):
    """Nearest-neighbour resize (cheap: big frames of test icons are upscaled small renders)."""
    out = bytearray(target * target * 4)
    for y in range(target):
        sy = y * size // target
        for x in range(target):
            sx = x * size // target
            out[(y * target + x) * 4:(y * target + x) * 4 + 4] = rgba[(sy * size + sx) * 4:(sy * size + sx) * 4 + 4]
    return bytes(out)


def _chunk(kind, data):
    return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))


def _filter_row(kind, row, prior, bpp):
    out = bytearray()
    for i, value in enumerate(row):
        left = row[i - bpp] if i >= bpp else 0
        up = prior[i]
        upleft = prior[i - bpp] if i >= bpp else 0
        if kind == 0:
            predictor = 0
        elif kind == 1:
            predictor = left
        elif kind == 2:
            predictor = up
        elif kind == 3:
            predictor = (left + up) >> 1
        else:
            p = left + up - upleft
            pa, pb, pc = abs(p - left), abs(p - up), abs(p - upleft)
            predictor = left if pa <= pb and pa <= pc else up if pb <= pc else upleft
        out.append((value - predictor) & 255)
    return bytes(out)


def encode_png(width, height, rgba, filters=(0, 1, 2, 3, 4), colour_type=6, depth=8, palette=None, interlace=0):
    """A PNG encoder for tests: rows cycle through `filters` so every filter type is exercised.
    colour_type 6 (RGBA), 2 (RGB), 0 (grey), 4 (grey+alpha), 3 (palette, needs `palette`)."""
    rows = []
    for y in range(height):
        pixels = [rgba[(y * width + x) * 4:(y * width + x) * 4 + 4] for x in range(width)]
        if colour_type == 6:
            row = b''.join(pixels)
        elif colour_type == 2:
            row = b''.join(p[:3] for p in pixels)
        elif colour_type == 0:
            row = bytes(p[0] for p in pixels)
        elif colour_type == 4:
            row = b''.join(bytes((p[0], p[3])) for p in pixels)
        else:
            row = bytes(palette.index(tuple(p[:3])) for p in pixels)
        if depth == 16:
            row = b''.join(bytes((b, b)) for b in row)
        rows.append(row)
    channels = {6: 4, 2: 3, 0: 1, 4: 2, 3: 1}[colour_type]
    bpp = max(1, channels * depth // 8)
    raw, prior = bytearray(), bytes(len(rows[0]))
    for y, row in enumerate(rows):
        kind = filters[y % len(filters)]
        raw.append(kind)
        raw += _filter_row(kind, row, prior, bpp)
        prior = row
    extra = b''
    if colour_type == 3:
        extra = _chunk(b'PLTE', b''.join(bytes(c) for c in palette))
    header = struct.pack('>IIBBBBB', width, height, depth, colour_type, 0, 0, interlace)
    return (game_ship.PNG_SIGNATURE + _chunk(b'IHDR', header) + extra + _chunk(b'IDAT', zlib.compress(bytes(raw), 6))
            + _chunk(b'IEND', b''))


def bmp_frame(size, rgba):
    """A 32-bit ICO BMP frame: BITMAPINFOHEADER (height doubled), bottom-up BGRA rows, then the AND mask."""
    header = struct.pack('<IiiHHIIiiII', 40, size, size * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    pixels, mask = bytearray(), bytearray()
    mask_stride = ((size + 31) // 32) * 4
    for y in range(size - 1, -1, -1):
        row_mask = bytearray(mask_stride)
        for x in range(size):
            r, g, b, a = rgba[(y * size + x) * 4:(y * size + x) * 4 + 4]
            pixels += bytes((b, g, r, a))
            if a == 0:
                row_mask[x >> 3] |= 0x80 >> (x & 7)
        mask += row_mask
    return header + bytes(pixels) + bytes(mask)


def build_ico(frames, png_filters=(1,)):
    """[(size, rgba)] -> .ico bytes: BMP frames below 64 px, PNG frames from 64 px (like be2-tools icon)."""
    blobs = []
    for size, rgba in frames:
        blobs.append((size, encode_png(size, size, rgba, filters=png_filters) if size >= 64 else bmp_frame(size, rgba)))
    out = bytearray(struct.pack('<HHH', 0, 1, len(blobs)))
    offset = 6 + 16 * len(blobs)
    for size, blob in blobs:
        out += struct.pack('<BBBBHHII', size % 256, size % 256, 0, 0, 1, 32, len(blob), offset)
        offset += len(blob)
    for _size, blob in blobs:
        out += blob
    return bytes(out)


_ICON_CACHE = {}


def icon_set(seed, shift=0.0):
    """All the files of an assets/ icon set for `seed`, as {name: bytes}. `shift` rotates the hue (a
    near-duplicate of the same art)."""
    key = (seed, shift)
    if key not in _ICON_CACHE:
        small = {size: draw_icon(seed, size) for size in (16, 20, 24, 32, 40, 48, 64)}
        if shift:
            small = {size: hue_shift_rgba(rgba, shift) for size, rgba in small.items()}
        frames = [(size, rgba) for size, rgba in small.items()]
        for big in (96, 128, 256):
            frames.append((big, upscale(small[64], 64, big)))
        _ICON_CACHE[key] = {
            'icon.ico': build_ico(frames), 'icon_16.rgba': small[16], 'icon_32.rgba': small[32],
            'icon_64.rgba': small[64], 'icon.png': encode_png(256, 256, upscale(small[64], 64, 256), filters=(1,))}
    return _ICON_CACHE[key]


def hue_shift_rgba(rgba, degrees):
    out = bytearray(rgba)
    for i in range(0, len(rgba), 4):
        h, s, v = colorsys.rgb_to_hsv(rgba[i] / 255, rgba[i + 1] / 255, rgba[i + 2] / 255)
        r, g, b = colorsys.hsv_to_rgb((h + degrees / 360.0) % 1.0, s, v)
        out[i:i + 3] = bytes((round(r * 255), round(g * 255), round(b * 255)))
    return bytes(out)


def make_game(root, title='Zephyr Quest', seed=1, exe='zephyr', identity=None, package=None, ship_py=True,
              build_rs=True, source=True, gitignore='/target/\n/dist/\n', cargo_extra='', crate='zephyr_quest'):
    """Write a minimal game project (Cargo.toml, assets/, src/, build.rs, scripts/ship.py, .gitignore)."""
    root = Path(root)
    (root / 'assets').mkdir(parents=True, exist_ok=True)
    (root / 'src').mkdir(exist_ok=True)
    (root / 'scripts').mkdir(exist_ok=True)
    (root / 'Cargo.toml').write_text(
        f'[package]\nname = "{crate}"\nversion = "0.1.0"\nedition = "2021"\n\n[[bin]]\nname = "{exe}"\n'
        f'path = "src/main.rs"\n{cargo_extra}', encoding='utf-8')
    data = {'title': title, 'tagline': 'Catch the wind before it catches you.',
            'controls': 'WASD move, Space jump, mouse look', 'exe': exe}
    if package is not None:
        data['package'] = package
    data.update(identity or {})
    (root / 'assets/identity.json').write_text(json.dumps(data, indent=2), encoding='utf-8')
    for name, content in icon_set(seed).items():
        (root / 'assets' / name).write_bytes(content)
    if build_rs:
        (root / 'build.rs').write_text('// embeds assets/icon.ico into the exe\nfn main() {}\n', encoding='utf-8')
    if source:
        (root / 'src/main.rs').write_text(
            f'fn main() {{ let _icon = include_bytes!("../assets/icon_64.rgba"); let _title = "{title}"; }}\n',
            encoding='utf-8')
    if gitignore is not None:
        (root / '.gitignore').write_text(gitignore, encoding='utf-8')
    if ship_py:
        shutil.copy(ROOT / 'templates/game_ship.py', root / 'scripts/ship.py')
    return root


def fake_dist(root, plat='linux', png=True, stamp=True, exe_bytes=b'#!/bin/sh\necho fake game\n'):
    """A hand-made dist/ for platform `plat` (the real one comes from `package`)."""
    project = game_ship.Project(root, plat)
    project.dist.mkdir(exist_ok=True)
    exe = project.dist / project.exe_file(plat)
    exe.write_bytes(exe_bytes)
    assets = Path(root) / 'assets'
    shutil.copy(assets / 'icon.ico', project.dist_ico)
    if png:
        shutil.copy(assets / 'icon.png', project.dist_png)
    if stamp:
        project.write_stamp({'title': project.identity.title, 'exe': exe.name,
                             'exe_sha256': game_ship.sha256_file(exe), 'packaged_at': '2026-01-01T00:00:00Z',
                             'file_sha256': {p.name: game_ship.sha256_file(p) for p in project.dist.iterdir()
                                             if p.name != 'ship.json'},
                             'files': sorted(p.name for p in project.dist.iterdir() if p.name != 'ship.json')})
    return project


def run_ship(root, *args, env=None, timeout=300):
    """Run the game's own scripts/ship.py like a user would; returns (returncode, stdout json or None, stderr)."""
    merged = dict(os.environ, **(env or {}))
    done = subprocess.run([sys.executable, str(Path(root) / 'scripts/ship.py'), *args], capture_output=True, text=True,
                          encoding='utf-8', timeout=timeout, env=merged, creationflags=NO_WINDOW)
    out = None
    if done.stdout.strip():
        out = json.loads(done.stdout)
    return done.returncode, out, done.stderr


def check_by_name(report, name):
    return next(c for c in report['checks'] if c['name'] == name)


class TempTestCase(unittest.TestCase):
    """A per-test temp folder (TMP/TEMP decide which drive)."""

    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix='shiptest-', ignore_cleanup_errors=True)
        self.addCleanup(temp.cleanup)
        # Some Windows hosts (observed on GitHub's windows-latest runners) set TEMP/TMP to an
        # 8.3 short-name alias (...\RUNNER~1\...); resolve it once here so every path this fixture
        # builds matches the long form the shipping tool reports back.
        self.tmp = Path(temp.name).resolve()


# --------------------------------------------------------------------------------------------------
# Identity and titles
# --------------------------------------------------------------------------------------------------


def good_identity(**overrides):
    data = {'title': 'Zephyr Quest', 'tagline': 'Catch the wind.', 'controls': 'WASD move, Space jump'}
    data.update(overrides)
    return data


class TitleTests(unittest.TestCase):
    def test_reasonable_titles_are_accepted(self):
        for title in ['Bouncer', 'BOUNCER', 'Neon Relay', 'Café Ünï', '日本の冒険', 'A', 'x' * 60, '  padded  ',
                      'Rocket League 2: no wait', 'Zephyr-Quest_2 (beta)', 'Game Over Man']:
            if ':' in title:
                continue  # colons are not allowed in file names; covered below
            with self.subTest(title=title):
                self.assertEqual(game_ship.title_problems(title), [])

    def test_length_is_counted_after_trimming(self):
        self.assertTrue(game_ship.title_problems(''))
        self.assertTrue(game_ship.title_problems('   \t '))
        self.assertTrue(game_ship.title_problems('x' * 61))
        self.assertEqual(game_ship.title_problems('  ' + 'x' * 60 + '  '), [])

    def test_path_and_control_characters_are_rejected(self):
        for char in '\\/:*?"<>|':
            with self.subTest(char=char):
                self.assertTrue(game_ship.title_problems(f'Bad{char}Name'))
        for char in ['\x00', '\n', '\t', '\x1f', '\x7f']:
            with self.subTest(char=repr(char)):
                self.assertTrue(game_ship.title_problems(f'Bad{char}Name'))

    def test_placeholders_need_their_own_name(self):
        for title in ['Play', 'PLAY', ' game ', 'BlueEngine Game', 'blueengine  game', 'BlueEngine', 'Untitled',
                      'My Game', 'NEW GAME']:
            with self.subTest(title=title):
                problems = game_ship.title_problems(title)
                self.assertTrue(any('placeholder' in p for p in problems), problems)
        self.assertEqual(game_ship.title_problems('Play Bouncer'), [])
        self.assertEqual(game_ship.title_problems('Game Over'), [])

    def test_names_windows_cannot_use_are_rejected(self):
        for title in ['CON', 'nul', 'Aux.txt', 'COM1', 'lpt9', 'Ends with a dot.']:
            with self.subTest(title=title):
                self.assertTrue(game_ship.title_problems(title))
        self.assertEqual(game_ship.title_problems('Console Quest'), [])

    def test_non_strings_are_rejected(self):
        for value in [None, 5, ['x'], {'a': 1}]:
            self.assertTrue(game_ship.title_problems(value))


class IdentityTests(unittest.TestCase):
    def test_minimal_and_full_identities_validate(self):
        self.assertEqual(game_ship.identity_problems(good_identity()), [])
        full = good_identity(exe='zephyr', package=['game.json', 'maps'], smoke_args=['--capture', '{dir}'],
                             engine_revision='5978a214d99f')
        self.assertEqual(game_ship.identity_problems(full), [])
        identity = game_ship.Identity.from_dict(full)
        self.assertEqual((identity.title, identity.exe, identity.package), ('Zephyr Quest', 'zephyr', ['game.json', 'maps']))

    def test_missing_and_empty_fields_are_named(self):
        for key in ['title', 'tagline', 'controls']:
            data = good_identity()
            del data[key]
            with self.subTest(missing=key):
                self.assertTrue(any(key in p for p in game_ship.identity_problems(data)))
        for key in ['tagline', 'controls']:
            with self.subTest(empty=key):
                self.assertTrue(any(key in p for p in game_ship.identity_problems(good_identity(**{key: '  '}))))
        self.assertTrue(game_ship.identity_problems([]))
        self.assertTrue(game_ship.identity_problems('text'))

    def test_controls_are_limited_and_text_is_single_line(self):
        self.assertEqual(game_ship.identity_problems(good_identity(controls='c' * 200)), [])
        self.assertTrue(game_ship.identity_problems(good_identity(controls='c' * 201)))
        self.assertTrue(game_ship.identity_problems(good_identity(tagline='two\nlines')))

    def test_exe_is_a_file_stem(self):
        for exe in ['zephyr.exe', 'a/b', 'a\\b', ' padded', '', 5]:
            with self.subTest(exe=exe):
                self.assertTrue(game_ship.identity_problems(good_identity(exe=exe)))
        self.assertEqual(game_ship.identity_problems(good_identity(exe='zephyr-quest_2')), [])

    def test_package_entries_stay_inside_the_project(self):
        for entry in ['/etc', 'C:\\x', '../up', 'a/../../b', '', 'dist/x', 'target', '.git/config', '.']:
            with self.subTest(entry=entry):
                self.assertTrue(game_ship.identity_problems(good_identity(package=[entry])))
        self.assertTrue(game_ship.identity_problems(good_identity(package='maps')))
        self.assertEqual(game_ship.identity_problems(good_identity(package=['maps/main.json', 'assets/audio'])), [])

    def test_smoke_args_and_engine_revision(self):
        self.assertTrue(game_ship.identity_problems(good_identity(smoke_args='--capture')))
        self.assertTrue(game_ship.identity_problems(good_identity(smoke_args=[1])))
        self.assertTrue(game_ship.identity_problems(good_identity(engine_revision='not hex')))
        self.assertTrue(game_ship.identity_problems(good_identity(engine_revision='abc')))
        self.assertEqual(game_ship.identity_problems(good_identity(engine_revision='ABCDEF0123456789')), [])

    def test_all_problems_are_reported_together(self):
        problems = game_ship.identity_problems({'title': 'Play', 'tagline': '', 'controls': 5})
        self.assertGreaterEqual(len(problems), 3)

    def test_invalid_identity_raises_a_config_error(self):
        with self.assertRaises(game_ship.ConfigError) as caught:
            game_ship.Identity.from_dict(good_identity(title='Game'))
        self.assertEqual(caught.exception.exit_code, 2)

    def test_tooltip_is_tagline_plus_controls_and_capped_at_255(self):
        identity = game_ship.Identity.from_dict(good_identity())
        self.assertEqual(identity.description(), 'Catch the wind. WASD move, Space jump')
        long = game_ship.Identity.from_dict(good_identity(tagline='t' * 180, controls='c' * 150))
        text = long.description()
        self.assertEqual(len(text), 255)
        self.assertTrue(text.endswith('...'))
        self.assertTrue(text.startswith('t' * 180))

    def test_slug(self):
        self.assertEqual(game_ship.Identity.from_dict(good_identity(title='Zephyr Quest!')).slug(), 'zephyr-quest')
        self.assertEqual(game_ship.Identity.from_dict(good_identity(title='日本の冒険')).slug(), 'game')


class ExeNameTests(unittest.TestCase):
    def test_identity_exe_wins(self):
        self.assertEqual(game_ship.resolve_exe_stem('mine', {'package': {'name': 'other'}}), 'mine')

    def test_sole_bin_then_package_name(self):
        self.assertEqual(game_ship.resolve_exe_stem(None, {'package': {'name': 'crate'}, 'bin': [{'name': 'thebin'}]}),
                         'thebin')
        self.assertEqual(game_ship.resolve_exe_stem(None, {'package': {'name': 'crate'}}), 'crate')

    def test_several_bins_need_a_default_or_the_package_name(self):
        bins = [{'name': 'server'}, {'name': 'client'}]
        self.assertEqual(game_ship.resolve_exe_stem(None, {'package': {'name': 'x', 'default-run': 'client'}, 'bin': bins}),
                         'client')
        self.assertEqual(game_ship.resolve_exe_stem(None, {'package': {'name': 'server'}, 'bin': bins}), 'server')
        with self.assertRaises(game_ship.ConfigError):
            game_ship.resolve_exe_stem(None, {'package': {'name': 'x'}, 'bin': bins})

    def test_nothing_to_go_on(self):
        with self.assertRaises(game_ship.ConfigError):
            game_ship.resolve_exe_stem(None, {})


# --------------------------------------------------------------------------------------------------
# Cargo.toml parsing
# --------------------------------------------------------------------------------------------------

NEW_GAME_TOML = '''[package]
name = "my-game"
version = "0.1.0"
edition = "2021"

[features]
default = ["client"]
client = ["vesper3d/client", "dep:macroquad", "dep:windows-sys"]

[[bin]]
name = "my-game"
path = "src/main.rs"
required-features = ["client"]

[dependencies]
vesper3d = { package = "be2", path = "../BlueEngine", default-features = false }
macroquad = { optional = true, version = "=0.4.14", default-features = false, features = ["audio"] }
serde = { version = "1.0", features = ["derive"] }

[target.'cfg(windows)'.dependencies]
windows-sys = { optional = true, version = "=0.61.2", features = ["Win32_UI_WindowsAndMessaging"] }
'''

TRICKY_TOML = '''# a comment
[package]
name = 'literal-name'   # trailing comment
version = "1.2.3-beta.1"
description = "line one\\nline \\"two\\" \\u00e9"
authors = [
    "A <a@example.com>",   # comment inside
    "B",
]

[dependencies]
serde.workspace = true
"quoted.key" = { version = "1", features = [
  "a",
  "b",
] }

[dependencies.be2]
path = "../engine"

[[bin]]
name = "one"

[[bin]]
name = "two"
path = "src/two.rs"

[profile.release]
lto = "thin"
opt-level = 3
debug = false
'''


class TomlTests(unittest.TestCase):
    def test_fallback_parser_matches_tomllib(self):
        try:
            import tomllib
        except ImportError:
            self.skipTest('tomllib needs Python 3.11+')
        for text in (NEW_GAME_TOML, TRICKY_TOML):
            self.assertEqual(game_ship._MiniToml(text).parse(), tomllib.loads(text))

    def test_fallback_parser_reads_the_facts_we_need(self):
        document = game_ship._MiniToml(TRICKY_TOML).parse()
        self.assertEqual(document['package']['name'], 'literal-name')
        self.assertEqual(document['package']['description'], 'line one\nline "two" \u00e9')
        self.assertEqual(document['package']['authors'], ['A <a@example.com>', 'B'])
        self.assertEqual([b['name'] for b in document['bin']], ['one', 'two'])
        self.assertEqual(document['dependencies']['be2']['path'], '../engine')
        self.assertTrue(document['dependencies']['serde']['workspace'])
        self.assertEqual(document['dependencies']['quoted.key']['features'], ['a', 'b'])
        self.assertIs(document['profile']['release']['debug'], False)

    def test_new_game_manifest_shape(self):
        document = game_ship.parse_toml(NEW_GAME_TOML)
        self.assertEqual(document['bin'][0]['name'], 'my-game')
        self.assertEqual(document['dependencies']['vesper3d']['package'], 'be2')

    def test_broken_toml_is_an_error(self):
        for text in ['[package', 'name = ', 'a = [1, 2', 'x = "unterminated', 'a b = 1']:
            with self.subTest(text=text), self.assertRaises(ValueError):
                game_ship._MiniToml(text).parse()

    def test_engine_path_is_found_by_name_rename_or_table(self):
        with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as directory:
            base = Path(directory)
            (base / 'engine').mkdir()
            (base / 'game').mkdir()
            for manifest in [
                '[dependencies]\nbe2 = { path = "../engine" }\n',
                '[dependencies]\nvesper3d = { path = "../engine" }\n',
                '[dependencies]\nrenamed = { package = "be2", path = "../engine", default-features = false }\n',
                '[dependencies.be2]\npath = "../engine"\n',
                '[target.\'cfg(windows)\'.dependencies]\nbe2 = { path = "../engine" }\n',
            ]:
                cargo = game_ship.parse_toml('[package]\nname = "g"\n' + manifest)
                with self.subTest(manifest=manifest):
                    self.assertEqual(game_ship.find_engine_path(base / 'game', cargo), (base / 'engine').resolve())
            other = game_ship.parse_toml('[dependencies]\nserde = { path = "../engine" }\nbe2 = "1"\n')
            self.assertIsNone(game_ship.find_engine_path(base / 'game', other))
            missing = game_ship.parse_toml('[dependencies]\nbe2 = { path = "../nowhere" }\n')
            self.assertIsNone(game_ship.find_engine_path(base / 'game', missing))

    def test_read_cargo_reports_missing_and_broken_manifests(self):
        with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as directory:
            with self.assertRaises(game_ship.ConfigError):
                game_ship.read_cargo(directory)
            (Path(directory) / 'Cargo.toml').write_text('[package', encoding='utf-8')
            with self.assertRaises(game_ship.ConfigError):
                game_ship.read_cargo(directory)


# --------------------------------------------------------------------------------------------------
# PNG and ICO codecs
# --------------------------------------------------------------------------------------------------


def noise_image(width, height, seed=5, opaque=False):
    rnd = random.Random(seed)
    data = bytearray(rnd.getrandbits(8) for _ in range(width * height * 4))
    if opaque:
        data[3::4] = b'\xff' * (width * height)
    return bytes(data)


class PngTests(unittest.TestCase):
    def test_every_filter_type_round_trips(self):
        image = noise_image(23, 11)
        for kind in range(5):
            with self.subTest(filter=kind):
                self.assertEqual(game_ship.decode_png(encode_png(23, 11, image, filters=(kind,))), (23, 11, image))

    def test_filters_alternate_between_rows(self):
        image = noise_image(17, 12, seed=9)
        self.assertEqual(game_ship.decode_png(encode_png(17, 12, image, filters=(4, 2, 0, 3, 1)))[2], image)

    def test_rgb_grey_grey_alpha_and_palette_images(self):
        rgb = bytearray(noise_image(9, 6, seed=2, opaque=True))
        self.assertEqual(game_ship.decode_png(encode_png(9, 6, bytes(rgb), colour_type=2))[2], bytes(rgb))
        grey = bytearray(9 * 6 * 4)
        for i in range(9 * 6):
            grey[i * 4:i * 4 + 4] = bytes((i * 4 % 256,) * 3 + (255,))
        self.assertEqual(game_ship.decode_png(encode_png(9, 6, bytes(grey), colour_type=0))[2], bytes(grey))
        grey_alpha = bytearray(grey)
        grey_alpha[3::4] = bytes((i * 5 % 256 for i in range(9 * 6)))
        self.assertEqual(game_ship.decode_png(encode_png(9, 6, bytes(grey_alpha), colour_type=4))[2], bytes(grey_alpha))
        palette = [(255, 0, 0), (0, 255, 0), (0, 0, 255)]
        indexed = bytearray(9 * 6 * 4)
        for i in range(9 * 6):
            indexed[i * 4:i * 4 + 4] = bytes(palette[i % 3] + (255,))
        self.assertEqual(game_ship.decode_png(encode_png(9, 6, bytes(indexed), colour_type=3, palette=palette))[2],
                         bytes(indexed))

    def test_sixteen_bit_samples_keep_their_high_byte(self):
        image = bytes(noise_image(5, 4, seed=3, opaque=True))
        self.assertEqual(game_ship.decode_png(encode_png(5, 4, image, colour_type=2, depth=16))[2], image)

    def test_header_is_read_without_decoding(self):
        data = encode_png(7, 5, noise_image(7, 5))
        self.assertEqual(game_ship.png_header(data), (7, 5, 8, 6, 0))

    def test_damage_is_reported_not_ignored(self):
        good = encode_png(8, 8, noise_image(8, 8))
        with self.assertRaises(game_ship.ImageError):
            game_ship.decode_png(b'not a png at all, sorry')
        with self.assertRaises(game_ship.ImageError):
            game_ship.decode_png(good[:len(good) // 2])  # truncated
        flipped = bytearray(good)
        flipped[40] ^= 0xFF
        with self.assertRaises(game_ship.ImageError):
            game_ship.decode_png(bytes(flipped))  # CRC
        with self.assertRaises(game_ship.ImageError):
            game_ship.decode_png(encode_png(8, 8, noise_image(8, 8), interlace=1))
        with self.assertRaises(game_ship.ImageError):
            game_ship.decode_png(game_ship.PNG_SIGNATURE + _chunk(b'IHDR', struct.pack('>IIBBBBB', 0, 5, 8, 6, 0, 0, 0)))
        header = _chunk(b'IHDR', struct.pack('>IIBBBBB', 65535, 65535, 8, 6, 0, 0, 0))
        with self.assertRaises(game_ship.ImageError):
            game_ship.decode_png(game_ship.PNG_SIGNATURE + header + _chunk(b'IDAT', zlib.compress(b'x')) + _chunk(b'IEND', b''))

    def test_wrong_amount_of_pixel_data_is_rejected(self):
        header = _chunk(b'IHDR', struct.pack('>IIBBBBB', 4, 4, 8, 6, 0, 0, 0))
        short = _chunk(b'IDAT', zlib.compress(b'\x00' * 10))
        with self.assertRaises(game_ship.ImageError):
            game_ship.decode_png(game_ship.PNG_SIGNATURE + header + short + _chunk(b'IEND', b''))


class IcoTests(unittest.TestCase):
    def frames(self, seed=1):
        return [(size, draw_icon(seed, size)) for size in (16, 32, 64, 128)]

    def test_directory_and_frame_kinds(self):
        data = build_ico(self.frames())
        parsed = game_ship.parse_ico(data)
        self.assertEqual([(f.width, f.kind) for f in parsed], [(16, 'bmp'), (32, 'bmp'), (64, 'png'), (128, 'png')])
        self.assertTrue(all(f.header_bpp == 32 for f in parsed))
        self.assertEqual([f.header_h for f in parsed if f.kind == 'bmp'], [32, 64])

    def test_256_is_stored_as_zero_and_read_back(self):
        parsed = game_ship.parse_ico(build_ico([(256, upscale(draw_icon(3, 16), 16, 256))]))
        self.assertEqual((parsed[0].width, parsed[0].height), (256, 256))

    def test_bmp_and_png_frames_decode_to_the_source_pixels(self):
        frames = self.frames(seed=4)
        data = build_ico(frames)
        for (size, rgba), frame in zip(frames, game_ship.parse_ico(data)):
            with self.subTest(size=size):
                width, height, decoded = game_ship.decode_ico_frame(data, frame)
                self.assertEqual((width, height), (size, size))
                # transparent pixels may lose their colour bytes; compare what a viewer can see
                for i in range(0, len(rgba), 4):
                    if rgba[i + 3]:
                        self.assertEqual(decoded[i:i + 4], rgba[i:i + 4])
                    else:
                        self.assertEqual(decoded[i + 3], 0)

    def test_and_mask_supplies_alpha_for_frames_without_it(self):
        rgba = draw_icon(2, 16)
        frame = bytearray(bmp_frame(16, rgba))
        header = 40
        for i in range(3, 16 * 16 * 4, 4):
            frame[header + i] = 0  # drop the alpha channel, keep the AND mask
        data = struct.pack('<HHH', 0, 1, 1) + struct.pack('<BBBBHHII', 16, 16, 0, 0, 1, 32, len(frame), 22) + bytes(frame)
        _w, _h, decoded = game_ship.decode_ico_frame(data, game_ship.parse_ico(data)[0])
        self.assertEqual([decoded[i + 3] for i in range(0, len(decoded), 4)],
                         [255 if rgba[i + 3] else 0 for i in range(0, len(rgba), 4)])

    def test_malformed_files_are_rejected(self):
        good = build_ico(self.frames())
        bad_inputs = {
            'empty': b'', 'short': good[:4], 'wrong type': struct.pack('<HHH', 0, 2, 1) + good[6:],
            'reserved': struct.pack('<HHH', 1, 1, 4) + good[6:], 'zero images': struct.pack('<HHH', 0, 1, 0),
            'truncated directory': good[:20], 'truncated data': good[:len(good) - 500],
            'huge count': struct.pack('<HHH', 0, 1, 5000) + good[6:],
        }
        for name, data in bad_inputs.items():
            with self.subTest(name):
                with self.assertRaises(game_ship.ImageError):
                    game_ship.parse_ico(data)

    def test_entry_pointing_into_the_directory_or_past_the_end_is_rejected(self):
        good = bytearray(build_ico(self.frames()))
        overlapping = bytearray(good)
        struct.pack_into('<I', overlapping, 6 + 12, 4)  # first offset inside the directory
        past_end = bytearray(good)
        struct.pack_into('<I', past_end, 6 + 8, 10 ** 8)  # first size far beyond the file
        for name, data in {'overlapping': overlapping, 'past end': past_end}.items():
            with self.subTest(name), self.assertRaises(game_ship.ImageError):
                game_ship.parse_ico(bytes(data))

    def test_garbage_where_an_image_should_be(self):
        good = bytearray(build_ico(self.frames()))
        offset = struct.unpack_from('<I', good, 6 + 12)[0]
        good[offset:offset + 4] = struct.pack('<I', 7)  # BITMAPINFOHEADER size nonsense
        with self.assertRaises(game_ship.ImageError):
            game_ship.parse_ico(bytes(good))

    def test_real_world_ico_with_pillow_style_png_frames(self):
        png = encode_png(32, 32, draw_icon(6, 32))
        data = struct.pack('<HHH', 0, 1, 1) + struct.pack('<BBBBHHII', 32, 32, 0, 0, 0, 32, len(png), 22) + png
        frame = game_ship.parse_ico(data)[0]  # planes = 0 is common for PNG frames
        self.assertEqual((frame.kind, frame.planes), ('png', 0))
        self.assertEqual(game_ship.decode_ico_frame(data, frame)[0], 32)


# --------------------------------------------------------------------------------------------------
# Perceptual signatures
# --------------------------------------------------------------------------------------------------


def sig(seed, size=32, shift=0.0):
    rgba = draw_icon(seed, size)
    if shift:
        rgba = hue_shift_rgba(rgba, shift)
    return game_ship.make_signature(size, size, rgba)


def shifted_pixels(rgba, size, dx, dy):
    out = bytearray(len(rgba))
    for y in range(size):
        for x in range(size):
            sx, sy = x - dx, y - dy
            if 0 <= sx < size and 0 <= sy < size:
                out[(y * size + x) * 4:(y * size + x) * 4 + 4] = rgba[(sy * size + sx) * 4:(sy * size + sx) * 4 + 4]
    return bytes(out)


class SignatureTests(unittest.TestCase):
    SEEDS = (1, 2, 3, 4, 5, 6, 7, 8)

    def test_the_test_icons_are_distinct_from_each_other(self):
        sigs = {seed: sig(seed) for seed in self.SEEDS}
        for a in self.SEEDS:
            for b in self.SEEDS:
                if a < b:
                    with self.subTest(a=a, b=b):
                        self.assertFalse(game_ship.too_similar(sigs[a], sigs[b]), game_ship.signature_distance(sigs[a], sigs[b]))

    def test_identical_images_have_zero_distance(self):
        a, b = sig(3), sig(3)
        self.assertEqual(game_ship.signature_distance(a, b), (0.0, 0.0))
        self.assertTrue(game_ship.too_similar(a, b))
        self.assertTrue(game_ship.same_picture(a, b))

    def test_distance_is_symmetric_and_bounded(self):
        a, b = sig(1), sig(5)
        self.assertEqual(game_ship.signature_distance(a, b), game_ship.signature_distance(b, a))
        shape, colour = game_ship.signature_distance(a, b)
        self.assertTrue(0 <= shape <= 1 and 0 <= colour <= 1)

    def test_near_duplicates_are_too_similar(self):
        for seed in (1, 2, 3, 4):
            base = sig(seed)
            rgba = draw_icon(seed, 32)
            variants = {
                'hue +10': game_ship.make_signature(32, 32, hue_shift_rgba(rgba, 10)),
                'hue -10': game_ship.make_signature(32, 32, hue_shift_rgba(rgba, -10)),
                'brightness +3%': game_ship.make_signature(32, 32, bytes(
                    min(255, round(v * 1.03)) if i % 4 != 3 else v for i, v in enumerate(rgba))),
                'brightness noise 3%': game_ship.make_signature(32, 32, noisy(rgba, 3, seed)),
                '1 px right': game_ship.make_signature(32, 32, shifted_pixels(rgba, 32, 1, 0)),
                '1 px down and left': game_ship.make_signature(32, 32, shifted_pixels(rgba, 32, -1, 1)),
                'rescaled from 64 px': game_ship.make_signature(64, 64, draw_icon(seed, 64)),
                'rescaled from 48 px': game_ship.make_signature(48, 48, draw_icon(seed, 48)),
            }
            for name, variant in variants.items():
                with self.subTest(seed=seed, variant=name):
                    self.assertTrue(game_ship.too_similar(base, variant), game_ship.signature_distance(base, variant))

    def test_same_art_at_other_sizes_is_the_same_picture(self):
        base = sig(2, 64)
        for size in (16, 32, 48, 96):
            with self.subTest(size=size):
                self.assertTrue(game_ship.same_picture(base, sig(2, size)), game_ship.signature_distance(base, sig(2, size)))

    def test_different_art_is_not_the_same_picture(self):
        self.assertFalse(game_ship.same_picture(sig(1), sig(2)))

    def test_same_layout_in_another_palette_is_distinct(self):
        rgba = draw_icon(5, 32)
        shifted = game_ship.make_signature(32, 32, hue_shift_rgba(rgba, 120))
        shape, colour = game_ship.signature_distance(sig(5), shifted)
        self.assertLessEqual(shape, game_ship.SIMILAR_SHAPE)
        self.assertGreater(colour, game_ship.SIMILAR_COLOUR)
        self.assertFalse(game_ship.too_similar(sig(5), shifted))

    def test_flat_and_empty_images(self):
        flat = game_ship.make_signature(32, 32, bytes((90, 90, 90, 255)) * 1024)
        empty = game_ship.make_signature(32, 32, bytes(32 * 32 * 4))
        self.assertEqual(flat.coverage, 1.0)
        self.assertEqual(empty.coverage, 0.0)
        self.assertEqual(game_ship.signature_distance(empty, flat)[1], 1.0)
        self.assertEqual(game_ship.signature_distance(empty, empty)[1], 1.0)

    def test_signature_serialises(self):
        original = sig(4)
        self.assertEqual(game_ship.Signature.from_dict(json.loads(json.dumps(original.to_dict()))).hashes, original.hashes)

    def test_pixel_data_must_match_the_size(self):
        with self.assertRaises(game_ship.ImageError):
            game_ship.make_signature(4, 4, b'\x00' * 10)
        with self.assertRaises(game_ship.ImageError):
            game_ship.make_signature(0, 4, b'')

    def test_box_resample_averages(self):
        self.assertEqual(game_ship.box_resample([1.0, 3.0, 5.0, 7.0], 2, 2, 1), [4.0])
        constant = game_ship.box_resample([7.0] * 25, 5, 5, 3)
        self.assertTrue(all(abs(v - 7.0) < 1e-9 for v in constant))
        up = game_ship.box_resample([1.0, 2.0, 3.0, 4.0], 2, 2, 4)
        self.assertEqual(up[0], 1.0)
        self.assertEqual(up[-1], 4.0)

    def test_image_statistics(self):
        flat = bytes((10, 10, 10, 255)) * 64
        self.assertEqual(game_ship.luma_stddev(8, 8, flat), 0.0)
        half = bytes((0, 0, 0, 255)) * 32 + bytes((255, 255, 255, 255)) * 32
        self.assertAlmostEqual(game_ship.luma_stddev(8, 8, half), 127.5, delta=0.1)
        self.assertEqual(game_ship.count_colours(half), 2)
        self.assertEqual(game_ship.count_colours(bytes((1, 2, 3, 0)) * 4), 0)
        self.assertEqual(game_ship.opaque_share(half), 1.0)
        self.assertEqual(game_ship.opaque_share(bytes(16)), 0.0)

    def test_miniquads_default_icon_is_embedded_and_recognised(self):
        pixels = game_ship.base64_logo()
        self.assertEqual(len(pixels), 32 * 32 * 4)
        logo = game_ship.make_signature(32, 32, pixels)
        self.assertTrue(game_ship.too_similar(logo, game_ship.make_signature(32, 32, pixels)))
        self.assertFalse(game_ship.too_similar(logo, sig(1)))
        # the same logo drawn at 64 px (miniquad ships 16/32/64) is still the logo
        self.assertTrue(game_ship.too_similar(logo, game_ship.make_signature(64, 64, upscale(pixels, 32, 64))))


def noisy(rgba, percent, seed):
    rnd = random.Random(seed)
    out = bytearray(rgba)
    for i in range(0, len(rgba), 4):
        factor = 1 + rnd.uniform(-percent, percent) / 100
        for c in range(3):
            out[i + c] = max(0, min(255, round(rgba[i + c] * factor)))
    return bytes(out)


# --------------------------------------------------------------------------------------------------
# package
# --------------------------------------------------------------------------------------------------


def call_main(root, *args, env=None):
    """Run the CLI in-process; returns (exit code, decoded stdout JSON or None, stderr text)."""
    out, err = io.StringIO(), io.StringIO()
    with mock.patch.dict(os.environ, env or {}), contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = game_ship.main(list(args), root=root)
    text = out.getvalue().strip()
    return code, (json.loads(text) if text else None), err.getvalue()


def exe_name(stem='zephyr'):
    return stem + ('.exe' if WINDOWS else '')


class PackageTests(TempTestCase):
    def setUp(self):
        super().setUp()
        self.root = make_game(self.tmp / 'game', exe='zephyr')
        self.target = self.tmp / 'build output'
        (self.target / 'release').mkdir(parents=True)
        self.built = self.target / 'release' / exe_name()
        self.built.write_bytes(b'MZ built exe v1')
        self.env = {'CARGO_TARGET_DIR': str(self.target)}

    def package(self, *extra):
        return call_main(self.root, 'package', '--no-build', *extra, env=self.env)

    def test_copies_exe_icons_and_extras_and_writes_the_stamp(self):
        (self.root / 'game.json').write_text('{"map": "maps/main.json"}', encoding='utf-8')
        (self.root / 'maps/sub').mkdir(parents=True)
        (self.root / 'maps/main.json').write_text('{}', encoding='utf-8')
        (self.root / 'maps/sub/deep.json').write_text('{"deep": true}', encoding='utf-8')
        self.write_identity(package=['game.json', 'maps'])
        code, result, _err = self.package()
        self.assertEqual(code, 0, result)
        dist = self.root / 'dist'
        self.assertEqual((dist / exe_name()).read_bytes(), b'MZ built exe v1')
        self.assertEqual((dist / 'zephyr.ico').read_bytes(), (self.root / 'assets/icon.ico').read_bytes())
        self.assertEqual((dist / 'zephyr.png').read_bytes(), (self.root / 'assets/icon.png').read_bytes())
        self.assertEqual((dist / 'maps/sub/deep.json').read_text(encoding='utf-8'), '{"deep": true}')
        self.assertEqual((dist / 'game.json').read_text(encoding='utf-8'), '{"map": "maps/main.json"}')
        stamp = json.loads((dist / 'ship.json').read_text(encoding='utf-8'))
        self.assertEqual(stamp['title'], 'Zephyr Quest')
        self.assertEqual(stamp['exe'], exe_name())
        self.assertEqual(stamp['exe_sha256'], game_ship.sha256_file(dist / exe_name()))
        self.assertEqual(stamp['ico_sha256'], game_ship.sha256_file(dist / 'zephyr.ico'))
        self.assertRegex(stamp['packaged_at'], r'^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$')
        self.assertIn('maps/sub/deep.json', stamp['files'])
        self.assertEqual(result['built_from'], str(self.built))
        self.assertEqual(result['ok'], True)

    def write_identity(self, **changes):
        path = self.root / 'assets/identity.json'
        data = json.loads(path.read_text(encoding='utf-8'))
        data.update(changes)
        path.write_text(json.dumps(data), encoding='utf-8')

    def test_repackaging_replaces_files_and_removes_only_what_it_installed(self):
        (self.root / 'extras').mkdir()
        (self.root / 'extras/a.txt').write_text('a', encoding='utf-8')
        (self.root / 'extras/b.txt').write_text('b', encoding='utf-8')
        self.write_identity(package=['extras'])
        self.assertEqual(self.package()[0], 0)
        dist = self.root / 'dist'
        (dist / 'records.json').write_text('{"best": 12345}', encoding='utf-8')  # the game's own save data
        (dist / 'extras' / 'user.txt').write_text('mine', encoding='utf-8')
        (self.root / 'extras/b.txt').unlink()
        self.built.write_bytes(b'MZ built exe v2')
        self.assertEqual(self.package()[0], 0)
        self.assertEqual((dist / exe_name()).read_bytes(), b'MZ built exe v2')
        self.assertFalse((dist / 'extras/b.txt').exists())  # no longer part of the package
        self.assertTrue((dist / 'extras/a.txt').exists())
        self.assertEqual((dist / 'records.json').read_text(encoding='utf-8'), '{"best": 12345}')
        self.assertEqual((dist / 'extras/user.txt').read_text(encoding='utf-8'), 'mine')
        self.write_identity(package=[])
        self.assertEqual(self.package()[0], 0)
        self.assertFalse((dist / 'extras/a.txt').exists())
        self.assertTrue((dist / 'extras/user.txt').exists())  # its folder stays because it is not empty
        self.assertTrue((dist / 'records.json').exists())

    def test_empty_folders_of_removed_extras_disappear(self):
        (self.root / 'maps/sub').mkdir(parents=True)
        (self.root / 'maps/sub/x.json').write_text('{}', encoding='utf-8')
        self.write_identity(package=['maps'])
        self.package()
        self.assertTrue((self.root / 'dist/maps/sub/x.json').exists())
        self.write_identity(package=[])
        self.package()
        self.assertFalse((self.root / 'dist/maps').exists())

    def test_the_stamp_keeps_the_shortcut_and_forgets_old_verification(self):
        self.package()
        project = game_ship.Project(self.root)
        stamp = project.read_stamp()
        stamp['shortcut'] = {'path': 'somewhere', 'created_at': 'then'}
        stamp['verified'] = {'at': 'then', 'launch': True, 'smoke': True}
        stamp['custom'] = 'kept'
        project.write_stamp(stamp)
        self.built.write_bytes(b'MZ a new build')
        self.package()
        stamp = project.read_stamp()
        self.assertEqual(stamp['shortcut'], {'path': 'somewhere', 'created_at': 'then'})
        self.assertEqual(stamp['custom'], 'kept')
        self.assertNotIn('verified', stamp)

    def test_missing_icon_is_a_config_error_before_any_build(self):
        (self.root / 'assets/icon.ico').unlink()
        with mock.patch.object(game_ship, 'cargo_build_release', side_effect=AssertionError('built anyway')):
            code, result, err = call_main(self.root, 'package', env=self.env)
        self.assertEqual(code, 2)
        self.assertIn('icon.ico', result['error'])
        self.assertIn('be2-tools icon', result['error'])
        self.assertFalse((self.root / 'dist').exists())

    def test_missing_png_is_only_a_gap_in_the_package(self):
        (self.root / 'assets/icon.png').unlink()
        code, result, _err = self.package()
        self.assertEqual(code, 0)
        self.assertFalse((self.root / 'dist/zephyr.png').exists())

    def test_missing_build_output_says_how_to_build(self):
        self.built.unlink()
        code, result, _err = self.package()
        self.assertEqual(code, 1)
        self.assertIn('cargo build --release', result['error'])
        self.assertIn(exe_name(), result['error'])

    def test_missing_extra_is_a_config_error(self):
        self.write_identity(package=['maps'])
        code, result, _err = self.package()
        self.assertEqual(code, 2)
        self.assertIn("'maps' does not exist", result['error'])

    def test_extras_cannot_collide_with_files_the_tool_writes(self):
        (self.root / 'ship.json').write_text('{}', encoding='utf-8')
        self.write_identity(package=['ship.json'])
        code, result, _err = self.package()
        self.assertEqual(code, 2)
        self.assertIn('collides', result['error'])

    def test_a_tampered_stamp_cannot_make_it_delete_outside_dist(self):
        self.package()
        outside = self.tmp / 'precious.txt'
        outside.write_text('keep me', encoding='utf-8')
        project = game_ship.Project(self.root)
        stamp = project.read_stamp()
        stamp['files'] = list(stamp['files']) + ['../../precious.txt', '../precious.txt', str(outside), 5]
        project.write_stamp(stamp)
        self.assertEqual(self.package()[0], 0)
        self.assertEqual(outside.read_text(encoding='utf-8'), 'keep me')

    def test_an_exe_in_use_is_reported_plainly(self):
        real_replace = os.replace

        def refuse(source, destination):
            if str(destination).endswith(exe_name()):
                raise PermissionError(13, 'in use')
            return real_replace(source, destination)

        with mock.patch.object(game_ship.os, 'replace', side_effect=refuse):
            code, result, _err = self.package()
        self.assertEqual(code, 1)
        self.assertIn('in use', result['error'])
        self.assertFalse(list((self.root / 'dist').glob('.*.tmp')))  # no half-copied temp files left

    def test_packaging_twice_without_changes_is_stable(self):
        self.package()
        first = {p.name: p.read_bytes() for p in (self.root / 'dist').iterdir() if p.name != 'ship.json'}
        self.package()
        second = {p.name: p.read_bytes() for p in (self.root / 'dist').iterdir() if p.name != 'ship.json'}
        self.assertEqual(first, second)

    def install_user_files(self):
        """What a running game writes beside its exe."""
        dist = self.root / 'dist'
        (dist / 'saves').mkdir(parents=True, exist_ok=True)
        files = {'saves/quick.be2save': b'\x00binary save', 'player_save.json': b'{"level": 7}',
                 'settings.json': b'{"volume": 3}', 'records.txt': b'best 12345', 'crash.log': b'log line',
                 'saves/slot 2.be2save': b'more'}
        for name, content in files.items():
            (dist / name).write_bytes(content)
        return files

    def assert_files_intact(self, files):
        for name, content in files.items():
            self.assertEqual((self.root / 'dist' / name).read_bytes(), content, name)

    def test_saves_settings_and_player_data_beside_the_exe_survive_repackaging(self):
        (self.root / 'game.json').write_text('{"map": "maps/main.json"}', encoding='utf-8')
        (self.root / 'notes.txt').write_text('shipped notes', encoding='utf-8')
        self.write_identity(package=['game.json', 'notes.txt'])
        self.assertEqual(self.package()[0], 0)
        files = self.install_user_files()
        self.built.write_bytes(b'MZ built exe v2')
        self.write_identity(package=['game.json'])  # an extra is dropped from the package
        code, result, _err = self.package()
        self.assertEqual(code, 0, result)
        self.assert_files_intact(files)
        self.assertFalse((self.root / 'dist/notes.txt').exists())  # the stale packaged extra is gone
        self.assertEqual(result['removed'], ['notes.txt'])
        self.assertEqual((self.root / 'dist' / exe_name()).read_bytes(), b'MZ built exe v2')  # and the rest refreshed
        self.assertTrue((self.root / 'dist/game.json').exists())
        self.assertEqual(self.package()[0], 0)  # a third run changes nothing about the player's files either
        self.assert_files_intact(files)
        stamp = game_ship.Project(self.root).read_stamp()
        self.assertFalse([n for n in stamp['files'] if 'save' in n or n in files])  # they were never claimed

    def test_the_tool_only_replaces_files_it_installs_and_claims_only_those(self):
        self.package()
        files = self.install_user_files()
        self.write_identity(package=[])
        self.package()
        stamp = game_ship.Project(self.root).read_stamp()
        self.assertEqual(sorted(stamp['files']), sorted([exe_name(), 'zephyr.ico', 'zephyr.png']))
        self.assert_files_intact(files)

    def test_a_missing_or_damaged_stamp_overwrites_package_files_and_deletes_nothing(self):
        (self.root / 'a.txt').write_text('a v1', encoding='utf-8')
        (self.root / 'b.txt').write_text('b v1', encoding='utf-8')
        self.write_identity(package=['a.txt', 'b.txt'])
        self.package()
        files = self.install_user_files()
        stamp_path = self.root / 'dist/ship.json'
        for label, damage in {'missing': lambda: stamp_path.unlink(),
                              'not JSON': lambda: stamp_path.write_text('{ broken', encoding='utf-8'),
                              'wrong shape': lambda: stamp_path.write_text('["files"]', encoding='utf-8'),
                              'files is not a list': lambda: stamp_path.write_text('{"files": "b.txt"}', encoding='utf-8'),
                              'files hold junk': lambda: stamp_path.write_text('{"files": [1, null, {"x": 1}, "../x"]}',
                                                                              encoding='utf-8')}.items():
            with self.subTest(label):
                (self.root / 'a.txt').write_text(f'a {label}', encoding='utf-8')
                (self.root / 'dist/b.txt').write_text('stale b', encoding='utf-8')
                self.write_identity(package=['a.txt'])  # b.txt is no longer part of the package
                (self.root / 'dist/stamp-backup').write_text('x', encoding='utf-8')
                damage()
                code, result, _err = self.package()
                self.assertEqual(code, 0, result)
                self.assertEqual(result['removed'], [])  # nothing was known to be ours, so nothing is deleted
                self.assertEqual((self.root / 'dist/b.txt').read_text(encoding='utf-8'), 'stale b')
                self.assertEqual((self.root / 'dist/a.txt').read_text(encoding='utf-8'), f'a {label}')  # overwritten
                self.assertTrue((self.root / 'dist/stamp-backup').exists())
                self.assert_files_intact(files)
                self.write_identity(package=['a.txt', 'b.txt'])
                self.package()  # b.txt is claimed again for the next round

    def test_a_shipped_file_the_player_changed_is_left_alone_when_it_leaves_the_package(self):
        (self.root / 'defaults.json').write_text('{"volume": 5}', encoding='utf-8')
        self.write_identity(package=['defaults.json'])
        self.package()
        (self.root / 'dist/defaults.json').write_text('{"volume": 9}', encoding='utf-8')  # the player's own settings now
        self.write_identity(package=[])
        code, result, _err = self.package()
        self.assertEqual(code, 0, result)
        self.assertEqual(result['kept_modified'], ['defaults.json'])
        self.assertEqual((self.root / 'dist/defaults.json').read_text(encoding='utf-8'), '{"volume": 9}')
        self.assertNotIn('defaults.json', game_ship.Project(self.root).read_stamp()['files'])  # no longer claimed
        # an unchanged shipped file that leaves the package is still removed
        (self.root / 'defaults.json').write_text('{"volume": 5}', encoding='utf-8')
        self.write_identity(package=['defaults.json'])
        self.package()
        (self.root / 'dist/defaults.json').write_text('{"volume": 5}', encoding='utf-8')
        self.write_identity(package=[])
        self.assertEqual(self.package()[1]['removed'], ['defaults.json'])

    def test_nested_extras_keep_their_relative_path_in_dist(self):
        (self.root / 'assets/audio').mkdir(parents=True)
        (self.root / 'assets/audio/theme.wav').write_bytes(b'RIFF wave')
        (self.root / 'assets/audio/sfx').mkdir()
        (self.root / 'assets/audio/sfx/hit.wav').write_bytes(b'RIFF hit')
        (self.root / 'config').mkdir()
        (self.root / 'config/keys.json').write_text('{}', encoding='utf-8')
        self.write_identity(package=['assets/audio', 'config/keys.json'])
        code, result, _err = self.package()
        self.assertEqual(code, 0, result)
        dist = self.root / 'dist'
        self.assertEqual((dist / 'assets/audio/theme.wav').read_bytes(), b'RIFF wave')
        self.assertEqual((dist / 'assets/audio/sfx/hit.wav').read_bytes(), b'RIFF hit')
        self.assertTrue((dist / 'config/keys.json').exists())
        self.assertFalse((dist / 'audio').exists())
        self.assertEqual(self.package()[0], 0)

    def test_exe_name_from_cargo_when_identity_does_not_say(self):
        self.write_identity(exe=None)
        data = json.loads((self.root / 'assets/identity.json').read_text(encoding='utf-8'))
        data.pop('exe', None)
        (self.root / 'assets/identity.json').write_text(json.dumps(data), encoding='utf-8')
        code, result, _err = self.package()
        self.assertEqual(code, 0, result)
        self.assertTrue((self.root / 'dist' / exe_name('zephyr')).exists())  # the sole [[bin]]


class SmokePackageTests(TempTestCase):
    """Run a stand-in game in the real staged filesystem on every platform."""

    def setUp(self):
        super().setUp()
        import base64
        self.root = make_game(self.tmp / 'source project', exe='zephyr')
        png = base64.b64encode(encode_png(32, 32, draw_icon(3, 32))).decode('ascii')
        script = (
            'import base64, pathlib, sys\n'
            'asset = pathlib.Path("runtime/data.txt")\n'
            'if not asset.is_file(): asset = pathlib.Path("../assets/data.txt")\n'
            'if not asset.is_file(): sys.exit("runtime asset missing")\n'
            'pathlib.Path("settings.json").write_text("smoke settings")\n'
            'out = pathlib.Path(sys.argv[sys.argv.index("--capture") + 1]); out.mkdir()\n'
            f'(out / "world.png").write_bytes(base64.b64decode({png!r}))\n'
        )
        self.project = fake_dist(self.root, exe_bytes=script.encode())
        self.verifier = game_ship.Verifier(self.project, smoke=True)
        self.verifier.outcomes['identity'] = 'pass'
        self.source_asset = self.root / 'assets/data.txt'
        self.source_asset.write_text('present only in source')
        self.dist_asset = self.project.dist / 'runtime/data.txt'
        self.dist_asset.parent.mkdir()
        self.dist_asset.write_text('packaged data')
        self.settings = self.project.dist / 'settings.json'
        self.settings.write_text('player settings')

    def declare_asset(self):
        stamp = self.project.read_stamp()
        stamp['files'].append('runtime/data.txt')
        stamp['file_sha256'] = {name: game_ship.sha256_file(self.project.dist / name) for name in stamp['files']}
        self.project.write_stamp(stamp)

    def smoke(self):
        run = game_ship.run_process
        # The fixture is Python code instead of a native binary; execution is still
        # a real subprocess with the production cwd, environment and staged files.
        def launch(args, **kwargs):
            return run([sys.executable, *args], **kwargs)
        with mock.patch.object(game_ship, 'has_display', return_value=True), \
                mock.patch.object(game_ship, 'run_process', side_effect=launch):
            return self.verifier.check_smoke()

    def test_source_and_unlisted_dist_assets_cannot_mask_a_broken_package(self):
        status, detail = self.smoke()
        self.assertEqual(status, 'fail', detail)
        self.assertIn('runtime asset missing', detail)
        self.assertEqual(self.settings.read_text(), 'player settings')
        self.assertTrue(self.source_asset.exists())

    def test_declared_assets_run_from_a_clean_package_without_changing_player_files(self):
        self.declare_asset()
        status, detail = self.smoke()
        self.assertEqual(status, 'pass', detail)
        self.assertIn('isolated declared-file package', detail)
        self.assertEqual(self.settings.read_text(), 'player settings')

    def test_missing_or_modified_shipped_assets_fail_before_launch(self):
        self.declare_asset()
        self.dist_asset.write_text('damaged')
        with self.assertRaisesRegex(game_ship.ShipError, 'modified or damaged'):
            self.smoke()
        self.dist_asset.unlink()
        with self.assertRaisesRegex(game_ship.ShipError, 'is missing'):
            self.smoke()

    def test_manifest_paths_cannot_escape_or_copy_files_from_source(self):
        for name in ['../assets/data.txt', '/outside', 'C:/outside', 'runtime/../data.txt', 'runtime\\data.txt']:
            stamp = self.project.read_stamp()
            stamp['files'] = [self.project.exe_file(), name]
            self.project.write_stamp(stamp)
            with self.subTest(name=name), self.assertRaisesRegex(game_ship.ShipError, 'unsafe'):
                game_ship.stage_smoke_package(self.project, self.tmp / 'staged', self.project.exe_file())


class FakeCargo:
    """A `cargo` on PATH that answers build/metadata like the real one (JSON messages, exit codes)."""

    def __init__(self, directory):
        self.directory = Path(directory)
        self.directory.mkdir(parents=True, exist_ok=True)
        self.script = self.directory / 'fake_cargo.py'
        self.script.write_text('''import json, os, sys
mode = os.environ.get("FAKE_CARGO_MODE", "ok")
if sys.argv[1] == "metadata":
    print(json.dumps({"target_directory": os.environ["FAKE_CARGO_TARGET"]}))
    sys.exit(0)
sys.stderr.write("   Compiling zephyr v0.1.0 (rendered diagnostic)\\n")
if mode == "fail":
    sys.stderr.write("error: could not compile\\n")
    sys.exit(101)
print("this line is not json")
print(json.dumps({"reason": "compiler-artifact", "target": {"kind": ["lib"], "name": "zephyr"}, "executable": None}))
print(json.dumps({"reason": "compiler-artifact", "target": {"kind": ["bin"], "name": "other"}, "executable": os.environ["FAKE_CARGO_OTHER"]}))
print(json.dumps({"reason": "compiler-artifact", "target": {"kind": ["bin"], "name": "zephyr"}, "executable": os.environ["FAKE_CARGO_EXE"]}))
print(json.dumps({"reason": "build-finished", "success": True}))
''', encoding='utf-8')
        if WINDOWS:
            (self.directory / 'cargo.cmd').write_text(f'@echo off\r\n"{sys.executable}" "{self.script}" %*\r\n', encoding='ascii')
        else:
            launcher = self.directory / 'cargo'
            launcher.write_text(f'#!/bin/sh\nexec "{sys.executable}" "{self.script}" "$@"\n', encoding='utf-8')
            launcher.chmod(0o755)

    def env(self, **extra):
        path = str(self.directory) + os.pathsep + os.environ.get('PATH', '')
        return dict({'PATH': path, 'CARGO_TARGET_DIR': ''}, **extra)


class CargoTests(TempTestCase):
    def setUp(self):
        super().setUp()
        self.root = make_game(self.tmp / 'game', exe='zephyr')
        self.cargo = FakeCargo(self.tmp / 'bin')
        self.elsewhere = self.tmp / 'somewhere odd' / 'out'
        self.elsewhere.mkdir(parents=True)
        self.exe = self.elsewhere / exe_name()
        self.exe.write_bytes(b'MZ from cargo')
        self.other = self.elsewhere / exe_name('other')
        self.other.write_bytes(b'MZ other')

    def env(self, **extra):
        return self.cargo.env(FAKE_CARGO_EXE=str(self.exe), FAKE_CARGO_OTHER=str(self.other), FAKE_CARGO_TARGET=str(self.tmp),
                              **extra)

    def test_the_executable_is_the_one_cargo_names_wherever_it_put_it(self):
        env = self.env()
        env.pop('CARGO_TARGET_DIR')  # a real environment would have none set here
        clean = {k: v for k, v in os.environ.items() if k != 'CARGO_TARGET_DIR'}
        done = subprocess.run([sys.executable, str(self.root / 'scripts/ship.py'), 'package'], capture_output=True,
                              text=True, env=dict(clean, **env), creationflags=NO_WINDOW)
        self.assertEqual(done.returncode, 0, done.stderr)
        result = json.loads(done.stdout)
        self.assertEqual(result['built_from'], str(self.exe))
        self.assertEqual((self.root / 'dist' / exe_name()).read_bytes(), b'MZ from cargo')
        self.assertIn('Compiling zephyr', done.stderr)  # the compiler's messages pass through to stderr
        self.assertEqual(len(done.stdout.strip().splitlines()), 1)  # stdout is exactly one JSON object

    def test_a_failed_build_stops_with_cargos_exit_code(self):
        code, result, _err = call_main(self.root, 'package', env=self.env(FAKE_CARGO_MODE='fail'))
        self.assertEqual(code, 1)
        self.assertIn('failed (exit 101)', result['error'])
        self.assertFalse((self.root / 'dist').exists())

    def test_no_build_asks_cargo_metadata_for_the_target_directory(self):
        release = self.tmp / 'release'
        release.mkdir()
        (release / exe_name()).write_bytes(b'MZ from metadata dir')
        env = self.env()
        env.pop('CARGO_TARGET_DIR')
        with mock.patch.dict(os.environ, env):
            os.environ.pop('CARGO_TARGET_DIR', None)
            code, result, _err = call_main(self.root, 'package', '--no-build')
        self.assertEqual(code, 0, result)
        self.assertEqual(result['built_from'], str(self.tmp / 'release' / exe_name()))

    def test_cargo_target_dir_beats_everything_else(self):
        chosen = self.tmp / 'chosen'
        (chosen / 'release').mkdir(parents=True)
        (chosen / 'release' / exe_name()).write_bytes(b'MZ chosen')
        code, result, _err = call_main(self.root, 'package', '--no-build', env=self.env(CARGO_TARGET_DIR=str(chosen)))
        self.assertEqual(code, 0, result)
        self.assertEqual((self.root / 'dist' / exe_name()).read_bytes(), b'MZ chosen')

    def test_relative_cargo_target_dir_is_relative_to_the_project(self):
        (self.root / 'out/release').mkdir(parents=True)
        (self.root / 'out/release' / exe_name()).write_bytes(b'MZ relative')
        code, result, _err = call_main(self.root, 'package', '--no-build', env={'CARGO_TARGET_DIR': 'out'})
        self.assertEqual(code, 0, result)
        self.assertEqual((self.root / 'dist' / exe_name()).read_bytes(), b'MZ relative')

    def test_without_cargo_the_error_says_so(self):
        empty = self.tmp / 'empty path'
        empty.mkdir()
        code, result, _err = call_main(self.root, 'package', env={'PATH': str(empty), 'CARGO_TARGET_DIR': ''})
        self.assertEqual(code, 1)
        self.assertIn('cargo was not found', result['error'])


# --------------------------------------------------------------------------------------------------
# shortcut: launcher files for every platform (generated into --folder, never into the OS)
# --------------------------------------------------------------------------------------------------


class LauncherFileTests(TempTestCase):
    def setUp(self):
        super().setUp()
        self.root = make_game(self.tmp / 'my game $100%', title="Zephyr's Quest & Co")
        self.folder = self.tmp / 'desktop'

    def dist(self, plat):
        return fake_dist(self.root, plat)

    def shortcut(self, plat, *extra, **kwargs):
        return call_main(self.root, 'shortcut', '--folder', str(self.folder), '--platform', plat, *extra, **kwargs)

    def test_linux_desktop_entry_has_every_field(self):
        project = self.dist('linux')
        code, result, _err = self.shortcut('linux')
        self.assertEqual(code, 0, result)
        path = self.folder / "Zephyr's Quest & Co.desktop"
        self.assertEqual(result['paths'], [str(path)])
        text = path.read_text(encoding='utf-8')
        entry = game_ship.parse_desktop_entry(text)
        self.assertEqual(entry['Type'], 'Application')
        self.assertEqual(entry['Name'], "Zephyr's Quest & Co")
        self.assertEqual(entry['Comment'], 'Catch the wind before it catches you. WASD move, Space jump, mouse look')
        self.assertEqual(game_ship.desktop_exec_program(entry['Exec']), str(project.dist / 'zephyr'))
        self.assertEqual(entry['Path'], str(project.dist))
        self.assertEqual(entry['Icon'], str(project.dist_png))
        self.assertEqual(entry['Terminal'], 'false')
        self.assertIn('Categories=Game;', text)
        self.assertTrue(text.startswith('[Desktop Entry]\n'))
        self.assertIn('Exec="', text)  # the program is always quoted
        if POSIX:
            self.assertEqual(path.stat().st_mode & 0o777, 0o755)

    def test_exec_quoting_survives_awkward_paths(self):
        for name in ['plain', 'with space', 'dollar $HOME', 'percent 100%', 'back`tick', 'quote"inside', 'back\\slash']:
            with self.subTest(name=name):
                path = '/games/' + name + '/game'
                value = game_ship._desktop_exec(path)
                self.assertNotIn('\n', value)
                # what a desktop environment does: undo the string escapes, then the Exec quoting
                exec_line = game_ship.parse_desktop_entry('[Desktop Entry]\nExec=' + value + '\n')['Exec']
                self.assertEqual(game_ship.desktop_exec_program(exec_line), path)

    def test_desktop_entry_parsing_undoes_string_escapes(self):
        entry = game_ship.parse_desktop_entry('[Other]\nName=nope\n[Desktop Entry]\nName=a\\\\b\nComment=x\\sy\n# c\nIcon=/i.png\n')
        self.assertEqual(entry, {'Name': 'a\\b', 'Comment': 'x y', 'Icon': '/i.png'})

    def test_linux_running_twice_changes_nothing(self):
        self.dist('linux')
        self.assertEqual(self.shortcut('linux')[1]['action'], 'created')
        path = self.folder / "Zephyr's Quest & Co.desktop"
        first = path.read_bytes()
        code, result, _err = self.shortcut('linux')
        self.assertEqual((code, result['action']), (0, 'refreshed'))
        self.assertEqual(path.read_bytes(), first)
        self.assertEqual(sorted(p.name for p in self.folder.iterdir()), [path.name])

    def test_linux_refuses_to_replace_another_programs_launcher(self):
        self.dist('linux')
        self.folder.mkdir()
        other = self.folder / "Zephyr's Quest & Co.desktop"
        other.write_text('[Desktop Entry]\nType=Application\nName=Other\nExec=/usr/bin/other\n', encoding='utf-8')
        code, result, _err = self.shortcut('linux')
        self.assertEqual(code, 1)
        self.assertIn('different program', result['error'])
        self.assertIn('--force', result['error'])
        self.assertIn('/usr/bin/other', other.read_text(encoding='utf-8'))
        code, result, _err = self.shortcut('linux', '--force')
        self.assertEqual(code, 0, result)
        self.assertIn('zephyr', other.read_text(encoding='utf-8'))

    def test_linux_applications_menu_copy_only_without_folder(self):
        self.dist('linux')
        desktop, data = self.tmp / 'home desktop', self.tmp / 'xdg data'
        desktop.mkdir()
        project = game_ship.Project(self.root, 'linux')
        with mock.patch.object(game_ship, 'host_platform', return_value='linux'), \
                mock.patch.object(game_ship, 'desktop_folders', return_value={'user': desktop, 'common': None}), \
                mock.patch.dict(os.environ, {'XDG_DATA_HOME': str(data)}):
            result = game_ship.cmd_shortcut(project)
        self.assertEqual(len(result['paths']), 2)
        self.assertTrue((desktop / "Zephyr's Quest & Co.desktop").is_file())
        menu = data / 'applications' / 'zephyr-s-quest-co.desktop'
        self.assertTrue(menu.is_file())
        self.assertEqual(menu.read_text(encoding='utf-8'), (desktop / "Zephyr's Quest & Co.desktop").read_text(encoding='utf-8'))
        # with --folder (tests, CI) nothing outside that folder is written
        other_data = self.tmp / 'xdg data 2'
        with mock.patch.object(game_ship, 'host_platform', return_value='linux'), \
                mock.patch.dict(os.environ, {'XDG_DATA_HOME': str(other_data)}):
            game_ship.cmd_shortcut(project, folder=str(self.folder))
        self.assertFalse(other_data.exists())

    def test_linux_needs_the_packaged_png_on_a_real_linux_host(self):
        self.dist('linux')
        (self.root / 'dist/zephyr.png').unlink()
        project = game_ship.Project(self.root, 'linux')
        with mock.patch.object(game_ship, 'host_platform', return_value='linux'):
            with self.assertRaises(game_ship.ShipError) as caught:
                game_ship.cmd_shortcut(project, folder=str(self.folder))
        self.assertIn('zephyr.png', str(caught.exception))

    def test_macos_command_launcher(self):
        project = self.dist('macos')
        code, result, _err = self.shortcut('macos')
        self.assertEqual(code, 0, result)
        path = self.folder / "Zephyr's Quest & Co.command"
        lines = path.read_text(encoding='utf-8').splitlines()
        self.assertEqual(lines[0], '#!/bin/bash')
        self.assertIn('cd ', lines[1])
        self.assertIn(str(project.dist) if ' ' not in str(project.dist) else "'" + str(project.dist) + "'", lines[1])
        self.assertEqual(lines[2], 'exec ./zephyr "$@"')
        if POSIX:
            self.assertEqual(path.stat().st_mode & 0o777, 0o755)

    def test_macos_refuses_foreign_launchers_and_refreshes_its_own(self):
        self.dist('macos')
        self.folder.mkdir()
        path = self.folder / "Zephyr's Quest & Co.command"
        path.write_text('#!/bin/bash\nexec /Applications/Other.app\n', encoding='utf-8')
        code, result, _err = self.shortcut('macos')
        self.assertEqual(code, 1)
        self.assertIn('different program', result['error'])
        self.assertEqual(self.shortcut('macos', '--force')[0], 0)
        code, result, _err = self.shortcut('macos')
        self.assertEqual((code, result['action']), (0, 'refreshed'))

    def test_macos_quotes_awkward_names(self):
        root = make_game(self.tmp / 'odd game', title='Odd Exe', exe='odd game exe')
        project = fake_dist(root, 'macos')
        code, result, _err = call_main(root, 'shortcut', '--folder', str(self.folder), '--platform', 'macos')
        self.assertEqual(code, 0, result)
        text = (self.folder / 'Odd Exe.command').read_text(encoding='utf-8')
        self.assertIn("exec ./'odd game exe'", text)
        self.assertIn(str(project.dist), text)

    def test_tooltip_is_capped_at_255_characters(self):
        root = make_game(self.tmp / 'chatty', title='Chatty', identity={'tagline': 't' * 200, 'controls': 'c' * 100})
        fake_dist(root, 'linux')
        code, result, _err = call_main(root, 'shortcut', '--folder', str(self.folder), '--platform', 'linux')
        self.assertEqual(code, 0, result)
        comment = game_ship.parse_desktop_entry((self.folder / 'Chatty.desktop').read_text(encoding='utf-8'))['Comment']
        self.assertEqual(len(comment), 255)
        self.assertTrue(comment.endswith('...'))

    def test_another_platform_without_folder_is_refused(self):
        other = 'macos' if sys.platform != 'darwin' else 'linux'
        code, result, _err = call_main(self.root, 'shortcut', '--platform', other)
        self.assertEqual(code, 2)
        self.assertIn('--folder', result['error'])

    def test_no_desktop_is_an_explicit_skip_not_a_failure(self):
        self.dist(game_ship.host_platform() if game_ship.host_platform() != 'other' else 'linux')
        with mock.patch.object(game_ship, 'desktop_folders', return_value={'user': None, 'common': None}):
            code, result, err = call_main(self.root, 'shortcut')
        self.assertEqual(code, 0)
        self.assertEqual(result['skipped'], 'no desktop')
        self.assertTrue(result['ok'])

    def test_unpackaged_game_says_to_package_first(self):
        code, result, _err = call_main(self.root, 'shortcut', '--folder', str(self.folder))
        if game_ship.host_platform() == 'other':
            self.skipTest('no launcher format for this platform')
        self.assertEqual(code, 1)
        self.assertIn('ship.py package', result['error'])

    def test_invalid_identity_is_a_config_error(self):
        self.dist('linux')
        make_game(self.root, title='Play')
        code, result, _err = self.shortcut('linux')
        self.assertEqual(code, 2)
        self.assertIn('placeholder', result['error'])
        self.assertFalse(self.folder.exists() and any(self.folder.iterdir()))

    def test_stamp_records_the_launcher(self):
        project = self.dist('linux')
        self.shortcut('linux')
        stamp = project.read_stamp()
        self.assertEqual(stamp['shortcut']['path'], str(self.folder / "Zephyr's Quest & Co.desktop"))
        self.assertRegex(stamp['shortcut']['created_at'], r'^\d{4}-')


# --------------------------------------------------------------------------------------------------
# verify: everything that does not need the Windows shell (Linux launcher, static checks, gates)
# --------------------------------------------------------------------------------------------------


def write_desktop(path, **fields):
    """Write a .desktop file (string escapes applied, as a real one would have them)."""
    lines = ['[Desktop Entry]'] + [f'{key}=' + str(value).replace(chr(92), chr(92) * 2) for key, value in fields.items()]
    Path(path).write_text(chr(10).join(lines) + chr(10), encoding='utf-8')


def make_engine(base, logo_seed=None):
    """A stand-in engine checkout (only its branding icon matters to verify)."""
    engine = Path(base) / 'engine'
    (engine / 'assets/branding').mkdir(parents=True, exist_ok=True)
    (engine / 'Cargo.toml').write_text('[package]\nname = "be2"\n', encoding='utf-8')
    if logo_seed is not None:
        (engine / 'assets/branding/blueengine.ico').write_bytes(icon_set(logo_seed)['icon.ico'])
    return engine


class VerifyBase(TempTestCase):
    """A finished Linux-style game: valid assets, a fake packaged dist and its launcher in ./desktop."""

    plat = 'linux'

    def setUp(self):
        super().setUp()
        self.root = make_game(self.tmp / 'game', exe='zephyr')
        self.folder = self.tmp / 'desktop'
        self.project = fake_dist(self.root, self.plat)
        code, result, _err = call_main(self.root, 'shortcut', '--folder', str(self.folder), '--platform', self.plat)
        self.assertEqual(code, 0, result)

    def verify(self, *extra, plat=None):
        code, report, err = call_main(self.root, 'verify', '--folder', str(self.folder), '--platform', plat or self.plat,
                                      '--json', '--icon-similarity', *extra)
        return code, report

    def statuses(self, report):
        return {c['name']: c['status'] for c in report['checks']}

    def assert_fails(self, name, fragment=None, *extra, plat=None):
        code, report = self.verify(*extra, plat=plat)
        self.assertEqual(code, 1, report)
        self.assertFalse(report['ok'])
        check = check_by_name(report, name)
        self.assertEqual(check['status'], 'fail', check)
        if fragment:
            self.assertIn(fragment, check['detail'])
        return report

    def assert_advisory(self, name, fragment):
        code, report = self.verify()
        self.assertEqual(code, 0, report)
        self.assertTrue(report['ok'])
        check = check_by_name(report, name)
        self.assertEqual(check['status'], 'warn', check)
        self.assertIn(fragment, check['detail'])
        return report

    def write_asset(self, name, data):
        (self.root / 'assets' / name).write_bytes(data)


class MaintainedProjectTests(unittest.TestCase):
    def test_leo_shipping_configuration_loads_without_build_or_desktop(self):
        root = Path(__file__).resolve().parents[1] / 'assets/games/leo'
        with mock.patch.object(game_ship.subprocess, 'run', side_effect=AssertionError('process')), \
             mock.patch.object(game_ship, 'desktop_folders', side_effect=AssertionError('desktop')):
            project = game_ship.load_project(root, 'windows')
            self.assertEqual(project.exe_stem(), 'leo')
        self.assertTrue((root / 'scripts/project.py').is_file())


class PackageDeliveryTests(VerifyBase):
    def test_package_verification_never_reads_desktop(self):
        with mock.patch.object(game_ship, 'desktop_folders', side_effect=AssertionError('desktop access')), \
             mock.patch.object(game_ship.Verifier, 'other_shortcuts', side_effect=AssertionError('scan')):
            code, report, _ = call_main(self.root, 'verify', '--json', '--platform', self.plat)
        self.assertEqual(code, 0, report)
        self.assertEqual(self.statuses(report)['shortcut-file'], 'skip')
        self.assertEqual(self.statuses(report)['shortcut-unique'], 'skip')

    def test_installation_verifies_own_shortcut_without_comparing_other_icons(self):
        with mock.patch.object(game_ship.Verifier, 'other_shortcuts', side_effect=AssertionError('scan')):
            code, report, _ = call_main(self.root, 'verify', '--json', '--platform', self.plat,
                                      '--folder', str(self.folder))
        self.assertEqual(code, 0, report)
        self.assertEqual(self.statuses(report)['shortcut-file'], 'pass')
        self.assertEqual(self.statuses(report)['shortcut-icon'], 'pass')
        self.assertEqual(self.statuses(report)['shortcut-unique'], 'skip')

    def test_failed_advisory_is_a_warning_not_a_package_failure(self):
        with mock.patch.object(game_ship.Verifier, 'check_shortcut_unique', side_effect=game_ship.ShipError('shell unavailable')):
            code, report, _ = call_main(self.root, 'verify', '--json', '--platform', self.plat,
                                      '--icon-similarity')
        self.assertEqual(code, 0, report)
        self.assertEqual(self.statuses(report)['shortcut-unique'], 'warn')

    def test_no_install_ship_does_not_create_a_shortcut(self):
        with mock.patch.object(game_ship, 'cmd_package', return_value={'ok': True}), \
             mock.patch.object(game_ship, 'cmd_shortcut', side_effect=AssertionError('install')), \
             mock.patch.object(game_ship, 'desktop_folders', side_effect=AssertionError('desktop access')):
            report = game_ship.cmd_ship(self.project, no_install=True, no_smoke=True)
        self.assertTrue(report['ok'], report)
        self.assertEqual(report['shortcut']['skipped'], '--no-install')


class VerifyPassTests(VerifyBase):
    def test_a_finished_game_passes_and_says_what_it_skipped(self):
        code, report = self.verify()
        self.assertEqual(code, 0, report)
        self.assertTrue(report['ok'])
        self.assertEqual(report['next'], '')
        statuses = self.statuses(report)
        self.assertEqual([c['name'] for c in report['checks']], [
            'identity', 'icon-files', 'icon-art', 'wiring', 'package', 'exe-resources', 'shortcut-file', 'shortcut-icon',
            'shortcut-unique', 'launch', 'smoke'])
        for name in ['identity', 'icon-files', 'icon-art', 'wiring', 'package', 'shortcut-file', 'shortcut-icon',
                     'shortcut-unique']:
            self.assertEqual(statuses[name], 'pass', check_by_name(report, name))
        self.assertEqual({statuses[n] for n in ('exe-resources', 'launch', 'smoke')}, {'skip'})
        self.assertTrue(any(s.startswith('exe-resources: ') for s in report['skipped']))
        self.assertIn('launch: not requested (pass --launch)', report['skipped'])
        self.assertIn('smoke: not requested (pass --smoke)', report['skipped'])

    def test_output_is_one_json_object_and_the_summary_goes_to_stderr(self):
        base = ['verify', '--folder', str(self.folder), '--platform', 'linux']
        done = subprocess.run([sys.executable, str(self.root / 'scripts/ship.py'), *base], capture_output=True, text=True,
                              creationflags=NO_WINDOW)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(len(done.stdout.strip().splitlines()), 1)
        self.assertTrue(json.loads(done.stdout)['ok'])
        self.assertIn('[pass] identity', done.stderr)
        self.assertIn('verify: ok', done.stderr)
        quiet = subprocess.run([sys.executable, str(self.root / 'scripts/ship.py'), *base, '--json'], capture_output=True,
                               text=True, creationflags=NO_WINDOW)
        self.assertEqual(quiet.stderr.strip(), '')
        self.assertEqual(json.loads(quiet.stdout)['ok'], True)

    def test_exit_codes(self):
        run = lambda *a: subprocess.run([sys.executable, str(self.root / 'scripts/ship.py'), *a], capture_output=True,  # noqa: E731
                                        text=True, creationflags=NO_WINDOW)
        good = run('verify', '--folder', str(self.folder), '--platform', 'linux', '--json')
        self.assertEqual(good.returncode, 0)
        (self.root / 'assets/icon.png').unlink()
        bad = run('verify', '--folder', str(self.folder), '--platform', 'linux', '--json')
        self.assertEqual(bad.returncode, 1)
        self.assertFalse(json.loads(bad.stdout)['ok'])
        self.assertEqual(run('verify', '--bogus').returncode, 2)  # usage error
        outside = self.tmp / 'not a game'
        (outside / 'scripts').mkdir(parents=True)
        shutil.copy(ROOT / 'templates/game_ship.py', outside / 'scripts/ship.py')
        config = subprocess.run([sys.executable, str(outside / 'scripts/ship.py'), 'verify', '--json'],
                                capture_output=True, text=True, creationflags=NO_WINDOW)
        self.assertEqual(config.returncode, 2)
        self.assertFalse(json.loads(config.stdout)['ok'])
        self.assertIn('Cargo.toml', json.loads(config.stdout)['error'])

    def test_verify_records_evidence_in_the_stamp_and_keeps_the_last_launch_result(self):
        self.verify()
        stamp = self.project.read_stamp()
        self.assertEqual((stamp['verified']['launch'], stamp['verified']['smoke']), (None, None))
        stamp['verified'] = {'at': 'old', 'launch': True, 'launch_at': 'earlier', 'smoke': False, 'smoke_at': 'earlier'}
        self.project.write_stamp(stamp)
        self.verify()
        verified = self.project.read_stamp()['verified']
        self.assertEqual((verified['launch'], verified['smoke']), (True, False))
        self.assertEqual(verified['launch_at'], 'earlier')
        self.assertNotEqual(verified['at'], 'old')

    def test_skip_package_only_relaxes_a_missing_dist(self):
        shutil.rmtree(self.root / 'dist')
        self.assert_fails('package', 'is missing')
        code, report = self.verify('--skip-package')
        statuses = self.statuses(report)
        self.assertEqual(statuses['package'], 'skip')
        self.assertIn('--skip-package', check_by_name(report, 'package')['detail'])
        self.assertEqual(statuses['shortcut-file'], 'skip')
        self.assertEqual(code, 0, report)

    def test_no_desktop_is_skipped_explicitly_and_listed(self):
        with mock.patch.object(game_ship, 'desktop_folders', return_value={'user': None, 'common': None}):
            code, report, _err = call_main(self.root, 'verify', '--platform', 'linux', '--json', '--check-shortcut', '--icon-similarity')
        self.assertEqual(code, 0, report)
        for name in ('shortcut-file', 'shortcut-icon', 'shortcut-unique'):
            check = check_by_name(report, name)
            self.assertEqual((check['status'], check['detail']), ('skip', 'no desktop'))
            self.assertIn(f'{name}: no desktop', report['skipped'])

    def test_dynamic_checks_need_a_display_and_say_so(self):
        with mock.patch.object(game_ship, 'has_display', return_value=False):
            code, report = self.verify('--launch', '--smoke')
        self.assertEqual(code, 0, report)
        self.assertEqual(check_by_name(report, 'launch')['detail'], 'no display')
        self.assertEqual(check_by_name(report, 'smoke')['detail'], 'no display')


class VerifyIdentityTests(VerifyBase):
    def test_placeholder_title_fails_and_blocks_the_dependent_checks(self):
        data = json.loads((self.root / 'assets/identity.json').read_text(encoding='utf-8'))
        data['title'] = 'Game'
        (self.root / 'assets/identity.json').write_text(json.dumps(data), encoding='utf-8')
        report = self.assert_fails('identity', 'placeholder')
        statuses = self.statuses(report)
        self.assertEqual(statuses['package'], 'skip')
        self.assertEqual(statuses['shortcut-file'], 'skip')
        self.assertIn('identity.json', report['next'])
        self.assertIsNone(report['title'])

    def test_missing_and_broken_identity_files(self):
        (self.root / 'assets/identity.json').write_text('{ not json', encoding='utf-8')
        self.assert_fails('identity', 'not valid JSON')
        (self.root / 'assets/identity.json').unlink()
        self.assert_fails('identity', 'missing')

    def test_every_problem_is_listed_at_once(self):
        (self.root / 'assets/identity.json').write_text(json.dumps({'title': 'Play', 'tagline': '', 'controls': ''}), encoding='utf-8')
        detail = check_by_name(self.verify()[1], 'identity')['detail']
        for word in ('placeholder', 'tagline', 'controls'):
            self.assertIn(word, detail)


class VerifyIconTests(VerifyBase):
    def frames(self, seed=1, sizes=game_ship.ICO_SIZES):
        small = {size: draw_icon(seed, size) for size in (16, 20, 24, 32, 40, 48, 64)}
        out = []
        for size in sizes:
            out.append((size, small[size] if size in small else upscale(small[64], 64, size)))
        return out

    def test_missing_and_unparseable_icons(self):
        (self.root / 'assets/icon.ico').write_bytes(b'nope')
        self.assert_fails('icon-files', 'does not parse')
        self.assertEqual(self.statuses(self.verify()[1])['icon-art'], 'skip')
        (self.root / 'assets/icon.ico').unlink()
        self.assert_fails('icon-files', 'missing')

    def test_missing_sizes_are_named(self):
        self.write_asset('icon.ico', build_ico(self.frames(sizes=(16, 32, 48, 64, 128, 256))))
        detail = check_by_name(self.verify()[1], 'icon-files')['detail']
        self.assertIn('lacks the sizes 20, 24, 40, 96', detail)

    def test_frame_encodings_are_checked(self):
        frames = self.frames()
        blobs = dict((size, encode_png(size, size, rgba, filters=(1,)) if size >= 64 else bmp_frame(size, rgba))
                     for size, rgba in frames)
        blobs[96] = bmp_frame(96, dict(frames)[96])  # a big frame must be a PNG
        blobs[32] = encode_png(32, 32, dict(frames)[32])  # a small frame must be a BMP
        out = bytearray(struct.pack('<HHH', 0, 1, len(blobs)))
        offset = 6 + 16 * len(blobs)
        for size, blob in blobs.items():
            out += struct.pack('<BBBBHHII', size % 256, size % 256, 0, 0, 1, 32, len(blob), offset)
            offset += len(blob)
        for blob in blobs.values():
            out += blob
        self.write_asset('icon.ico', bytes(out))
        detail = check_by_name(self.verify()[1], 'icon-files')['detail']
        self.assertIn('96x96 frame must be a PNG frame', detail)
        self.assertIn('32x32 frame must be a 32-bit BMP frame', detail)

    def test_window_icon_blobs_must_have_the_exact_sizes(self):
        self.write_asset('icon_32.rgba', b'\x00' * 4000)
        self.assert_fails('icon-files', 'icon_32.rgba is 4000 bytes, expected 4096')
        (self.root / 'assets/icon_64.rgba').unlink()
        self.assert_fails('icon-files', 'icon_64.rgba is missing')

    def test_icon_png_must_be_a_png(self):
        self.write_asset('icon.png', b'GIF89a not a png')
        self.assert_fails('icon-files', 'icon.png is not a PNG')
        (self.root / 'assets/icon.png').unlink()
        self.assert_fails('icon-files', 'icon.png is missing')

    def flat_frames(self, colour=(60, 60, 200, 255)):
        return [(size, bytes(colour) * (size * size)) for size in game_ship.ICO_SIZES]

    def test_one_flat_colour_is_not_art(self):
        self.write_asset('icon.ico', build_ico(self.flat_frames()))
        report = self.assert_fails('icon-art')
        detail = check_by_name(report, 'icon-art')['detail']
        self.assertIn('flat colour', detail)
        self.assertIn('covers 100%', detail)  # and an opaque square has no transparent corners

    def test_too_few_colours(self):
        frames = []
        for size in game_ship.ICO_SIZES:
            rgba = bytearray(size * size * 4)
            for y in range(size):
                for x in range(size):
                    if (x - size / 2) ** 2 + (y - size / 2) ** 2 < (size * 0.45) ** 2:
                        inner = size // 4 <= x < 3 * size // 4 and size // 4 <= y < size // 2
                        rgba[(y * size + x) * 4:(y * size + x) * 4 + 4] = bytes((10, 10, 10, 255) if inner else (200, 30, 30, 255))
            frames.append((size, bytes(rgba)))
        self.write_asset('icon.ico', build_ico(frames))
        self.assert_fails('icon-art', 'colours (need 12)')

    def test_barely_any_art_is_not_enough_coverage(self):
        frames = []
        for size in game_ship.ICO_SIZES:
            small = draw_icon(1, 16)
            rgba = bytearray(upscale(small, 16, size))
            for i in range(0, len(rgba), 4):
                if (i // 4) % size > size // 3 or (i // 4) // size > size // 3:
                    rgba[i + 3] = 0
            frames.append((size, bytes(rgba)))
        self.write_asset('icon.ico', build_ico(frames))
        self.assert_fails('icon-art', 'of the square')

    def test_window_icon_blobs_must_be_the_same_art_as_the_ico(self):
        self.write_asset('icon_32.rgba', icon_set(2)['icon_32.rgba'])
        self.assert_fails('icon-art', 'icon_32.rgba is not the same art as the 32 px frame')

    def test_miniquads_default_logo_is_not_an_icon(self):
        logo = game_ship.base64_logo()
        frames = [(size, upscale(logo, 32, size) if size != 32 else logo) for size in game_ship.ICO_SIZES]
        self.write_asset('icon.ico', build_ico(frames))
        for name, size in game_ship.RGBA_BLOBS.items():
            self.write_asset(name, upscale(logo, 32, size) if size != 32 else logo)
        self.assert_fails('icon-art', "miniquad's default window icon")

    def test_the_blueengine_logo_is_refused(self):
        engine = make_engine(self.tmp, logo_seed=7)
        self.root.joinpath('Cargo.toml').write_text(
            self.root.joinpath('Cargo.toml').read_text(encoding='utf-8')
            + f'\n[dependencies]\nvesper3d = {{ package = "be2", path = "{engine.as_posix()}" }}\n', encoding='utf-8')
        # unrelated art passes and says so
        detail = check_by_name(self.verify()[1], 'icon-art')['detail']
        self.assertIn('not the engine logo', detail)
        # a byte-identical copy fails
        for name, content in icon_set(7).items():
            self.write_asset(name, content)
        self.assert_fails('icon-art', 'byte-identical to the BlueEngine logo')
        # the same picture re-encoded fails on its looks
        frames = [(f.width, game_ship.decode_ico_frame(icon_set(7)['icon.ico'], f)[2]) for f in
                  game_ship.parse_ico(icon_set(7)['icon.ico'])]
        recoded = build_ico(frames, png_filters=(0,))
        self.assertNotEqual(recoded, icon_set(7)['icon.ico'])
        self.write_asset('icon.ico', recoded)
        self.assert_fails('icon-art', 'looks like the BlueEngine logo')
        # a hue-shifted copy still looks like it
        shifted = icon_set(7, shift=10)
        for name, content in shifted.items():
            self.write_asset(name, content)
        self.assert_fails('icon-art', 'looks like the BlueEngine logo')

    def test_without_an_engine_checkout_the_logo_comparison_is_skipped_visibly(self):
        self.assertIn('engine logo comparison skipped', check_by_name(self.verify()[1], 'icon-art')['detail'])


class VerifyWiringTests(VerifyBase):
    def test_build_rs_must_exist_and_mention_the_icon(self):
        (self.root / 'build.rs').write_text('fn main() {}\n', encoding='utf-8')
        self.assert_fails('wiring', 'build.rs does not mention assets/icon.ico')
        (self.root / 'build.rs').unlink()
        self.assert_fails('wiring', 'build.rs is missing')

    def test_build_rs_may_join_the_path_in_pieces(self):
        (self.root / 'build.rs').write_text('fn main() { let p = m.join("assets").join("icon.ico"); }\n', encoding='utf-8')
        self.assertEqual(self.statuses(self.verify()[1])['wiring'], 'pass')

    def test_the_window_icon_must_be_set_in_the_sources(self):
        (self.root / 'src/main.rs').write_text('fn main() { let _title = "Zephyr Quest"; }\n', encoding='utf-8')
        self.assert_fails('wiring', 'sets the window icon')
        (self.root / 'src/main.rs').write_text(
            'fn main() { let _t = "Zephyr Quest"; Icon::icon_from_rgba(&[]); }\n', encoding='utf-8')
        self.assertEqual(self.statuses(self.verify()[1])['wiring'], 'pass')

    def test_the_title_must_appear_in_the_sources_or_come_from_identity_json(self):
        (self.root / 'src/main.rs').write_text('fn main() { let _icon = include_bytes!("../assets/icon_64.rgba"); }\n',
                                               encoding='utf-8')
        self.assert_fails('wiring', 'window title "Zephyr Quest" does not appear')
        (self.root / 'src/title.rs').write_text(
            'const IDENTITY: &str = include_str!("../assets/identity.json");\n', encoding='utf-8')
        self.assertEqual(self.statuses(self.verify()[1])['wiring'], 'pass')

    def test_sources_in_subfolders_count(self):
        (self.root / 'src/gfx').mkdir()
        (self.root / 'src/main.rs').write_text('fn main() {}\n', encoding='utf-8')
        (self.root / 'src/gfx/window.rs').write_text(
            'fn conf() { include_bytes!("../../assets/icon_64.rgba"); "Zephyr Quest"; }\n', encoding='utf-8')
        self.assertEqual(self.statuses(self.verify()[1])['wiring'], 'pass')

    def test_dist_must_be_ignored_by_git(self):
        (self.root / '.gitignore').write_text('/target/\n', encoding='utf-8')
        self.assert_fails('wiring', '.gitignore has no /dist/ line')
        for line in ('/dist/', '/dist', 'dist/'):
            (self.root / '.gitignore').write_text(f'/target/\n{line}\n', encoding='utf-8')
            self.assertEqual(self.statuses(self.verify()[1])['wiring'], 'pass', line)

    def test_missing_gitignore_is_only_a_warning(self):
        (self.root / '.gitignore').unlink()
        code, report = self.verify()
        self.assertEqual(code, 0, report)
        self.assertEqual(check_by_name(report, 'wiring')['status'], 'warn')
        self.assertIn('.gitignore is missing', check_by_name(report, 'wiring')['detail'])


class VerifyPackageTests(VerifyBase):
    def test_missing_dist_is_a_failure_with_the_command_to_run(self):
        shutil.rmtree(self.root / 'dist')
        report = self.assert_fails('package', 'ship.py package')
        self.assertIn('ship.py package', report['next'])

    def test_the_packaged_icon_must_be_the_asset_icon(self):
        (self.root / 'dist/zephyr.ico').write_bytes(icon_set(4)['icon.ico'])
        self.assert_fails('package', 'differs from assets/icon.ico')

    def test_missing_packaged_ico_and_extras(self):
        (self.root / 'dist/zephyr.ico').unlink()
        self.assert_fails('package', 'zephyr.ico is missing')
        (self.root / 'dist/zephyr.ico').write_bytes((self.root / 'assets/icon.ico').read_bytes())
        data = json.loads((self.root / 'assets/identity.json').read_text(encoding='utf-8'))
        data['package'] = ['maps']
        (self.root / 'assets/identity.json').write_text(json.dumps(data), encoding='utf-8')
        self.assert_fails('package', 'package entry maps is missing from dist/')

    def test_a_replaced_exe_no_longer_matches_the_stamp(self):
        (self.root / 'dist/zephyr').write_bytes(b'someone swapped me')
        self.assert_fails('package', 'does not match dist/ship.json')

    def test_a_missing_stamp_fails(self):
        (self.root / 'dist/ship.json').unlink()
        self.assert_fails('package', 'ship.json is missing')

    def test_a_stale_package_is_only_a_warning(self):
        build = self.root / 'target/release'
        build.mkdir(parents=True)
        newer = build / 'zephyr'
        newer.write_bytes(b'a newer build')
        os.utime(newer, (time_now() + 100, time_now() + 100))
        code, report = self.verify()
        self.assertEqual(code, 0, report)
        check = check_by_name(report, 'package')
        self.assertEqual(check['status'], 'warn')
        self.assertIn('older than the release build', check['detail'])
        self.assertEqual(report['ok'], True)

    def test_files_the_game_wrote_beside_the_exe_are_not_a_problem(self):
        dist = self.root / 'dist'
        (dist / 'saves').mkdir()
        (dist / 'saves/quick.be2save').write_bytes(b'save')
        (dist / 'player_save.json').write_text('{}', encoding='utf-8')
        (dist / 'settings.json').write_text('{}', encoding='utf-8')
        (dist / 'crash.log').write_text('log', encoding='utf-8')
        code, report = self.verify()
        self.assertEqual(code, 0, report)
        self.assertEqual(check_by_name(report, 'package')['status'], 'pass')
        self.assertEqual(report['ok'], True)

    def test_missing_png_in_dist_is_a_failure(self):
        (self.root / 'dist/zephyr.png').unlink()
        code, report = self.verify()
        self.assertEqual(code, 1)
        self.assertEqual(check_by_name(report, 'package')['status'], 'fail')


class PackageIntegrityTests(VerifyBase):
    def setUp(self):
        super().setUp()
        (self.root / 'maps/sub').mkdir(parents=True)
        (self.root / 'maps/sub/level.json').write_text('{}')
        identity = json.loads((self.root / 'assets/identity.json').read_text())
        identity['package'] = ['maps']
        (self.root / 'assets/identity.json').write_text(json.dumps(identity))
        shutil.copytree(self.root / 'maps', self.project.dist / 'maps')
        self.refresh_stamp()

    def refresh_stamp(self):
        stamp = self.project.read_stamp()
        files = sorted(p.relative_to(self.project.dist).as_posix() for p in self.project.dist.rglob('*')
                       if p.is_file() and p != self.project.stamp_path)
        stamp.update(files=files, file_sha256={n: game_ship.sha256_file(self.project.dist / n) for n in files},
                     exe_sha256=game_ship.sha256_file(self.project.dist_exe))
        self.project.write_stamp(stamp)

    def test_missing_nested_file_fails_without_display_even_when_source_exists(self):
        (self.project.dist / 'maps/sub/level.json').unlink()
        with mock.patch.dict(os.environ, {'DISPLAY': '', 'WAYLAND_DISPLAY': ''}):
            self.assert_fails('package', 'maps/sub/level.json is missing', '--smoke')
        self.assertTrue((self.root / 'maps/sub/level.json').is_file())

    def test_modified_nested_file_fails_without_display(self):
        (self.project.dist / 'maps/sub/level.json').write_text('{"changed": true}')
        with mock.patch.object(game_ship, 'has_display', return_value=False):
            self.assert_fails('package', 'maps/sub/level.json does not match', '--smoke')

    def test_stock_runtime_dependency_must_be_declared_even_when_checkout_has_it(self):
        (self.root / 'maps/sub/checkout-only.json').write_text('{}')
        # Leave a stale runtime dependency in dist, then deliberately omit it from the manifest.
        (self.project.dist / 'maps/sub/checkout-only.json').write_text('{}')
        level = self.project.dist / 'maps/sub/level.json'
        for reference, fragment in [('checkout-only.json', 'undeclared map'),
                                    ('../../../outside.json', 'unsafe package file'),
                                    ('C:/outside.json', 'unsafe package file')]:
            level.write_text(json.dumps({'schema_version': 1, 'player_profile': {}, 'map': reference}))
            self.refresh_stamp()
            stamp = self.project.read_stamp()
            stamp['files'].remove('maps/sub/checkout-only.json')
            del stamp['file_sha256']['maps/sub/checkout-only.json']
            self.project.write_stamp(stamp)
            # Remove source inventory so this test isolates the runtime-reference check. The source
            # level still exists; no bytes may be read from it to fulfill the reference.
            (self.root / 'maps/sub/checkout-only.json').unlink(missing_ok=True)
            with self.subTest(reference=reference), mock.patch.object(game_ship, 'has_display', return_value=False):
                self.assert_fails('package', fragment, '--smoke')

    def test_omitting_file_from_both_manifest_and_dist_cannot_use_source(self):
        (self.project.dist / 'maps/sub/level.json').unlink()
        self.refresh_stamp()
        self.assert_fails('package', 'maps')

    def test_unsafe_manifest_paths_and_invalid_hashes_are_actionable(self):
        original = self.project.read_stamp()
        for name in ('../secret', '/secret', 'C:/secret', 'maps\\secret', './secret', 'maps//secret', 'NUL.txt'):
            stamp = dict(original, files=original['files'] + [name],
                         file_sha256={**original['file_sha256'], name: '0' * 64})
            self.project.write_stamp(stamp)
            with self.subTest(name=name):
                self.assert_fails('package', 'unsafe package file')
        stamp = dict(original, file_sha256={**original['file_sha256'], 'maps/sub/level.json': 'not a hash'})
        self.project.write_stamp(stamp)
        self.assert_fails('package', 'invalid SHA-256')

    def test_missing_hash_and_duplicate_manifest_entries_fail(self):
        stamp = self.project.read_stamp()
        stamp['file_sha256'].pop('maps/sub/level.json')
        self.project.write_stamp(stamp)
        self.assert_fails('package', 'exactly every packaged file')
        stamp['files'].append('MAPS/sub/level.json')
        self.project.write_stamp(stamp)
        self.assert_fails('package', 'duplicate')

    @unittest.skipUnless(POSIX, 'symlink permissions differ on Windows')
    def test_symlink_cannot_satisfy_a_packaged_asset_from_the_checkout(self):
        level = self.project.dist / 'maps/sub/level.json'
        level.unlink()
        level.symlink_to(self.root / 'maps/sub/level.json')
        self.assert_fails('package', 'symlinks')

    @unittest.skipUnless(POSIX, 'runs a POSIX executable fixture')
    def test_isolated_smoke_rejects_an_undeclared_dependency_left_in_dist(self):
        exe = self.project.dist_exe
        exe.write_text('#!/usr/bin/env python3\n'
                       'from pathlib import Path\nimport sys\n'
                       'Path("hidden.txt").read_text()\n'
                       'out = Path(sys.argv[sys.argv.index("--capture") + 1])\n'
                       'out.mkdir()\n(out / "world.png").write_bytes(Path("zephyr.png").read_bytes())\n')
        exe.chmod(0o755)
        self.refresh_stamp()
        (self.project.dist / 'hidden.txt').write_text('left over from development')
        (self.root / 'hidden.txt').write_text('also in the checkout')
        with mock.patch.object(game_ship, 'has_display', return_value=True):
            check = check_by_name(self.verify('--smoke')[1], 'smoke')
            self.assertEqual(check['status'], 'fail', check)
            self.assertIn('hidden.txt', check['detail'])
        # Declaring it then makes the SAME executable work outside both source and dist.
        self.refresh_stamp()
        with mock.patch.object(game_ship, 'has_display', return_value=True):
            check = check_by_name(self.verify('--smoke')[1], 'smoke')
            self.assertEqual(check['status'], 'pass', check)
            self.assertIn('isolated declared-file package', check['detail'])


class VerifyLinuxLauncherTests(VerifyBase):
    def entry_path(self):
        return self.folder / 'Zephyr Quest.desktop'

    def rewrite(self, **changes):
        entry = game_ship.parse_desktop_entry(self.entry_path().read_text(encoding='utf-8'))
        entry.update(changes)
        write_desktop(self.entry_path(), **entry)

    def test_missing_launcher(self):
        self.entry_path().unlink()
        self.assert_fails('shortcut-file', 'does not exist')
        self.assertEqual(self.statuses(self.verify()[1])['shortcut-icon'], 'skip')
        self.assertEqual(self.statuses(self.verify()[1])['shortcut-unique'], 'skip')

    def test_each_field_is_checked(self):
        project = self.project
        cases = {
            'Exec': ('"/somewhere/else/game"', 'Exec starts'),
            'Path': ('/tmp', 'Path is'),
            'Icon': ('/tmp/other.png', 'Icon is'),
            'Terminal': ('true', 'Terminal is not false'),
            'Comment': ('Something else entirely', 'Comment does not contain the tagline'),
            'Name': ('Wrong Name', 'Name is'),
            'Type': ('Link', 'Type is not Application'),
        }
        original = self.entry_path().read_text(encoding='utf-8')
        for key, (value, fragment) in cases.items():
            with self.subTest(key=key):
                self.entry_path().write_text(original, encoding='utf-8')
                self.rewrite(**{key: value})
                report = self.assert_fails('shortcut-file', fragment)
        self.entry_path().write_text(original, encoding='utf-8')
        self.assertEqual(self.verify()[0], 0)
        del project

    def test_icon_must_be_the_game_art(self):
        other = self.tmp / 'other.png'
        other.write_bytes(icon_set(4)['icon.png'])
        self.rewrite(Icon=str(other))
        self.assert_fails('shortcut-file', 'Icon is')  # not the packaged png
        self.rewrite(Icon=str(self.project.dist_png))
        self.project.dist_png.write_bytes(icon_set(4)['icon.png'])  # the packaged png itself shows other art
        self.assert_fails('shortcut-icon', 'not the game icon')

    def test_uniqueness_against_other_launchers_with_icon_files(self):
        other_icon = self.tmp / 'other.png'
        other_icon.write_bytes(icon_set(4)['icon.png'])
        write_desktop(self.folder / 'Another Game.desktop', Type='Application', Name='Another Game', Exec='/bin/true',
                      Icon=other_icon)
        detail = check_by_name(self.verify()[1], 'shortcut-unique')['detail']
        self.assertIn('distinct from 1 other launcher', detail)
        # the same picture under another name
        same = self.tmp / 'same.png'
        same.write_bytes(icon_set(1)['icon.png'])
        write_desktop(self.folder / 'Another Game.desktop', Type='Application', Name='Another Game', Exec='/bin/true',
                      Icon=same)
        self.assert_advisory('shortcut-unique', 'too similar to "Another Game"')

    def test_a_near_duplicate_icon_is_too_similar(self):
        near = self.tmp / 'near.png'
        near.write_bytes(icon_set(1, shift=10)['icon.png'])
        write_desktop(self.folder / 'Lookalike.desktop', Type='Application', Name='Lookalike', Exec='/bin/true', Icon=near)
        report = self.assert_advisory('shortcut-unique', 'too similar to "Lookalike"')
        self.assertIn('--variant N --replace', check_by_name(report, 'shortcut-unique')['detail'])
        self.assertEqual(report['next'], '')

    def test_another_launcher_with_our_name_is_a_clash(self):
        sub = self.folder / 'games'
        sub.mkdir()
        (sub / 'Copy.desktop').write_text('[Desktop Entry]\nType=Application\nName=Zephyr Quest\nExec=/bin/true\n', encoding='utf-8')
        self.assert_advisory('shortcut-unique', 'also called "Zephyr Quest"')

    def test_launchers_without_usable_icons_say_nothing_about_looks(self):
        (self.folder / 'Themed.desktop').write_text(
            '[Desktop Entry]\nType=Application\nName=Themed\nExec=/bin/true\nIcon=applications-games\n', encoding='utf-8')
        (self.folder / 'Broken.desktop').write_text('[Desktop Entry]\nIcon=/does/not/exist.png\n', encoding='utf-8')
        self.assertIn('no other launchers with icon files', check_by_name(self.verify()[1], 'shortcut-unique')['detail'])


class VerifyMacLauncherTests(VerifyBase):
    plat = 'macos'

    def test_a_finished_mac_game_passes_with_visible_skips(self):
        code, report = self.verify()
        self.assertEqual(code, 0, report)
        statuses = self.statuses(report)
        self.assertEqual(statuses['shortcut-file'], 'pass')
        self.assertEqual(statuses['shortcut-icon'], 'skip')
        self.assertEqual(statuses['shortcut-unique'], 'skip')
        self.assertIn('no icon', check_by_name(report, 'shortcut-icon')['detail'])

    def test_launcher_must_start_the_packaged_exe_from_dist(self):
        path = self.folder / 'Zephyr Quest.command'
        path.write_text('#!/bin/bash\nexec /Applications/Other.app\n', encoding='utf-8')
        self.assert_fails('shortcut-file', 'does not cd into')
        path.write_text(f'#!/bin/bash\ncd "{self.project.dist}"\nexec ./nope\n', encoding='utf-8')
        self.assert_fails('shortcut-file', 'does not exec ./zephyr')
        path.write_text(f'cd "{self.project.dist}"\nexec ./zephyr\n', encoding='utf-8')
        self.assert_fails('shortcut-file', '#!/bin/bash')


def time_now():
    import time
    return time.time()


# --------------------------------------------------------------------------------------------------
# Windows: the real shell (WScript.Shell, IShellItemImageFactory) and real executables
# --------------------------------------------------------------------------------------------------

GAME_CS = r'''
using System;
using System.Drawing;
using System.IO;
using System.Reflection;
using System.Text;
using System.Threading;
using System.Windows.Forms;

[assembly: AssemblyTitle("@@TITLE@@")]
[assembly: AssemblyProduct("@@TITLE@@")]
[assembly: AssemblyVersion("1.2.3.0")]

// A stand-in game: `--capture DIR [--mode ok|flat|none|nodir|crash|hang]` behaves like a game's capture
// run (it refuses an existing folder, like the stock runner); without arguments it opens a window whose
// title and icon come from window.txt beside the exe (line 1 title, line 2 icon file, line 3 "nowindow" or "stubborn").
static class ShipTestGame
{
    const string DetailedPng = "@@PNG@@";
    const string SmallPng = "@@SMALL@@";
    const string FlatPng = "@@FLAT@@";

    [STAThread]
    static int Main(string[] args)
    {
        string capture = null, mode = "window";
        for (int i = 0; i + 1 < args.Length; i += 2)
        {
            if (args[i] == "--capture") capture = args[i + 1];
            if (args[i] == "--mode") mode = args[i + 1];
        }
        if (capture == null && mode == "window") return Window();
        return Capture(capture, mode);
    }

    static int Capture(string dir, string mode)
    {
        if (mode == "hang") { Thread.Sleep(600000); return 0; }
        if (mode == "crash") { Console.Error.WriteLine("boom"); return 3; }
        if (dir == null || mode == "nodir") return 0;
        if (Directory.Exists(dir)) { Console.Error.WriteLine("capture directory already exists"); return 9; }
        Directory.CreateDirectory(dir);
        if (mode == "none") return 0;
        if (mode == "flat") { File.WriteAllBytes(Path.Combine(dir, "flat.png"), Convert.FromBase64String(FlatPng)); return 0; }
        File.WriteAllBytes(Path.Combine(dir, "world.png"), Convert.FromBase64String(DetailedPng));
        File.WriteAllBytes(Path.Combine(dir, "menu.png"), Convert.FromBase64String(SmallPng));
        return 0;
    }

    static int Window()
    {
        string home = AppDomain.CurrentDomain.BaseDirectory;
        string[] lines = File.ReadAllLines(Path.Combine(home, "window.txt"), Encoding.UTF8);
        string title = lines.Length > 0 ? lines[0] : "untitled";
        string icon = lines.Length > 1 ? lines[1] : "";
        string behaviour = lines.Length > 2 ? lines[2] : "";
        if (behaviour == "nowindow") { Thread.Sleep(120000); return 0; }
        Form form = new Form();
        form.Text = title;
        if (behaviour == "stubborn") form.FormClosing += delegate(object sender, FormClosingEventArgs e) { e.Cancel = true; };
        if (icon.Length > 0) form.Icon = new Icon(Path.Combine(home, icon));
        System.Windows.Forms.Timer safety = new System.Windows.Forms.Timer();
        safety.Interval = 90000;
        safety.Tick += delegate { form.Close(); };
        safety.Start();
        Application.Run(form);
        return 0;
    }
}
'''


def compile_game(directory, title, ico, name):
    """Compile the stand-in game with the .NET Framework C# compiler; `ico` (bytes or None) becomes the exe icon."""
    import base64
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    png = lambda data: base64.b64encode(data).decode('ascii')  # noqa: E731
    detailed = encode_png(64, 64, draw_icon(3, 64))
    small = encode_png(32, 32, draw_icon(6, 32))
    flat = encode_png(16, 16, bytes((90, 140, 200, 255)) * 256)
    source = (GAME_CS.replace('@@TITLE@@', title).replace('@@PNG@@', png(detailed)).replace('@@SMALL@@', png(small))
              .replace('@@FLAT@@', png(flat)))
    (directory / f'{name}.cs').write_text(source, encoding='utf-8')
    command = [str(CSC), '/nologo', '/target:winexe', '/optimize', f'/out:{directory / (name + ".exe")}',
               '/reference:System.Windows.Forms.dll', '/reference:System.Drawing.dll']
    if ico is not None:
        (directory / f'{name}.ico').write_bytes(ico)
        command.append(f'/win32icon:{directory / (name + ".ico")}')
    command.append(str(directory / f'{name}.cs'))
    done = subprocess.run(command, capture_output=True, text=True, creationflags=NO_WINDOW)
    if done.returncode != 0:
        raise RuntimeError('csc failed: ' + done.stdout + done.stderr)
    return directory / f'{name}.exe'


def is_running(image_name):
    done = subprocess.run(['tasklist', '/FI', f'IMAGENAME eq {image_name}', '/NH'], capture_output=True, text=True,
                          creationflags=NO_WINDOW)
    return image_name.lower() in done.stdout.lower()


def windows_game(tmp, title='Zephyr Quest', seed=1, exe='zephyr', exe_bytes=b'MZ stand-in exe', **kwargs):
    """A game project whose release build (a stand-in file) sits in a redirected target folder."""
    root = make_game(Path(tmp) / 'game', title=title, seed=seed, exe=exe, **kwargs)
    release = Path(tmp) / 'redirected target' / 'release'
    release.mkdir(parents=True, exist_ok=True)
    (release / f'{exe}.exe').write_bytes(exe_bytes)
    return root, {'CARGO_TARGET_DIR': str(release.parent)}


def ship_files(root, env, folder):
    """package --no-build and shortcut --folder, asserting both worked."""
    for args in (['package', '--no-build'], ['shortcut', '--folder', str(folder)]):
        code, result, _err = call_main(root, *args, env=env)
        assert code == 0, result


@unittest.skipUnless(WINDOWS, 'exercises the Windows shell')
class WindowsShortcutTests(TempTestCase):
    def setUp(self):
        super().setUp()
        self.root, self.env = windows_game(self.tmp)
        self.folder = self.tmp / 'desktop'
        self.lnk = self.folder / 'Zephyr Quest.lnk'
        self.project = game_ship.Project(self.root)
        code, _result, _err = call_main(self.root, 'package', '--no-build', env=self.env)
        self.assertEqual(code, 0)

    def shortcut(self, *extra):
        return call_main(self.root, 'shortcut', '--folder', str(self.folder), *extra, env=self.env)

    def other_exe(self, name='other.exe', content=b'MZ some other program'):
        path = self.tmp / name
        path.write_bytes(content)
        return path

    def test_the_shortcut_has_the_fields_the_spec_asks_for(self):
        code, result, _err = self.shortcut()
        self.assertEqual((code, result['action'], result['kind']), (0, 'created', 'lnk'), result)
        info = game_ship.read_shortcut(self.lnk)
        dist = self.project.dist
        self.assertTrue(game_ship.same_file(info['target'], dist / 'zephyr.exe'))
        self.assertEqual(os.path.normcase(info['working_dir']), os.path.normcase(str(dist)))
        self.assertEqual(os.path.normcase(info['icon_location']), os.path.normcase(f'{dist / "zephyr.ico"},0'))
        self.assertEqual(info['description'], 'Catch the wind before it catches you. WASD move, Space jump, mouse look')
        self.assertEqual(info['window_style'], 1)
        self.assertEqual(info['arguments'], '')
        self.assertTrue(game_ship.is_lnk_file(self.lnk))
        self.assertEqual([p.name for p in self.folder.iterdir()], ['Zephyr Quest.lnk'])  # nothing else was touched
        self.assertNotIn(str(self.project.root / 'target'), info['target'])  # a package, never a build folder
        stamp = self.project.read_stamp()
        self.assertEqual(stamp['shortcut']['path'], str(self.lnk))

    def test_running_twice_gives_identical_results_and_no_duplicates(self):
        self.shortcut()
        first = game_ship.read_shortcut(self.lnk)
        code, result, _err = self.shortcut()
        self.assertEqual((code, result['action']), (0, 'refreshed'))
        self.assertEqual(game_ship.read_shortcut(self.lnk), first)
        self.assertEqual([p.name for p in self.folder.iterdir()], ['Zephyr Quest.lnk'])

    def test_a_same_named_shortcut_of_another_program_is_refused_unless_forced(self):
        other = self.other_exe()
        self.folder.mkdir()
        game_ship.write_shortcut(self.lnk, other, self.tmp, f'{other},0', 'someone else', 1)
        code, result, err = self.shortcut()
        self.assertEqual(code, 1)
        self.assertIn('different program', result['error'])
        self.assertIn(str(other), result['error'])
        self.assertIn('--force', result['error'])
        self.assertTrue(game_ship.same_file(game_ship.read_shortcut(self.lnk)['target'], other))  # untouched
        code, result, _err = self.shortcut('--force')
        self.assertEqual((code, result['action']), (0, 'replaced'))
        self.assertTrue(game_ship.same_file(game_ship.read_shortcut(self.lnk)['target'], self.project.dist / 'zephyr.exe'))

    def test_a_dangling_shortcut_of_that_name_is_simply_refreshed(self):
        gone = self.other_exe('gone.exe')
        self.folder.mkdir()
        game_ship.write_shortcut(self.lnk, gone, self.tmp, f'{gone},0', 'old', 1)
        gone.unlink()
        code, result, _err = self.shortcut()
        self.assertEqual((code, result['action']), (0, 'refreshed'), result)

    def test_a_development_build_shortcut_of_this_game_is_refreshed_too(self):
        build = self.root / 'target' / 'release'
        build.mkdir(parents=True)
        (build / 'zephyr.exe').write_bytes(b'MZ dev build')
        self.folder.mkdir()
        game_ship.write_shortcut(self.lnk, build / 'zephyr.exe', build, f'{build / "zephyr.exe"},0', 'dev', 1)
        code, result, _err = self.shortcut()
        self.assertEqual((code, result['action']), (0, 'refreshed'), result)
        self.assertTrue(game_ship.same_file(game_ship.read_shortcut(self.lnk)['target'], self.project.dist / 'zephyr.exe'))

    def test_a_file_that_is_not_a_shortcut_is_protected(self):
        self.folder.mkdir()
        self.lnk.write_text('this is a note, not a shortcut', encoding='utf-8')
        code, result, _err = self.shortcut()
        self.assertEqual(code, 1)
        self.assertIn('not a shortcut', result['error'])
        self.assertEqual(self.lnk.read_text(encoding='utf-8'), 'this is a note, not a shortcut')
        code, result, _err = self.shortcut('--force')
        self.assertEqual((code, result['action']), (0, 'replaced'))
        self.assertTrue(game_ship.is_lnk_file(self.lnk))

    def test_unicode_titles_and_tooltips_survive_the_trip(self):
        # WScript.Shell squeezes paths through the ANSI code page and cannot save these; IShellLinkW can
        title = 'Caf\u00e9 \u00dcn\u00ef \u65e5\u672c \U0001F680'
        tagline = 'Ca va \u2014 \u201cquoted\u201d \u65e5\u672c\u8a9e'
        make_game(self.root, title=title, exe='zephyr', identity={'tagline': tagline, 'controls': 'ZQSD'})
        code, result, _err = self.shortcut()
        self.assertEqual(code, 0, result)
        lnk = self.folder / f'{title}.lnk'
        self.assertTrue(lnk.is_file())
        self.assertEqual(game_ship.read_shortcut(lnk)['description'], f'{tagline} ZQSD')
        self.assertEqual([p.name for p in self.folder.iterdir()], [lnk.name])

    def test_wscript_shell_reads_what_we_wrote(self):
        """The spec names WScript.Shell as the reader: what it sees must be what the tool reports."""
        self.shortcut()
        script = ("$l = (New-Object -ComObject WScript.Shell).CreateShortcut($env:SHIPTEST_LNK); "
                  "'{0}|{1}|{2}|{3}|{4}|{5}' -f $l.TargetPath, $l.WorkingDirectory, $l.IconLocation, $l.Description, "
                  "$l.WindowStyle, $l.Arguments")
        done = subprocess.run(['powershell.exe', '-NoProfile', '-NonInteractive', '-Command', script],
                              env=dict(os.environ, SHIPTEST_LNK=str(self.lnk)), capture_output=True, text=True,
                              creationflags=NO_WINDOW)
        info = game_ship.read_shortcut(self.lnk)
        expected = '|'.join([info['target'], info['working_dir'], info['icon_location'], info['description'],
                             str(info['window_style']), info['arguments']])
        self.assertEqual(done.stdout.strip(), expected, done.stderr)

    def test_the_os_desktop_matches_what_powershell_reports(self):
        answer = game_ship.run_windows_helper('desktop', {})
        user, common = game_ship.windows_desktops()
        self.assertEqual(os.path.normcase(user), os.path.normcase(answer['desktop']))
        self.assertEqual(os.path.normcase(common), os.path.normcase(answer['common']))
        self.assertEqual(game_ship.desktop_folders()['user'], Path(user))

    def test_the_helper_reports_failures_instead_of_hanging(self):
        with self.assertRaises(game_ship.ShipError) as caught:
            game_ship.run_windows_helper('no-such-action', {})
        self.assertIn('unknown action', str(caught.exception))

    def test_a_timed_out_helper_is_a_clear_error(self):
        with mock.patch.object(game_ship.subprocess, 'run', side_effect=subprocess.TimeoutExpired(['powershell'], 1)):
            with self.assertRaises(game_ship.ShipError) as caught:
                game_ship.run_windows_helper('desktop', {}, timeout=1)
        self.assertIn('timed out', str(caught.exception))

    def test_left_behind_game_processes_are_killed_by_pid_and_image_name(self):
        process = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(120)'], creationflags=NO_WINDOW)
        self.addCleanup(process.kill)
        state = self.tmp / 'state.json'
        state.write_text(json.dumps({'pids': [process.pid]}), encoding='utf-8')
        game_ship._kill_left_behind(state, 'some-other-name.exe')  # a mismatching image name protects the process
        self.assertIsNone(process.poll())
        game_ship._kill_left_behind(state, Path(sys.executable).name)
        process.wait(timeout=20)
        self.assertIsNotNone(process.returncode)

    def test_shell_render_survives_missing_and_odd_inputs(self):
        pictures = game_ship.shell_render([str(self.tmp / 'nope.lnk'), str(self.root / 'assets/icon.ico')], sizes=(32,))
        self.assertFalse(pictures[str(self.tmp / 'nope.lnk')]['ok'])
        self.assertTrue(pictures[str(self.root / 'assets/icon.ico')]['ok'])
        self.assertEqual(pictures[str(self.root / 'assets/icon.ico')]['images'][32][:2], (32, 32))
        self.assertEqual(game_ship.shell_render([]), {})


@unittest.skipUnless(WINDOWS, 'exercises the Windows shell')
class WindowsVerifyTests(TempTestCase):
    """verify against real shortcuts made in a temp folder; the exe is a stand-in file, so the
    exe-resources check is expected to fail here (CompiledGameTests covers the passing case)."""

    def setUp(self):
        super().setUp()
        self.root, self.env = windows_game(self.tmp)
        self.folder = self.tmp / 'desktop'
        self.project = game_ship.Project(self.root)
        ship_files(self.root, self.env, self.folder)
        self.lnk = self.folder / 'Zephyr Quest.lnk'

    def verify(self, *extra):
        code, report, _err = call_main(self.root, 'verify', '--folder', str(self.folder), '--json', '--icon-similarity', *extra, env=self.env)
        return code, report

    def statuses(self, report):
        return {c['name']: c['status'] for c in report['checks']}

    def make_other(self, name, ico_bytes, target_name=None, folder=None):
        """A shortcut of some other program whose icon is `ico_bytes`."""
        folder = Path(folder) if folder else self.folder
        folder.mkdir(parents=True, exist_ok=True)
        ico = self.tmp / f'{name}.ico'
        ico.write_bytes(ico_bytes)
        exe = self.tmp / (target_name or f'{name}.exe')
        exe.write_bytes(b'MZ another program')
        path = folder / f'{name}.lnk'
        game_ship.write_shortcut(path, exe, self.tmp, f'{ico},0', 'another program', 1)
        return path

    def test_a_real_shortcut_passes_the_shell_checks(self):
        code, report = self.verify()
        statuses = self.statuses(report)
        for name in ('identity', 'icon-files', 'icon-art', 'wiring', 'package', 'shortcut-file', 'shortcut-icon',
                     'shortcut-unique'):
            self.assertEqual(statuses[name], 'pass', check_by_name(report, name))
        self.assertIn('no other shortcuts', check_by_name(report, 'shortcut-unique')['detail'])
        self.assertIn('32 px shape 0.00', check_by_name(report, 'shortcut-icon')['detail'])
        self.assertEqual(statuses['exe-resources'], 'fail')  # the stand-in exe has no version info or icon
        self.assertEqual(code, 1)

    def test_exe_resources_name_what_is_missing(self):
        detail = check_by_name(self.verify()[1], 'exe-resources')['detail']
        self.assertIn('FileDescription is "", expected "Zephyr Quest"', detail)
        self.assertIn('ProductName', detail)
        self.assertIn('not the game', detail)

    def test_a_copy_of_a_real_exe_exercises_shortcut_icon_and_uniqueness(self):
        """The spec's end-to-end: the shortcut targets a copy of a real GUI-less exe (python.exe)."""
        release = Path(self.env['CARGO_TARGET_DIR']) / 'release'
        shutil.copy(sys.executable, release / 'zephyr.exe')
        self.assertEqual(call_main(self.root, 'package', '--no-build', env=self.env)[0], 0)
        self.make_other('Another Game', icon_set(4)['icon.ico'])
        game_ship.write_shortcut(self.lnk, self.project.dist / 'zephyr.exe', self.project.dist,
                                 f'{self.project.dist / "zephyr.ico"},0', self.project.identity.description(), 1)
        code, report = self.verify()
        statuses = self.statuses(report)
        self.assertEqual(statuses['shortcut-file'], 'pass', check_by_name(report, 'shortcut-file'))
        self.assertEqual(statuses['shortcut-icon'], 'pass')
        self.assertEqual(statuses['shortcut-unique'], 'pass')
        self.assertIn('nearest is "Another Game"', check_by_name(report, 'shortcut-unique')['detail'])
        self.assertEqual(statuses['exe-resources'], 'fail')  # python.exe carries Python's name and icon
        self.assertIn('Python', check_by_name(report, 'exe-resources')['detail'].replace('""', ''))
        self.assertEqual(code, 1)
        self.assertFalse(report['ok'])

    def test_no_shortcut_at_all(self):
        self.lnk.unlink()
        code, report = self.verify()
        self.assertIn('does not exist', check_by_name(report, 'shortcut-file')['detail'])
        self.assertEqual(self.statuses(report)['shortcut-file'], 'fail')
        self.assertEqual(self.statuses(report)['shortcut-icon'], 'skip')
        self.assertEqual(self.statuses(report)['shortcut-unique'], 'skip')

    def rewrite_shortcut(self, **changes):
        fields = {'target': self.project.dist / 'zephyr.exe', 'working_dir': self.project.dist,
                  'icon_location': f'{self.project.dist / "zephyr.ico"},0', 'description': self.project.identity.description()}
        fields.update(changes)
        game_ship.write_shortcut(self.lnk, fields['target'], fields['working_dir'], fields['icon_location'],
                                 fields['description'], 1)

    def check_lnk(self):
        """The shortcut-file check alone (a full verify also renders icons: much slower)."""
        verifier = game_ship.Verifier(self.project, self.folder)
        verifier.run_check('identity', verifier.check_identity)
        verifier.run_check('shortcut-file', verifier.check_shortcut_file)
        return check_by_name({'checks': verifier.checks}, 'shortcut-file')

    def test_every_field_of_the_shortcut_is_checked(self):
        build = self.root / 'target' / 'release'
        build.mkdir(parents=True)
        (build / 'zephyr.exe').write_bytes(b'MZ dev build')
        cases = {
            'a development build as the target': ({'target': build / 'zephyr.exe'}, 'a development build'),
            'another program as the target': ({'target': self.tmp / 'nothing.exe'}, 'target is'),
            'the wrong start-in folder': ({'working_dir': self.tmp}, 'start in is'),
            'the wrong icon file': ({'icon_location': f'{self.root / "assets/icon.ico"},0'}, 'icon is'),
            'the wrong icon index': ({'icon_location': f'{self.project.dist / "zephyr.ico"},3'}, 'icon is'),
            'a tooltip without the tagline': ({'description': 'just words'}, 'does not contain the tagline'),
        }
        for label, (changes, fragment) in cases.items():
            with self.subTest(label):
                self.rewrite_shortcut(**changes)
                check = self.check_lnk()
                self.assertEqual(check['status'], 'fail', check)
                self.assertIn(fragment, check['detail'])
        self.rewrite_shortcut()
        self.assertEqual(self.check_lnk()['status'], 'pass')

    def test_a_stale_tooltip_is_a_warning_not_a_failure(self):
        self.rewrite_shortcut(description='Catch the wind before it catches you. Old controls')
        check = check_by_name(self.verify()[1], 'shortcut-file')
        self.assertEqual(check['status'], 'warn')
        self.assertIn('refresh', check['detail'])

    def test_the_shell_icon_must_be_the_game_icon(self):
        other_ico = self.tmp / 'stranger.ico'
        other_ico.write_bytes(icon_set(4)['icon.ico'])
        self.rewrite_shortcut(icon_location=f'{other_ico},0')
        report = self.verify()[1]
        self.assertEqual(check_by_name(report, 'shortcut-icon')['status'], 'fail')
        self.assertIn('not the game icon at 32 px', check_by_name(report, 'shortcut-icon')['detail'])
        cmd = Path(os.environ.get('SystemRoot', r'C:\Windows')) / 'System32' / 'cmd.exe'
        self.rewrite_shortcut(icon_location=f'{cmd},0')  # some other program's icon
        self.assertEqual(check_by_name(self.verify()[1], 'shortcut-icon')['status'], 'fail')

    def test_a_packaged_ico_from_other_art_is_a_package_failure(self):
        (self.project.dist / 'zephyr.ico').write_bytes(icon_set(4)['icon.ico'])  # packaged from other art
        report = self.verify()[1]
        self.assertEqual(check_by_name(report, 'package')['status'], 'fail')
        self.assertIn('differs from assets/icon.ico', check_by_name(report, 'package')['detail'])

    def test_unique_against_different_art_and_names_the_nearest(self):
        self.make_other('Stranger', icon_set(4)['icon.ico'])
        self.make_other('Another Stranger', icon_set(6)['icon.ico'])
        detail = check_by_name(self.verify()[1], 'shortcut-unique')['detail']
        self.assertIn('distinct from 2 other shortcut(s)', detail)
        self.assertIn('nearest is', detail)

    def test_the_same_art_on_another_shortcut_is_a_clash(self):
        self.make_other('Lookalike', icon_set(1)['icon.ico'])
        report = self.verify()[1]
        check = check_by_name(report, 'shortcut-unique')
        self.assertEqual(check['status'], 'warn')
        self.assertIn('too similar to "Lookalike"', check['detail'])
        self.assertIn('shape 0.00', check['detail'])
        self.assertIn('--variant N --replace', check['detail'])

    def test_near_duplicates_of_the_art_are_clashes(self):
        for label, shift in {'hue +10': 10, 'hue -10': -10}.items():
            with self.subTest(label):
                path = self.make_other('Near Copy', icon_set(1, shift=shift)['icon.ico'])
                check = check_by_name(self.verify()[1], 'shortcut-unique')
                self.assertEqual(check['status'], 'warn', check)
                self.assertIn('too similar to "Near Copy"', check['detail'])
                path.unlink()

    def test_another_way_to_start_this_game_is_not_a_clash(self):
        exe = self.project.dist / 'zephyr.exe'
        ico = self.tmp / 'same.ico'
        ico.write_bytes(icon_set(1)['icon.ico'])
        game_ship.write_shortcut(self.folder / 'Zephyr Quest (again).lnk', exe, self.project.dist, f'{ico},0', 'x', 1)
        dev = self.root / 'target' / 'release'
        dev.mkdir(parents=True)
        (dev / 'zephyr.exe').write_bytes(b'MZ dev')
        game_ship.write_shortcut(self.folder / 'Dev Zephyr.lnk', dev / 'zephyr.exe', dev, f'{ico},0', 'x', 1)
        self.assertEqual(check_by_name(self.verify()[1], 'shortcut-unique')['status'], 'pass')

    def test_another_shortcut_with_our_title_is_a_clash(self):
        self.make_other('Zephyr Quest', icon_set(4)['icon.ico'], folder=self.folder / 'games')
        check = check_by_name(self.verify()[1], 'shortcut-unique')
        self.assertEqual(check['status'], 'warn')
        self.assertIn('also called "Zephyr Quest"', check['detail'])

    def test_the_scan_goes_two_folders_deep_no_further(self):
        deep = self.folder / 'a' / 'b' / 'c'
        self.make_other('Lookalike', icon_set(1)['icon.ico'], folder=deep)
        self.assertEqual(check_by_name(self.verify()[1], 'shortcut-unique')['status'], 'pass')
        (deep / 'Lookalike.lnk').replace(self.folder / 'a' / 'b' / 'Lookalike.lnk')
        self.assertEqual(check_by_name(self.verify()[1], 'shortcut-unique')['status'], 'warn')

    def test_broken_shortcuts_are_skipped_not_fatal(self):
        (self.folder / 'Corrupt.lnk').write_bytes(b'not a shortcut at all')
        dangling = self.tmp / 'gone.exe'
        dangling.write_bytes(b'MZ')
        game_ship.write_shortcut(self.folder / 'Dangling.lnk', dangling, self.tmp, ',0', 'target vanishes', 1)
        dangling.unlink()
        self.make_other('Stranger', icon_set(4)['icon.ico'])
        check = check_by_name(self.verify()[1], 'shortcut-unique')
        self.assertEqual(check['status'], 'pass', check)

    def test_scan_limits(self):
        for index in range(5):
            (self.folder / f'x{index}.lnk').write_bytes(b'x')
        verifier = game_ship.Verifier(self.project, self.folder)
        self.assertEqual(len(verifier.other_shortcuts()), 6)
        with mock.patch.object(game_ship, 'SCAN_MAX_FILES', 3):
            self.assertEqual(len(verifier.other_shortcuts()), 3)

    def test_folder_restricts_the_scan_to_that_folder(self):
        """With --folder the real desktop is never even read (without it both desktops are scanned)."""
        scanned = []
        real_render = game_ship.shell_render

        def spy(paths, sizes=(32,)):
            scanned.extend(paths)
            return real_render(paths, sizes)

        with mock.patch.object(game_ship, 'shell_render', side_effect=spy):
            self.verify()
        real_desktop = str(game_ship.desktop_folders()['user']).lower()
        self.assertFalse([p for p in scanned if p.lower().startswith(real_desktop)])

    def test_no_desktop_is_skipped_explicitly(self):
        with mock.patch.object(game_ship, 'desktop_folders', return_value={'user': None, 'common': None}):
            code, report, _err = call_main(self.root, 'verify', '--json', '--check-shortcut', '--icon-similarity', env=self.env)
        for name in ('shortcut-file', 'shortcut-icon', 'shortcut-unique'):
            self.assertEqual(check_by_name(report, name)['detail'], 'no desktop')
            self.assertIn(f'{name}: no desktop', report['skipped'])

    def test_ship_stops_when_the_package_cannot_be_made(self):
        (self.root / 'assets/icon.ico').unlink()
        code, result, _err = call_main(self.root, 'ship', '--folder', str(self.folder), '--no-build', env=self.env)
        self.assertEqual(code, 2)
        self.assertIn('icon.ico', result['error'])

    def test_ship_runs_package_shortcut_and_verify_with_visible_skips(self):
        shutil.rmtree(self.project.dist)
        self.lnk.unlink()
        code, result, _err = call_main(self.root, 'ship', '--folder', str(self.folder), '--no-build', '--no-launch',
                                       '--no-smoke', env=self.env)
        self.assertEqual(set(result), {'ok', 'command', 'package', 'shortcut', 'verify'})
        self.assertTrue(result['package']['ok'])
        self.assertEqual(result['shortcut']['action'], 'created')
        self.assertEqual(check_by_name(result['verify'], 'launch')['detail'], '--no-launch')
        self.assertEqual(check_by_name(result['verify'], 'smoke')['detail'], '--no-smoke')
        self.assertIn('launch: --no-launch', result['verify']['skipped'])
        self.assertFalse(result['ok'])  # the stand-in exe fails exe-resources, and ship reports it
        self.assertEqual(code, 1)


@unittest.skipUnless(WINDOWS and CSC.exists(), 'needs Windows and the .NET Framework C# compiler')
class CompiledGameTests(unittest.TestCase):
    """One real, compiled, GUI-subsystem exe that carries the game's icon and version info, shipped once
    to a temp folder; the tests verify it, break one thing at a time and put it back."""

    stem = f'shiptest{os.getpid()}'

    @classmethod
    def setUpClass(cls):
        cls.work = Path(tempfile.mkdtemp(prefix='compiled-', dir=tempfile.gettempdir()))
        ico = icon_set(1)['icon.ico']
        cls.exe_bytes = compile_game(cls.work / 'good', 'Zephyr Quest', ico, cls.stem).read_bytes()
        cls.plain_bytes = compile_game(cls.work / 'plain', 'Zephyr Quest', None, cls.stem).read_bytes()  # no icon
        cls.root, cls.env = windows_game(cls.work, exe=cls.stem, exe_bytes=cls.exe_bytes,
                                         identity={'smoke_args': ['--capture', '{dir}']})
        cls.folder = cls.work / 'desktop'
        cls.project = game_ship.Project(cls.root)
        ship_files(cls.root, cls.env, cls.folder)
        cls.lnk = cls.folder / 'Zephyr Quest.lnk'
        (cls.project.dist / 'window.txt').write_text(f'Zephyr Quest\n{cls.stem}.ico\n', encoding='utf-8')

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.work, ignore_errors=True)
        assert not is_running(f'{cls.stem}.exe'), 'a test game process was left behind'

    def verify(self, *extra):
        code, report, _err = call_main(self.root, 'verify', '--folder', str(self.folder), '--json', '--icon-similarity', *extra, env=self.env)
        return code, report

    def statuses(self, report):
        return {c['name']: c['status'] for c in report['checks']}

    @contextlib.contextmanager
    def changed(self, path, content):
        """Temporarily replace a file's content (bytes or text)."""
        path = Path(path)
        original = path.read_bytes() if path.exists() else None
        path.write_bytes(content if isinstance(content, bytes) else content.encode('utf-8'))
        try:
            yield
        finally:
            if original is None:
                path.unlink(missing_ok=True)
            else:
                path.write_bytes(original)

    def set_identity(self, **changes):
        path = self.root / 'assets/identity.json'
        data = json.loads(path.read_text(encoding='utf-8'))
        return self.changed(path, json.dumps(dict(data, **changes)))

    # ---- the whole chain, statically ----
    def test_a_finished_game_passes_every_check_that_does_not_open_it(self):
        code, report = self.verify()
        self.assertEqual(code, 0, report)
        statuses = self.statuses(report)
        for name in ('identity', 'icon-files', 'icon-art', 'wiring', 'package', 'exe-resources', 'shortcut-file',
                     'shortcut-icon', 'shortcut-unique'):
            self.assertEqual(statuses[name], 'pass', check_by_name(report, name))
        self.assertIn('FileDescription and ProductName are "Zephyr Quest"', check_by_name(report, 'exe-resources')['detail'])
        self.assertIn('exe icon matches', check_by_name(report, 'exe-resources')['detail'])
        self.assertEqual(statuses['launch'], 'skip')
        self.assertEqual(statuses['smoke'], 'skip')
        self.assertTrue(report['ok'])

    def test_advisory_shell_failure_cannot_fail_own_resource_verification(self):
        other = self.folder / 'Unrelated.lnk'
        render = game_ship.shell_render
        def fail_other(paths, sizes=(32,)):
            if str(other) in map(str, paths):
                raise game_ship.ShipError('unrelated shell icon unavailable')
            return render(paths, sizes)
        with mock.patch.object(game_ship.Verifier, 'other_shortcuts', return_value=[other]), \
             mock.patch.object(game_ship, 'shell_render', side_effect=fail_other):
            code, report = self.verify()
        self.assertEqual(code, 0, report)
        self.assertEqual(self.statuses(report)['exe-resources'], 'pass')
        self.assertEqual(self.statuses(report)['shortcut-icon'], 'pass')
        self.assertEqual(self.statuses(report)['shortcut-unique'], 'warn')

    def test_real_windows_resources_verify_without_desktop_access(self):
        with mock.patch.object(game_ship, 'desktop_folders', side_effect=AssertionError('desktop access')), \
             mock.patch.object(game_ship.Verifier, 'other_shortcuts', side_effect=AssertionError('scan')):
            code, report, _ = call_main(self.root, 'verify', '--json', env=self.env)
        self.assertEqual(code, 0, report)
        self.assertEqual(self.statuses(report)['exe-resources'], 'pass')
        self.assertEqual(self.statuses(report)['shortcut-file'], 'skip')

    def test_the_exe_must_carry_the_title(self):
        with self.set_identity(title='Another Name'):
            check = check_by_name(self.verify()[1], 'exe-resources')
        self.assertEqual(check['status'], 'fail')
        self.assertIn('FileDescription is "Zephyr Quest", expected "Another Name"', check['detail'])
        self.assertIn('ProductName is "Zephyr Quest", expected "Another Name"', check['detail'])

    def test_an_exe_without_the_icon_resource_is_caught(self):
        with self.changed(self.project.dist / f'{self.stem}.exe', self.plain_bytes):
            check = check_by_name(self.verify()[1], 'exe-resources')
        self.assertEqual(check['status'], 'fail')
        self.assertIn("exe's icon is not the game's art", check['detail'])
        self.assertNotIn('FileDescription is', check['detail'])  # its version info was right

    # ---- launching through the shortcut ----
    def can_open_windows(self):
        if not game_ship.has_display():
            self.skipTest('no display')

    def test_launch_through_the_shortcut_checks_title_and_window_icon(self):
        self.can_open_windows()
        code, report = self.verify('--launch')
        check = check_by_name(report, 'launch')
        self.assertEqual(check['status'], 'pass', check)
        self.assertIn('window "Zephyr Quest"', check['detail'])
        self.assertIn('big icon 32x32 is the game art', check['detail'])
        self.assertIn('closed gracefully', check['detail'])
        self.assertFalse(is_running(f'{self.stem}.exe'))
        verified = self.project.read_stamp()['verified']
        self.assertTrue(verified['launch'])
        self.assertRegex(verified['launch_at'], r'^\d{4}-')

    def test_a_wrong_window_title_fails_the_launch_check(self):
        self.can_open_windows()
        with self.changed(self.project.dist / 'window.txt', f'Some Other Title\n{self.stem}.ico\n'):
            check = check_by_name(self.verify('--launch')[1], 'launch')
        self.assertEqual(check['status'], 'fail')
        self.assertIn('the window title is "Some Other Title", expected "Zephyr Quest"', check['detail'])
        self.assertFalse(is_running(f'{self.stem}.exe'))

    def test_miniquads_default_icon_in_the_window_fails_the_launch_check(self):
        self.can_open_windows()
        logo = game_ship.base64_logo()
        default_ico = build_ico([(32, logo), (16, downscale2(logo, 32)), (64, upscale(logo, 32, 64))])
        (self.project.dist / 'default.ico').write_bytes(default_ico)
        self.addCleanup((self.project.dist / 'default.ico').unlink)
        with self.changed(self.project.dist / 'window.txt', 'Zephyr Quest\ndefault.ico\n'):
            report = self.verify('--launch')[1]
        check = check_by_name(report, 'launch')
        self.assertEqual(check['status'], 'fail')
        self.assertIn("miniquad's default logo", check['detail'])
        self.assertFalse(is_running(f'{self.stem}.exe'))
        self.assertFalse(self.project.read_stamp()['verified']['launch'])

    def test_a_window_with_some_other_icon_fails_the_launch_check(self):
        self.can_open_windows()
        (self.project.dist / 'stranger.ico').write_bytes(icon_set(4)['icon.ico'])
        self.addCleanup((self.project.dist / 'stranger.ico').unlink)
        with self.changed(self.project.dist / 'window.txt', 'Zephyr Quest\nstranger.ico\n'):
            check = check_by_name(self.verify('--launch')[1], 'launch')
        self.assertEqual(check['status'], 'fail')
        self.assertIn('is not the game art', check['detail'])

    def test_a_window_that_keeps_the_generic_icon_fails_the_launch_check(self):
        self.can_open_windows()  # a window nobody gave an icon shows the system's generic application icon
        with self.changed(self.project.dist / 'window.txt', 'Zephyr Quest\n'):
            check = check_by_name(self.verify('--launch')[1], 'launch')
        self.assertEqual(check['status'], 'fail', check)
        self.assertIn("big icon is not the game art", check['detail'])
        self.assertFalse(is_running(f'{self.stem}.exe'))

    def test_a_game_that_never_shows_a_window_is_killed_and_reported(self):
        self.can_open_windows()
        with self.changed(self.project.dist / 'window.txt', 'Zephyr Quest\n\nnowindow\n'), \
                mock.patch.object(game_ship, 'LAUNCH_TIMEOUT', 3):
            check = check_by_name(self.verify('--launch')[1], 'launch')
        self.assertEqual(check['status'], 'fail')
        self.assertIn('showed no titled window', check['detail'])
        self.assertFalse(is_running(f'{self.stem}.exe'))  # no process may be left behind

    def test_a_window_that_ignores_the_close_request_is_ended_after_three_seconds(self):
        self.can_open_windows()
        with self.changed(self.project.dist / 'window.txt', f'Zephyr Quest\n{self.stem}.ico\nstubborn\n'):
            check = check_by_name(self.verify('--launch')[1], 'launch')
        self.assertEqual(check['status'], 'pass', check)  # title and icon were right
        self.assertIn('did not close on WM_CLOSE within 3 s: ended by the checker', check['detail'])
        self.assertFalse(is_running(f'{self.stem}.exe'))  # ended, not left running

    def test_launch_is_skipped_visibly_when_the_shortcut_is_wrong(self):
        self.can_open_windows()
        other = self.work / 'elsewhere.exe'
        other.write_bytes(b'MZ')
        original = game_ship.read_shortcut(self.lnk)
        game_ship.write_shortcut(self.lnk, other, self.work, original['icon_location'], original['description'], 1)
        try:
            report = self.verify('--launch')[1]
        finally:
            game_ship.write_shortcut(self.lnk, original['target'], original['working_dir'], original['icon_location'],
                                     original['description'], 1)
        check = check_by_name(report, 'launch')
        self.assertEqual(check['status'], 'skip')
        self.assertIn('see shortcut-file', check['detail'])
        self.assertFalse(is_running(f'{self.stem}.exe'))

    # ---- the smoke run ----
    def smoke(self, mode=None, **extra):
        args = ['--capture', '{dir}'] + (['--mode', mode] if mode else [])
        with self.set_identity(smoke_args=args, **extra), mock.patch.object(game_ship, 'has_display', return_value=True):
            return check_by_name(self.verify('--smoke')[1], 'smoke')

    def test_smoke_collects_every_png_and_records_its_size(self):
        check = self.smoke()
        self.assertEqual(check['status'], 'pass', check)
        self.assertIn('2 PNG capture(s)', check['detail'])
        self.assertIn('world.png 64x64', check['detail'])
        self.assertIn('menu.png 32x32', check['detail'])
        verified = self.project.read_stamp()['verified']
        self.assertTrue(verified['smoke'])

    def test_smoke_passes_a_folder_that_does_not_exist_yet(self):
        # the stand-in refuses an existing folder (exit 9) exactly like the stock runner
        self.assertEqual(self.smoke()['status'], 'pass')
        # Failed launches can retain a smoke-*.log without creating a capture folder.
        # Count the folders this check promises to prune, preserving failure logs.
        folders = sorted(p for p in (self.root / '.blue-check').glob('smoke-*') if p.is_dir())
        self.assertTrue(folders)
        self.assertLessEqual(len(folders), 2)  # old evidence is pruned

    def test_smoke_fails_on_blank_frames(self):
        check = self.smoke('flat')
        self.assertEqual(check['status'], 'fail')
        self.assertIn('flat.png is blank', check['detail'])

    def test_smoke_fails_without_pngs_or_without_a_capture_folder(self):
        check = self.smoke('none')
        self.assertEqual(check['status'], 'fail')
        self.assertIn('contains no PNG captures', check['detail'])
        check = self.smoke('nodir')
        self.assertEqual(check['status'], 'fail')
        self.assertIn('exited without creating', check['detail'])

    def test_smoke_fails_when_the_game_crashes_or_hangs(self):
        check = self.smoke('crash')
        self.assertEqual(check['status'], 'fail')
        self.assertIn('exited with code 3', check['detail'])
        self.assertIn('boom', check['detail'])
        with mock.patch.object(game_ship, 'SMOKE_TIMEOUT', 3):
            check = self.smoke('hang')
        self.assertEqual(check['status'], 'fail')
        self.assertIn('did not finish within 3 s', check['detail'])
        self.assertFalse(is_running(f'{self.stem}.exe'))

    def test_the_default_smoke_arguments_are_capture_dir(self):
        with self.changed(self.root / 'assets/identity.json', json.dumps(
                {k: v for k, v in json.loads((self.root / 'assets/identity.json').read_text(encoding='utf-8')).items()
                 if k != 'smoke_args'})), mock.patch.object(game_ship, 'has_display', return_value=True):
            check = check_by_name(self.verify('--smoke')[1], 'smoke')
        self.assertEqual(check['status'], 'pass', check)

    # ---- everything at once, the way the game's scripts/blue ship runs it ----
    def test_ship_end_to_end_records_launch_and_smoke_in_the_stamp(self):
        self.can_open_windows()
        code, result, _err = call_main(self.root, 'ship', '--folder', str(self.folder), '--no-build', env=self.env)
        self.assertEqual(code, 0, result)
        self.assertTrue(result['ok'])
        self.assertEqual(result['shortcut']['action'], 'refreshed')
        statuses = self.statuses(result['verify'])
        self.assertEqual(statuses.pop('shortcut-unique'), 'skip')
        self.assertEqual(set(statuses.values()), {'pass'})
        stamp = self.project.read_stamp()
        self.assertEqual((stamp['verified']['launch'], stamp['verified']['smoke']), (True, True))
        self.assertEqual(stamp['exe'], f'{self.stem}.exe')
        self.assertFalse(is_running(f'{self.stem}.exe'))
        # the stamp has everything the spec lists
        for key in ('title', 'exe', 'exe_sha256', 'ico_sha256', 'packaged_at', 'engine_revision', 'shortcut', 'verified'):
            self.assertIn(key, stamp)


def downscale2(rgba, size):
    """Halve an RGBA image by keeping every second pixel (enough for a stand-in 16 px frame)."""
    half = size // 2
    out = bytearray(half * half * 4)
    for y in range(half):
        for x in range(half):
            out[(y * half + x) * 4:(y * half + x) * 4 + 4] = rgba[(2 * y * size + 2 * x) * 4:(2 * y * size + 2 * x) * 4 + 4]
    return bytes(out)


# --------------------------------------------------------------------------------------------------
# info, robustness of the command line, honest skip reasons
# --------------------------------------------------------------------------------------------------


class InfoTests(TempTestCase):
    def setUp(self):
        super().setUp()
        self.engine = make_engine(self.tmp)
        self.root = make_game(self.tmp / 'game', exe='zephyr',
                              cargo_extra=f'\n[dependencies]\nvesper3d = {{ package = "be2", path = "{self.engine.as_posix()}" }}\n')

    def test_info_json_reports_what_the_tool_resolved(self):
        code, info, _err = call_main(self.root, 'info', '--json')
        self.assertEqual(code, 0)
        self.assertEqual(info['identity']['title'], 'Zephyr Quest')
        self.assertIsNone(info['identity_error'])
        self.assertEqual(info['exe'], 'zephyr')
        self.assertEqual(info['exe_file'], exe_name())
        self.assertEqual(info['root'], str(self.root.resolve()))
        self.assertEqual(info['dist_exe']['exists'], False)
        self.assertTrue(info['icon']['exists'])
        self.assertEqual(info['engine']['path'], str(self.engine.resolve()))
        self.assertEqual(set(info['desktop']), {'user', 'common'})
        self.assertIsNone(info['stamp'])
        self.assertEqual(info['platform'], game_ship.host_platform())

    def test_info_sees_the_package_once_it_exists(self):
        release = self.tmp / 'out' / 'release'
        release.mkdir(parents=True)
        (release / exe_name()).write_bytes(b'MZ')
        self.assertEqual(call_main(self.root, 'package', '--no-build', env={'CARGO_TARGET_DIR': str(release.parent)})[0], 0)
        _code, info, _err = call_main(self.root, 'info', '--json')
        self.assertTrue(info['dist_exe']['exists'])
        self.assertEqual(info['dist_exe']['bytes'], 2)
        self.assertEqual(info['stamp']['title'], 'Zephyr Quest')

    def test_info_survives_an_invalid_identity_and_says_why(self):
        make_game(self.root, title='Play', exe='zephyr')
        code, info, _err = call_main(self.root, 'info', '--json')
        self.assertEqual(code, 0)
        self.assertIsNone(info['identity'])
        self.assertIn('placeholder', info['identity_error'])
        self.assertEqual(info['exe'], 'zephyr')

    def test_info_in_plain_text(self):
        done = subprocess.run([sys.executable, str(self.root / 'scripts/ship.py'), 'info'], capture_output=True, text=True,
                              encoding='utf-8', creationflags=NO_WINDOW)
        self.assertEqual(done.returncode, 0, done.stderr)
        for fragment in ('title     Zephyr Quest', 'tagline   Catch the wind', 'controls  WASD move', 'exe       zephyr',
                         'desktop   ', 'engine    ', 'icon      '):
            self.assertIn(fragment, done.stdout)

    def test_info_reports_a_missing_exe_name_instead_of_crashing(self):
        (self.root / 'Cargo.toml').write_text(
            '[package]\nname = "g"\n[[bin]]\nname = "one"\n[[bin]]\nname = "two"\n', encoding='utf-8')
        data = json.loads((self.root / 'assets/identity.json').read_text(encoding='utf-8'))
        del data['exe']
        (self.root / 'assets/identity.json').write_text(json.dumps(data), encoding='utf-8')
        code, info, _err = call_main(self.root, 'info', '--json')
        self.assertEqual(code, 0)
        self.assertIsNone(info['exe'])
        self.assertIn('several [[bin]]', info['exe_error'])


class RobustnessTests(TempTestCase):
    def test_pruning_capture_folders_preserves_logs_from_failed_launches(self):
        scratch = self.tmp / '.blue-check'; scratch.mkdir()
        for number in range(4):
            name = f'smoke-20261006T12000000000{number}'
            (scratch / name).mkdir()
            (scratch / (name + '.log')).write_text('completed capture')
        failed = scratch / 'smoke-20261006T120000000004.log'
        failed.write_text('failed before creating capture directory')
        game_ship.prune_smoke_folders(scratch, keep=1)
        self.assertEqual(len([p for p in scratch.glob('smoke-*') if p.is_dir()]), 1)
        self.assertEqual(len(list(scratch.glob('smoke-*.log'))), 2)
        self.assertEqual(failed.read_text(), 'failed before creating capture directory')

    def test_a_broken_check_does_not_stop_the_others(self):
        root = make_game(self.tmp / 'game')
        project = fake_dist(root, 'linux')
        verifier = game_ship.Verifier(project, self.tmp / 'desktop', 'linux')
        with mock.patch.object(verifier, 'check_wiring', side_effect=TypeError('boom')):
            report = verifier.run()
        wiring = check_by_name(report, 'wiring')
        self.assertEqual(wiring['status'], 'fail')
        self.assertIn('internal error, TypeError: boom', wiring['detail'])
        self.assertEqual([c['name'] for c in report['checks']][-1], 'smoke')  # every later check still ran
        self.assertFalse(report['ok'])

    def test_a_bug_in_the_tool_still_ends_in_one_json_object(self):
        root = make_game(self.tmp / 'game')
        with mock.patch.object(game_ship, 'cmd_info', side_effect=RuntimeError('unexpected')):
            code, result, err = call_main(root, 'info', '--json')
        self.assertEqual(code, 1)
        self.assertFalse(result['ok'])
        self.assertIn('internal error, RuntimeError: unexpected', result['error'])
        self.assertIn('Traceback', err)

    def test_a_command_line_mistake_is_exit_code_2_without_a_traceback(self):
        root = make_game(self.tmp / 'game')
        done = subprocess.run([sys.executable, str(root / 'scripts/ship.py'), 'shortcut', '--platform', 'beos'],
                              capture_output=True, text=True, creationflags=NO_WINDOW)
        self.assertEqual(done.returncode, 2)
        self.assertNotIn('Traceback', done.stderr)
        self.assertIn('usage', done.stderr)

    def test_help_lists_every_command(self):
        done = subprocess.run([sys.executable, str(ROOT / 'templates/game_ship.py'), '--help'], capture_output=True,
                              text=True, creationflags=NO_WINDOW)
        self.assertEqual(done.returncode, 0)
        for command in ('info', 'package', 'shortcut', 'verify', 'ship'):
            self.assertIn(command, done.stdout)

    def test_windows_only_checks_say_why_they_skip_on_other_platforms(self):
        root = make_game(self.tmp / 'game')
        project = fake_dist(root, 'linux')
        report = game_ship.Verifier(project, self.tmp / 'desktop', 'linux').run()
        self.assertEqual(check_by_name(report, 'exe-resources')['detail'], 'Windows executables only (platform is linux)')
        with mock.patch.object(game_ship, 'host_platform', return_value='linux'):
            windows = game_ship.Verifier(game_ship.Project(root, 'windows'), self.tmp / 'desktop', 'windows').run()
        self.assertEqual(check_by_name(windows, 'exe-resources')['detail'], 'reading exe resources needs a Windows host')
        self.assertEqual(check_by_name(windows, 'shortcut-file')['status'], 'fail')  # the .lnk is simply not there

    def test_desktop_folders_of_another_platform_are_never_read_from_the_os(self):
        other = 'linux' if game_ship.host_platform() != 'linux' else 'windows'
        self.assertEqual(game_ship.desktop_folders(other), {'user': None, 'common': None})

    def test_display_detection_answers_without_raising(self):
        self.assertIsInstance(game_ship.has_display(), bool)
        with mock.patch.object(game_ship, 'host_platform', return_value='linux'), \
                mock.patch.dict(os.environ, {'DISPLAY': '', 'WAYLAND_DISPLAY': ''}):
            self.assertFalse(game_ship.has_display())
        with mock.patch.object(game_ship, 'host_platform', return_value='linux'), \
                mock.patch.dict(os.environ, {'DISPLAY': ':0'}):
            self.assertTrue(game_ship.has_display())

    def test_linux_desktop_comes_from_the_user_dirs_file(self):
        home = self.tmp / 'home'
        (home / '.config').mkdir(parents=True)
        (home / 'Bureau').mkdir()
        (home / '.config' / 'user-dirs.dirs').write_text('XDG_DESKTOP_DIR="$HOME/Bureau"\n', encoding='utf-8')
        with mock.patch.object(game_ship.Path, 'home', return_value=home), \
                mock.patch.object(game_ship.shutil, 'which', return_value=None), \
                mock.patch.dict(os.environ, {'XDG_CONFIG_HOME': str(home / '.config')}):
            self.assertEqual(Path(game_ship._linux_desktop()), home / 'Bureau')
        (home / '.config' / 'user-dirs.dirs').write_text('XDG_DESKTOP_DIR="$HOME/"\n', encoding='utf-8')
        with mock.patch.object(game_ship.Path, 'home', return_value=home), \
                mock.patch.object(game_ship.shutil, 'which', return_value=None), \
                mock.patch.dict(os.environ, {'XDG_CONFIG_HOME': str(home / '.config')}):
            self.assertIsNone(game_ship._linux_desktop())  # "no desktop" is configured as the home folder itself


# --------------------------------------------------------------------------------------------------
# templates/game_build.rs: the build script that puts the icon and the title into the exe
# --------------------------------------------------------------------------------------------------

BUILD_RS = ROOT / 'templates' / 'game_build.rs'


def find_rc():
    if os.environ.get('RC') and Path(os.environ['RC']).is_file():
        return Path(os.environ['RC'])
    found = shutil.which('rc.exe')
    if found:
        return Path(found)
    for variable in ('ProgramFiles(x86)', 'ProgramFiles'):
        base = os.environ.get(variable)
        if base:
            hits = sorted(Path(base).glob('Windows Kits/10/bin/*/x64/rc.exe'))
            if hits:
                return hits[-1]
    return None


@unittest.skipUnless(shutil.which('rustc'), 'needs a Rust toolchain')
class BuildScriptUnitTests(TempTestCase):
    def test_the_templates_own_unit_tests_pass(self):
        """`rustc --test` on the template: the hand-written JSON string reader and the RC escaping."""
        binary = self.tmp / ('build_script_tests.exe' if WINDOWS else 'build_script_tests')
        built = subprocess.run(['rustc', '--edition', '2021', '--test', str(BUILD_RS), '-o', str(binary)],
                               capture_output=True, text=True, creationflags=NO_WINDOW)
        self.assertEqual(built.returncode, 0, built.stderr)
        self.assertNotIn('warning', built.stderr)  # the template is warning-free
        ran = subprocess.run([str(binary)], capture_output=True, text=True, creationflags=NO_WINDOW)
        self.assertEqual(ran.returncode, 0, ran.stdout + ran.stderr)
        self.assertIn('3 passed', ran.stdout)

    def test_the_template_names_what_the_spec_requires(self):
        text = BUILD_RS.read_text(encoding='utf-8')
        for needle in ('assets/icon.ico', 'assets/identity.json', 'CARGO_FEATURE_CLIENT', 'cargo:rustc-link-arg-bins',
                       'cargo:rerun-if-changed=build.rs', 'rerun-if-changed={IDENTITY}', 'rerun-if-changed={ICON}',
                       'Windows Kits', 'FileDescription', 'ProductName', 'InternalName', 'OriginalFilename', 'Comments',
                       'cargo:warning='):
            self.assertIn(needle, text)
        self.assertNotIn('unsafe', text)
        self.assertNotIn('TODO', text)


@unittest.skipUnless(WINDOWS and shutil.which('cargo') and find_rc(), 'needs Windows, cargo and the Windows SDK (rc.exe)')
class BuildScriptEmbedTests(TempTestCase):
    """Builds a scratch crate that uses the template unchanged and checks what Explorer would see."""

    def build(self, title, tagline, features=True):
        crate = self.tmp / 'crate'
        (crate / 'src').mkdir(parents=True)
        (crate / 'assets').mkdir()
        (crate / 'Cargo.toml').write_text(
            '[package]\nname = "embed_test"\nversion = "3.4.5"\nedition = "2021"\ndescription = "package description"\n\n'
            '[features]\ndefault = ["client"]\nclient = []\n\n[[bin]]\nname = "embed_test"\npath = "src/main.rs"\n',
            encoding='utf-8')
        (crate / 'src/main.rs').write_text('fn main() {}\n', encoding='utf-8')
        shutil.copy(ROOT / 'templates/game_build.rs', crate / 'build.rs')
        (crate / 'assets/icon.ico').write_bytes(icon_set(5)['icon.ico'])
        (crate / 'assets/identity.json').write_text(json.dumps(
            {'title': title, 'tagline': tagline, 'controls': 'WASD', 'exe': 'embed_test'}), encoding='utf-8')
        command = ['cargo', 'build', '--release', '--offline'] + ([] if features else ['--no-default-features'])
        env = dict(os.environ, CARGO_TARGET_DIR=str(self.tmp / 'cargo target'), CARGO_HOME=str(self.tmp / 'cargo home'),
                   CARGO_TERM_COLOR='never')
        done = subprocess.run(command, cwd=crate, env=env, capture_output=True, text=True, creationflags=NO_WINDOW)
        self.assertEqual(done.returncode, 0, done.stderr)
        return self.tmp / 'cargo target' / 'release' / 'embed_test.exe', done.stderr

    def test_title_tagline_version_and_icon_reach_the_exe(self):
        title = 'Caf\u00e9 "\u00dcn\u00ef" \u65e5\u672c \U0001F680'
        tagline = 'Say "hello" \\o/ 100% \u2014 done'
        exe, log = self.build(title, tagline)
        self.assertNotIn('warning', log)
        info = game_ship.exe_version_info(exe)
        self.assertEqual(info['file_description'], title)
        self.assertEqual(info['product_name'], title)
        self.assertEqual(info['comments'], tagline)
        self.assertEqual(info['internal_name'], 'embed_test')
        self.assertEqual(info['original_filename'], 'embed_test.exe')
        self.assertEqual((info['file_version'], info['product_version']), ('3.4.5', '3.4.5'))
        ico = self.tmp / 'crate/assets/icon.ico'
        pictures = game_ship.shell_render([str(exe), str(ico)], sizes=(32, 256))
        for size in (32, 256):
            shape, colour = game_ship.signature_distance(game_ship.make_signature(*pictures[str(exe)]['images'][size]),
                                                         game_ship.make_signature(*pictures[str(ico)]['images'][size]))
            self.assertLessEqual(max(shape, colour), game_ship.EQUAL_MAX, (size, shape, colour))

    def test_the_headless_build_carries_no_resources(self):
        exe, _log = self.build('Headless', 'no window here', features=False)
        info = game_ship.exe_version_info(exe)
        self.assertNotEqual(info['file_description'], 'Headless')  # the `client` feature is off: nothing embedded
        self.assertEqual(info['product_name'], '')


# @@MORE-TESTS@@


if __name__ == '__main__':
    unittest.main()
