"""Reproduce Leo's PCM source excerpts; never opens an audio device.

Provide downloaded NPS dawn.mp3, wind.mp3, night.mp3 in INPUT (credits in ../AUDIO.md).
FFmpeg does decoding/filtering only. The three credited excerpts are committed; seeded leaves are regenerated before packaging.
"""
import argparse
import array
import math
from pathlib import Path
import random
import subprocess
import sys
import tempfile
import wave

RATE = 44100

def write(path, data):
    if sys.byteorder != "little":
        data.byteswap()
    with wave.open(str(path), "wb") as out:
        out.setnchannels(2)
        out.setsampwidth(2)
        out.setframerate(RATE)
        out.writeframes(data.tobytes())

def prepare_leaves(path):
    rng = random.Random(0x4C454F)
    data = array.array("h")
    low = [0., 0.]
    slow = [0., 0.]
    for i in range(RATE * 25):
        t = i / RATE
        envelope = 0.5 + 0.22 * math.sin(t * 0.6) + 0.16 * math.sin(t * 1.37)
        for channel in range(2):
            low[channel] += 0.14 * (rng.uniform(-1., 1.) - low[channel])
            slow[channel] += 0.002 * (low[channel] - slow[channel])
            data.append(round((low[channel] - slow[channel]) * envelope * 12000))
    write(path, data)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parents[1] / "assets/audio-source")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    for name, start in [("dawn", 30), ("wind", 15), ("night", 0)]:
        with tempfile.TemporaryDirectory() as scratch:
            decoded = Path(scratch) / "decoded.wav"
            subprocess.run(["ffmpeg", "-v", "error", "-nostdin", "-ss", str(start), "-i", str(args.input / f"{name}.mp3"),
                            "-t", "25", "-af", "highpass=f=55,lowpass=f=11000", "-ar", str(RATE), "-ac", "2", "-c:a", "pcm_s16le", str(decoded)], check=True)
            with wave.open(str(decoded), "rb") as source:
                if source.getnframes() != RATE * 25:
                    raise ValueError(f"{name} must provide 25 seconds")
                data = array.array("h", source.readframes(source.getnframes()))
            if sys.byteorder != "little":
                data.byteswap()
            peak = max(abs(v) for v in data)
            if not peak:
                raise ValueError(f"{name} is silent")
            # Fixed peak reserve. The project's per-layer gain/headroom manages the final mix.
            normalized = array.array("h", (round(v * 0.70 * 32767 / peak) for v in data))
            write(args.output / f"{name}.wav", normalized)
    prepare_leaves(args.output / "leaves.wav")
    print("Prepared three credited field-recording excerpts and original synthetic leaf rustle; no playback.")

if __name__ == "__main__":
    main()
