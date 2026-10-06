#!/usr/bin/env python3
"""Numeric quality report for WAV files: an agent cannot listen to audio, but it can read numbers.

    python tools/audio_report.py FILE.wav [FILE.wav ...] [--spectrogram OUT.png] [--json]
        [--fail-on-issues] [--replace] [--loop]

Standard library only (Python 3.10+).  Reads PCM 8/16/24/32-bit and IEEE float 32/64-bit WAV
files, plain or WAVE_FORMAT_EXTENSIBLE, mono or multichannel; a data chunk that is truncated or
declares an unknown (streaming) size is analysed as far as it goes.  Per file it reports, in dB
relative to full scale (dBFS) and milliseconds:

* duration, sample rate, channels, peak (also per channel), RMS over all samples, crest factor,
  DC offset (mean), clipped samples (|x| >= 0.999) with the longest run;
* approx LUFS: ITU-R BS.1770-4 K-weighting (RLB high-pass + high shelf, coefficients derived for
  the file's own sample rate), 400 ms blocks every 100 ms, absolute gate -70 LUFS and relative gate
  -10 LU, every channel weighted 1.0.  A file shorter than 400 ms is measured as one block.  It is an
  estimate, not a calibrated meter;
* leading and trailing silence and the longest silence inside the file (10 ms windows whose RMS is
  below -60 dBFS, so the resolution is 10 ms);
* click detection.  "starts/ends with a click": the first/last sample is not near zero
  (|x| >= 0.02).  Clicks inside: short groups (under 64 samples) of large values of the third
  difference x[k+3] - 3x[k+2] + 3x[k+1] - x[k], which cancels bass and slow content, holding a
  sample-to-sample jump >= 0.02 and towering 6x over the level (90th percentile) of the ~96
  samples on each side.  So noise, bright tones and steep but steady waveforms are not flagged,
  a dropout of a few samples counts as one click, and discontinuities that repeat (2 or more
  similar ones within 50 ms, such as square or saw edges) are waveform, not clicks.  Count and
  first positions are reported.  Files under ~200 samples are not scanned;
  `--loop` checks the wrapped boundary with the same discontinuity detector instead of treating
  nonzero endpoints as one-shot clicks. The boundary jump and scan availability are reported;
* spectral centroid, 85% rolloff and dominant frequency in Hz (mono mix, 2048-sample Hann frames,
  hop 1024, at most 600 evenly chosen frames, own radix-2 FFT, DC bin ignored) and the share of
  energy in sub (<60 Hz), bass (60-250), low-mid (250-2k), high-mid (2k-8k) and air (>8k);
* `issues`: plain-English problems, each starting with a code and carrying its numbers:
  `silent` (peak below -60 dBFS), `clipping` (3 or more samples at full scale in runs of 3+, or over
  0.01% of all samples), `dc offset` (|mean| >= 0.01 and the median of eight segment means agrees,
  so bass content is not mistaken for an offset), `starts with a click`, `ends with a click`,
  `clicks`, `too quiet` (peak below -24 dBFS), `too loud` (approx LUFS above -6), `non-finite
  samples` (NaN/inf in a float file) and `truncated` (the data chunk is shorter than declared).

`--spectrogram OUT.png` (one input file only) draws a log-frequency (40 Hz - 18 kHz) versus time
picture, at most 720 px wide, colour = dB relative to the loudest bin (-90 dB dark ... 0 dB
bright, magma-like ramp), with frequency, time and dB labels.  OUT must be a new file unless
--replace is given.  Short sounds use a smaller FFT (512 or 1024) so that they still show
detail in time; the title line states the FFT size.

Default output is a readable report; --json prints {"files": [...], "errors": [...]} instead.
Exit status: 0 done, 1 with --fail-on-issues when any file has issues, 2 for an unreadable
input, unusable arguments or a refused overwrite.

Importable: read_wav(path), analyze(path or wav dict), render_spectrogram(...).
"""

from __future__ import annotations

import argparse
import array
import cmath
import json
import math
import os
import struct
import sys
import zlib
from operator import mul, sub

__all__ = ["WavError", "parse_wav", "read_wav", "analyze", "k_weighting_coeffs", "render_spectrogram", "main"]

SILENCE_DBFS = -60.0
SILENCE_WINDOW_S = 0.010
CLIP_LEVEL = 0.999
DC_LIMIT = 0.01
EDGE_LIMIT = 0.02
CLICK_STEP = 0.02
CLICK_RATIO = 6.0
CLICK_GROUP = 40
CLICK_MAXW = 64
CLICK_BG = 96
CLICK_MARGIN = 8
CLICK_MERGE = 16
CLICK_PERIOD_S = 0.05
CLICK_BLOCK = 1 << 18
CLICK_LISTED = 8
QUIET_PEAK_DBFS = -24.0
LOUD_LUFS = -6.0
FFT_SIZE = 2048
FFT_HOP = 1024
MAX_FRAMES = 600
MAX_FILE_BYTES = 1 << 29
FLOAT_LIMIT = 1e6  # float samples beyond this (120 dB over full scale) are corrupt, like NaN/inf
BANDS = (("sub", 0.0, 60.0), ("bass", 60.0, 250.0), ("low_mid", 250.0, 2000.0), ("high_mid", 2000.0, 8000.0), ("air", 8000.0, 1e9))

_I32 = "i" if array.array("i").itemsize == 4 else "l"
_BIG = sys.byteorder == "big"


def _sumprod_fallback(a, b):
    return math.fsum(map(mul, a, b))


_sumprod = getattr(math, "sumprod", _sumprod_fallback)  # math.sumprod exists from Python 3.12


class WavError(ValueError):
    """The file is not a WAV this tool can read."""


# --------------------------------------------------------------------------------------
# WAV reading
# --------------------------------------------------------------------------------------

_TAG_NAMES = {2: "MS ADPCM", 6: "A-law", 7: "mu-law", 0x11: "IMA ADPCM", 0x55: "MP3", 0x161: "WMA"}


def _decode_samples(chunk, kind, bits):
    """Little-endian sample bytes -> array('d') of floats (full scale = +-1.0), interleaved."""
    if kind == "pcm":
        if bits == 8:
            lut = [(i - 128) / 128.0 for i in range(256)]
            return array.array("d", map(lut.__getitem__, chunk))
        if bits == 16:
            raw = array.array("h")
            raw.frombytes(chunk)
            scale = 1.0 / 32768.0
        else:
            if bits == 24:  # place each 3-byte sample in the top of a 32-bit lane
                lane = bytearray(4 * (len(chunk) // 3))
                lane[1::4] = chunk[0::3]
                lane[2::4] = chunk[1::3]
                lane[3::4] = chunk[2::3]
                chunk = bytes(lane)
            raw = array.array(_I32)
            raw.frombytes(chunk)
            scale = 1.0 / 2147483648.0
        if _BIG:
            raw.byteswap()
        return array.array("d", map(scale.__mul__, raw))
    raw = array.array("f" if bits == 32 else "d")
    raw.frombytes(chunk)
    if _BIG:
        raw.byteswap()
    return array.array("d", raw) if bits == 32 else raw


def parse_wav(buf):
    """Parse WAV bytes; returns a dict with rate, channels, bits, kind, format, frames, data.

    ``data`` is a list of array('d'), one per channel, full scale = +-1.0.
    """
    if len(buf) >= 4 and bytes(buf[:4]) == b"RIFX":
        raise WavError("big-endian RIFX files are not supported")
    if len(buf) < 12 or bytes(buf[:4]) not in (b"RIFF", b"RF64") or bytes(buf[8:12]) != b"WAVE":
        raise WavError("not a WAV file (no RIFF/WAVE header)")
    pos, size = 12, len(buf)
    fmt = data_range = None
    declared = 0
    truncated = False
    while pos + 8 <= size:
        cid = bytes(buf[pos : pos + 4])
        length = struct.unpack_from("<I", buf, pos + 4)[0]
        body = pos + 8
        if cid == b"fmt ":
            fmt = bytes(buf[body : body + length])
            if len(fmt) < 16:
                raise WavError("the fmt chunk is too short")
        elif cid == b"data":
            declared = length
            end = body + length
            if length == 0xFFFFFFFF or end > size:  # streaming header, or the file was cut off
                truncated = length != 0xFFFFFFFF
                end = size
            data_range = (body, end)
            if fmt is not None:
                break
        pos = body + length + (length & 1)
    if fmt is None:
        raise WavError("no fmt chunk found")
    if data_range is None:
        raise WavError("no data chunk found")
    tag, channels, rate, _byte_rate, _align, bits = struct.unpack_from("<HHIIHH", fmt)
    extensible = tag == 0xFFFE
    if extensible:
        if len(fmt) < 40:
            raise WavError("the WAVE_FORMAT_EXTENSIBLE fmt chunk is too short")
        tag = struct.unpack_from("<H", fmt, 24)[0]  # first two bytes of the SubFormat GUID
    if tag not in (1, 3):
        raise WavError(
            "unsupported WAV encoding (format tag 0x%04x%s): only PCM and IEEE float are read"
            % (tag, " = " + _TAG_NAMES[tag] if tag in _TAG_NAMES else "")
        )
    kind = "pcm" if tag == 1 else "float"
    if kind == "pcm" and bits not in (8, 16, 24, 32):
        raise WavError("unsupported PCM bit depth %d (8, 16, 24 and 32 are read)" % bits)
    if kind == "float" and bits not in (32, 64):
        raise WavError("unsupported float bit depth %d (32 and 64 are read)" % bits)
    if channels < 1 or channels > 64:
        raise WavError("unsupported channel count %d" % channels)
    if not 1 <= rate <= 10_000_000:
        raise WavError("invalid sample rate %d" % rate)
    frame_bytes = channels * bits // 8
    start, end = data_range
    frames = (end - start) // frame_bytes
    if frames < 1:
        raise WavError("the data chunk holds no complete sample frame")
    chunk = bytes(buf[start : start + frames * frame_bytes])
    samples = _decode_samples(chunk, kind, bits)
    nonfinite = 0
    if kind == "float" and (
        not math.isfinite(sum(samples)) or max(samples) > FLOAT_LIMIT or min(samples) < -FLOAT_LIMIT
    ):
        for i, v in enumerate(samples):
            if not -FLOAT_LIMIT <= v <= FLOAT_LIMIT:  # also true for NaN
                samples[i] = 0.0
                nonfinite += 1
    data = [samples[c::channels] for c in range(channels)] if channels > 1 else [samples]
    label = ("PCM %d-bit" if kind == "pcm" else "float %d-bit") % bits
    return {
        "rate": rate,
        "channels": channels,
        "bits": bits,
        "kind": kind,
        "format": label + (" (extensible)" if extensible else ""),
        "frames": frames,
        "data": data,
        "nonfinite": nonfinite,
        "truncated": truncated,
        "declared_bytes": declared,
        "available_bytes": end - start,
    }


def read_wav(path):
    """Read a WAV file (see parse_wav).  Raises WavError or OSError."""
    if os.path.getsize(path) > MAX_FILE_BYTES:
        raise WavError("file is larger than %d MB, too large for this tool" % (MAX_FILE_BYTES >> 20))
    with open(path, "rb") as fh:
        return parse_wav(fh.read())


# --------------------------------------------------------------------------------------
# Levels, loudness, silence
# --------------------------------------------------------------------------------------


def _db(x):
    return 20.0 * math.log10(x) if x > 0 else -math.inf


def k_weighting_coeffs(rate):
    """ITU-R BS.1770 K-weighting filters for a sample rate.

    Returns (shelf_b, shelf_a, highpass_b, highpass_a): the numerator (b0, b1, b2) and the
    denominator tail (a1, a2) of the high-shelf stage and of the RLB high-pass stage.  They are
    derived from the analogue prototypes (as in libebur128), so the 48 kHz values equal the
    coefficient table of the recommendation.
    """
    f0, gain_db, q = 1681.974450955533, 3.999843853973347, 0.7071752369554196
    k = math.tan(math.pi * f0 / rate)
    vh = 10.0 ** (gain_db / 20.0)
    vb = vh**0.4996667741545416
    a0 = 1.0 + k / q + k * k
    shelf_b = ((vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0)
    shelf_a = (2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0)
    f0, q = 38.13547087602444, 0.5003270373238773
    k = math.tan(math.pi * f0 / rate)
    a0 = 1.0 + k / q + k * k
    high_a = (2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0)
    return shelf_b, shelf_a, (1.0, -2.0, 1.0), high_a


def _k_energies(ch, rate):
    """Sum of squares of the K-weighted signal per 100 ms sub-block; returns (list, sub_block_len)."""
    (b0, b1, b2), (a1, a2), (c0, c1, c2), (d1, d2) = k_weighting_coeffs(rate)
    sub_len = max(1, int(round(rate * 0.1)))
    z1 = z2 = w1 = w2 = 0.0
    out = []
    for start in range(0, len(ch), sub_len):
        energy = 0.0
        for v in ch[start : start + sub_len]:
            y = b0 * v + z1
            z1 = b1 * v - a1 * y + z2
            z2 = b2 * v - a2 * y
            u = c0 * y + w1
            w1 = c1 * y - d1 * u + w2
            w2 = c2 * y - d2 * u
            energy += u * u
        out.append(energy)
    return out, sub_len


def _lufs(z):
    return -0.691 + 10.0 * math.log10(z) if z > 0 else -math.inf


def _integrated_lufs(per_channel, sub_len, frames):
    """(approx LUFS or None, method text) from per-channel sub-block energies."""
    nsub = len(per_channel[0])
    total = [math.fsum(e[i] for e in per_channel) for i in range(nsub)]
    if frames < 4 * sub_len:
        level = _lufs(math.fsum(total) / frames)
        return (level if level > -70.0 else None), "one block: the file is shorter than 400 ms"
    blocks = [(total[j] + total[j + 1] + total[j + 2] + total[j + 3]) / (4 * sub_len) for j in range(nsub - 3)]
    kept = [z for z in blocks if _lufs(z) > -70.0]
    method = "BS.1770-4 gating: 400 ms blocks, -70 LUFS absolute and -10 LU relative gate"
    if not kept:
        return None, method
    relative = _lufs(math.fsum(kept) / len(kept)) - 10.0
    final = [z for z in kept if _lufs(z) > relative] or kept
    return _lufs(math.fsum(final) / len(final)), method


def _window_energies(ch, win):
    return [_sumprod(seg, seg) for seg in (ch[i : i + win] for i in range(0, len(ch), win))]


def _silence(energies, win, channels, frames, rate):
    """(leading_ms, trailing_ms, longest_internal_ms) from per-window energies.

    A file that is silent throughout counts entirely as leading silence.
    """
    limit = 10.0 ** (SILENCE_DBFS / 10.0)
    quiet = []
    for i, e in enumerate(energies):
        length = min(win, frames - i * win)
        quiet.append(e / (length * channels) < limit)
    if all(quiet):
        return frames / rate * 1000.0, 0.0, 0.0
    first = quiet.index(False)
    last = len(quiet) - 1 - quiet[::-1].index(False)
    longest = run = 0
    for q in quiet[first : last + 1]:
        run = run + 1 if q else 0
        longest = max(longest, run)
    to_ms = 1000.0 / rate
    return first * win * to_ms, (frames - min(frames, (last + 1) * win)) * to_ms, longest * win * to_ms


def _clip_stats(ch):
    count = longest = run = 0
    for v in ch:
        if v >= CLIP_LEVEL or v <= -CLIP_LEVEL:
            count += 1
            run += 1
            if run > longest:
                longest = run
        else:
            run = 0
    return count, longest


def _dc_offset(ch):
    """(mean, median of eight segment means): the two agree only for a real, constant offset."""
    n = len(ch)
    mean = math.fsum(ch) / n
    parts = 8 if n >= 8 * 32 else 1
    size = -(-n // parts)
    means = sorted(math.fsum(ch[i : i + size]) / len(ch[i : i + size]) for i in range(0, n, size))
    mid = len(means) // 2
    median = means[mid] if len(means) % 2 else 0.5 * (means[mid - 1] + means[mid])
    return mean, median


def _mix_down(chans):
    if len(chans) == 1:
        return chans[0]
    out = array.array("d")
    step = 1 << 16
    scale = 1.0 / len(chans)
    for start in range(0, len(chans[0]), step):
        parts = [c[start : start + step] for c in chans]
        if len(parts) == 2:
            out.extend([(a + b) * scale for a, b in zip(*parts)])
        else:
            out.extend([sum(t) * scale for t in zip(*parts)])
    return out


# --------------------------------------------------------------------------------------
# Click detection
# --------------------------------------------------------------------------------------


def _click_block(x):
    """Clicks in one block: list of (index of the first sample after the strongest step, jump).

    A click is a short group (under CLICK_MAXW samples) of large third-difference values that
    towers CLICK_RATIO times over the level (90th percentile) of the CLICK_BG samples on each
    side of it.
    """
    n = len(x)
    if n < 2 * CLICK_BG + 16:
        return []
    d = list(map(sub, x[1:], x))
    dd = list(map(sub, d[1:], d))
    del d
    ddd = list(map(sub, dd[1:], dd))
    del dd
    a = list(map(abs, ddd))
    del ddd
    limit = 2.0 * CLICK_STEP
    hits = [k for k, v in enumerate(a) if v >= limit]
    count = len(hits)
    events = []
    i = 0
    while i < count:
        j = i
        while j + 1 < count and hits[j + 1] - hits[j] <= CLICK_GROUP:
            j += 1
        first, last = hits[i], hits[j]
        if last - first <= CLICK_MAXW:
            best = max(hits[i : j + 1], key=a.__getitem__)
            # The surroundings on both sides, taken as their 90th percentile: a second click
            # nearby (or the far edge of a dropout) then does not hide this one.
            around = sorted(
                a[max(0, first - CLICK_BG) : max(0, first - CLICK_MARGIN)]
                + a[last + CLICK_MARGIN + 1 : last + CLICK_BG + 1]
            )
            level = around[len(around) * 9 // 10] if around else 0.0
            if a[best] >= CLICK_RATIO * level:
                pos = best + 2  # the 3rd difference peaks two samples before the step
                jump = max(abs(x[p] - x[p - 1]) for p in range(max(1, pos - 2), min(n, pos + 3)))
                if jump >= CLICK_STEP:
                    events.append((pos, jump))
        i = j + 1
    return events


def _click_events(x):
    """Clicks over a whole channel, processed in overlapping blocks (bounded memory)."""
    n = len(x)
    pad = CLICK_BG + CLICK_MAXW + CLICK_GROUP + 32
    events = []
    for start in range(0, n, CLICK_BLOCK):
        lo = max(0, start - pad)
        hi = min(n, start + CLICK_BLOCK + pad)
        for pos, jump in _click_block(x[lo:hi]):
            if start <= lo + pos < start + CLICK_BLOCK:
                events.append((lo + pos, jump))
    return _merge_events(events)


def _merge_events(events):
    """Sort events and merge those within a few samples of each other (keep the larger jump)."""
    merged = []
    for pos, jump in sorted(events):
        if merged and pos - merged[-1][0] <= CLICK_MERGE:
            if jump > merged[-1][1]:
                merged[-1] = (pos, jump)
        else:
            merged.append((pos, jump))
    return merged


def _drop_periodic(events, rate):
    """Remove edges that repeat (>= 2 similar neighbours within 50 ms): that is waveform, not a click."""
    window = max(1, int(CLICK_PERIOD_S * rate))
    keep = []
    count = len(events)
    for idx, (pos, jump) in enumerate(events):
        similar = 0
        for j in range(idx - 1, max(-1, idx - 17), -1):
            if pos - events[j][0] > window:
                break
            if 0.5 * jump <= events[j][1] <= 2.0 * jump:
                similar += 1
        for j in range(idx + 1, min(count, idx + 17)):
            if events[j][0] - pos > window:
                break
            if 0.5 * jump <= events[j][1] <= 2.0 * jump:
                similar += 1
        if similar < 2:
            keep.append((pos, jump))
    return keep


# --------------------------------------------------------------------------------------
# FFT and spectral summary
# --------------------------------------------------------------------------------------

_FFT_PLANS = {}


def _fft_plan(n):
    plan = _FFT_PLANS.get(n)
    if plan is None:
        bits = n.bit_length() - 1
        rev = [int(format(i, "0%db" % bits)[::-1], 2) for i in range(n)]
        stages = []
        size = 2
        while size <= n:
            half = size // 2
            stages.append((size, half, [cmath.exp(-2j * math.pi * k / size) for k in range(half)]))
            size *= 2
        hann = [0.5 - 0.5 * math.cos(2.0 * math.pi * i / (n - 1)) for i in range(n)]
        plan = (rev, stages, hann)
        _FFT_PLANS[n] = plan
    return plan


def _fft(x, plan):
    """Iterative radix-2 FFT of a complex list whose length is a power of two."""
    rev, stages, _ = plan
    a = [x[i] for i in rev]
    n = len(a)
    for size, half, tw in stages:
        for start in range(0, n, size):
            for k in range(half):
                i = start + k
                j = i + half
                t = tw[k] * a[j]
                u = a[i]
                a[j] = u - t
                a[i] = u + t
    return a


def _windowed(x, centre, n, hann):
    """n samples of x centred on `centre` (zero padded outside the signal), Hann windowed."""
    lo = centre - n // 2
    seg = x[max(0, lo) : max(0, min(len(x), lo + n))]
    left = max(0, -lo)
    if left or len(seg) < n:
        seg = [0.0] * left + list(seg) + [0.0] * (n - left - len(seg))
    return [v * w for v, w in zip(seg, hann)]


def _power_pair(fa, fb, plan):
    """Power spectra (bins 0..n/2) of two real frames with a single complex FFT."""
    rev = plan[0]
    n = len(rev)
    z = _fft([complex(p, q) for p, q in zip(fa, fb)], plan)
    half = n // 2
    head = z[: half + 1]
    mirror = [z[0]] + z[n - 1 : half - 1 : -1]  # Z[(n - k) % n]
    pa = [((p.real + q.real) ** 2 + (p.imag - q.imag) ** 2) * 0.25 for p, q in zip(head, mirror)]
    pb = [((p.imag + q.imag) ** 2 + (p.real - q.real) ** 2) * 0.25 for p, q in zip(head, mirror)]
    return pa, pb


def _frame_powers(x, centres, n):
    """Yield the power spectrum of each frame centred on `centres` (two frames per FFT)."""
    plan = _fft_plan(n)
    hann = plan[2]
    zero = [0.0] * n
    for i in range(0, len(centres), 2):
        fa = _windowed(x, centres[i], n, hann)
        has_b = i + 1 < len(centres)
        fb = _windowed(x, centres[i + 1], n, hann) if has_b else zero
        pa, pb = _power_pair(fa, fb, plan)
        yield pa
        if has_b:
            yield pb


def _frame_centres(total, size, hop, cap):
    """Centres of at most `cap` frames of `size` samples, hop apart, covering the signal evenly."""
    if total <= size:
        return [total // 2]
    starts = list(range(0, total - size + 1, hop))
    if starts[-1] != total - size:
        starts.append(total - size)
    if len(starts) > cap:
        count = len(starts)
        starts = [starts[(i * (count - 1)) // (cap - 1)] for i in range(cap)]
    return [s + size // 2 for s in starts]


def _spectral_summary(x, rate):
    """Centroid, rolloff, dominant frequency, band shares from the mean power spectrum."""
    n = FFT_SIZE
    total_len = len(x)
    if total_len < n:
        # a short signal: one frame around its middle; the window covers the signal only
        hann = [0.5 - 0.5 * math.cos(2.0 * math.pi * i / max(1, total_len - 1)) for i in range(total_len)]
        frame = [v * w for v, w in zip(x, hann)] + [0.0] * (n - total_len)
        plan = _fft_plan(n)
        power = _power_pair(frame, [0.0] * n, plan)[0]
        frames = 1
    else:
        centres = _frame_centres(total_len, n, FFT_HOP, MAX_FRAMES)
        power = [0.0] * (n // 2 + 1)
        for pa in _frame_powers(x, centres, n):
            power = [u + v for u, v in zip(power, pa)]
        frames = len(centres)
    width = rate / n
    freqs = [k * width for k in range(len(power))]
    body = power[1:]
    total = math.fsum(body)
    empty = {"frames": frames, "centroid_hz": None, "rolloff85_hz": None, "dominant_hz": None, "band_energy_pct": None}
    if not total > 1e-24:
        return empty
    centroid = math.fsum(f * p for f, p in zip(freqs[1:], body)) / total
    running = 0.0
    rolloff = freqs[-1]
    for f, p in zip(freqs[1:], body):
        running += p
        if running >= 0.85 * total:
            rolloff = f
            break
    peak = max(range(1, len(power)), key=power.__getitem__)
    dominant = peak * width
    if 1 < peak < len(power) - 1:
        lo, mid, hi = (math.log(power[peak + d] + 1e-30) for d in (-1, 0, 1))
        denom = lo - 2.0 * mid + hi
        if denom < 0:
            dominant = (peak + 0.5 * (lo - hi) / denom) * width
    shares = {}
    for name, f_lo, f_hi in BANDS:
        shares[name] = 100.0 * math.fsum(p for f, p in zip(freqs[1:], body) if f_lo <= f < f_hi) / total
    return {
        "frames": frames,
        "centroid_hz": centroid,
        "rolloff85_hz": rolloff,
        "dominant_hz": dominant,
        "band_energy_pct": shares,
    }


# --------------------------------------------------------------------------------------
# Analysis
# --------------------------------------------------------------------------------------


def _r(v, digits):
    return None if v is None or not math.isfinite(v) else round(v, digits)


def _fmt_db(v):
    return "-inf" if v == -math.inf else "%.1f" % v


def analyze(source, spectrogram=None, replace=False, loop=False):
    """Analyse a WAV file path (or the dict from parse_wav) and return the report dict.

    ``spectrogram`` is an optional output PNG path (written as a new file).
    """
    if isinstance(source, dict):
        wav, path = source, "<data>"
    else:
        wav, path = read_wav(source), os.fspath(source)
    spectrogram = os.fspath(spectrogram) if spectrogram else None
    chans, rate, nch, n = wav["data"], wav["rate"], wav["channels"], wav["frames"]
    duration_ms = n / rate * 1000.0

    peaks = [max(max(c), -min(c)) for c in chans]
    peak = max(peaks)
    win = max(1, int(round(rate * SILENCE_WINDOW_S)))
    per_channel_windows = [_window_energies(c, win) for c in chans]
    windows = [math.fsum(t) for t in zip(*per_channel_windows)]
    rms = math.sqrt(math.fsum(windows) / (n * nch))
    peak_db, rms_db = _db(peak), _db(rms)
    crest = peak_db - rms_db if peak > 0 and rms > 0 else None

    dc_pairs = [_dc_offset(c) for c in chans]
    dc_means = [m for m, _ in dc_pairs]
    worst = max(range(nch), key=lambda i: abs(dc_means[i]))
    dc_mean, dc_median = dc_pairs[worst]
    dc_real = abs(dc_mean) >= DC_LIMIT and dc_mean * dc_median > 0 and abs(dc_median) >= 0.5 * abs(dc_mean)

    clipped = longest_run = 0
    if peak >= CLIP_LEVEL:
        for c in chans:
            count, run = _clip_stats(c)
            clipped += count
            longest_run = max(longest_run, run)
    first = max(chans, key=lambda c: abs(c[0]))[0]
    last = max(chans, key=lambda c: abs(c[-1]))[-1]

    lead_ms, trail_ms, inner_ms = _silence(windows, win, nch, n, rate)

    energies = [_k_energies(c, rate) for c in chans]
    lufs, lufs_method = _integrated_lufs([e for e, _ in energies], energies[0][1], n)

    mix = _mix_down(chans)
    click_channels = chans if nch <= 2 else [mix]
    events = _merge_events([e for c in click_channels for e in _drop_periodic(_click_events(c), rate)])
    seam_jump = max(abs(c[0] - c[-1]) for c in chans)
    seam_events = []
    pad = min(n, CLICK_BG + CLICK_MAXW + CLICK_GROUP + 32)
    if loop:
        for c in chans:  # do not let opposite-polarity stereo hide a discontinuity
            boundary = c[-pad:] + c[:pad]
            seam_events.extend((p, j) for p, j in _drop_periodic(_click_block(boundary), rate)
                               if abs(p - pad) <= CLICK_MAXW)

    spectrum = _spectral_summary(mix, rate)
    if spectrogram:
        w, h, rgb, _info = render_spectrogram(mix, rate, os.path.basename(path))
        _write_new_png(spectrogram, w, h, rgb, replace)

    issues = []

    def add(code, text):
        issues.append((code, "%s: %s" % (code, text)))

    if wav["nonfinite"]:
        add(
            "non-finite samples",
            "%d NaN/inf/absurd (|x| > %g) values were replaced by 0 for this analysis" % (wav["nonfinite"], FLOAT_LIMIT),
        )
    if wav["truncated"]:
        add(
            "truncated",
            "the data chunk declares %d bytes but the file holds %d" % (wav["declared_bytes"], wav["available_bytes"]),
        )
    silent = peak < 10.0 ** (SILENCE_DBFS / 20.0)
    if silent:
        add("silent", "peak %s dBFS is below %.0f dBFS" % (_fmt_db(peak_db), SILENCE_DBFS))
    else:
        if clipped >= 3 and (longest_run >= 3 or clipped >= 1e-4 * n * nch):
            add(
                "clipping",
                "%d samples (%.2f%%) at |x| >= %.3f, longest run %d"
                % (clipped, 100.0 * clipped / (n * nch), CLIP_LEVEL, longest_run),
            )
        if dc_real:
            add("dc offset", "mean %+.4f (%s dBFS), present throughout the file" % (dc_mean, _fmt_db(_db(abs(dc_mean)))))
        if not loop and abs(first) >= EDGE_LIMIT:
            add("starts with a click", "first sample %+.3f (%s dBFS) is not near zero (limit %.2f)" % (first, _fmt_db(_db(abs(first))), EDGE_LIMIT))
        if not loop and abs(last) >= EDGE_LIMIT:
            add("ends with a click", "last sample %+.3f (%s dBFS) is not near zero (limit %.2f)" % (last, _fmt_db(_db(abs(last))), EDGE_LIMIT))
        if seam_events:
            add("loop seam click", "wrapped boundary discontinuity (sample jump %.4f)" % seam_jump)
        if events:
            add(
                "clicks",
                "%d discontinuities inside the file, first at %.1f ms (jump %.3f)"
                % (len(events), events[0][0] / rate * 1000.0, events[0][1]),
            )
        if peak_db < QUIET_PEAK_DBFS:
            add("too quiet", "peak %.1f dBFS (below %.0f dBFS)" % (peak_db, QUIET_PEAK_DBFS))
        if lufs is not None and lufs > LOUD_LUFS:
            add("too loud", "approx %.2f LUFS (above %.0f LUFS)" % (lufs, LOUD_LUFS))

    report = {
        "path": path.replace(os.sep, "/"),
        "format": wav["format"],
        "sample_rate": rate,
        "channels": nch,
        "frames": n,
        "duration_ms": _r(duration_ms, 3),
        "peak_dbfs": _r(peak_db, 2),
        "peak_dbfs_channels": [_r(_db(p), 2) for p in peaks],
        "rms_dbfs": _r(rms_db, 2),
        "crest_db": _r(crest, 2),
        "loudness_lufs_approx": _r(lufs, 2),
        "loudness_method": lufs_method,
        "dc_offset": _r(dc_mean, 5),
        "dc_offset_channels": [_r(m, 5) for m in dc_means],
        "clipped_samples": clipped,
        "clipped_pct": _r(100.0 * clipped / (n * nch), 4),
        "clipped_longest_run": longest_run,
        "first_sample": _r(first, 5),
        "last_sample": _r(last, 5),
        "loop": {"enabled": loop, "seam_jump": _r(seam_jump, 6),
                 "scan_available": 2 * pad >= 2 * CLICK_BG + 16, "suspected_click": bool(seam_events) if loop else None},
        "silence": {
            "threshold_dbfs": SILENCE_DBFS,
            "leading_ms": _r(lead_ms, 1),
            "trailing_ms": _r(trail_ms, 1),
            "longest_internal_ms": _r(inner_ms, 1),
        },
        "clicks": {
            "count": len(events),
            "min_jump": CLICK_STEP,
            "first": [
                {"sample": p, "ms": _r(p / rate * 1000.0, 2), "jump": _r(j, 4)} for p, j in events[:CLICK_LISTED]
            ],
        },
        "spectrum": {
            "frames": spectrum["frames"],
            "centroid_hz": _r(spectrum["centroid_hz"], 1),
            "rolloff85_hz": _r(spectrum["rolloff85_hz"], 1),
            "dominant_hz": _r(spectrum["dominant_hz"], 1),
            "band_energy_pct": None
            if spectrum["band_energy_pct"] is None
            else {k: _r(v, 2) for k, v in spectrum["band_energy_pct"].items()},
        },
        "issues": [text for _, text in issues],
        "issue_codes": [code for code, _ in issues],
    }
    if spectrogram:
        report["spectrogram"] = spectrogram.replace(os.sep, "/")
    return report


# --------------------------------------------------------------------------------------
# PNG output and the spectrogram (own copies of the small helpers; no imports between tools)
# --------------------------------------------------------------------------------------


def _png_rgb_bytes(width, height, rgb):
    """Encode 8-bit RGB pixels as PNG bytes (filter 1 on every row, zlib level 6)."""
    stride = 3 * width
    m7 = int.from_bytes(b"\x7f" * stride, "little")
    m8 = int.from_bytes(b"\x80" * stride, "little")
    full = (1 << (8 * stride)) - 1
    view = memoryview(rgb)
    body = bytearray()
    for y in range(height):
        a = int.from_bytes(view[y * stride : (y + 1) * stride], "little")
        b = (a << 24) & full
        diff = ((a | m8) - (b & m7)) ^ ((a ^ b ^ m8) & m8)
        body += b"\x01" + diff.to_bytes(stride, "little")

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(data, zlib.crc32(kind)))

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(body), 6))
        + chunk(b"IEND", b"")
    )


def _write_new_png(path, width, height, rgb, replace):
    """Write the PNG as a new file; with replace, via a temporary file so a failure keeps the old one."""
    data = _png_rgb_bytes(width, height, rgb)
    path = os.fspath(path)
    if os.path.dirname(path):
        os.makedirs(os.path.dirname(path), exist_ok=True)
    target = "%s.tmp%d" % (path, os.getpid()) if replace else path
    fh = open(target, "wb" if replace else "xb")  # FileExistsError leaves the existing file alone
    try:
        with fh:
            fh.write(data)
        if replace:
            os.replace(target, path)
    except BaseException:
        try:
            os.remove(target)  # only ever a file this call created
        except OSError:
            pass
        raise


# 5x7 bitmap font, ASCII 32..126, five column bytes per glyph, bit 0 = top row.
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


def _text_mask(text, scale=2):
    """Render text: (width, rows) with rows as 0/1 bytearrays; non-ASCII becomes '?'."""
    text = "".join(c if 32 <= ord(c) <= 126 else "?" for c in text)
    width = max(0, len(text) * 6 * scale - scale)
    rows = [bytearray(width) for _ in range(7 * scale)]
    for i, ch in enumerate(text):
        base = (ord(ch) - 32) * 5
        for col in range(5):
            bits = _FONT[base + col]
            x0 = (i * 6 + col) * scale
            for row in range(7):
                if bits >> row & 1:
                    for dy in range(scale):
                        rows[row * scale + dy][x0 : x0 + scale] = b"\x01" * scale
    return width, rows


def _draw_text(canvas, stride, x, y, text, rgb, scale=2, align="left"):
    width, rows = _text_mask(text, scale)
    if align == "right":
        x -= width
    elif align == "center":
        x -= width // 2
    color = bytes(rgb)
    for j, mask in enumerate(rows):
        base = ((y + j) * stride + x) * 3
        for i, v in enumerate(mask):
            if v:
                canvas[base + 3 * i : base + 3 * i + 3] = color


SPEC_W = 720
SPEC_ML = 58
SPEC_MR = 62
SPEC_MT = 26
SPEC_PH = 288
SPEC_MB = 34
SPEC_PW = SPEC_W - SPEC_ML - SPEC_MR
SPEC_FMIN = 40.0
SPEC_FMAX = 18000.0
SPEC_DB = 90.0
_MAGMA = (
    (0, 0, 4),
    (28, 16, 68),
    (79, 18, 123),
    (129, 37, 129),
    (181, 54, 122),
    (229, 80, 100),
    (251, 135, 97),
    (254, 194, 135),
    (252, 253, 191),
)


def _colour_ramp():
    ramp = []
    for i in range(256):
        t = i / 255.0 * (len(_MAGMA) - 1)
        k = min(int(t), len(_MAGMA) - 2)
        f = t - k
        ramp.append(bytes(int(round(_MAGMA[k][c] + (_MAGMA[k + 1][c] - _MAGMA[k][c]) * f)) for c in range(3)))
    return ramp


def spectrogram_row(freq, plot_height=SPEC_PH):
    """Row inside the plot (0 = top = 18 kHz) that shows `freq` Hz on the log axis."""
    t = math.log(freq / SPEC_FMIN) / math.log(SPEC_FMAX / SPEC_FMIN)
    return int(round((plot_height - 1) * (1.0 - t)))


def _row_bins(rate, size, plot_height):
    """For every plot row: ('max', lo, hi) bin range or ('lerp', k, frac) for sparse low rows."""
    width = rate / size
    ratio = SPEC_FMAX / SPEC_FMIN
    nyquist_bin = size // 2

    def bin_at(t):  # fractional FFT bin of the frequency shown at plot row t
        return SPEC_FMIN * ratio ** ((plot_height - 1 - t) / (plot_height - 1)) / width

    spec = []
    for r in range(plot_height):
        b_lo, b_hi, b_mid = bin_at(r + 0.5), bin_at(r - 0.5), bin_at(r)
        lo, hi = max(1, math.ceil(b_lo)), min(nyquist_bin, math.floor(b_hi))
        if b_mid > nyquist_bin:
            spec.append(None)
        elif hi >= lo:
            spec.append(("max", lo, hi))
        else:
            k = max(1, min(int(math.floor(b_mid)), nyquist_bin - 1))
            spec.append(("lerp", k, min(1.0, max(0.0, b_mid - k))))
    return spec


def _nice_step(span, max_ticks):
    raw = span / max_ticks
    mag = 10.0 ** math.floor(math.log10(raw))
    for m in (1.0, 2.0, 5.0, 10.0):
        if m * mag >= raw:
            return m * mag
    return 10.0 * mag


def render_spectrogram(mix, rate, title):
    """Render the spectrogram of a mono signal; returns (width, height, rgb_bytes, info).

    ``info`` holds the plot rectangle and FFT size so callers can locate a frequency row.
    """
    total = len(mix)
    duration = total / rate
    size = 2048 if duration >= 1.5 else 1024 if duration >= 0.4 else 512
    hop = max(64, size // 8)
    wanted = min(SPEC_PW, max(1, -(-total // hop)))
    col_px = max(1, min(8, SPEC_PW // wanted))
    cols = SPEC_PW // col_px
    centres = [min(total - 1, int((j + 0.5) * total / cols)) for j in range(cols)]
    powers = list(_frame_powers(mix, centres, size))
    top = max((max(p) for p in powers), default=0.0)
    rows = _row_bins(rate, size, SPEC_PH)
    ramp = _colour_ramp()
    black = ramp[0]
    scale = 1.0 / top if top > 0 else 0.0
    shade = []  # per column: colour index for every plot row
    for power in powers:
        norm = [max(p * scale, 1e-12) for p in power]
        col = []
        for spec in rows:
            if spec is None:
                col.append(0)
                continue
            if spec[0] == "max":
                v = max(norm[spec[1] : spec[2] + 1])
            else:
                v = norm[spec[1]] * (1.0 - spec[2]) + norm[spec[1] + 1] * spec[2]
            db = 10.0 * math.log10(v) if v > 0 else -SPEC_DB
            col.append(int(round(255.0 * min(1.0, max(0.0, (db + SPEC_DB) / SPEC_DB)))) if scale else 0)
        shade.append(col)
    height = SPEC_MT + SPEC_PH + SPEC_MB
    canvas = bytearray(bytes((16, 16, 20)) * (SPEC_W * height))
    plot_w = cols * col_px
    for r in range(SPEC_PH):
        line = b"".join(ramp[col[r]] * col_px for col in shade) if scale else black * plot_w
        off = ((SPEC_MT + r) * SPEC_W + SPEC_ML) * 3
        canvas[off : off + 3 * plot_w] = line
    ink, dim = (232, 232, 238), (120, 120, 132)
    # frame around the plot and the colour bar
    _rect(canvas, SPEC_ML - 1, SPEC_MT - 1, plot_w + 2, SPEC_PH + 2, dim)
    bar_x = SPEC_ML + SPEC_PW + 10
    for r in range(SPEC_PH):
        level = 255 - int(round(255.0 * r / (SPEC_PH - 1)))
        off = ((SPEC_MT + r) * SPEC_W + bar_x) * 3
        canvas[off : off + 30] = ramp[level] * 10
    _rect(canvas, bar_x - 1, SPEC_MT - 1, 12, SPEC_PH + 2, dim)
    for db, y in (("0", SPEC_MT), ("-45", SPEC_MT + SPEC_PH // 2 - 7), ("-90", SPEC_MT + SPEC_PH - 14)):
        _draw_text(canvas, SPEC_W, bar_x + 16, y, db, ink)
    # frequency axis
    for f, label in ((50, "50"), (100, "100"), (200, "200"), (500, "500"), (1000, "1k"), (2000, "2k"), (5000, "5k"), (10000, "10k"), (16000, "16k")):
        if f > rate / 2:
            continue
        y = SPEC_MT + spectrogram_row(f)
        for x in range(SPEC_ML - 6, SPEC_ML - 1):
            canvas[(y * SPEC_W + x) * 3 : (y * SPEC_W + x) * 3 + 3] = bytes(dim)
        _draw_text(canvas, SPEC_W, SPEC_ML - 9, y - 7, label, ink, align="right")
    # time axis
    step = _nice_step(duration, max(2, plot_w // 90))
    t = 0.0
    while t <= duration + 1e-9:
        x = SPEC_ML + int(round(t / duration * plot_w)) if duration > 0 else SPEC_ML
        x = min(x, SPEC_ML + plot_w - 1)
        for y in range(SPEC_MT + SPEC_PH + 1, SPEC_MT + SPEC_PH + 6):
            canvas[(y * SPEC_W + x) * 3 : (y * SPEC_W + x) * 3 + 3] = bytes(dim)
        _draw_text(canvas, SPEC_W, x, SPEC_MT + SPEC_PH + 9, "%gs" % round(t, 6), ink, align="center")
        t += step
    _draw_text(canvas, SPEC_W, SPEC_ML, 6, _fit(title + "  %dHz  fft %d" % (rate, size), (SPEC_W - SPEC_ML) // 12), ink)
    info = {"plot": (SPEC_ML, SPEC_MT, plot_w, SPEC_PH), "fft_size": size, "columns": cols, "column_px": col_px}
    return SPEC_W, height, bytes(canvas), info


def _fit(text, max_chars):
    text = "".join(c if 32 <= ord(c) <= 126 else "?" for c in text)
    return text if len(text) <= max_chars else text[: max_chars - 3] + "..."


def _rect(canvas, x, y, w, h, rgb):
    color = bytes(rgb)
    for xx in range(x, x + w):
        for yy in (y, y + h - 1):
            canvas[(yy * SPEC_W + xx) * 3 : (yy * SPEC_W + xx) * 3 + 3] = color
    for yy in range(y, y + h):
        for xx in (x, x + w - 1):
            canvas[(yy * SPEC_W + xx) * 3 : (yy * SPEC_W + xx) * 3 + 3] = color


# --------------------------------------------------------------------------------------
# Output and command line
# --------------------------------------------------------------------------------------


def _num(v, fmt, none="-"):
    return none if v is None else fmt % v


def _report_lines(rep):
    lines = [rep["path"]]
    sil = rep["silence"]
    spec = rep["spectrum"]
    lines.append(
        "  format     %s, %d ch, %d Hz, %d frames, %.1f ms"
        % (rep["format"], rep["channels"], rep["sample_rate"], rep["frames"], rep["duration_ms"])
    )
    lines.append(
        "  level      peak %s dBFS | rms %s dBFS | crest %s dB | approx LUFS %s | dc %s"
        % (
            _num(rep["peak_dbfs"], "%.1f", "-inf"),
            _num(rep["rms_dbfs"], "%.1f", "-inf"),
            _num(rep["crest_db"], "%.1f"),
            _num(rep["loudness_lufs_approx"], "%.1f", "-inf"),
            _num(rep["dc_offset"], "%+.4f"),
        )
    )
    if rep["channels"] > 1:
        lines.append("  channels   peak dBFS " + " | ".join(_num(v, "%.1f", "-inf") for v in rep["peak_dbfs_channels"]))
    lines.append(
        "  clipping   %d samples (%.3f%%), longest run %d, at |x| >= %.3f"
        % (rep["clipped_samples"], rep["clipped_pct"], rep["clipped_longest_run"], CLIP_LEVEL)
    )
    lines.append("  edges      first sample %+.4f | last sample %+.4f" % (rep["first_sample"], rep["last_sample"]))
    if rep["loop"]["enabled"]:
        lines.append("  loop seam  jump %.6f | discontinuity scan %s" %
                     (rep["loop"]["seam_jump"], "available" if rep["loop"]["scan_available"] else "unavailable (too short)"))
    lines.append(
        "  silence    leading %.1f ms | trailing %.1f ms | longest inside %.1f ms (below %.0f dBFS, 10 ms windows)"
        % (sil["leading_ms"], sil["trailing_ms"], sil["longest_internal_ms"], sil["threshold_dbfs"])
    )
    clicks = rep["clicks"]
    where = ", ".join("%.1f ms (%.3f)" % (c["ms"], c["jump"]) for c in clicks["first"][:5])
    lines.append("  clicks     %d inside%s" % (clicks["count"], "  at " + where if where else ""))
    lines.append(
        "  spectrum   centroid %s Hz | rolloff85 %s Hz | dominant %s Hz"
        % (_num(spec["centroid_hz"], "%.0f"), _num(spec["rolloff85_hz"], "%.0f"), _num(spec["dominant_hz"], "%.1f"))
    )
    bands = spec["band_energy_pct"]
    if bands:
        lines.append("  bands %    " + " | ".join("%s %.1f" % (k.replace("_", "-"), v) for k, v in bands.items()))
    if rep.get("spectrogram"):
        lines.append("  spectrogram written to " + rep["spectrogram"])
    if rep["issues"]:
        lines.append("  issues     " + rep["issues"][0])
        lines.extend("             " + text for text in rep["issues"][1:])
    else:
        lines.append("  issues     none")
    return lines


def _summary_table(reports):
    head = "%-30s %9s %6s %2s %6s %6s %6s %5s %6s  %s" % ("file", "ms", "rate", "ch", "peak", "rms", "LUFS", "clip", "clicks", "issues")
    rows = [head]
    for r in reports:
        name = os.path.basename(r["path"])
        name = name if len(name) <= 30 else name[:27] + "..."
        rows.append(
            "%-30s %9.1f %6d %2d %6s %6s %6s %5d %6d  %s"
            % (
                name,
                r["duration_ms"],
                r["sample_rate"],
                r["channels"],
                _num(r["peak_dbfs"], "%.1f", "-inf"),
                _num(r["rms_dbfs"], "%.1f", "-inf"),
                _num(r["loudness_lufs_approx"], "%.1f", "-inf"),
                r["clipped_samples"],
                r["clicks"]["count"],
                ", ".join(r["issue_codes"]) or "none",
            )
        )
    return rows


def _build_parser():
    parser = argparse.ArgumentParser(
        prog="audio_report.py",
        description="Numeric quality report for WAV files: levels, loudness, clicks, silence and spectrum.",
    )
    parser.add_argument("files", nargs="+", help="WAV files to analyse")
    parser.add_argument("--spectrogram", metavar="OUT.png", help="write a spectrogram PNG (one input file only)")
    parser.add_argument("--json", action="store_true", help="print JSON instead of the readable report")
    parser.add_argument("--fail-on-issues", action="store_true", help="exit 1 when any file has issues")
    parser.add_argument("--replace", action="store_true", help="overwrite the spectrogram PNG if it exists")
    parser.add_argument("--loop", action="store_true", help="check the wrapped seam instead of one-shot endpoint clicks")
    return parser


def main(argv=None):
    """Command line entry point; returns the exit status."""
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(errors="backslashreplace")
        except (AttributeError, ValueError):
            pass
    parser = _build_parser()
    args = parser.parse_args(argv)
    if args.spectrogram:
        if len(args.files) != 1:
            parser.error("--spectrogram works with exactly one input file")
        if os.path.exists(args.spectrogram) and not args.replace:
            print("error: %s already exists (use --replace to overwrite it)" % args.spectrogram, file=sys.stderr)
            return 2
    reports, errors = [], []
    for path in args.files:
        try:
            reports.append(analyze(path, spectrogram=args.spectrogram, replace=args.replace, loop=args.loop))
        except (WavError, OSError) as exc:
            errors.append({"path": path.replace(os.sep, "/"), "error": str(exc)})
    if args.json:
        payload = {"files": reports, "errors": errors}
        print(json.dumps(payload, indent=2))
    else:
        for i, rep in enumerate(reports):
            if i:
                print()
            print("\n".join(_report_lines(rep)))
        if len(reports) > 1:
            print()
            print("\n".join(_summary_table(reports)))
    for err in errors:
        print("error: %s: %s" % (err["path"], err["error"]), file=sys.stderr)
    if errors:
        return 2
    if args.fail_on_issues and any(r["issues"] for r in reports):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
