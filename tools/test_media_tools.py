"""Tests for tools/contact_sheet.py and tools/audio_report.py (standard library only, about 10 s).

Run from the repository root:  python -m unittest tools.test_media_tools
Temporary files go to the system temp directory; set BE2_TEST_TMP to use another directory
(for example on a drive with more free space).
"""

import array
import contextlib
import importlib.util
import io
import json
import math
import os
import random
import runpy
import struct
import sys
import tempfile
import time
import unittest
import wave
import zlib
from pathlib import Path
from unittest import mock

TOOLS = Path(__file__).resolve().parent


def load_tool(name):
    spec = importlib.util.spec_from_file_location(name, TOOLS / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


cs = load_tool("contact_sheet")
ar = load_tool("audio_report")


def run_main(module, *args):
    """Run module.main in-process; returns (exit status, stdout, stderr)."""
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        try:
            code = module.main([str(a) for a in args])
        except SystemExit as exc:  # argparse usage errors
            code = exc.code
    return code, out.getvalue(), err.getvalue()


def run_script(name, *args):
    """Run tools/NAME as a script (its __main__ block) in-process; returns (status, stdout, stderr).

    This exercises the command line entry point without the cost and the unpredictable start-up
    stalls of launching a new interpreter.
    """
    out, err = io.StringIO(), io.StringIO()
    argv = [str(TOOLS / name)] + [str(a) for a in args]
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err), mock.patch.object(sys, "argv", argv):
        try:
            runpy.run_path(argv[0], run_name="__main__")
            code = 0
        except SystemExit as exc:
            code = exc.code
    return code, out.getvalue(), err.getvalue()


def make_tempdir():
    """A temporary directory (in BE2_TEST_TMP when set); files still locked at cleanup are ignored."""
    return tempfile.TemporaryDirectory(
        prefix="be2-media-test-", dir=os.environ.get("BE2_TEST_TMP") or None, ignore_cleanup_errors=True
    )


_MODULE_TMP = None


def setUpModule():
    global _MODULE_TMP
    _MODULE_TMP = make_tempdir()  # one temporary root for the whole module: fewer file system round trips


def tearDownModule():
    _MODULE_TMP.cleanup()


def scratch(name):
    """A new empty directory (as a str path) inside the module's temporary root."""
    path = os.path.join(_MODULE_TMP.name, name)
    os.mkdir(path)
    return path


class TempDirCase(unittest.TestCase):
    """Every test gets its own empty directory (self.dir, made on first use); class fixtures live in cls.root."""

    @classmethod
    def setUpClass(cls):
        cls.root = Path(scratch(cls.__name__))

    def setUp(self):
        self._dir = None

    @property
    def dir(self):
        if self._dir is None:
            self._dir = self.root / self._testMethodName
            self._dir.mkdir()
        return self._dir


# ======================================================================================
# An independent PNG encoder for the tests (every colour type, depth, filter and Adam7)
# ======================================================================================

ADAM7 = [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)]
CHANNELS = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}


def png_chunk(kind, body, crc=None):
    if crc is None:
        crc = zlib.crc32(kind + body)
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", crc)


def pack_row(samples, depth):
    if depth == 8:
        return bytes(samples)
    if depth == 16:
        return b"".join(struct.pack(">H", v) for v in samples)
    out, acc, nbits = bytearray(), 0, 0
    for v in samples:
        acc = (acc << depth) | v
        nbits += depth
        if nbits == 8:
            out.append(acc)
            acc, nbits = 0, 0
    if nbits:
        out.append(acc << (8 - nbits))
    return bytes(out)


def filter_row(ftype, raw, prior, bpp):
    out = bytearray()
    for i, x in enumerate(raw):
        a = raw[i - bpp] if i >= bpp else 0
        b = prior[i]
        c = prior[i - bpp] if i >= bpp else 0
        if ftype == 0:
            p = 0
        elif ftype == 1:
            p = a
        elif ftype == 2:
            p = b
        elif ftype == 3:
            p = (a + b) // 2
        else:
            pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
            p = a if pa <= pb and pa <= pc else b if pb <= pc else c
        out.append((x - p) & 255)
    return bytes(out)


def encode_png(w, h, ctype, depth, pixels, palette=None, interlace=False, filters=(0, 1, 2, 3, 4)):
    """pixels[y][x] is a tuple of channel samples; returns PNG bytes."""
    channels = CHANNELS[ctype]
    bpp = max(1, channels * depth // 8)

    def encode_pass(rows):
        out, prior = bytearray(), None
        for y, row in enumerate(rows):
            raw = pack_row([s for px in row for s in px], depth)
            if prior is None:
                prior = bytes(len(raw))
            ftype = filters[y % len(filters)]
            out.append(ftype)
            out += filter_row(ftype, raw, prior, bpp)
            prior = raw
        return out

    if not interlace:
        body = encode_pass(pixels)
    else:
        body = bytearray()
        for xs, ys, dx, dy in ADAM7:
            rows = [[pixels[y][x] for x in range(xs, w, dx)] for y in range(ys, h, dy)]
            rows = [r for r in rows if r]
            if rows:
                body += encode_pass(rows)
    out = b"\x89PNG\r\n\x1a\n" + png_chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, depth, ctype, 0, 0, 1 if interlace else 0))
    if palette is not None:
        out += png_chunk(b"PLTE", bytes(v for rgb in palette for v in rgb))
    return out + png_chunk(b"IDAT", zlib.compress(bytes(body))) + png_chunk(b"IEND", b"")


def expected_rgba(pixels, ctype, depth, palette):
    out = bytearray()
    hi = (lambda v: v >> 8) if depth == 16 else (lambda v: v)
    for row in pixels:
        for px in row:
            if ctype == 0:
                v = px[0] * 255 // ((1 << depth) - 1) if depth < 8 else hi(px[0])
                out += bytes((v, v, v, 255))
            elif ctype == 2:
                out += bytes((hi(px[0]), hi(px[1]), hi(px[2]), 255))
            elif ctype == 3:
                out += bytes(palette[px[0]]) + b"\xff"
            elif ctype == 4:
                out += bytes((hi(px[0]),) * 3 + (hi(px[1]),))
            else:
                out += bytes(hi(c) for c in px)
    return bytes(out)


def random_pixels(rng, w, h, ctype, depth):
    channels = CHANNELS[ctype]
    top = (1 << depth) - 1
    return [[tuple(rng.randint(0, top) for _ in range(channels)) for _ in range(w)] for _ in range(h)]


def reference_decode_sub(data):
    """Decode a PNG made only of filter-1 rows (what write_png emits): (w, h, channels, bytes)."""
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    pos, idat, ihdr = 8, b"", None
    while pos < len(data):
        length, kind = struct.unpack(">I4s", data[pos : pos + 8])
        body = data[pos + 8 : pos + 8 + length]
        (crc,) = struct.unpack(">I", data[pos + 8 + length : pos + 12 + length])
        assert crc == zlib.crc32(kind + body), "bad CRC in " + kind.decode()
        if kind == b"IHDR":
            ihdr = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat += body
        pos += 12 + length
    w, h, depth, ctype, _, _, interlace = ihdr
    assert depth == 8 and interlace == 0
    channels = CHANNELS[ctype]
    raw = zlib.decompress(idat)
    stride = w * channels
    assert len(raw) == h * (stride + 1)
    out = bytearray()
    for y in range(h):
        line = raw[y * (stride + 1) : (y + 1) * (stride + 1)]
        assert line[0] == 1
        row = bytearray(line[1:])
        for i in range(channels, stride):
            row[i] = (row[i] + row[i - channels]) & 255
        out += row
    return w, h, channels, bytes(out)


def pixel(rgba, width, x, y):
    o = (y * width + x) * 4
    return tuple(rgba[o : o + 4])


def solid(w, h, rgb):
    return bytes(rgb) * (w * h)


def gradient_rgb(w, h):
    rows = []
    for y in range(h):
        rows.append(b"".join(bytes(((x * 255) // max(1, w - 1), (y * 255) // max(1, h - 1), (x * y) % 256)) for x in range(w)))
    return b"".join(rows)


# ======================================================================================
# contact_sheet: PNG codec
# ======================================================================================


def render(items, **kwargs):
    """A sheet built in memory from [(name, png_bytes)]: returns (summary, (width, height, rgba))."""
    summary, data = cs.render_sheet(items, **kwargs)
    return summary, cs.decode_png(data)


class PngDecodeTests(unittest.TestCase):
    FORMATS = [(0, 1), (0, 2), (0, 4), (0, 8), (0, 16), (2, 8), (2, 16), (3, 1), (3, 2), (3, 4), (3, 8), (4, 8), (4, 16), (6, 8), (6, 16)]
    SIZES = [(1, 1), (3, 2), (13, 9), (17, 5), (8, 8), (9, 17)]

    def test_every_type_depth_filter_and_interlace(self):
        rng = random.Random(1234)
        checked = 0
        for ctype, depth in self.FORMATS:
            for w, h in self.SIZES:
                pixels = random_pixels(rng, w, h, ctype, depth)
                palette = [tuple(rng.randrange(256) for _ in range(3)) for _ in range(min(256, 1 << depth))] if ctype == 3 else None
                if ctype == 3:  # keep palette indexes inside the palette
                    pixels = [[(px[0] % len(palette),) for px in row] for row in pixels]
                want = expected_rgba(pixels, ctype, depth, palette)
                for interlace in (False, True):
                    with self.subTest(ctype=ctype, depth=depth, size=(w, h), interlace=interlace):
                        data = encode_png(w, h, ctype, depth, pixels, palette, interlace)
                        got_w, got_h, rgba = cs.decode_png(data)
                        self.assertEqual((got_w, got_h), (w, h))
                        self.assertEqual(rgba, want)
                        checked += 1
        self.assertEqual(checked, len(self.FORMATS) * len(self.SIZES) * 2)

    def test_each_filter_type_alone(self):
        rng = random.Random(5)
        pixels = random_pixels(rng, 11, 7, 6, 8)
        want = expected_rgba(pixels, 6, 8, None)
        for ftype in range(5):
            with self.subTest(filter=ftype):
                data = encode_png(11, 7, 6, 8, pixels, filters=(ftype,))
                self.assertEqual(cs.decode_png(data)[2], want)

    def test_transparency_chunk_is_ignored_and_ancillary_chunks_are_skipped(self):
        rng = random.Random(6)
        palette = [(10, 20, 30), (200, 100, 50)]
        pixels = [[(rng.randrange(2),) for _ in range(5)] for _ in range(4)]
        data = encode_png(5, 4, 3, 8, pixels, palette)
        marker = data.index(b"IDAT") - 4
        extra = png_chunk(b"tRNS", b"\x00\x80") + png_chunk(b"tEXt", b"key\x00value") + png_chunk(b"gAMA", struct.pack(">I", 45455))
        data = data[:marker] + extra + data[marker:]
        self.assertEqual(cs.decode_png(data)[2], expected_rgba(pixels, 3, 8, palette))

    def test_write_png_output_is_a_valid_png(self):
        rng = random.Random(7)
        for channels in (1, 3, 4):
            w, h = 19, 6
            pixels = bytes(rng.randrange(256) for _ in range(w * h * channels))
            data = cs.png_bytes(w, h, pixels, channels)
            self.assertEqual(reference_decode_sub(data), (w, h, channels, pixels))

    def test_write_read_roundtrip_and_overwrite_rules(self):
        tmp = scratch("png_roundtrip")
        path = os.path.join(tmp, "a.png")
        rng = random.Random(8)
        rgb = bytes(rng.randrange(256) for _ in range(7 * 5 * 3))
        cs.write_png(path, 7, 5, rgb, 3)
        w, h, rgba = cs.read_png(path)
        self.assertEqual((w, h), (7, 5))
        self.assertEqual(rgba[0::4] + rgba[1::4] + rgba[2::4], rgb[0::3] + rgb[1::3] + rgb[2::3])
        self.assertEqual(set(rgba[3::4]), {255})
        self.assertEqual(cs.png_size(path), (7, 5))  # the header alone is enough for the size
        before = Path(path).read_bytes()
        with self.assertRaises(FileExistsError):
            cs.write_png(path, 7, 5, bytes(len(rgb)), 3)
        self.assertEqual(Path(path).read_bytes(), before, "a refused overwrite must leave the file alone")
        cs.write_png(path, 7, 5, bytes(len(rgb)), 3, replace=True)
        self.assertEqual(set(cs.read_png(path)[2]) - {255}, {0})
        self.assertEqual([n for n in os.listdir(tmp) if n != "a.png"], [])
        with self.assertRaises(ValueError):
            cs.png_bytes(2, 2, bytes(5), 3)
        with self.assertRaises(ValueError):
            cs.png_bytes(2, 2, bytes(8), 5)


class PngCorruptionTests(unittest.TestCase):
    def setUp(self):
        rng = random.Random(9)
        self.pixels = random_pixels(rng, 6, 4, 2, 8)
        self.good = encode_png(6, 4, 2, 8, self.pixels)

    def assertRejected(self, data, *words):
        with self.assertRaises(cs.PngError) as caught:
            cs.decode_png(data)
        message = str(caught.exception).lower()
        for word in words:
            self.assertIn(word, message)

    def test_good_file_decodes(self):
        self.assertEqual(cs.decode_png(self.good)[2], expected_rgba(self.pixels, 2, 8, None))

    def test_truncated_files(self):
        for cut in (len(self.good) // 2, len(self.good) - 13, 20, 8):
            with self.subTest(cut=cut):
                self.assertRejected(self.good[:cut], "png")

    def test_bad_signature_and_empty(self):
        self.assertRejected(b"GIF89a" + self.good[6:], "not a png")
        self.assertRejected(b"", "not a png")

    def test_crc_mismatch(self):
        at = self.good.index(b"IDAT") + 6
        broken = self.good[:at] + bytes([self.good[at] ^ 0xFF]) + self.good[at + 1 :]
        self.assertRejected(broken, "crc")

    def test_valid_crc_but_garbage_zlib(self):
        ihdr = png_chunk(b"IHDR", struct.pack(">IIBBBBB", 6, 4, 8, 2, 0, 0, 0))
        data = b"\x89PNG\r\n\x1a\n" + ihdr + png_chunk(b"IDAT", b"this is not zlib data at all") + png_chunk(b"IEND", b"")
        self.assertRejected(data, "zlib")

    def test_too_little_image_data(self):
        ihdr = png_chunk(b"IHDR", struct.pack(">IIBBBBB", 6, 4, 8, 2, 0, 0, 0))
        data = b"\x89PNG\r\n\x1a\n" + ihdr + png_chunk(b"IDAT", zlib.compress(b"\x00" * 30)) + png_chunk(b"IEND", b"")
        self.assertRejected(data, "truncated")

    def test_header_problems(self):
        sig = b"\x89PNG\r\n\x1a\n"
        idat = png_chunk(b"IDAT", zlib.compress(b"\x00" * 100)) + png_chunk(b"IEND", b"")
        for header, word in (
            (struct.pack(">IIBBBBB", 6, 4, 4, 2, 0, 0, 0), "bit depth"),  # RGB cannot be 4 bit
            (struct.pack(">IIBBBBB", 6, 4, 8, 5, 0, 0, 0), "colour type"),
            (struct.pack(">IIBBBBB", 6, 4, 8, 2, 1, 0, 0), "method"),
            (struct.pack(">IIBBBBB", 6, 4, 8, 2, 0, 0, 2), "interlace"),
            (struct.pack(">IIBBBBB", 0, 4, 8, 2, 0, 0, 0), "zero"),
        ):
            with self.subTest(word=word):
                self.assertRejected(sig + png_chunk(b"IHDR", header) + idat, word)
        self.assertRejected(sig + idat, "ihdr")

    def test_palette_problems(self):
        sig = b"\x89PNG\r\n\x1a\n"
        header = png_chunk(b"IHDR", struct.pack(">IIBBBBB", 3, 1, 8, 3, 0, 0, 0))
        body = png_chunk(b"IDAT", zlib.compress(b"\x00\x00\x01\x07")) + png_chunk(b"IEND", b"")
        self.assertRejected(sig + header + body, "plte")
        plte = png_chunk(b"PLTE", bytes([1, 2, 3, 4, 5, 6]))
        self.assertRejected(sig + header + plte + body, "palette index")

    def test_bad_filter_type(self):
        sig = b"\x89PNG\r\n\x1a\n"
        header = png_chunk(b"IHDR", struct.pack(">IIBBBBB", 2, 1, 8, 0, 0, 0, 0))
        body = png_chunk(b"IDAT", zlib.compress(b"\x09\x01\x02")) + png_chunk(b"IEND", b"")
        self.assertRejected(sig + header + body, "filter")

    def test_cli_reports_a_corrupt_input_and_writes_nothing(self):
        tmp = scratch("png_corrupt_cli")
        good, bad, out = (os.path.join(tmp, n) for n in ("good.png", "bad.png", "sheet.png"))
        Path(good).write_bytes(self.good)
        Path(bad).write_bytes(self.good[: len(self.good) // 2])
        code, stdout, stderr = run_main(cs, out, good, bad)
        self.assertEqual(code, 2)
        self.assertEqual(stdout, "")
        self.assertIn("bad.png", stderr)
        self.assertFalse(os.path.exists(out))
        code, _, stderr = run_main(cs, out, os.path.join(tmp, "missing.png"))
        self.assertEqual(code, 2)
        self.assertIn("missing.png", stderr)
        Path(bad).write_bytes(b"just some text")
        self.assertEqual(run_main(cs, out, bad)[0], 2)


# ======================================================================================
# contact_sheet: font, resize, statistics, layout, CLI
# ======================================================================================


class FontTests(unittest.TestCase):
    def test_every_printable_glyph_is_defined_and_distinct(self):
        glyphs = {}
        for code in range(33, 127):
            columns = bytes(cs._FONT[(code - 32) * 5 : (code - 32) * 5 + 5])
            self.assertTrue(any(columns), "empty glyph for %r" % chr(code))
            self.assertNotIn(columns, glyphs, "%r and %r look identical" % (chr(code), glyphs.get(columns)))
            glyphs[columns] = chr(code)
        self.assertFalse(any(cs._FONT[0:5]))  # space
        self.assertEqual(len(cs._FONT), 95 * 5)

    def test_known_glyphs_and_rendering(self):
        self.assertEqual(bytes(cs._FONT[(ord("A") - 32) * 5 : (ord("A") - 32) * 5 + 5]), bytes.fromhex("7e1111117e"))
        width, rows = cs.text_mask("A", 1)
        self.assertEqual((width, len(rows)), (5, 7))
        picture = ["".join("#" if v else "." for v in row) for row in rows]
        self.assertEqual(picture, [".###.", "#...#", "#...#", "#...#", "#####", "#...#", "#...#"])
        width, rows = cs.text_mask("AB", 2)
        self.assertEqual((width, len(rows), len(rows[0])), (22, 14, 22))

    def test_non_ascii_becomes_a_question_mark(self):
        self.assertEqual(cs.text_mask("\xe9\u4e2d")[1], cs.text_mask("??")[1])
        self.assertEqual(cs._fit_text("abcdefghij", 10), "abcdefghij")
        shortened = cs._fit_text("bots_visible_wizard_red_crosshair", 26)
        self.assertEqual(len(shortened), 26)
        self.assertTrue(shortened.endswith("crosshair"))
        self.assertIn("...", shortened)


class ResizeAndStatsTests(unittest.TestCase):
    @staticmethod
    def reference_area(rows, w, h, tw, th):
        """Exact fractional-area average in floating point (independent of the integer code)."""

        def weights(src, dst):
            out = []
            scale = src / dst
            for j in range(dst):
                lo, hi = j * scale, (j + 1) * scale
                out.append([(k, (min(hi, k + 1) - max(lo, k)) / scale) for k in range(int(math.floor(lo)), min(src, int(math.ceil(hi)))) if min(hi, k + 1) > max(lo, k)])
            return out

        wx, wy = weights(w, tw), weights(h, th)
        out = []
        for jy in range(th):
            line = []
            for jx in range(tw):
                for c in range(3):
                    total = sum(wty * wtx * rows[ky][3 * kx + c] for ky, wty in wy[jy] for kx, wtx in wx[jx])
                    line.append(total)
            out.append(line)
        return out

    def test_area_resize_matches_float_reference(self):
        rng = random.Random(11)
        for w, h, tw, th in ((37, 23, 8, 5), (32, 16, 8, 4), (10, 10, 3, 7), (5, 40, 5, 3), (9, 9, 1, 1), (64, 3, 21, 3)):
            rows = [bytes(rng.randrange(256) for _ in range(3 * w)) for _ in range(h)]
            got = cs._area_resize(rows, w, h, tw, th)
            want = self.reference_area(rows, w, h, tw, th)
            worst = max(abs(g - v) for grow, wrow in zip(got, want) for g, v in zip(grow, wrow))
            self.assertLessEqual(worst, 0.5 + 1e-9, (w, h, tw, th))
            self.assertEqual((len(got), len(got[0])), (th, 3 * tw))

    def test_checkerboard_reduces_to_gray_without_aliasing(self):
        w = h = 64
        rows = []
        for y in range(h):
            rows.append(b"".join(b"\xff\xff\xff" if (x + y) % 2 == 0 else b"\x00\x00\x00" for x in range(w)))
        for tw in (8, 16, 32):
            reduced = cs._area_resize(rows, w, h, tw, tw)
            self.assertEqual({v for row in reduced for v in row}, {128})

    def test_thumbnail_fit_rules(self):
        self.assertEqual(cs._fit_size(1920, 1080, 320, 180)[:3], ("down", 320, 180))
        self.assertEqual(cs._fit_size(1028, 720, 320, 224)[:3], ("down", 320, 224))
        self.assertEqual(cs._fit_size(100, 50, 60, 60)[:3], ("down", 60, 30))  # letterboxed
        self.assertEqual(cs._fit_size(10, 10, 40, 40), ("up", 40, 40, 4))  # integer multiple only
        self.assertEqual(cs._fit_size(20, 10, 50, 50), ("up", 40, 20, 2))
        self.assertEqual(cs._fit_size(64, 64, 64, 64), ("up", 64, 64, 1))

    def test_uniform_flag_and_luma_statistics(self):
        def stats(w, h, data, channels=3):
            return cs.frame_stats(cs.png_bytes(w, h, data, channels))  # PNG bytes work as well as a path

        flat = stats(40, 30, solid(40, 30, (90, 90, 90)))
        self.assertTrue(flat["uniform"])
        self.assertEqual((flat["width"], flat["height"]), (40, 30))
        self.assertAlmostEqual(flat["luma_stddev"], 0.0, places=3)
        self.assertAlmostEqual(flat["luma_mean"], 90.0, delta=0.05)
        self.assertTrue(stats(20, 20, bytes(20 * 20 * 3))["uniform"])
        half = stats(20, 10, b"\x00\x00\x00" * 100 + b"\xff\xff\xff" * 100)
        self.assertFalse(half["uniform"])
        self.assertAlmostEqual(half["luma_stddev"], 127.5, delta=0.5)
        # a single small bright feature on black is not a blank frame
        cross = bytearray(200 * 100 * 3)
        for i in range(-6, 7):
            for x, y in ((100 + i, 50), (100, 50 + i)):
                cross[(y * 200 + x) * 3 : (y * 200 + x) * 3 + 3] = b"\xff\xff\xff"
        self.assertFalse(stats(200, 100, bytes(cross))["uniform"])
        # faint noise (stddev below 2) is flat, visible texture is not
        rng = random.Random(3)
        faint = bytes(100 + rng.choice((-1, 0, 1)) for _ in range(50 * 50 * 3))
        self.assertTrue(stats(50, 50, faint)["uniform"])
        busy = bytes(rng.randrange(256) for _ in range(50 * 50 * 3))
        self.assertFalse(stats(50, 50, busy)["uniform"])
        # a fully transparent picture is composited over the background, so it is flat
        self.assertTrue(stats(30, 30, bytes((255, 0, 0, 0)) * 900, 4)["uniform"])
        # an opaque RGBA picture keeps its own colours
        self.assertFalse(stats(30, 30, bytes((255, 0, 0, 255)) * 450 + bytes((0, 0, 255, 255)) * 450, 4)["uniform"])
        # the same numbers come back from a file path
        path = os.path.join(scratch("luma_stats"), "flat.png")
        cs.write_png(path, 40, 30, solid(40, 30, (90, 90, 90)), 3)
        self.assertEqual(cs.frame_stats(path), flat)


class SheetTests(TempDirCase):
    @classmethod
    def setUpClass(cls):
        super().setUpClass()
        cls.grad = str(cls.root / "grad8.png")  # small inputs shared by several tests
        cs.write_png(cls.grad, 8, 8, gradient_rgb(8, 8), 3)
        cls.black = str(cls.root / "black.png")
        cs.write_png(cls.black, 50, 40, bytes(50 * 40 * 3), 3)
        cls.live = str(cls.root / "live.png")
        cs.write_png(cls.live, 50, 40, gradient_rgb(50, 40), 3)

    def sheet(self, *args):
        out = self.dir / "sheet.png"
        if out.exists():
            out.unlink()
        code, stdout, stderr = run_main(cs, out, *args)
        self.assertEqual(code, 0, stderr)
        summary = json.loads(stdout)
        return summary, cs.read_png(out)

    def test_layout_dimensions_and_pixels(self):
        colors = [(255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 0)]
        items = [("c%d.png" % i, cs.png_bytes(40, 30, solid(40, 30, rgb), 3)) for i, rgb in enumerate(colors)]
        summary, (w, h, rgba) = render(items, cell=40, cols=2, gap=4)
        gap, cw, ch = 4, 40, 30
        self.assertEqual((summary["width"], summary["height"]), (w, h))
        self.assertEqual(w, gap + 2 * (cw + gap))
        self.assertEqual(h, gap + 2 * (ch + cs.LABEL_H + gap))
        self.assertEqual((summary["cols"], summary["rows"]), (2, 2))
        self.assertEqual(summary["cell"], {"width": 40, "height": 30})
        self.assertEqual(pixel(rgba, w, 0, 0), (0x20, 0x20, 0x20, 255))  # default background
        for i, rgb in enumerate(colors):
            r, c = divmod(i, 2)
            x, y = gap + c * (cw + gap), gap + r * (ch + cs.LABEL_H + gap)
            for dx, dy in ((0, 0), (cw - 1, ch - 1), (20, 15)):
                self.assertEqual(pixel(rgba, w, x + dx, y + dy), rgb + (255,))
            strip = [pixel(rgba, w, x + dx, y + ch + dy)[:3] for dx in range(cw) for dy in range(cs.LABEL_H)]
            self.assertTrue(any(min(p) >= 180 for p in strip), "no light caption strip")
            self.assertTrue(any(max(p) <= 60 for p in strip), "no caption text")
        self.assertEqual([i["uniform"] for i in summary["images"]], [True] * 4)

    def test_json_fields_and_original_sizes_are_reported(self):
        wide = cs.png_bytes(64, 32, gradient_rgb(64, 32), 3)
        tall = cs.png_bytes(20, 60, gradient_rgb(20, 60), 3)
        summary, _ = render([("wide.png", wide), ("tall.png", tall)], cell=40)
        keys = ["output", "width", "height", "cols", "rows", "cell", "labels", "uniform_count", "images", "warnings"]
        self.assertEqual(sorted(summary), sorted(keys))
        self.assertEqual([(i["width"], i["height"]) for i in summary["images"]], [(64, 32), (20, 60)])
        self.assertEqual(set(summary["images"][0]), {"path", "width", "height", "uniform", "luma_stddev"})
        self.assertEqual(summary["cell"], {"width": 40, "height": 20})  # height from the first image
        self.assertTrue(any("mixed image sizes" in w for w in summary["warnings"]))
        self.assertEqual(summary["uniform_count"], 0)

    def test_aspect_ratio_is_kept_and_small_images_are_not_blurred(self):
        wide = cs.png_bytes(100, 50, solid(100, 50, (200, 0, 0)), 3)
        summary, (w, _h, rgba) = render([("wide.png", wide)], cell=(60, 60), gap=2, cols=1)
        self.assertEqual(summary["cell"], {"width": 60, "height": 60})
        x, y = 2, 2
        self.assertEqual(pixel(rgba, w, x + 30, y + 30), (200, 0, 0, 255))  # inside the 60x30 thumbnail
        self.assertEqual(pixel(rgba, w, x + 30, y + 5), (0x20, 0x20, 0x20, 255))  # letterbox above
        self.assertEqual(pixel(rgba, w, x + 30, y + 54), (0x20, 0x20, 0x20, 255))  # letterbox below
        checker = bytes((255, 255, 255, 0, 0, 0, 0, 0, 0, 255, 255, 255))  # 2x2 black/white
        _, (w, _h, rgba) = render([("tiny.png", cs.png_bytes(2, 2, checker, 3))], cell=(40, 40), gap=2, cols=1)
        for dx, dy, want in ((0, 0, 255), (19, 19, 255), (20, 0, 0), (39, 19, 0), (0, 20, 0), (39, 39, 255)):
            self.assertEqual(pixel(rgba, w, 2 + dx, 2 + dy)[:3], (want,) * 3, (dx, dy))

    def test_blank_frames_are_flagged(self):
        black = cs.png_bytes(50, 40, bytes(50 * 40 * 3), 3)
        live = cs.png_bytes(50, 40, gradient_rgb(50, 40), 3)
        summary, (w, _h, rgba) = render([("black.png", black), ("live.png", live)], cell=50, cols=2)
        self.assertEqual([i["uniform"] for i in summary["images"]], [True, False])
        self.assertEqual(summary["uniform_count"], 1)
        self.assertTrue(any("black.png" in warning and "blank" in warning for warning in summary["warnings"]))
        strip_y = 8 + 40 + 2
        self.assertEqual(pixel(rgba, w, 8 + 1, strip_y)[:3], cs.STRIP_BLANK_RGB)
        self.assertEqual(pixel(rgba, w, 8 + 50 + 8 + 1, strip_y)[:3], cs.STRIP_RGB)

    def test_natural_sort_directory_and_explicit_order(self):
        grad = Path(self.grad).read_bytes()
        for name in ("shot_10.png", "shot_2.png", "shot_1.png", "frame_3.PNG"):
            (self.dir / name).write_bytes(grad)  # real PNGs, so the same folder also feeds a sheet below
        (self.dir / "notes.txt").write_text("not an image")
        out = str(self.dir / "sheet.png")

        def order(images, directory):
            return [os.path.basename(p) for p in cs._gather_inputs(images, directory, out)]

        self.assertEqual(order([], str(self.dir)), ["frame_3.PNG", "shot_1.png", "shot_2.png", "shot_10.png"])
        first = str(self.dir / "shot_10.png")  # explicit paths keep their place and come first
        self.assertEqual(order([first], str(self.dir)), ["shot_10.png", "frame_3.PNG", "shot_1.png", "shot_2.png"])
        # a pattern is expanded by the tool itself, in natural order
        self.assertEqual(order([str(self.dir / "shot_*.png")], None), ["shot_1.png", "shot_2.png", "shot_10.png"])
        # and the order reaches the sheet
        summary, _ = self.sheet("--dir", str(self.dir), "--cell", "32")
        self.assertEqual(
            [os.path.basename(i["path"]) for i in summary["images"]],
            ["frame_3.PNG", "shot_1.png", "shot_2.png", "shot_10.png"],
        )

    def test_output_inside_the_input_directory_is_not_used_as_input(self):
        (self.dir / "a.png").write_bytes(Path(self.grad).read_bytes())
        out = self.dir / "sheet.png"
        self.assertEqual(run_main(cs, out, "--dir", self.dir)[0], 0)
        code, stdout, _ = run_main(cs, out, "--dir", self.dir, "--replace")
        self.assertEqual(code, 0)
        self.assertEqual([os.path.basename(i["path"]) for i in json.loads(stdout)["images"]], ["a.png"])

    def test_refuses_to_overwrite_unless_replace(self):
        a = self.grad
        out = self.dir / "sheet.png"
        self.assertEqual(run_main(cs, out, a)[0], 0)
        before = out.read_bytes()
        code, stdout, stderr = run_main(cs, out, a)
        self.assertEqual((code, stdout), (2, ""))
        self.assertIn("already exists", stderr)
        self.assertEqual(out.read_bytes(), before)
        self.assertEqual(run_main(cs, out, a, "--replace", "--cell", "64")[0], 0)
        self.assertNotEqual(out.read_bytes(), before)
        self.assertEqual([n for n in os.listdir(self.dir) if ".tmp" in n], [])

    def test_natural_key_never_compares_text_with_numbers(self):
        names = ["x", "x5", "shot_10.png", "shot_2.png", "shot.png", "shot1.png", "a10b2.png", "a10b10.png", "a9.png", "Shot_3.PNG"]
        self.assertEqual(
            sorted(names, key=cs.natural_key),
            ["a9.png", "a10b2.png", "a10b10.png", "shot1.png", "shot.png", "shot_2.png", "Shot_3.PNG", "shot_10.png", "x", "x5"],
        )

    def test_output_directory_is_created_when_missing(self):
        a = self.grad
        out = self.dir / "new" / "deeper" / "sheet.png"
        self.assertEqual(run_main(cs, out, a, "--cell", "32")[0], 0)
        self.assertTrue(out.exists())

    def test_default_columns_and_options(self):
        for n, want in ((1, 1), (2, 2), (3, 2), (4, 2), (5, 3), (9, 3), (10, 4), (16, 4), (17, 4), (100, 4)):
            self.assertEqual(cs.default_cols(n), want, n)
        grad = cs.png_bytes(8, 8, gradient_rgb(8, 8), 3)
        summary, _ = cs.render_sheet([("g.png", grad)] * 5, cell=(16, 16))
        self.assertEqual((summary["cols"], summary["rows"]), (3, 2))
        wide = cs.png_bytes(8, 4, solid(8, 4, (250, 250, 250)), 3)  # shown at 2x (16x8) inside a 16x16 cell
        plain, data = cs.render_sheet([("wide.png", wide)] * 2, cell=(16, 16), cols=2, gap=0, labels=False, bg=(1, 2, 3))
        self.assertEqual((plain["width"], plain["height"]), (32, 16))
        self.assertFalse(plain["labels"])
        w, h, rgba = cs.decode_png(data)
        self.assertEqual((w, h), (32, 16))
        self.assertEqual(pixel(rgba, w, 0, 0), (1, 2, 3, 255))  # letterbox shows the --bg colour
        self.assertEqual(pixel(rgba, w, 8, 8), (250, 250, 250, 255))

    def test_bad_arguments(self):
        a = self.grad
        out = self.root / "o.png"  # never created, so no per-test directory is needed
        for args in (("--cell", "abc"), ("--cell", "8"), ("--cols", "0"), ("--bg", "12345"), ("--gap", "-1")):
            with self.subTest(args=args):
                self.assertEqual(run_main(cs, out, a, *args)[0], 2)
                self.assertFalse(out.exists())
        self.assertEqual(run_main(cs, out)[0], 2)  # no inputs
        self.assertEqual(run_main(cs, out, str(self.root / "nothing_*.png"))[0], 2)
        self.assertEqual(run_main(cs, out, str(self.root))[0], 2)  # a directory as an image

    def test_huge_sheets_are_refused_before_any_work(self):
        a = self.grad
        code, _, stderr = run_main(cs, self.root / "huge.png", *([a] * 200), "--cell", "4000", "--cols", "1")
        self.assertEqual(code, 2)
        self.assertIn("sheet would be", stderr)

    def test_unicode_file_name_is_captioned_and_reported(self):
        png = cs.png_bytes(16, 16, gradient_rgb(16, 16), 3)
        name = "caf\u00e9_\u4e2d.png"
        summary, data = cs.render_sheet([(name, png)], cell=64)
        self.assertEqual(summary["images"][0]["path"], name)
        self.assertTrue(json.dumps(summary, separators=(",", ":")).isascii())  # what the command line prints
        _, same_with_question_marks = cs.render_sheet([("caf?_?.png", png)], cell=64)
        self.assertEqual(data, same_with_question_marks, "characters outside ASCII are captioned as '?'")

    def test_command_line_end_to_end(self):
        a, b = self.live, self.black
        out = self.dir / "cli.png"
        code, stdout, stderr = run_script("contact_sheet.py", out, a, b, "--cell", "48", "--json")
        self.assertEqual(code, 0, stderr)
        summary = json.loads(stdout)
        self.assertEqual(summary["uniform_count"], 1)
        self.assertEqual(summary["output"], str(out).replace(os.sep, "/"))
        self.assertEqual(cs.read_png(out)[:2], (summary["width"], summary["height"]))
        code, stdout, stderr = run_script("contact_sheet.py", out, a)  # OUT exists now
        self.assertEqual((code, stdout), (2, ""))
        self.assertIn("already exists", stderr)
        self.assertEqual(run_script("contact_sheet.py", "--help")[0], 0)

    def test_downscaling_1080p_is_fast_enough(self):
        rng = random.Random(1)
        texture = bytes(rng.randrange(256) for _ in range(64)) * 100  # cheap to compress, far from flat
        rows = b"".join(texture[(y * 7) % 64 : (y * 7) % 64 + 5760] for y in range(1080))
        big = cs.png_bytes(1920, 1080, rows, 3)
        started = time.process_time()  # CPU time: a busy machine must not fail the test
        summary, data = cs.render_sheet([("big.png", big)])  # decode, exact area reduction, statistics, encode
        elapsed = time.process_time() - started
        self.assertEqual((summary["images"][0]["width"], summary["images"][0]["height"]), (1920, 1080))
        self.assertFalse(summary["images"][0]["uniform"])
        self.assertEqual(summary["cell"], {"width": 320, "height": 180})
        self.assertEqual(cs.decode_png(data)[:2], (summary["width"], summary["height"]))
        self.assertLess(elapsed, 20.0, "1920x1080 took %.1f s of CPU" % elapsed)  # about 1 s normally


# ======================================================================================
# audio_report
# ======================================================================================

RATE = 44100


def fade_env(n, rate=RATE, ms=10):
    k = int(rate * ms / 1000)
    env = [1.0] * n
    for i in range(k):
        g = 0.5 - 0.5 * math.cos(math.pi * i / k)
        env[i] = g
        env[n - 1 - i] = g
    return env


def sine(freq, amp, secs, rate=RATE, phase=0.0, fade=True):
    n = int(rate * secs)
    env = fade_env(n, rate) if fade else [1.0] * n
    return [amp * env[i] * math.sin(2 * math.pi * freq * i / rate + phase) for i in range(n)]


def pcm_bytes(samples, width):
    """Little-endian PCM of `width` bytes (1 = unsigned 8-bit) from floats, clipped to full scale."""
    peak = {1: 127, 2: 32767, 3: 8388607, 4: 2147483647}[width]
    ints = [max(-peak, min(peak, int(round(v * peak)))) for v in samples]
    if width == 1:
        return bytes(v + 128 for v in ints)
    if width == 2:
        return array.array("h", ints).tobytes()
    if width == 3:
        return b"".join(struct.pack("<i", v)[:3] for v in ints)
    return array.array("i", ints).tobytes()


def wav_bytes(chans, rate=RATE, kind="pcm16", extensible=False, extra_chunks=b"", data_first=False):
    nch, n = len(chans), len(chans[0])
    inter = [chans[c][i] for i in range(n) for c in range(nch)]
    bits = {"pcm8": 8, "pcm16": 16, "pcm24": 24, "pcm32": 32, "f32": 32, "f64": 64}[kind]
    if kind.startswith("pcm"):
        data = pcm_bytes(inter, bits // 8)
    elif kind == "f32":
        data = array.array("f", inter).tobytes()
    else:
        data = array.array("d", inter).tobytes()
    tag = 3 if kind.startswith("f") else 1
    if extensible:
        guid_tail = bytes.fromhex("000000001000800000aa00389b71")
        fmt = struct.pack("<HHIIHH", 0xFFFE, nch, rate, rate * nch * bits // 8, nch * bits // 8, bits)
        fmt += struct.pack("<HHI", 22, bits, 3 if nch == 2 else 4) + struct.pack("<H", tag) + guid_tail
    else:
        fmt = struct.pack("<HHIIHH", tag, nch, rate, rate * nch * bits // 8, nch * bits // 8, bits)
    fmt_chunk = b"fmt " + struct.pack("<I", len(fmt)) + fmt
    data_chunk = b"data" + struct.pack("<I", len(data)) + data + (b"\x00" if len(data) % 2 else b"")
    body = b"WAVE" + (data_chunk + extra_chunks + fmt_chunk if data_first else extra_chunks + fmt_chunk + data_chunk)
    return b"RIFF" + struct.pack("<I", len(body)) + body


def report_for(chans, rate=RATE, **kwargs):
    return ar.analyze(ar.parse_wav(wav_bytes(chans, rate, **kwargs)))


class WavReadingTests(unittest.TestCase):
    def test_all_sample_formats_are_decoded_exactly(self):
        tone = sine(440, 0.5, 0.05, fade=False)
        tolerance = {"pcm8": 0.012, "pcm16": 1e-4, "pcm24": 1e-6, "pcm32": 1e-8, "f32": 1e-7, "f64": 1e-12}
        bits = {"pcm8": 8, "pcm16": 16, "pcm24": 24, "pcm32": 32, "f32": 32, "f64": 64}
        for kind in tolerance:
            for extensible in (False, True):
                with self.subTest(kind=kind, extensible=extensible):
                    wav = ar.parse_wav(wav_bytes([tone], kind=kind, extensible=extensible))
                    self.assertEqual((wav["frames"], wav["rate"], wav["channels"]), (len(tone), RATE, 1))
                    label = ("float %d-bit" if kind[0] == "f" else "PCM %d-bit") % bits[kind]
                    self.assertEqual(wav["format"], label + (" (extensible)" if extensible else ""))
                    worst = max(abs(a - b) for a, b in zip(wav["data"][0], tone))
                    self.assertLess(worst, tolerance[kind])
        rep = report_for([tone], kind="pcm24", extensible=True)  # and once through the whole report
        self.assertAlmostEqual(rep["peak_dbfs"], -6.02, delta=0.05)
        self.assertAlmostEqual(rep["rms_dbfs"], -9.03, delta=0.1)

    def test_files_written_by_the_wave_module_are_read(self):
        tone = sine(440, 0.5, 0.05)
        tolerance = {1: 0.012, 2: 1e-4, 3: 1e-6, 4: 1e-8}
        for width in (1, 2, 3, 4):
            buffer = io.BytesIO()  # the standard library writer, into memory
            with wave.open(buffer, "wb") as w:
                w.setnchannels(1)
                w.setsampwidth(width)
                w.setframerate(22050)
                w.writeframes(pcm_bytes(tone, width))
            wav = ar.parse_wav(buffer.getvalue())
            self.assertEqual((wav["rate"], wav["frames"], wav["bits"]), (22050, len(tone), 8 * width))
            worst = max(abs(a - b) for a, b in zip(wav["data"][0], tone))
            self.assertLess(worst, tolerance[width], "width %d" % width)

    def test_extra_chunks_padding_and_chunk_order(self):
        tone = sine(440, 0.5, 0.05)
        odd = b"LIST" + struct.pack("<I", 5) + b"abcde" + b"\x00"  # odd size, padded
        for data_first in (False, True):
            wav = ar.parse_wav(wav_bytes([tone], extra_chunks=odd, data_first=data_first))
            self.assertEqual(wav["frames"], len(tone), data_first)

    def test_stereo_channel_peaks(self):
        left, right = sine(440, 0.5, 0.25), sine(1000, 0.25, 0.25)
        rep = report_for([left, right])
        self.assertEqual(rep["channels"], 2)
        self.assertAlmostEqual(rep["peak_dbfs_channels"][0], -6.02, delta=0.05)
        self.assertAlmostEqual(rep["peak_dbfs_channels"][1], -12.04, delta=0.05)
        self.assertAlmostEqual(rep["peak_dbfs"], -6.02, delta=0.05)
        self.assertAlmostEqual(rep["spectrum"]["dominant_hz"], 440, delta=15)

    def test_surround_file_is_accepted(self):
        chans = [sine(200 + 100 * i, 0.3, 0.1) for i in range(6)]
        rep = report_for(chans, extensible=True)
        self.assertEqual(rep["channels"], 6)
        self.assertEqual(len(rep["peak_dbfs_channels"]), 6)

    def test_float_file_with_nan_and_infinity(self):
        tone = sine(440, 0.3, 0.1)
        tone[100], tone[200], tone[300] = float("nan"), float("inf"), float("-inf")
        rep = report_for([tone], kind="f32")
        self.assertIn("non-finite samples", rep["issue_codes"])
        self.assertTrue(math.isfinite(rep["peak_dbfs"]))
        self.assertIn("3 NaN/inf", " ".join(rep["issues"]))

    def test_truncated_and_streamed_files(self):
        tone = sine(440, 0.5, 0.2)
        data = wav_bytes([tone])
        cut = ar.analyze(ar.parse_wav(data[: len(data) // 2]))
        self.assertIn("truncated", cut["issue_codes"])
        self.assertLess(cut["frames"], len(tone))
        self.assertGreater(cut["frames"], len(tone) // 3)
        streamed = bytearray(data)
        streamed[4:8] = b"\xff\xff\xff\xff"
        streamed[data.index(b"data") + 4 : data.index(b"data") + 8] = b"\xff\xff\xff\xff"
        wav = ar.parse_wav(bytes(streamed))
        self.assertEqual(wav["frames"], len(tone))
        self.assertFalse(wav["truncated"])

    def test_unreadable_files(self):
        good = wav_bytes([sine(440, 0.5, 0.02)])
        for label, data in (
            ("garbage", b"this is not a wav file, just some text!"),
            ("empty", b""),
            ("riff only", b"RIFF\x04\x00\x00\x00WAVE"),
            ("no data", good[: good.index(b"data")]),
            ("no samples", good[: good.index(b"data") + 8]),
        ):
            with self.subTest(label):
                with self.assertRaises(ar.WavError):
                    ar.parse_wav(data)
        adpcm = bytearray(good)
        adpcm[good.index(b"fmt ") + 8 : good.index(b"fmt ") + 10] = struct.pack("<H", 0x11)
        with self.assertRaises(ar.WavError) as caught:
            ar.parse_wav(bytes(adpcm))
        self.assertIn("IMA ADPCM", str(caught.exception))
        bad_bits = bytearray(good)
        bad_bits[good.index(b"fmt ") + 22 : good.index(b"fmt ") + 24] = struct.pack("<H", 12)
        with self.assertRaises(ar.WavError):
            ar.parse_wav(bytes(bad_bits))

    def test_tiny_files_do_not_break_the_analysis(self):
        for frames in (1, 2, 10, 100, 500, 2047, 2048, 2049):
            with self.subTest(frames=frames):
                rep = report_for([[0.5 * math.sin(i + 1.0) for i in range(frames)]])
                self.assertEqual(rep["frames"], frames)
                self.assertIsNotNone(rep["peak_dbfs"])


class LevelAndIssueTests(unittest.TestCase):
    def test_clean_sine_has_no_issues(self):
        rep = report_for([sine(440, 0.5, 0.6)])
        self.assertEqual(rep["issues"], [])
        self.assertAlmostEqual(rep["spectrum"]["dominant_hz"], 440, delta=15)
        self.assertAlmostEqual(rep["spectrum"]["centroid_hz"], 440, delta=60)
        self.assertAlmostEqual(rep["peak_dbfs"], -6.02, delta=0.05)
        self.assertAlmostEqual(rep["rms_dbfs"], -9.03, delta=0.1)
        self.assertAlmostEqual(rep["crest_db"], 3.01, delta=0.15)
        self.assertAlmostEqual(rep["loudness_lufs_approx"], -9.7, delta=0.15)
        self.assertEqual((rep["clipped_samples"], rep["clicks"]["count"]), (0, 0))
        self.assertGreater(rep["spectrum"]["band_energy_pct"]["low_mid"], 99.0)
        self.assertAlmostEqual(sum(rep["spectrum"]["band_energy_pct"].values()), 100.0, delta=0.05)
        self.assertLess(rep["silence"]["leading_ms"] + rep["silence"]["trailing_ms"], 25)

    def test_dominant_frequency_over_a_range(self):
        for freq in (100, 440, 1000, 5000, 12000):
            rep = report_for([sine(freq, 0.4, 0.15)])
            self.assertAlmostEqual(rep["spectrum"]["dominant_hz"], freq, delta=15, msg=str(freq))

    def test_loudness_of_a_997_hz_sine_follows_the_definition(self):
        # BS.1770: a 997 Hz sine at -23.01 dBFS RMS reads -23.01 LUFS (the -0.691 offset)
        rep = report_for([sine(997, 0.1, 1.0)])
        self.assertAlmostEqual(rep["loudness_lufs_approx"], -23.01, delta=0.12)

    def test_k_weighting_coefficients_match_the_itu_table_at_48k(self):
        shelf_b, shelf_a, high_b, high_a = ar.k_weighting_coeffs(48000)
        for got, want in zip(shelf_b, (1.53512485958697, -2.69169618940638, 1.19839281085285)):
            self.assertAlmostEqual(got, want, places=9)
        for got, want in zip(shelf_a, (-1.69065929318241, 0.73248077421585)):
            self.assertAlmostEqual(got, want, places=9)
        self.assertEqual(high_b, (1.0, -2.0, 1.0))
        for got, want in zip(high_a, (-1.99004745483398, 0.99007225036621)):
            self.assertAlmostEqual(got, want, places=9)

    def test_gating_ignores_near_silence(self):
        loud = sine(300, 0.5, 1.0)
        quiet = [v * 0.001 for v in loud]  # -60 dB below: under the relative gate
        both = report_for([loud + quiet])
        alone = report_for([loud])
        # 400 ms blocks straddling the change are legitimately counted (ffmpeg's ebur128 reads
        # -10.6 and -9.8 LUFS for these two files, this tool -10.55 and -9.83), but the silent
        # half must not drag the value down the way a plain average would (-3 dB)
        self.assertAlmostEqual(both["loudness_lufs_approx"], alone["loudness_lufs_approx"], delta=1.0)
        self.assertLess(both["rms_dbfs"], alone["rms_dbfs"] - 2.5)  # plain RMS does drop by 3 dB

    def test_short_file_is_measured_as_one_block(self):
        rep = report_for([sine(997, 0.1, 0.2, fade=False)])
        self.assertIn("one block", rep["loudness_method"])
        self.assertAlmostEqual(rep["loudness_lufs_approx"], -23.01, delta=0.3)

    def test_clipped_square_wave(self):
        n = RATE // 8
        square = [1.0 if (i // 110) % 2 == 0 else -1.0 for i in range(n)]
        rep = report_for([square])
        self.assertIn("clipping", rep["issue_codes"])
        self.assertGreater(rep["clipped_pct"], 95.0)
        self.assertGreaterEqual(rep["clipped_longest_run"], 100)
        self.assertEqual(rep["clicks"]["count"], 0, "repeating square edges are waveform, not clicks")
        self.assertTrue(any(text.startswith("clipping: ") and "%" in text for text in rep["issues"]))

    def test_single_full_scale_sample_is_not_clipping(self):
        tone = sine(440, 0.5, 0.2)
        tone[len(tone) // 2] = 1.0
        rep = report_for([tone])
        self.assertNotIn("clipping", rep["issue_codes"])

    def test_hot_sine_is_reported_as_clipping_and_loud(self):
        rep = report_for([sine(200, 1.0, 0.2)])
        self.assertIn("clipping", rep["issue_codes"])
        self.assertIn("too loud", rep["issue_codes"])

    def test_dc_offset(self):
        n = int(RATE * 0.3)
        env = fade_env(n)
        biased = [0.3 * env[i] * math.sin(2 * math.pi * 440 * i / RATE) + 0.08 for i in range(n)]
        rep = report_for([biased])
        self.assertIn("dc offset", rep["issue_codes"])
        self.assertAlmostEqual(rep["dc_offset"], 0.08, delta=0.002)
        small = [0.3 * env[i] * math.sin(2 * math.pi * 440 * i / RATE) + 0.004 for i in range(n)]
        self.assertNotIn("dc offset", report_for([small])["issue_codes"])

    def test_bass_content_is_not_mistaken_for_dc_offset(self):
        for freq, decay in ((60, 7.0), (50, 12.0), (40, 9.0)):
            kick = [0.9 * math.exp(-i / RATE * decay) * math.sin(2 * math.pi * freq * i / RATE) for i in range(int(RATE * 0.3))]
            rep = report_for([kick])
            self.assertNotIn("dc offset", rep["issue_codes"], (freq, rep["dc_offset"]))

    def test_start_and_end_clicks(self):
        n = int(RATE * 0.15)
        env = fade_env(n)
        starts = [0.8 * (env[i] if i > n // 2 else 1.0) * math.sin(2 * math.pi * 440 * i / RATE + math.pi / 2) for i in range(n)]
        rep = report_for([starts])
        self.assertIn("starts with a click", rep["issue_codes"])
        self.assertNotIn("ends with a click", rep["issue_codes"])
        self.assertTrue(any("first sample +0.8" in text for text in rep["issues"]))
        ends = [0.8 * (env[i] if i < n // 2 else 1.0) * math.sin(2 * math.pi * 440 * i / RATE + math.pi / 2) for i in range(n)]
        rep = report_for([ends])
        self.assertIn("ends with a click", rep["issue_codes"])
        self.assertNotIn("starts with a click", rep["issue_codes"])

    def test_loop_boundary_checks_continuity_instead_of_zero_endpoints(self):
        n = RATE // 5
        tone = [0.3 * math.cos(2 * math.pi * 440 * i / RATE) for i in range(n)]
        wav = ar.parse_wav(wav_bytes([tone]))
        self.assertIn("starts with a click", ar.analyze(wav)["issue_codes"])
        loop = ar.analyze(wav, loop=True)
        self.assertNotIn("starts with a click", loop["issue_codes"])
        self.assertNotIn("ends with a click", loop["issue_codes"])
        self.assertFalse(loop["loop"]["suspected_click"])
        # A tapered start followed by an abrupt held endpoint creates a real repeating seam.
        broken = tone[:]
        for i in range(200): broken[i] *= i / 200
        for i in range(n - 200, n): broken[i] = 0.3
        rep = ar.analyze(ar.parse_wav(wav_bytes([broken, [-x for x in broken]])), loop=True)
        self.assertIn("loop seam click", rep["issue_codes"])
        self.assertTrue(rep["loop"]["scan_available"])

    def test_silence(self):
        rep = report_for([[0.0] * (RATE // 4)])
        self.assertEqual(rep["issue_codes"], ["silent"])
        self.assertIsNone(rep["loudness_lufs_approx"])
        self.assertIsNone(rep["peak_dbfs"])
        self.assertIsNone(rep["spectrum"]["dominant_hz"])
        self.assertEqual(rep["silence"]["leading_ms"], 250.0)
        self.assertEqual(rep["clicks"]["count"], 0)
        quiet = report_for([[0.0004 * math.sin(i * 0.05) for i in range(RATE // 8)]])
        self.assertEqual(quiet["issue_codes"], ["silent"])

    def test_too_quiet_and_too_loud(self):
        quiet = report_for([sine(440, 0.02, 0.3)])  # -34 dBFS peak
        self.assertEqual(quiet["issue_codes"], ["too quiet"])
        self.assertIn("-34.0 dBFS", quiet["issues"][0])
        square = [0.98 if (i // 60) % 2 == 0 else -0.98 for i in range(RATE // 5)]
        env = fade_env(len(square))
        loud = report_for([[v * env[i] for i, v in enumerate(square)]])
        self.assertIn("too loud", loud["issue_codes"])
        self.assertNotIn("too loud", report_for([sine(440, 0.5, 0.3)])["issue_codes"])

    def test_leading_trailing_and_internal_silence(self):
        tone = sine(440, 0.5, 0.15)
        gap = [0.0] * int(RATE * 0.05)
        chans = [[0.0] * int(RATE * 0.1) + tone + gap + tone + [0.0] * int(RATE * 0.2)]
        rep = report_for(chans)
        silence = rep["silence"]
        self.assertAlmostEqual(silence["leading_ms"], 100.0, delta=10.0)
        self.assertAlmostEqual(silence["trailing_ms"], 200.0, delta=10.0)
        self.assertAlmostEqual(silence["longest_internal_ms"], 50.0, delta=20.0)
        # the fades put the audible part a little inside the tone edges, so allow that above
        self.assertEqual(rep["issue_codes"], [])


class ClickTests(unittest.TestCase):
    @staticmethod
    def with_step(at, amount, secs=0.4, amp=0.5):
        tone = sine(440, amp, secs)
        return tone[:at] + [v + amount for v in tone[at:]]

    def test_step_phase_jump_and_pop_are_found_at_the_right_place(self):
        at = RATE // 4
        n = int(RATE * 0.4)
        env = fade_env(n)
        for label, signal in (
            ("step", self.with_step(at, 0.3)),
            ("pop", [v + (0.6 if i == at else 0.0) for i, v in enumerate(sine(440, 0.5, 0.4))]),
            ("phase", [0.5 * env[i] * math.sin(2 * math.pi * 440 * i / RATE + (1.2 if i >= at else 0.0)) for i in range(n)]),
        ):
            with self.subTest(label):
                rep = report_for([signal])
                clicks = rep["clicks"]
                self.assertEqual(clicks["count"], 1)
                self.assertLessEqual(abs(clicks["first"][0]["sample"] - at), 2)
                self.assertGreater(clicks["first"][0]["jump"], 0.25)
                self.assertIn("clicks", rep["issue_codes"])
                self.assertAlmostEqual(clicks["first"][0]["ms"], at / RATE * 1000.0, delta=0.1)

    def test_small_steps_below_the_limit_are_ignored(self):
        self.assertEqual(report_for([self.with_step(RATE // 4, 0.01)])["clicks"]["count"], 0)
        self.assertEqual(report_for([self.with_step(RATE // 4, 0.05)])["clicks"]["count"], 1)

    def test_dropouts_count_once(self):
        tone = array.array("d", sine(440, 0.4, 0.3))
        at = 5000
        for length in (1, 5, 20, 40):
            damaged = array.array("d", tone)
            for i in range(at, at + length):
                damaged[i] = 0.0
            self.assertEqual(len(ar._click_events(damaged)), 1, length)
        long_gap = array.array("d", tone)
        for i in range(at, at + 200):
            long_gap[i] = 0.0
        self.assertEqual(len(ar._click_events(long_gap)), 2)  # both edges
        rep = report_for([list(tone[:at]) + [0.0] * 5 + list(tone[at + 5 :])])  # and through the report
        self.assertEqual(rep["clicks"]["count"], 1)
        self.assertIn("clicks", rep["issue_codes"])

    def test_steady_signals_have_no_clicks(self):
        rng = random.Random(4)
        n = int(RATE * 0.3)
        signals = {
            "sine": sine(440, 0.5, 0.3),
            "high sine": sine(12000, 0.5, 0.3),
            "white noise": [rng.uniform(-0.3, 0.3) for _ in range(n)],
            "gaussian noise": [rng.gauss(0, 0.1) for _ in range(n)],
            "square 220": [0.5 if math.sin(2 * math.pi * 220 * i / RATE) >= 0 else -0.5 for i in range(n)],
            "saw 100": [0.6 * (2 * ((100 * i / RATE) % 1.0) - 1) for i in range(n)],
            "square 3 kHz": [0.5 if math.sin(2 * math.pi * 3000 * i / RATE) >= 0 else -0.5 for i in range(n)],
            "8-bit sine": [round(0.3 * math.sin(2 * math.pi * 300 * i / RATE) * 127) / 127 for i in range(n)],
        }
        for name, signal in signals.items():
            with self.subTest(name):
                count = report_for([signal])["clicks"]["count"]
                self.assertEqual(count, 0, name)

    def test_isolated_edges_far_apart_are_clicks(self):
        n = int(RATE * 0.6)  # a 5 Hz square wave: edges 100 ms apart
        square = [0.5 if math.sin(2 * math.pi * 5 * i / RATE) >= 0 else -0.5 for i in range(n)]
        self.assertGreaterEqual(report_for([square])["clicks"]["count"], 5)

    def test_clicks_in_either_stereo_channel_are_found_once(self):
        left, right = sine(440, 0.5, 0.4), sine(660, 0.5, 0.4)
        right = right[:12000] + [v + 0.4 for v in right[12000:]]
        rep = report_for([left, right])
        self.assertEqual(rep["clicks"]["count"], 1)
        both = report_for([self.with_step(12000, 0.4, secs=0.4), right])
        self.assertEqual(both["clicks"]["count"], 1)  # the same place in both channels is one click

    def test_clicks_around_a_block_boundary(self):
        old = ar.CLICK_BLOCK
        ar.CLICK_BLOCK = 4096
        try:
            n = 14000
            env = fade_env(n)
            base = [0.4 * env[i] * math.sin(2 * math.pi * 440 * i / RATE) for i in range(n)]
            cases = ((4090,), (4096,), (4095,), (4097,), (4090, 8192), (4000, 4100, 8000, 8300, 12288))
            for positions in cases:
                x = array.array("d", base)
                sign = 1
                for p in positions:
                    for i in range(p, n):
                        x[i] += 0.25 * sign
                    sign = -sign
                found = [p for p, _ in ar._click_events(x)]
                self.assertEqual(len(found), len(positions), positions)
                for want, got in zip(positions, found):
                    self.assertLessEqual(abs(want - got), 2)
        finally:
            ar.CLICK_BLOCK = old


class CommandLineTests(TempDirCase):
    @classmethod
    def setUpClass(cls):
        super().setUpClass()

        def make(name, chans):
            path = cls.root / name
            path.write_bytes(wav_bytes(chans))
            return str(path)

        cls.tone = make("tone.wav", [sine(440, 0.5, 0.15)])
        cls.tone2 = make("tone2.wav", [sine(880, 0.5, 0.15)])
        cls.long_tone = make("long_tone.wav", [sine(440, 0.5, 0.6)])
        cls.square = make("square.wav", [[1.0 if (i // 100) % 2 == 0 else -1.0 for i in range(8000)]])
        cls.silent = make("silent.wav", [[0.0] * 4000])
        cls.tiny = make("tiny.wav", [[0.5 * math.sin(i) for i in range(30)]])
        junk = cls.root / "junk.wav"
        junk.write_bytes(b"not a wav")
        cls.junk = str(junk)

    def test_json_output_structure(self):
        code, stdout, _ = run_main(ar, self.tone, "--json")
        self.assertEqual(code, 0)
        payload = json.loads(stdout)
        self.assertEqual(payload["errors"], [])
        rep = payload["files"][0]
        for key in ("path", "format", "sample_rate", "channels", "frames", "duration_ms", "peak_dbfs", "rms_dbfs", "crest_db",
                    "loudness_lufs_approx", "dc_offset", "clipped_samples", "silence", "clicks", "spectrum", "issues"):
            self.assertIn(key, rep)
        self.assertEqual(set(rep["spectrum"]["band_energy_pct"]), {"sub", "bass", "low_mid", "high_mid", "air"})
        self.assertEqual(set(rep["silence"]), {"threshold_dbfs", "leading_ms", "trailing_ms", "longest_internal_ms"})

    def test_text_report_and_summary_table(self):
        code, stdout, _ = run_main(ar, self.tone, self.silent)
        self.assertEqual(code, 0)
        self.assertIn("tone.wav", stdout)
        self.assertIn("issues     none", stdout)
        self.assertIn("issues     silent", stdout)
        self.assertIn("LUFS", stdout)
        self.assertEqual(run_main(ar, self.tone)[1].count("tone.wav"), 1)  # a single file has no summary table

    def test_fail_on_issues_and_exit_codes(self):
        clean, square, junk = self.tone, self.square, self.junk
        self.assertEqual(run_main(ar, clean, "--fail-on-issues")[0], 0)
        self.assertEqual(run_main(ar, square)[0], 0)
        self.assertEqual(run_main(ar, square, "--fail-on-issues")[0], 1)
        self.assertEqual(run_main(ar, clean, square, "--fail-on-issues", "--json")[0], 1)
        code, stdout, stderr = run_main(ar, clean, junk, "--json")
        self.assertEqual(code, 2)
        payload = json.loads(stdout)
        self.assertEqual(len(payload["files"]), 1)
        self.assertEqual(payload["errors"][0]["path"], junk.replace(os.sep, "/"))
        self.assertIn("junk.wav", stderr)
        self.assertEqual(run_main(ar, str(self.root / "missing.wav"))[0], 2)
        self.assertEqual(run_main(ar, junk, "--fail-on-issues")[0], 2)

    def test_spectrogram_shows_the_tone_at_the_right_row(self):
        png = self.dir / "tone.png"
        code, stdout, stderr = run_main(ar, self.long_tone, "--spectrogram", png)
        self.assertEqual(code, 0, stderr)
        self.assertIn("spectrogram written", stdout)
        width, height, rgba = cs.read_png(png)
        self.assertLessEqual(width, 720)
        self.assertEqual(height, ar.SPEC_MT + ar.SPEC_PH + ar.SPEC_MB)
        x0, y0 = ar.SPEC_ML, ar.SPEC_MT
        brightness = []
        for row in range(ar.SPEC_PH):
            total = 0
            for x in range(x0 + 40, x0 + ar.SPEC_PW - 40, 4):
                r, g, b, _ = pixel(rgba, width, x, y0 + row)
                total += r + g + b
            brightness.append(total)
        peak_row = max(range(ar.SPEC_PH), key=brightness.__getitem__)
        self.assertLessEqual(abs(peak_row - ar.spectrogram_row(440)), 3, (peak_row, ar.spectrogram_row(440)))
        # frequency rows: 100 Hz is lower on the picture than 440 Hz and 5 kHz higher
        self.assertGreater(ar.spectrogram_row(100), ar.spectrogram_row(440))
        self.assertLess(ar.spectrogram_row(5000), ar.spectrogram_row(440))
        # much darker away from the tone
        self.assertLess(brightness[ar.spectrogram_row(5000)] * 5, brightness[peak_row])
        self.assertEqual(ar.spectrogram_row(40), ar.SPEC_PH - 1)
        self.assertEqual(ar.spectrogram_row(18000), 0)

    def test_spectrogram_rules(self):
        a, b = self.tone, self.tone2
        png = self.dir / "s.png"
        code, _, stderr = run_main(ar, a, b, "--spectrogram", png)
        self.assertEqual(code, 2)
        self.assertFalse(png.exists())
        self.assertEqual(run_main(ar, a, "--spectrogram", png)[0], 0)
        before = png.read_bytes()
        code, _, stderr = run_main(ar, a, "--spectrogram", png)
        self.assertEqual(code, 2)
        self.assertIn("already exists", stderr)
        self.assertEqual(png.read_bytes(), before)
        self.assertEqual(run_main(ar, b, "--spectrogram", png, "--replace")[0], 0)
        self.assertNotEqual(png.read_bytes(), before)
        self.assertEqual([n for n in os.listdir(self.dir) if ".tmp" in n], [])

    def test_spectrogram_accepts_a_path_object_and_creates_directories(self):
        out = self.dir / "made" / "here" / "tone.png"
        rep = ar.analyze(self.tone, spectrogram=out)
        self.assertTrue(out.is_file())
        self.assertTrue(rep["spectrogram"].endswith("made/here/tone.png"))
        self.assertTrue(Path(self.tone).is_file(), "the input must be untouched")
        with self.assertRaises(FileExistsError):
            ar.analyze(self.tone2, spectrogram=out)
        self.assertTrue(out.is_file())

    def test_spectrogram_of_silence_and_of_a_tiny_file(self):
        self.assertEqual(run_main(ar, self.silent, "--spectrogram", self.dir / "silent.png")[0], 0)
        self.assertEqual(run_main(ar, self.tiny, "--spectrogram", self.dir / "tiny.png")[0], 0)
        self.assertEqual(cs.read_png(self.dir / "tiny.png")[0], 720)

    def test_command_line_end_to_end(self):
        code, stdout, stderr = run_script("audio_report.py", self.tone, "--json")
        self.assertEqual(code, 0, stderr)
        self.assertEqual(json.loads(stdout)["files"][0]["frames"], int(RATE * 0.15))
        code, stdout, stderr = run_script("audio_report.py", str(self.root / "nope.wav"))
        self.assertEqual(code, 2)
        self.assertIn("nope.wav", stderr)
        self.assertEqual(run_script("audio_report.py", "--help")[0], 0)


class TimingTests(unittest.TestCase):
    def test_sixty_seconds_at_44100_hz_analyse_quickly(self):
        one_second = array.array("h", (int(9000 * math.sin(2 * math.pi * 440 * i / RATE) + 3000 * math.sin(2 * math.pi * 1250 * i / RATE)) for i in range(RATE)))
        data = (one_second * 60).tobytes()
        fmt = struct.pack("<HHIIHH", 1, 1, RATE, RATE * 2, 2, 16)
        body = b"WAVE" + b"fmt " + struct.pack("<I", 16) + fmt + b"data" + struct.pack("<I", len(data)) + data
        blob = b"RIFF" + struct.pack("<I", len(body)) + body  # in memory: no 5 MB file to write and scan
        started = time.process_time()  # CPU time: a busy machine must not fail the test
        rep = ar.analyze(ar.parse_wav(blob))
        elapsed = time.process_time() - started
        self.assertLess(elapsed, 30.0, "60 s of audio took %.1f s of CPU" % elapsed)  # about 3 s normally
        self.assertEqual(rep["duration_ms"], 60000.0)
        self.assertEqual(rep["spectrum"]["frames"], ar.MAX_FRAMES)
        self.assertAlmostEqual(rep["spectrum"]["dominant_hz"], 440, delta=15)
        self.assertEqual(rep["clicks"]["count"], 0)
        self.assertNotIn("clipping", rep["issue_codes"])


if __name__ == "__main__":
    unittest.main()
