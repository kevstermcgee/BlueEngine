#!/usr/bin/env python3
"""Contact sheet: many screenshots -> ONE labelled PNG, with blank-frame detection.

An AI agent verifies a game through screenshots, and every image it looks at is re-read (and
paid for) on each later turn.  This tool packs N screenshots into one downscaled, captioned PNG
and reports what the frames really are, in numbers, so a wrong capture size or a blank frame is
visible without opening every image.  Standard library only (Python 3.10+): the PNG codec, the
box-filter resize and the 5x7 caption font are built in.

    python tools/contact_sheet.py OUT.png IMAGE.png [IMAGE.png ...] [--dir DIR] [--cols N]
        [--cell W|WxH] [--gap N] [--bg RRGGBB] [--no-labels] [--replace] [--json]

* Inputs are explicit PNG paths (kept in the order given; a pattern such as shots/*.png is
  expanded here, so it also works in shells that do not glob) and/or every *.png in --dir,
  natural-sorted (shot_2 before shot_10).  OUT itself is never used as an input.
* OUT is a new file: an existing OUT is refused (exit 2) unless --replace is given.
* --cell W or WxH sets the size of one image cell; the default is 320 wide with the height
  taken from the first image's aspect ratio.  Images keep their aspect ratio (letterboxed in
  the cell), are reduced with an exact area-average filter, and are never blurred up: a small
  image is shown at native size or an integer multiple of it.
* --cols defaults to ceil(sqrt(n)) capped at 4.  --gap is the spacing and margin in pixels
  (default 8).  --bg is the background colour (default 202020); transparent pixels are
  composited over it.  Every cell gets a caption: file stem, then the ORIGINAL size WxH.
  A cell whose frame is flat is tagged BLANK and gets a pink caption strip.
* stdout is one line of compact JSON (--json is accepted for symmetry with audio_report.py):
    {"output","width","height","cols","rows","cell":{"width","height"},"labels","uniform_count",
     "images":[{"path","width","height","uniform","luma_stddev"}], "warnings":[...]}
  "width"/"height" of an image are its ORIGINAL pixel size; "uniform" is true when the luma
  standard deviation (0..255 scale, full resolution) is below 2.0, i.e. a blank/flat frame.
  Uniform frames do not change the exit status.
* Exit status: 0 success, 2 unreadable input, unusable arguments or refused overwrite.

Importable helpers (for other tools):
    read_png(path) -> (width, height, rgba_bytes)      8-bit RGBA, any colour type/bit depth
    decode_png(data) -> (width, height, rgba_bytes)    the same from bytes
    write_png(path, width, height, pixels, channels)   channels 1, 2, 3 or 4; refuses to
                                                       overwrite unless replace=True
    png_bytes(width, height, pixels, channels) -> bytes
    frame_stats(path_or_png_bytes) -> {"width","height","uniform","luma_mean","luma_stddev"}
    make_sheet(out, images, ...) -> the JSON summary as a dict (writes OUT)
    render_sheet([(name, path_or_png_bytes), ...], ...) -> (summary, png_bytes), no files involved

PNG support: colour types 0, 2, 3, 4, 6; bit depths 1/2/4/8/16 as allowed by the type; all five
row filters; Adam7 interlacing.  16-bit samples keep their high byte; tRNS is ignored (palette
images are opaque); ancillary chunks are skipped.  Corrupt or truncated files raise PngError.
"""

from __future__ import annotations

import argparse
import array
import glob
import json
import math
import os
import re
import struct
import sys
import zlib
from itertools import accumulate
from operator import itemgetter, mul

__all__ = [
    "PngError",
    "decode_png",
    "read_png",
    "png_bytes",
    "write_png",
    "frame_stats",
    "make_sheet",
    "main",
]

UNIFORM_STDDEV = 2.0
LABEL_SCALE = 2
LABEL_PAD = 3
LABEL_LINE = 7 * LABEL_SCALE
LABEL_H = 2 * LABEL_PAD + 2 * LABEL_LINE + 2  # two caption lines
DEFAULT_BG = (0x20, 0x20, 0x20)
STRIP_RGB = (232, 232, 232)
STRIP_BLANK_RGB = (255, 196, 184)
TEXT_RGB = (16, 16, 16)
OUTLINE_RGB = (84, 84, 84)
MAX_SIDE = 16384
MAX_SHEET_PIXELS = 100_000_000
MAX_IMAGE_PIXELS = 1 << 27

_SIG = b"\x89PNG\r\n\x1a\n"
_CHANNELS = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}
_DEPTHS = {0: (1, 2, 4, 8, 16), 2: (8, 16), 3: (1, 2, 4, 8), 4: (8, 16), 6: (8, 16)}
_MODES = {0: "L", 2: "RGB", 3: "RGB", 4: "LA", 6: "RGBA"}
# Adam7 passes: x start, y start, x step, y step
_ADAM7 = (
    (0, 0, 8, 8),
    (4, 0, 8, 8),
    (0, 4, 4, 8),
    (2, 0, 4, 4),
    (0, 2, 2, 4),
    (1, 0, 2, 2),
    (0, 1, 1, 2),
)
_AND255 = (255).__and__
_U32 = "I" if array.array("I").itemsize == 4 else "L"


def _sumprod_fallback(a, b):
    return sum(map(mul, a, b))


_sumprod = getattr(math, "sumprod", _sumprod_fallback)  # math.sumprod exists from Python 3.12


class PngError(ValueError):
    """The data is not a PNG this module can decode (corrupt, truncated or unsupported)."""


class ToolError(Exception):
    """A usage or input problem reported by the command line tool (exit status 2)."""


# --------------------------------------------------------------------------------------
# PNG decoding
# --------------------------------------------------------------------------------------


def _parse_png(data):
    """Return ((w, h, depth, ctype, interlace), palette_bytes_or_None, [idat_parts])."""
    if bytes(data[:8]) != _SIG:
        raise PngError("not a PNG file (bad signature)")
    view = memoryview(data)
    pos, size = 8, len(data)
    ihdr = plte = None
    idat = []
    while pos + 8 <= size:
        length = struct.unpack_from(">I", data, pos)[0]
        ctype = bytes(data[pos + 4 : pos + 8])
        start = pos + 8
        end = start + length
        if end + 4 > size:
            if ihdr is None or ctype in (b"IHDR", b"PLTE", b"IDAT"):
                raise PngError("truncated PNG: %s chunk is cut off" % ctype.decode("latin-1"))
            break
        if not ctype[0] & 0x20:  # critical chunk: verify its checksum
            crc = struct.unpack_from(">I", data, end)[0]
            if zlib.crc32(view[start:end], zlib.crc32(ctype)) != crc:
                raise PngError("corrupt PNG: CRC mismatch in %s chunk" % ctype.decode("latin-1"))
        if ihdr is None:
            if ctype != b"IHDR":
                raise PngError("corrupt PNG: first chunk is not IHDR")
            if length != 13:
                raise PngError("corrupt PNG: bad IHDR length")
            ihdr = struct.unpack(">IIBBBBB", bytes(view[start:end]))
        elif ctype == b"IHDR":
            raise PngError("corrupt PNG: duplicate IHDR chunk")
        elif ctype == b"PLTE":
            plte = bytes(view[start:end])
        elif ctype == b"IDAT":
            idat.append(view[start:end])
        elif ctype == b"IEND":
            break
        pos = end + 4
    if ihdr is None:
        raise PngError("corrupt PNG: no IHDR chunk")
    w, h, depth, ctype, comp, filt, interlace = ihdr
    if w < 1 or h < 1:
        raise PngError("corrupt PNG: zero image size")
    if ctype not in _CHANNELS:
        raise PngError("unsupported PNG colour type %d" % ctype)
    if depth not in _DEPTHS[ctype]:
        raise PngError("unsupported PNG bit depth %d for colour type %d" % (depth, ctype))
    if comp != 0 or filt != 0:
        raise PngError("unsupported PNG compression/filter method")
    if interlace not in (0, 1):
        raise PngError("unsupported PNG interlace method %d" % interlace)
    if w * h > MAX_IMAGE_PIXELS:
        raise PngError("PNG is too large (%dx%d)" % (w, h))
    if ctype == 3:
        if plte is None:
            raise PngError("corrupt PNG: palette image without a PLTE chunk")
        if len(plte) % 3 or not 3 <= len(plte) <= 768:
            raise PngError("corrupt PNG: bad PLTE chunk length")
    return (w, h, depth, ctype, interlace), plte, idat


def _inflate(parts, expected):
    stream = b"".join(parts)
    if not stream:
        raise PngError("corrupt PNG: no image data (IDAT)")
    try:
        # Bounded output: a header that lies about the size cannot balloon memory.
        raw = zlib.decompressobj().decompress(stream, expected + 1)
    except zlib.error as exc:
        raise PngError("corrupt PNG: image data is not valid zlib (%s)" % exc) from None
    if len(raw) < expected:
        raise PngError("truncated PNG: image data has %d of %d bytes" % (len(raw), expected))
    return raw


_SWAR_CACHE = {}


def _swar(n):
    masks = _SWAR_CACHE.get(n)
    if masks is None:
        masks = (
            int.from_bytes(b"\x7f" * n, "little"),
            int.from_bytes(b"\x80" * n, "little"),
            (1 << (8 * n)) - 1,
        )
        if len(_SWAR_CACHE) > 8:
            _SWAR_CACHE.clear()
        _SWAR_CACHE[n] = masks
    return masks


def _unsub(line, bpp):
    out = bytearray(len(line))
    for c in range(min(bpp, len(line))):
        out[c::bpp] = bytes(map(_AND255, accumulate(line[c::bpp])))
    return bytes(out)


def _unup(line, prior):
    n = len(line)
    m7, m8, _ = _swar(n)
    a = int.from_bytes(line, "little")
    b = int.from_bytes(prior, "little")
    return (((a & m7) + (b & m7)) ^ ((a ^ b) & m8)).to_bytes(n, "little")


def _unavg(line, prior, bpp):
    out = bytearray(len(line))
    for c in range(min(bpp, len(line))):
        left = 0
        res = []
        push = res.append
        for x, up in zip(line[c::bpp], prior[c::bpp]):
            left = (x + ((left + up) >> 1)) & 255
            push(left)
        out[c::bpp] = bytes(res)
    return bytes(out)


def _unpaeth(line, prior, bpp):
    out = bytearray(len(line))
    for c in range(min(bpp, len(line))):
        left = 0
        upleft = 0
        res = []
        push = res.append
        for x, up in zip(line[c::bpp], prior[c::bpp]):
            pa = up - upleft
            pb = left - upleft
            pc = pa + pb
            if pa < 0:
                pa = -pa
            if pb < 0:
                pb = -pb
            if pc < 0:
                pc = -pc
            if pa <= pb and pa <= pc:
                left = (x + left) & 255
            elif pb <= pc:
                left = (x + up) & 255
            else:
                left = (x + upleft) & 255
            push(left)
            upleft = up
        out[c::bpp] = bytes(res)
    return bytes(out)


_UNPACK = {}


def _unpack_table(depth, gray):
    """256 entries: the samples of a packed byte, one byte each (grays scaled to 0..255)."""
    key = (depth, gray)
    table = _UNPACK.get(key)
    if table is None:
        per = 8 // depth
        mask = (1 << depth) - 1
        scale = 255 // mask if gray else 1
        table = [
            bytes(((b >> (8 - depth * (i + 1))) & mask) * scale for i in range(per))
            for b in range(256)
        ]
        _UNPACK[key] = table
    return table


def _decode_pass(raw, pos, pw, ph, channels, depth, gray):
    """Unfilter a (sub)image; return its rows (one byte per sample below 8 bits) and new pos."""
    bitspp = channels * depth
    rowbytes = (pw * bitspp + 7) // 8
    bpp = max(1, bitspp // 8)
    table = _unpack_table(depth, gray) if depth < 8 else None
    nsamples = pw * channels
    prior = bytes(rowbytes)
    rows = []
    for y in range(ph):
        ftype = raw[pos]
        line = raw[pos + 1 : pos + 1 + rowbytes]
        pos += 1 + rowbytes
        if ftype == 0:
            cur = line
        elif ftype == 1:
            cur = _unsub(line, bpp)
        elif ftype == 2:
            cur = _unup(line, prior)
        elif ftype == 3:
            cur = _unavg(line, prior, bpp)
        elif ftype == 4:
            cur = _unpaeth(line, prior, bpp)
        else:
            raise PngError("corrupt PNG: bad filter type %d on row %d" % (ftype, y))
        prior = cur
        if table is not None:
            cur = b"".join(map(table.__getitem__, cur))[:nsamples]
        rows.append(cur)
    return rows, pos


def _expand_palette(rows, plte):
    n = len(plte) // 3
    tables = [bytes(plte[c::3]) + bytes(256 - n) for c in range(3)]
    out = []
    for row in rows:
        if row and max(row) >= n:
            raise PngError("corrupt PNG: palette index out of range")
        rgb = bytearray(3 * len(row))
        for c in range(3):
            rgb[c::3] = row.translate(tables[c])
        out.append(bytes(rgb))
    return out


def _decode(data):
    """Decode PNG bytes to (width, height, mode, rows): 8-bit interleaved rows, mode L/LA/RGB/RGBA."""
    (w, h, depth, ctype, interlace), plte, idat = _parse_png(data)
    channels = _CHANNELS[ctype]
    gray = ctype in (0, 4)
    if interlace:
        passes = []
        for xs, ys, dx, dy in _ADAM7:
            pw = (w - xs + dx - 1) // dx
            ph = (h - ys + dy - 1) // dy
            if pw > 0 and ph > 0:
                passes.append((xs, ys, dx, dy, pw, ph))
    else:
        passes = [(0, 0, 1, 1, w, h)]
    expected = sum(ph * (1 + (pw * channels * depth + 7) // 8) for _, _, _, _, pw, ph in passes)
    raw = _inflate(idat, expected)
    if not interlace:
        rows, _ = _decode_pass(raw, 0, w, h, channels, depth, gray)
    else:
        px = channels * 2 if depth == 16 else channels  # bytes per pixel after unpacking
        canvas = [bytearray(w * px) for _ in range(h)]
        pos = 0
        for xs, ys, dx, dy, pw, ph in passes:
            sub, pos = _decode_pass(raw, pos, pw, ph, channels, depth, gray)
            for j, srow in enumerate(sub):
                dest = canvas[ys + j * dy]
                for c in range(px):
                    dest[xs * px + c :: dx * px] = srow[c::px]
        rows = [bytes(r) for r in canvas]
    if depth == 16:
        rows = [r[0::2] for r in rows]  # keep the high byte of each sample
    if ctype == 3:
        rows = _expand_palette(rows, plte)
    return w, h, _MODES[ctype], rows


def _rows_to_rgba(mode, rows, w, h):
    flat = b"".join(rows)
    if mode == "RGBA":
        return flat
    out = bytearray(b"\xff") * (4 * w * h)
    if mode == "RGB":
        out[0::4] = flat[0::3]
        out[1::4] = flat[1::3]
        out[2::4] = flat[2::3]
    elif mode == "L":
        out[0::4] = flat
        out[1::4] = flat
        out[2::4] = flat
    else:  # LA
        gray = flat[0::2]
        out[0::4] = gray
        out[1::4] = gray
        out[2::4] = gray
        out[3::4] = flat[1::2]
    return bytes(out)


def decode_png(data):
    """Decode PNG bytes; returns (width, height, rgba_bytes) with 8-bit RGBA, row by row."""
    w, h, mode, rows = _decode(data)
    return w, h, _rows_to_rgba(mode, rows, w, h)


def _read_file(path):
    with open(path, "rb") as fh:
        return fh.read()


def _is_bytes(value):
    return isinstance(value, (bytes, bytearray, memoryview))


def _source_bytes(source):
    """PNG bytes from a file path, or the bytes themselves."""
    return bytes(source) if _is_bytes(source) else _read_file(source)


def read_png(path):
    """Read a PNG file; returns (width, height, rgba_bytes).  Raises PngError or OSError."""
    return decode_png(_read_file(path))


def png_size(source):
    """(width, height) from the header only; source is a file path or the PNG bytes.

    Raises PngError or OSError.
    """
    if _is_bytes(source):
        head = bytes(source[:33])
    else:
        with open(source, "rb") as fh:
            head = fh.read(33)
    if head[:8] != _SIG:
        raise PngError("not a PNG file (bad signature)")
    if len(head) < 33 or head[12:16] != b"IHDR":
        raise PngError("corrupt PNG: no IHDR chunk")
    return struct.unpack(">II", head[16:24])


# --------------------------------------------------------------------------------------
# PNG encoding
# --------------------------------------------------------------------------------------


def _chunk(ctype, body):
    return struct.pack(">I", len(body)) + ctype + body + struct.pack(">I", zlib.crc32(body, zlib.crc32(ctype)))


def png_bytes(width, height, pixels, channels):
    """Encode 8-bit pixels (1, 2, 3 or 4 channels: gray, gray+alpha, RGB, RGBA) as PNG bytes.

    Every row uses filter 1 (Sub); the data is compressed with zlib level 6.
    """
    if channels not in (1, 2, 3, 4):
        raise ValueError("channels must be 1, 2, 3 or 4")
    if width < 1 or height < 1:
        raise ValueError("image size must be positive")
    stride = width * channels
    if len(pixels) != stride * height:
        raise ValueError("pixel buffer has %d bytes, expected %d" % (len(pixels), stride * height))
    m7, m8, full = _swar(stride)
    view = memoryview(pixels)
    shift = 8 * channels
    body = bytearray()
    for y in range(height):
        a = int.from_bytes(view[y * stride : (y + 1) * stride], "little")
        b = (a << shift) & full  # the pixel to the left, lane by lane
        diff = ((a | m8) - (b & m7)) ^ ((a ^ b ^ m8) & m8)  # (a - b) mod 256 in every byte
        body += b"\x01" + diff.to_bytes(stride, "little")
    ctype = {1: 0, 2: 4, 3: 2, 4: 6}[channels]
    return (
        _SIG
        + _chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, ctype, 0, 0, 0))
        + _chunk(b"IDAT", zlib.compress(bytes(body), 6))
        + _chunk(b"IEND", b"")
    )


def write_png(path, width, height, pixels, channels, replace=False):
    """Write a PNG file.  An existing file is refused (FileExistsError) unless replace=True.

    With replace=True the data goes to a temporary file first, so a failure keeps the old file.
    """
    _write_file(path, png_bytes(width, height, pixels, channels), replace)


def _write_file(path, data, replace):
    """Write bytes as a new file (FileExistsError if it exists), or over an old one via a temp file."""
    path = os.fspath(path)
    target = path if not replace else "%s.tmp%d" % (path, os.getpid())
    fh = open(target, "wb" if replace else "xb")  # FileExistsError leaves the existing file alone
    try:
        with fh:
            fh.write(data)
        if replace:
            os.replace(target, path)
    except BaseException:
        _remove_quietly(target)  # only ever a file this call created
        raise


def _remove_quietly(path):
    try:
        os.remove(path)
    except OSError:
        pass


# --------------------------------------------------------------------------------------
# Image operations
# --------------------------------------------------------------------------------------


def _blend(channel, alpha, bgc):
    """Composite one channel over a constant background value."""
    return bytes([(c * a + bgc * (255 - a) + 127) // 255 for c, a in zip(channel, alpha)])


def _to_rgb_rows(mode, rows, w, bg):
    """RGB rows (bytes of 3*w) from decoded rows; alpha is composited over the bg colour."""
    if mode == "RGB":
        return rows
    out = []
    solid_bg = bytes(bg) * w
    if mode == "L":
        for row in rows:
            rgb = bytearray(3 * w)
            rgb[0::3] = row
            rgb[1::3] = row
            rgb[2::3] = row
            out.append(bytes(rgb))
        return out
    for row in rows:
        if mode == "RGBA":
            chans = (row[0::4], row[1::4], row[2::4])
            alpha = row[3::4]
        else:  # LA
            gray = row[0::2]
            chans = (gray, gray, gray)
            alpha = row[1::2]
        if alpha.count(255) != w:  # not fully opaque: composite over the background colour
            if alpha.count(0) == w:
                out.append(solid_bg)
                continue
            chans = tuple(_blend(chans[c], alpha, bg[c]) for c in range(3))
        rgb = bytearray(3 * w)
        rgb[0::3] = chans[0]
        rgb[1::3] = chans[1]
        rgb[2::3] = chans[2]
        out.append(bytes(rgb))
    return out


def _luma_stats(rgb_rows, w):
    """Mean and standard deviation of luma (0..255 scale) over RGB rows, at full resolution."""
    total = s1 = s2 = 0
    for row in rgb_rows:
        lanes = []
        for c in range(3):  # 16-bit lanes: 3 weighted channels sum without carrying over
            buf = bytearray(2 * w)
            buf[0::2] = row[c::3]
            lanes.append(int.from_bytes(buf, "little"))
        luma = array.array("H")
        luma.frombytes((lanes[0] * 54 + lanes[1] * 183 + lanes[2] * 19).to_bytes(2 * w, "little"))
        if sys.byteorder == "big":
            luma.byteswap()
        s1 += sum(luma)
        s2 += _sumprod(luma, luma)
        total += w
    mean = s1 / total
    var = max(0.0, s2 / total - mean * mean)
    return mean / 256.0, math.sqrt(var) / 256.0


def frame_stats(source, bg=DEFAULT_BG):
    """Statistics of one PNG (a file path, or the PNG bytes): original size, luma mean and
    standard deviation on the 0..255 scale, and the blank-frame flag (stddev < 2)."""
    w, h, mode, rows = _decode(_source_bytes(source))
    mean, sd = _luma_stats(_to_rgb_rows(mode, rows, w, bg), w)
    return {
        "width": w,
        "height": h,
        "uniform": sd < UNIFORM_STDDEV,
        "luma_mean": round(mean, 3),
        "luma_stddev": round(sd, 3),
    }


def _lane_int(row):
    """The bytes of a row spread into 32-bit little-endian lanes, as one big integer."""
    buf = bytearray(4 * len(row))
    buf[0::4] = row
    return int.from_bytes(buf, "little")


def _area_resize(rows, w, h, tw, th):
    """Exact area-average reduction of RGB rows from w x h to tw x th (tw <= w, th <= h).

    All arithmetic is integer: a target pixel averages every covered source pixel weighted by
    the overlapped area, so a 1920 px wide frame reduced to 320 px does not alias.
    """
    n = 3 * w
    getq = itemgetter(*[(j * w) // tw for j in range(tw + 1)])
    rem = [(j * w) % tw for j in range(tw + 1)]
    total = w * h
    half = total // 2
    out = []
    for i in range(th):
        lo, hi = i * h, (i + 1) * h
        acc = 0
        for k in range(lo // th, (hi - 1) // th + 1):
            overlap = min(hi, (k + 1) * th) - max(lo, k * th)
            acc += overlap * _lane_int(rows[k])
        vals = array.array(_U32)
        vals.frombytes(acc.to_bytes(4 * n, "little"))
        if sys.byteorder == "big":
            vals.byteswap()
        line = bytearray(3 * tw)
        for c in range(3):
            v = vals[c::3]
            v.append(0)  # v[w] exists for the final boundary (its weight is zero)
            prefix = list(accumulate(v, initial=0))
            edge = [tw * p + r * x for p, r, x in zip(getq(prefix), rem, getq(v))]
            line[c::3] = bytes([(b - a + half) // total for a, b in zip(edge, edge[1:])])
        out.append(bytes(line))
    return out


def _upscale(rows, w, k):
    """Integer pixel replication (crisp) of RGB rows by k."""
    out = []
    for row in rows:
        big = bytearray(3 * w * k)
        for j in range(k):
            for c in range(3):
                big[3 * j + c :: 3 * k] = row[c::3]
        big = bytes(big)
        out.extend([big] * k)
    return out


def _fit_size(w, h, cw, ch):
    """(mode, tw, th, k): how an image of w x h is placed in a cw x ch cell.

    mode "up": w <= cw and h <= ch, shown at k times its size (k = 1 is native);
    mode "down": reduced to tw x th, keeping the aspect ratio (k is 1).
    """
    if w <= cw and h <= ch:
        k = min(cw // w, ch // h)
        return "up", w * k, h * k, k
    if cw * h <= ch * w:  # limited by the cell width
        tw, th = cw, (2 * h * cw + w) // (2 * w)
    else:
        th, tw = ch, (2 * w * ch + h) // (2 * h)
    return "down", max(1, min(tw, cw, w)), max(1, min(th, ch, h)), 1


def _thumbnail(rows, w, h, cw, ch):
    kind, tw, th, k = _fit_size(w, h, cw, ch)
    if kind == "up":
        return tw, th, rows if k == 1 else _upscale(rows, w, k)
    if (tw, th) == (w, h):
        return tw, th, rows
    return tw, th, _area_resize(rows, w, h, tw, th)


# --------------------------------------------------------------------------------------
# Caption font: 5x7 bitmap, ASCII 32..126, five column bytes per glyph, bit 0 = top row.
# --------------------------------------------------------------------------------------

_FONT = bytes.fromhex(
    "0000000000" "0000005f00" "0007000700" "147f147f14" "242a7f2a12" "2313086462" "3649552250" "0005030000"  # space ! " # $ % & '
    "001c224100" "0041221c00" "14083e0814" "08083e0808" "0050300000" "0808080808" "0060600000" "2010080402"  # ( ) * + , - . /
    "3e5149453e" "00427f4000" "4261514946" "2141454b31" "1814127f10" "2745454539" "3c4a494930" "0171090503"  # 0-7
    "3649494936" "064949291e" "0036360000" "0056360000" "0814224100" "1414141414" "0041221408" "0201510906"  # 8 9 : ; < = > ?
    "324979413e" "7e1111117e" "7f49494936" "3e41414122" "7f4141221c" "7f49494941" "7f09090901" "3e4149497a"  # @ A B C D E F G
    "7f0808087f" "00417f4100" "2040413f01" "7f08142241" "7f40404040" "7f020c027f" "7f0408107f" "3e4141413e"  # H I J K L M N O
    "7f09090906" "3e4151215e" "7f09192946" "4649494931" "01017f0101" "3f4040403f" "1f2040201f" "3f4038403f"  # P Q R S T U V W
    "6314081463" "0708700807" "6151494543" "007f414100" "0204081020" "0041417f00" "0402010204" "4040404040"  # X Y Z [ backslash ] ^ _
    "0001020400" "2054545478" "7f48444438" "3844444420" "384444487f" "3854545418" "087e090102" "081454543c"  # ` a b c d e f g
    "7f08040478" "00447d4000" "2040443d00" "7f10284400" "00417f4000" "7c04180478" "7c08040478" "3844444438"  # h i j k l m n o
    "7c14141408" "081414187c" "7c08040408" "4854545420" "043f444020" "3c4040207c" "1c2040201c" "3c4030403c"  # p q r s t u v w
    "4428102844" "0c5050503c" "4464544c44" "0008364100" "00007f0000" "0041360800" "0804081008"  # x y z { | } ~
)
assert len(_FONT) == 95 * 5


def _ascii(text):
    return "".join(c if 32 <= ord(c) <= 126 else "?" for c in text)


def text_mask(text, scale=LABEL_SCALE):
    """Render text with the built-in font: returns (width, rows) where rows are 0/1 bytearrays."""
    text = _ascii(text)
    width = max(0, len(text) * 6 * scale - scale)
    rows = [bytearray(width) for _ in range(7 * scale)]
    for i, ch in enumerate(text):
        base = (ord(ch) - 32) * 5
        for col in range(5):
            bits = _FONT[base + col]
            if not bits:
                continue
            x0 = (i * 6 + col) * scale
            for row in range(7):
                if bits >> row & 1:
                    for dy in range(scale):
                        rows[row * scale + dy][x0 : x0 + scale] = b"\x01" * scale
    return width, rows


def _fit_text(text, max_chars):
    """Shorten text to max_chars characters with a middle ellipsis (keeps the distinctive tail)."""
    text = _ascii(text)
    if len(text) <= max_chars:
        return text
    if max_chars <= 4:
        return text[: max(0, max_chars)]
    keep = max_chars - 3
    head = (keep + 1) // 2
    return text[:head] + "..." + text[len(text) - (keep - head) :]


def _blit_mask(canvas, stride, x, y, rows, rgb):
    color = bytes(rgb)
    for j, mask in enumerate(rows):
        base = ((y + j) * stride + x) * 3
        for i, v in enumerate(mask):
            if v:
                o = base + 3 * i
                canvas[o : o + 3] = color


# --------------------------------------------------------------------------------------
# Sheet layout and composition
# --------------------------------------------------------------------------------------


def natural_key(text):
    """Sort key: digit runs compare as numbers, so shot_2 sorts before shot_10."""
    parts = re.split(r"(\d+)", text.lower())  # text, number, text, number, ...: types line up
    return [int(p) if i % 2 else p for i, p in enumerate(parts)], text


def _norm(path):
    return os.path.normcase(os.path.abspath(path))


def _slash(path):
    return os.fspath(path).replace(os.sep, "/")


def _gather_inputs(images, directory, out):
    """Explicit paths first (in order, wildcards expanded), then the natural-sorted --dir files."""
    found = []
    for item in images:
        if not os.path.exists(item) and any(ch in item for ch in "*?["):
            matches = sorted(glob.glob(item), key=natural_key)
            if not matches:
                raise ToolError("no files match %s" % item)
            found.extend(matches)
        elif os.path.isdir(item):
            raise ToolError("%s is a directory (use --dir to add every PNG in it)" % item)
        else:
            found.append(item)
    if directory is not None:
        if not os.path.isdir(directory):
            raise ToolError("--dir %s is not a directory" % directory)
        names = [n for n in os.listdir(directory) if n.lower().endswith(".png")]
        given = {_norm(p) for p in found}
        for name in sorted(names, key=natural_key):
            path = os.path.join(directory, name)
            if os.path.isfile(path) and _norm(path) not in given:
                found.append(path)
    out_key = _norm(out)
    return [p for p in found if _norm(p) != out_key]


def default_cols(n):
    """Default number of columns for n images: ceil(sqrt(n)), at most 4."""
    return min(4, math.isqrt(n - 1) + 1) if n > 1 else 1


def _parse_cell(text):
    m = re.fullmatch(r"(\d+)(?:[xX](\d+))?", text.strip())
    if not m:
        raise ToolError("bad --cell %r: expected W or WxH, e.g. 320 or 320x180" % text)
    cw = int(m.group(1))
    ch = int(m.group(2)) if m.group(2) else None
    for value in (cw, ch):
        if value is not None and not 16 <= value <= 4096:
            raise ToolError("--cell sizes must be between 16 and 4096 pixels")
    return cw, ch


def _parse_bg(text):
    m = re.fullmatch(r"#?([0-9a-fA-F]{6})", text.strip())
    if not m:
        raise ToolError("bad --bg %r: expected RRGGBB, e.g. 202020" % text)
    v = int(m.group(1), 16)
    return (v >> 16) & 255, (v >> 8) & 255, v & 255


def render_sheet(items, cols=None, cell=None, gap=8, bg=DEFAULT_BG, labels=True):
    """Compose the contact sheet in memory; returns (summary, png_bytes).

    ``items`` is a list of (name, source) pairs: source is a PNG file path or the PNG bytes, and
    name is what the caption and the JSON call the image.  ``cell`` is None, a width, or a
    (width, height) tuple.  The summary is the JSON described in the module docstring, with
    "output" set to None until make_sheet fills it in.  Raises ToolError for usage problems and
    PngError/OSError for unreadable inputs.
    """
    items = list(items)
    if not items:
        raise ToolError("no input images (give PNG paths or --dir DIR)")
    if not 0 <= gap <= 256:
        raise ToolError("--gap must be between 0 and 256")
    if cols is not None and cols < 1:
        raise ToolError("--cols must be at least 1")
    cell_w, cell_h = cell if isinstance(cell, tuple) else (cell, None)
    cell_w = 320 if cell_w is None else cell_w
    sizes = []
    for name, source in items:
        try:
            sizes.append(png_size(source))
        except (PngError, OSError) as exc:
            raise ToolError("%s: %s" % (name, exc)) from None
    if cell_h is None:
        cell_h = max(1, (2 * cell_w * sizes[0][1] + sizes[0][0]) // (2 * sizes[0][0]))
    n = len(items)
    if cols is None:
        cols = default_cols(n)
    cols = min(cols, n)
    rows = -(-n // cols)
    label_h = LABEL_H if labels else 0
    sheet_w = gap + cols * (cell_w + gap)
    sheet_h = gap + rows * (cell_h + label_h + gap)
    if max(sheet_w, sheet_h) > MAX_SIDE or sheet_w * sheet_h > MAX_SHEET_PIXELS:
        raise ToolError(
            "the sheet would be %dx%d px for %d images; use fewer images, a smaller --cell or more --cols"
            % (sheet_w, sheet_h, n)
        )

    canvas = bytearray(bytes(bg) * (sheet_w * sheet_h))
    max_chars = max(1, (cell_w - 6) // (6 * LABEL_SCALE))
    entries = []
    for index, (name, source) in enumerate(items):
        try:
            w, h, mode, pix_rows = _decode(_source_bytes(source))
        except (PngError, OSError) as exc:
            raise ToolError("%s: %s" % (name, exc)) from None
        rgb = _to_rgb_rows(mode, pix_rows, w, bg)
        sd = _luma_stats(rgb, w)[1]
        uniform = sd < UNIFORM_STDDEV
        tw, th, thumb = _thumbnail(rgb, w, h, cell_w, cell_h)
        r, c = divmod(index, cols)
        x = gap + c * (cell_w + gap)
        y = gap + r * (cell_h + label_h + gap)
        if gap >= 1:  # thin outline so an all-black frame stays visible against the background
            _outline(canvas, sheet_w, x - 1, y - 1, cell_w + 2, cell_h + label_h + 2)
        ox, oy = x + (cell_w - tw) // 2, y + (cell_h - th) // 2
        for j, line in enumerate(thumb):
            off = ((oy + j) * sheet_w + ox) * 3
            canvas[off : off + 3 * tw] = line
        stem = os.path.splitext(os.path.basename(name))[0]
        if labels:
            strip = bytes(STRIP_BLANK_RGB if uniform else STRIP_RGB) * cell_w
            for j in range(label_h):
                off = ((y + cell_h + j) * sheet_w + x) * 3
                canvas[off : off + 3 * cell_w] = strip
            second = "%dx%d%s" % (w, h, " BLANK" if uniform else "")
            for text_line, line_y in (
                (stem, y + cell_h + LABEL_PAD),
                (second, y + cell_h + LABEL_PAD + LABEL_LINE + 2),
            ):
                _, mask = text_mask(_fit_text(text_line, max_chars))
                _blit_mask(canvas, sheet_w, x + 3, line_y, mask, TEXT_RGB)
        entries.append(
            {
                "path": _slash(name),
                "width": w,
                "height": h,
                "uniform": uniform,
                "luma_stddev": round(sd, 3),
            }
        )
    summary = {
        "output": None,
        "width": sheet_w,
        "height": sheet_h,
        "cols": cols,
        "rows": rows,
        "cell": {"width": cell_w, "height": cell_h},
        "labels": bool(labels),
        "uniform_count": sum(1 for e in entries if e["uniform"]),
        "images": entries,
    }
    warnings = []
    for e in entries:
        if e["uniform"]:
            warnings.append("%s is a blank/flat frame (luma stddev %.2f)" % (os.path.basename(e["path"]), e["luma_stddev"]))
    counts = {}
    for e in entries:
        key = "%dx%d" % (e["width"], e["height"])
        counts[key] = counts.get(key, 0) + 1
    if len(counts) > 1:
        warnings.append("mixed image sizes: " + ", ".join("%s x%d" % kv for kv in counts.items()))
    if warnings:
        summary["warnings"] = warnings
    return summary, png_bytes(sheet_w, sheet_h, canvas, 3)


def make_sheet(
    out,
    images,
    directory=None,
    cols=None,
    cell=None,
    gap=8,
    bg=DEFAULT_BG,
    labels=True,
    replace=False,
):
    """Build the contact sheet file OUT and return its JSON summary (see the module docstring).

    ``images`` are file paths (a pattern with * ? [ is expanded), ``directory`` adds every *.png
    in it, ``cell`` is None, a width, or a (width, height) tuple.  OUT is created (with missing
    parent directories) only when everything else worked.  Raises ToolError for usage problems
    and PngError/OSError for unreadable inputs; nothing is written in that case.
    """
    out = os.fspath(out)
    if os.path.exists(out) and not replace:
        raise ToolError("%s already exists (use --replace to overwrite it)" % out)
    if os.path.isdir(out):
        raise ToolError("%s is a directory" % out)
    paths = _gather_inputs(list(images), directory, out)
    summary, data = render_sheet([(p, p) for p in paths], cols=cols, cell=cell, gap=gap, bg=bg, labels=labels)
    if os.path.dirname(out):
        os.makedirs(os.path.dirname(out), exist_ok=True)
    _write_file(out, data, replace)
    summary["output"] = _slash(out)
    return summary


def _outline(canvas, stride, x, y, w, h):
    color = bytes(OUTLINE_RGB)
    top = (y * stride + x) * 3
    bottom = ((y + h - 1) * stride + x) * 3
    canvas[top : top + 3 * w] = color * w
    canvas[bottom : bottom + 3 * w] = color * w
    for j in range(1, h - 1):
        left = ((y + j) * stride + x) * 3
        canvas[left : left + 3] = color
        right = left + 3 * (w - 1)
        canvas[right : right + 3] = color


# --------------------------------------------------------------------------------------
# Command line
# --------------------------------------------------------------------------------------


def _build_parser():
    parser = argparse.ArgumentParser(
        prog="contact_sheet.py",
        description="Pack many PNG screenshots into one labelled contact sheet and report blank frames.",
    )
    parser.add_argument("output", help="new PNG file to write")
    parser.add_argument("images", nargs="*", help="input PNG files (patterns such as shots/*.png work)")
    parser.add_argument("--dir", help="also use every *.png in this directory (natural sort)")
    parser.add_argument("--cols", type=int, help="columns (default ceil(sqrt(n)), at most 4)")
    parser.add_argument("--cell", help="cell size W or WxH (default 320, height from the first image)")
    parser.add_argument("--gap", type=int, default=8, help="spacing and margin in pixels (default 8)")
    parser.add_argument("--bg", default="202020", help="background colour RRGGBB (default 202020)")
    parser.add_argument("--no-labels", action="store_true", help="omit the captions")
    parser.add_argument("--replace", action="store_true", help="overwrite OUT if it exists")
    parser.add_argument("--json", action="store_true", help="print the JSON summary (the default)")
    return parser


def main(argv=None):
    """Command line entry point; returns the exit status."""
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(errors="backslashreplace")
        except (AttributeError, ValueError):
            pass
    args = _build_parser().parse_args(argv)
    try:
        cell = _parse_cell(args.cell) if args.cell else None
        summary = make_sheet(
            args.output,
            args.images,
            directory=args.dir,
            cols=args.cols,
            cell=cell,
            gap=args.gap,
            bg=_parse_bg(args.bg),
            labels=not args.no_labels,
            replace=args.replace,
        )
    except ToolError as exc:
        print("error: %s" % exc, file=sys.stderr)
        return 2
    except (PngError, OSError) as exc:
        print("error: %s" % exc, file=sys.stderr)
        return 2
    print(json.dumps(summary, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    sys.exit(main())
